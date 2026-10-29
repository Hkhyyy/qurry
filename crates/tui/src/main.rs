//! Qurry TUI — DataGrip-style terminal client. Uses the same engine (qurry-core)·same Data Sources as the GUI.
mod app;
mod complete;
mod datatools;
mod designer;
mod dba;
mod exec;
mod find;
mod grid;
mod help;
mod input;
mod keys;
mod kit;
mod migrate;
mod panes;
mod rag;
mod workspace;
#[cfg(test)]
mod testkit;
#[cfg(test)]
mod kit_snapshots;
mod scope;
mod selfupdate;
mod snippets;
mod transfer;
mod tree;
mod ui;

use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, EventStream, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use futures::StreamExt;
use ratatui::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "qurry-tui {} [--config-dir DIR] [--no-update-check]\n       qurry-tui --update     download and install the latest release\n\nTerminal UI for Qurry (PostgreSQL / MySQL / Redis). Shares data sources with the Qurry app.\nA new version is checked every hour (off: --no-update-check or QURRY_NO_UPDATE_CHECK=1).\nPress F1 inside for this keymap.\n{}",
            env!("CARGO_PKG_VERSION"),
            help::text()
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("qurry-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--update") {
        return selfupdate::run_update().await;
    }
    let check_update = !args.iter().any(|a| a == "--no-update-check") && !qurry_core::update::disabled() && !selfupdate::current_exe().is_some_and(|e| selfupdate::from_source(&e));
    let dir = args.iter().position(|a| a == "--config-dir").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    let cfg_dir = dir.clone().unwrap_or_else(qurry_core::config_dir);
    // Keep engine logs from breaking the screen
    qurry_core::QUIET.store(true, std::sync::atomic::Ordering::Relaxed);
    let state = Arc::new(qurry_core::open_state(dir).map_err(|e| std::io::Error::other(e.to_string()))?);

    enable_raw_mode()?;
    let mut out = std::io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    // Turn on extended key reporting in terminals that can distinguish Ctrl+Enter etc. (kitty, WezTerm, iTerm2, Ghostty…)
    let enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        let _ = execute!(out, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES));
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        let _ = restore();
        hook(p);
    }));
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let res = run(&mut term, state, check_update.then_some(cfg_dir)).await;
    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    restore()?;
    // After an in-app update, relaunch the new executable with the same arguments
    if res? {
        let exe = selfupdate::current_exe().ok_or_else(|| std::io::Error::other("cannot find this executable"))?;
        use std::os::unix::process::CommandExt;
        return Err(std::process::Command::new(exe).args(&args[1..]).exec());
    }
    Ok(())
}

fn restore() -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)
}

async fn run(term: &mut Terminal<CrosstermBackend<std::io::Stdout>>, state: Arc<qurry_core::app::AppState>, update_dir: Option<std::path::PathBuf>) -> std::io::Result<bool> {
    // Return value = whether to relaunch (in-app update finished)
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<app::Apply>();
    let mut app = app::App::new(state.clone(), tx.clone());
    // Check for a new version (at launch + hourly, in the background). Failures are silently ignored
    if let Some(dir) = update_dir {
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(Some(r)) = qurry_core::update::check(&dir, env!("CARGO_PKG_VERSION"), false).await {
                    if tx.send(Box::new(move |a: &mut app::App| a.update = Some(r.version))).is_err() {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
            }
        });
    }
    // keep-alive / auto-disconnect (same rules as the GUI)
    {
        let tx = tx.clone();
        let st = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                // Save open tabs·splits (only when changed; mostly survives even if the terminal is just closed)
                // Workspace list: pull in changes made in the GUI
                if tx.send(Box::new(|a: &mut app::App| {
                    a.ws_pull();
                    a.save_workspace();
                })).is_err() {
                    break;
                }
                let dropped = qurry_core::app::housekeeping(&st).await;
                if !dropped.is_empty() && tx.send(Box::new(move |a: &mut app::App| {
                    for id in &dropped {
                        a.forget_completion(id);
                        a.tree.forget(id);
                        a.tree.expanded.retain(|k| !(k == id || k.starts_with(&format!("{id}|"))));
                    }
                    a.status = format!("Disconnected (idle): {}", dropped.len());
                    a.reload_conns();
                })).is_err() {
                    break;
                }
            }
        });
    }
    let mut events = EventStream::new();
    loop {
        term.draw(|f| ui::draw(f, &mut app))?;
        tokio::select! {
            ev = events.next() => match ev {
                Some(Ok(ev)) => input::on_event(&mut app, ev),
                Some(Err(e)) => return Err(e),
                None => break,
            },
            Some(apply) = rx.recv() => {
                apply(&mut app);
                while let Ok(a) = rx.try_recv() {
                    a(&mut app);
                }
            }
            // While running, redraw the elapsed time
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)), if app.any_running() => {}
        }
        // If workspaces were created · deleted · renamed · had Data Sources changed, write to the list shared with the GUI
        app.ws_push();
        if app.quit {
            break;
        }
    }
    app.ws_push();
    app.save_workspace();
    Ok(app.restart)
}
