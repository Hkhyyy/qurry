//! A console's result tabs follow its statements (#151, app/result_tabs.rs): one per statement in the result panel's title strip, shown
//! when the caret enters its statement, replaced by a rerun, closed with its statement, at most ⌃k › Result Tabs… unpinned
use crate::app::{Action, Popup, TabKind};
use crate::testkit::Harness;
use qurry_core::db::{ColumnInfo, QueryResult};
use qurry_core::exec::Inside;

/// Stands in for the database: `select … limit n` returns n rows (8 without a limit), "boom" fails
fn fake(q: &str) -> Result<QueryResult, String> {
    if q.contains("boom") {
        return Err("ERROR: relation \"boom\" does not exist".into());
    }
    let n = q.split(" limit ").nth(1).and_then(|s| s.trim().trim_end_matches(';').parse().ok()).unwrap_or(8);
    Ok(QueryResult {
        columns: vec![ColumnInfo { name: "id".into(), type_name: "int4".into() }],
        rows: (1..=n).map(|i: i64| vec![i.into()]).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 4,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
    })
}

fn console(h: &Harness) -> &crate::app::Console {
    match &h.app.tabs[h.app.active].kind {
        TabKind::Console(c) => c,
        _ => panic!("active tab is not a console"),
    }
}

/// The strip's labels, and the shown one (None: Output)
fn tabs(h: &Harness) -> (Vec<String>, Option<String>) {
    let r = &console(h).results;
    (r.tabs.iter().map(|t| t.label()).collect(), r.shown_tab().map(|t| t.label()))
}

/// The result panel's title row
fn strip(h: &mut Harness, w: u16) -> String {
    h.screen(w, 24).lines().find(|l| l.contains("Output")).unwrap_or_default().to_string()
}

/// A console with `sql`, ⌃↩ running the statement at the caret
async fn open(sql: &str) -> Harness {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = Inside::Smallest;
    h.console_with(sql, 0, 3);
    h
}

async fn run(h: &mut Harness) {
    h.key("F5");
    h.settle().await;
}

const S2: &str = "select * from customers;\nselect * from orders limit 3;";

#[tokio::test]
async fn one_tab_per_statement_the_caret_shows_it() {
    let mut h = open(S2).await;
    run(&mut h).await;
    h.key("Down");
    run(&mut h).await;
    assert_eq!(tabs(&h), (vec!["1: customers".into(), "2: orders".into()], Some("2: orders".into())));
    assert_eq!(console(&h).grid().unwrap().rows(), 3);
    assert!(strip(&mut h, 120).contains("Output │ 1: customers │ 2: orders — 3 rows"), "{}", strip(&mut h, 120));

    // The caret back in statement 1 shows its tab, without running it
    let out = h.console_output().len();
    h.key("Up");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    assert_eq!(console(&h).grid().unwrap().rows(), 8);
    assert_eq!(h.console_output().len(), out, "nothing ran");
    // Between keys that don't move the caret, the tab picked by hand stays
    h.key("Alt+o");
    assert_eq!(tabs(&h).1, None);
    h.key("F12");
    assert_eq!(tabs(&h).1, None);
    h.key("Alt+o");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
}

#[tokio::test]
async fn a_rerun_after_an_edit_replaces_its_tab() {
    let mut h = open(S2).await;
    run(&mut h).await;
    h.key("Down");
    run(&mut h).await;
    let id = console(&h).results.tabs[0].id;
    h.key("Up");
    h.key("End");
    h.key("Backspace");
    h.type_str(" limit 2;");
    run(&mut h).await;
    let c = console(&h);
    assert_eq!(c.results.tabs.len(), 2);
    assert_eq!((c.results.tabs[0].id, c.results.shown), (id, Some(id)));
    assert_eq!(c.grid().unwrap().rows(), 2);
}

