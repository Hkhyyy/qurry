use super::*;
use qurry_core::editable::{EditTarget, Editable, JoinEdit};

/// Where a grid's edits go (App::edit_to)
pub(crate) enum EditTo {
    /// One table: cells, and rows added and deleted
    Table(EditTarget),
    /// A join (#102): each cell to its table, found by that table's key; no rows added or deleted
    Join(JoinEdit),
}
use qurry_core::format::{column_width, ColumnWidths};
use qurry_core::store::TableRef;
use qurry_shared::errors;

/// What < > = and the palette's Fit All Columns / Reset Column Widths do to the grid's columns
#[derive(Clone, Copy)]
pub enum Resize {
    /// Narrow (-1) / widen (+1) the current column by one step
    Step(i64),
    Fit,
    FitAll,
    Reset,
}

/// The table a table tab shows, which its column widths are kept for (settings.json "columnWidths", shared with the GUI)
pub(super) fn table_ref(t: &Tab) -> Option<TableRef> {
    match &t.kind {
        TabKind::Table(tt) => Some(TableRef { source: t.conn.clone(), database: t.db.clone(), schema: tt.schema.clone(), table: tt.table.clone() }),
        _ => None,
    }
}

impl App {
    pub fn open_table(&mut self, conn: &str, db: String, schema: String, table: String, kind: String) {
        if let Some(i) = self.tabs.iter().position(|t| t.conn == conn && t.db == db && matches!(&t.kind, TabKind::Table(tt) if tt.schema == schema && tt.table == table)) {
            return self.activate(i);
        }
        let id = self.push_tab(conn, db, table.clone(), TabKind::Table(TableTab { schema, table, kind, where_: String::new(), order: String::new(), session: uuid::Uuid::new_v4().to_string(), grid: None, running: false, pk: vec![], error: None }));
        self.reload_table(id);
    }

    /// Before reloading: if there are unsaved edits, ask whether to discard them (a plain reload would silently drop them)
    pub fn reload_table_checked(&mut self, id: u64) {
        let n = match self.tab_mut(id).map(|t| &t.kind) {
            Some(TabKind::Table(tt)) => tt.grid.as_ref().map_or(0, |g| g.pending()),
            _ => 0,
        };
        if n == 0 {
            return self.reload_table(id);
        }
        self.popup = Some(Popup::Confirm { opened: None, sql: None, text: self.pending_text(n, "reload"), on_yes: Box::new(move |a| a.reload_table(id)) });
    }

    /// The question before a reload or a run drops a grid's pending edits
    pub(super) fn pending_text(&self, n: usize, then: &str) -> String {
        format!("{n} pending change(s) are not saved to the database ({} submits them). Discard them and {then}?", self.keys.label(Action::Submit))
    }

