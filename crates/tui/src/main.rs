//! Qurry TUI — DataGrip-style terminal client. Uses the same engine (qurry-core)·same Data Sources as the GUI.
mod app;
mod cli;
mod datatools;
mod designer;
mod dba;
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
#[cfg(test)]
mod parity_dump;
#[cfg(test)]
mod tests;
mod scope;
mod structure;
mod selfupdate;
mod snippets;
mod theme;
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
    let cli = cli::parse(&args).unwrap_or_else(|e| fail(&e));
    if cli.help {
        println!(
            "qurry-tui {}\n{}\n\nTerminal UI for Qurry (PostgreSQL / MySQL / Redis). Shares data sources with the Qurry app.\nA new version is checked every hour (off: --no-update-check or QURRY_NO_UPDATE_CHECK=1).\nColors follow the terminal: COLORTERM=truecolor for 24-bit, TERM=…-256color for 256, otherwise 16; NO_COLOR=1 turns them off. Light colors when COLORFGBG says the background is light (⌃k › Theme… picks Auto, Dark or Light).\nPress F1 inside for this keymap.\n{}",
            env!("CARGO_PKG_VERSION"),
            cli::USAGE,
            help::text()
        );
        return Ok(());
    }
    if cli.version {
        println!("qurry-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let dir = cli.config_dir.clone();
    let cfg_dir = dir.clone().unwrap_or_else(qurry_core::config_dir);
    if cli.update {
        return selfupdate::run_update(qurry_core::store::Store::new(cfg_dir).update_channel()).await;
    }
    let check_update = !cli.no_update_check && !qurry_core::update::disabled() && !selfupdate::current_exe().is_some_and(|e| selfupdate::from_source(&e));
    // Before the screen is taken: a bad URL says why on the terminal
    let temporary = cli.start.as_ref().map(cli::temporary).transpose().unwrap_or_else(|e| fail(&e));
    // Also on the terminal, where it stays after qurry-tui exits (the status line is one row)
    for n in temporary.iter().flat_map(|t| &t.notes) {
        eprintln!("qurry-tui: {n}");
    }
    // Keep engine logs from breaking the screen
    qurry_core::QUIET.store(true, std::sync::atomic::Ordering::Relaxed);
    let state = Arc::new(qurry_core::open_state(dir).map_err(|e| std::io::Error::other(e.to_string()))?);
    let start = temporary.map(|t| {
        let notes = t.notes.join(" ");
        (state.open_temporary(t).id, cli.start.as_ref().is_some_and(|s| s.incident), notes)
    });

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
    // AWS SSM tunnels run the aws CLI in its own process group: stop it on a panic, a closed terminal (SIGHUP) or SIGTERM too
    qurry_core::ssm::kill_on_panic();
    tokio::spawn(qurry_core::ssm::kill_on_signals());
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let res = run(&mut term, state, check_update.then_some(cfg_dir), start).await;
    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    restore()?;
    // Background tasks may still hold the state (and its tunnels), and exec() drops nothing
    qurry_core::ssm::kill_all();
    // After an in-app update, relaunch the new executable with the same arguments
    if res? {
        let exe = selfupdate::current_exe().ok_or_else(|| std::io::Error::other("cannot find this executable"))?;
        use std::os::unix::process::CommandExt;
        return Err(std::process::Command::new(exe).args(&args[1..]).exec());
    }
    Ok(())
}

/// A command-line mistake: say it on the terminal and stop (exit code 2, as for a usage error)
fn fail(e: &str) -> ! {
    eprintln!("qurry-tui: {e}\nRun qurry-tui --help for the forms it takes.");
    std::process::exit(2)
}

fn restore() -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)
}

async fn run(term: &mut Terminal<CrosstermBackend<std::io::Stdout>>, state: Arc<qurry_core::app::AppState>, update_dir: Option<std::path::PathBuf>, start: Option<(String, bool, String)>) -> std::io::Result<bool> {
    // Return value = whether to relaunch (in-app update finished)
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<app::Apply>();
    let mut app = app::App::new(state.clone(), tx.clone());
    app.color = theme::ColorMode::from_env();
    app.auto_light = theme::auto_light(|k| std::env::var(k).ok());
    if let Some(n) = app.startup_notice() {
        app.status = n;
    }
    if let Some((id, incident, notes)) = start {
        app.start_with(id, incident);
        // The notes first, then a startup notice that was already there
        app.status = match (notes.is_empty(), app.status.is_empty()) {
            (true, true) => "Temporary data source: not saved, gone when qurry-tui exits".into(),
            (true, false) => std::mem::take(&mut app.status),
            (false, true) => notes,
            (false, false) => format!("{notes} · {}", app.status),
        };
    }
    // Check for a new version (at launch + hourly, in the background). Failures are silently ignored
    if let Some(dir) = update_dir {
        let tx = tx.clone();
        let st = state.clone();
        tokio::spawn(async move {
            loop {
                // No update clears the ⬆ badge too: the GUI may have switched the shared channel since the last check
                if let Ok(r) = qurry_core::update::check(&dir, env!("CARGO_PKG_VERSION"), st.store.update_channel(), false).await
                    && tx.send(Box::new(move |a: &mut app::App| a.update = r.map(|r| r.version))).is_err()
                {
                    break;
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
