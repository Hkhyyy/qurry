use crate::app::{Popup, TabKind};
use crate::testkit::Harness;
use qurry_core::db::{ColumnInfo, QueryResult};

/// Stands in for the database: SELECT returns two rows, "boom" fails, anything else changes 3 rows
fn fake(q: &str) -> Result<QueryResult, String> {
    if q.contains("boom") {
        return Err("ERROR: relation \"boom\" does not exist".into());
    }
    let select = q.trim_start().to_lowercase().starts_with("select");
    Ok(QueryResult {
        columns: if select { vec![ColumnInfo { name: "id".into(), type_name: "int4".into() }] } else { vec![] },
        rows: if select { vec![vec![1.into()], vec![2.into()]] } else { vec![] },
        rows_affected: if select { 0 } else { 3 },
        truncated: false,
        elapsed_ms: 7,
        has_result_set: select,
        in_transaction: false,
        warning: None,
    })
}

fn console(h: &Harness) -> &crate::app::Console {
    match &h.app.tabs[h.app.active].kind {
        TabKind::Console(c) => c,
        _ => panic!("active tab is not a console"),
    }
}

/// F5 with several statements: pick "All statements" → each is echoed and answered in order; the last result set fills the grid
#[tokio::test]
async fn f5_runs_several_statements_in_order() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.console_with("update t set a = 1 where id = 1;\nselect id from t;\ndelete from t where a = 1;", 0, 3);
    h.key("F5");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no chooser") };
    let labels: Vec<_> = p.items.iter().map(|i| i.label.as_str()).collect();
    // "From caret to end" is left out: from the first statement it is the same as all
    assert_eq!(labels, ["update t set a = 1 where id = 1", "All statements (3)"]);
    h.key("Down");
    h.key("Enter");
    h.settle().await;
    // Statements are echoed as they start; results are logged together when the run ends
    assert_eq!(
        h.console_output(),
        [
            "> update t set a = 1 where id = 1",
            "> select id from t",
            "> delete from t where a = 1",
            "3 row(s) affected in 7 ms",
            "2 row(s) retrieved in 7 ms",
            "3 row(s) affected in 7 ms",
        ]
    );
    let c = console(&h);
    assert!(!c.running);
    assert_eq!(c.result_title, "Result 2: select id from t");
    assert_eq!(c.grid.as_ref().map(|g| g.result.rows.len()), Some(2));
    assert!(!c.show_output, "the result grid is shown, not Output");
    assert_eq!(h.app.status, "3 row(s) affected in 7 ms");
}

/// A failing statement stops the run: later ones don't run, and the ones already committed are named
#[tokio::test]
async fn f5_stops_at_the_first_error() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = crate::exec::Inside::Whole;
    h.console_with("update t set a = 1 where id = 1;\nselect * from boom;\nupdate t set a = 2 where id = 2;", 0, 3);
    h.key("F5");
    assert!(h.app.popup.is_none());
    h.settle().await;
    let out = h.console_output();
    assert_eq!(
        out,
        [
            "> update t set a = 1 where id = 1",
            "> select * from boom",
            "3 row(s) affected in 7 ms",
            "ERROR: relation \"boom\" does not exist",
            "Note: 1 earlier statement(s) of this run were already committed (Auto mode) and were not rolled back",
        ]
    );
    assert!(console(&h).show_output);
}