    pub fn reload_table(&mut self, id: u64) {
        let kind = match self.tab_mut(id) {
            Some(t) => t.conn.clone(),
            None => return,
        };
        let kind = self.kind_of(&kind);
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Table(tt) = &mut t.kind else { return };
        let q = |s: &str| sql::quote_ident(kind, s);
        let mut sqls = format!("SELECT * FROM {}.{}", q(&tt.schema), q(&tt.table));
        if !tt.where_.trim().is_empty() {
            sqls += &format!(" WHERE {}", tt.where_.trim());
        }
        if !tt.order.trim().is_empty() {
            sqls += &format!(" ORDER BY {}", tt.order.trim());
        }
        tt.running = true;
        tt.error = None;
        let order = tt.order.trim().to_string();
        let (session, schema, table) = (tt.session.clone(), tt.schema.clone(), tt.table.clone());
        let (t_target, t_schema, t_table) = (target.clone(), schema.clone(), table.clone());
        let st = self.state.clone();
        let wait_target = target.clone();
        self.spawn_wait(
            &wait_target,
            super::WaitFor::Table(id),
            async move {
                // Send both together (if the connection fails, waiting on each would take twice as long)
                let (cols, r) = tokio::join!(st.table_detail(target.clone(), schema, table), st.run_query(target, sqls, Some(500), Some(session)));
                let cols = cols.map(|d| d.columns).unwrap_or_default();
                let pk = cols.iter().filter(|c| c.is_pk).map(|c| c.name.clone()).collect::<Vec<_>>();
                let names: Vec<(String, String)> = cols.iter().map(|c| (c.name.clone(), c.data_type.clone())).collect();
                let loaded = (!cols.is_empty()).then(|| super::status_items::loaded(cols));
                let r = r.map_err(err);
                (pk, (names, loaded), r)
            },
            move |a, (pk, (cols, loaded), r), seen| {
                // The status line's Size and Submit's hint read the columns from here, fresh on each load; when they couldn't be
                // read, Size asks for them itself (status_items::table_cols)
                if let Some(l) = loaded {
                    a.table_cols.insert((t_target, t_schema, t_table), l);
                }
                // A table opened from the search cache was just connected: sync the explorer display
                let conn = a.tab_mut(id).map(|t| t.conn.clone()).unwrap_or_default();
                let tls_err = r.as_ref().err().cloned();
                let fresh = r.is_ok() && a.conn(&conn).is_some_and(|c| !c.connected);
                if fresh {
                    a.reload_conns();
                }
                let table = a.tab_mut(id).and_then(|t| table_ref(t));
                let saved = table.as_ref().map(|r| a.state.store.column_widths(r)).unwrap_or_default();
                let saved_layout = table.as_ref().map(|r| a.state.store.column_layout(r)).unwrap_or_default();
                let Some(t) = a.tab_mut(id) else { return };
                let TabKind::Table(tt) = &mut t.kind else { return };
                tt.running = false;
                tt.pk = pk;
                // The widths the tab has (also ones a failed save kept, until it closes) with the saved ones on top (the GUI's changes)
                let mut widths = tt.grid.as_ref().map(|g| g.user_widths.clone()).unwrap_or_default();
                widths.extend(saved);
                // The saved layout (the GUI's changes too); none saved: the tab's own, which a failed save kept
                let layout = match tt.grid.as_ref() {
                    Some(g) if saved_layout.is_empty() => g.layout.clone(),
                    _ => saved_layout,
                };
                match r {
                    Ok(res) => {
                        let n = res.rows.len();
                        let more = res.truncated;
                        let (row, col) = tt.grid.as_ref().map(|g| (g.row, g.col)).unwrap_or((0, 0));
                        let mut g = Grid::new(res);
                        g.set_user_widths(widths);
                        g.more_session = Some(tt.session.clone());
                        g.row = row.min(n.saturating_sub(1));
                        g.col = col.min(g.cols().saturating_sub(1));
                        g.set_layout(layout);
                        // Show in the header if ORDER BY is a single column ASC/DESC
                        g.sort = g.result.columns.iter().enumerate().find_map(|(i, c)| {
                            let q = sql::quote_ident(kind, &c.name);
                            if order == format!("{q} ASC") { Some((i, false)) } else if order == format!("{q} DESC") { Some((i, true)) } else { None }
                        });
                        tt.grid = Some(g);
                        a.status = format!("{n}{} rows", if more { "+" } else { "" });
                    }
                    Err(e) => {
                        // No results, so show an empty table (columns kept). If there are unsaved edits, leave that table as is
                        if !tt.grid.as_ref().is_some_and(|g| g.dirty()) {
                            let columns = match &tt.grid {
                                Some(g) => g.result.columns.clone(),
                                None => cols.into_iter().map(|(name, type_name)| qurry_core::db::ColumnInfo { name, type_name }).collect(),
                            };
                            let empty = qurry_core::db::QueryResult { columns, rows: vec![], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] };
                            let mut g = Grid::new(empty);
                            g.set_user_widths(widths);
                            g.set_layout(layout);
                            tt.grid = Some(g);
                        }
                        tt.error = Some(e.clone());
                        a.status = e.clone();
                        // It waited on a reconnect: if that failed (not the load itself), why, and r retries (the Message popup)
                        if let Some(rc) = seen {
                            a.reconnect_failed(&rc, &e, true);
                        }
                    }
                }
                if let Some(e) = tls_err {
                    a.offer_tls_opt_in(&conn.clone(), &e, move |a| a.reconnect_after_tls(conn, None));
                }
            },
        );
    }

    /// Reload table tabs still sitting on a failed query (with no unsaved edits)
    pub(super) fn retry_failed_tables(&mut self, conn: &str) {
        let ids: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| t.conn == conn && !t.stale && matches!(&t.kind, TabKind::Table(tt) if tt.error.is_some() && !tt.running && !tt.grid.as_ref().is_some_and(|g| g.dirty())))
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.reload_table(id);
        }
    }

    /// Table tab header sort: ORDER BY on the selected column ↑ → ↓ → off (the GUI's Sort via ORDER BY)
    pub fn sort_table(&mut self, id: u64) {
        let Some(conn) = self.tab_mut(id).map(|t| t.conn.clone()) else { return };
        let kind = self.kind_of(&conn);
        let Some(TabKind::Table(tt)) = self.tab_mut(id).map(|t| &mut t.kind) else { return };
        let Some(g) = &tt.grid else { return };
        if g.cols() == 0 {
            return;
        }
        if g.dirty() {
            self.status = "Save (⌃s) or revert (⌃z) pending changes before sorting".into();
            return;
        }
        let q = sql::quote_ident(kind, &g.result.columns[g.col].name);
        tt.order = if tt.order == format!("{q} ASC") {
            format!("{q} DESC")
        } else if tt.order == format!("{q} DESC") {
            String::new()
        } else {
            format!("{q} ASC")
        };
        self.reload_table(id);
    }

    pub fn open_ddl(&mut self, conn: &str, db: String, schema: String, name: String, kind: String) {
        let target = sql::target_id(conn, Some(&db));
        let st = self.state.clone();
        let (conn, db2) = (conn.to_string(), db.clone());
        let title = format!("{name} [DDL]");
        let ddl_of = Some((schema.clone(), name.clone()));
        let obj = kind.clone();
        self.spawn(async move { st.object_ddl(target, schema, name, kind).await.map_err(err) }, move |a, r| match r {
            Ok(text) => {
                if let Some(i) = a.tabs.iter().position(|t| t.title == title && t.conn == conn) {
                    a.activate(i);
                } else {
                    // A table also gets the Structure view (v), loaded only when it is shown
                    let tt = TextTab { text, kind: obj, ddl_of, ..Default::default() };
                    let structure_view = tt.has_structure() && a.ddl_structure;
                    let id = a.push_tab(&conn, db2, title, TabKind::Text(TextTab { structure_view, ..tt }));
                    if structure_view {
                        a.load_structure(id);
                    }
                }
            }
            Err(e) => a.status = format!("DDL failed: {e}"),
        });
    }

    /// DDL tab of a table: switch between the SQL and the Structure view (v); the choice is kept for the next DDL tab
    pub fn toggle_structure(&mut self, id: u64) {
        let Some(TabKind::Text(t)) = self.tab_mut(id).map(|t| &mut t.kind) else { return };
        if !t.has_structure() {
            self.status = "Only a table's DDL has a Structure view".into();
            return;
        }
        t.structure_view = !t.structure_view;
        t.scroll = 0;
        let (on, load) = (t.structure_view, t.structure_view && t.structure.is_none());
        self.ddl_structure = on;
        if load {
            self.load_structure(id);
        }
    }

    /// (Re)load a DDL tab's Structure view (restored tabs · r after a failure)
    pub fn load_structure(&mut self, id: u64) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Text(tt) = &mut t.kind else { return };
        let Some((schema, table)) = tt.ddl_of.clone().filter(|_| tt.has_structure()) else { return };
        tt.structure = None;
        let st = self.state.clone();
        self.spawn(async move { st.table_structure(target, schema, table).await.map_err(err) }, move |a, r| {
            if let Some(TabKind::Text(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                tt.structure = Some(r);
            }
        });
    }

    /// DDL tab, r: reload what it shows (the Structure view, or the DDL text)
    pub fn reload_ddl(&mut self, id: u64) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Text(tt) = &t.kind else { return };
        if tt.structure_view {
            return self.load_structure(id);
        }
        // A DDL tab (kind set) saved by an older build doesn't know its object; other text (diagram) has nothing to reload
        let Some((schema, name)) = tt.ddl_of.clone() else {
            if !tt.kind.is_empty() {
                self.status = "Reopen the DDL to reload".into();
            }
            return;
        };
        let (kind, st) = (tt.kind.clone(), self.state.clone());
        self.status = format!("Loading DDL of {schema}.{name}…");
        self.spawn(async move { st.object_ddl(target, schema, name, kind).await.map_err(err) }, move |a, r| match r {
            Ok(text) => {
                if let Some(TabKind::Text(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    tt.text = text;
                }
                a.status = "Ready".into();
            }
            Err(e) => a.status = format!("DDL failed: {e}"),
        });
    }

    // ───────── Table editing ─────────

    /// Where the active grid's edits go: a table tab's table, the table a console result comes from, or a join's tables
    /// (QueryResult::editable). In Manual mode or an open transaction a console's edits run in it (AppState::submit_edits).
    /// Err: why the grid can't be edited ("" when the tab has no editable grid at all)
    pub(crate) fn edit_to(&self) -> Result<EditTo, String> {
        let none = || String::new();
        let t = self.tabs.get(self.active).ok_or_else(none)?;
        match &t.kind {
            TabKind::Table(tt) => {
                let g = tt.grid.as_ref().ok_or_else(none)?;
                Ok(EditTo::Table(EditTarget { schema: tt.schema.clone(), table: tt.table.clone(), pk: tt.pk.clone(), columns: g.result.columns.iter().map(|c| c.name.clone()).collect() }))
            }
            TabKind::Console(c) => {
                let g = c.grid().ok_or_else(none)?;
                match &g.result.editable {
                    Some(Editable::Table(e)) => Ok(EditTo::Table(e.clone())),
                    Some(Editable::Join(j)) => Ok(EditTo::Join(j.clone())),
                    Some(Editable::ReadOnly(why)) => Err(why.clone()),
                    None => Err(errors::CONSOLE_READ_ONLY.into()),
                }
            }
            _ => Err(none()),
        }
    }

    /// The status line says why; None
    fn refuse(&mut self, why: String) -> Option<&mut Grid> {
        if !why.is_empty() {
            self.status = why;
        }
        None
    }

    /// The active grid when its current cell can be edited (a join: per table, its outer side where no row matched read-only);
    /// otherwise the status line says why
    pub(crate) fn edit_grid(&mut self) -> Option<&mut Grid> {
        match self.edit_to() {
            Ok(EditTo::Table(_)) => self.grid_mut(),
            Ok(EditTo::Join(j)) => {
                let g = self.grid_mut()?;
                let row = g.result.rows.get(g.row).cloned().unwrap_or_default();
                match j.cell(g.col, |i| row.get(i).is_none_or(serde_json::Value::is_null)) {
                    Ok(_) => self.grid_mut(),
                    Err(why) => self.refuse(why),
                }
            }
            Err(why) => self.refuse(why),
        }
    }

    /// The active grid when rows can be added or deleted in it (one table, not a join); otherwise the status line says why
    pub(crate) fn edit_rows(&mut self) -> Option<&mut Grid> {
        match self.edit_to() {
            Ok(EditTo::Table(_)) => self.grid_mut(),
            Ok(EditTo::Join(_)) => self.refuse(errors::EDIT_JOIN_ROWS.into()),
            Err(why) => self.refuse(why),
        }
    }

    /// Paste into the active grid from its current cell. A join's paste reaches only cells that can be edited and adds no rows
    pub(crate) fn paste_grid(&mut self, text: &str) {
        let join = match self.edit_to() {
            Ok(EditTo::Join(j)) => Some(j),
            _ => None,
        };
        let Some(g) = self.edit_grid().filter(|g| g.cols() > 0) else { return };
        if let Some(j) = join {
            let bad = g.paste_spots(text).into_iter().find_map(|(r, c)| match g.result.rows.get(r) {
                None => Some(errors::EDIT_JOIN_ROWS.to_string()),
                Some(row) => j.cell(c, |i| row.get(i).is_none_or(serde_json::Value::is_null)).err(),
            });
            if let Some(why) = bad {
                self.status = why;
                return;
            }
        }
        g.paste(text);
    }

    /// < > =, Fit All Columns, Reset Column Widths on a table tab or a result (the GUI's header menu and ⌃⌥← / ⌃⌥→). A table tab's
    /// widths are saved for its table, so the GUI shows them too; a console result keeps them until it runs again
    pub fn resize_columns(&mut self, how: Resize) {
        let Some(t) = self.tabs.get(self.active).filter(|t| matches!(t.kind, TabKind::Table(_) | TabKind::Console(_) | TabKind::Dba(_))) else {
            self.status = "Open a result or table to resize its columns".into();
            return;
        };
        let table = table_ref(t);
        let Some(g) = self.grid_mut().filter(|g| g.cols() > 0) else { return };
        let c = g.col;
        let name = g.result.columns[c].name.clone();
        let changed: ColumnWidths = match how {
            Resize::Step(s) => [g.set_width(c, column_width(g.widths[c].into(), s))].into(),
            Resize::Fit => [g.toggle_fit(c)].into(),
            Resize::FitAll => (0..g.cols()).map(|c| g.set_width(c, g.fit_width(c))).collect(),
            Resize::Reset => {
                g.set_user_widths(ColumnWidths::new());
                ColumnWidths::new()
            }
        };
        self.status = match (how, changed.iter().next()) {
            (Resize::Reset, _) => "Column widths are the computed ones again".into(),
            (Resize::FitAll, _) => "Every column fits its content".into(),
            (_, Some((_, 0))) => format!("{name}: its computed width again"),
            (_, Some((_, w))) => format!("{name}: {w} cells wide"),
            _ => String::new(),
        };
        let Some(table) = table else { return };
        let saved = match how {
            Resize::Reset => self.state.store.reset_column_widths(&table),
            _ => self.state.store.set_column_widths(&table, &changed).map(|_| ()),
        };
        if let Err(e) = saved {
            self.status = format!("Couldn't save the column widths ({e}). They apply until the tab closes; Reset Column Widths in the actions palette ({}) goes back to the computed widths.", self.keys.label(Action::Palette));
        }
    }

    pub fn submit(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let conn = self.conn(&t.conn);
        let ro = conn.map(|c| c.config.read_only).unwrap_or(false);
        let prod = conn.filter(|c| c.config.options.confirm_changes).map(|c| c.config.name.clone());
        let (id, target) = (t.id, t.target.clone());
        // A console's edits run in its transaction in Manual mode or with one open, and stay pending (AppState::submit_edits)
        let (g, tab, in_tx) = match &t.kind {
            TabKind::Table(tt) => (tt.grid.as_ref(), None, false),
            TabKind::Console(c) => (c.grid(), Some(c.session.clone()), c.tx_manual || c.in_tx),
            _ => (None, None, false),
        };
        let Some(g) = g else { return };
        if !g.dirty() {
            self.status = "No pending changes".into();
            return;
        }
        // A table's new rows: the table, for the hint when their key's sequence is behind (SeqFix)
        let (stmts, seq) = match self.edit_to() {
            Ok(EditTo::Table(to)) => (table_stmts(kind, &to, g), (g.added > 0).then_some(SeqFix { kind, schema: to.schema, table: to.table, pk: to.pk })),
            Ok(EditTo::Join(j)) => (join_stmts(kind, &j, g), None),
            Err(why) => (Err(why), None),
        };
        let stmts = match stmts {
            _ if ro => return self.status = "Data source is read-only".into(),
            Ok(s) => s,
            Err(why) => return self.status = why,
        };
        // Production asks first, listing the statements (each changes exactly one row)
        let gid = g.id;
        match prod {
            Some(name) => {
                let lines: Vec<String> = stmts.iter().map(|s| format!("  {}", s.chars().take(100).collect::<String>())).collect();
                let what = match in_tx {
                    true => format!("run {} change(s) in this console's transaction? They stay pending until you Commit or Roll back.", stmts.len()),
                    false => format!("save {} change(s)?", stmts.len()),
                };
                self.popup = Some(Popup::Confirm {
                    opened: None,
                    sql: Some(kind),
                    text: format!("Production data source \"{name}\" — {what}\n\n{}", lines.join("\n")),
                    on_yes: Box::new(move |a| a.submit_stmts(id, gid, target, tab, stmts, seq)),
                });
            }
            None => self.submit_stmts(id, gid, target, tab, stmts, seq),
        }
    }

    /// Save the statements Submit built for grid `gid` of tab `id`, then show the saved rows (reload the table, run the console's
    /// statement again). tab = the console's session: in Manual mode or with a transaction open they run in it and stay pending
    fn submit_stmts(&mut self, id: u64, gid: u64, target: String, tab: Option<String>, stmts: Vec<String>, seq: Option<SeqFix>) {
        let n = stmts.len();
        let st = self.state.clone();
        let fake = self.fake_db();
        let filled = seq.as_ref().and_then(|s| match self.table_cols.get(&(target.clone(), s.schema.clone(), s.table.clone())) {
            Some(super::TableCols::Loaded(_, f)) => Some(f.clone()),
            _ => None,
        });
        self.status = format!("Submitting {n} statement(s)…");
        self.spawn(
            async move {
                let r = match fake {
                    // Tests: the stand-in database gets the statements, in order
                    Some(f) => stmts.iter().try_fold(0, |n, s| f(s).map(|r| n + r.rows_affected)).map(|rows| qurry_core::app::BatchResult { rows, warning: None }),
                    None => st.submit_edits(target.clone(), tab.clone(), stmts).await.map_err(err),
                };
                // Pending now, or ended by the failure (a deadlock, a lost connection)
                let in_tx = match &tab {
                    Some(t) => st.tx_state(t.clone()).await,
                    None => false,
                };
                let fix = match (&r, seq) {
                    (Err(e), Some(s)) => s.fix(&st, target, e, filled).await,
                    _ => None,
                };
                (r.map_err(|e| match fix { Some(f) => format!("{e}\n\n{}\n{}", f.text, f.sql), None => e }), in_tx)
            },
            move |a, (r, in_tx)| {
                if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    c.in_tx = in_tx;
                }
                match r {
                    Ok(r) => {
                        let pending = if in_tx { " · pending until Commit / Roll back" } else { "" };
                        a.status = format!("{n} statement(s) submitted, {} row(s) affected{pending}", r.rows);
                        match a.tab_mut(id).map(|t| &t.kind) {
                            Some(TabKind::Table(_)) => a.reload_table(id),
                            // The console still shows that result: run its statement again, into its tab (a newer result stays as it is)
                            Some(TabKind::Console(c)) => {
                                if let Some(t) = c.results.shown_tab().filter(|t| t.grid.id == gid) {
                                    let at = result_tabs::RunAt { stmt: t.stmt, sql: t.sql.clone(), into: Some(t.id) };
                                    Self::in_console(id, move |a| a.run_stmts(vec![at.sql.clone()], vec![at], false, false))(a);
                                }
                            }
                            _ => {}
                        }
                        // Saved, but its result couldn't be logged: not a failure (submitting again would save it twice)
                        if let Some(w) = r.warning {
                            a.status.push_str(" · Warning: the Production log couldn't record it");
                            a.notice("Submitted, with a warning".into(), w);
                        }
                    }
                    Err(e) => a.notice("Submit failed".into(), e),
                }
            },
        );
    }
}

