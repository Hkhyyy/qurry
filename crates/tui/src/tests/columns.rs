//! Column widths (#109): < > = on the current column, Fit All / Reset from the palette; a table tab's widths are kept per table in
//! settings.json "columnWidths", where the GUI reads and writes them too
use crate::app::{Action, Focus, Popup, TabKind};
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
    let Some(e) = qurry_core::testenv::var("QURRY_TEST_PG") else { return };
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

// ───────── Column layout (#178): hide and pin columns, kept per table in settings.json "columnLayout" with the GUI ─────────

fn orders() -> TableRef {
    TableRef { source: "test".into(), database: "postgres".into(), schema: "public".into(), table: "orders".into() }
}

/// A table tab `orders` with wide columns, so only a few fit at 80 cells; focus in its grid
fn wide_orders(h: &mut Harness) -> u64 {
    let id = h.table_tab("orders", &["id", "order_no", "memo", "note", "status", "total"], &[&["486", "O-20261101", MEMO, MEMO, "paid", "12000"], &["487", "O-20261102", "gift wrap", "-", "sent", "800"]], &["id"]);
    h.app.focus = Focus::Grid;
    id
}

/// The header line of the grid (the line with its column names, under its title)
fn header(s: &str) -> String {
    s.lines().skip_while(|l| !l.contains("public.orders")).nth(1).unwrap_or_default().to_string()
}

/// Shift+H hides the cursor's column: the header no longer has it, the status line says 1 hidden, the cursor is on the next
/// column; the layout is saved for the table, where the GUI (another Store) reads it. Columns… › Show All brings it back
#[tokio::test]
async fn hide_the_current_column() {
    let mut h = Harness::new().await;
    wide_orders(&mut h);
    h.key("Right");
    assert!(header(&h.screen(170, 12)).contains("order_no"));
    h.key("Shift+H");
    let s = h.screen(170, 12);
    assert!(!header(&s).contains("order_no"), "hidden:\n{s}");
    assert!(s.contains("1 hidden"), "the status line counts it:\n{s}");
    let status = std::mem::take(&mut h.app.status);
    let wide = h.screen(260, 12);
    h.app.status = status;
    assert!(wide.contains("x export · H P C columns"), "the key bar has the keys:\n{wide}");
    assert_eq!(grid(&h).result.columns[grid(&h).col].name, "memo", "the cursor moves to the next column");
    assert!(h.app.status.starts_with("order_no hidden · Columns… (C) shows it again"), "{}", h.app.status);
    let saved = std::fs::read_to_string(h.app.state.store.dir().join("settings.json")).unwrap();
    assert!(saved.contains("\"columnLayout\""), "{saved}");
    assert_eq!(Store::new(h.app.state.store.dir()).column_layout(&orders()).hidden, ["order_no"]);

    // Columns… lists every column, the hidden one unchecked; Show All brings it back and saves that
    h.key("Shift+C");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no Columns…: {}", h.app.status) };
    let labels: Vec<&str> = p.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["[x] id", "[ ] order_no", "[x] memo", "[x] note", "[x] status", "[x] total", "    Show All"]);
    h.type_str("show all");
    h.key("Enter");
    assert!(matches!(&h.app.popup, Some(Popup::Palette(p)) if p.items[1].label == "[x] order_no"), "stays open, checked again");
    h.key("Esc");
    assert!(header(&h.screen(170, 12)).contains("order_no"));
    assert!(Store::new(h.app.state.store.dir()).column_layout(&orders()).is_empty());
}

/// In Columns…, Space hides or shows the row's column and ⇥ pins it, typing filters; the last shown column can't be hidden
#[tokio::test]
async fn columns_picker_hides_and_pins() {
    let mut h = Harness::new().await;
    h.table_tab("orders", &["id", "memo"], &[&["486", MEMO]], &["id"]);
    h.app.focus = Focus::Grid;
    h.key("Shift+C");
    h.type_str("mem");
    h.key("Tab");
    assert!(grid(&h).is_pinned(1), "memo pinned");
    h.key("Space");
    assert_eq!(grid(&h).view.view, [0], "memo hidden (and so not pinned)");
    assert!(!grid(&h).is_pinned(1));
    // The filter stays: one row and Show All
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!() };
    assert_eq!(p.input.lines().concat(), "mem");
    h.key("Esc");
    h.key("Shift+H");
    assert_eq!(grid(&h).view.view, [0], "the only column shown stays");
    assert_eq!(h.app.status, "id is the only column shown");
}

