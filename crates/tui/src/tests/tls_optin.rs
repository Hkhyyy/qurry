//! The TLS opt-in after a connect error that needs TLS: dev sources get a confirm popup (yes turns on "Allow unencrypted" in the store, then
//! reconnects or loads the tree again), Production gets only a status line pointing to the data source form
use crate::app::{Focus, Popup};
use crate::testkit::Harness;
use qurry_core::db::QueryResult;
use std::sync::atomic::{AtomicUsize, Ordering};

static RUNS: AtomicUsize = AtomicUsize::new(0);
/// no_tls, counted (to see that a run isn't repeated)
fn counted(q: &str) -> Result<QueryResult, String> {
    RUNS.fetch_add(1, Ordering::SeqCst);
    no_tls(q)
}

/// Every statement fails the way a remote PostgreSQL without TLS does
fn no_tls(_: &str) -> Result<QueryResult, String> {
    Err(format!("{}\n(error communicating with database: TLS required)", qurry_shared::errors::needs_tls_pg()))
}

/// Data source "test" (the harness console's), remote by name: 127.1 isn't a loopback literal, and nothing listens on port 1
async fn lan(production: bool) -> Harness {
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "LAN dev", "kind": "postgres", "host": "127.1", "port": 1, "user": "u", "database": "app", "options": { "confirmChanges": production } })).unwrap();
    h.app.state.save_connection(cfg, Some("pw".into()), None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.fake_db = Some(no_tls);
    h
}

async fn saved(h: &Harness) -> qurry_core::store::ConnectionConfig {
    h.app.state.list_connections().await.unwrap().into_iter().find(|c| c.config.id == "test").unwrap().config
}

#[tokio::test]
async fn run_error_asks_then_allows_and_only_reconnects() {
    let mut h = lan(false).await;
    h.app.fake_db = Some(counted);
    h.console_with("delete from t where id = 1", 0, 0);
    let runs = RUNS.load(Ordering::SeqCst);
    h.key("F5");
    h.settle().await;
    assert_eq!(RUNS.load(Ordering::SeqCst), runs + 1);
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.starts_with("Allow unencrypted for this data source?"), "{text}");
    assert!(text.contains("The password will be sent unencrypted to 127.1. Use only on a network you trust."), "{text}");
    assert!(text.contains("To undo, turn off \"Allow unencrypted\" in the data source form"), "{text}");
    let s = h.screen(110, 24);
    assert!(s.contains("Allow unencrypted for this data source?"), "{s}");
    h.key("y");
    h.settle().await;
    let c = saved(&h).await;
    assert!(c.options.allow_plaintext && c.password_saved, "{c:?}");
    assert!(h.app.conn("test").unwrap().config.options.allow_plaintext, "the form's copy is updated too");
    // Only a reconnect (to nothing here, which takes a while to give up): the statement isn't run again
    assert_eq!(h.app.status, "Connecting to \"LAN dev\"…");
    assert_eq!(RUNS.load(Ordering::SeqCst), runs + 1);
    assert!(h.app.popup.is_none());
}

#[tokio::test]
async fn production_gets_only_a_status_line() {
    let mut h = lan(true).await;
    h.console_with("select 1", 0, 0);
    h.key("F5");
    h.settle().await;
    assert!(h.app.popup.is_none());
    assert_eq!(h.app.status, "Production: turn on \"Allow unencrypted\" in its form (e) yourself");
    assert!(!saved(&h).await.options.allow_plaintext);
}

/// Tree: n changes nothing, other errors don't ask, yes loads the node again
#[tokio::test]
async fn tree_error_yes_loads_again_no_keeps_it() {
    let mut h = lan(false).await;
    let tls = no_tls("");
    let err = tls.unwrap_err();
    h.app.tree.expanded.insert("test".into());
    h.app.offer_tls_opt_in("test", &err, |a| a.reexpand("test"));
    h.key("n");
    assert!(h.app.popup.is_none() && !saved(&h).await.options.allow_plaintext);
    h.app.offer_tls_opt_in("test", "connection refused", |a| a.reexpand("test"));
    assert!(h.app.popup.is_none(), "only TLS errors ask");
    h.app.offer_tls_opt_in("test", &err, |a| a.reexpand("test"));
    h.key("y");
    h.settle().await;
    assert!(saved(&h).await.options.allow_plaintext);
    assert!(h.app.tree.expanded.contains("test") && h.app.tree.loading.contains("test"), "loading again");
    assert!(h.app.popup.is_none());
}

/// The shared PostgreSQL has no TLS; at "127.1" Qurry treats it as remote. Expand → popup → y → the databases load with the saved password
/// QURRY_TEST_PG=host:port:user:db
#[tokio::test]
#[ignore]
async fn pg_tree_end_to_end() {
    let e = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG");
    let p: Vec<&str> = e.split(':').collect();
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "pg", "name": "LAN pg", "kind": "postgres", "host": "127.1", "port": p[1].parse::<u16>().unwrap(), "user": p[2], "database": p[3] })).unwrap();
    h.app.state.save_connection(cfg, Some("pw".into()), None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.focus = Focus::Tree;
    h.key("Right");
    h.settle().await;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.contains("sent unencrypted to 127.1"), "{text}");
    h.key("y");
    h.settle().await;
    let dbs: Vec<String> = h.app.tree.children.get("pg").map(|n| n.iter().map(|n| n.label.clone()).collect()).unwrap_or_default();
    assert!(dbs.iter().any(|d| d == p[3]), "{dbs:?} {}", h.app.status);
    assert!(h.app.status.contains("Not encrypted"), "{}", h.app.status);
    let _ = h.app.state.disconnect("pg".into()).await;
}
