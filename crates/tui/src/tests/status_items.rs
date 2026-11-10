//! The status line's items for a grid (#132): Position in the corner (the cursor's cell, a range, or the marked rows over the cursor's
//! column), the selection's Not null / Sum / Avg / Min / Max, the cell's Size with its column's limit, ⌃k › Status Bar Items… (shared
//! with the GUI through settings.json), and what goes first when the line is narrow
use crate::app::{Focus, Popup, TabKind, TableCols};
use crate::grid::Grid;
use crate::testkit::Harness;
use qurry_core::db::{ColumnInfo, QueryResult};
use qurry_core::editable::{EditTarget, Editable};
use qurry_core::statusbar::{ColumnType, Item, DEFAULTS};
use qurry_core::store::Store;

fn payment(h: &mut Harness) -> u64 {
    let id = h.table_tab("payment", &["id", "user_id", "amount", "card_brand"], &[&["1", "41", "129000", "VISA"], &["2", "7", "59000", "MASTER"], &["3", "18", "1200000", "AMEX"], &["4", "41", "0", ""]], &["id"]);
    h.app.focus = Focus::Grid;
    id
}

fn status_line(h: &mut Harness, w: u16) -> String {
    h.screen(w, 20).lines().last().unwrap().trim_end().to_string()
}

/// The palette's rows (label and hint) while it is open
fn rows(h: &Harness) -> Vec<String> {
    match &h.app.popup {
        Some(Popup::Palette(p)) => p.items.iter().map(|i| format!("{} {}", i.label.trim(), i.hint)).collect(),
        _ => panic!("no palette"),
    }
}

/// Defaults: Position in the corner (the cursor's cell, then a ⇧-arrow range), nothing while focus is elsewhere
#[tokio::test]
async fn position_is_the_cell_or_the_range() {
    let mut h = Harness::new().await;
    assert_eq!(h.app.status_items, DEFAULTS);
    payment(&mut h);
    assert!(status_line(&mut h, 160).ends_with("⌃k actions   Row 1, Col 1"), "{}", status_line(&mut h, 160));
    h.key("Shift+Down");
    h.key("Shift+Down");
    assert!(status_line(&mut h, 160).ends_with("   Rows 1–3 (3) · Col 1"), "{}", status_line(&mut h, 160));
    h.key("Shift+Right");
    assert!(status_line(&mut h, 160).ends_with("   Rows 1–3 (3) · Cols 1–2"), "{}", status_line(&mut h, 160));
    h.app.focus = Focus::Tree;
    assert!(!status_line(&mut h, 160).contains("Rows"), "{}", status_line(&mut h, 160));
}

/// The marked rows (m) are the selection, over the cursor's column: Not null counts every value (an empty one too), the numbers
/// items only the numbers; rows that aren't one range are counted
#[tokio::test]
async fn marked_rows_over_the_cursors_column() {
    let mut h = Harness::new().await;
    h.app.status_items = vec![Item::NotNull, Item::Sum, Item::Avg, Item::Min, Item::Max, Item::Position];
    payment(&mut h);
    h.key("Right");
    h.key("Right");
    h.key("m");
    h.key("Down");
    h.key("Down");
    h.key("m");
    assert!(status_line(&mut h, 220).ends_with("   Not null: 2 │ Sum: 1,329,000 │ Avg: 664,500 │ Min: 129,000 │ Max: 1,200,000 │ 2 rows · Col 3"), "{}", status_line(&mut h, 220));
    // An edit of a marked cell counts at once (the sums are kept between frames only while nothing changed)
    h.app.grid_mut().unwrap().set_value(0, 2, "1000".into());
    assert!(status_line(&mut h, 220).contains("Sum: 1,201,000 │"), "{}", status_line(&mut h, 220));
    // card_brand: one empty value (not NULL), no numbers
    h.key("Right");
    h.key("Down");
    h.key("m");
    assert!(status_line(&mut h, 220).ends_with("   Not null: 3 │ 3 rows · Col 4"), "{}", status_line(&mut h, 220));
    // Rows 1–3 marked: one range
    h.key("Esc");
    for k in ["Up", "Up", "Up", "m", "Down", "m", "Down", "m"] {
        h.key(k);
    }
    assert!(status_line(&mut h, 220).ends_with("│ Rows 1–3 (3) · Col 4"), "{}", status_line(&mut h, 220));
}

