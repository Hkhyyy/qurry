//! Gutter marks of executed statements (DataGrip): ✓ done, ✗ failed, … running, in the line-number column
use crate::app::{MarkState, Marks, Popup, TabKind};
use crate::testkit::Harness;
use qurry_core::db::QueryResult;

/// Stands in for the database: "boom" fails, anything else changes nothing
fn fake(q: &str) -> Result<QueryResult, String> {
    if q.contains("boom") {
        return Err("ERROR: relation \"boom\" does not exist".into());
    }
    Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: 0, truncated: false, elapsed_ms: 7, has_result_set: false, in_transaction: false, warning: None, editable: None, notices: vec![] })
}

/// The focused console editor's lines as drawn, from the line-number column on ("✓1 select 1;")
fn editor_lines(h: &mut Harness) -> Vec<String> {
    let s = h.screen(100, 24);
    s.lines().filter_map(|l| l.split_once('┃')).map(|(_, r)| r.trim_end_matches('┃').trim_end().to_string()).filter(|r| r.contains("select")).collect()
}

/// A run stops at the failed statement: ✓ on the 1st, ✗ on the 2nd, nothing on the 3rd (it never ran)
#[tokio::test]
async fn marks_stop_at_the_failed_statement() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.console_with("select 1;\nselect * from boom;\nselect 2;", 0, 0);
    h.key("F5");
    h.settle().await;
    assert_eq!(editor_lines(&mut h), ["✓1 select 1;", "✗2 select * from boom;", " 3 select 2;"]);

    // A new line above: the marks follow their statements
    h.key("Enter");
    assert_eq!(editor_lines(&mut h), ["✓2 select 1;", "✗3 select * from boom;", " 4 select 2;"]);
    // Editing the failed statement drops its mark, the other stays
    h.key("Down");
    h.key("End");
    h.type_str(" -- x");
    assert_eq!(editor_lines(&mut h), ["✓2 select 1;", " 3 select * from boom; -- x", " 4 select 2;"]);

    // Clicking a mark shows Output (where its line is)
    if let TabKind::Console(c) = &mut h.app.tabs[h.app.active].kind {
        c.results.show(c.results.tabs.first().map(|t| t.id));
    }
    let buf = h.buffer(100, 24);
    let (x, y) = (0..24).flat_map(|y| (0..100).map(move |x| (x, y))).find(|&(x, y)| buf[(x, y)].symbol() == "✓").unwrap();
    h.mouse("down", x, y);
    assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Console(c) if c.results.shown.is_none()));
}

/// Each statement's mark changes as it finishes, not when the whole run ends
#[tokio::test]
async fn marks_change_as_each_statement_finishes() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.console_with("select 1;\nselect 2;", 0, 0);
    h.key("F5");
    // The screens between background results, each once
    let mut seen = vec![editor_lines(&mut h)];
    while h.step().await {
        let now = editor_lines(&mut h);
        if seen.last() != Some(&now) {
            seen.push(now);
        }
    }
    assert_eq!(
        seen,
        [[" 1 select 1;", " 2 select 2;"], ["…1 select 1;", " 2 select 2;"], ["✓1 select 1;", " 2 select 2;"], ["✓1 select 1;", "…2 select 2;"], ["✓1 select 1;", "✓2 select 2;"]]
    );
}

/// A selection's statements are marked on their own rows
#[tokio::test]
async fn selection_marks_its_own_rows() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.console_with("select 1;\nselect boom;\nselect 3;", 1, 0);
    h.key("Shift+Down");
    h.key("Shift+End");
    h.key("F5");
    h.settle().await;
    assert_eq!(editor_lines(&mut h), [" 1 select 1;", "✗2 select boom;", " 3 select 3;"]);
    // The failed one fixed (its ✗ goes with the edit), then the whole console runs: each statement gets its mark
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.key("Up");
    h.key("End");
    h.key("Backspace");
    h.key("Backspace");
    h.key("Backspace");
    h.key("Backspace");
    h.key("Backspace");
    h.type_str("2;");
    h.key("F5");
    h.settle().await;
    assert_eq!(editor_lines(&mut h), ["✓1 select 1;", "✓2 select 2;", "✓3 select 3;"]);
}

/// Marks follow edits line by line: kept above the change, shifted below it, dropped on it; a statement that runs replaces what it overlaps
#[test]
fn marks_follow_edits() {
    let lines = |s: &str| s.split('\n').map(String::from).collect::<Vec<_>>();
    let mut m = Marks::default();
    let a = lines("select 1;\nselect\n  2;\nselect 3;");
    m.start(&a, &[(0, 0), (1, 2), (3, 3)]);
    // Not started yet: nothing drawn
    assert_eq!(m.at(0), None);
    m.set(&a, 0, MarkState::Ok);
    m.set(&a, 1, MarkState::Running);
    m.set(&a, 2, MarkState::Err);
    assert_eq!((m.at(0), m.at(1), m.at(3)), (Some(MarkState::Ok), Some(MarkState::Running), Some(MarkState::Err)));
    // Two lines added between the 1st and 2nd statements
    let b = lines("select 1;\n\n\nselect\n  2;\nselect 3;");
    m.follow(&b);
    assert_eq!((m.at(0), m.at(3), m.at(5)), (Some(MarkState::Ok), Some(MarkState::Running), Some(MarkState::Err)));
    // The 2nd statement's second line edited: its mark goes
    let c = lines("select 1;\n\n\nselect\n  22;\nselect 3;");
    m.follow(&c);
    assert_eq!((m.at(0), m.at(3), m.at(5)), (Some(MarkState::Ok), None, Some(MarkState::Err)));
    // A new run clears every earlier mark once its first statement starts, not before (it may still be cancelled)
    m.start(&c, &[(0, 0)]);
    assert_eq!((m.at(0), m.at(5)), (Some(MarkState::Ok), Some(MarkState::Err)));
    m.set(&c, 0, MarkState::Running);
    assert_eq!((m.at(0), m.at(5)), (Some(MarkState::Running), None));
}

