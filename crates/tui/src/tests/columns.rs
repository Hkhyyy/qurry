//! Column widths (#109): < > = on the current column, Fit All / Reset from the palette; a table tab's widths are kept per table in
//! settings.json "columnWidths", where the GUI reads and writes them too
use crate::app::{Action, Focus, TabKind};
use crate::testkit::Harness;
use qurry_core::format::ColumnWidths;
use qurry_core::store::{Store, TableRef};

const MEMO: &str = "first order after the promotion; the customer asked for a call before delivery";

fn widths(h: &Harness) -> Vec<u16> {
    h.app.tabs.get(h.app.active).and_then(|t| match &t.kind {
        TabKind::Table(tt) => tt.grid.as_ref(),
        TabKind::Console(c) => c.grid.as_ref(),
        _ => None,
    })
    .map(|g| g.widths.clone())
    .unwrap_or_default()
}

/// What the GUI reads: the same settings file, through another Store (another process)
fn gui_side(h: &Harness, t: &TableRef) -> ColumnWidths {
    Store::new(h.app.state.store.dir()).column_widths(t)
}

#[tokio::test]
async fn keys_resize_the_current_column_and_save_it_for_the_table() {
    let mut h = Harness::new().await;
    h.table_tab("orders", &["idx", "order_no", "memo"], &[&["486", "O-20261101", MEMO], &["487", "O-20261102", "gift wrap"]], &["idx"]);
    h.app.focus = Focus::Grid;
    h.key("Right");
    h.key("Right");
    // Computed: the longest value, up to 40 cells (+1)
    assert_eq!(widths(&h)[2], 41);
    let s = h.screen(140, 12);
    assert!(s.contains("< > = width"), "the key bar shows the keys:\n{s}");
    assert!(!s.contains(MEMO), "cut while computed:\n{s}");

    for _ in 0..3 {
        h.key(">");
    }
    assert_eq!(widths(&h)[2], 53);
    assert_eq!(h.app.status, "memo: 53 cells wide");
    h.key("<");
    assert_eq!(widths(&h)[2], 49);
    h.key("=");
    assert_eq!(widths(&h)[2], MEMO.len() as u16);
    assert!(h.screen(140, 12).contains(MEMO), "fits its content");
    assert_eq!(widths(&h)[..2], [6, 11], "the other columns keep their computed widths");

    // Saved for the table (data source · database · schema · table), where the GUI reads it
    let orders = TableRef { source: "test".into(), database: "postgres".into(), schema: "public".into(), table: "orders".into() };
    assert_eq!(gui_side(&h, &orders), ColumnWidths::from([("memo".into(), MEMO.len() as u16)]));

    // Fit All Columns, then Reset Column Widths (the palette's): computed again, and nothing saved
    h.app.run_action(Action::ColumnFitAll);
    assert_eq!(widths(&h), [5, 10, MEMO.len() as u16]);
    assert_eq!(gui_side(&h, &orders).len(), 3);
    h.app.run_action(Action::ColumnReset);
    assert_eq!(widths(&h), [6, 11, 41]);
    assert!(gui_side(&h, &orders).is_empty());
    assert_eq!(h.app.status, "Column widths are the computed ones again");
}

/// A console result keeps its widths until it runs again; nothing is saved
#[tokio::test]
async fn console_result_widths_are_not_saved() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(|_| {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "text".into() };
        Ok(qurry_core::db::QueryResult { columns: vec![col("a"), col("b")], rows: vec![vec!["x".into(), "y".into()]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable: None })
    });
    h.console_with("select a, b from t", 0, 0);
    h.key("F5");
    h.settle().await;
    h.app.focus = Focus::Grid;
    assert_eq!(widths(&h), [5, 5]);
    h.key(">");
    assert_eq!(widths(&h), [9, 5]);
    let s = std::fs::read_to_string(h.app.state.store.dir().join("settings.json")).unwrap_or_default();
    assert!(!s.contains("columnWidths"), "{s}");
}

/// The shared PostgreSQL (QURRY_TEST_PG=host:port:user:db): widths the GUI saved for a table (its tab on the source's default database
/// names none) show when the TUI opens it; > saves the new width, which the reopened tab and the GUI see
#[tokio::test]
#[ignore]
async fn pg_table_tab_widths_are_shared_with_the_gui() {
    let e = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG");
    let p: Vec<&str> = e.split(':').collect();
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "pg", "kind": "postgres", "host": p[0], "port": p[1].parse::<u16>().unwrap(), "user": p[2], "database": p[3] })).unwrap();
    h.app.state.save_connection(cfg, None, None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    let gui_tab = TableRef { source: "test".into(), database: String::new(), schema: "pg_catalog".into(), table: "pg_am".into() };
    Store::new(h.app.state.store.dir()).set_column_widths(&gui_tab, &ColumnWidths::from([("amname".into(), 30)])).unwrap();

    let open = |h: &mut Harness| h.app.open_table("test", p[3].to_string(), "pg_catalog".into(), "pg_am".into(), "table".into());
    open(&mut h);
    h.settle().await;
    let amname = |h: &Harness| match h.app.tabs.get(h.app.active).map(|t| &t.kind) {
        Some(TabKind::Table(tt)) => tt.grid.as_ref().and_then(|g| Some(g.widths[g.result.columns.iter().position(|c| c.name == "amname")?])),
        _ => None,
    };
    assert_eq!(amname(&h), Some(30), "{}", h.app.status);
    h.app.focus = Focus::Grid;
    let Some(TabKind::Table(tt)) = h.app.tabs.get_mut(h.app.active).map(|t| &mut t.kind) else { panic!() };
    tt.grid.as_mut().unwrap().col = tt.grid.as_ref().unwrap().result.columns.iter().position(|c| c.name == "amname").unwrap();
    h.key(">");
    assert_eq!(amname(&h), Some(34));
    assert_eq!(gui_side(&h, &gui_tab), ColumnWidths::from([("amname".into(), 34)]));

    h.app.close_tab_now(h.app.active);
    open(&mut h);
    h.settle().await;
    assert_eq!(amname(&h), Some(34), "the reopened tab keeps it");
    let _ = h.app.state.disconnect("test".into()).await;
}