/// The table Submit adds rows to, for the hint under its error when their key's sequence is behind the table
/// (qurry_shared::errors::sequence_behind)
struct SeqFix {
    kind: DbKind,
    schema: String,
    table: String,
    pk: Vec<String>,
}

impl SeqFix {
    /// filled = the table's numbered columns as already read (TableCols); else they are read now, only when the error is a
    /// duplicate of a key the INSERT left out
    async fn fix(self, st: &qurry_core::app::AppState, target: String, e: &str, filled: Option<Vec<String>>) -> Option<qurry_shared::errors::ErrorFix> {
        let hint = |f: &[String]| qurry_shared::errors::sequence_behind(self.kind, e, &self.schema, &self.table, f);
        hint(&self.pk)?;
        let filled = match filled {
            Some(f) => f,
            None => st.table_detail(target, self.schema.clone(), self.table.clone()).await.ok()?.columns.into_iter().filter(|c| c.auto_increment).map(|c| c.name).collect(),
        };
        hint(&filled)
    }
}

/// The result rows' raw text, as qurry_shared::result_edit reads the key values
fn raw_rows(g: &Grid) -> Vec<Vec<Option<String>>> {
    g.result.rows.iter().map(|row| row.iter().map(raw_text).collect()).collect()
}

/// One table's statements (qurry_shared::result_edit::edit_statements, the GUI's rules): the grid's added rows are the last
/// `g.added` rows, each with the cells set on it. Err: the table has no key
fn table_stmts(kind: DbKind, to: &EditTarget, g: &Grid) -> Result<Vec<String>, String> {
    use qurry_shared::result_edit::edit_statements;
    let key_at: Vec<usize> = to.pk.iter().filter_map(|k| to.columns.iter().position(|c| c == k)).collect();
    let keyed_rows = g.edits.keys().map(|(r, _)| *r).chain(g.deleted.iter().copied());
    cut_guard(g, keyed_rows.flat_map(|r| key_at.iter().map(move |&k| (r, k))))?;
    let base = g.rows() - g.added;
    let mut rows = raw_rows(g);
    rows.truncate(base);
    let edits = g.edits.iter().filter(|((r, _), _)| *r < base).map(|(k, v)| (*k, v.clone())).collect();
    let deleted = g.deleted.iter().copied().filter(|r| *r < base).collect();
    let added: Vec<_> = (base..g.rows()).filter(|r| !g.deleted.contains(r)).map(|r| g.edits.range((r, 0)..(r + 1, 0)).map(|((_, c), v)| (*c, v.clone())).collect()).collect();
    edit_statements(kind, to, &rows, &edits, &deleted, &added)
}

