//! The command list: every rebindable command fires through its default key, and the palette list and the --help / F1 order
//! stay as they are (goldens next to this file; run with QURRY_TUI_BLESS=1 to rewrite them after an intended change)
use crate::app::{Action, Focus, PAction, Popup, TabKind, ACTIONS};
use crate::keys::{action_id, Slot, COMMANDS, FORCE_MAC_GLYPHS};
use qurry_core::keymap::Scope;
use crate::testkit::Harness;

/// Compares with a golden file in src/tests/, or rewrites it with QURRY_TUI_BLESS=1
fn golden(file: &str, got: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests").join(file);
    if std::env::var_os("QURRY_TUI_BLESS").is_some() {
        std::fs::write(&path, got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(got == want, "{file} changed (if that's intended, run the test with QURRY_TUI_BLESS=1 and commit the file):\n{got}");
}

/// Every Action variant has exactly one ActionDef (variants read from app/mod.rs: the enum has no variant list of its own), and each
/// default key of each rebindable command runs that command (from a fresh console, so no popup or earlier command is in the way)
#[tokio::test]
async fn every_action_has_one_def_and_fires_through_its_default_keys() {
    let src = include_str!("../app/mod.rs");
    let body = src.split("pub enum Action {").nth(1).and_then(|s| s.split("\n}").next()).expect("pub enum Action in app/mod.rs");
    let variants: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).map(|l| l.trim_end_matches(',')).collect();
    for v in &variants {
        let n = ACTIONS.iter().filter(|d| action_id(d.action) == *v).count();
        assert_eq!(n, 1, "Action::{v} has {n} ActionDefs: give it exactly one in ACTIONS (crates/tui/src/app/actions.rs)");
    }
    assert_eq!(ACTIONS.len(), variants.len(), "ACTIONS has a row for an Action that isn't in the enum");
    for (s, keys) in COMMANDS.iter() {
        for key in keys.iter() {
            let mut h = Harness::new().await;
            // Keys anywhere from a console, pane keys from their pane
            match s.pane {
                Scope::Tree => h.app.focus = Focus::Tree,
                Scope::Grid => drop(h.table_tab("t", &["id"], &[&["1"]], &["id"])),
                _ => drop(h.console_tab()),
            }
            h.key(&typed(key));
            assert_eq!(h.app.ran.first(), Some(&s.action), "{key} should run {:?} ({:?})", s.action, s.pane);
        }
    }
}

/// The key the terminal sends for a COMMANDS spec: the table writes letters uppercase ("Alt+N"), without Shift it's the lowercase one
fn typed(key: &str) -> String {
    match key.rsplit_once('+') {
        Some((m, l)) if !m.contains("Shift") && l.len() == 1 => format!("{m}+{}", l.to_ascii_lowercase()),
        None if key.len() == 1 => key.to_ascii_lowercase(),
        _ => key.to_string(),
    }
}

/// `ran` is recorded before the ActionDef runs, so the test above can't tell a row pointing at the wrong function: a few commands'
/// default keys checked by the state they leave (the other commands are covered by the flow tests next to their code)
#[tokio::test]
async fn default_keys_apply_their_command() {
    type Applied = fn(&Harness) -> bool;
    let cases: [(Action, Applied); 7] = [
        (Action::ToggleOutput, |h| matches!(&h.app.tabs[h.app.active].kind, TabKind::Console(c) if !c.show_output)),
        (Action::Help, |h| matches!(h.app.popup, Some(Popup::Help { .. }))),
        (Action::Palette, |h| matches!(h.app.popup, Some(Popup::Palette(_)))),
        (Action::Find, |h| matches!(h.app.popup, Some(Popup::Find(_)))),
        (Action::ToggleExplorer, |h| h.app.focus == Focus::Tree),
        // A pair that differs only in its argument (42 = the starting split)
        (Action::ShrinkEditor, |h| h.app.console_pct < 42),
        (Action::GrowEditor, |h| h.app.console_pct > 42),
    ];
    for (a, applied) in cases {
        let key = COMMANDS.iter().find(|d| d.0 == Slot::global(a)).and_then(|d| d.1.first()).unwrap_or_else(|| panic!("{a:?} has no default key"));
        let mut h = Harness::new().await;
        h.console_tab();
        h.key(&typed(key));
        assert!(applied(&h), "{key} ran {a:?} but didn't apply it");
    }
}

/// ⌃k lists the same commands, names and hints in the same order
#[tokio::test]
async fn palette_list_is_unchanged() {
    FORCE_MAC_GLYPHS.set(true);
    let mut h = Harness::new().await;
    h.app.palette_actions();
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    let list: String = p.items.iter().map(|i| format!("{:?}\t{}\t{}\n", match i.action { PAction::Act(a) => a, _ => panic!("not a command") }, i.label, i.hint)).collect();
    FORCE_MAC_GLYPHS.set(false);
    golden("actions_palette.txt", &list);
}

/// ⌃k › Show Log says where the log file is on the status line (nothing is opened in a test)
#[tokio::test]
async fn show_log_puts_the_path_on_the_status_line() {
    let mut h = Harness::new().await;
    let file = std::env::temp_dir().join(format!("qurry-show-log-{}", uuid::Uuid::new_v4())).join("qurry.log");
    h.app.log_file = file.clone();
    h.key("Ctrl+K");
    h.type_str("Show Log");
    h.key("Enter");
    assert_eq!(h.app.ran.last(), Some(&Action::ShowLog));
    assert_eq!(h.app.status, format!("Show Log: {}", file.display()));
    assert!(!file.parent().unwrap().exists(), "a test opens nothing and creates no folder");
    assert!(h.screen(120, 30).contains("Show Log: "), "the path is on the status line");
}

/// --help (and the F1 window, drawn from the same COMMANDS) lists the commands in keys::ORDER with the same names
#[test]
fn help_text_is_unchanged() {
    FORCE_MAC_GLYPHS.set(true);
    let text = crate::help::text();
    FORCE_MAC_GLYPHS.set(false);
    golden("actions_help.txt", &text);
}

/// Key to applied state, 10k keys: a command early in keys::ORDER (⌥o output toggle), one near the end (⌃⌥→ pane right), and a key
/// that is no command (F12, ignored by the editor). Debug build numbers; run with --ignored --nocapture
#[tokio::test]
#[ignore]
async fn key_dispatch_bench() {
    const N: u32 = 10_000;
    let mut h = Harness::new().await;
    let id = h.console_tab();
    let shown = |h: &Harness| matches!(&h.app.tabs[h.app.tab_index(id).unwrap()].kind, TabKind::Console(c) if c.show_output);
    let before = shown(&h);
    // Lowercase: "Alt+O" is the Shift form, bound to nothing
    for key in ["Alt+o", "Ctrl+Alt+Right", "F12"] {
        let t = std::time::Instant::now();
        for _ in 0..N {
            h.key(key);
        }
        let ns = t.elapsed().as_nanos() / N as u128;
        println!("bench {key:<16} {ns:>7} ns/key");
    }
    // Every key applied: 10k output toggles end where they started, each one counted
    assert_eq!(shown(&h), before);
    assert_eq!(h.app.ran.iter().filter(|a| **a == Action::ToggleOutput).count(), N as usize);
}