/// ⌃k › Status Bar Items…: a checklist; Space or Enter toggles and saves for the GUI, it stays open on the row; Space types into a
/// filter being typed; Restore defaults; a change the GUI made shows when it opens again
#[tokio::test]
async fn checklist_toggles_and_shares_the_setting() {
    let mut h = Harness::new().await;
    payment(&mut h);
    h.key("Ctrl+K");
    h.type_str("Status Bar Items");
    h.key("Enter");
    assert_eq!(rows(&h)[0], "[ ] Not null selection: cells with a value");
    assert_eq!(rows(&h)[7], "[x] Position grid: Row 3, Col 4 · Rows 5–26 (22)");
    assert_eq!(rows(&h)[8], "Restore defaults Shown columns, Position");
    let gui = Store::new(h.app.state.store.dir());
    h.key(" ");
    assert_eq!(gui.status_bar_items(), [Item::NotNull, Item::ShownColumns, Item::Position], "saved where the GUI reads it");
    assert_eq!(rows(&h)[0], "[x] Not null selection: cells with a value");
    h.key("Down");
    h.key("Enter");
    assert!(rows(&h)[1].starts_with("[x] Sum"));
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("closed") };
    assert_eq!((p.title.as_str(), p.sel), ("Status Bar Items", 1), "stays open on the row");
    // A filter: Space is a character of it
    h.type_str("max");
    h.key(" ");
    assert_eq!(gui.status_bar_items(), [Item::NotNull, Item::Sum, Item::ShownColumns, Item::Position]);
    h.key("Esc");
    assert!(h.app.popup.is_none());
    // The GUI turns Size on: the checklist reads it when it opens
    gui.set_status_bar_item(Item::Size, true).unwrap();
    h.app.run_action(crate::app::Action::StatusBarItems);
    assert!(rows(&h)[5].starts_with("[x] Size"));
    h.key("End");
    h.key("PgDn");
    h.key("Enter");
    assert_eq!(gui.status_bar_items(), DEFAULTS);
    assert_eq!(h.app.status_items, DEFAULTS);
}

/// Narrow: the screen's keys go first, then the global ones but ⌃k actions, then items (the limit, Max … Size, Shown columns),
/// then ⌃k actions; Position stays. The status text keeps a third of the width while there are items
#[tokio::test]
async fn narrow_keeps_position() {
    let mut h = Harness::new().await;
    h.app.status_items = vec![Item::Size, Item::ShownColumns, Item::Position];
    payment(&mut h);
    let target = h.app.tabs[h.app.active].target.clone();
    h.app.table_cols.insert((target, "public".into(), "payment".into()), TableCols::Loaded(vec![("id".into(), ColumnType { data_type: "character varying(8)".into(), charset: None })]));
    let wide = status_line(&mut h, 200);
    assert!(wide.ends_with("⌃k actions   Size: 1 byte │ varchar(8): up to 8 chars (32 bytes in UTF-8) │ Row 1, Col 1"), "{wide}");
    assert!(wide.contains("Enter edit"), "{wide}");
    let mid = status_line(&mut h, 100);
    assert!(mid.ends_with("⌃k actions   Size: 1 byte │ Row 1, Col 1") && !mid.contains("Enter edit") && !mid.contains("varchar"), "{mid}");
    // The narrowest layout drawn: only ⌃k actions (where the items are chosen) and Position are left
    let narrow = status_line(&mut h, 40);
    assert!(narrow.ends_with("… ⌃k actions   Row 1, Col 1") && !narrow.contains("Size"), "{narrow}");
    // A long status text keeps at least a third of the line (cut with …) while there are items, half without them; the rest is
    // the key hints' and the items'
    h.app.status = "x".repeat(200);
    let xs = |l: &str| l.chars().filter(|&c| c == 'x').count();
    let with = status_line(&mut h, 120);
    assert!(with.contains("x… ") && xs(&with) >= 38 && with.ends_with("⌃k actions   Size: 1 byte │ Row 1, Col 1"), "{with}");
    h.app.status_items = vec![];
    let without = status_line(&mut h, 120);
    assert!(xs(&without) >= 58 && !without.contains("Row 1"), "{without}");
}

