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
