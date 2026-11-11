//! Started with a URL (qurry-tui postgres://…): a temporary data source at the top of the explorer, selected and connected, never
//! saved; --read-only refuses a write the way a Read-only data source does; incident mode opens Database Admin › Sessions on it
use crate::app::TabKind;
use crate::cli::{Start, temporary};
use crate::testkit::Harness;

/// The harness with a temporary source for `url` (on tls_optin's fake PostgreSQL, which answers every statement with one row)
async fn started(url: &str, read_only: bool, incident: bool) -> (Harness, String) {
    let mut h = Harness::new().await;
    let t = temporary(&Start { url: Some(url.into()), read_only, incident, ssh: None }).unwrap();
    let id = h.app.state.open_temporary(t).id;
    h.app.start_with(id.clone(), incident);
    h.settle().await;
    (h, id)
}

#[tokio::test]
async fn url_opens_a_connected_temporary_source_and_read_only_blocks_a_write() {
    let port = super::tls_optin::no_tls_server().await;
    let (mut h, id) = started(&format!("postgres://app:hunter2@127.0.0.1:{port}/shop"), true, false).await;
    let root = h.app.tree.roots.first().expect("a root");
    assert_eq!(root.conn, id);
    assert_eq!(root.label, format!("postgres://app@127.0.0.1:{port}/shop"));
    assert_eq!(h.app.tree.selected().map(|n| n.conn), Some(id.clone()));
    assert!(h.app.conn(&id).is_some_and(|c| c.connected), "connected: {}", h.app.status);
    let screen = h.screen(160, 40);
    assert!(screen.contains("postgres://app@") && !screen.contains("hunter2"), "{screen}");
    // The mark is before the name, so the default explorer width shows it whatever the URL's length
    let row = screen.lines().find(|l| l.contains("postgres://app@")).unwrap();
    assert!(row.contains("◉  TEMP · READ-ONLY  postgres://app@"), "{row}");

    // A write is refused before it reaches the server; a read runs
    h.app.open_console(&id, None);
    h.type_str("update orders set paid = true where id = 1");
    h.key("F5");
    h.settle().await;
    let out = h.console_output().join("\n");
    assert!(out.contains("read-only"), "{out} / {}", h.app.status);
    let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind else { panic!("console") };
    c.editor = tui_textarea::TextArea::new(vec!["select 1".into()]);
    h.key("F5");
    h.settle().await;
    let TabKind::Console(c) = &h.app.tabs[h.app.active].kind else { panic!("console") };
    assert!(c.grid().is_some(), "{:?}", h.console_output());

    // Workspace actions don't take it: every workspace shows it and none lists it (the list is shared with the GUI)
    h.app.ws_add_conn_pick();
    assert!(h.app.popup.is_none() && h.app.status.starts_with("Every data source is already in this workspace"), "{}", h.app.status);
    h.app.workspaces.push(crate::app::Workspace { id: "w2".into(), name: "Ops".into(), conns: vec![], saved: Some(Default::default()) });
    for act in [crate::app::App::ws_move_conn_pick as fn(&mut crate::app::App), crate::app::App::ws_remove_conn] {
        h.app.status.clear();
        act(&mut h.app);
        assert!(h.app.popup.is_none() && h.app.status.contains("temporary data source"), "{}", h.app.status);
    }

    // Nothing of it is saved: not in connections.json, not in a workspace list (shared with the GUI), not its console text, not
    // the history (its statements ran above)
    h.app.ws_push();
    h.app.save_workspace();
    let dir = h.app.state.store.dir();
    for f in ["connections.json", "workspaces.json", "workspace-tui.json", "secrets.json", "history.json", "history.jsonl"] {
        let text = std::fs::read_to_string(dir.join(f)).unwrap_or_default();
        assert!(!text.contains("127.0.0.1") && !text.contains("hunter2") && !text.contains("orders") && !text.contains(&id), "{f}: {text}");
    }
    assert!(h.app.state.history.list(None, "", 100).is_empty());
    let ws = serde_json::to_string(&h.app.state.store.workspace("tui")).unwrap();
    assert!(!ws.contains("orders") && !ws.contains("shop"), "{ws}");
    assert!(h.app.workspaces.iter().all(|w| !w.conns.contains(&id)));
    // Its form can't save it
    let e = h.app.state.save_connection(h.app.conn(&id).unwrap().config.clone(), None, None).await.unwrap_err().to_string();
    assert!(e.contains("temporary"), "{e}");
}

#[tokio::test]
async fn incident_mode_opens_sessions() {
    let port = super::tls_optin::no_tls_server().await;
    let (h, id) = started(&format!("postgres://app@127.0.0.1:{port}/shop"), false, true).await;
    let t = &h.app.tabs[h.app.active];
    assert_eq!(t.conn, id);
    let TabKind::Dba(d) = &t.kind else { panic!("not a Database Admin tab: {}", t.title) };
    assert_eq!(d.report, "sessions");
}
