use crate::app::{Action, PAction, Popup};
use crate::testkit::Harness;

fn active(h: &Harness) -> &str {
    &h.app.tabs[h.app.active].title
}

/// ⌥→ / ⌥← (and ⌃PgDn / ⌃PgUp) cycle the tabs and wrap around at both ends
#[tokio::test]
async fn tab_cycling_wraps() {
    let mut h = Harness::new().await;
    for t in ["a", "b", "c"] {
        h.table_tab(t, &["id"], &[&["1"]], &["id"]);
    }
    assert_eq!(active(&h), "c");
    h.key("Alt+Right");
    assert_eq!(active(&h), "a");
    h.key("Alt+Left");
    assert_eq!(active(&h), "c");
    h.key("Ctrl+PgUp");
    assert_eq!(active(&h), "b");
    h.key("Ctrl+PgDn");
    assert_eq!(active(&h), "c");
}

/// ⌥\ splits off the current tab to the right; ⌃⌥←/→ and ⌥Tab move between panes; cycling tabs stays inside the focused pane; Unsplit merges back
#[tokio::test]
async fn splits_and_pane_focus() {
    let mut h = Harness::new().await;
    for t in ["a", "b", "c"] {
        h.table_tab(t, &["id"], &[&["1"]], &["id"]);
    }
    h.key("Alt+\\");
    assert_eq!(h.app.panes.leaves().len(), 2);
    assert_eq!(active(&h), "c");
    // Pane moves use the drawn layout
    h.screen(160, 20);
    h.key("Ctrl+Alt+Left");
    assert_eq!(active(&h), "b", "the left pane shows the tab next to the one split off");
    h.key("Alt+Right");
    assert_eq!(active(&h), "a", "cycling stays in the left pane (a, b)");
    h.key("Alt+Right");
    assert_eq!(active(&h), "b");
    h.key("Ctrl+Alt+Right");
    assert_eq!(active(&h), "c");
    // Nothing further right: stays, and says so
    h.key("Ctrl+Alt+Right");
    assert_eq!(active(&h), "c");
    assert!(h.app.status.contains("on the right"), "{}", h.app.status);
    h.key("Alt+Tab");
    assert_eq!(active(&h), "b");
    h.key("Alt+Tab");
    assert_eq!(active(&h), "c");
    // New tabs open in the focused pane
    let g = h.app.focused_group();
    h.table_tab("d", &["id"], &[&["1"]], &["id"]);
    assert_eq!(h.app.tabs[h.app.active].group, g);
    h.app.run_action(Action::Unsplit);
    assert_eq!(h.app.panes.leaves().len(), 1);
    assert!(h.app.tabs.iter().all(|t| t.group == 0));
    assert_eq!(active(&h), "d");
}

/// ⌃k lists every command with its key (not the palette itself); typing filters; Enter runs the command
#[tokio::test]
async fn palette_lists_actions_and_runs_one() {
    let mut h = Harness::new().await;
    h.table_tab("a", &["id"], &[&["1"]], &["id"]);
    h.key("Ctrl+K");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    assert_eq!(p.title, "Actions");
    assert_eq!(p.items.len(), crate::app::ACTIONS.len() - 1);
    assert!(!p.items.iter().any(|i| matches!(i.action, PAction::Act(Action::Palette))));
    let item = |label: &str| p.items.iter().find(|i| i.label == label).unwrap_or_else(|| panic!("no {label}"));
    assert_eq!(item("New Query Console").hint, h.app.keys.label(Action::NewConsole));
    assert_eq!(item("Clear Console (delete all text)").hint, "⌘A ⌫ (console)");
    h.type_str("split right");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    let matches: Vec<_> = crate::app::App::palette_matches(p).iter().map(|i| i.label.clone()).collect();
    assert_eq!(matches[0], "Split Right (focused pane, up to 4)", "{matches:?}");
    let s = h.screen(120, 30);
    assert!(s.contains("Split Right"), "{s}");
    h.key("Enter");
    assert!(h.app.popup.is_none());
    assert_eq!(h.app.panes.leaves().len(), 2);
}