/// A join's statements (#102, qurry_shared::result_edit::join_statements). Cells that can't be edited never got an edit (App::edit_grid)
fn join_stmts(kind: DbKind, j: &JoinEdit, g: &Grid) -> Result<Vec<String>, String> {
    use qurry_shared::result_edit::ColumnEdit;
    let key_cells = g.edits.keys().filter_map(|&(r, c)| match j.columns.get(c) {
        Some(ColumnEdit::To { table, .. }) => Some(j.tables[*table].pk_at.iter().map(move |&k| (r, k))),
        _ => None,
    });
    cut_guard(g, key_cells.flatten())?;
    Ok(qurry_shared::result_edit::join_statements(kind, j, &raw_rows(g), &g.edits))
}

/// Submit never writes a cut value's start (#183). An edit of a cut cell is a whole value (App::read_then reads it before the
/// editor opens; a paste or NULL is a value of its own), so what is left is a row found by a key cell that came cut: refused.
/// `cells` = the key cells Submit reads
fn cut_guard(g: &Grid, mut cells: impl Iterator<Item = (usize, usize)>) -> Result<(), String> {
    match cells.find(|k| g.cut.contains(k)) {
        Some((r, c)) => Err(format!("Not submitted: row {}'s key {} came cut, so the row can't be found. Set ⌃k › Grid Long Values… to 0 and run again", r + 1, g.result.columns[c].name)),
        None => Ok(()),
    }
}

