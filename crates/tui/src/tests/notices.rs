//! Popups an async result opens while a notice (a failed Commit, a warning) is shown: they wait under it, they never replace it
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
