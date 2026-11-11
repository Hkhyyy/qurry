use crate::app::{Popup, TabKind};
use crate::testkit::Harness;
use qurry_core::db::{ColumnInfo, QueryResult};
use serde_json::Value;

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
        editable: None,
    })
}

fn console(h: &Harness) -> &crate::app::Console {
    match &h.app.tabs[h.app.active].kind {
        TabKind::Console(c) => c,
        _ => panic!("active tab is not a console"),
    }
}

fn console_mut(h: &mut Harness) -> &mut crate::app::Console {
    let i = h.app.active;
    match &mut h.app.tabs[i].kind {
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
    assert_eq!(c.results.shown_tab().map(|t| t.label()), Some("2: t".into()));
    assert_eq!(c.grid().map(|g| g.result.rows.len()), Some(2));
    assert_eq!(h.app.status, "3 row(s) affected in 7 ms");
}

/// A failing statement stops the run: later ones don't run, and the ones already committed are named
#[tokio::test]
async fn f5_stops_at_the_first_error() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
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
    assert!(console(&h).results.shown.is_none(), "Output is shown");
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
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
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
        Ok(QueryResult { columns: vec![ColumnInfo { name: "count".into(), type_name: "int8".into() }], rows: vec![vec![n]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable: None })
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
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
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
    // It came up on its own: a y counts after a pause
    let Some(Popup::Confirm { opened: Some(t), .. }) = &mut h.app.popup else { panic!("not a quiet confirm") };
    *t -= crate::app::App::TLS_ASK_GRACE;
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
    h.app.run_stmts(vec!["delete from t where a = 1".into()], vec![], true, false);
    h.app.analyze_next = false;
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["BEGIN", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) delete from t where a = 1", "ROLLBACK"]);
}

/// Explain Analyze of a write a rollback can't undo (nextval) asks first, and afterwards doesn't say nothing was changed
#[tokio::test]
async fn explain_analyze_asks_before_what_a_rollback_cant_undo() {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn record(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        fake(q)
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(record);
    let sql = "select nextval('s')";
    h.console_with(sql, 0, 0);
    h.app.analyze_next = true;
    h.app.run_stmts(vec![sql.into()], vec![], true, false);
    h.app.analyze_next = false;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.contains("a rollback can't undo what it does outside the transaction: sequence values"), "{text}");
    h.settle().await;
    assert!(SEEN.lock().unwrap().is_empty(), "nothing runs before the answer");
    h.key("y");
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["BEGIN", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select nextval('s')", "ROLLBACK"]);
    let out = h.console_output();
    assert!(out.iter().any(|l| l.contains("rolled it back, but what it did outside the transaction stays")), "{out:?}");
    assert!(!out.iter().any(|l| l.contains("nothing was changed")), "{out:?}");
    // It ran: a warning (drawn in the warning colour, as the GUI does), not an error
    let TabKind::Console(c) = &h.app.tabs[h.app.active].kind else { panic!() };
    let note = c.output.iter().find(|l| l.1.contains("what it did outside the transaction stays")).unwrap();
    assert!(!note.0 && note.1.starts_with(crate::app::WARNING), "{note:?}");
}

/// Explain Analyze of several statements that need a look for different reasons names each with its own reason (as the GUI does)
#[tokio::test]
async fn explain_analyze_lists_each_statement_with_its_reason() {
    let mut h = Harness::new().await;
    h.console_with("select nextval('s'); delete from audit", 0, 0);
    h.app.analyze_next = true;
    h.app.run_stmts(vec!["select nextval('s')".into(), "delete from audit".into()], vec![], true, false);
    h.app.analyze_next = false;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.starts_with("Explain Analyze really runs the statements, and 2 of them need a look:"), "{text}");
    assert!(text.contains("  select nextval('s')\n    a rollback can't undo what it does outside the transaction"), "{text}");
    assert!(text.contains("  delete from audit\n    DELETE without WHERE changes every row, holding their locks"), "{text}");
    // Drawn with the danger border, as the ask for one statement is
    assert_eq!(confirm_border(&mut h), crate::kit::DANGER);
}

/// The colour of the Confirm popup's top border, just left of its title
fn confirm_border(h: &mut Harness) -> ratatui::style::Color {
    let (w, ht) = (120u16, 40u16);
    let buf = h.buffer(w, ht);
    let title: Vec<&str> = " Confirm ".split("").filter(|c| !c.is_empty()).collect();
    let at = (0..ht).flat_map(|y| (1..w - title.len() as u16).map(move |x| (x, y))).find(|&(x, y)| title.iter().enumerate().all(|(i, c)| buf[(x + i as u16, y)].symbol() == *c));
    let (x, y) = at.expect("no Confirm popup drawn");
    buf[(x - 1, y)].fg
}

/// Explain Analyze of a SELECT runs in a rolled-back transaction too (a function it calls may write), without asking (a function's
/// name says nothing of what it does) and saying nothing more
#[tokio::test]
async fn explain_analyze_wraps_reads() {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn record(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        fake(q)
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(record);
    let sql = "select archive_old_orders()";
    h.console_with(sql, 0, 0);
    h.app.analyze_next = true;
    h.app.run_stmts(vec![sql.into()], vec![], true, false);
    h.app.analyze_next = false;
    assert!(h.app.popup.is_none(), "a read that calls a user-defined function doesn't ask");
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["BEGIN", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select archive_old_orders()", "ROLLBACK"]);
    SEEN.lock().unwrap().clear();
    let before = h.console_output().len();
    h.app.analyze_next = true;
    h.app.run_stmts(vec!["select count(*) from t".into()], vec![], true, false);
    h.app.analyze_next = false;
    assert!(h.app.popup.is_none(), "a plain read doesn't ask");
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["BEGIN", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select count(*) from t", "ROLLBACK"]);
    let out = h.console_output();
    assert!(!out[before..].iter().any(|l| l.contains("rolled it back")), "{out:?}");
}

/// Explain Analyze of a SELECT inside an open transaction on a read-only Data Source: the savepoint it wraps the statement in
/// passes the read-only check (the fake refuses what core's check_read_only refuses), so the plan runs
#[tokio::test]
async fn explain_analyze_in_a_transaction_on_a_read_only_source_runs() {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn read_only(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        match qurry_core::guard::read_only_violation(q, qurry_core::store::DbKind::Postgres, false) {
            Some(why) => Err(why),
            None => fake(q).map(|r| QueryResult { in_transaction: true, ..r }),
        }
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(read_only);
    let sql = "select * from orders";
    h.console_with(sql, 0, 0);
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.in_tx = true;
    }
    h.app.analyze_next = true;
    h.app.run_stmts(vec![sql.into()], vec![], true, false);
    h.app.analyze_next = false;
    assert!(h.app.popup.is_none(), "a plain read doesn't ask");
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["SAVEPOINT qurry_explain", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select * from orders", "ROLLBACK TO SAVEPOINT qurry_explain"]);
    let out = h.console_output();
    assert!(!out.iter().any(|l| l.contains("Read-only mode")), "{out:?}");
    let TabKind::Console(c) = &h.app.tabs[h.app.active].kind else { panic!() };
    assert!(c.in_tx, "still in the transaction the user opened");
}

/// A console on a source of `kind` (saved, so its kind is known) with `sql`, its caret at the start
async fn kind_console(kind: &str, sql: &str) -> Harness {
    let mut h = Harness::new().await;
    // In the test's folder: a file DB's path is opened (an empty file is made) when the console loads its metadata
    let db = h.app.import_home.with_file_name("d.db");
    let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": "k", "name": "K", "kind": kind, "host": "127.0.0.1", "port": 1, "user": "u", "database": db
    }))
    .unwrap();
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    let id = h.app.restore_console("k", "d".into(), "console".into(), sql).unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    h
}

/// Explain sends the kind's statement (qurry_shared::explain, the GUI's and MCP's), without the trailing ;
#[tokio::test]
async fn explain_sends_the_kinds_statement() {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    // One row with PostgreSQL's JSON plan, whatever the kind
    fn record(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        let plan = r#"[{"Plan": {"Node Type": "Seq Scan", "Relation Name": "t", "Total Cost": 1.0, "Plan Rows": 1, "Plan Width": 4}}]"#;
        Ok(QueryResult { columns: vec![ColumnInfo { name: "QUERY PLAN".into(), type_name: "json".into() }], rows: vec![vec![plan.into()]], has_result_set: true, ..fake("x")? })
    }
    for (kind, want, column) in [
        ("postgres", "EXPLAIN (FORMAT JSON) select * from t", "operation"),
        ("mysql", "EXPLAIN select * from t", "QUERY PLAN"),
        ("sqlite", "EXPLAIN QUERY PLAN select * from t", "QUERY PLAN"),
    ] {
        let mut h = kind_console(kind, "select * from t;").await;
        h.app.fake_db = Some(record);
        SEEN.lock().unwrap().clear();
        h.app.run_action(crate::app::Action::Explain);
        h.settle().await;
        assert_eq!(*SEEN.lock().unwrap(), [want], "{kind}");
        // Only PostgreSQL's JSON becomes the plan tree; the others' rows are shown as they came
        let TabKind::Console(c) = &h.app.tabs[h.app.active].kind else { panic!() };
        assert_eq!(c.results.tabs[0].grid.result.columns[0].name, column, "{kind}");
    }
}

/// Redis has no Explain: Output, the status line and the statement's mark say so, and nothing is sent (it used to run as is).
/// Output comes in front of an earlier run's result, so the reason is what shows
#[tokio::test]
async fn explain_on_redis_says_it_isnt_available_and_sends_nothing() {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn record(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        Ok(QueryResult { columns: vec![ColumnInfo { name: "value".into(), type_name: "string".into() }], rows: vec![vec!["v".into()]], has_result_set: true, ..fake(q)? })
    }
    let mut h = kind_console("redis", "GET k").await;
    h.app.fake_db = Some(record);
    h.app.execute(false);
    h.settle().await;
    assert!(console(&h).results.shown.is_some(), "the run's result is shown");
    SEEN.lock().unwrap().clear();
    let before = h.console_output().len();
    h.app.run_action(crate::app::Action::Explain);
    h.settle().await;
    assert!(SEEN.lock().unwrap().is_empty(), "{:?}", SEEN.lock().unwrap());
    let msg = "Explain isn't available for Redis: run the statement without Explain";
    assert_eq!(h.console_output()[before..], ["K> GET k", msg]);
    assert!(console(&h).results.shown.is_none(), "Output with the reason is in front");
    assert_eq!(h.app.status, msg);
    assert_eq!(console(&h).marks.at(0), Some(crate::app::MarkState::Err));
}

/// ⌃Space lists columns after "alias." (from the loaded metadata); Enter replaces the typed prefix with the picked one
#[tokio::test]
async fn completion_popup_lists_and_inserts() {
    use qurry_core::meta::CompletionTable;
    let mut h = Harness::new().await;
    let sql = "select u. from users u";
    h.console_with(sql, 0, 9);
    let t = |n: &str, cols: &[&str]| CompletionTable { schema: "public".into(), name: n.into(), columns: cols.iter().map(|c| (c.to_string(), "text".to_string())).collect(), ..Default::default() };
    let target = h.app.tabs[h.app.active].target.clone();
    h.app.completion.insert(target, std::sync::Arc::new(qurry_core::complete::Meta::new(vec![t("users", &["id", "user_name", "email"]), t("orders", &["id", "total"])])));
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
    assert!(console(&h).results.shown.is_none(), "Output is shown");
    assert_eq!(h.app.status, "Session closed");
    // And a notice to acknowledge, as in the GUI
    let Some(crate::app::Popup::Message { title, text }) = &h.app.popup else { panic!("no notice") };
    assert_eq!((title.as_str(), text.as_str()), ("Commit failed", "Session closed"));
    // A failed Roll back doesn't get one
    h.key("Esc");
    h.app.tx(crate::app::Action::Rollback);
    h.settle().await;
    assert!(h.app.popup.is_none());
    // A popup opened while the COMMIT was running (a confirmation here) stays: the notice goes over it, and Esc goes back to it
    h.app.tx(crate::app::Action::Commit);
    h.app.popup = Some(crate::app::Popup::Confirm { opened: None, text: "Roll back and close the tab?".into(), on_yes: Box::new(|_| {}) });
    h.settle().await;
    let Some(crate::app::Popup::Note { title, back, .. }) = &h.app.popup else { panic!("no notice over the confirmation") };
    assert_eq!(title, "Commit failed");
    assert!(matches!(**back, crate::app::Popup::Confirm { .. }));
    h.key("Esc");
    assert!(matches!(h.app.popup, Some(crate::app::Popup::Confirm { .. })));
}

/// Output shows its newest line on the bottom row even when long lines wrap onto several rows (#92)
#[tokio::test]
async fn output_shows_the_last_wrapped_line() {
    let mut h = Harness::new().await;
    h.console_tab();
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        for i in 0..20 {
            c.log(false, format!("[DEV] shop> update orders set note = '{}' where id = {i}", "x".repeat(150)));
        }
        c.log(true, format!("ERROR: {} newest-line-end", "y".repeat(150)));
    }
    let s = h.screen(80, 20);
    let rows: Vec<&str> = s.lines().collect();
    let last = rows.iter().position(|r| r.contains("newest-line-end")).unwrap_or_else(|| panic!("newest line cut off:\n{s}"));
    assert!(rows[last + 1].contains('└') || rows[last + 1].contains('╰'), "newest line is not on the bottom row:\n{s}");
    assert!(rows.iter().any(|r| r.contains("│ERROR:")), "its start shows too:\n{s}");
}

/// `select … from users` comes back editable (the backend's edit target, with an alias), a join with its reason, anything else unchecked
fn users(q: &str) -> Result<QueryResult, String> {
    use qurry_core::editable::{EditTarget, Editable};
    let col = |n: &str| ColumnInfo { name: n.into(), type_name: "text".into() };
    if q.contains("voc") {
        return voc_join(q);
    }
    let editable = if q.contains("join") {
        Some(Editable::ReadOnly("Read-only: the columns come from more than one table.".into()))
    } else if q.contains("users") {
        Some(Editable::Table(EditTarget { schema: "public".into(), table: "users".into(), pk: vec!["id".into()], columns: vec!["id".into(), "user_name".into()] }))
    } else {
        None
    };
    Ok(QueryResult { columns: vec![col("id"), col("name")], rows: vec![vec!["1".into(), "kim".into()], vec!["2".into(), "lee".into()]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable })
}

/// #102's join, as the backend reports it: voc (idx, status) left join users (idx, role, and a computed column); voc 2 has no user
fn voc_join(_: &str) -> Result<QueryResult, String> {
    use qurry_core::editable::{ColumnEdit, Editable, JoinEdit, JoinTable};
    let col = |n: &str| ColumnInfo { name: n.into(), type_name: "text".into() };
    let jt = |s: &str, t: &str, at: usize| JoinTable { schema: s.into(), table: t.into(), pk: vec!["idx".into()], pk_at: vec![at] };
    let to = |t: usize, c: &str| ColumnEdit::To { table: t, column: c.into() };
    let j = JoinEdit {
        tables: vec![jt("crm", "voc", 0), jt("public", "users", 2)],
        columns: vec![to(0, "idx"), to(0, "status"), to(1, "idx"), to(1, "role"), ColumnEdit::ReadOnly { read_only: qurry_shared::errors::edit_computed("shout") }],
    };
    let rows = vec![vec!["1".into(), "open".into(), "10".into(), "member".into(), "MEMBER".into()], vec!["2".into(), "new".into(), Value::Null, Value::Null, Value::Null]];
    Ok(QueryResult { columns: ["idx", "status", "idx", "role", "shout"].map(col).to_vec(), rows, rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable: Some(Editable::Join(j)) })
}

/// A Production console whose result is shown, with the grid focused
async fn prod_console(sql: &str) -> Harness {
    let mut h = Harness::new().await;
    let mut cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
    }))
    .unwrap();
    cfg.options.confirm_changes = true;
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.fake_db = Some(users);
    let id = h.app.restore_console("p", "d".into(), "console".into(), sql).unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    h.key("F5");
    h.settle().await;
    h.app.focus = crate::app::Focus::Grid;
    h
}

