use crate::app::{Focus, TabKind};
use crate::testkit::Harness;
use crate::tree::{Node, NodeKind};

/// Data Source "pg" (nothing listens on port 1) with db → schema → tables already loaded in the explorer
async fn explorer() -> Harness {
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "pg", "name": "local pg", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "shop" })).unwrap();
    h.app.state.save_connection(cfg, None, None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    let node = |key: &str, label: &str, kind: NodeKind| Node {
        key: key.into(),
        label: label.into(),
        hint: String::new(),
        expandable: !matches!(kind, NodeKind::Object(_)),
        kind,
        conn: "pg".into(),
        db: Some("shop".into()),
        schema: Some("public".into()),
        name: Some(label.into()),
    };
    let tree = &mut h.app.tree;
    tree.children.insert("pg".into(), vec![node("pg|shop", "shop", NodeKind::Db)]);
    tree.children.insert("pg|shop".into(), vec![node("pg|shop|public", "public", NodeKind::Schema)]);
    tree.children.insert("pg|shop|public".into(), vec![node("pg|shop|public|g:tables", "tables", NodeKind::Group)]);
    tree.children.insert("pg|shop|public|g:tables".into(), ["orders", "users"].iter().map(|t| node(&format!("pg|shop|public|g:tables|o:{t}"), t, NodeKind::Object("table".into()))).collect());
    h.app.focus = Focus::Tree;
    h
}

fn rows(h: &Harness) -> Vec<String> {
    h.app.tree.visible().iter().map(|(depth, n)| format!("{}{}", "  ".repeat(*depth), n.label)).collect()
}

/// → expands (and steps into an expanded node), ← collapses (and steps out to the parent of a collapsed one)
#[tokio::test]
async fn tree_expands_and_collapses() {
    let mut h = explorer().await;
    assert_eq!(rows(&h), ["local pg"]);
    h.key("Right");
    h.settle().await;
    assert_eq!(rows(&h), ["local pg", "  shop"]);
    // Already expanded: → moves to the first child
    h.key("Right");
    assert_eq!(h.app.tree.selected().unwrap().label, "shop");
    for _ in 0..3 {
        h.key("Right");
        h.key("Right");
    }
    assert_eq!(rows(&h), ["local pg", "  shop", "    public", "      tables", "        orders", "        users"]);
    assert_eq!(h.app.tree.selected().unwrap().label, "orders");
    let s = h.screen(100, 12);
    assert!(s.contains("orders") && s.contains("users"), "{s}");
    // Leaf: ← goes to the parent, ← again collapses it
    h.key("Left");
    assert_eq!(h.app.tree.selected().unwrap().label, "tables");
    h.key("Left");
    assert_eq!(rows(&h), ["local pg", "  shop", "    public", "      tables"]);
    // Collapsing a parent keeps what's expanded below it (shown again on expand)
    h.key("Home");
    h.key("Left");
    assert_eq!(rows(&h), ["local pg"]);
    h.key("Right");
    assert_eq!(rows(&h), ["local pg", "  shop", "    public", "      tables"]);
    // Enter on a folder toggles it too
    h.key("End");
    h.key("Enter");
    assert_eq!(rows(&h).len(), 6);
}

/// Enter on a table opens its data tab; F4 / ⌃l on the Data Source opens a query console for it
#[tokio::test]
async fn tree_opens_table_and_console() {
    let mut h = explorer().await;
    for k in ["pg", "pg|shop", "pg|shop|public", "pg|shop|public|g:tables"] {
        h.app.tree.expanded.insert(k.into());
    }
    assert!(h.app.tree.select_key("pg|shop|public|g:tables|o:users"));
    h.key("Enter");
    let t = &h.app.tabs[h.app.active];
    assert_eq!((t.title.as_str(), t.conn.as_str(), t.db.as_str()), ("users", "pg", "shop"));
    assert!(matches!(&t.kind, TabKind::Table(tt) if tt.schema == "public" && tt.table == "users"));
    assert_eq!(h.app.focus, Focus::Grid);
    // Opening it again goes to the same tab
    h.app.focus = Focus::Tree;
    h.key("Enter");
    assert_eq!(h.app.tabs.len(), 1);
    h.settle().await;
    // The Data Source row: F4 = console on its default database
    h.app.focus = Focus::Tree;
    h.key("Home");
    h.key("F4");
    let t = &h.app.tabs[h.app.active];
    assert!(matches!(t.kind, TabKind::Console(_)), "{}", t.title);
    assert_eq!((t.conn.as_str(), h.app.tabs.len(), h.app.focus), ("pg", 2, Focus::Editor));
    // ⌃l from the explorer: another console for the selected Data Source
    h.app.focus = Focus::Tree;
    h.key("Ctrl+L");
    assert_eq!(h.app.tabs.len(), 3);
    assert!(matches!(h.app.tabs[h.app.active].kind, TabKind::Console(_)));
}

