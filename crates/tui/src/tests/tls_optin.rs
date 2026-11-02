//! The TLS opt-in after a connect error that needs TLS: dev sources get a confirm popup (y turns on "Allow unencrypted" in the store, then
//! reconnects or loads the tree again), Production gets only a status line pointing to the data source form. The popup opens on its own, so
//! only a y pressed once it has been on screen for App::TLS_ASK_GRACE counts (never Enter)
use crate::app::{App, Focus, Popup, TabKind};
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

/// One backend message: tag, length, body
fn pg_msg(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut m = vec![tag];
    m.extend((body.len() as i32 + 4).to_be_bytes());
    m.extend(body);
    m
}

/// The one-column, one-row answer to `sql`: (RowDescription, DataRow + CommandComplete). A session's pg_backend_pid() gets an int4 (binary,
/// as sqlx asks for it), everything else the text "PostgreSQL 16 (no TLS)"
fn pg_answer(sql: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let pid = sql.windows(14).any(|w| w == b"pg_backend_pid");
    let (oid, len, v): (i32, i16, Vec<u8>) = if pid { (23, 4, 7i32.to_be_bytes().to_vec()) } else { (25, -1, b"PostgreSQL 16 (no TLS)".to_vec()) };
    let cols = [&1i16.to_be_bytes()[..], b"v\0", &0i32.to_be_bytes(), &0i16.to_be_bytes(), &oid.to_be_bytes(), &len.to_be_bytes(), &(-1i32).to_be_bytes(), &0i16.to_be_bytes()].concat();
    let row = [pg_msg(b'D', &[&1i16.to_be_bytes()[..], &(v.len() as i32).to_be_bytes(), &v].concat()), pg_msg(b'C', b"SELECT 1\0")].concat();
    (pg_msg(b'T', &cols), row)
}

