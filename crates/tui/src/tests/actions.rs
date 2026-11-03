//! The command list: every rebindable command fires through its default key, and the palette list and the --help / F1 order
//! stay as they are (goldens next to this file; run with QURRY_TUI_BLESS=1 to rewrite them after an intended change)
use crate::app::{PAction, Popup, TabKind};
use crate::keys::{DEFAULT_KEYS, FORCE_MAC_GLYPHS};
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

/// Each default key of each rebindable command runs that command (from a fresh console, so no popup or earlier command is in the way)
#[tokio::test]
async fn every_command_fires_through_its_default_keys() {
    for (a, keys) in DEFAULT_KEYS.iter() {
        for key in keys.iter() {
            let mut h = Harness::new().await;
            h.console_tab();
            // The table writes letters uppercase ("Alt+N"); without Shift the terminal sends the lowercase one
            let typed = match key.rsplit_once('+') {
                Some((m, l)) if !m.contains("Shift") && l.len() == 1 => format!("{m}+{}", l.to_ascii_lowercase()),
                _ => key.clone(),
            };
            h.key(&typed);
            assert_eq!(h.app.ran.first(), Some(a), "{key} should run {a:?}");
        }
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

/// --help (and the F1 window, drawn from the same DEFAULT_KEYS) lists the commands in keys::ORDER with the same names
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
    for key in ["Alt+O", "Ctrl+Alt+Right", "F12"] {
        let t = std::time::Instant::now();
        for _ in 0..N {
            h.key(key);
        }
        let ns = t.elapsed().as_nanos() / N as u128;
        println!("bench {key:<16} {ns:>7} ns/key");
    }
    // Every key applied: 10k output toggles end where they started, each one counted
    assert_eq!(shown(&h), before);
    assert_eq!(h.app.ran.iter().filter(|a| **a == crate::app::Action::ToggleOutput).count(), N as usize);
}
