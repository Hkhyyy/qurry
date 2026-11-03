use crate::app::{Action, App, Popup};
use crate::keys::{Keymap, Slot, COMMANDS};
use qurry_core::keymap::Scope;
use crate::testkit::Harness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;

fn message(h: &Harness) -> Option<String> {
    match &h.app.popup {
        Some(Popup::Help { message, .. }) => message.clone(),
        _ => panic!("keymap window closed"),
    }
}

/// F1 › Enter on a command › new key: saved to keymap.json, read back by the next start; Backspace resets it (nothing left saved)
#[tokio::test]
async fn keymap_rebinding_round_trip() {
    let mut h = Harness::new().await;
    let ctrl_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
    h.key("F1");
    let at = COMMANDS.iter().position(|(s, _)| *s == Slot::global(Action::NewConsole)).unwrap();
    for _ in 0..at {
        h.key("Down");
    }
    h.key("Enter");
    assert!(matches!(h.app.popup, Some(Popup::Help { record: Some(false), .. })));
    // Not a command key: says why and keeps the old binding
    h.key("x");
    assert!(message(&h).unwrap().contains("can't be a command key"));
    h.key("Enter");
    h.key("Ctrl+J");
    assert_eq!(message(&h).unwrap(), format!("New Query Console → {}", h.app.keys.label(Action::NewConsole)));
    assert_eq!(h.app.keys.keys(Action::NewConsole), ["Ctrl+J"]);
    let saved = h.app.state.store.keymap("tui").unwrap();
    assert_eq!(saved.get("NewConsole"), Some(&vec!["Ctrl+J".to_string()]));
    // The next start reads it back
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let next = App::new(h.app.state.clone(), tx);
    assert_eq!(next.keys.lookup(&ctrl_j), Some(Action::NewConsole));
    assert_eq!(next.keys.lookup(&KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)), None);
    assert_eq!(Keymap::new(&saved).overrides(), saved);
    // Reset to the default
    h.key("Backspace");
    assert_eq!(message(&h).unwrap(), format!("New Query Console reset to {}", h.app.keys.label(Action::NewConsole)));
    assert_eq!(h.app.keys.keys(Action::NewConsole), ["Ctrl+L"]);
    assert!(h.app.state.store.keymap("tui").unwrap().is_empty());
    h.key("Esc");
    assert!(h.app.popup.is_none());
}

/// A keymap.json saved by an earlier version loads to the same TUI bindings as before (same action ids and defaults; lowercase notation normalized)
#[tokio::test]
async fn saved_keymap_file_loads_the_same() {
    let h = Harness::new().await;
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../shared/tests/keymap/keymap.json"), h.app.state.store.dir().join("keymap.json")).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let next = App::new(h.app.state.clone(), tx);
    let now: BTreeMap<String, Vec<String>> = COMMANDS.iter().map(|(s, _)| (s.id(), next.keys.slot_keys(*s).to_vec())).collect();
    let want: serde_json::Value = serde_json::from_str(include_str!("../../../shared/tests/keymap/effective.json")).unwrap();
    let want: BTreeMap<String, Vec<String>> = serde_json::from_value(want["tui"].clone()).unwrap();
    // On failure: an action id or a default key changed, so users' keymap.json would load differently.
    // Keep the old id (keymap.json stores the Action's name) and the old default; change effective.json only for an intended default change
    let old: BTreeMap<String, Vec<String>> = now.iter().filter(|(id, _)| want.contains_key(*id)).map(|(id, k)| (id.clone(), k.clone())).collect();
    assert_eq!(old, want);
    // Lines added since (commands without a default key, pane keys) start at their defaults: the file has nothing for them
    for (s, keys) in COMMANDS.iter().filter(|(s, _)| !want.contains_key(&s.id())) {
        assert_eq!(next.keys.slot_keys(*s), *keys, "{}", s.id());
    }
}

