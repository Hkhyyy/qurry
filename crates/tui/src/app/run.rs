use super::*;

/// Each planned statement's editor rows (first, last): `text` is what was planned, its first line on editor row `row0`
fn stmt_rows(stmts: &[qurry_core::exec::Stmt], text: &str, row0: usize) -> Vec<(usize, usize)> {
    let row = |at: usize| row0 + text[..at.min(text.len())].matches('\n').count();
    stmts.iter().map(|s| (row(s.start), row(s.end))).collect()
}

/// Runs a console query on the session, or asks the test stand-in when there is one
async fn console_query(st: &AppState, fake: Option<FakeDb>, target: &str, q: String, limit: Option<usize>, session: &str) -> Result<QueryResult, String> {
    match fake {
        Some(f) => f(&q),
        None => st.run_query(target.into(), q, limit, Some(session.into())).await.map_err(err),
    }
}

/// Explain Analyze's statements before and after the one it runs (AppState::explain_wrap); the test fake follows the tab's transaction
async fn explain_wrap(st: &AppState, fake: Option<FakeDb>, target: &str, session: &str, in_tx: bool) -> Result<[&'static str; 2], String> {
    use qurry_core::session::{EXPLAIN_BEGIN, EXPLAIN_SAVEPOINT};
    match fake {
        Some(_) => Ok(if in_tx { EXPLAIN_SAVEPOINT } else { EXPLAIN_BEGIN }),
        None => st.explain_wrap(target.into(), session.into()).await.map_err(err),
    }
}

/// The Explain Analyze ask for (statement preview, reason) pairs. Several: each statement with its own reason (as the GUI lists them)
pub(crate) fn explain_analyze_ask_text(asks: &[(String, String)]) -> String {
    match asks {
        [(preview, why)] => format!("Explain Analyze really runs the statement ({why}):\n  {preview}\nRun it anyway?"),
        _ => format!(
            "Explain Analyze really runs the statements, and {} of them need a look:\n{}\nRun them anyway?",
            asks.len(),
            asks.iter().map(|(p, why)| format!("  {p}\n    {why}")).collect::<Vec<_>>().join("\n")
        ),
    }
}

impl App {
    // ───────── Execution ─────────

    pub fn execute(&mut self, explain: bool) {
        self.execute_checked(explain, false)
    }