/// m marks nodes (the GUI's ⌘-click): Delete asks once for every marked object (the Production confirmation on a Production source),
/// F4 opens a tab each, a command that doesn't fit every marked node says how to get back to one, Esc clears the marks
#[tokio::test]
async fn marked_nodes_act_together() {
    let mut h = explorer().await;
    for k in ["pg", "pg|shop", "pg|shop|public", "pg|shop|public|g:tables"] {
        h.app.tree.expanded.insert(k.into());
    }
    assert!(h.app.tree.select_key("pg|shop|public|g:tables|o:orders"));
    h.key("m");
    h.key("Down");
    h.key("m");
    let labels = |h: &Harness| h.app.tree.targets().iter().map(|n| n.label.clone()).collect::<Vec<_>>();
    assert_eq!(labels(&h), ["orders", "users"]);
    let s = h.screen(120, 12);
    assert!(s.contains("┏ 2 marked · Database") && h.app.status == "2 marked · Esc clears", "{s}");
    // A folder isn't an object: it can't be marked
    h.key("Up");
    h.key("Up");
    h.key("m");
    assert!(h.app.status.starts_with("Only data sources"), "{}", h.app.status);
    assert_eq!(labels(&h), ["orders", "users"]);
    // Delete: one confirmation, the tables in one statement (the server orders them by their foreign keys)
    h.key("Delete");
    h.settle().await;
    let Some(crate::app::Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirmation") };
    assert!(text.starts_with("Run this?") && text.contains("DROP TABLE public.orders, public.users;"), "{text}");
    h.key("n");
    // A Production source: the Production confirmation (red), the same as for one object
    h.app.conns[0].config.options.confirm_changes = true;
    h.key("Delete");
    h.settle().await;
    let Some(crate::app::Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirmation") };
    assert!(text.starts_with("Production data source \"local pg\"") && text.contains("DROP TABLE public.orders, public.users;"), "{text}");
    h.key("n");
    assert_eq!(labels(&h), ["orders", "users"]);
    // F4: a table tab each
    h.key("F4");
    assert_eq!(h.app.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["orders", "users"]);
    // With the data source marked too, Go to DDL (objects only) says how to get back to one node and opens nothing
    h.app.focus = Focus::Tree;
    h.key("Home");
    h.key("m");
    h.key("Ctrl+B");
    assert_eq!(h.app.status, "Go to DDL doesn't work on every marked node — m unmarks one, Esc clears the marks");
    assert_eq!(h.app.tabs.len(), 2);
    // Esc clears the marks (and stays in the explorer); the commands act on the cursor's node again
    h.key("Esc");
    assert!(h.app.tree.marked.is_empty());
    assert_eq!((h.app.focus, labels(&h)), (Focus::Tree, vec!["local pg".to_string()]));
}

/// Drop… of a table and a view over it: the view's statement first, whatever the tree order; marks in a collapsed folder are counted
/// apart in the title, as the commands skip them
#[tokio::test]
async fn drop_runs_views_first_and_hidden_marks_show() {
    let mut h = explorer().await;
    let mut view = h.app.tree.children["pg|shop|public|g:tables"][1].clone();
    (view.key, view.label, view.name, view.kind) = ("pg|shop|public|g:views|o:v_users".into(), "v_users".into(), Some("v_users".into()), NodeKind::Object("view".into()));
    let mut group = h.app.tree.children["pg|shop|public"][0].clone();
    (group.key, group.label) = ("pg|shop|public|g:views".into(), "views".into());
    h.app.tree.children.get_mut("pg|shop|public").unwrap().push(group);
    h.app.tree.children.insert("pg|shop|public|g:views".into(), vec![view]);
    for k in ["pg", "pg|shop", "pg|shop|public", "pg|shop|public|g:tables", "pg|shop|public|g:views"] {
        h.app.tree.expanded.insert(k.into());
    }
    for k in ["pg|shop|public|g:tables|o:users", "pg|shop|public|g:views|o:v_users"] {
        assert!(h.app.tree.select_key(k));
        h.key("m");
    }
    h.key("Delete");
    h.settle().await;
    let Some(crate::app::Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirmation") };
    assert!(text.contains("  DROP VIEW public.v_users;\n  DROP TABLE public.users;"), "{text}");
    h.key("n");
    h.app.tree.expanded.remove("pg|shop|public|g:views");
    let s = h.screen(120, 12);
    assert!(s.contains("┏ 1 marked (1 hidden) · Data"), "{s}");
}

/// A Redis key is not a table: Delete doesn't offer DROP TABLE for it
#[tokio::test]
async fn delete_on_a_redis_key_offers_no_drop() {
    let mut h = explorer().await;
    let mut key = h.app.tree.children["pg|shop|public|g:tables"][0].clone();
    key.kind = NodeKind::Object("key".into());
    h.app.tree.children.insert("pg|shop|public|g:tables".into(), vec![key]);
    for k in ["pg", "pg|shop", "pg|shop|public", "pg|shop|public|g:tables"] {
        h.app.tree.expanded.insert(k.into());
    }
    assert!(h.app.tree.select_key("pg|shop|public|g:tables|o:orders"));
    h.key("Delete");
    assert!(h.app.popup.is_none());
    assert_eq!(h.app.status, "Drop: select a table, view or sequence in the explorer");
}

/// A Drop… that fails still reloads the tree and clears the marks: on MySQL the statements before the failing one ran
#[tokio::test]
async fn failed_drop_reloads_the_tree() {
    let mut h = explorer().await;
    // Read-only: the run fails at once, before connecting
    let mut cfg = h.app.conns[0].config.clone();
    cfg.read_only = true;
    h.app.state.store.save(&cfg).unwrap();
    for k in ["pg", "pg|shop", "pg|shop|public", "pg|shop|public|g:tables"] {
        h.app.tree.expanded.insert(k.into());
    }
    for k in ["pg|shop|public|g:tables|o:orders", "pg|shop|public|g:tables|o:users"] {
        assert!(h.app.tree.select_key(k));
        h.key("m");
    }
    h.key("Delete");
    h.settle().await;
    // It comes up quiet (an async result): answered after the pause
    let Some(crate::app::Popup::Confirm { opened: Some(t), .. }) = &mut h.app.popup else { panic!("no quiet confirmation") };
    *t -= crate::app::App::TLS_ASK_GRACE;
    h.key("y");
    for _ in 0..20 {
        h.settle().await;
        if !h.app.status.contains("marked") {
            break;
        }
    }
    assert!(h.app.tree.marked.is_empty(), "{}", h.app.status);
    assert!(h.app.status.contains("read-only"), "{}", h.app.status);
    assert!(h.app.tree.children.get("pg|shop|public|g:tables").is_none_or(|c| c.len() != 2), "the tables were not reloaded");
}

/// Refresh with no node selected (an empty explorer) or from a console (the palette) reloads the data source list, as the GUI's ↻ and Refresh
#[tokio::test]
async fn refresh_without_a_node_reloads_the_data_sources() {
    let mut h = Harness::new().await;
    h.app.focus = Focus::Tree;
    let added = |id: &str| serde_json::from_value(serde_json::json!({ "id": id, "name": id, "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "shop" })).unwrap();
    // Added elsewhere (the GUI): only a reload of the list shows it
    h.app.state.store.save(&added("first")).unwrap();
    assert!(h.app.tree.selected().is_none());
    h.key("r");
    h.settle().await;
    assert_eq!(rows(&h), ["first"]);
    h.console_tab();
    h.app.focus = Focus::Editor;
    h.app.state.store.save(&added("second")).unwrap();
    h.app.run_action(crate::app::Action::Refresh);
    h.settle().await;
    assert_eq!(rows(&h), ["first", "second"]);
}