/// The status text keeps half the line (as without items) unless an item would otherwise go; a run's line is never cut for them
#[tokio::test]
async fn status_text_keeps_half_unless_an_item_would_go() {
    let mut h = Harness::new().await;
    payment(&mut h);
    h.key("m");
    let line = status_line(&mut h, 120);
    assert!(line.contains("2 rows marked") || line.contains("1 row marked"), "{line}");
    assert!(!line.contains('…') && line.ends_with("Row 1, Col 1"), "{line}");
    // Every item on and a cell of 4 bytes: the text gives way to a third so they fit
    h.app.status_items = vec![Item::NotNull, Item::Size, Item::ShownColumns, Item::Position];
    h.app.status = "y".repeat(70);
    let line = status_line(&mut h, 120);
    assert!(line.contains("y… ") && line.ends_with("Size: 1 byte │ Row 1, Col 1"), "{line}");
}

/// Columns that couldn't be read are asked for again after a while
#[tokio::test]
async fn failed_column_types_are_asked_again() {
    let mut h = Harness::new().await;
    h.app.status_items = vec![Item::Size, Item::Position];
    payment(&mut h);
    let key = (h.app.tabs[h.app.active].target.clone(), "public".to_string(), "payment".to_string());
    let long_ago = std::time::Instant::now() - std::time::Duration::from_secs(31);
    h.app.table_cols.insert(key.clone(), TableCols::Failed(std::time::Instant::now()));
    status_line(&mut h, 160);
    assert!(matches!(h.app.table_cols[&key], TableCols::Failed(_)), "not right away");
    h.app.table_cols.insert(key.clone(), TableCols::Failed(long_ago));
    status_line(&mut h, 160);
    assert!(matches!(h.app.table_cols[&key], TableCols::Loading));
}

/// Size: a console result's column mapped to a table column (QueryResult.edit, aliases too) gets the limit; one that isn't, the bytes only
#[tokio::test]
async fn size_in_a_console_result() {
    let mut h = Harness::new().await;
    h.app.status_items = vec![Item::Size, Item::Position];
    let id = h.console_with("select id, user_name as n from users", 0, 0);
    let res = |editable| QueryResult {
        columns: vec![ColumnInfo { name: "id".into(), type_name: "INT8".into() }, ColumnInfo { name: "n".into(), type_name: "VARCHAR".into() }],
        rows: vec![vec![serde_json::json!(1), serde_json::json!("Grüße ☕")]],
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 1,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable,
    };
    let set = |h: &mut Harness, r: QueryResult| {
        let i = h.app.tab_index(id).unwrap();
        let TabKind::Console(c) = &mut h.app.tabs[i].kind else { unreachable!() };
        let mut g = Grid::new(r);
        g.col = 1;
        c.grid = Some(g);
        h.app.focus = Focus::Grid;
    };
    set(&mut h, res(None));
    assert!(status_line(&mut h, 200).ends_with("   Size: 11 bytes (7 chars) │ Row 1, Col 2"), "{}", status_line(&mut h, 200));
    let target = h.app.tabs[h.app.active].target.clone();
    h.app.table_cols.insert((target, "public".into(), "users".into()), TableCols::Loaded(vec![("user_name".into(), ColumnType { data_type: "character varying(8)".into(), charset: None })]));
    set(&mut h, res(Some(Editable::Table(EditTarget { schema: "public".into(), table: "users".into(), pk: vec!["id".into()], columns: vec!["id".into(), "user_name".into()] }))));
    assert!(status_line(&mut h, 200).ends_with("   Size: 11 bytes (7 chars) │ varchar(8): up to 8 chars (32 bytes in UTF-8) │ Row 1, Col 2"), "{}", status_line(&mut h, 200));
    // A number has no Size
    h.app.grid_mut().unwrap().col = 0;
    assert!(status_line(&mut h, 200).ends_with("⌃k actions   Row 1, Col 1"), "{}", status_line(&mut h, 200));
}