/// A console result from one table with its key edits as a table tab does: ↩ edits a cell, Submit builds the SQL with the table's
/// column names (the result has an alias) and asks first on Production. Production consoles start in Manual (#106): the edits
/// run in its transaction, and the question says they stay pending
#[tokio::test]
async fn console_result_edits_and_submits_like_a_table() {
    let mut h = prod_console("select id, user_name as name from users").await;
    assert!(console(&h).tx_manual, "Production starts in Manual");
    assert!(console(&h).grid().is_some(), "{}", h.app.status);
    h.key("Down");
    h.key("Right");
    h.key("Enter");
    for _ in 0..3 {
        h.key("Backspace");
    }
    h.type_str("LEE");
    h.key("Enter");
    assert_eq!(console(&h).grid().unwrap().pending(), 1);
    h.key("a");
    assert_eq!(console(&h).grid().unwrap().pending(), 2);
    h.key("Ctrl+S");
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no Production confirm: {}", h.app.status) };
    let sql = "\n\n  UPDATE public.users SET user_name = 'LEE' WHERE id = '2'\n  INSERT INTO public.users DEFAULT VALUES";
    assert_eq!(text, &format!("Production data source \"PROD\" — run 2 change(s) in this console's transaction? They stay pending until you Commit or Roll back.{sql}"));
    // In Auto mode (the option off, or Tx: Auto) they are saved at once
    h.app.popup = None;
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.tx_manual = false;
    }
    h.key("Ctrl+S");
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no Production confirm: {}", h.app.status) };
    assert_eq!(text, &format!("Production data source \"PROD\" — save 2 change(s)?{sql}"));
}