/// A PostgreSQL without TLS: answers N to the TLS request (the core's NEEDS_TLS), then lets the plain-text login in and answers every
/// statement with one row (pg_answer: enough for a connect after "Allow unencrypted" to succeed and a table tab to show rows)
async fn no_tls_server() -> u16 {
    let ready = pg_msg(b'Z', b"I");
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            let ready = ready.clone();
            tokio::spawn(async move {
                // Startup: the TLS request (N), then the plain-text startup message (no password asked)
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
                    let ok = [pg_msg(b'R', &0i32.to_be_bytes()), pg_msg(b'K', &[1i32.to_be_bytes(), 2i32.to_be_bytes()].concat()), ready.clone()].concat();
                    if s.write_all(&ok).await.is_err() {
                        return;
                    }
                    break;
                }
                // Simple and extended query protocol: prepared statements by name (sqlx reuses them), Bind picks the one to answer for
                let mut prepared = std::collections::HashMap::<Vec<u8>, (Vec<u8>, Vec<u8>)>::new();
                let mut answer = pg_answer(b"");
                let name = |b: &[u8], skip: usize| b.split(|c| *c == 0).nth(skip).unwrap_or_default().to_vec();
                loop {
                    let Ok(tag) = s.read_u8().await else { return };
                    let Ok(len) = s.read_i32().await else { return };
                    let mut body = vec![0; (len - 4).max(0) as usize];
                    if s.read_exact(&mut body).await.is_err() {
                        return;
                    }
                    match tag {
                        b'Q' => answer = pg_answer(&body),
                        b'P' => {
                            answer = pg_answer(&body);
                            prepared.insert(name(&body, 0), answer.clone());
                        }
                        b'B' => answer = prepared.get(&name(&body, 1)).cloned().unwrap_or_else(|| answer.clone()),
                        // Describe of a statement by name
                        b'D' if body.first() == Some(&b'S') => answer = prepared.get(&name(&body[1..], 0)).cloned().unwrap_or_else(|| answer.clone()),
                        _ => {}
                    }
                    let (cols, row) = &answer;
                    let out = match tag {
                        b'Q' => [cols.clone(), row.clone(), ready.clone()].concat(),
                        b'P' => pg_msg(b'1', &[]),
                        b'B' => pg_msg(b'2', &[]),
                        b'C' => pg_msg(b'3', &[]),
                        b'D' if body.first() == Some(&b'S') => [pg_msg(b't', &0i16.to_be_bytes()), cols.clone()].concat(),
                        b'D' => cols.clone(),
                        b'E' => row.clone(),
                        b'S' => ready.clone(),
                        b'X' => return,
                        _ => vec![],
                    };
                    if s.write_all(&out).await.is_err() {
                        return;
                    }
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

/// The user has had time to read the popup (as if TLS_ASK_GRACE had passed)
fn read_it(h: &mut Harness) {
    let Some(Popup::AllowPlaintext { opened, .. }) = &mut h.app.popup else { panic!("no Allow unencrypted popup: {}", h.app.status) };
    *opened = std::time::Instant::now() - App::TLS_ASK_GRACE;
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
    let Some(Popup::AllowPlaintext { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    // It may open over another tab: it names the source y changes
    assert!(text.starts_with("Allow unencrypted for \"LAN dev\"?"), "{text}");
    assert!(text.contains("The password will be sent unencrypted to 127.1. Use only on a network you trust."), "{text}");
    assert!(text.contains("To undo, turn off \"Allow unencrypted\" in the data source form"), "{text}");
    let s = h.screen(110, 24);
    assert!(s.contains("Allow unencrypted for \"LAN dev\"?") && s.contains("y allow") && !s.contains("Enter"), "{s}");
    read_it(&mut h);
    h.key("y");
    h.settle().await;
    let c = saved(&h).await;
    assert!(c.options.allow_plaintext && c.password_saved, "{c:?}");
    assert!(h.app.conn("test").unwrap().config.options.allow_plaintext, "the form's copy is updated too");
    // Only a reconnect (now in plain text, and it works): the console says what to do next, the statement isn't run again
    assert_eq!(h.console_output().last().map(String::as_str), Some("Connected. Run again."));
    assert_eq!(RUNS.load(Ordering::SeqCst), runs + 1, "the statement isn't run again");
    assert!(h.app.popup.is_none());
}

/// A table tab that failed on the TLS error: y reconnects and the tab loads its rows again (reload_conns → retry_failed_tables)
#[tokio::test]
async fn table_tab_reloads_after_allow() {
    let mut h = lan(false).await;
    h.app.open_table("test", String::new(), "public".into(), "users".into(), "table".into());
    h.settle().await;
    let table = |h: &Harness| match h.app.tabs.get(h.app.active).map(|t| &t.kind) {
        Some(TabKind::Table(t)) => (t.error.clone(), t.grid.as_ref().map(|g| g.result.rows.len()).unwrap_or(0)),
        _ => panic!("no table tab"),
    };
    assert!(table(&h).0.is_some_and(|e| qurry_shared::errors::tls_opt_in(&e)), "{:?}", table(&h));
    read_it(&mut h);
    h.key("y");
    h.settle().await;
    let (error, rows) = table(&h);
    assert!(error.is_none() && rows == 1, "{error:?} {rows} {}", h.app.status);
}

/// The data source form's ⌃t Test hits the TLS error: like the GUI dialog's Allow button, it points at the setting (selects the field,
/// changes nothing); Space turns it on in the form only and ⌃t tests again
#[tokio::test]
async fn form_test_selects_allow_unencrypted() {
    let mut h = lan(false).await;
    let cfg = h.app.conn("test").unwrap().config.clone();
    h.app.conn_form(Some(cfg));
    h.key("Ctrl+t");
    h.settle().await;
    let field = |h: &Harness| match &h.app.popup {
        Some(Popup::ConnForm(f)) => (f.fields[f.sel].label, f.fields[f.sel].value.clone(), f.message.clone().unwrap_or_default().1),
        _ => panic!("no form"),
    };
    let (label, value, msg) = field(&h);
    assert_eq!((label, value.as_str()), ("Allow unencrypted", "off"));
    assert!(msg.ends_with("\nOn a trusted network, Space turns on \"Allow unencrypted\" (selected), then ⌃t tests again."), "{msg}");
    // The form grows to show the whole result and keeps its key hints
    let s = h.screen(120, 50);
    assert!(s.contains("then ⌃t tests again.") && s.contains("Esc cancel") && s.contains("(error occurred while"), "{s}");
    h.key("Space");
    h.key("Ctrl+t");
    h.settle().await;
    let (_, value, msg) = field(&h);
    assert!(value == "on" && msg.starts_with("✓ PostgreSQL 16 (no TLS)"), "{value} {msg}");
    assert!(!saved(&h).await.options.allow_plaintext, "nothing is saved before Enter");
}

#[tokio::test]
async fn production_gets_only_a_status_line() {
    let mut h = lan(true).await;
    h.console_with("select 1", 0, 0);
    h.key("F5");
    h.settle().await;
    assert!(h.app.popup.is_none());
    assert_eq!(h.app.status, "\"LAN dev\" is Production: turn on \"Allow unencrypted\" yourself in its form (e in the explorer)");
    assert!(!saved(&h).await.options.allow_plaintext);
}

/// Through the key path: → on the data source loads it, the server has no TLS → popup; n changes nothing, y loads the node again in plain text
#[tokio::test]
async fn tree_expand_asks_no_keeps_it_yes_loads_again() {
    let mut h = lan(false).await;
    h.app.focus = Focus::Tree;
    h.key("Right");
    h.settle().await;
    let Some(Popup::AllowPlaintext { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.contains("sent unencrypted to 127.1"), "{text}");
    read_it(&mut h);
    h.key("n");
    assert!(h.app.popup.is_none() && !saved(&h).await.options.allow_plaintext);
    h.app.offer_tls_opt_in("test", "connection refused", |a| a.reexpand("test"));
    assert!(h.app.popup.is_none(), "only TLS errors ask");
    // The error isn't cached: ← → (collapse, expand again) loads again and asks again
    h.key("Left");
    h.key("Right");
    h.settle().await;
    assert!(matches!(h.app.popup, Some(Popup::AllowPlaintext { .. })), "expanding again asks again: {}", h.app.status);
    read_it(&mut h);
    h.key("y");
    h.settle().await;
    assert!(saved(&h).await.options.allow_plaintext);
    // Loaded again, now in plain text: no error, no second ask
    assert!(!h.app.tree.failed("test"), "{:?}", h.app.tree.children.get("test").map(|n| n.iter().map(|n| n.label.clone()).collect::<Vec<_>>()));
    assert!(h.app.popup.is_none());
}

/// The popup opens while the user may still be typing (F5, then the next line): a y or Enter that arrives with it, or an Enter later, never
/// allows plaintext; a y pressed once it has been on screen does
#[tokio::test]
async fn only_a_y_after_the_popup_was_read_allows() {
    let mut h = lan(false).await;
    h.console_with("select 1", 0, 0);
    h.key("F5");
    h.settle().await;
    h.key("y");
    h.key("Enter");
    assert!(matches!(h.app.popup, Some(Popup::AllowPlaintext { .. })), "{}", h.app.status);
    read_it(&mut h);
    h.key("Enter");
    h.settle().await;
    assert!(matches!(h.app.popup, Some(Popup::AllowPlaintext { .. })), "Enter doesn't answer it");
    assert!(!saved(&h).await.options.allow_plaintext);
    h.key("y");
    h.settle().await;
    assert!(saved(&h).await.options.allow_plaintext);
    assert_eq!(h.console_text(), "select 1", "no key went to the editor");
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
    assert_eq!(h.app.status, "\"LAN dev\": the server has no TLS — expand it again (← →) or run again to allow unencrypted");
    h.key("y");
    assert_eq!(h.app.status, "ran");
    assert!(!saved(&h).await.options.allow_plaintext);
    // What the status line says works: ← → loads again, and now that no popup is open it asks
    h.key("Left");
    h.key("Right");
    h.settle().await;
    assert!(matches!(h.app.popup, Some(Popup::AllowPlaintext { .. })), "{}", h.app.status);
}

/// The shared PostgreSQL has no TLS; at "127.1" Qurry treats it as remote. Expand → popup → y → the databases load with the saved password
/// QURRY_TEST_PG=host:port:user:db
#[tokio::test]
#[ignore]
async fn pg_tree_end_to_end() {
    let db = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG").split(':').nth(3).unwrap().to_string();
    let mut h = shared_pg().await;
    h.app.focus = Focus::Tree;
    h.key("Right");
    h.settle().await;
    let Some(Popup::AllowPlaintext { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.contains("sent unencrypted to 127.1"), "{text}");
    read_it(&mut h);
    h.key("y");
    h.settle().await;
    let dbs: Vec<String> = h.app.tree.children.get("test").map(|n| n.iter().map(|n| n.label.clone()).collect()).unwrap_or_default();
    assert!(dbs.contains(&db), "{dbs:?} {}", h.app.status);
    assert!(h.app.status.contains("Not encrypted"), "{}", h.app.status);
    let _ = h.app.state.disconnect("test".into()).await;
}

/// The shared PostgreSQL at "127.1" (QURRY_TEST_PG): the tree is left on the TLS error (n), then a console run asks again; y only reconnects
/// ("Connected. Run again.", the statement isn't run again) and the tree under the now connected source loads again
#[tokio::test]
#[ignore]
async fn pg_console_reconnects_and_reloads_the_tree() {
    let mut h = shared_pg().await;
    h.app.focus = Focus::Tree;
    h.key("Right");
    h.settle().await;
    read_it(&mut h);
    h.key("n");
    let kids = |h: &Harness| -> Vec<String> { h.app.tree.children.get("test").map(|n| n.iter().map(|n| n.label.clone()).collect()).unwrap_or_default() };
    assert!(kids(&h).len() == 1 && kids(&h)[0].starts_with("⚠"), "{:?}", kids(&h));
    // The run fails like the server does without TLS (the console's run goes through fake_db; the reconnect is real)
    h.app.fake_db = Some(counted);
    h.console_with("delete from t where id = 1", 0, 0);
    let runs = RUNS.load(Ordering::SeqCst);
    h.key("F5");
    h.settle().await;
    read_it(&mut h);
    h.key("y");
    h.settle().await;
    // The status line ends on the plain-text warning (the tree loads again in plain text); the console keeps what to do next
    assert!(h.app.status.contains("Not encrypted"), "{}", h.app.status);
    assert_eq!(h.console_output().last().map(String::as_str), Some("Connected. Run again."));
    assert_eq!(RUNS.load(Ordering::SeqCst), runs + 1, "the statement isn't run again");
    let db = std::env::var("QURRY_TEST_PG").unwrap().split(':').nth(3).unwrap().to_string();
    assert!(kids(&h).contains(&db), "{:?}", kids(&h));
    let _ = h.app.state.disconnect("test".into()).await;
}

/// A table tab on the shared PostgreSQL at "127.1": y reconnects and the tab loads its rows again
#[tokio::test]
#[ignore]
async fn pg_table_tab_reloads() {
    let mut h = shared_pg().await;
    h.app.open_table("test", String::new(), "pg_catalog".into(), "pg_am".into(), "table".into());
    h.settle().await;
    read_it(&mut h);
    h.key("y");
    h.settle().await;
    let Some(TabKind::Table(t)) = h.app.tabs.get(h.app.active).map(|t| &t.kind) else { panic!("no table tab") };
    assert!(t.error.is_none() && t.grid.as_ref().is_some_and(|g| !g.result.rows.is_empty()), "{:?} {}", t.error, h.app.status);
    let _ = h.app.state.disconnect("test".into()).await;
}

/// Data source "test" on the shared PostgreSQL (QURRY_TEST_PG=host:port:user:db), at "127.1" so Qurry treats it as remote
async fn shared_pg() -> Harness {
    let e = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG");
    let p: Vec<&str> = e.split(':').collect();
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "LAN pg", "kind": "postgres", "host": "127.1", "port": p[1].parse::<u16>().unwrap(), "user": p[2], "database": p[3] })).unwrap();
    h.app.state.save_connection(cfg, Some("pw".into()), None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    h
}
