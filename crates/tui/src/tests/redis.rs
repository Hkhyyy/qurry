//! The Redis tab's loads (#220), as a table tab's: Loading… while the keys or a value load (the status line with the time and the
//! Cancel key), a failure in the tab with its first line and that r retries, and a reconnect through the status line (#135)
use crate::app::{Action, Focus, TabKind, WaitFor};
use crate::testkit::Harness;

/// A Redis source "rd" ([DEV] cache, through SSH) and its tab on db0, focused
async fn redis_tab(h: &mut Harness) -> u64 {
    let cfg = serde_json::from_value(serde_json::json!({
        "id": "rd", "name": "[DEV] cache", "kind": "redis", "host": "127.0.0.1", "port": 1, "user": "", "database": "0",
        "ssh": { "enabled": true, "host": "bastion.example.com", "port": 22, "user": "dev" }
    }))
    .unwrap();
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let id = h.redis_tab(&[]);
    let t = h.app.tabs.iter_mut().find(|t| t.id == id).unwrap();
    (t.conn, t.target) = ("rd".into(), "rd::db0".into());
    h.app.focus = Focus::Grid;
    id
}

fn tab(h: &mut Harness, id: u64) -> &mut crate::app::RedisTab {
    match h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        Some(TabKind::Redis(r)) => r,
        _ => panic!("no Redis tab"),
    }
}

/// The tab scans and loads user:1's value; neither answers
fn loading(h: &mut Harness, id: u64) {
    let r = tab(h, id);
    (r.scanning, r.loading) = (true, Some("user:1".into()));
    h.app.spawn_wait("rd::db0", WaitFor::Redis(id), std::future::pending::<()>(), |_, _, _| {});
}

#[tokio::test]
async fn loading_keys_and_a_value_shows_and_the_cancel_key_stops_them() {
    let mut h = Harness::new().await;
    let id = redis_tab(&mut h).await;
    loading(&mut h, id);
    let s = h.screen(120, 20);
    // The key list and the value say Loading…; the status line what loads, how long, and the Cancel key
    assert_eq!(s.matches("Loading…").count(), 2, "{s}");
    let cancel = h.app.keys.label(Action::Cancel);
    assert!(s.contains(&format!("Scanning keys… 0.0 s · {cancel} cancel")), "{s}");
    h.app.run_action(Action::Cancel);
    assert!(h.app.waits.is_empty());
    let r = tab(&mut h, id);
    assert!(!r.scanning && r.loading.is_none());
    assert_eq!(r.key_error, Some(("user:1".into(), "Cancelled".into())));
    assert_eq!(h.app.status, "Cancelled · r retry");
    let s = h.screen(120, 20);
    assert_eq!(s.matches("✗ Cancelled").count(), 2, "{s}");
    assert!(s.contains("r retry") && !s.contains("Loading…"), "{s}");
}

#[tokio::test]
async fn a_failed_scan_key_counts_or_value_says_why_and_r_loads_them_again() {
    let mut h = Harness::new().await;
    let id = redis_tab(&mut h).await;
    let refused = "Redis: Connection refused (os error 61)";
    let r = tab(&mut h, id);
    (r.scan_error, r.counts_error, r.key_error) = (Some(refused.into()), Some(refused.into()), Some(("user:1".into(), refused.into())));
    let s = h.screen(160, 20);
    // The scan's failure covers the key counts': said once in the list, once for the value
    assert_eq!(s.matches(&format!("✗ {refused}")).count(), 2, "{s}");
    assert!(!s.contains("Couldn't read the key counts"), "{s}");
    assert_eq!(s.matches("r retry").count(), 2, "{s}");
    // The keys came, the counts didn't
    tab(&mut h, id).scan_error = None;
    let s = h.screen(160, 20);
    // Cut at the list's edge (the status line has it all)
    assert!(s.contains("✗ Couldn't read the key counts: Redis: Conn"), "{s}");
    h.key("r");
    let r = tab(&mut h, id);
    assert!(r.scanning && r.scan_error.is_none(), "r scans again");
    assert_eq!(r.loading.as_deref(), Some("user:1"), "and loads the value that failed");
    assert!(r.key_error.is_none());
    assert!(h.screen(160, 20).contains("Loading…"));
}

#[tokio::test]
async fn a_reconnect_says_so_on_the_status_line_and_its_cancel_says_how_to_retry() {
    let mut h = Harness::new().await;
    let id = redis_tab(&mut h).await;
    loading(&mut h, id);
    let st = h.app.state.clone();
    let shown = st.reconnecting_now("rd");
    let cancel = h.app.keys.label(Action::Cancel);
    let s = h.screen(160, 20);
    assert!(s.contains(&format!("Reconnecting [DEV] cache through SSH… 0.0 s · {cancel} cancel")), "{s}");
    h.app.run_action(Action::Cancel);
    assert!(h.app.waits.is_empty());
    assert_eq!(h.app.status, "Cancelled while reconnecting [DEV] cache · r retry");
    let s = h.screen(160, 20);
    assert!(s.contains("✗ Cancelled while reconnecting [DEV] cache through SSH."), "{s}");
    // A failed reconnect: the Message popup names the Redis tab
    let r = h.app.state.reconnecting().pop().unwrap();
    h.app.reconnect_failed(&r, &format!("{}: refused", r.failed), Some("Redis"));
    assert!(matches!(&h.app.popup, Some(crate::app::Popup::Message { text, .. }) if text.ends_with("r in the Redis tab retries.")));
    drop(shown);
}

/// What the tab sent (a Terminal line, an edit) isn't sent again by r: its failed reconnect says to run it again
#[tokio::test]
async fn a_sent_line_whose_reconnect_failed_says_to_run_it_again() {
    let mut h = Harness::new().await;
    redis_tab(&mut h).await;
    let st = h.app.state.clone();
    let shown = st.reconnecting_now("rd");
    let r = h.app.state.reconnecting().pop().unwrap();
    h.app.reconnect_failed_run(&r, &format!("{}: refused", r.failed));
    assert!(matches!(&h.app.popup, Some(crate::app::Popup::Message { text, .. }) if text.ends_with("Run it again: r in the Redis tab only loads its keys again.")), "{:?}", h.app.status);
    assert_eq!(h.app.status, "Couldn't reconnect [DEV] cache · run it again");
    drop(shown);
}
