//! Reformat Code (⌥⇧F) and the keyword case as you type in the console editor: the GUI's rules (qurry_shared::format), each change
//! one ⌃u step; the keyword case is picked in ⌃k › Keyword Case… and shared with the GUI through settings.json
use crate::app::{Action, App, Popup};
use crate::testkit::Harness;
use qurry_core::format::KeywordCase;

/// ⌥⇧F without a selection: the statement at the cursor only (keywords upper case while the setting is Off); ⌃u undoes it in one
/// step, ⌃r redoes it in one
#[tokio::test]
async fn reformat_key_lays_out_the_statement_at_the_cursor_one_undo() {
    let mut h = Harness::new().await;
    let text = "select 1;\n\nselect a, b from t where x = 'it''s'\n\nselect 3";
    h.console_with(text, 2, 5);
    h.key("Alt+Shift+F");
    let done = "select 1;\n\nSELECT\n    a,\n    b\nFROM\n    t\nWHERE\n    x = 'it''s'\n\nselect 3";
    assert_eq!(h.console_text(), done);
    assert!(h.app.status.starts_with("Reformatted"), "{}", h.app.status);
    h.key("Ctrl+u");
    assert_eq!(h.console_text(), text);
    h.key("Ctrl+r");
    assert_eq!(h.console_text(), done);
    // Laid out already
    h.key("Alt+Shift+F");
    assert_eq!(h.console_text(), done);
    assert!(h.app.status.starts_with("Nothing to reformat"), "{}", h.app.status);
}

/// With a selection only the selected text is reformatted, in the keyword case the setting has
#[tokio::test]
async fn reformat_key_lays_out_the_selection() {
    let mut h = Harness::new().await;
    h.app.keyword_case = KeywordCase::Lower;
    h.console_with("SELECT A FROM B; SELECT C FROM D", 0, 0);
    for _ in 0..15 {
        h.key("Shift+Right");
    }
    h.key("Alt+Shift+F");
    assert_eq!(h.console_text(), "select\n    A\nfrom\n    B; SELECT C FROM D");
    h.key("Ctrl+u");
    assert_eq!(h.console_text(), "SELECT A FROM B; SELECT C FROM D");
    // Also from the palette, on the statement at the cursor; outside a console it says where it works
    h.app.run_action(Action::Reformat);
    assert_eq!(h.console_text(), "select\n    A\nfrom\n    B; SELECT C FROM D");
    h.table_tab("t", &["id"], &[&["1"]], &[]);
    h.app.run_action(Action::Reformat);
    assert!(h.app.status.starts_with("Reformat Code works in a console"), "{}", h.app.status);
}

/// Keyword case UPPER: a space, newline or punctuation after a keyword re-cases it; not in strings, comments or quoted names, not
/// a name after a dot, not a word still being typed. ⌃u undoes the case change in one step, then the character that made it
#[tokio::test]
async fn keyword_case_as_you_type() {
    let mut h = Harness::new().await;
    h.app.keyword_case = KeywordCase::Upper;
    h.console_tab();
    h.type_str("select id, 'select from' from users u -- where x\n");
    h.type_str("where u.order = 1 and id in(");
    // `(` closes itself (pairs) and still ends the keyword before it
    assert_eq!(h.console_text(), "SELECT id, 'select from' FROM users u -- where x\nWHERE u.order = 1 AND id IN()");
    h.type_str("selection");
    assert!(h.console_text().ends_with("IN(selection)"));
    // Undo: the case change in one step (the word as typed), then the space
    h.console_with("", 0, 0);
    h.type_str("from ");
    assert_eq!(h.console_text(), "FROM ");
    h.key("Ctrl+u");
    assert_eq!(h.console_text(), "from ");
    h.key("Ctrl+u");
    assert_eq!(h.console_text(), "from");
    // Off: typing stays as it is
    h.app.keyword_case = KeywordCase::Off;
    h.console_with("", 0, 0);
    h.type_str("select a from b ");
    assert_eq!(h.console_text(), "select a from b ");
}

/// ⌃k › Keyword Case…: opens on the saved case, the pick is saved in settings.json for the GUI too and read again at the next start
#[tokio::test]
async fn keyword_case_is_picked_from_the_palette() {
    let mut h = Harness::new().await;
    h.app.run_action(Action::KeywordCase);
    let s = h.screen(120, 30);
    assert!(s.contains("Keyword Case") && s.contains("● Off") && s.contains("  UPPER") && s.contains("  lower"), "{s}");
    h.key("Down");
    h.key("Enter");
    assert_eq!(h.app.keyword_case, KeywordCase::Upper);
    assert_eq!(h.app.state.store.keyword_case(), KeywordCase::Upper);
    assert!(h.app.status.starts_with("Keyword case: UPPER — saved for the GUI too"), "{}", h.app.status);
    // The GUI picked lower meanwhile: the palette shows it, and the next start reads it
    h.app.state.store.set_keyword_case(KeywordCase::Lower).unwrap();
    h.app.run_action(Action::KeywordCase);
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    assert_eq!(p.items[p.sel].label, "● lower");
    h.key("Esc");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    assert_eq!(App::new(h.app.state.clone(), tx).keyword_case, KeywordCase::Lower);
}

/// A keymap.json from before 0.3.2 that gives ⌥⇧F to another command keeps it there: Reformat Code goes without a key, and the
/// status line at startup says so and how to give it one
#[tokio::test]
async fn a_saved_key_stays_when_a_new_default_wants_it() {
    let h = Harness::new().await;
    std::fs::write(h.app.state.store.dir().join("keymap.json"), r#"{"tui": {"Find": ["Alt+Shift+F"]}}"#).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let next = App::new(h.app.state.clone(), tx);
    assert_eq!(next.keys.keys(Action::Find), ["Alt+Shift+F"]);
    assert!(next.keys.keys(Action::Reformat).is_empty());
    let f = crate::keys::disp("Alt+Shift+F");
    assert!(next.status.starts_with(&format!("{f} stays on Find / Replace in Editor (regex) (your keymap.json), so Reformat Code")) && next.status.contains("› Keymap gives it one"), "{}", next.status);
    // Without that, the default
    std::fs::remove_file(h.app.state.store.dir().join("keymap.json")).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    assert_eq!(App::new(h.app.state.clone(), tx).keys.keys(Action::Reformat), ["Alt+Shift+F"]);
}