    /// What to run follows the execution settings (self.exec). If it needs asking, open the picker (DataGrip)
    fn execute_checked(&mut self, explain: bool, confirmed: bool) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let id = t.id;
        let TabKind::Console(c) = &t.kind else {
            if let TabKind::Table(_) = t.kind {
                if explain {
                    self.status = "Explain Plan runs in a query console".into();
                } else {
                    self.reload_table_checked(id);
                }
            }
            return;
        };
        if c.running {
            return;
        }
        if let Some(((r0, c0), (r1, c1))) = c.editor.selection_range() {
            let sel = selected_text(c.editor.lines(), r0, c0, r1, c1);
            let stmts = qurry_core::exec::plan_selection(&sel, kind, self.exec);
            if !stmts.is_empty() {
                let rows = stmt_rows(&stmts, &sel, r0.min(r1));
                self.run_marked(sqls(stmts), rows, explain, confirmed);
            }
            return;
        }
        let (row, col) = c.editor.cursor();
        let text = c.editor.lines().join("\n");
        let plan = qurry_core::exec::plan(&text, byte_offset(c.editor.lines(), row, col), kind, self.exec);
        match plan {
            qurry_core::exec::Plan::Nothing => {}
            qurry_core::exec::Plan::Run(stmts) => {
                let rows = stmt_rows(&stmts, &text, 0);
                self.run_marked(sqls(stmts), rows, explain, confirmed)
            }
            qurry_core::exec::Plan::Ask(opts) => {
                let items = opts.into_iter().map(|(label, stmts)| PItem { label, hint: String::new(), action: PAction::Run { rows: stmt_rows(&stmts, &text, 0), stmts: sqls(stmts), explain } }).collect();
                self.popup = Some(Popup::Palette(Palette { title: "Execute".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
            }
        }
    }

    /// Execution settings (DataGrip Settings › Database › Query Execution). Picking applies immediately and the window stays open
    pub fn palette_exec(&mut self) {
        use qurry_core::exec::{ExecSettings, Inside, Outside, Selection};
        let e = self.exec;
        let mut items = vec![];
        let mut add = |group: &str, on: bool, label: &str, v: ExecSettings| items.push(PItem { label: format!("{} {label}", if on { "●" } else { " " }), hint: group.into(), action: PAction::Exec(v) });
        for (v, l) in [(Inside::Ask, "Ask what to execute"), (Inside::Smallest, "Smallest statement"), (Inside::Whole, "Whole script"), (Inside::FromCaret, "Everything from caret")] {
            add("caret inside statement", e.inside == v, l, ExecSettings { inside: v, ..e });
        }
        for (v, l) in [(Outside::Nothing, "Nothing"), (Outside::Whole, "Whole script"), (Outside::Below, "Everything below caret")] {
            add("caret outside statement", e.outside == v, l, ExecSettings { outside: v, ..e });
        }
        for (v, l) in [(Selection::Single, "Exactly as a single statement"), (Selection::Separate, "Exactly as separate statements")] {
            add("selection", e.selection == v, l, ExecSettings { selection: v, ..e });
        }
        self.popup = Some(Popup::Palette(Palette { title: "Query Execution Settings".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// The test stand-in for the database (always None outside tests)
    fn fake_db(&self) -> Option<FakeDb> {
        #[cfg(test)]
        return self.fake_db;
        #[cfg(not(test))]
        None
    }

    /// Runs statements planned over the console's text, each on its editor rows (first, last): they get gutter marks as they run
    pub fn run_marked(&mut self, stmts: Vec<String>, rows: Vec<(usize, usize)>, explain: bool, confirmed: bool) {
        if let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind)
            && !c.running
        {
            c.marks.start(c.editor.lines(), &rows);
        }
        self.run_stmts(stmts, explain, confirmed)
    }

    /// confirmed = whether a dangerous statement (UPDATE/DELETE without WHERE, DROP, TRUNCATE) was confirmed
    pub fn run_stmts(&mut self, stmts: Vec<String>, explain: bool, confirmed: bool) {
        self.run_stmts_checked(stmts, explain, confirmed, confirmed)
    }

    /// on_yes of a confirm that runs statements: they run in the console that asked (tab id), shown again, never in whichever tab
    /// is active when it is answered (that may be another source, whose own checks would be skipped). Nothing runs (and the status
    /// says why) while it is busy, once it is closed or while it is in another workspace
    fn in_console(id: u64, run: impl FnOnce(&mut App) + 'static) -> Box<dyn FnOnce(&mut App)> {
        Box::new(move |a| match a.tab_index(id) {
            Some(i) if matches!(&a.tabs[i].kind, TabKind::Console(c) if c.running) => a.status = "The console is busy: nothing ran. Run it again when it finishes.".into(),
            Some(i) => {
                a.activate(i);
                run(a)
            }
            None if a.tab_mut(id).is_some() => a.status = "The console is in another workspace: nothing ran".into(),
            None => a.status = "The console was closed: nothing ran".into(),
        })
    }

    /// unsafe_ok = dangerous statement confirmed, prod_ok = Production confirmation (rows to change) given
    fn run_stmts_checked(&mut self, stmts: Vec<String>, explain: bool, confirmed: bool, prod_ok: bool) {
        let analyze = explain && self.analyze_next;
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let id = t.id;
        let target = t.target.clone();
        let conn_name = self.conn(&t.conn).map(|c| c.config.name.clone()).unwrap_or_default();
        // History (same log as the GUI)
        let (hist_conn, hist_db, hist) = (t.conn.clone(), t.db.clone(), self.state.clone());
        let prod = self.tabs.get(self.active).and_then(|t| self.conn(&t.conn)).filter(|c| c.config.options.confirm_changes).map(|c| (c.config.name.clone(), self.tabs[self.active].target.clone()));
        let watch = prod.is_some();
        let t = &mut self.tabs[self.active];
        let TabKind::Console(c) = &mut t.kind else { return };
        if c.running || stmts.is_empty() {
            return;
        }
        if !confirmed
            && !explain
            && let Some((why, s)) = stmts.iter().find_map(|s| qurry_core::guard::unsafe_statement(s, kind).map(|w| (w, s.clone())))
        {
            let preview: String = s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
            self.popup = Some(Popup::Confirm { opened: None, text: format!("{why}:\n  {preview}\nRun it anyway?"), on_yes: Self::in_console(id, move |a| a.run_stmts_checked(stmts, explain, true, prod_ok)) });
            return;
        }
        // Production Data Source: count the rows to change, show it, and ask (counting is SELECT count(*) — changes nothing)
        if let Some((name, target)) = prod
            && !prod_ok
            && !explain
        {
            let writes: Vec<String> = stmts.iter().filter(|s| !qurry_core::guard::is_read(s, kind)).cloned().collect();
            if !writes.is_empty() {
                let st = self.state.clone();
                let fake = self.fake_db();
                self.status = "Counting the rows this will change…".into();
                self.spawn(
                    async move {
                        let mut out = vec![];
                        for w in writes {
                            let n = match fake {
                                Some(f) => match qurry_core::guard::impact_count_sql(&w, kind) {
                                    Some(q) => f(&q).map(|r| qurry_core::guard::impact_count(&r)).map_err(qurry_core::error::AppError::Msg),
                                    None => Ok(None),
                                },
                                None => st.preview_impact(target.clone(), w.clone()).await,
                            };
                            out.push((w, n));
                        }
                        out
                    },
                    move |a, counts| {
                        let lines: Vec<String> = counts
                            .iter()
                            .map(|(s, n)| {
                                let head: String = s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(70).collect();
                                let n = match n {
                                    Ok(Some(n)) => format!("{} row{}", crate::transfer::thousands(*n as u64), if *n == 1 { "" } else { "s" }),
                                    Ok(None) if qurry_core::guard::is_ddl(s, kind) => "schema change".into(),
                                    Ok(None) => "can't count this statement".into(),
                                    Err(e) => format!("can't count: {}", e.to_string().lines().next().unwrap_or("")),
                                };
                                format!("  {head}\n    → {n}")
                            })
                            .collect();
                        a.status.clear();
                        a.open_after_notices(Popup::Confirm {
                            opened: None,
                            text: format!("Production data source \"{name}\" — this changes data:\n\n{}\n\nRun it?", lines.join("\n")),
                            on_yes: Self::in_console(id, move |a| a.run_stmts_checked(stmts, explain, true, true)),
                        });
                    },
                );
                return;
            }
        }
        // EXPLAIN ANALYZE actually runs the statement. PostgreSQL·MySQL run it inside a transaction and roll back (below); drivers may
        // not support transactions. Ask when the rollback may not undo it all or nothing does (guard::explain_analyze_ask, the GUI's rule and words)
        let wraps = matches!(kind, DbKind::Postgres | DbKind::Mysql);
        let asks: Vec<(String, String)> = match analyze && !confirmed {
            true => stmts.iter().filter_map(|s| qurry_core::guard::explain_analyze_ask(s, kind).map(|w| (s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect(), w))).collect(),
            false => vec![],
        };
        if !asks.is_empty() {
            self.popup = Some(Popup::Confirm {
                opened: None,
                text: explain_analyze_ask_text(&asks),
                on_yes: Self::in_console(id, move |a| {
                    a.analyze_next = true;
                    a.run_stmts(stmts, explain, true);
                    a.analyze_next = false;
                }),
            });
            return;
        }
        c.running = true;
        // While running, show Output (the SQL being run instead of the previous results)
        c.show_output = true;
        let session = c.session.clone();
        let in_tx0 = c.in_tx;
        let st = self.state.clone();
        let stmts2 = stmts.clone();
        let tx = self.tx.clone();
        let echo_name = conn_name.clone();
        let fake = self.fake_db();
        self.status = "Executing…".into();
        self.spawn(
            async move {
                let mut out = vec![];
                // A statement's gutter mark, set as it starts and finishes
                let mark = |i: usize, state: MarkState| {
                    let _ = tx.send(Box::new(move |a: &mut App| {
                        if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            c.marks.set(c.editor.lines(), i, state);
                        }
                    }));
                };
                for (i, s) in stmts2.into_iter().enumerate() {
                    // Put the running statement in Output right away (on one line, truncated if too long), and its gutter mark
                    let echo: String = s.replace('\n', " ").chars().take(300).collect();
                    let line = format!("{echo_name}> {echo}{}", if s.chars().count() > 300 { "…" } else { "" });
                    let _ = tx.send(Box::new(move |a: &mut App| {
                        if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            c.log(false, line);
                        }
                    }));
                    mark(i, MarkState::Running);
                    let q = if explain {
                        let bare = s.trim_end_matches(';');
                        match (kind, analyze) {
                            // PG: fetch as JSON and turn it into a tree table (% · warnings)
                            (DbKind::Postgres, false) => format!("EXPLAIN (FORMAT JSON) {bare}"),
                            (DbKind::Postgres, true) => format!("EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) {bare}"),
                            (DbKind::Mysql, false) => format!("EXPLAIN {bare}"),
                            (DbKind::Mysql, true) => format!("EXPLAIN ANALYZE {bare}"),
                            (DbKind::Redis, _) => s.clone(),
                            // Drivers: most accept EXPLAIN (SQL Server uses SHOWPLAN, so it runs as is)
                            (_, true) => format!("EXPLAIN ANALYZE {bare}"),
                            (_, false) => format!("EXPLAIN {bare}"),
                        }
                    } else {
                        s.clone()
                    };
                    // Explain Analyze: run inside a transaction (a savepoint if one may be open: AppState::explain_wrap) and always roll
                    // back. Reads too: a function a SELECT calls may write
                    let dry = analyze && wraps;
                    let mut wrap = qurry_core::session::EXPLAIN_BEGIN;
                    if dry {
                        let begun = match explain_wrap(&st, fake, &target, &session, in_tx0).await {
                            Ok(w) => {
                                wrap = w;
                                console_query(&st, fake, &target, w[0].into(), None, &session).await
                            }
                            Err(e) => Err(e),
                        };
                        if let Err(e) = begun {
                            mark(i, MarkState::Err);
                            out.push((s, Err(e), false));
                            break;
                        }
                    }
                    let r = console_query(&st, fake, &target, q, Some(500), &session).await;
                    let r = if dry {
                        match console_query(&st, fake, &target, wrap[1].into(), None, &session).await {
                            Ok(back) => r.map(|res| qurry_core::db::QueryResult { in_transaction: back.in_transaction, ..res }),
                            Err(e) => Err(format!("Roll back after Explain Analyze failed: {e}")),
                        }
                    } else {
                        r
                    };
                    let stop = r.is_err();
                    mark(i, if stop { MarkState::Err } else { MarkState::Ok });
                    out.push((s, r, dry));
                    if stop {
                        break;
                    }
                }
                out
            },
            move |a, out| {
                let Some(t) = a.tab_mut(id) else { return };
                let TabKind::Console(c) = &mut t.kind else { return };
                c.running = false;
                let mut status = String::new();
                let n = out.len();
                // Changing statements already committed in Auto mode (reported on failure midway: earlier ones are not rolled back)
                let mut committed = 0;
                let mut warned = 0;
                let mut failed = false;
                let mut tls_err = None;
                let ddl = out.iter().any(|(s, r, dry)| r.is_ok() && !dry && !explain && qurry_core::guard::is_ddl(s, kind));
                for (i, (s, r, dry)) in out.into_iter().enumerate() {
                    if r.is_ok() {
                        let _ = hist.history.add(&hist_conn, Some(&hist_db), &s);
                    }
                    match r {
                        Ok(res) => {
                            // PG execution plan: indented operations · share (%) · warnings table
                            let res = match (explain && kind == DbKind::Postgres, res.rows.first().and_then(|r| r.first())) {
                                (true, Some(v)) => match qurry_core::plan::parse_pg(&crate::grid::raw_text(v).unwrap_or_default()) {
                                    Ok(p) => qurry_core::db::QueryResult { elapsed_ms: res.elapsed_ms, in_transaction: res.in_transaction, ..qurry_core::plan::to_result(&p) },
                                    Err(_) => res,
                                },
                                _ => res,
                            };
                            c.in_tx = res.in_transaction;
                            let warning = res.warning.clone();
                            if dry {
                                // A warning, not an error: it ran (Output draws WARNING lines in the warning colour, as the GUI does)
                                if let Some((warn, note)) = qurry_core::guard::explain_analyze_note(&s, kind) {
                                    c.log(false, format!("{}Explain Analyze {note}", if warn { WARNING } else { "" }));
                                }
                            } else if !res.in_transaction && !qurry_core::guard::is_read(&s, kind) {
                                committed += 1;
                            }
                            if res.has_result_set {
                                let msg = format!("{}{} row(s) retrieved in {} ms", res.rows.len(), if res.truncated { "+" } else { "" }, res.elapsed_ms);
                                c.log(false, msg.clone());
                                status = msg;
                                c.result_title = format!("Result {}: {}", i + 1, s.split_whitespace().take(6).collect::<Vec<_>>().join(" "));
                                let mut g = Grid::new(res);
                                g.more_session = Some(c.session.clone());
                                c.grid = Some(g);
                                c.show_output = false;
                            } else {
                                let msg = format!("{} row(s) affected in {} ms", res.rows_affected, res.elapsed_ms);
                                c.log(false, msg.clone());
                                // Production: announce if undo SQL was recorded (production audit log)
                                if watch && !dry && res.rows_affected > 0 && hist.audit.list(Some(&hist_conn), "", 1).first().is_some_and(|e| e.undo.is_some()) {
                                    c.log(false, "Undo saved — ⌃k › Production Log…".into());
                                }
                                status = msg;
                                if i + 1 == n && c.grid.is_none() {
                                    c.show_output = true;
                                }
                            }
                            // It ran, but its result couldn't be logged: not a failure (don't run it again). Several statements: counted below (as the GUI does)
                            if let Some(w) = warning {
                                warned += 1;
                                if n == 1 {
                                    status = format!("Warning: {}", w.lines().next().unwrap_or(""));
                                }
                                c.log(false, format!("{WARNING}{w}"));
                            }
                        }
                        Err(e) => {
                            c.log(true, e.clone());
                            tls_err = Some(e.clone());
                            if committed > 0 {
                                c.log(true, format!("Note: {committed} earlier statement(s) of this run were already committed (Auto mode) and were not rolled back"));
                            }
                            c.show_output = true;
                            failed = true;
                            status = format!("Error: {}", e.lines().next().unwrap_or(""));
                        }
                    }
                }
                if warned > 0 && n > 1 {
                    status = format!("{status} · {warned} with a warning (see Output)");
                }
                a.status = status;
                // If tables·columns were created, changed, or dropped, show it in the explorer right away
                if ddl {
                    a.schema_changed(&hist_conn, &hist_db);
                }
                // Even on error the session may have an open transaction (e.g. first Manual statement failed) → sync the Commit/Roll back display
                if failed {
                    a.sync_tx(id);
                }
                if let Some(e) = tls_err {
                    let conn = hist_conn.clone();
                    a.offer_tls_opt_in(&hist_conn, &e, move |a| a.reconnect_after_tls(conn, Some(id)));
                }
            },
        );
    }

    /// Read the console session's transaction state into the tab (after an error, the Commit / Roll back display may be stale)
    fn sync_tx(&mut self, id: u64) {
        let st = self.state.clone();
        let session = self.tab_mut(id).and_then(|t| if let TabKind::Console(c) = &t.kind { Some(c.session.clone()) } else { None });
        if let Some(session) = session {
            self.spawn(async move { st.tx_state(session).await }, move |a, in_tx| {
                if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    c.in_tx = in_tx;
                }
            });
        }
    }

    pub fn fetch_more(&mut self) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let id = t.id;
        // A run in the tab is replacing the result: a load-more now would be answered for the old one
        let g = match &mut t.kind {
            TabKind::Console(c) if !c.running => c.grid.as_mut(),
            TabKind::Table(tt) if !tt.running => tt.grid.as_mut(),
            _ => None,
        };
        let Some(g) = g else { return };
        if !g.wants_more() {
            return;
        }
        g.loading_more = true;
        let sent = g.id;
        let session = g.more_session.clone().unwrap();
        let st = self.state.clone();
        self.spawn(async move { st.fetch_more(session, Some(500)).await.map_err(err) }, move |a, r| {
            let Some(t) = a.tab_mut(id) else { return };
            let g = match &mut t.kind {
                TabKind::Console(c) => c.grid.as_mut(),
                TabKind::Table(tt) => tt.grid.as_mut(),
                _ => None,
            };
            // The tab ran again meanwhile: the answer is for the grid it replaced
            let Some(g) = g.filter(|g| g.id == sent) else { return };
            g.loading_more = false;
            match r {
                Ok(more) => {
                    g.append(more);
                    a.status = if g.result.truncated && g.rows() >= crate::grid::MAX_ROWS {
                        format!("Showing the first {} rows — export to CSV (x) for all of them", g.rows())
                    } else {
                        format!("{}{} rows", g.rows(), if g.result.truncated { "+" } else { "" })
                    };
                }
                Err(e) => {
                    // The result isn't complete: keep its "+" and say at the grid's edge why no more rows came
                    let why = qurry_shared::errors::more_rows_failed(&e);
                    g.more_error = Some(why.clone());
                    a.status = why;
                }
            }
        });
    }

    pub fn cancel(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let session = match &t.kind {
            TabKind::Console(c) if c.running => c.session.clone(),
            TabKind::Table(tt) if tt.running => tt.session.clone(),
            _ => return,
        };
        let st = self.state.clone();
        self.spawn(async move { st.cancel_query(session).await.map_err(err) }, |a, r| {
            a.status = match r {
                Ok(true) => "Cancel requested".into(),
                Ok(false) => "Nothing to cancel".into(),
                Err(e) => e,
            }
        });
    }

    pub fn tx(&mut self, what: Action) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let id = t.id;
        let target = t.target.clone();
        let TabKind::Console(c) = &mut t.kind else { return };
        let session = c.session.clone();
        let st = self.state.clone();
        match what {
            Action::TxToggle => {
                let manual = !c.tx_manual;
                self.spawn(async move { st.set_tx_mode(target, session, manual).await.map_err(err) }, move |a, r| match r {
                    Ok(()) => {
                        if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            c.tx_manual = manual;
                            if !manual {
                                c.in_tx = false;
                            }
                        }
                        a.status = format!("Transaction mode: {}", if manual { "Manual" } else { "Auto" });
                    }
                    Err(e) => a.status = e,
                });
            }
            Action::Commit | Action::Rollback => {
                let commit = what == Action::Commit;
                self.spawn(
                    async move { if commit { st.commit_tx(session).await } else { st.rollback_tx(session).await }.map_err(err) },
                    move |a, r| match r {
                        Ok(warning) => {
                            let done = if commit { "Committed" } else { "Rolled back" };
                            if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                                c.in_tx = false;
                                c.log(false, done.into());
                                if let Some(w) = &warning {
                                    c.log(false, format!("{WARNING}{w}"));
                                }
                            }
                            a.status = match warning {
                                Some(w) => format!("{done} · Warning: {}", w.lines().next().unwrap_or("")),
                                None => done.into(),
                            };
                        }
                        Err(e) => {
                            // In Output too: the status bar cuts it off, and a COMMIT on a lost connection says there to check the data
                            if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                                c.log(true, e.clone());
                                c.show_output = true;
                            }
                            // A failed COMMIT gets a notice to acknowledge, as in the GUI (not saved, or outcome unknown). A slow one
                            // can return while another popup is open: the notice goes over it
                            if commit {
                                a.notice(qurry_core::audit::commit_failed_title(&e).into(), e.clone());
                            }
                            a.status = e;
                            a.sync_tx(id);
                        }
                    },
                );
            }
            _ => {}
        }
    }

    /// It ran, but the Production log couldn't record it: a popup, not a failure (running it again would repeat it)
    pub(crate) fn warn_ran(&mut self, warning: Option<String>) {
        if let Some(w) = warning {
            self.notice("Ran, with a warning".into(), w);
        }
    }
}
