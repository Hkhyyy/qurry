use super::*;

pub(super) fn grid_key(app: &mut App, k: KeyEvent) {
    // Esc: if there's a selection (a range, marked rows), clear just that first
    if k.code == KeyCode::Esc {
        if let Some(g) = app.grid_mut().filter(|g| g.anchor.is_some() || !g.marked.is_empty()) {
            g.anchor = None;
            g.marked.clear();
            return;
        }
    }
    // The grid's commands (n set NULL, a add row …: pane keys, F1 › Data grid keys), in table data and query results
    let grid = match app.tabs.get(app.active).map(|t| &t.kind) {
        Some(TabKind::Table(_)) => true,
        Some(TabKind::Console(c)) => !c.show_output && c.grid.is_some(),
        _ => false,
    };
    if grid && let Some(a) = app.keys.lookup_in(Scope::Grid, &k) {
        return app.run_action(a);
    }
    let Some(k) = plain(k) else { return };
    let Some(t) = app.tabs.get_mut(app.active) else {
        app.focus = Focus::Tree;
        return;
    };
    let id = t.id;
    match &mut t.kind {
        TabKind::Dba(d) => match k.code {
            KeyCode::Char('r') => app.dba_reload(id),
            KeyCode::Char('a') => app.dba_toggle_auto(),
            KeyCode::Char('c') => app.dba_kill(false),
            KeyCode::Char('K') => app.dba_kill(true),
            KeyCode::Enter => app.dba_actions(true),
            KeyCode::Char('g') => app.dba_actions(false),
            KeyCode::Char('p') => app.dba_palette(),
            KeyCode::Esc => app.focus = Focus::Tree,
            _ => {
                // Moving selects a new row (instead of the vanished session)
                d.gone = None;
                // The report's own letters first, then the grid's commands (Y IN list, x export)
                match app.keys.lookup_in(Scope::Grid, &k) {
                    Some(a) => app.run_action(a),
                    None => nav(app, k),
                }
            }
        },
        TabKind::Text(tt) => match k.code {
            KeyCode::Down | KeyCode::Char('j') => tt.scroll = tt.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => tt.scroll = tt.scroll.saturating_sub(1),
            KeyCode::PageDown => tt.scroll = tt.scroll.saturating_add(20),
            KeyCode::PageUp => tt.scroll = tt.scroll.saturating_sub(20),
            KeyCode::Home | KeyCode::Char('g') => tt.scroll = 0,
            // Up to the last line that fits the screen (trimmed when drawing)
            KeyCode::End | KeyCode::Char('G') => tt.scroll = u16::MAX,
            // The SQL in either view (as the GUI's Copy)
            KeyCode::Char('y') => {
                let s = tt.text.clone();
                copy(app, &s)
            }
            KeyCode::Char('v') => app.toggle_structure(id),
            KeyCode::Char('r') => app.reload_ddl(id),
            KeyCode::Esc => app.focus = Focus::Tree,
            _ => {}
        },
        TabKind::Redis(_) => redis_key(app, id, k),
        TabKind::Console(c) => {
            if matches!(k.code, KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab) {
                app.focus = Focus::Editor;
                return;
            }
            if c.show_output || c.grid.is_none() {
                return;
            }
            match k.code {
                KeyCode::Enter | KeyCode::F(2) => edit_cell(app, id),
                _ => nav(app, k),
            }
        }
        TabKind::Table(_) => {
            match k.code {
                KeyCode::Char('w') => app.input_filter(id, false),
                KeyCode::Char('o') => app.input_filter(id, true),
                KeyCode::Enter | KeyCode::F(2) => edit_cell(app, id),
                KeyCode::Char('r') => app.reload_table_checked(id),
                KeyCode::Char('s') => app.sort_table(id),
                KeyCode::Char('/') => app.input_filter(id, false),
                KeyCode::Esc => app.focus = Focus::Tree,
                _ => nav(app, k),
            }
        }
    }
}