/// A command without a default key gets one in F1 and then runs from it; it is saved and read back like any other
#[tokio::test]
async fn binding_an_unbound_command_runs_it() {
    let mut h = Harness::new().await;
    h.console_tab();
    h.type_str("select 1");
    assert_eq!(h.app.keys.label(Action::ClearConsole), "—");
    h.key("F1");
    let at = COMMANDS.iter().position(|(s, _)| *s == Slot::global(Action::ClearConsole)).unwrap();
    for _ in 0..at {
        h.key("Down");
    }
    h.key("Enter");
    h.key("Alt+j");
    assert_eq!(message(&h).unwrap(), format!("Clear Console (delete all text) → {}", h.app.keys.label(Action::ClearConsole)));
    h.key("Esc");
    h.key("Alt+j");
    assert_eq!(h.app.ran.last(), Some(&Action::ClearConsole));
    assert_eq!(h.console_text(), "");
    assert_eq!(h.app.state.store.keymap("tui").unwrap().get("ClearConsole"), Some(&vec!["Alt+J".to_string()]));
    // It shows in the palette, and the next start reads it back
    h.app.palette_actions();
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    assert_eq!(p.items.iter().find(|i| i.label.starts_with("Clear Console")).unwrap().hint, h.app.keys.label(Action::ClearConsole));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    assert_eq!(App::new(h.app.state.clone(), tx).keys.lookup(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT)), Some(Action::ClearConsole));
}

/// The explorer's letters are pane keys: F1 rebinds one, the old letter stops, the new one runs it in the explorer only.
/// Fixed letters (j moves) can't be taken
#[tokio::test]
async fn explorer_letter_keys_are_rebindable() {
    let mut h = Harness::new().await;
    h.app.focus = crate::app::Focus::Tree;
    h.key("c");
    assert_eq!(h.app.ran.last(), Some(&Action::Connect));
    h.key("F1");
    let at = COMMANDS.iter().position(|(s, _)| *s == Slot { action: Action::Connect, pane: Scope::Tree }).unwrap();
    for _ in 0..at {
        h.key("Down");
    }
    h.key("Enter");
    h.key("j");
    assert!(message(&h).unwrap().contains("can't be a key in the explorer"), "{:?}", message(&h));
    h.key("Enter");
    h.key("o");
    assert_eq!(message(&h).unwrap(), "Connect → o");
    h.key("Esc");
    h.app.ran.clear();
    h.key("c");
    assert!(h.app.ran.is_empty(), "{:?}", h.app.ran);
    h.key("o");
    assert_eq!(h.app.ran.last(), Some(&Action::Connect));
    assert_eq!(h.app.state.store.keymap("tui").unwrap().get("tree.Connect"), Some(&vec!["O".to_string()]));
}

/// A keymap.json that doesn't parse: the default keys work, the status line says how to get the saved keys back, and the file is
/// left as it is (rebinding refuses to overwrite it)
#[tokio::test]
async fn broken_keymap_file_says_how_to_recover() {
    let h = Harness::new().await;
    let path = h.app.state.store.dir().join("keymap.json");
    std::fs::write(&path, "{ \"tui\": { \"NewConsole\": [\"Ctrl+J\"] ").unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut next = App::new(h.app.state.clone(), tx);
    assert!(next.status.contains("keymap.json ignored") && next.status.contains(&format!("fix or delete {}", path.display())), "{}", next.status);
    assert_eq!(next.keys.keys(Action::NewConsole), ["Ctrl+L"]);
    next.keys.bind(Slot::global(Action::NewConsole), "Ctrl+J".into(), false);
    assert!(next.save_keys().is_err());
    assert!(std::fs::read_to_string(&path).unwrap().contains("Ctrl+J"));
}

/// A pane key can't take the Keymap window's or the palette's key (F1, ⌃k, ?): F1 gone anywhere would leave no way back into F1
#[tokio::test]
async fn pane_key_cannot_take_the_keymap_or_palette_key() {
    let mut h = Harness::new().await;
    h.key("F1");
    let at = COMMANDS.iter().position(|(s, _)| *s == Slot { action: Action::Connect, pane: Scope::Tree }).unwrap();
    for _ in 0..at {
        h.key("Down");
    }
    for (key, owner) in [("F1", "Keymap"), ("Ctrl+K", "Actions Palette"), ("?", "Keymap")] {
        h.key("Enter");
        h.key(key);
        let m = message(&h).unwrap();
        assert!(m.contains(&format!("is the key of {owner}")), "{key}: {m}");
    }
    assert_eq!(h.app.keys.keys(Action::Help), ["F1"]);
    assert_eq!(h.app.keys.keys(Action::Palette), ["Ctrl+K"]);
    assert_eq!(h.app.keys.slot_keys(Slot { action: Action::Connect, pane: Scope::Tree }), ["C"]);
    // The F1 window still opens from F1 after closing it
    h.key("Esc");
    h.key("Esc");
    assert!(h.app.popup.is_none());
    h.key("F1");
    assert!(matches!(h.app.popup, Some(Popup::Help { .. })));
}
