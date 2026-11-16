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
        TabKind::Console(c) => c.grid(),
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
    // Wide enough for the key bar beside the status line's items (Position: the key hints go first when narrow)
    let s = h.screen(170, 12);
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
        Ok(qurry_core::db::QueryResult { columns: vec![col("a"), col("b")], rows: vec![vec!["x".into(), "y".into()]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] })
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

/// Wide characters take 2 cells, so Fit sizes a Hangul column to them (as the GUI does); a name the result repeats (a join's two
/// `id` columns) resizes each column on its own
#[tokio::test]
async fn fit_counts_wide_characters_and_repeated_names_resize_on_their_own() {
    const HANGUL: &str = "배송전에전화주세요";
    let mut h = Harness::new().await;
    h.app.fake_db = Some(|_| {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "text".into() };
        Ok(qurry_core::db::QueryResult { columns: vec![col("id"), col("memo"), col("id")], rows: vec![vec!["1".into(), HANGUL.into(), "2".into()]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] })
    });
    h.console_with("select o.id, memo, u.id from o join u on true", 0, 0);
    h.key("F5");
    h.settle().await;
    h.app.focus = Focus::Grid;
    assert_eq!(widths(&h), [5, 19, 5]);
    h.key("Right");
    h.key("Right");
    h.key(">");
    assert_eq!(widths(&h), [5, 19, 9], "the first id keeps its width");
    assert_eq!(h.app.status, "id: 9 cells wide");
    h.key("Left");
    h.key("Left");
    h.key(">");
    assert_eq!(widths(&h), [9, 19, 9]);
    h.key("Right");
    h.key("<");
    h.key("<");
    // The screen text has a blank after each wide character (its second cell)
    let drawn: String = HANGUL.chars().map(|c| format!("{c} ")).collect();
    assert!(!h.screen(80, 14).contains(drawn.trim_end()), "cut while narrow");
    h.key("=");
    assert_eq!(widths(&h)[1], 18, "9 Hangul characters are 18 cells ({})", h.app.status);
    let s = h.screen(80, 14);
    assert!(s.contains(&format!("│{drawn}│")), "fits its content:\n{s}");
}

/// A settings file that can't be written: the widths still apply, also after the table reloads (until the tab closes, as the status says)
#[tokio::test]
async fn unsaved_widths_last_until_the_tab_closes() {
    let mut h = Harness::new().await;
    let id = h.table_tab("orders", &["idx", "memo"], &[&["486", MEMO]], &["idx"]);
    h.app.focus = Focus::Grid;
    h.key("Right");
    // A folder where the file should be: saving it fails
    std::fs::create_dir_all(h.app.state.store.dir().join("settings.json")).unwrap();
    h.key(">");
    assert_eq!(widths(&h)[1], 45);
    assert!(h.app.status.starts_with("Couldn't save the column widths"), "{}", h.app.status);
    assert!(h.app.status.contains("They apply until the tab closes"), "{}", h.app.status);
    // Reload (⌃r, a WHERE, ORDER BY): no server here, so the reload fails and keeps the columns; the width stays
    h.app.reload_table(id);
    h.settle().await;
    assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Table(tt) if tt.error.is_some()), "the reload ran");
    assert_eq!(widths(&h)[1], 45);
}

// ───────── Cut values (#183): read whole by the table's key before an edit, a copy, the value panel ─────────

/// A table tab `docs` (id, body) whose bodies came cut (`cut`: the rows), keyed by id unless `pk` is empty; the body cell selected
fn cut_docs(h: &mut Harness, pk: &[&str], cut: &[usize]) -> u64 {
    let id = h.table_tab("docs", &["id", "body"], &[&["1", "{\"a\":"], &["2", "{\"b\":"]], pk);
    if let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind {
        tt.grid.as_mut().unwrap().cut = cut.iter().map(|&r| (r, 1)).collect();
    }
    h.app.focus = Focus::Grid;
    h.key("Right");
    id
}

fn grid(h: &Harness) -> &crate::grid::Grid {
    match &h.app.tabs[h.app.active].kind {
        TabKind::Table(tt) => tt.grid.as_ref().unwrap(),
        _ => panic!("not a table tab"),
    }
}

fn answer(rows: Vec<Vec<serde_json::Value>>) -> qurry_core::db::QueryResult {
    qurry_core::db::QueryResult { columns: vec![], rows, rows_affected: 1, truncated: false, elapsed_ms: 0, has_result_set: false, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] }
}

/// The stand-in database: the whole body of id 1 (id 2 is gone from the table); the statements Submit sends are recorded
static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
fn docs_db(q: &str) -> Result<qurry_core::db::QueryResult, String> {
    let Some(keys) = q.strip_prefix("-- fetch_values public.docs [\"body\"] by [\"id\"]: [") else {
        SEEN.lock().unwrap().push(q.into());
        return Ok(answer(vec![]));
    };
    // The keys asked, in order: [Some("1")], [Some("2")] …
    let rows = keys.split("], [").enumerate().filter(|(_, k)| k.contains("Some(\"1\")"));
    Ok(answer(rows.map(|(i, _)| vec![serde_json::json!(i), serde_json::json!("{\"a\": \"the whole value\"}")]).collect()))
}