/// Two pinned columns stay at the left while the others scroll (> keys, the wheel); the line after them is heavier
#[tokio::test]
async fn pinned_columns_stay_while_scrolling() {
    let mut h = Harness::new().await;
    wide_orders(&mut h);
    // total, then status: pinned in that order
    h.key("End");
    h.key("Shift+P");
    h.key("End");
    h.key("Shift+P");
    assert_eq!(grid(&h).view.view, [5, 4, 0, 1, 2, 3]);
    assert_eq!(Store::new(h.app.state.store.dir()).column_layout(&orders()).pinned, ["total", "status"]);
    let s = h.screen(80, 12);
    assert!(header(&s).trim_start_matches(|c: char| !c.is_alphabetic()).starts_with("total"), "pinned first:\n{s}");
    assert!(header(&s).contains("║"), "the pinned columns' edge:\n{s}");
    // To the last column and scroll: the screen still starts with the pinned ones
    h.key("End");
    for _ in 0..3 {
        h.key("Right");
    }
    let s = h.screen(80, 12);
    let head = header(&s);
    assert!(head.contains("note") && !head.contains("order_no"), "scrolled right:\n{s}");
    assert!(head.find("total") < head.find("status") && head.find("status") < head.find("note"), "pinned ones first:\n{s}");
    grid_mut(&mut h).hscroll(-10);
    grid_mut(&mut h).hscroll(10);
    let s = h.screen(80, 12);
    assert!(header(&s).find("total") < header(&s).find("status"), "the wheel keeps them:\n{s}");
    // Unpin: back in result order
    h.key("Home");
    h.key("Shift+P");
    assert_eq!(grid(&h).view.view, [4, 0, 1, 2, 3, 5]);
}

/// A saved layout is read when the table loads (here the load fails without a server and keeps the columns); a key the result
/// doesn't have (a column renamed since) is ignored
#[tokio::test]
async fn saved_layout_applies_when_the_table_loads() {
    let mut h = Harness::new().await;
    let id = wide_orders(&mut h);
    let layout = qurry_core::format::ColumnLayout { hidden: vec!["memo".into(), "renamed".into()], pinned: vec!["gone".into(), "total".into()] };
    Store::new(h.app.state.store.dir()).set_column_layout(&orders(), &layout).unwrap();
    h.app.reload_table(id);
    h.settle().await;
    assert_eq!(grid(&h).view.view, [5, 0, 1, 3, 4]);
    assert_eq!((grid(&h).view.pinned, grid(&h).view.hidden), (1, 1));
    assert!(h.screen(170, 12).contains("1 hidden"));
}

/// Pinned columns that fill the grid leave room for the cursor's column: past them it is still drawn (the pinned ones scroll away
/// while it is there, as DataGrip's frozen area does), and they come back with the cursor
#[tokio::test]
async fn cursor_column_shows_when_pinned_ones_fill_the_grid() {
    let mut h = Harness::new().await;
    h.table_tab("orders", &["id", "memo", "note", "status", "total"], &[&["486", MEMO, "-", "paid", "12000"]], &["id"]);
    h.app.focus = Focus::Grid;
    h.key("Right");
    h.key("Shift+P");
    h.key("Right");
    h.key("Shift+P");
    assert_eq!(grid(&h).view.view, [1, 0, 2, 3, 4], "memo, id pinned");
    h.key("End");
    for w in [80, 60] {
        let s = h.screen(w, 12);
        assert!(header(&s).contains("total"), "the cursor's column at {w}:\n{s}");
        assert!(!header(&s).contains("║"), "the pinned ones scrolled away:\n{s}");
    }
    h.key("Home");
    let s = h.screen(80, 12);
    assert!(header(&s).contains("memo") && header(&s).contains("║"), "pinned again on a pinned column:\n{s}");
}

