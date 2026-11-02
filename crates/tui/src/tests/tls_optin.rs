//! The TLS opt-in after a connect error that needs TLS: dev sources get a confirm popup (yes turns on "Allow unencrypted" in the store, then
//! reconnects or loads the tree again), Production gets only a status line pointing to the data source form
use crate::app::{Focus, Popup};
use crate::testkit::Harness;
use qurry_core::db::QueryResult;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

/// A PostgreSQL without TLS that rejects every login: answers N to the TLS request (the core's NEEDS_TLS) and fails the plain-text
/// startup with an authentication error (so a connect after "Allow unencrypted" fails at once instead of waiting for a timeout)
async fn no_tls_server() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            tokio::spawn(async move {
                loop {
                    let Ok(len) = s.read_i32().await else { return };
                    let mut body = vec![0; (len - 4).max(0) as usize];
                    if s.read_exact(&mut body).await.is_err() {
                        return;
                    }
                    if body.starts_with(&80877103i32.to_be_bytes()) {
                        let _ = s.write_all(b"N").await;
                        continue;
                    }
                    let fields = b"SFATAL\0C28P01\0Mpassword authentication failed for user \"u\"\0\0";
                    let mut msg = vec![b'E'];
                    msg.extend((fields.len() as i32 + 4).to_be_bytes());
                    msg.extend(fields);
                    let _ = s.write_all(&msg).await;
                    return;
                }
            });
        }
    });
    port
}

/// Data source "test" (the harness console's), remote by name: 127.1 isn't a loopback literal. Its server is no_tls_server, or for Production
/// nothing on port 1 (so the console's completion metadata, which fails slowly there, doesn't take the status line)
async fn lan(production: bool) -> Harness {
    let port = if production { 1 } else { no_tls_server().await };
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "LAN dev", "kind": "postgres", "host": "127.1", "port": port, "user": "u", "database": "app", "options": { "confirmChanges": production } })).unwrap();
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
    // Only a reconnect (the server turns the login down): the console says how it went, the statement isn't run again
    assert!(h.app.status.starts_with("Connection failed:") && h.app.status.contains("password authentication failed"), "{}", h.app.status);
    let out = h.console_output();
    assert!(out.last().is_some_and(|l| l.starts_with("Connection failed:")), "{out:?}");
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
    assert_eq!(h.app.status, "Production: trusted network only — explorer: e › \"Allow unencrypted\"");
    assert!(!saved(&h).await.options.allow_plaintext);
}

/// Through the key path: → on the data source loads it, the server has no TLS → popup; n changes nothing, y loads the node again in plain text
#[tokio::test]
async fn tree_expand_asks_no_keeps_it_yes_loads_again() {
    let mut h = lan(false).await;
    h.app.focus = Focus::Tree;
    h.key("Right");
    h.settle().await;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.contains("sent unencrypted to 127.1"), "{text}");
    h.key("n");
    assert!(h.app.popup.is_none() && !saved(&h).await.options.allow_plaintext);
    h.app.offer_tls_opt_in("test", "connection refused", |a| a.reexpand("test"));
    assert!(h.app.popup.is_none(), "only TLS errors ask");
    h.app.reexpand("test");
    h.settle().await;
    assert!(matches!(h.app.popup, Some(Popup::Confirm { .. })), "expanding again asks again: {}", h.app.status);
    h.key("y");
    h.settle().await;
    assert!(saved(&h).await.options.allow_plaintext);
    // Loaded again, now in plain text: the server's own answer, no second ask
    let kids: Vec<String> = h.app.tree.children.get("test").map(|n| n.iter().map(|n| n.label.clone()).collect()).unwrap_or_default();
    assert!(kids.iter().any(|l| l.contains("password authentication failed")), "{kids:?}");
    assert!(h.app.popup.is_none());
}

/// The error comes back from a background load: a popup that is already open (another confirm, a form being typed in) stays
#[tokio::test]
async fn an_open_popup_is_never_replaced() {
    let mut h = lan(false).await;
    h.app.popup = Some(Popup::Confirm { text: "Production: this changes data. Run it?".into(), on_yes: Box::new(|a| a.status = "ran".into()) });
    h.app.focus = Focus::Tree;
    h.app.reexpand("test");
    h.settle().await;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("popup gone") };
    assert_eq!(text, "Production: this changes data. Run it?");
    assert_eq!(h.app.status, "\"LAN dev\": the server has no TLS — expand or run again to allow unencrypted");
    h.key("y");
    assert_eq!(h.app.status, "ran");
    assert!(!saved(&h).await.options.allow_plaintext);
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