#[tokio::test]
async fn a_pinned_tab_stays_and_a_new_one_follows() {
    let mut h = open(S2).await;
    run(&mut h).await;
    h.app.run_action(Action::PinResult);
    assert_eq!(tabs(&h).0, ["📌 1: customers"]);
    h.key("End");
    h.key("Backspace");
    h.type_str(" limit 1;");
    run(&mut h).await;
    let c = console(&h);
    assert_eq!(tabs(&h), (vec!["📌 1: customers".into(), "1: customers".into()], Some("1: customers".into())));
    assert_eq!((c.results.tabs[0].stmt, c.results.tabs[0].grid.rows()), (None, 8));
    let s = strip(&mut h, 120);
    assert!(s.contains("Output │ 📌") && s.contains("1: customers │ 1: customers — 1 rows"), "{s}");
    // The command acts on the shown tab: pinned too, then unpinned again
    h.app.run_action(Action::PinResult);
    assert_eq!(tabs(&h).0[1], "📌 1: customers");
    h.app.run_action(Action::PinResult);
    assert_eq!(tabs(&h).0[1], "1: customers");
}

#[tokio::test]
async fn a_deleted_statement_closes_its_tab_and_the_next_renumbers() {
    let mut h = open("select * from customers;\nselect * from orders;\nselect * from payments limit 5;").await;
    h.app.exec.inside = Inside::Whole;
    run(&mut h).await;
    assert_eq!(tabs(&h).0, ["1: customers", "2: orders", "3: payments"]);
    h.key("Down");
    h.key("Home");
    h.key("Shift+Down");
    h.key("Backspace");
    assert_eq!(h.console_text(), "select * from customers;\nselect * from payments limit 5;");
    assert_eq!(tabs(&h).0, ["1: customers", "2: payments"]);
}

#[tokio::test]
async fn result_tabs_setting_caps_the_unpinned_tabs() {
    let mut h = open("select * from customers;\nselect * from orders;\nselect * from payments limit 5;").await;
    h.key("Ctrl+K");
    h.type_str("Result Tabs");
    h.key("Enter");
    let body = |h: &Harness| match &h.app.popup {
        Some(Popup::Input { title, body, input, .. }) => (title.clone(), body.clone(), input.lines().join("")),
        _ => panic!("no input popup"),
    };
    assert_eq!(body(&h), ("Result tabs".into(), "Unpinned result tabs kept per console; past it, the least recently shown closes.".into(), "10".into()));
    h.key("Backspace");
    h.key("Backspace");
    h.type_str("0");
    h.key("Enter");
    assert_eq!(body(&h), ("Result tabs".into(), "Enter 1 or more.".into(), "0".into()));
    assert_eq!(h.app.state.store.result_tabs(), 10, "not saved");
    assert!(h.screen(120, 24).contains("Enter 1 or more."));
    h.key("Backspace");
    h.type_str("2");
    h.key("Enter");
    assert!(h.app.popup.is_none());
    assert_eq!((h.app.result_tabs, h.app.state.store.result_tabs()), (2, 2));
    h.app.exec.inside = Inside::Whole;
    run(&mut h).await;
    assert_eq!(tabs(&h), (vec!["2: orders".into(), "3: payments".into()], Some("3: payments".into())));
}

#[tokio::test]
async fn an_error_shows_output_and_the_tab_keeps_its_result() {
    let mut h = open(S2).await;
    run(&mut h).await;
    h.key("End");
    h.key("Backspace");
    h.type_str(" join boom;");
    run(&mut h).await;
    let c = console(&h);
    assert_eq!(c.results.shown, None, "Output");
    assert_eq!((tabs(&h).0, c.results.tabs[0].grid.rows()), (vec!["1: customers".to_string()], 8));
    assert!(h.console_output().iter().any(|l| l.contains("boom")));
}

#[tokio::test]
async fn alt_period_and_comma_cycle_through_output_and_the_tabs() {
    let mut h = open(S2).await;
    h.app.exec.inside = Inside::Whole;
    run(&mut h).await;
    assert_eq!(tabs(&h).1.as_deref(), Some("2: orders"));
    h.key("Alt+.");
    assert_eq!(tabs(&h).1, None);
    h.key("Alt+.");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    h.key("Alt+,");
    h.key("Alt+,");
    assert_eq!(tabs(&h).1.as_deref(), Some("2: orders"));
    // ⌥o still toggles Output and back
    h.key("Alt+o");
    assert_eq!(tabs(&h).1, None);
    h.key("Alt+o");
    assert_eq!(tabs(&h).1.as_deref(), Some("2: orders"));
}

