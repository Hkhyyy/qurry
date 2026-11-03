use super::*;

pub(super) fn grid_key(app: &mut App, k: KeyEvent) {
    // Esc: if there's a selection, clear just that first
    if k.code == KeyCode::Esc {
        if let Some(g) = app.grid_mut().filter(|g| g.anchor.is_some()) {
            g.anchor = None;
            return;
        }
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
                nav(app, k)
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
            KeyCode::Char('y') => {
                let s = tt.text.clone();
                copy(app, &s)
            }
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
            nav(app, k);
        }
        TabKind::Table(tt) => {
            match k.code {
                KeyCode::Char('w') => {
                    let w = tt.where_.clone();
                    let words = app.table_words(id, false);
                    app.input_words("WHERE", &w, words, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            tt.where_ = s;
                        }
                        a.reload_table_checked(id);
                    });
                }
                KeyCode::Char('o') => {
                    let o = tt.order.clone();
                    let words = app.table_words(id, true);
                    app.input_words("ORDER BY", &o, words, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            tt.order = s;
                        }
                        a.reload_table_checked(id);
                    });
                }
                KeyCode::Enter | KeyCode::F(2) => {
                    let Some(g) = &tt.grid else { return };
                    let (r, c) = (g.row, g.col);
                    if g.cols() == 0 {
                        return;
                    }
                    let name = g.result.columns[c].name.clone();
                    let cur = g.value(r, c).unwrap_or_default();
                    app.input(format!("{name} (row {})", r + 1), &cur, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            if let Some(g) = &mut tt.grid {
                                g.set_value(r, c, s);
                            }
                        }
                    });
                }
                KeyCode::Char('n') => app.run_action(Action::SetNull),
                KeyCode::Char('a') | KeyCode::Insert => app.run_action(Action::AddRow),
                KeyCode::Char('d') | KeyCode::Delete => app.run_action(Action::DeleteRow),
                KeyCode::Char('z') if k.modifiers.contains(KeyModifiers::CONTROL) => app.run_action(Action::Revert),
                KeyCode::Char('r') => app.reload_table_checked(id),
                KeyCode::Char('s') => app.sort_table(id),
                KeyCode::Char('x') => app.run_action(Action::ExportCsv),
                KeyCode::Char('/') => {
                    let w = tt.where_.clone();
                    let words = app.table_words(id, false);
                    app.input_words("WHERE", &w, words, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            tt.where_ = s;
                        }
                        a.reload_table_checked(id);
                    });
                }
                KeyCode::Esc => app.focus = Focus::Tree,
                _ => nav(app, k),
            }
        }
    }
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
            // TSV if a range is selected (pastes into cells in a sheet), otherwise the current cell
            let Some((r0, c0, r1, c1)) = g.sel_rect() else {
                let v = g.value(g.row, g.col).unwrap_or_else(|| "NULL".into());
                return copy(app, &v);
            };
            let text = g.selection_tsv();
            copy(app, &text);
            let (nr, nc) = (r1 - r0 + 1, c1 - c0 + 1);
            app.status = format!("Copied {nr} row{} × {nc} column{}", if nr == 1 { "" } else { "s" }, if nc == 1 { "" } else { "s" });
            return;
        }
        KeyCode::Char('Y') => return app.run_action(Action::CopyInList),
        KeyCode::Char('x') => return app.run_action(Action::ExportCsv),
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
}
