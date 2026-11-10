//! A load waiting on a source that reconnects (#135): the status line says so with the time (past 10 s why), Cancel stops it, and a
//! failure opens the Message popup with the reason and the key that loads it again. As the GUI's status bar and toast
use crate::app::{Action, Popup, TabKind, WaitFor};
use crate::testkit::Harness;

/// An AWS SSM source "s" ([PROD] orders) whose port refuses at once, and a table tab of it loading
async fn source(h: &mut Harness) -> u64 {
    let cfg = serde_json::from_value(serde_json::json!({
        "id": "s", "name": "[PROD] orders", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d",
        "ssm": { "enabled": true, "target": "i-0123456789abcdef0" }
    }))
    .unwrap();
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let id = h.table_tab("payment", &["id"], &[], &["id"]);
    let t = h.app.tabs.iter_mut().find(|t| t.id == id).unwrap();
    t.conn = "s".into();
    t.target = "s".into();
    id
}

fn running(h: &Harness, id: u64) -> bool {
    matches!(&h.app.tabs.iter().find(|t| t.id == id).unwrap().kind, TabKind::Table(tt) if tt.running)
}

#[tokio::test]
async fn the_status_line_says_a_source_reconnects_and_cancel_stops_its_loads() {
    let mut h = Harness::new().await;
    let id = source(&mut h).await;
    if let Some(TabKind::Table(tt)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        (tt.grid, tt.running) = (None, true);
    }
    h.app.spawn_wait("s", WaitFor::Table(id), std::future::pending::<()>(), |_, _, _| {});
    // Waiting, nothing reconnects: the tab's own line
    assert!(h.screen(160, 20).contains("Loading public.payment… "));
    assert!(!h.screen(160, 20).contains("Reconnecting"));

    let st = h.app.state.clone();
    let shown = st.reconnecting_now("s");
    let cancel = h.app.keys.label(Action::Cancel);
    let s = h.screen(160, 20);
    assert!(s.contains(&format!("Reconnecting [PROD] orders through AWS SSM… 0.0 s · {cancel} cancel")), "{s}");
    assert!(!s.contains("Taking longer"));
    // Past 10 s: why it may take a while (after the Cancel key, which the cut never takes)
    *h.app.reconnect_since.get_mut("s").unwrap() -= std::time::Duration::from_secs(11);
    let s = h.screen(220, 20);
    assert!(s.contains(&format!("through AWS SSM… 11.0 s · {cancel} cancel · Taking longer than usual")), "{s}");

    h.app.run_action(Action::Cancel);
    assert!(h.app.waits.is_empty() && !running(&h, id));
    assert_eq!(h.app.status, "Cancelled while reconnecting [PROD] orders through AWS SSM. r retries.");
    assert!(!h.screen(160, 20).contains("Reconnecting"));
    drop(shown);
}

#[tokio::test]
async fn a_failed_load_while_reconnecting_says_why_and_how_to_retry() {
    let mut h = Harness::new().await;
    let id = source(&mut h).await;
    let st = h.app.state.clone();
    let shown = st.reconnecting_now("s");
    h.app.reload_table(id);
    for _ in 0..50 {
        h.settle().await;
        if h.app.waits.is_empty() {
            break;
        }
    }
    drop(shown);
    let Some(Popup::Message { title, text }) = &h.app.popup else { panic!("no popup: {}", h.app.status) };
    assert_eq!(title, "Couldn't reconnect");
    assert!(text.starts_with("Couldn't reconnect [PROD] orders through AWS SSM.\n\n") && text.ends_with("\n\nr in the table tab retries."), "{text}");
    assert!(h.app.status.starts_with("Couldn't reconnect [PROD] orders through AWS SSM: ") && h.app.status.ends_with(" · r retry"), "{}", h.app.status);
    assert!(!running(&h, id));
}

/// A load that ends while nothing reconnects leaves no wait behind and opens no popup
#[tokio::test]
async fn a_load_without_a_reconnect_leaves_nothing_behind() {
    let mut h = Harness::new().await;
    let id = source(&mut h).await;
    h.app.reload_table(id);
    for _ in 0..50 {
        h.settle().await;
        if h.app.waits.is_empty() {
            break;
        }
    }
    assert!(h.app.waits.is_empty() && h.app.popup.is_none() && !running(&h, id));
    assert!(!h.app.status.contains("reconnect"), "{}", h.app.status);
}