// ───────── Cut values (#183): QueryResult::cut, read whole by the table's key ─────────

/// The grid a tab shows (a console's shown result)
fn grid_of(t: &Tab) -> Option<&Grid> {
    match &t.kind {
        TabKind::Table(tt) => tt.grid.as_ref(),
        TabKind::Console(c) => c.grid(),
        _ => None,
    }
}

/// The table a tab's cut values are read whole from, by its key; else why they can't be (what follows "Cut at N characters; ")
fn whole_from(t: &Tab, g: &Grid) -> Result<EditTarget, String> {
    let needs = "the whole value needs a primary key";
    match &t.kind {
        TabKind::Table(tt) if tt.pk.is_empty() => Err(format!("{needs} ({}.{} has none)", tt.schema, tt.table)),
        TabKind::Table(tt) => Ok(EditTarget { schema: tt.schema.clone(), table: tt.table.clone(), pk: tt.pk.clone(), columns: g.result.columns.iter().map(|c| c.name.clone()).collect() }),
        TabKind::Console(_) => match &g.result.editable {
            Some(Editable::Table(e)) if !e.pk.is_empty() => Ok(e.clone()),
            Some(Editable::Table(e)) => Err(format!("{needs} ({}.{} has none)", e.schema, e.table)),
            Some(Editable::Join(_)) => Err("whole values are read only in a result from one table. Open the table to edit it".into()),
            Some(Editable::ReadOnly(why)) => Err(format!("{needs}. {why}")),
            None => Err(format!("{needs}. {}", errors::CONSOLE_READ_ONLY)),
        },
        _ => Err(needs.into()),
    }
}

