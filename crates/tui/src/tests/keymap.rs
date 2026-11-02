use crate::app::{Action, App, Popup};
use crate::keys::{DEFAULT_KEYS, Keymap};
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
    let at = DEFAULT_KEYS.iter().position(|(a, _)| *a == Action::NewConsole).unwrap();
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
    let now: BTreeMap<String, Vec<String>> = DEFAULT_KEYS.iter().map(|(a, _)| (crate::keys::action_id(*a), next.keys.keys(*a).to_vec())).collect();
    let want: serde_json::Value = serde_json::from_str(include_str!("../../../shared/tests/keymap/effective.json")).unwrap();
    let want: BTreeMap<String, Vec<String>> = serde_json::from_value(want["tui"].clone()).unwrap();
    // On failure: an action id or a default key changed, so users' keymap.json would load differently.
    // Keep the old id (keymap.json stores the Action's name) and the old default; change effective.json only for an intended default change
    assert_eq!(now, want);
}
