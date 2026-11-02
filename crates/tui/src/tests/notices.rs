//! Popups an async result opens while a notice (a failed Commit, a warning) is shown: they wait under it, they never replace it.
//! A confirm that comes up on its own takes y / Enter only after a pause, and waits while another popup is being typed in
use crate::app::Popup;
use crate::testkit::Harness;

/// The Production confirm arrives after the row counts, while a failed-Commit notice is open: the notice stays on top and the confirm
/// comes up once it is closed
#[tokio::test]
async fn production_confirm_waits_under_a_notice() {
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({
        "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d", "options": { "confirmChanges": true }
    }))
    .unwrap();
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let id = h.app.restore_console("p", "d".into(), "console".into(), "delete from t where id = 1").unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    h.key("F5");
    h.app.notice("Commit outcome unknown".into(), "connection lost while committing".into());
    for _ in 0..100 {
        h.settle().await;
        if matches!(&h.app.popup, Some(Popup::Note { .. })) {
            break;
        }
    }
    let Some(Popup::Note { title, back, .. }) = &h.app.popup else { panic!("notice gone: {}", h.app.status) };
    assert_eq!(title, "Commit outcome unknown");
    assert!(matches!(&**back, Popup::Confirm { text, .. } if text.starts_with("Production data source \"PROD\"")));
    h.key("Esc");
    assert!(matches!(&h.app.popup, Some(Popup::Confirm { .. })));
}

/// Database lists that come in while a notice covers the palette still reach it (loading ends, nothing waits forever)
#[tokio::test]
async fn palette_under_a_notice_gets_its_databases() {
    let mut h = Harness::new().await;
    h.app.popup = Some(Popup::Palette(crate::app::Palette { title: "Go to Database Object".into(), input: Default::default(), items: vec![], sel: 0, loading: 1, cache: Default::default() }));
    h.app.notice("Ran, with a warning".into(), "…".into());
    let cfg = serde_json::from_value(serde_json::json!({ "id": "r", "name": "cache", "kind": "redis", "host": "127.0.0.1", "port": 1, "user": "" })).unwrap();
    h.app.index_into_palette(&cfg, vec![]);
    h.key("Esc");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("palette gone") };
    assert_eq!(p.loading, 0);
}

/// Production data source PROD with a console "delete from t where id = 1" (the counts fail fast: nothing listens on port 1)
async fn production_console() -> Harness {
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({
        "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d", "options": { "confirmChanges": true }
    }))
    .unwrap();
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let id = h.app.restore_console("p", "d".into(), "console".into(), "delete from t where id = 1").unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    h
}

/// The confirm has been on screen for a while without a key (as if TLS_ASK_GRACE had passed)
fn read_it(h: &mut Harness) {
    let Some(Popup::Confirm { opened: Some(t), .. }) = &mut h.app.popup else { panic!("no quiet confirm: {}", h.app.status) };
    *t -= crate::app::App::TLS_ASK_GRACE;
}

fn production_confirm() -> Popup {
    Popup::Confirm { opened: None, text: "Production data source \"PROD\" — this changes data:\n\n  delete from t\n    → 3 rows\n\nRun it?".into(), on_yes: Box::new(|a| a.status = "ran".into()) }
}

/// The user goes on typing while the rows are counted: the Enter that comes right after the confirm shows (meant for the editor)
/// doesn't run the write; one after a pause does
#[tokio::test]
async fn production_confirm_takes_enter_only_after_a_pause() {
    let mut h = production_console().await;
    h.key("F5");
    for _ in 0..100 {
        h.settle().await;
        if matches!(&h.app.popup, Some(Popup::Confirm { .. })) {
            break;
        }
    }
    h.key("Enter");
    h.key("x");
    h.key("Enter");
    assert!(matches!(&h.app.popup, Some(Popup::Confirm { .. })), "{}", h.app.status);
    assert_eq!(h.app.status, "Wait a moment, then answer (n / Esc cancels)");
    read_it(&mut h);
    h.key("Enter");
    assert!(h.app.popup.is_none());
}