/// A join result (#102) edits per table: each cell by its table's key, the outer side where no row matched and a computed column
/// read-only with the reason, no rows added or deleted; Submit builds one UPDATE per row and table
#[tokio::test]
async fn console_join_result_edits_per_table() {
    use qurry_shared::errors as e;
    let mut h = prod_console("select vo.idx, vo.status, pu.idx, pu.role, upper(pu.role) as shout from crm.voc vo left join users pu on pu.idx = vo.user_idx").await;
    let edit = |h: &mut Harness, row: usize, col: usize, v: &str| {
        let g = console_mut(h).grid_mut().unwrap();
        (g.row, g.col) = (row, col);
        h.key("Enter");
        if h.app.popup.is_some() {
            h.type_str(v);
            h.key("Enter");
        }
    };
    edit(&mut h, 0, 1, "-done");
    edit(&mut h, 0, 3, "-x");
    assert_eq!(console(&h).grid().unwrap().pending(), 2);
    // users' side of voc 2: no row matched
    edit(&mut h, 1, 3, "x");
    assert_eq!(h.app.status, e::edit_no_row("public.users"));
    edit(&mut h, 0, 4, "x");
    assert_eq!(h.app.status, e::edit_computed("shout"));
    h.key("a");
    assert_eq!(h.app.status, e::EDIT_JOIN_ROWS);
    h.key("d");
    assert_eq!(h.app.status, e::EDIT_JOIN_ROWS);
    assert_eq!(console(&h).grid().unwrap().pending(), 2);
    // A paste reaching a read-only cell writes nothing
    let g = console_mut(&mut h).grid_mut().unwrap();
    (g.row, g.col) = (0, 3);
    h.app.paste_grid("a\tb");
    assert_eq!(h.app.status, e::edit_computed("shout"));
    assert_eq!(console(&h).grid().unwrap().pending(), 2);
    h.key("Ctrl+S");
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no Production confirm: {}", h.app.status) };
    assert!(text.ends_with("\n\n  UPDATE crm.voc SET status = 'open-done' WHERE idx = '1'\n  UPDATE public.users SET role = 'member-x' WHERE idx = '10'"), "{text}");
}