/// The result column of each key column of `to`
fn key_columns(to: &EditTarget) -> Vec<usize> {
    to.pk.iter().filter_map(|k| to.columns.iter().position(|c| c == k)).collect()
}

/// Why cut cell (r, c) of tab `t` can't be read whole; None: it can (its table has a key and none of the row's key cells came cut)
fn cut_reason(t: &Tab, g: &Grid, r: usize, c: usize) -> Option<String> {
    let at = format!("Cut at {} characters", g.value(r, c).map_or(0, |v| v.chars().count()));
    match whole_from(t, g) {
        Err(why) => Some(format!("{at}; {why}")),
        Ok(to) if key_columns(&to).iter().any(|&k| g.cut.contains(&(r, k))) => Some(format!("{at}; the whole value is read by the row's key, which is cut too")),
        Ok(_) => None,
    }
}

/// The status line and the value panel while a cut value is read whole
const LOADING: &str = "Loading the whole value…";

/// The value panel's line over a cut value: the whole value is on its way, or why it can't come
pub(crate) fn cut_note(t: &Tab) -> Option<String> {
    let g = grid_of(t)?;
    if !g.is_cut(g.row, g.col) {
        return None;
    }
    Some(match cut_reason(t, g, g.row, g.col) {
        Some(why) => why,
        None => match &g.unread {
            Some((at, why)) if *at == (g.row, g.col) => why.clone(),
            _ => LOADING.into(),
        },
    })
}