#[tokio::test]
async fn a_click_on_a_label_shows_it() {
    let mut h = open(S2).await;
    h.app.exec.inside = Inside::Whole;
    run(&mut h).await;
    let buf = h.buffer(120, 24);
    let row = (0..24).find(|&y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>().contains("Output │")).unwrap();
    let line: Vec<String> = (0..120).map(|x| buf[(x, row)].symbol().to_string()).collect();
    let at = |s: &str| (0..120).find(|&x| line[x as usize..].concat().starts_with(s)).unwrap();
    h.mouse("down", at("1: customers") + 2, row);
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    h.mouse("down", at("Output"), row);
    assert_eq!(tabs(&h).1, None);
}

#[tokio::test]
async fn twelve_tabs_in_80_columns_fold_and_keep_the_shown_one() {
    let sql: Vec<String> = (1..=12).map(|i| format!("select * from table_{i} limit {i};")).collect();
    let mut h = open(&sql.join("\n")).await;
    h.app.exec.inside = Inside::Whole;
    h.app.result_tabs = 12;
    run(&mut h).await;
    assert_eq!(tabs(&h).0.len(), 12);
    let s = strip(&mut h, 80);
    assert!(s.contains("Output │ ‹"), "{s}");
    assert!(s.contains("12: table_12 — 12 rows"), "{s}");
    // Shown on the left: the tabs after it fold instead
    h.key("Alt+.");
    h.key("Alt+.");
    let s = strip(&mut h, 80);
    assert!(s.contains("1: table_1"), "{s}");
}

#[tokio::test]
async fn a_caret_move_keeps_a_grid_with_pending_edits_shown() {
    let mut h = open(S2).await;
    h.app.exec.inside = Inside::Whole;
    run(&mut h).await;
    h.app.run_action(Action::ResultNext);
    h.app.run_action(Action::ResultNext);
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.grid_mut().unwrap().set_value(0, 0, "9".into());
    }
    h.key("Down");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"), "the edit isn't dropped");
}

#[tokio::test]
async fn moving_within_a_statement_keeps_the_tab_picked_entering_another_shows_its_own() {
    let mut h = open(S2).await;
    run(&mut h).await;
    h.key("Down");
    run(&mut h).await;
    h.key("Alt+,");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    // Still in statement 2: the pick stays
    h.key("Left");
    h.key("End");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    h.key("Up");
    h.key("Alt+o");
    h.key("Left");
    assert_eq!(tabs(&h).1, None, "Output picked in statement 1 stays");
    h.key("Down");
    assert_eq!(tabs(&h).1.as_deref(), Some("2: orders"));
}

#[tokio::test]
async fn fixing_a_failed_statement_keeps_output_shown() {
    let mut h = open(S2).await;
    run(&mut h).await;
    h.key("End");
    h.key("Backspace");
    h.type_str(" join boom;");
    run(&mut h).await;
    assert_eq!(tabs(&h).1, None);
    for k in ["Left", "Left", "Backspace", "Home"] {
        h.key(k);
    }
    h.type_str("x");
    assert_eq!(tabs(&h).1, None, "Output stays while the statement is fixed");
}

#[tokio::test]
async fn edits_in_a_result_tab_not_shown_are_asked_about_before_a_run() {
    let mut h = open(S2).await;
    h.app.exec.inside = Inside::Whole;
    run(&mut h).await;
    h.key("Alt+.");
    h.key("Alt+.");
    assert_eq!(tabs(&h).1.as_deref(), Some("1: customers"));
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.grid_mut().unwrap().set_value(0, 0, "9".into());
    }
    h.key("Alt+.");
    assert_eq!(tabs(&h).1.as_deref(), Some("2: orders"));
    h.key("F5");
    assert!(matches!(h.app.popup, Some(Popup::Confirm { .. })), "the edit in tab 1 is asked about");
    h.key("Enter");
    h.settle().await;
    assert_eq!(console(&h).results.tabs.iter().map(|t| t.grid.pending()).sum::<usize>(), 0);
}