/// A new console of a Production data source starts in Manual and says so once, with where to turn it off (#106); with the
/// option off it starts in the source's Transaction control mode
#[tokio::test]
async fn production_console_starts_in_manual_and_says_so_once() {
    let mut h = Harness::new().await;
    let mut cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
    }))
    .unwrap();
    cfg.options.confirm_changes = true;
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let told = |h: &Harness| console(h).output.iter().any(|(_, l)| l == qurry_shared::errors::PRODUCTION_MANUAL);
    h.app.open_console("p", None);
    assert!(console(&h).tx_manual && told(&h));
    assert_eq!(h.app.status, qurry_shared::errors::PRODUCTION_MANUAL);
    h.app.open_console("p", None);
    assert!(console(&h).tx_manual && !told(&h), "once");
    cfg.options.production_manual = Some(false);
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.open_console("p", None);
    assert!(!console(&h).tx_manual);
    // The reminder about a transaction left open goes into that console and the status line
    let session = console(&h).session.clone();
    h.app.tx_reminded(vec![(session, "open for 5 min".into())]);
    assert_eq!(console(&h).output.last().map(|l| l.1.as_str()), Some("open for 5 min"));
    assert_eq!(h.app.status, "open for 5 min");
}

/// Edit keys on a console result that can't be edited say why in the status line: the backend's reason, the generic text when it
/// didn't check. In Manual mode it can be edited (Submit runs in the transaction)
#[tokio::test]
async fn console_result_read_only_says_why() {
    let mut h = prod_console("select u.id, u.name from users u join orders o on o.uid = u.id").await;
    h.key("Enter");
    assert_eq!(h.app.status, "Read-only: the columns come from more than one table.");
    assert!(h.app.popup.is_none());
    let mut h = prod_console("select 1 as id, 'x' as name").await;
    h.key("a");
    assert_eq!(h.app.status, qurry_shared::errors::CONSOLE_READ_ONLY);
    let mut h = prod_console("select id, user_name as name from users").await;
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.tx_manual = true;
    }
    h.key("a");
    assert_eq!(console(&h).grid().unwrap().pending(), 1);
}

/// Running again with pending edits asks first: no keeps them, yes drops them and runs
#[tokio::test]
async fn console_rerun_with_pending_edits_asks_first() {
    let mut h = prod_console("select id, user_name as name from users").await;
    h.key("a");
    h.app.focus = crate::app::Focus::Editor;
    h.key("F5");
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert_eq!(text, "1 pending change(s) are not saved to the database (⌃s submits them). Discard them and run the statement?");
    h.key("n");
    assert_eq!(console(&h).grid().unwrap().pending(), 1);
    let before = h.console_output().len();
    h.key("F5");
    h.key("y");
    h.settle().await;
    assert!(h.console_output().len() > before, "it ran");
    assert_eq!(console(&h).grid().unwrap().pending(), 0);
}

/// Metadata with a foreign key (orders.user_id → users.id) and a user function, for the console's target
fn assist_meta(h: &mut Harness) {
    use qurry_core::complete::{ForeignKey, Routine};
    use qurry_core::meta::CompletionTable;
    let t = |n: &str, cols: &[&str]| CompletionTable { schema: "public".into(), name: n.into(), columns: cols.iter().map(|c| (c.to_string(), "text".to_string())).collect(), ..Default::default() };
    let mut orders = t("orders", &["id", "user_id", "status"]);
    orders.fks = vec![ForeignKey { columns: vec!["user_id".into()], ref_schema: "public".into(), ref_table: "users".into(), ref_columns: vec!["id".into()] }];
    let f = CompletionTable { schema: "public".into(), name: "order_total".into(), routine: Some(Routine { args: "order_id bigint, with_tax boolean".into(), returns: "numeric".into() }), ..Default::default() };
    let target = h.app.tabs[h.app.active].target.clone();
    h.app.completion.insert(target, std::sync::Arc::new(qurry_core::complete::Meta::new(vec![t("users", &["id", "user_name", "email"]), orders, f])));
}

fn items(h: &Harness) -> Vec<String> {
    console(h).complete.as_ref().map(|c| c.items.iter().map(|i| i.label.clone()).collect()).unwrap_or_default()
}