/// ↩ / F2: edit the current cell of tab `id`'s grid (table data, or a console result that can be edited)
fn edit_cell(app: &mut App, id: u64) {
    let Some(g) = app.edit_grid() else { return };
    let (r, c, gid) = (g.row, g.col, g.id);
    if g.cols() == 0 {
        return;
    }
    let name = g.result.columns[c].name.clone();
    let ty = g.result.columns[c].type_name.clone();
    let cur = g.value(r, c).unwrap_or_default();
    app.input(format!("{name} (row {})", r + 1), &cur.clone(), move |a, s| {
        // Same rule as the GUI's Value Editor: a json / jsonb column refuses text that isn't JSON
        let v = match qurry_shared::value::value_to_apply(&ty, &cur, &s) {
            Ok(v) => v,
            Err(e) => return a.status = format!("{name} not changed: {e}"),
        };
        // Into the grid it was asked for (a console run meanwhile replaced it: dropped)
        let grid = match a.tab_mut(id).map(|t| &mut t.kind) {
            Some(TabKind::Table(tt)) => tt.grid.as_mut(),
            Some(TabKind::Console(c)) => c.grid.as_mut(),
            _ => None,
        };
        if let Some(g) = grid.filter(|g| g.id == gid) {
            g.set_value(r, c, v);
        }
    });
}