/// A cell range or marked rows never make P and H act on several columns (#216): the TUI has no column selection (the GUI's
/// header clicks), so they act on the cursor's column
#[tokio::test]
async fn pin_and_hide_act_on_the_cursor_column_with_a_range_or_rows_selected() {
    let mut h = Harness::new().await;
    wide_orders(&mut h);
    // order_no..note selected (⇧ arrows), the cursor on note
    h.key("Right");
    h.key("Shift+Right");
    h.key("Shift+Right");
    h.key("Shift+P");
    assert_eq!(grid(&h).view.view, [3, 0, 1, 2, 4, 5], "only note pinned");
    assert_eq!(h.app.status, "note pinned at the left");
    h.key("Shift+P");
    assert_eq!(grid(&h).view.view, [0, 1, 2, 3, 4, 5]);
    h.key("Shift+H");
    assert_eq!(grid(&h).view.view, [0, 1, 2, 4, 5], "only note hidden");
    assert_eq!(Store::new(h.app.state.store.dir()).column_layout(&orders()).hidden, ["note"]);

    // Both rows marked (every column of them selected): H still hides the cursor's column alone
    h.key("Esc");
    h.key("m");
    h.key("Down");
    h.key("m");
    let c = grid(&h).col;
    h.key("Shift+H");
    assert_eq!(grid(&h).view.view.len(), 4);
    assert!(!grid(&h).view.view.contains(&c), "the cursor's column hidden");
}

/// A DBA report's auto-refresh keeps its hidden and pinned columns (it runs again on its own)
#[tokio::test]
async fn dba_refresh_keeps_the_column_layout() {
    use qurry_core::db::{ColumnInfo, QueryResult};
    let mut h = Harness::new().await;
    let res = || QueryResult {
        columns: ["pid", "state", "query"].iter().map(|n| ColumnInfo { name: (*n).into(), type_name: "TEXT".into() }).collect(),
        rows: vec![vec!["10".into(), "active".into(), "select 1".into()]],
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 0,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
        cut: vec![],
    };
    let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "sessions");
    let d = crate::app::DbaTab { report: "sessions".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res())), loaded: None, error: None, running: false, run: 0, auto: true };
    let id = h.app.push_tab("test", "postgres".into(), "Sessions".into(), TabKind::Dba(d));
    h.app.focus = Focus::Grid;
    h.key("End");
    h.key("Shift+H");
    h.key("Right");
    h.key("Shift+P");
    for _ in 0..2 {
        h.app.dba_apply(id, 0, Ok(res()));
        let g = h.app.grid_mut().unwrap();
        assert_eq!((g.view.view.clone(), g.view.pinned), (vec![1, 0], 1), "query hidden, state pinned");
    }
}

/// A layout the GUI reset (Show All removes the table's entry) is what the tab's reload shows; the tab's own stays only while
/// saving it failed
#[tokio::test]
async fn reload_takes_the_saved_layout_unless_saving_failed() {
    let mut h = Harness::new().await;
    let id = wide_orders(&mut h);
    h.key("Right");
    h.key("Shift+H");
    assert_eq!(Store::new(h.app.state.store.dir()).column_layout(&orders()).hidden, ["order_no"]);
    Store::new(h.app.state.store.dir()).set_column_layout(&orders(), &Default::default()).unwrap();
    h.app.reload_table(id);
    h.settle().await;
    assert_eq!(grid(&h).view.hidden, 0, "the GUI's reset");

    // A folder where the settings file was: saving fails
    std::fs::remove_file(h.app.state.store.dir().join("settings.json")).unwrap();
    std::fs::create_dir_all(h.app.state.store.dir().join("settings.json")).unwrap();
    h.key("Shift+H");
    assert!(h.app.status.starts_with("Couldn't save the column layout"), "{}", h.app.status);
    h.app.reload_table(id);
    h.settle().await;
    assert_eq!(grid(&h).view.hidden, 1, "kept while unsaved");
}