/// After `join <table>`, ⌃Space offers the whole ON condition from the foreign key; after `join`, the related table comes first
#[tokio::test]
async fn completion_offers_fk_join_conditions() {
    let mut h = Harness::new().await;
    h.console_with("select * from orders o join users u ", 0, 36);
    assist_meta(&mut h);
    h.key("Ctrl+Space");
    assert_eq!(items(&h), ["on o.user_id = u.id"]);
    let s = h.screen(120, 30);
    assert!(s.contains("⋈ on o.user_id = u.id") && s.contains("fk orders → users"), "{s}");
    h.key("Enter");
    assert_eq!(h.console_text(), "select * from orders o join users u on o.user_id = u.id");
    h.console_with("select * from orders o join ", 0, 28);
    assist_meta(&mut h);
    h.key("Ctrl+Space");
    assert_eq!(items(&h)[0], "users", "the table with a foreign key to orders first");
}

/// Templates at a statement start: `sel` writes `select * from ` with the caret at its end; `upd` selects its first placeholder,
/// so typing replaces it. Statement keywords come first at a statement start, not the previous statement's columns
#[tokio::test]
async fn templates_expand_and_select_their_placeholder() {
    let mut h = Harness::new().await;
    h.console_with("select * from orders", 0, 20);
    assist_meta(&mut h);
    h.key("Enter");
    h.type_str("se");
    assert_eq!(items(&h), ["select", "set", "sel"], "statement keywords first, no columns of orders");
    h.key("Down");
    h.key("Down");
    h.key("Enter");
    assert_eq!(h.console_text(), "select * from orders\nselect * from ");
    assert_eq!(console(&h).editor.cursor(), (1, 14));
    h.console_with("", 0, 0);
    assist_meta(&mut h);
    h.type_str("upd");
    assert_eq!(items(&h), ["update", "upd"]);
    h.key("Down");
    h.key("Enter");
    h.type_str("users");
    assert_eq!(h.console_text(), "update users set column = value where condition");
}

/// ⌃Space right after `*` lists the column list it stands for; picking it writes the columns
#[tokio::test]
async fn star_expands_into_columns_on_request() {
    let mut h = Harness::new().await;
    h.console_with("select * from users", 0, 8);
    assist_meta(&mut h);
    h.key("Ctrl+Space");
    assert_eq!(items(&h), ["expand *"]);
    h.key("Enter");
    assert_eq!(h.console_text(), "select id, user_name, email from users");
}

/// Inside a call's parentheses the hint line above the caret shows its signature, the current parameter highlighted
#[tokio::test]
async fn hint_line_shows_the_signature_inside_a_call() {
    let mut h = Harness::new().await;
    h.console_with("\nselect order_total(", 1, 19);
    assist_meta(&mut h);
    h.app.focus = crate::app::Focus::Editor;
    h.type_str("1, ");
    let sig = console(&h).assist.sig.clone().expect("a signature");
    assert_eq!(sig.active, 1);
    let buf = h.buffer(120, 30);
    let row = |y: u16| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let y = (0..30).find(|&y| row(y).contains("order_total(order_id bigint, with_tax boolean) → numeric")).expect("hint line");
    let x = row(y).find("with_tax").map(|b| row(y)[..b].chars().count() as u16).unwrap();
    assert_eq!(buf[(x, y)].fg, crate::kit::ACCENT, "the parameter at the caret is highlighted");
    h.type_str("true)");
    assert!(console(&h).assist.sig.is_none(), "past the closing parenthesis");
}

/// An unknown column is underlined; ⌥↩ on it lists the closest names and picking one replaces it. ⌥↩ anywhere else still runs
#[tokio::test]
async fn unknown_names_are_underlined_and_alt_enter_fixes_them() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.console_with("select nme from users", 0, 21);
    assist_meta(&mut h);
    h.app.focus = crate::app::Focus::Editor;
    h.key("Left");
    assert_eq!(console(&h).assist.names.iter().map(|m| (m.row, m.from, m.to, m.fixes.clone())).collect::<Vec<_>>(), [(0, 7, 10, vec!["user_name".to_string()])]);
    let buf = h.buffer(120, 30);
    let row = |y: u16| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let y = (0..30).find(|&y| row(y).contains("select nme from users")).unwrap();
    let x = row(y).find("nme").map(|b| row(y)[..b].chars().count() as u16).unwrap();
    let under = |x: u16| buf[(x, y)].modifier.contains(ratatui::style::Modifier::UNDERLINED);
    assert!(under(x) && under(x + 2) && !under(x + 3) && !under(x - 1));
    // The caret on it: the hint line says why and how to fix it
    h.console_with("select nme from users", 0, 9);
    assist_meta(&mut h);
    h.key("Right");
    let s = h.screen(120, 30);
    assert!(s.contains("Unknown column \"nme\" · ⌥Enter user_name"), "{s}");
    h.key("Alt+Enter");
    assert_eq!(items(&h), ["user_name"]);
    h.key("Enter");
    assert_eq!(h.console_text(), "select user_name from users");
    assert!(console(&h).assist.names.is_empty());
    h.key("Alt+Enter");
    h.settle().await;
    assert!(h.console_output().iter().any(|l| l.ends_with("> select user_name from users")), "{:?}", h.console_output());
}

/// A paste is marked at once, and ⌥↩ reads the marks of the text as it is now: SQL that moved the name without a key (inserted
/// by a command) still gets its name fixed, not the text at its old place
#[tokio::test]
async fn pasted_and_moved_names_are_marked_where_they_are() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.console_with("", 0, 0);
    assist_meta(&mut h);
    h.app.focus = crate::app::Focus::Editor;
    h.paste("select nme from users");
    assert_eq!(console(&h).assist.names.iter().map(|m| (m.row, m.from, m.to)).collect::<Vec<_>>(), [(0, 7, 10)]);
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.editor.move_cursor(tui_textarea::CursorMove::Jump(0, 0));
        c.editor.insert_str("\n");
        c.editor.move_cursor(tui_textarea::CursorMove::Jump(1, 9));
    }
    h.key("Alt+Enter");
    assert_eq!(items(&h), ["user_name"]);
    h.key("Enter");
    assert_eq!(h.console_text(), "\nselect user_name from users");
    // Valid SQL the metadata has no names for (FETCH, a named window): no mark, so ⌥↩ at it runs the statement
    h.console_with("select id from users fetch next 5 rows only", 0, 27);
    assist_meta(&mut h);
    h.key("Right");
    assert!(console(&h).assist.names.is_empty(), "{:?}", console(&h).assist.names);
    h.key("Alt+Enter");
    h.settle().await;
    assert!(h.console_output().iter().any(|l| l.ends_with("> select id from users fetch next 5 rows only")), "{:?}", h.console_output());
}

