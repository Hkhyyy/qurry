//! Qurry TUI — DataGrip 식 터미널 클라이언트. GUI 와 같은 엔진(qurry-core)·같은 Data Source 를 쓴다.
mod app;
mod complete;
mod datatools;
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
    // 엔진 로그가 화면을 깨지 않도록
    qurry_core::QUIET.store(true, std::sync::atomic::Ordering::Relaxed);
    let state = Arc::new(qurry_core::open_state(dir).map_err(|e| std::io::Error::other(e.to_string()))?);

    enable_raw_mode()?;
    let mut out = std::io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    // Ctrl+Enter 등을 구분할 수 있는 터미널(kitty, WezTerm, iTerm2, Ghostty…)에서는 확장 키 보고를 켠다
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
    // 앱 안 업데이트를 마쳤으면 새 실행 파일로 같은 인자를 주고 다시 켠다
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
    // 돌려주는 값 = 다시 켤지 (앱 안 업데이트를 마쳤다)
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<app::Apply>();
    let mut app = app::App::new(state.clone(), tx.clone());
    // 새 버전 확인 (켤 때 + 1시간마다, 백그라운드). 실패해도 조용히 넘어간다
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
    // keep-alive / auto-disconnect (GUI 와 같은 규칙)
    {
        let tx = tx.clone();
        let st = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                // 열어 둔 탭·분할을 저장 (바뀌었을 때만. 터미널을 그냥 닫아도 거의 남는다)
                if tx.send(Box::new(|a: &mut app::App| a.save_workspace())).is_err() {
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
        }
        if app.quit {
            break;
        }
    }
    app.save_workspace();
    Ok(app.restart)
}