/// The gutter shows only the latest run: running one statement again drops the marks of the others from earlier runs
#[tokio::test]
async fn a_new_run_clears_the_marks_of_earlier_runs() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.console_with("select * from orders;\nselect * from users;", 0, 0);
    h.key("F5");
    h.settle().await;
    assert_eq!(editor_lines(&mut h), ["✓1 select * from orders;", "✓2 select * from users;"]);
    // Only the 2nd (a selection of its line)
    h.key("Down");
    h.key("Home");
    h.key("Shift+End");
    h.key("F5");
    h.settle().await;
    assert_eq!(editor_lines(&mut h), [" 1 select * from orders;", "✓2 select * from users;"]);
}

/// Fails like PostgreSQL on "boom", with the position of it in the statement sent (qurry_shared::errors::with_position)
fn positioned(q: &str) -> Result<QueryResult, String> {
    match q.find("boom") {
        Some(i) => Err(qurry_shared::errors::with_position("ERROR: relation \"boom\" does not exist", q[..i].chars().count() + 1)),
        None => fake(q),
    }
}

/// The editor text drawn underlined in the danger colour (the run's error mark), one entry per row that has some
fn underlined(h: &mut Harness) -> Vec<String> {
    let buf = h.buffer(100, 24);
    let marked = |x: u16, y: u16| buf[(x, y)].modifier.contains(ratatui::style::Modifier::UNDERLINED) && buf[(x, y)].fg == crate::kit::DANGER;
    (0..24).map(|y| (0..100).filter(|&x| marked(x, y)).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).filter(|s| !s.is_empty()).collect()
}

/// A failed statement that isn't the first: the server's spot is underlined in the editor (through the statement's place in the
/// console, past a wide character before it) and the status line says where; an edit clears it, Output keeps the error
#[tokio::test]
async fn run_error_underlines_the_servers_spot() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(positioned);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.console_with("select 1;\nselect '한', x from boom;", 0, 0);
    h.key("F5");
    h.settle().await;
    assert_eq!(underlined(&mut h), ["boom"]);
    assert_eq!(h.app.status, "Error at line 2, col 20: ERROR: relation \"boom\" does not exist — fix it and run again");
    // An edit anywhere clears it; Output keeps the error
    h.key("End");
    h.type_str(" ");
    assert!(underlined(&mut h).is_empty());
    assert!(h.console_output().iter().any(|l| l.contains("relation \"boom\" does not exist")), "{:?}", h.console_output());
}

/// No position from the server: the whole statement is underlined; the next run clears it before it starts
#[tokio::test]
async fn run_error_without_a_position_marks_the_statement() {
    let mut h = Harness::new().await;
    h.app.fake_db = Some(fake);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.console_with("select 1;\nselect *\n  from boom;", 0, 0);
    h.key("F5");
    h.settle().await;
    assert_eq!(underlined(&mut h), ["select *", "  from boom"]);
    assert_eq!(h.app.status, "Error: ERROR: relation \"boom\" does not exist — fix the statement and run it again");
    h.app.fake_db = Some(|q| fake(&q.replace("boom", "t")));
    h.key("F5");
    assert!(underlined(&mut h).is_empty(), "cleared when the next run starts");
    h.settle().await;
    assert!(underlined(&mut h).is_empty());
}

/// Production: the rows are counted before the confirm comes up, and the editor still takes keys meanwhile. A line typed above
/// the statement then moves it: its error isn't underlined on the old place (the status line says the error only)
#[tokio::test]
async fn run_error_is_not_marked_on_text_edited_while_production_rows_counted() {
    fn counts(q: &str) -> Result<QueryResult, String> {
        if !q.starts_with("SELECT count(*)") {
            return positioned(q);
        }
        let col = qurry_core::db::ColumnInfo { name: "count".into(), type_name: "int8".into() };
        Ok(QueryResult { columns: vec![col], rows: vec![vec![1.into()]], rows_affected: 0, truncated: false, elapsed_ms: 1, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![] })
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
    let id = h.app.restore_console("p", "d".into(), "console".into(), "delete from boom where id = 1;").unwrap();
    let i = h.app.tab_index(id).unwrap();
    h.app.activate(i);
    h.key("F5");
    // Typed while counting: a line above the statement
    h.key("Enter");
    h.settle().await;
    let TabKind::Console(c) = &h.app.tabs[h.app.active].kind else { panic!() };
    assert_eq!(c.editor.lines(), ["", "delete from boom where id = 1;"]);
    let Some(Popup::Confirm { opened: Some(t), .. }) = &mut h.app.popup else { panic!("no confirm: {}", h.app.status) };
    *t -= crate::app::App::TLS_ASK_GRACE;
    h.key("y");
    h.settle().await;
    assert!(h.console_output().iter().any(|l| l.contains("relation \"boom\" does not exist")), "{:?}", h.console_output());
    assert!(underlined(&mut h).is_empty(), "{:?}", underlined(&mut h));
    assert_eq!(h.app.status, "Error: ERROR: relation \"boom\" does not exist");
}