/// No marks for what the metadata can't tell: a CTE, an alias, a function, another database's table, or before the metadata loads
#[tokio::test]
async fn no_marks_when_unsure() {
    let mut h = Harness::new().await;
    h.console_with("with r as (select id k from users) select r.k, lower(x.anything), n from r, otherdb.t x, (select 1 n) s", 0, 0);
    h.key("Right");
    assert!(console(&h).assist.names.is_empty(), "no metadata yet");
    assist_meta(&mut h);
    h.key("Right");
    assert!(console(&h).assist.names.is_empty(), "{:?}", console(&h).assist.names);
}

/// The text of each editor row drawn on the selection background (the Execute chooser's highlight)
fn highlighted(h: &mut Harness) -> Vec<String> {
    let buf = h.buffer(140, 24);
    let ed = h.app.hits.editor;
    (ed.top()..ed.bottom()).map(|y| (ed.left()..ed.right()).filter(|&x| buf[(x, y)].bg == crate::ui::SEL).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).filter(|s| !s.trim().is_empty()).collect()
}

/// Inside a subquery F5 offers it after the whole statement, innermost first (DataGrip); the editor highlights exactly its text,
/// and picking it runs only that text
#[tokio::test]
async fn execute_chooser_offers_the_subqueries_around_the_caret() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    let sql = "select 1;\nselect * from orders where user_id in (select id from users where n > (select avg(n) from users));";
    h.console_with(sql, 1, 75);
    h.key("F5");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no chooser") };
    let labels: Vec<_> = p.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels[1..], ["select avg(n) from users", "select id from users where n > (select avg(n) from users)", "All statements (2)"]);
    assert!(highlighted(&mut h)[0].contains("from orders where user_id in (select id"), "{:?}", highlighted(&mut h));
    h.key("Down");
    assert_eq!(highlighted(&mut h), ["select avg(n) from users"]);
    h.key("Down");
    assert_eq!(highlighted(&mut h), ["select id from users where n > (select avg(n) from users)"]);
    h.key("Enter");
    h.settle().await;
    assert_eq!(h.console_output(), ["> select id from users where n > (select avg(n) from users)", "2 row(s) retrieved in 7 ms"]);
}

/// A subquery picked in the chooser goes through the same checks as any run: on a Production source a write in it asks first
/// with its counted rows, on a read-only one the server-side check refuses it (the fake refuses what core's check_read_only does)
#[tokio::test]
async fn a_picked_subquery_goes_through_the_guards() {
    let sql = "select 1;\nselect * from (with d as (delete from payment where id = 4 returning id) select id from d) x;";
    let pick = |h: &mut Harness| {
        let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind else { panic!() };
        c.editor.move_cursor(tui_textarea::CursorMove::Jump(1, 85));
        h.key("F5");
        let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no chooser: {}", h.app.status) };
        assert_eq!(p.items[1].label, "with d as (delete from payment where id = 4 returning id) select id from d");
        h.key("Down");
        h.key("Enter");
    };
    let mut h = Harness::new().await;
    let mut cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
    }))
    .unwrap();
    cfg.options.confirm_changes = true;
    h.app.state.store.save(&cfg).unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.fake_db = Some(fake);
    let id = h.app.restore_console("p", "d".into(), "console".into(), sql).unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    pick(&mut h);
    h.settle().await;
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert!(text.starts_with("Production data source \"PROD\" — this changes data:\n\n  with d as (delete from payment where id = 4 returning id) select"), "{text}");
    assert!(h.console_output().is_empty(), "nothing runs before the answer");
    h.key("Esc");

    fn read_only(q: &str) -> Result<QueryResult, String> {
        match qurry_core::guard::read_only_violation(q, qurry_core::store::DbKind::Postgres, false) {
            Some(why) => Err(why),
            None => fake(q),
        }
    }
    h.app.fake_db = Some(read_only);
    h.console_with(sql, 0, 0);
    pick(&mut h);
    h.settle().await;
    let out = h.console_output();
    assert_eq!(out[0], "> with d as (delete from payment where id = 4 returning id) select id from d");
    assert!(out[1].contains("Read-only"), "{out:?}");
}

/// The theme's color of a highlight token (keyword, number, string, function, comment)
fn token(h: &Harness, tok: &str) -> ratatui::style::Color {
    let t = h.app.themes.resolve(qurry_shared::theme::SYSTEM, !h.app.auto_light);
    let (r, g, b) = qurry_shared::theme::rgb(t.color(tok)).unwrap();
    ratatui::style::Color::Rgb(r, g, b)
}

/// The cell where `pat` (a part of `after`) is, on the first screen row holding `after`
fn cell_at<'a>(buf: &'a ratatui::buffer::Buffer, after: &str, pat: &str) -> &'a ratatui::buffer::Cell {
    let (w, ht) = (buf.area.width, buf.area.height);
    let row = |y: u16| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let y = (0..ht).find(|&y| row(y).contains(after)).unwrap_or_else(|| panic!("no row with {after:?}"));
    let r = row(y);
    let at = r[..r.find(after).unwrap()].chars().count() + after[..after.find(pat).unwrap()].chars().count();
    &buf[(at as u16, y)]
}

const COLORED: &str = "select count(*), 'it''s' from t where id = 42; -- tail\n/* block\n   inside the comment */\ncreate function f() returns int as $$\n  select 1 from body;\n$$ language sql; select 2;";