impl App {
    /// Read the active grid's cut `cells` whole (AppState::fetch_values by the table's key, in the tab's session, so a Manual
    /// console's own changes too): the values go into the grid's rows and the cells leave its cut set, wherever their rows are by
    /// then (found by their key). `then` runs once the answer is in, while that grid is still the active one; a row no longer
    /// in the table is said on the status line after it, and the cell under the cursor left cut keeps why (Grid::unread). Then
    /// the value panel reads the cell the cursor went to meanwhile. Cells of rows without a key (cut_reason) are left as they are
    pub(crate) fn fetch_full(&mut self, cells: Vec<(usize, usize)>, then: impl FnOnce(&mut App) + Send + 'static) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let Some(g) = grid_of(t) else { return };
        let Ok(to) = whole_from(t, g) else { return };
        let session = match &t.kind {
            TabKind::Table(tt) => tt.session.clone(),
            TabKind::Console(c) => c.session.clone(),
            _ => return,
        };
        let cells: Vec<(usize, usize)> = cells.into_iter().filter(|&(r, c)| g.is_cut(r, c) && cut_reason(t, g, r, c).is_none()).collect();
        if cells.is_empty() {
            return then(self);
        }
        let key_at = key_columns(&to);
        let rows: Vec<usize> = cells.iter().map(|x| x.0).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let cols: Vec<usize> = cells.iter().map(|x| x.1).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let keys: Vec<Vec<Option<String>>> = rows.iter().map(|&r| key_at.iter().map(|&k| g.result.rows[r].get(k).and_then(raw_text)).collect()).collect();
        let req = qurry_core::app::FullValues { schema: to.schema.clone(), table: to.table.clone(), key: to.pk.clone(), columns: cols.iter().map(|&c| to.columns[c].clone()).collect(), rows: keys.clone() };
        let (id, gid, target, table) = (t.id, g.id, t.target.clone(), format!("{}.{}", to.schema, to.table));
        if let Some(g) = self.grid_mut() {
            g.reading = true;
        }
        let (st, fake) = (self.state.clone(), self.fake_db());
        self.spawn(
            async move {
                match fake {
                    // Tests: the stand-in database answers a row per key it has, its place in the ask first (as the core's read)
                    Some(f) => f(&format!("-- fetch_values {}.{} {:?} by {:?}: {:?}", req.schema, req.table, req.columns, req.key, req.rows)).map(|q| {
                        let mut out = vec![None; req.rows.len()];
                        for row in q.rows {
                            if let Some(at) = row.first().and_then(Value::as_u64).and_then(|i| out.get_mut(i as usize)) {
                                *at = Some(row[1..].to_vec());
                            }
                        }
                        out
                    }),
                    None => st.fetch_values(target, session, req).await.map_err(err),
                }
            },
            move |a, r| {
                let active = a.tabs.get(a.active).is_some_and(|t| t.id == id);
                let grid = a.tab_mut(id).and_then(|t| match &mut t.kind {
                    TabKind::Table(tt) => tt.grid.as_mut(),
                    TabKind::Console(c) => c.results.tabs.iter_mut().map(|t| &mut t.grid).find(|g| g.id == gid),
                    _ => None,
                });
                // A run (or a reload) replaced the grid meanwhile: the answer is for the one it replaced
                let Some(g) = grid.filter(|g| g.id == gid) else { return };
                g.reading = false;
                let (got, err) = match r {
                    Ok(got) => (got, None),
                    Err(e) => (vec![None; rows.len()], Some(e)),
                };
                let mut missing = 0;
                // The asked cells this read left cut, where their rows are now
                let mut left = Vec::new();
                for (i, sent) in rows.iter().enumerate() {
                    if got[i].is_none() {
                        missing += 1;
                    }
                    // The row asked for, wherever a sort or a delete moved it since
                    let key_of = |r: usize| key_at.iter().map(|&k| g.result.rows[r].get(k).and_then(raw_text)).collect::<Vec<_>>();
                    let base = g.rows() - g.added;
                    let Some(r) = std::iter::once(*sent).chain(0..base).find(|&r| r < base && key_of(r) == keys[i]) else { continue };
                    for (j, &c) in cols.iter().enumerate().filter(|&(_, c)| cells.contains(&(*sent, *c))) {
                        match &got[i] {
                            Some(vals) if g.cut.remove(&(r, c)) => g.result.rows[r][c] = vals[j].clone(),
                            _ if g.is_cut(r, c) => left.push((r, c)),
                            _ => {}
                        }
                    }
                }
                // The asked cell under the cursor that this read left cut keeps why: the value panel says it and doesn't ask again
                let at = (g.row, g.col);
                if left.contains(&at) {
                    let why = match err {
                        Some(_) => "The whole value couldn't be read (see the status line): this is its start".into(),
                        None => format!("This row isn't in {table} any more: this is its start"),
                    };
                    g.unread = Some((at, why));
                }
                if let Some(e) = err {
                    a.status = format!("Couldn't read the whole value: {e}");
                } else {
                    if active && a.grid_mut().is_some_and(|g| g.id == gid) {
                        then(a);
                    } else if a.status == LOADING {
                        a.status = "Not done: the result changed before the whole value came".into();
                    }
                    if missing > 0 {
                        let (s, its) = if missing == 1 { (" isn't", "its") } else { ("s aren't", "their") };
                        let note = format!("{missing} row{s} in {table} any more: {its} values stay cut. Run the query again to see the rows as they are now.");
                        if !a.status.ends_with(&note) {
                            a.status = if a.status.is_empty() || a.status == LOADING { note } else { format!("{} · {note}", a.status) };
                        }
                    }
                    // Nothing came of it here (the cursor moved on, or ↩ waited on the value panel's read): the loading note goes
                    if a.status == LOADING {
                        a.status = "Ready".into();
                    }
                }
                // The cursor may have gone to another cut cell while this one loaded
                a.panel_whole();
            },
        );
    }

