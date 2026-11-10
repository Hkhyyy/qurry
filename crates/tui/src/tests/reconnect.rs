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
    // The short form, never cut, so the key that loads it again shows (120 columns, the PROD badge in front, the grid's items)
    assert_eq!(h.app.status, "Cancelled while reconnecting [PROD] orders · r retry");
    h.app.focus = crate::app::Focus::Grid;
    let s = h.screen(120, 20);
    assert!(s.contains("Cancelled while reconnecting [PROD] orders · r retry"), "{s}");
    assert!(!s.contains("Reconnecting"));
    drop(shown);
}

/// A load waiting on the reconnect: the core says it couldn't reconnect (the tunnel didn't open again)
fn failed(h: &Harness, reason: &str) -> (qurry_core::app::Reconnecting, String) {
    let r = h.app.state.reconnecting().pop().expect("shown");
    let e = format!("{}: {reason}", r.failed);
    (r, e)
}

#[tokio::test]
async fn a_failed_reconnect_says_why_and_how_to_retry_once_per_source() {
    let mut h = Harness::new().await;
    let id = source(&mut h).await;
    let st = h.app.state.clone();
    let shown = st.reconnecting_now("s");
    let (r, e) = failed(&h, "TargetNotConnected");
    // A table tab and an explorer node waited on it: one popup
    h.app.reconnect_failed(&r, &e, true);
    h.app.reconnect_failed(&r, &e, false);
    let Some(Popup::Message { title, text }) = &h.app.popup else { panic!("no popup: {}", h.app.status) };
    assert_eq!(title, "Couldn't reconnect");
    assert_eq!(text, "Couldn't reconnect [PROD] orders through AWS SSM.\n\nTargetNotConnected\n\nr in the table tab retries.");
    assert_eq!(h.app.status, "Couldn't reconnect [PROD] orders · r retry");
    h.app.focus = crate::app::Focus::Grid;
    let s = h.screen(120, 20);
    assert!(s.contains("Couldn't reconnect [PROD] orders · r retry"), "{s}");
    // Another source's failure still opens its own
    let mut other = r.clone();
    other.failed = "Couldn't reconnect stage through SSH".into();
    h.app.reconnect_failed(&other, &format!("{}: refused", other.failed), true);
    assert!(matches!(&h.app.popup, Some(Popup::Note { text, back, .. }) if text.starts_with("Couldn't reconnect [PROD]") && matches!(&**back, Popup::Message { text, .. } if text.starts_with("Couldn't reconnect stage"))));
    drop(shown);
    let _ = id;
}

/// A load that fails for its own reason while (or after) its source reconnected says that reason: no 'Couldn't reconnect' popup
#[tokio::test]
async fn a_loads_own_error_after_a_reconnect_is_not_a_failed_reconnect() {
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
    assert!(h.app.popup.is_none(), "{:?}", h.app.status);
    assert!(!h.app.status.contains("reconnect"), "{}", h.app.status);
    assert!(!running(&h, id));
    let r = h.app.state.reconnecting();
    assert!(r.is_empty());
}

/// The Cancel key stops the active tab's own run first (as the GUI's ⌘F2), not a reconnect another source's load waits on; the
/// status line keeps its running line. With nothing of its own to stop, or when it is the one waiting, Cancel stops the reconnect
#[tokio::test]
async fn cancel_stops_the_active_tabs_own_run_before_a_reconnect_elsewhere() {
    let mut h = Harness::new().await;
    let table = source(&mut h).await;
    let console = h.console_tab();
    if let Some(TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == console).map(|t| &mut t.kind) {
        c.running = true;
    }
    h.app.active = h.app.tabs.iter().position(|t| t.id == console).unwrap();
    let st = h.app.state.clone();
    let shown = st.reconnecting_now("s");
    h.app.tree.loading.insert("s|d|public".into());
    h.app.spawn_wait("s", WaitFor::Tree("s|d|public".into()), std::future::pending::<()>(), |_, _, _| {});
    let s = h.screen(160, 20);
    assert!(s.contains("Executing… "), "{s}");
    assert!(!s.contains("Reconnecting"), "{s}");
    h.app.run_action(Action::Cancel);
    assert_eq!(h.app.waits.len(), 1, "the tree's load waits on");

    // The table tab waiting on the source: its Cancel stops the reconnect
    if let Some(TabKind::Table(tt)) = h.app.tabs.iter_mut().find(|t| t.id == table).map(|t| &mut t.kind) {
        (tt.grid, tt.running) = (None, true);
    }
    h.app.spawn_wait("s", WaitFor::Table(table), std::future::pending::<()>(), |_, _, _| {});
    h.app.active = h.app.tabs.iter().position(|t| t.id == table).unwrap();
    assert!(h.screen(160, 20).contains("Reconnecting [PROD] orders"));
    h.app.run_action(Action::Cancel);
    assert!(h.app.waits.is_empty() && !running(&h, table));
    drop(shown);
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