/// #150: the console draws its SQL in the theme's colors, as the GUI console and the table filter do; a block comment and a
/// dollar-quoted body keep their color on every line they span
#[tokio::test]
async fn console_draws_sql_in_colors() {
    use ratatui::style::Modifier;
    let mut h = Harness::new().await;
    for theme in ["dark", "light"] {
        h.app.themes.theme = Some(theme.into());
        h.console_with(COLORED, 3, 0);
        let buf = h.buffer(100, 30);
        let fg = |after: &str, pat: &str| cell_at(&buf, after, pat).fg;
        assert_eq!(fg("select count", "select"), token(&h, "keyword"), "{theme}");
        assert_eq!(fg("t where id", "where"), token(&h, "keyword"), "{theme}");
        assert_eq!(fg("select count", "count"), token(&h, "function"), "{theme}");
        assert_eq!(fg("= 42", "42"), token(&h, "number"), "{theme}");
        assert_eq!(fg("'it''s'", "'it"), token(&h, "string"), "{theme}");
        assert_eq!(fg("'it''s'", "''s'"), token(&h, "string"), "{theme}");
        assert_eq!(fg("-- tail", "--"), token(&h, "comment"), "{theme}");
        assert!(cell_at(&buf, "-- tail", "tail").modifier.contains(Modifier::ITALIC), "{theme}");
        // Lines inside a block comment and a $$ body
        assert_eq!(fg("inside the comment", "inside"), token(&h, "comment"), "{theme}");
        assert_eq!(fg("the comment */", "*/"), token(&h, "comment"), "{theme}");
        assert_eq!(fg("select 1 from body", "select"), token(&h, "string"), "{theme}");
        assert_eq!(fg("select 1 from body", "1"), token(&h, "string"), "{theme}");
        assert_eq!(fg("; select 2", "select"), token(&h, "keyword"), "{theme}");
        // Names and symbols keep the text color
        let colored = ["keyword", "function", "number", "string", "comment"].map(|t| token(&h, t));
        for (after, pat) in [("t where", "t"), ("id = 42", "id"), ("id = 42", "="), ("function f()", "f")] {
            assert!(!colored.contains(&fg(after, pat)), "{theme}: {after}");
        }
        h.app.close_tab_now(h.app.active);
    }
}

/// The selection, search matches and the caret keep their look over the colors
#[tokio::test]
async fn console_colors_give_way_to_selection_search_and_caret() {
    use ratatui::style::Modifier;
    use tui_textarea::CursorMove;
    let mut h = Harness::new().await;
    h.console_with(COLORED, 0, 0);
    let plain = cell_at(&h.buffer(100, 30), "select count", "elect").clone();
    {
        let c = console_mut(&mut h);
        c.editor.start_selection();
        c.editor.move_cursor(CursorMove::Jump(0, 6));
        let _ = c.editor.set_search_pattern("where");
    }
    let buf = h.buffer(100, 30);
    let kw = token(&h, "keyword");
    assert_eq!(plain.fg, kw);
    let sel = cell_at(&buf, "select count", "elect");
    assert_ne!(sel.fg, kw, "selected text is drawn as selected");
    assert_ne!(sel.bg, plain.bg, "the selection has its background");
    let found = cell_at(&buf, "t where", "where");
    assert_ne!(found.fg, kw, "a search match is drawn as a match");
    assert_ne!(found.bg, plain.bg);
    // The caret right after the selection (on the space) and an unselected keyword elsewhere
    assert!(cell_at(&buf, "select count", " count").modifier.contains(Modifier::REVERSED));
    assert_eq!(cell_at(&buf, "; select 2", "select").fg, kw);
}