/// ↩ on a cut cell never opens the editor on the cut text: the whole value is read first (no server: it says it is loading, then
/// why it couldn't be read); a table without a key says why
#[tokio::test]
async fn edit_of_a_cut_value_waits_for_the_whole_value() {
    let mut h = Harness::new().await;
    cut_docs(&mut h, &["id"], &[0]);
    h.key("Enter");
    assert!(h.app.popup.is_none(), "no editor on the cut text");
    assert_eq!(h.app.status, "Loading the whole value…");
    h.settle().await;
    assert!(h.app.popup.is_none() && h.app.status.starts_with("Couldn't read the whole value"), "{}", h.app.status);
    assert!(grid(&h).is_cut(0, 1) && grid(&h).edits.is_empty());

    let mut h = Harness::new().await;
    cut_docs(&mut h, &[], &[0]);
    h.key("Enter");
    assert!(h.app.popup.is_none());
    assert_eq!(h.app.status, "Cut at 5 characters; the whole value needs a primary key (public.docs has none)");
    // y copies the cut text and says how to get it whole
    h.key("y");
    assert_eq!(h.app.status, "1 value is cut (no key to fetch it whole); ⌃k › Grid Long Values… raises the limit · Copied: {\"a\":");
    // The value panel says why over the cut text
    h.key("v");
    let s = h.screen(160, 16);
    assert!(s.contains("│Cut at 5 characters; the whole value needs a") && s.contains("│{\"a\":…"), "{s}");
}

/// With the key: the editor opens on the whole value, Submit writes the edited whole value (never the cut text); y and the value
/// panel read it whole too; a row the table no longer has stays cut and says so
#[tokio::test]
async fn cut_values_are_read_whole_by_key() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(docs_db);
    cut_docs(&mut h, &["id"], &[0, 1]);
    h.key("y");
    h.settle().await;
    assert_eq!(h.app.status, "Copied: {\"a\": \"the whole value\"}");
    assert!(!grid(&h).is_cut(0, 1) && grid(&h).cut.contains(&(1, 1)));
    // Row 2 is gone from the table: it stays cut, and the editor doesn't open
    h.key("Down");
    h.key("Enter");
    h.settle().await;
    assert!(h.app.popup.is_none() && h.app.status.contains("1 row isn't in public.docs any more"), "{}", h.app.status);
    // The value panel reads row 1 whole (already) and shows it pretty-printed
    h.key("Up");
    h.key("v");
    h.settle().await;
    assert!(h.screen(160, 16).contains("\"the whole value\""));
    h.key("v");
    // Edit row 1's whole value and submit
    h.key("Enter");
    assert!(matches!(&h.app.popup, Some(crate::app::Popup::Input { input, .. }) if input.lines().join("") == "{\"a\": \"the whole value\"}"));
    h.key("Backspace");
    h.type_str(", \"b\": 1}");
    h.key("Enter");
    SEEN.lock().unwrap().clear();
    h.key("Ctrl+S");
    h.settle().await;
    let seen = SEEN.lock().unwrap().clone();
    assert_eq!(seen.first().map(String::as_str), Some("UPDATE public.docs SET body = '{\"a\": \"the whole value\", \"b\": 1}' WHERE id = '1'"), "{seen:?}");
}

/// Submit never finds a row by a key cell that came cut (the WHERE would hold only its start)
#[tokio::test]
async fn submit_refuses_a_row_found_by_a_cut_key() {
    static SENT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn record(q: &str) -> Result<qurry_core::db::QueryResult, String> {
        SENT.lock().unwrap().push(q.into());
        Ok(answer(vec![]))
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(record);
    let id = h.table_tab("docs", &["id", "body"], &[&["k-long", "x"]], &["id"]);
    if let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind {
        tt.grid.as_mut().unwrap().cut.insert((0, 0));
    }
    h.app.focus = Focus::Grid;
    h.key("d");
    h.key("Ctrl+S");
    h.settle().await;
    assert!(SENT.lock().unwrap().is_empty());
    assert_eq!(h.app.status, "Not submitted: row 1's key id came cut, so the row can't be found. Set ⌃k › Grid Long Values… to 0 and run again");
    assert!(h.table(id).grid.as_ref().unwrap().dirty());
}

/// ⌃k › Grid Long Values… shows the setting and saves it where the GUI reads it; text that isn't a number is refused
#[tokio::test]
async fn grid_long_values_setting() {
    let mut h = Harness::new().await;
    h.app.run_action(Action::GridLongValues);
    assert!(matches!(&h.app.popup, Some(crate::app::Popup::Input { input, .. }) if input.lines().join("") == "4096"));
    for _ in 0..4 {
        h.key("Backspace");
    }
    h.type_str("0");
    h.key("Enter");
    assert_eq!(h.app.status, "Long values come whole from the next run");
    assert_eq!(Store::new(h.app.state.store.dir()).grid_cut_chars(), 0);
    h.app.run_action(Action::GridLongValues);
    h.type_str("x");
    h.key("Enter");
    assert_eq!(h.app.status, "Not saved: \"0x\" isn't a number. Enter 0 or more");
    assert_eq!(h.app.state.store.grid_cut_chars(), 0);
}
