use super::*;
use qurry_core::editable::{EditTarget, Editable};
use qurry_shared::errors;

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
        self.popup = Some(Popup::Confirm { opened: None, text: self.pending_text(n, "reload"), on_yes: Box::new(move |a| a.reload_table(id)) });
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
        let st = self.state.clone();
        self.spawn(
            async move {
                // Send both together (if the connection fails, waiting on each would take twice as long)
                let (cols, r) = tokio::join!(st.table_detail(target.clone(), schema, table), st.run_query(target, sqls, Some(500), Some(session)));
                let cols = cols.map(|d| d.columns).unwrap_or_default();
                let pk = cols.iter().filter(|c| c.is_pk).map(|c| c.name.clone()).collect::<Vec<_>>();
                let cols: Vec<(String, String)> = cols.into_iter().map(|c| (c.name, c.data_type)).collect();
                let r = r.map_err(err);
                (pk, cols, r)
            },
            move |a, (pk, cols, r)| {
                // A table opened from the search cache was just connected: sync the explorer display
                let conn = a.tab_mut(id).map(|t| t.conn.clone()).unwrap_or_default();
                let tls_err = r.as_ref().err().cloned();
                let fresh = r.is_ok() && a.conn(&conn).is_some_and(|c| !c.connected);
                if fresh {
                    a.reload_conns();
                }
                let Some(t) = a.tab_mut(id) else { return };
                let TabKind::Table(tt) = &mut t.kind else { return };
                tt.running = false;
                tt.pk = pk;
                match r {
                    Ok(res) => {
                        let n = res.rows.len();
                        let more = res.truncated;
                        let (row, col) = tt.grid.as_ref().map(|g| (g.row, g.col)).unwrap_or((0, 0));
                        let mut g = Grid::new(res);
                        g.more_session = Some(tt.session.clone());
                        g.row = row.min(n.saturating_sub(1));
                        g.col = col.min(g.cols().saturating_sub(1));
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
                            let empty = qurry_core::db::QueryResult { columns, rows: vec![], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None };
                            tt.grid = Some(Grid::new(empty));
                        }
                        tt.error = Some(e.clone());
                        a.status = e;
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

    /// Where the active grid's edits go: a table tab's table, or the table a console result comes from (QueryResult::editable).
    /// Err: why the grid can't be edited ("" when the tab has no editable grid at all)
    pub(crate) fn edit_to(&self) -> Result<EditTarget, String> {
        let none = || String::new();
        let t = self.tabs.get(self.active).ok_or_else(none)?;
        match &t.kind {
            TabKind::Table(tt) => {
                let g = tt.grid.as_ref().ok_or_else(none)?;
                Ok(EditTarget { schema: tt.schema.clone(), table: tt.table.clone(), pk: tt.pk.clone(), columns: g.result.columns.iter().map(|c| c.name.clone()).collect() })
            }
            TabKind::Console(c) => {
                let g = c.grid.as_ref().filter(|_| !c.show_output).ok_or_else(none)?;
                match &g.result.editable {
                    // Edits save in a transaction of their own (execute_batch): not while the console's transaction is open
                    Some(Editable::Table(_)) if c.tx_manual || c.in_tx => Err(errors::CONSOLE_IN_TX.into()),
                    Some(Editable::Table(e)) => Ok(e.clone()),
                    Some(Editable::ReadOnly(why)) => Err(why.clone()),
                    None => Err(errors::CONSOLE_READ_ONLY.into()),
                }
            }
            _ => Err(none()),
        }
    }

    /// The active grid when its rows can be edited; otherwise the status line says why
    pub(crate) fn edit_grid(&mut self) -> Option<&mut Grid> {
        match self.edit_to() {
            Ok(_) => self.grid_mut(),
            Err(why) => {
                if !why.is_empty() {
                    self.status = why;
                }
                None
            }
        }
    }

    pub fn submit(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let conn = self.conn(&t.conn);
        let ro = conn.map(|c| c.config.read_only).unwrap_or(false);
        let prod = conn.filter(|c| c.config.options.confirm_changes).map(|c| c.config.name.clone());
        let (id, target) = (t.id, t.target.clone());
        let Some(g) = (match &t.kind {
            TabKind::Table(tt) => tt.grid.as_ref(),
            TabKind::Console(c) => c.grid.as_ref(),
            _ => None,
        }) else {
            return;
        };
        if !g.dirty() {
            self.status = "No pending changes".into();
            return;
        }
        let to = match self.edit_to() {
            Ok(to) => to,
            Err(why) => return self.status = why,
        };
        if ro {
            self.status = "Data source is read-only".into();
            return;
        }
        let q = |s: &str| sql::quote_ident(kind, s);
        let table = format!("{}.{}", q(&to.schema), q(&to.table));
        // The table column of each result column (a console result's aliases): what the SQL names
        let col = |i: usize| q(&to.columns[i]);
        let pk_idx: Vec<usize> = to.pk.iter().filter_map(|p| to.columns.iter().position(|c| c == p)).collect();
        let where_of = |r: usize| -> Option<String> {
            if pk_idx.is_empty() || pk_idx.len() != to.pk.len() {
                return None;
            }
            Some(pk_idx.iter().map(|&i| format!("{} = {}", col(i), sql::literal(kind, raw_text(&g.result.rows[r][i]).as_deref()))).collect::<Vec<_>>().join(" AND "))
        };
        let mut stmts = vec![];
        let base = g.rows() - g.added;
        let mut rows_with_edits: Vec<usize> = g.edits.keys().map(|(r, _)| *r).collect();
        rows_with_edits.dedup();
        for r in rows_with_edits.iter().copied().filter(|r| *r < base && !g.deleted.contains(r)) {
            let sets: Vec<String> = g.edits.iter().filter(|((rr, _), _)| *rr == r).map(|((_, c), v)| format!("{} = {}", col(*c), sql::literal(kind, v.as_deref()))).collect();
            match where_of(r) {
                Some(w) => stmts.push(format!("UPDATE {table} SET {} WHERE {w}", sets.join(", "))),
                None => {
                    self.status = "Table has no primary key — cannot update rows".into();
                    return;
                }
            }
        }
        for &r in &g.deleted {
            if r >= base {
                continue;
            }
            match where_of(r) {
                Some(w) => stmts.push(format!("DELETE FROM {table} WHERE {w}")),
                None => {
                    self.status = "Table has no primary key — cannot delete rows".into();
                    return;
                }
            }
        }
        for r in base..g.rows() {
            if g.deleted.contains(&r) {
                continue;
            }
            let set: Vec<(usize, &Option<String>)> = g.edits.iter().filter(|((rr, _), _)| *rr == r).map(|((_, c), v)| (*c, v)).collect();
            if set.is_empty() {
                stmts.push(match kind {
                    DbKind::Mysql => format!("INSERT INTO {table} () VALUES ()"),
                    _ => format!("INSERT INTO {table} DEFAULT VALUES"),
                });
            } else {
                stmts.push(format!(
                    "INSERT INTO {table} ({}) VALUES ({})",
                    set.iter().map(|(c, _)| col(*c)).collect::<Vec<_>>().join(", "),
                    set.iter().map(|(_, v)| sql::literal(kind, v.as_deref())).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        // Production asks first, listing the statements (each changes exactly one row: execute_batch's exact_one)
        let gid = g.id;
        match prod {
            Some(name) => {
                let lines: Vec<String> = stmts.iter().map(|s| format!("  {}", s.chars().take(100).collect::<String>())).collect();
                self.popup = Some(Popup::Confirm {
                    opened: None,
                    text: format!("Production data source \"{name}\" — save {} change(s)?\n\n{}", stmts.len(), lines.join("\n")),
                    on_yes: Box::new(move |a| a.submit_stmts(id, gid, target, stmts)),
                });
            }
            None => self.submit_stmts(id, gid, target, stmts),
        }
    }

    /// Save the statements Submit built for grid `gid` of tab `id`, then show the saved rows (reload the table, run the console's statement again)
    fn submit_stmts(&mut self, id: u64, gid: u64, target: String, stmts: Vec<String>) {
        let n = stmts.len();
        let st = self.state.clone();
        self.status = format!("Submitting {n} statement(s)…");
        self.spawn(async move { st.execute_batch(target, stmts, true).await.map_err(err) }, move |a, r| match r {
            Ok(r) => {
                a.status = format!("{n} statement(s) submitted, {} row(s) affected", r.rows);
                match a.tab_mut(id).map(|t| &t.kind) {
                    Some(TabKind::Table(_)) => a.reload_table(id),
                    // The console still shows that result: run its statement again (a newer result stays as it is)
                    Some(TabKind::Console(c)) if c.grid.as_ref().is_some_and(|g| g.id == gid) => {
                        let sql = c.result_sql.clone();
                        Self::in_console(id, move |a| a.run_stmts(vec![sql], false, false))(a);
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
        });
    }
}