/// A statement that ran but whose result the Production log couldn't record: a later statement's status doesn't hide its warning
#[tokio::test]
async fn f5_keeps_an_earlier_warning_in_the_status() {
    fn warns(q: &str) -> Result<QueryResult, String> {
        let mut r = fake(q)?;
        if q.starts_with("update") {
            r.warning = Some("It ran, but the result couldn't be written to the Production log".into());
        }
        Ok(r)
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(warns);
    h.app.exec.inside = crate::exec::Inside::Whole;
    h.console_with("update t set a = 1 where id = 1;\nselect id from t;", 0, 3);
    h.key("F5");
    h.settle().await;
    assert!(h.console_output().contains(&"Warning: It ran, but the result couldn't be written to the Production log".to_string()));
    assert_eq!(h.app.status, "2 row(s) retrieved in 7 ms · 1 with a warning (see Output)");
    // One statement: the warning itself
    h.console_with("update t set a = 1 where id = 1", 0, 3);
    h.key("F5");
    h.settle().await;
    assert_eq!(h.app.status, "Warning: It ran, but the result couldn't be written to the Production log");
}

/// Production Data Source: the counted rows of each changing statement are shown before anything runs; yes runs them
#[tokio::test]
async fn production_confirm_shows_counted_rows() {
    // Counting queries answer 1 for id = 4 and 12,408 (as text, as MySQL and some drivers return it) otherwise; the statements themselves go to `fake`
    fn counts(q: &str) -> Result<QueryResult, String> {
        if !q.starts_with("SELECT count(*)") {
            return fake(q);
        }
        let n: serde_json::Value = if q.contains("id = 4") { 1.into() } else { "12408".into() };
        Ok(QueryResult { columns: vec![ColumnInfo { name: "count".into(), type_name: "int8".into() }], rows: vec![vec![n]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None })
    }
    let mut h = Harness::new().await;
    let mut cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
    }))
    .unwrap();
    cfg.options.confirm_changes = true;
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.fake_db = Some(counts);
    h.app.exec.inside = crate::exec::Inside::Whole;
    let sql = "update payment set amount = 0 where id = 4;\ndelete from payment where amount < 0;";
    let id = h.app.restore_console("p", "d".into(), "console".into(), sql).unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    h.key("F5");
    h.settle().await;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert_eq!(
        text,
        "Production data source \"PROD\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n    → 1 row\n  delete from payment where amount < 0\n    → 12,408 rows\n\nRun it?"
    );
    assert!(h.console_output().is_empty(), "nothing runs before the answer");
    h.key("y");
    h.settle().await;
    assert_eq!(
        h.console_output(),
        ["PROD> update payment set amount = 0 where id = 4", "PROD> delete from payment where amount < 0", "3 row(s) affected in 7 ms", "3 row(s) affected in 7 ms"]
    );
}

/// Explain Analyze of a changing statement on PostgreSQL: BEGIN, the EXPLAIN and ROLLBACK all go through the same session (the fake here)
#[tokio::test]
async fn explain_analyze_wraps_changes_in_a_rolled_back_transaction() {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn record(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        fake(q)
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(record);
    h.console_with("delete from t where a = 1", 0, 0);
    h.app.analyze_next = true;
    h.app.run_stmts(vec!["delete from t where a = 1".into()], true, false);
    h.app.analyze_next = false;
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["BEGIN", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) delete from t where a = 1", "ROLLBACK"]);
}

/// ⌃Space lists columns after "alias." (from the loaded metadata); Enter replaces the typed prefix with the picked one
#[tokio::test]
async fn completion_popup_lists_and_inserts() {
    use qurry_core::meta::CompletionTable;
    let mut h = Harness::new().await;
    let sql = "select u. from users u";
    h.console_with(sql, 0, 9);
    let t = |n: &str, cols: &[&str]| CompletionTable { schema: "public".into(), name: n.into(), columns: cols.iter().map(|c| (c.to_string(), "text".to_string())).collect() };
    let target = h.app.tabs[h.app.active].target.clone();
    h.app.completion.insert(target, std::sync::Arc::new(crate::complete::Meta::new(vec![t("users", &["id", "user_name", "email"]), t("orders", &["id", "total"])])));
    h.key("Ctrl+Space");
    let items = |h: &Harness| console(h).complete.as_ref().map(|c| c.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>()).unwrap_or_default();
    assert_eq!(items(&h), ["id", "user_name", "email"]);
    let s = h.screen(120, 30);
    assert!(s.contains("user_name") && s.contains("email"), "{s}");
    // Typing narrows the list (by prefix); Enter picks
    h.type_str("e");
    assert_eq!(items(&h), ["email"]);
    h.key("Enter");
    assert_eq!(h.console_text(), "select u.email from users u");
    assert!(console(&h).complete.is_none());
}

/// A failed Commit goes to Output too, not only to the status bar that cuts it off (a COMMIT on a lost connection says there to check the data)
#[tokio::test]
async fn failed_commit_is_in_output() {
    let mut h = Harness::new().await;
    h.console_tab();
    h.app.tx(crate::app::Action::Commit);
    h.settle().await;
    // No session for this console: commit_tx fails
    assert_eq!(h.console_output(), ["Session closed"]);
    assert!(console(&h).show_output);
    assert_eq!(h.app.status, "Session closed");
}