/// A saved layout that hides every column shows them all and is kept so: H then hides the cursor's column
#[tokio::test]
async fn layout_hiding_every_column_shows_them_and_hide_works() {
    let mut h = Harness::new().await;
    let id = h.table_tab("orders", &["id", "memo"], &[&["486", "a"]], &["id"]);
    h.app.focus = Focus::Grid;
    let all = qurry_core::format::ColumnLayout { hidden: vec!["id".into(), "memo".into()], pinned: vec![] };
    Store::new(h.app.state.store.dir()).set_column_layout(&orders(), &all).unwrap();
    h.app.reload_table(id);
    h.settle().await;
    assert_eq!(grid(&h).view.view, [0, 1]);
    assert!(grid(&h).layout.hidden.is_empty());
    h.key("Right");
    h.key("Shift+H");
    assert_eq!(grid(&h).view.view, [0], "memo hidden");
    assert_eq!(Store::new(h.app.state.store.dir()).column_layout(&orders()).hidden, ["memo"]);
}

/// A copy skips hidden columns: a range across one, and a marked row; the IN list of a range too
#[tokio::test]
async fn copied_selection_skips_the_hidden_column() {
    let mut h = Harness::new().await;
    h.table_tab("orders", &["id", "memo", "total"], &[&["486", "a", "10"], &["487", "b", "20"]], &["id"]);
    h.app.focus = Focus::Grid;
    h.key("Right");
    h.key("Shift+H");
    h.key("Home");
    h.key("Shift+Down");
    h.key("Shift+Right");
    assert_eq!(grid(&h).selection_tsv(), "486\t10\n487\t20");
    assert_eq!(grid(&h).in_list(qurry_core::store::DbKind::Postgres).0, "(('486', '10'), ('487', '20'))");
    h.key("Esc");
    h.key("g");
    h.key("m");
    assert_eq!(grid(&h).selection_tsv(), "486\t10");
    // A paste from the first cell fills the shown columns
    h.key("Home");
    assert_eq!(grid(&h).paste_spots("x\ty"), [(0, 0), (0, 2)]);
}

fn grid_mut(h: &mut Harness) -> &mut crate::grid::Grid {
    h.app.grid_mut().unwrap()
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

/// docs_db, counting the `-- fetch_values` asks in `$n`
macro_rules! counting_docs_db {
    ($n:ident) => {{
        static $n: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn db(q: &str) -> Result<qurry_core::db::QueryResult, String> {
            if q.starts_with("-- fetch_values") {
                $n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            docs_db(q)
        }
        (db as fn(&str) -> Result<qurry_core::db::QueryResult, String>, &$n)
    }};
}

/// The value panel on a cut cell whose row is gone asks once while the cursor stays on it (not per key or mouse move) and says so
/// over the cut text; the status line doesn't grow. Back on it later, it asks again
#[tokio::test]
async fn value_panel_asks_once_for_a_row_that_is_gone() {
    let (db, n) = counting_docs_db!(N);
    let mut h = Harness::new().await;
    h.app.fake_db = Some(db);
    cut_docs(&mut h, &["id"], &[0, 1]);
    h.key("Down");
    h.key("v");
    h.settle().await;
    assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
    let status = h.app.status.clone();
    for _ in 0..5 {
        h.key("Right");
        h.settle().await;
    }
    assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1, "no ask per key");
    assert_eq!(h.app.status, status, "the note is said once");
    let s = h.screen(160, 16);
    assert!(s.contains("│This row isn't in public.docs any more: this"), "{s}");
    // Row 1 is read whole; back on row 2 it asks once more
    h.key("Up");
    h.settle().await;
    h.key("Down");
    h.settle().await;
    assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 3);
}

/// The cursor moved to another cut cell while the panel read one: that one is read once the first answer is in
#[tokio::test]
async fn value_panel_reads_where_the_cursor_went_meanwhile() {
    let (db, n) = counting_docs_db!(N);
    let mut h = Harness::new().await;
    h.app.fake_db = Some(db);
    cut_docs(&mut h, &["id"], &[0, 1]);
    h.key("v");
    h.key("Down");
    h.settle().await;
    assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert!(!grid(&h).reading && !grid(&h).is_cut(0, 1));
    let s = h.screen(160, 16);
    assert!(!s.contains("Loading the whole value") && s.contains("This row isn't in public.docs any more"), "{s}");
}