/// Grid navigation
pub(super) fn nav(app: &mut App, k: KeyEvent) {
    let Some(g) = app.grid_mut() else { return };
    let page = g.page as isize;
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    match k.code {
        // Shift+arrows = range selection (DataGrip). Horizontal scroll is trackpad·Shift+wheel·scrollbar
        KeyCode::Down if shift => g.extend(1, 0),
        KeyCode::Up if shift => g.extend(-1, 0),
        KeyCode::Right if shift => g.extend(0, 1),
        KeyCode::Left if shift => g.extend(0, -1),
        KeyCode::Down | KeyCode::Char('j') => g.move_by(1, 0),
        KeyCode::Up | KeyCode::Char('k') => g.move_by(-1, 0),
        KeyCode::Right | KeyCode::Char('l') => g.move_by(0, 1),
        KeyCode::Left | KeyCode::Char('h') => g.move_by(0, -1),
        KeyCode::PageDown => g.move_by(page, 0),
        KeyCode::PageUp => g.move_by(-page, 0),
        KeyCode::Home => g.col = 0,
        KeyCode::End => g.col = g.cols().saturating_sub(1),
        KeyCode::Char('g') => g.row = 0,
        KeyCode::Char('G') => g.row = g.rows().saturating_sub(1),
        KeyCode::Char('y') => {
            // TSV if a range or rows are selected (pastes into cells in a sheet), otherwise the current cell
            let Some((rows, cols)) = g.selection() else {
                let v = g.value(g.row, g.col).unwrap_or_else(|| "NULL".into());
                return copy(app, &v);
            };
            let text = g.selection_tsv();
            copy(app, &text);
            let (nr, nc) = (rows.len(), cols.len());
            app.status = format!("Copied {nr} row{} × {nc} column{}", if nr == 1 { "" } else { "s" }, if nc == 1 { "" } else { "s" });
            return;
        }
        KeyCode::Char('s') => {
            g.toggle_local_sort();
            if g.result.truncated {
                app.status = "Sorted the loaded rows only".into();
            }
            return;
        }
        _ => return,
    }
    app.fetch_more();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    /// ⌘← arriving as ⌃A doesn't add a row (a adds a row)
    #[tokio::test]
    async fn ctrl_a_in_table_moves_to_first_column() {
        let mut h = Harness::new().await;
        let id = h.table_tab("bank_info", &["bank_code", "bank_name"], &[&["002", "산업"]], &["bank_code"]);
        h.key("Right");
        h.key("Ctrl+A");
        assert_eq!(h.table(id).grid.as_ref().unwrap().added, 0);
        assert_eq!(h.table(id).grid.as_ref().unwrap().col, 0);
        h.key("a");
        assert_eq!(h.table(id).grid.as_ref().unwrap().added, 1);
    }

    /// Grid range selection: Shift+↑↓←→ (or drag), y = TSV, Y = IN list of selected cells only, Esc = clear selection
    #[tokio::test]
    async fn grid_range_select_and_copy() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["1.2.3.4", "1"], &["5.6.7.8", "2"], &["9.9.9.9", "3"]], &[]);
        h.key("Shift+Down");
        h.key("y");
        assert_eq!(h.app.status, "Copied 2 rows × 1 column");
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('1.2.3.4', '5.6.7.8')");
        // Esc clears only the selection first
        h.key("Esc");
        assert_eq!(h.app.focus, crate::app::Focus::Grid);
        h.key("Y");
        assert!(h.app.status.starts_with("Copied 3 values"), "{}", h.app.status);
        // Dragging with the mouse selects too
        let s = h.screen(120, 20);
        let (y0, line) = s.lines().enumerate().find(|(_, l)| l.contains("1.2.3.4")).unwrap();
        let x = line[..line.find("1.2.3.4").unwrap()].chars().count() as u16;
        h.mouse("down", x, y0 as u16);
        h.mouse("drag", x, y0 as u16 + 2);
        h.mouse("up", x, y0 as u16 + 2);
        h.key("Y");
        assert_eq!(h.app.status, "Copied 3 values as an IN list: ('1.2.3.4', '5.6.7.8', '9.9.9.9')");
    }

    /// m marks rows (the GUI's ⌘-click on a row number) and moving keeps them: y copies the marked rows in grid order, d deletes them
    /// all (again: back), m on a marked row takes it out, Esc clears the marks
    #[tokio::test]
    async fn marked_rows_copy_and_delete() {
        let mut h = Harness::new().await;
        let id = h.table_tab("logs", &["ip", "n"], &[&["a", "1"], &["b", "2"], &["c", "3"], &["d", "4"], &["e", "5"]], &["n"]);
        fn grid(h: &Harness, id: u64) -> &crate::grid::Grid {
            h.table(id).grid.as_ref().unwrap()
        }
        for _ in 0..4 {
            h.key("Down");
        }
        h.key("m");
        assert_eq!(h.app.status, "1 row marked: y copies, d deletes them · Esc clears");
        h.key("g");
        h.key("m");
        h.key("Down");
        assert_eq!(grid(&h, id).marked, [0, 4].into());
        assert_eq!(grid(&h, id).selection_tsv(), "a\t1\ne\t5");
        h.key("y");
        assert_eq!(h.app.status, "Copied 2 rows × 2 columns");
        // the cursor's row (2) is not marked: d leaves it alone
        h.key("d");
        assert_eq!(grid(&h, id).deleted, [0, 4].into());
        h.key("d");
        assert!(grid(&h, id).deleted.is_empty());
        // A range and marked rows copy together: the union of their columns, cells outside both empty
        h.key("Shift+Down");
        assert_eq!(grid(&h, id).selection_tsv(), "a\t1\nb\t\nc\t\ne\t5");
        h.key("Esc");
        assert!(grid(&h, id).marked.is_empty() && grid(&h, id).anchor.is_none());
        assert_eq!(h.app.focus, crate::app::Focus::Grid);
        h.key("m");
        h.key("m");
        assert!(grid(&h, id).marked.is_empty());
        assert_eq!(h.app.status, "No rows marked");
    }

    /// Y with marked rows lists the current column of those rows only, in grid order (the GUI's ⌘-clicked rows + Copy as IN List);
    /// with nothing marked: the whole current column
    #[tokio::test]
    async fn marked_rows_copy_as_in_list() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["a", "1"], &["b", "2"], &["c", "3"], &["d", "4"], &["e", "5"]], &["n"]);
        for _ in 0..4 {
            h.key("Down");
        }
        h.key("m");
        h.key("g");
        h.key("m");
        h.key("Down");
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('a', 'e')");
        h.key("Esc");
        h.key("Y");
        assert_eq!(h.app.status, "Copied 5 values as an IN list: ('a', 'b', 'c', 'd', 'e')");
    }

    /// Marked rows that were just added go away on d, edits of the added rows below them move up with them
    #[tokio::test]
    async fn deleting_marked_added_rows_removes_them() {
        let mut h = Harness::new().await;
        let id = h.table_tab("logs", &["ip"], &[&["a"]], &["ip"]);
        for v in ["x", "y", "z"] {
            h.key("a");
            if let Some(TabKind::Table(t)) = h.app.tab_mut(id).map(|t| &mut t.kind) {
                let g = t.grid.as_mut().unwrap();
                g.set_value(g.row, 0, v.into());
            }
        }
        h.key("m");
        h.key("Up");
        h.key("Up");
        h.key("m");
        h.key("d");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!((g.added, g.rows()), (1, 2));
        assert_eq!(g.value(1, 0).as_deref(), Some("y"));
        assert!(g.marked.is_empty());
    }

    /// A table whose second column is jsonb, its cursor on that column's first cell
    fn jsonb_table(h: &mut Harness) -> u64 {
        let id = h.table_tab("payments", &["id", "data"], &[&["1", r#"{"kind": "card", "n": 12345678901234567890}"#], &["2", "[1, 2]"]], &["id"]);
        if let Some(TabKind::Table(t)) = h.app.tab_mut(id).map(|t| &mut t.kind) {
            t.grid.as_mut().unwrap().result.columns[1].type_name = "jsonb".into();
        }
        h.key("Right");
        id
    }

    /// v: the value panel sits beside the grid (both titles on one line) and follows the cursor, JSON pretty-printed as the GUI shows it;
    /// ⌃w from the grid closes the panel first, then the tab (as the GUI's ⌘w)
    #[tokio::test]
    async fn value_panel_follows_the_cursor_beside_the_grid() {
        let mut h = Harness::new().await;
        let id = jsonb_table(&mut h);
        h.key("v");
        let s = h.screen(120, 24);
        assert!(s.lines().any(|l| l.contains("public.payments") && l.contains("data : jsonb")), "{s}");
        assert!(s.contains(r#"  "kind": "card","#) && s.contains(r#"  "n": 12345678901234567890"#), "{s}");
        h.key("Down");
        let s = h.screen(120, 24);
        assert!(s.contains("│  1,") && !s.contains(r#"│  "kind""#), "{s}");
        // ↑↓ still move the grid's cursor
        assert_eq!(h.table(id).grid.as_ref().unwrap().row, 1);
        h.key("Ctrl+W");
        assert!(!h.app.value_panel);
        assert!(h.app.tab_mut(id).is_some());
        assert!(!h.screen(120, 24).contains("data : jsonb"));
        h.key("Ctrl+W");
        assert!(h.app.tab_mut(id).is_none());
    }

    /// Enter on a jsonb cell: text that isn't JSON is refused with the reason (the GUI's Value Editor rule), JSON is written
    #[tokio::test]
    async fn json_column_edit_refuses_invalid_json() {
        let mut h = Harness::new().await;
        let id = jsonb_table(&mut h);
        let edit = |h: &mut Harness, text: &str| {
            h.key("Enter");
            let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no input") };
            *input = tui_textarea::TextArea::from([text]);
            h.key("Enter");
        };
        edit(&mut h, "{oops");
        assert!(h.app.status.starts_with("data not changed: Invalid JSON"), "{}", h.app.status);
        assert!(h.table(id).grid.as_ref().unwrap().edits.is_empty());
        edit(&mut h, r#"{"kind":"bank"}"#);
        assert_eq!(h.table(id).grid.as_ref().unwrap().value(0, 1).as_deref(), Some(r#"{"kind":"bank"}"#));
    }

    /// ⌃K › Database Admin…: picking a report opens a DBA tab (reuses that tab for the same connection target)
    #[tokio::test]
    async fn dba_report_tab_opens_and_switches() {
        let mut h = Harness::new().await;
        h.table_tab("t", &["id"], &[&["1"]], &[]);
        h.app.run_action(Action::DbaReports);
        let labels = |h: &Harness| match &h.app.popup {
            Some(crate::app::Popup::Palette(p)) => p.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>(),
            _ => panic!("no chooser"),
        };
        let l = labels(&h);
        assert!(l.contains(&"Activity › Sessions".to_string()) && l.contains(&"Storage › Table Sizes".to_string()), "{l:?}");
        h.key("Enter");
        let n = h.app.tabs.len();
        let report = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Dba(d) => d.report.clone(),
            _ => panic!("not a dba tab"),
        };
        assert_eq!(report(&h), "sessions");
        h.settle().await;
        // No connection, so an error is shown
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.error.is_some()));
        h.app.run_action(Action::DbaReports);
        h.type_str("Table Sizes");
        h.key("Enter");
        assert_eq!(report(&h), "table_sizes");
        assert_eq!(h.app.tabs.len(), n);
    }

    /// Switching the report of a reused DBA tab clears its grid, and a result of an older run that lands afterwards is dropped (also A → B → A)
    #[tokio::test]
    async fn dba_switch_drops_the_old_reports_result() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        let res = |col: &str, v: i64| QueryResult {
            columns: vec![ColumnInfo { name: col.into(), type_name: "INT4".into() }],
            rows: vec![vec![serde_json::json!(v)]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
            editable: None,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "sessions");
        let d = crate::app::DbaTab { report: "sessions".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res("pid", 4123))), loaded: None, error: None, running: true, run: 0, auto: true };
        let id = h.app.push_tab("test", "postgres".into(), "Sessions".into(), TabKind::Dba(d));
        h.app.run_action(Action::DbaReports);
        h.type_str("Table Sizes");
        h.key("Enter");
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.report == "table_sizes" && d.grid.is_none()));
        // The sessions refresh that was running before the switch (run 0; Table Sizes is run 1)
        h.app.dba_apply(id, 0, Ok(res("pid", 4123)));
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.grid.is_none() && d.running));
        assert!(!h.screen(120, 24).contains("4123"));
        // Back to Sessions (run 2) before either older run lands: neither clears running, though run 0 is the same report
        h.app.run_action(Action::DbaReports);
        h.type_str("Sessions");
        h.key("Enter");
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.report == "sessions" && d.run == 2));
        h.app.dba_apply(id, 1, Ok(res("bytes", 777)));
        h.app.dba_apply(id, 0, Ok(res("pid", 4123)));
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.grid.is_none() && d.running));
        h.app.dba_apply(id, 2, Ok(res("pid", 5555)));
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.grid.is_some() && !d.running));
    }

    /// Follows the selected session (pid) even when auto-refresh reorders rows (so another session isn't killed)
    #[tokio::test]
    async fn dba_selection_follows_pid_across_reloads() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        let res = |pids: &[i64]| QueryResult {
            columns: vec![ColumnInfo { name: "pid".into(), type_name: "INT4".into() }],
            rows: pids.iter().map(|p| vec![serde_json::json!(p)]).collect(),
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
            editable: None,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "sessions");
        let d = crate::app::DbaTab { report: "sessions".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res(&[10, 20, 30]))), loaded: None, error: None, running: false, run: 0, auto: true };
        let id = h.app.push_tab("test", "postgres".into(), "Sessions".into(), TabKind::Dba(d));
        h.key("Down");
        h.app.dba_apply(id, 0, Ok(res(&[30, 20, 10])));
        let TabKind::Dba(d) = &h.app.tabs[h.app.active].kind else { panic!() };
        let g = d.grid.as_ref().unwrap();
        assert_eq!(g.value(g.row, 0).as_deref(), Some("20"));
        // If the selected session disappears, nothing is selected (so the wrong session isn't killed)
        h.app.dba_apply(id, 0, Ok(res(&[30, 10])));
        let TabKind::Dba(d) = &h.app.tabs[h.app.active].kind else { panic!() };
        assert!(d.grid.as_ref().unwrap().result.rows.len() == 2);
        h.key("K");
        assert!(h.app.popup.is_none(), "must not offer to kill a session that is gone");
    }

    /// A refresh that fails (time limit on a busy server) keeps the last sessions on screen, so the blocker can still be killed
    #[tokio::test]
    async fn dba_failed_refresh_keeps_the_last_grid() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        let res = QueryResult {
            columns: vec![ColumnInfo { name: "pid".into(), type_name: "INT4".into() }],
            rows: vec![vec![serde_json::json!(4123)]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
            editable: None,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "sessions");
        let d = crate::app::DbaTab { report: "sessions".into(), info, gone: None, grid: None, loaded: None, error: None, running: false, run: 0, auto: true };
        let id = h.app.push_tab("test", "postgres".into(), "Sessions".into(), TabKind::Dba(d));
        // No result yet: the error replaces the grid
        h.app.dba_apply(id, 0, Err("The report ran into its time limit".into()));
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.grid.is_none() && d.error.is_some()));
        h.app.dba_apply(id, 0, Ok(res));
        h.app.dba_apply(id, 0, Err("The report ran into its time limit (30 s, or a stricter one set on the server).\n(statement timeout)".into()));
        let TabKind::Dba(d) = &h.app.tabs[h.app.active].kind else { panic!() };
        assert!(d.grid.as_ref().is_some_and(|g| g.rows() == 1) && d.error.is_some());
        let screen = h.screen(120, 24);
        assert!(screen.contains("Refresh failed, showing data from 0 s ago: The report ran into its time limit") && screen.contains("4123"), "{screen}");
        h.key("K");
        assert!(h.app.popup.is_some(), "the kept session can still be terminated");
    }

    /// Vacuum Progress only cancels: K (kill connection) doesn't even ask, and isn't in the hint line
    #[tokio::test]
    async fn dba_vacuum_progress_only_cancels() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        let res = QueryResult {
            columns: vec![ColumnInfo { name: "pid".into(), type_name: "INT4".into() }, ColumnInfo { name: "query".into(), type_name: "TEXT".into() }],
            rows: vec![vec![serde_json::json!(77), serde_json::json!("autovacuum: VACUUM public.big")]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
            editable: None,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "vacuum_progress");
        let d = crate::app::DbaTab { report: "vacuum_progress".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res)), loaded: None, error: None, running: false, run: 0, auto: true };
        h.app.push_tab("test", "postgres".into(), "Vacuum Progress".into(), TabKind::Dba(d));
        h.key("K");
        assert!(h.app.popup.is_none(), "must not offer to terminate a vacuum");
        let s = h.screen(160, 20);
        assert!(s.contains("cancel query") && !s.contains("terminate"), "{s}");
        h.key("c");
        assert!(matches!(&h.app.popup, Some(crate::app::Popup::Confirm { text, .. }) if text.contains("Cancel the query of session 77")));
    }

    /// Slow Queries: g lists Explain in Console, which opens a console with the EXPLAIN (a normalized statement gets a note to fill in the values first)
    #[tokio::test]
    async fn dba_slow_query_explains_in_console() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        // MySQL needs no server for this (no generic plans)
        let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "test", "kind": "mysql", "host": "127.0.0.1", "port": 1, "user": "u", "database": "shop" })).unwrap();
        h.app.state.save_connection(cfg, None, None).await.unwrap();
        h.app.reload_conns();
        let res = QueryResult {
            columns: vec![ColumnInfo { name: "calls".into(), type_name: "INT8".into() }, ColumnInfo { name: "query".into(), type_name: "TEXT".into() }],
            rows: vec![vec![serde_json::json!(12), serde_json::json!("SELECT * FROM `orders` WHERE `id` IN (...)")]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
            editable: None,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Mysql).into_iter().find(|r| r.id == "slow_queries");
        let d = crate::app::DbaTab { report: "slow_queries".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res)), loaded: None, error: None, running: false, run: 0, auto: true };
        h.app.push_tab("test", "shop".into(), "Slow Queries".into(), TabKind::Dba(d));
        h.key("g");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no action list") };
        assert!(p.items.iter().any(|i| i.label == "Explain in Console"), "{:?}", p.items.iter().map(|i| &i.label).collect::<Vec<_>>());
        h.type_str("Explain");
        h.key("Enter");
        h.settle().await;
        let text = h.console_text();
        assert!(text.starts_with("-- Replace the parameters") && text.ends_with("\nEXPLAIN SELECT * FROM `orders` WHERE `id` IN (...)"), "{text}");
    }

    /// ⌃K › Copy Related Rows: condition → child steps → destination (only other Data Sources of the same type). Generate Test Data asks for the row count
    #[tokio::test]
    async fn copy_rows_and_test_data_flows() {
        let mut h = Harness::new().await;
        for (id, name, kind) in [("test", "PROD", "postgres"), ("dev", "DEV", "postgres"), ("my", "MY", "mysql")] {
            let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
                "id": id, "name": name, "kind": kind, "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
            })).unwrap();
            h.app.state.store.save(&cfg).unwrap();
        }
        h.app.reload_conns();
        h.settle().await;
        h.table_tab("users", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(Action::GenerateTestData);
        let Some(crate::app::Popup::Input { title, .. }) = &h.app.popup else { panic!("no rows input") };
        assert!(title.contains("public.users"), "{title}");
        h.key("Esc");
        h.app.run_action(Action::CopyRelatedRows);
        let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no where input") };
        *input = tui_textarea::TextArea::from(["id = 1"]);
        h.key("Enter");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no depth") };
        assert_eq!(p.items.len(), 4);
        h.key("Enter");
        // Personal data: masked by default, below is as is
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no mask choice") };
        assert_eq!(p.items.len(), 2);
        h.key("Enter");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no target") };
        assert_eq!(p.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), vec!["DEV"]);
        let Some(crate::migrate::Flow::CopyTarget(b)) = &h.app.flow else { panic!("no flow") };
        assert_eq!((b.2.filter.as_str(), b.2.depth, b.2.mask), ("id = 1", 1, true));
    }

    /// Loading more that fails (#80) keeps the result unfinished ("+") and says why at the grid's edge, the GUI's words; scrolling
    /// on doesn't ask again
    #[tokio::test]
    async fn a_failed_load_more_stays_in_the_grid() {
        let mut h = Harness::new().await;
        let id = h.table_tab("users", &["id"], &[&["1"], &["2"]], &["id"]);
        {
            let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind else { unreachable!() };
            let g = tt.grid.as_mut().unwrap();
            (g.result.truncated, g.more_session) = (true, Some(tt.session.clone()));
        }
        h.key("Down");
        assert!(h.table(id).grid.as_ref().unwrap().loading_more);
        // While it loads, the bottom edge says so, in words that read without colors too
        assert!(h.screen(120, 30).contains("↓ Loading more rows…"), "{}", h.screen(120, 30));
        h.app.color = crate::theme::ColorMode::NoColor;
        assert!(h.screen(120, 30).contains("↓ Loading more rows…"));
        h.app.color = crate::theme::ColorMode::TrueColor;
        h.settle().await;
        let g = h.table(id).grid.as_ref().unwrap();
        let why = qurry_shared::errors::more_rows_failed("Session closed");
        assert_eq!((g.more_error.as_deref(), g.result.truncated, g.loading_more), (Some(why.as_str()), true, false));
        assert!(h.screen(120, 30).contains(&format!("✗ {why}")), "{}", h.screen(120, 30));
        assert!(h.screen(120, 30).contains("2+ rows"));
        h.key("Up");
        h.key("Down");
        assert!(!h.table(id).grid.as_ref().unwrap().loading_more, "not asked again");
    }

    /// On a result wider than the grid, the bottom edge's words stay whole beside the horizontal scrollbar (they share that edge)
    #[tokio::test]
    async fn the_load_more_words_stay_beside_the_horizontal_scrollbar() {
        let mut h = Harness::new().await;
        let cols: Vec<String> = (0..40).map(|i| format!("column_{i}")).collect();
        let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
        let row = vec!["value"; 40];
        let id = h.table_tab("wide", &cols, &[&row, &row], &["column_0"]);
        {
            let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind else { unreachable!() };
            let g = tt.grid.as_mut().unwrap();
            (g.result.truncated, g.more_session) = (true, Some(tt.session.clone()));
        }
        h.key("Down");
        assert!(h.table(id).grid.as_ref().unwrap().loading_more);
        let screen = h.screen(120, 30);
        let edge = screen.lines().find(|l| l.contains("Loading more")).unwrap_or_else(|| panic!("{screen}"));
        assert!(edge.contains("↓ Loading more rows…") && edge.contains('█'), "{edge}");
        h.settle().await;
        let screen = h.screen(120, 30);
        let edge = screen.lines().find(|l| l.contains('✗')).unwrap_or_else(|| panic!("{screen}"));
        let why = qurry_shared::errors::more_rows_failed("Session closed");
        assert!(edge.contains(&format!("✗ {why}")) && edge.contains('█'), "{edge}");
    }

    /// A load-more answer that arrives after the tab ran again belongs to the grid it replaced: dropped
    #[tokio::test]
    async fn a_load_more_answer_for_a_replaced_grid_is_dropped() {
        let mut h = Harness::new().await;
        let id = h.table_tab("users", &["id"], &[&["1"], &["2"]], &["id"]);
        let fresh = || {
            let mut g = crate::grid::Grid::new(qurry_core::db::QueryResult { columns: vec![qurry_core::db::ColumnInfo { name: "id".into(), type_name: "text".into() }], rows: vec![vec!["9".into()]], rows_affected: 0, truncated: true, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None });
            g.more_session = Some("s".into());
            g
        };
        {
            let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind else { unreachable!() };
            tt.grid = Some(fresh());
        }
        h.key("Down");
        assert!(h.table(id).grid.as_ref().unwrap().loading_more);
        // The tab ran again before the answer came
        {
            let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind else { unreachable!() };
            tt.grid = Some(fresh());
        }
        h.settle().await;
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!((g.more_error.as_deref(), g.rows(), g.result.truncated), (None, 1, true), "the new grid is untouched");
        // While the tab runs, no load-more is asked
        {
            let TabKind::Table(tt) = &mut h.app.tabs[h.app.active].kind else { unreachable!() };
            tt.running = true;
        }
        h.app.fetch_more();
        assert!(!h.table(id).grid.as_ref().unwrap().loading_more);
    }
}