/// A double Enter on a notice with a confirm under it: the first closes the notice, the second doesn't answer the confirm
#[tokio::test]
async fn a_second_enter_on_a_notice_doesnt_answer_the_confirm_under_it() {
    let mut h = Harness::new().await;
    h.app.notice("Commit outcome unknown".into(), "connection lost while committing".into());
    h.app.open_after_notices(production_confirm());
    // Even long after it was queued
    let Some(Popup::Note { back, .. }) = &mut h.app.popup else { panic!("notice gone") };
    let Popup::Confirm { opened: Some(t), .. } = &mut **back else { panic!("no confirm under the notice") };
    *t -= std::time::Duration::from_secs(5);
    h.key("Enter");
    h.key("Enter");
    assert!(matches!(&h.app.popup, Some(Popup::Confirm { .. })) && h.app.status != "ran", "{}", h.app.status);
    read_it(&mut h);
    h.key("y");
    assert_eq!(h.app.status, "ran");
}

/// While the user types in another popup (an input, a form), an async confirm waits instead of replacing it, and comes up quiet
/// once that one is closed
#[tokio::test]
async fn a_confirm_waits_for_the_popup_being_typed_in() {
    let mut h = Harness::new().await;
    h.app.input("Rename", "abc", |a, v| a.status = format!("renamed to {v}"));
    h.app.open_after_notices(production_confirm());
    assert!(matches!(&h.app.popup, Some(Popup::Input { .. })));
    h.type_str("d");
    h.key("Enter");
    assert_eq!(h.app.status, "renamed to abcd");
    assert!(matches!(&h.app.popup, Some(Popup::Confirm { .. })));
    h.key("Enter");
    assert!(matches!(&h.app.popup, Some(Popup::Confirm { .. })) && h.app.status != "ran", "{}", h.app.status);
    read_it(&mut h);
    h.key("Enter");
    assert_eq!(h.app.status, "ran");
}

/// Tab A (PROD, the delete) and tab B (a console on another Production source, OTHER). F5 in tab A, then Recent Tabs opens before
/// the counts are back, so the confirm waits; the user picks tab B and the palette closes. Returns (A, B) with the confirm read
async fn confirm_waits_while_the_palette_switches_tabs(close_a: bool) -> (Harness, u64, u64) {
    let mut h = production_console().await;
    let a = h.app.tabs[h.app.active].id;
    let cfg = serde_json::from_value(serde_json::json!({
        "id": "o", "name": "OTHER", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d", "options": { "confirmChanges": true }
    }))
    .unwrap();
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let b = h.app.restore_console("o", "d".into(), "other".into(), "select 1").unwrap();
    h.key("F5");
    h.app.palette_tabs();
    for _ in 0..100 {
        h.settle().await;
        if !h.app.waiting.is_empty() {
            break;
        }
    }
    assert!(!h.app.waiting.is_empty(), "the confirm doesn't wait: {}", h.app.status);
    if close_a {
        h.app.close_tab(h.app.tab_index(a).unwrap());
    }
    let Some(Popup::Palette(p)) = &mut h.app.popup else { panic!("palette gone") };
    p.sel = p.items.iter().position(|i| matches!(i.action, crate::app::PAction::Tab(id) if id == b)).unwrap();
    h.key("Enter");
    assert_eq!(h.app.tabs[h.app.active].id, b);
    read_it(&mut h);
    (h, a, b)
}

fn running(h: &Harness, id: u64) -> bool {
    h.app.tab_index(id).is_some_and(|i| matches!(&h.app.tabs[i].kind, crate::app::TabKind::Console(c) if c.running))
}

/// The confirm runs its statements in the console that asked (tab A, shown again), never in the tab active when y is pressed
#[tokio::test]
async fn a_waiting_production_confirm_runs_in_the_console_that_asked() {
    let (mut h, a, b) = confirm_waits_while_the_palette_switches_tabs(false).await;
    h.key("y");
    assert!(!running(&h, b), "ran in tab B: {}", h.app.status);
    assert!(running(&h, a), "tab A didn't run: {}", h.app.status);
    assert_eq!(h.app.tabs[h.app.active].id, a);
}

/// The console that asked was closed meanwhile: y runs nothing and says so
#[tokio::test]
async fn a_waiting_production_confirm_runs_nothing_once_its_console_is_closed() {
    let (mut h, a, b) = confirm_waits_while_the_palette_switches_tabs(true).await;
    assert!(h.app.tab_index(a).is_none());
    h.key("y");
    assert!(!running(&h, b), "ran in tab B: {}", h.app.status);
    assert_eq!(h.app.status, "The console was closed: nothing ran");
}