/// Rows sorted while the panel's read is out: the answer goes to the row asked for, and the row now under the cursor (which the
/// table has) isn't said to be gone but is read in turn
#[tokio::test]
async fn value_panel_read_follows_its_row_through_a_sort() {
    fn both_db(q: &str) -> Result<qurry_core::db::QueryResult, String> {
        let keys = q.strip_prefix("-- fetch_values public.docs [\"body\"] by [\"id\"]: [").ok_or("not a read")?;
        let rows = keys.split("], [").enumerate().map(|(i, k)| vec![serde_json::json!(i), serde_json::json!(format!("whole {}", if k.contains("Some(\"1\")") { 1 } else { 2 }))]);
        Ok(answer(rows.collect()))
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(both_db);
    cut_docs(&mut h, &["id"], &[0, 1]);
    h.key("v");
    // id ↓ before the answer comes: id 2 is on row 0 now, under the cursor
    let g = h.app.grid_mut().unwrap();
    g.col = 0;
    g.toggle_local_sort();
    g.toggle_local_sort();
    g.col = 1;
    h.settle().await;
    let g = grid(&h);
    assert_eq!(g.unread, None);
    assert_eq!(g.result.rows[0], vec![serde_json::json!("2"), serde_json::json!("whole 2")]);
    assert_eq!(g.result.rows[1], vec![serde_json::json!("1"), serde_json::json!("whole 1")]);
    assert!(g.cut.is_empty() && !h.app.status.contains("any more"), "{}", h.app.status);
}

/// ↩ while the value panel reads the cell says it is still loading (not a silent Ready); once it's in, ↩ edits the whole value
#[tokio::test]
async fn enter_while_the_panel_reads_says_so() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(docs_db);
    cut_docs(&mut h, &["id"], &[0]);
    h.key("v");
    h.key("Enter");
    h.settle().await;
    assert!(h.app.popup.is_none());
    assert!(h.app.status.starts_with("Still loading the whole value"), "{}", h.app.status);
    h.key("Enter");
    assert!(matches!(&h.app.popup, Some(crate::app::Popup::Input { input, .. }) if input.lines().join("") == "{\"a\": \"the whole value\"}"));
}

/// A second ↩ while the whole value loads asks nothing more and doesn't reopen the editor over what was typed
#[tokio::test]
async fn second_enter_while_loading_keeps_the_editor() {
    let (db, n) = counting_docs_db!(N);
    let mut h = Harness::new().await;
    h.app.fake_db = Some(db);
    cut_docs(&mut h, &["id"], &[0]);
    h.key("Enter");
    h.key("Enter");
    assert!(h.app.popup.is_none());
    h.step().await;
    h.type_str("XYZ");
    h.settle().await;
    assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(matches!(&h.app.popup, Some(crate::app::Popup::Input { input, .. }) if input.lines().join("") == "{\"a\": \"the whole value\"}XYZ"));
}

/// A failed read is said over that cell only: the panel asks again once the cursor comes back
#[tokio::test]
async fn value_panel_retries_after_a_failed_read() {
    static FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
    fn db(q: &str) -> Result<qurry_core::db::QueryResult, String> {
        if FAIL.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err("timeout".into());
        }
        docs_db(q)
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(db);
    cut_docs(&mut h, &["id"], &[0, 1]);
    h.key("v");
    h.settle().await;
    assert!(h.app.status.starts_with("Couldn't read the whole value") && h.app.status.contains("timeout"), "{}", h.app.status);
    assert!(h.screen(160, 16).contains("│The whole value couldn't be read"));
    h.key("Right");
    h.settle().await;
    assert!(grid(&h).is_cut(0, 1), "not asked again while the cursor stays");
    h.key("Down");
    h.settle().await;
    h.key("Up");
    h.settle().await;
    assert!(!grid(&h).is_cut(0, 1), "asked again on the way back");
}

/// A copy whose whole values come after another tab was shown says it wasn't done (the status line doesn't stay on loading)
#[tokio::test]
async fn copy_after_the_tab_changed_says_not_done() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(docs_db);
    h.table_tab("other", &["id"], &[&["1"]], &["id"]);
    let other = h.app.active;
    cut_docs(&mut h, &["id"], &[0]);
    h.key("y");
    assert_eq!(h.app.status, "Loading the whole value…");
    h.app.active = other;
    h.settle().await;
    assert_eq!(h.app.status, "Not done: the result changed before the whole value came");
}
