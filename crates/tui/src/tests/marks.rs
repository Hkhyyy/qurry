//! Gutter marks of executed statements (DataGrip): ✓ done, ✗ failed, … running, in the line-number column
use crate::app::{MarkState, Marks, TabKind};
use crate::testkit::Harness;
use qurry_core::db::QueryResult;

/// Stands in for the database: "boom" fails, anything else changes nothing
fn fake(q: &str) -> Result<QueryResult, String> {
    if q.contains("boom") {
        return Err("ERROR: relation \"boom\" does not exist".into());
    }
    Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: 0, truncated: false, elapsed_ms: 7, has_result_set: false, in_transaction: false, warning: None })
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
        c.show_output = false;
    }
    let buf = h.buffer(100, 24);
    let (x, y) = (0..24).flat_map(|y| (0..100).map(move |x| (x, y))).find(|&(x, y)| buf[(x, y)].symbol() == "✓").unwrap();
    h.mouse("down", x, y);
    assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Console(c) if c.show_output));
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
    // A new run of the whole text replaces the earlier marks as each statement starts
    m.start(&c, &[(0, 0), (3, 4), (5, 5)]);
    m.set(&c, 0, MarkState::Running);
    assert_eq!((m.at(0), m.at(5)), (Some(MarkState::Running), Some(MarkState::Err)));
}