    /// Before an edit of cell (r, c) of the active grid: true when it isn't cut (go on). A cut one is read whole first and `then`
    /// runs once it is (the status line says it is loading), or the status line says why it can't be
    pub(crate) fn read_then(&mut self, r: usize, c: usize, then: impl FnOnce(&mut App) + Send + 'static) -> bool {
        let Some(t) = self.tabs.get(self.active) else { return true };
        let Some(g) = grid_of(t).filter(|g| g.is_cut(r, c)) else { return true };
        if let Some(why) = cut_reason(t, g, r, c) {
            self.status = why;
            return false;
        }
        // One read at a time: a second ↩ while it loads would open the editor again over what was typed. Said apart from
        // LOADING, which the value panel's read (its `then` opens nothing) turns to Ready
        if g.reading {
            self.status = "Still loading the whole value: edit it once it's in".into();
            return false;
        }
        self.status = LOADING.into();
        self.fetch_full(vec![(r, c)], move |a| {
            if a.grid_mut().is_some_and(|g| !g.is_cut(r, c) && (g.row, g.col) == (r, c)) {
                a.status = "Ready".into();
                then(a);
            }
        });
        false
    }

    /// A copy of the active grid's `cells` (Grid::cut_in_copy / cut_in_list): the cut ones are read whole first, then `copy` runs;
    /// cells of rows without a key are copied as they are, and the status line says how many and how to get them whole
    pub(crate) fn copy_whole(&mut self, cells: Vec<(usize, usize)>, copy: fn(&mut App)) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let Some(g) = grid_of(t) else { return };
        let (keyed, left): (Vec<_>, Vec<_>) = cells.into_iter().partition(|&(r, c)| cut_reason(t, g, r, c).is_none());
        let n = left.len();
        let note = (n > 0).then(|| {
            let (s, it) = if n == 1 { (" is", "it") } else { ("s are", "them") };
            format!("{n} value{s} cut (no key to fetch {it} whole); {} › Grid Long Values… raises the limit", self.keys.label(Action::Palette))
        });
        let then = move |a: &mut App| {
            copy(a);
            // The note first: a narrow status line cuts its end
            if let Some(n) = note {
                a.status = format!("{n} · {}", a.status);
            }
        };
        if keyed.is_empty() {
            return then(self);
        }
        self.status = LOADING.into();
        self.fetch_full(keyed, then);
    }

    /// The value panel on a cut cell: its whole value is read by itself, once while the cursor stays on it (Grid::unread)
    pub(crate) fn panel_whole(&mut self) {
        if let Some(g) = self.grid_mut().filter(|g| g.unread.as_ref().is_some_and(|(at, _)| *at != (g.row, g.col))) {
            g.unread = None;
        }
        if !self.value_panel {
            return;
        }
        let Some(t) = self.tabs.get(self.active) else { return };
        let Some(g) = grid_of(t).filter(|g| !g.reading && g.unread.is_none() && g.is_cut(g.row, g.col)) else { return };
        if cut_reason(t, g, g.row, g.col).is_none() {
            let cell = (g.row, g.col);
            self.fetch_full(vec![cell], |_| {});
        }
    }
}