/// Scrolled down and sideways (tui-textarea's scroll, read from the caret): the colors stay on their characters, a block comment
/// opened above the first row on screen included
#[tokio::test]
async fn console_colors_follow_the_scroll() {
    let mut h = Harness::new().await;
    let mut sql = String::from("/* opened far above\n");
    sql += &"   still the comment\n".repeat(60);
    sql += "end */ select 1;\n";
    sql += &(0..40).map(|i| format!("{i} as n{i}, 'v{i}'")).collect::<Vec<_>>().join(", ");
    sql += " from t;";
    let last = sql.lines().count() as u16 - 1;
    let screen = |buf: &ratatui::buffer::Buffer| (0..30).map(|y| (0..80).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n").collect::<String>();
    // Scrolled down to the comment's end: the rows above it on screen are inside the comment
    h.console_with(&sql, last - 1, 0);
    let buf = h.buffer(80, 30);
    assert!(!screen(&buf).contains("opened far above") && screen(&buf).contains("still the comment"), "{}", screen(&buf));
    assert_eq!(cell_at(&buf, "still the comment", "still").fg, token(&h, "comment"));
    assert_eq!(cell_at(&buf, "still the comment", "comment").fg, token(&h, "comment"));
    assert_eq!(cell_at(&buf, "end */ select", "*/").fg, token(&h, "comment"));
    assert_eq!(cell_at(&buf, "end */ select", "select").fg, token(&h, "keyword"));
    assert_eq!(cell_at(&buf, "select 1;", "1;").fg, token(&h, "number"));
    // The end of the long last line: scrolled sideways, its tail on screen
    h.app.close_tab_now(h.app.active);
    h.console_with(&sql, last, u16::MAX);
    let buf = h.buffer(80, 30);
    assert!(screen(&buf).contains("from t;") && !screen(&buf).contains("end */ select"), "{}", screen(&buf));
    assert_eq!(cell_at(&buf, "from t;", "from").fg, token(&h, "keyword"));
    assert_eq!(cell_at(&buf, "'v39'", "'v39'").fg, token(&h, "string"));
    assert_eq!(cell_at(&buf, "'v39'", "39'").fg, token(&h, "string"));
    assert_eq!(cell_at(&buf, "39 as", "39").fg, token(&h, "number"));
    assert_eq!(cell_at(&buf, "39 as", "as").fg, token(&h, "keyword"));
}

/// A pasted tab is drawn up to the next stop: the colors after it stay on their characters
#[tokio::test]
async fn console_colors_after_a_tab() {
    let mut h = Harness::new().await;
    h.console_with("x\tselect\t\t42 from t;", 0, 0);
    let buf = h.buffer(100, 30);
    assert_eq!(cell_at(&buf, "select", "select").fg, token(&h, "keyword"));
    assert_eq!(cell_at(&buf, "select", "elect").fg, token(&h, "keyword"));
    assert_eq!(cell_at(&buf, "42 from", "42").fg, token(&h, "number"));
    assert_eq!(cell_at(&buf, "42 from", "from").fg, token(&h, "keyword"));
}

/// NO_COLOR draws the console without colors; 16 colors only with the terminal's own (no truecolor or 256 escapes)
#[tokio::test]
async fn console_colors_without_truecolor() {
    use crate::theme::ColorMode;
    use ratatui::style::Color;
    let mut h = Harness::new().await;
    h.console_with(COLORED, 3, 0);
    // The editor's text cells (inside its border, past the line numbers)
    let text = |buf: &ratatui::buffer::Buffer| {
        let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let y0 = (0..30).find(|&y| row(y).contains("select count")).unwrap();
        let x0 = row(y0).find("select").unwrap() as u16;
        (y0..y0 + 6).flat_map(|y| (x0..90).map(move |x| (x, y))).map(|p| buf[p].clone()).collect::<Vec<_>>()
    };
    h.app.color = ColorMode::NoColor;
    assert!(text(&h.buffer(100, 30)).iter().all(|c| c.fg == Color::Reset), "NO_COLOR: no fg colors");
    h.app.color = ColorMode::Ansi16;
    let cells = text(&h.buffer(100, 30));
    assert!(cells.iter().all(|c| !matches!(c.fg, Color::Rgb(..) | Color::Indexed(_))), "16 colors: no truecolor or 256 escapes");
    assert!(cells.iter().any(|c| c.fg != Color::Reset), "16 colors still tell the classes apart");
}

/// After a failed load (#157), the console's grid says why, wrapped, with the run key that runs its result's statement again: that
/// key in the grid runs the result's own statement (as the GUI's Run Again), not the one at the editor cursor; a rebound run key shows
#[tokio::test]
async fn run_key_in_the_grid_after_a_failed_load_runs_the_results_statement() {
    use crate::{app::Action, keys::Slot};
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn record(q: &str) -> Result<QueryResult, String> {
        SEEN.lock().unwrap().push(q.to_string());
        fake(q)
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(record);
    h.console_with("select id from t;\nupdate t set a = 1 where id = 2;", 0, 0);
    let fail_a_load = |h: &mut Harness| {
        let c = console_mut(h);
        let g = c.results.grid_mut().unwrap();
        g.result.truncated = true;
        g.row = g.rows() - 1;
        // The editor's cursor on the other statement: the run key in the editor would run that one
        c.editor.move_cursor(tui_textarea::CursorMove::Jump(1, 0));
        h.app.fetch_more();
    };
    // The run key in the editor asks which statement: the one at the cursor
    h.key("F5");
    h.key("Enter");
    h.settle().await;
    fail_a_load(&mut h);
    h.settle().await;
    let why = console(&h).results.grid().and_then(|g| g.more_error.clone()).expect("the load failed");
    assert_eq!(why, "Couldn't load more rows (F5 in the grid runs it again): Session closed");
    // In a narrow grid the reason wraps on the grid's last lines, and its end still shows
    let s = h.screen(60, 30);
    assert!(s.contains("│✗ Couldn't load more rows (F5 in│") && s.contains("│the grid runs it again): Session│") && s.contains("│closed "), "{s}");
    h.app.focus = crate::app::Focus::Grid;
    SEEN.lock().unwrap().clear();
    h.key("F5");
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["select id from t"], "the result's statement, not the editor's");
    assert_eq!(console(&h).results.tabs.len(), 1, "into the same result tab");
    assert!(console(&h).results.grid().is_some_and(|g| g.more_error.is_none()));
    // Without a failed load, the run key in the grid runs the editor's statement as before
    SEEN.lock().unwrap().clear();
    h.key("F5");
    h.key("Enter");
    h.settle().await;
    assert_eq!(SEEN.lock().unwrap().len(), 1);
    assert!(SEEN.lock().unwrap()[0].starts_with("update t"), "{:?}", SEEN.lock().unwrap());
    // A rebound run key: the hint names it, and it runs the result's statement again
    h.app.keys.bind(Slot::global(Action::Execute), "F9".into(), false);
    h.app.focus = crate::app::Focus::Editor;
    console_mut(&mut h).editor.move_cursor(tui_textarea::CursorMove::Jump(0, 0));
    h.key("F9");
    h.key("Enter");
    h.settle().await;
    fail_a_load(&mut h);
    h.settle().await;
    let why = console(&h).results.grid().and_then(|g| g.more_error.clone()).unwrap_or_default();
    assert!(why.starts_with("Couldn't load more rows (F9 in the grid runs it again)"), "{why}");
    h.app.focus = crate::app::Focus::Grid;
    SEEN.lock().unwrap().clear();
    h.key("F9");
    h.settle().await;
    assert_eq!(*SEEN.lock().unwrap(), ["select id from t"]);
}

/// The run key in the grid after a failed load, with a pending edit, asks first as the editor's run does: no keeps the edit, yes
/// drops it and runs the result's statement again
#[tokio::test]
async fn rerun_after_a_failed_load_with_pending_edits_asks_first() {
    let mut h = prod_console("select id, user_name as name from users").await;
    h.key("a");
    assert_eq!(console(&h).grid().unwrap().pending(), 1);
    let g = console_mut(&mut h).results.grid_mut().unwrap();
    g.result.truncated = true;
    g.row = g.rows() - 1;
    h.app.fetch_more();
    h.settle().await;
    assert!(console(&h).results.grid().is_some_and(|g| g.more_error.is_some()), "the load failed");
    h.app.focus = crate::app::Focus::Grid;
    let before = h.console_output().len();
    h.key("F5");
    let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
    assert_eq!(text, "1 pending change(s) are not saved to the database (⌃s submits them). Discard them and run the statement?");
    h.key("n");
    assert_eq!(console(&h).grid().unwrap().pending(), 1, "the edit stays");
    assert_eq!(h.console_output().len(), before, "nothing ran");
    h.key("F5");
    h.key("y");
    h.settle().await;
    assert!(h.console_output().len() > before, "it ran");
    assert_eq!(console(&h).grid().unwrap().pending(), 0);
    assert!(console(&h).results.grid().is_some_and(|g| g.more_error.is_none()));
}
