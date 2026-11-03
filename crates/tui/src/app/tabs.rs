use super::*;

impl App {
    // ───────── Tabs ─────────

    pub(crate) fn push_tab(&mut self, conn: &str, db: String, title: String, kind: TabKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let target = sql::target_id(conn, Some(&db));
        // New tabs open in the focused pane
        let group = self.focused_group();
        self.tabs.push(Tab { id, title, conn: conn.into(), db, target, kind, group, stale: false });
        self.activate(self.tabs.len() - 1);
        id
    }

    /// The focused pane
    pub fn focused_group(&self) -> usize {
        self.tabs.get(self.active).map(|t| t.group).unwrap_or_else(|| self.panes.leaves()[0])
    }

    /// Tab shown in the pane (index)
    pub fn group_tab(&self, g: usize) -> Option<usize> {
        self.group_active[g]
            .and_then(|id| self.tab_index(id))
            .filter(|&i| self.tabs[i].group == g)
            .or_else(|| self.tabs.iter().position(|t| t.group == g))
    }

    pub fn activate(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        let id = self.tabs[i].id;
        self.active = i;
        self.group_active[self.tabs[i].group] = Some(id);
        self.mru.retain(|x| *x != id);
        self.mru.insert(0, id);
        self.focus = match &self.tabs[i].kind {
            TabKind::Console(_) => Focus::Editor,
            _ => Focus::Grid,
        };
        if std::mem::take(&mut self.tabs[i].stale) {
            self.load_restored(i);
        }
    }

    /// When a restored tab is first viewed: data for tables, key list for Redis, autocomplete·transaction mode for consoles
    fn load_restored(&mut self, i: usize) {
        let (id, target) = (self.tabs[i].id, self.tabs[i].target.clone());
        match &self.tabs[i].kind {
            TabKind::Table(_) => self.reload_table(id),
            TabKind::Redis(_) => self.redis_scan(id, true),
            TabKind::Console(c) => {
                let (manual, session) = (c.tx_manual, c.session.clone());
                self.load_completion(target.clone());
                if manual {
                    let st = self.state.clone();
                    self.spawn(async move { st.set_tx_mode(target, session, true).await.map_err(err) }, |a, r| {
                        if let Err(e) = r {
                            a.status = e;
                        }
                    });
                }
            }
            TabKind::Text(_) => {}
            TabKind::Dba(_) => {
                self.dba_reload(id);
                self.dba_ticker(id);
            }
        }
    }

    // ───────── Restoring on relaunch (workspace.rs) ─────────

    /// Save if changed (periodically·on quit)
    pub fn save_workspace(&mut self) {
        if !self.restored {
            return;
        }
        let s = crate::workspace::to_json(self).to_string();
        if s != self.saved_ws {
            if let Ok(v) = serde_json::from_str(&s) {
                if self.state.store.save_workspace("tui", &v).is_ok() {
                    self.saved_ws = s;
                }
            }
        }
    }

    fn restored_tab(&mut self, conn: &str, db: String, title: String, kind: TabKind) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let target = sql::target_id(conn, Some(&db));
        self.tabs.push(Tab { id, title, conn: conn.into(), db, target, kind, group: 0, stale: true });
        Some(id)
    }

    pub fn restore_console(&mut self, conn: &str, db: String, title: String, sql: &str) -> Option<u64> {
        let mut editor = TextArea::new(sql.lines().map(String::from).collect());
        editor.set_cursor_line_style(ratatui::style::Style::default());
        let manual = self.conn(conn).map(|c| c.config.options.tx_manual).unwrap_or(false);
        let c = Console { editor, session: uuid::Uuid::new_v4().to_string(), grid: None, result_title: String::new(), output: vec![], show_output: true, running: false, tx_manual: manual, in_tx: false, complete: None };
        self.restored_tab(conn, db, title, TabKind::Console(c))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore_table(&mut self, conn: &str, db: String, schema: String, table: String, kind: String, where_: String, order: String) -> Option<u64> {
        let title = table.clone();
        let tt = TableTab { schema, table, kind, where_, order, session: uuid::Uuid::new_v4().to_string(), grid: None, running: false, pk: vec![], error: None };
        self.restored_tab(conn, db, title, TabKind::Table(tt))
    }

    pub fn restore_text(&mut self, conn: &str, db: String, title: String, text: &str, kind: &str) -> Option<u64> {
        self.restored_tab(conn, db, title, TabKind::Text(TextTab { text: text.into(), kind: kind.into(), scroll: 0 }))
    }

    pub fn restore_dba(&mut self, conn: &str, db: String, title: String, report: &str) -> Option<u64> {
        let d = DbaTab { report: report.into(), info: None, grid: None, loaded: None, error: None, running: false, run: 0, auto: true, gone: None };
        self.restored_tab(conn, db, title, TabKind::Dba(d))
    }

    pub fn restore_redis(&mut self, conn: &str, db: String, title: String, pattern: &str) -> Option<u64> {
        let r = RedisTab { pattern: pattern.into(), keys: vec![], cursor: "0".into(), sel: 0, off: 0, open: HashSet::new(), detail: None, text: None, value: None, focus_value: false, scanning: false, want: None };
        self.restored_tab(conn, db, title, TabKind::Redis(r))
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }
    /// Also search tabs stashed in other workspaces (so query results finished meanwhile aren't lost)
    pub fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        if let Some(i) = self.tab_index(id) {
            return self.tabs.get_mut(i);
        }
        self.workspaces.iter_mut().filter_map(|w| w.saved.as_mut()).flat_map(|s| s.tabs.iter_mut()).find(|t| t.id == id)
    }

    /// Console tabs with uncommitted changes
    pub(super) fn pending_tabs(&self, conn: Option<&str>) -> Vec<u64> {
        let saved = self.workspaces.iter().filter_map(|w| w.saved.as_ref()).flat_map(|s| s.tabs.iter());
        self.tabs.iter().chain(saved).filter(|t| conn.is_none_or(|c| t.conn == c) && matches!(&t.kind, TabKind::Console(c) if c.in_tx)).map(|t| t.id).collect()
    }

    /// What has to be answered on screen before quitting (a dialog or the designer is open, or uncommitted changes would
    /// be asked about): under the 'terminal too small' message the quit key then waits for a larger window
    pub fn quit_waits_for(&self) -> Option<String> {
        if self.popup.is_some() || self.designer.is_some() {
            return Some("A dialog is open: make the window larger to answer it".into());
        }
        let n = self.pending_tabs(None).len();
        (n > 0).then(|| format!("{n} console(s) have uncommitted changes: make the window larger to quit"))
    }

    /// Close a tab. Ask first if there are uncommitted changes (closing rolls them back)
    pub fn close_tab(&mut self, i: usize) {
        let Some(id) = self.tabs.get(i).map(|t| t.id) else { return };
        if self.pending_tabs(None).contains(&id) {
            self.popup = Some(Popup::Confirm {
                opened: None,
                text: "This console has uncommitted changes. Roll them back and close the tab?".into(),
                on_yes: Box::new(move |a| {
                    if let Some(i) = a.tab_index(id) {
                        a.close_tab_now(i)
                    }
                }),
            });
            return;
        }
        self.close_tab_now(i);
    }

    pub fn close_tab_now(&mut self, i: usize) {
        if let Some(t) = self.detach_tab(i) {
            self.close_sessions(&[t]);
        }
    }

    /// Close the tab's DB session (uncommitted changes are rolled back)
    pub fn close_sessions(&self, tabs: &[Tab]) {
        for t in tabs {
            let session = match &t.kind {
                TabKind::Console(c) => Some(c.session.clone()),
                TabKind::Table(tt) => Some(tt.session.clone()),
                _ => None,
            };
            if let Some(s) = session {
                let st = self.state.clone();
                tokio::spawn(async move {
                    let _ = st.close_session(s).await;
                });
            }
        }
    }

    /// Detach a tab from the screen (session kept — also used when moving to another workspace). Shows one of the remaining tabs
    pub fn detach_tab(&mut self, i: usize) -> Option<Tab> {
        if i >= self.tabs.len() {
            return None;
        }
        let t = self.tabs.remove(i);
        self.mru.retain(|x| *x != t.id);
        if self.group_active[t.group] == Some(t.id) {
            self.group_active[t.group] = None;
        }
        if self.tabs.is_empty() {
            self.active = 0;
            self.focus = Focus::Tree;
            self.panes = crate::panes::Pane::Leaf(0);
            self.group_active = [None; crate::panes::MAX];
            return Some(t);
        }
        // Remove the pane if it becomes empty
        let emptied = !self.tabs.iter().any(|x| x.group == t.group);
        // Go to the most recently used tab in the same pane. If none (restored tabs aren't in the recent list), the neighboring tab in the same pane
        let same = |j: &usize| self.tabs[*j].group == t.group;
        let next = self
            .mru
            .iter()
            .filter_map(|id| self.tab_index(*id))
            .find(same)
            .or_else(|| (i..self.tabs.len()).find(same).or_else(|| (0..i).rev().find(same)))
            .or_else(|| self.mru.first().and_then(|id| self.tab_index(*id)))
            .unwrap_or(0);
        self.activate(next);
        if emptied {
            self.panes.remove(t.group);
        }
        Some(t)
    }

    /// Move a tab to another pane (before the given tab) or reorder, or split anew and send it to that pane
    pub fn drop_tab(&mut self, id: u64, target: DropTarget) {
        let Some(i) = self.tab_index(id) else { return };
        let from = self.tabs[i].group;
        let (g, before) = match target {
            DropTarget::NewSplit(g, dir) => {
                if self.tabs.len() < 2 {
                    // With only one tab, create a new console and split
                    self.activate(i);
                    return self.split_focused(dir);
                }
                match self.panes.split(g, dir) {
                    Some(new) => (new, None),
                    None => {
                        self.status = format!("Up to {} panes", crate::panes::MAX);
                        return;
                    }
                }
            }
            DropTarget::Group(g, before) => (g, before),
        };
        if before == Some(id) {
            return self.activate(i);
        }
        let mut t = self.tabs.remove(i);
        t.group = g;
        let at = before.and_then(|b| self.tab_index(b)).unwrap_or(self.tabs.len());
        self.tabs.insert(at, t);
        if from != g && self.group_active[from] == Some(id) {
            // The pane left behind shows its most recently used tab
            self.group_active[from] = self.mru.iter().copied().find(|x| *x != id && self.tab_index(*x).is_some_and(|j| self.tabs[j].group == from));
        }
        self.activate(at);
        if !self.tabs.iter().any(|x| x.group == from) {
            self.panes.remove(from);
        }
    }

    /// Rows near the table tab's current row (pgvector): the cursor column if it's a vector, otherwise the first vector column. SQL into a new console
    pub(super) fn find_similar(&mut self) {
        let Some(Tab { kind: TabKind::Table(tt), conn, db, target, .. }) = self.tabs.get(self.active) else {
            self.status = "Open a table with a vector column first".into();
            return;
        };
        if self.kind_of(conn) != DbKind::Postgres {
            self.status = "Find Similar Rows is for PostgreSQL tables with a pgvector column".into();
            return;
        }
        let Some(g) = &tt.grid else { return };
        let cols = &g.result.columns;
        let Some(vc) = cols.get(g.col).filter(|c| sql::is_vector_type(&c.type_name)).or_else(|| cols.iter().find(|c| sql::is_vector_type(&c.type_name))).map(|c| c.name.clone()) else {
            self.status = format!("{}.{} has no vector column", tt.schema, tt.table);
            return;
        };
        let Some(row) = g.result.rows.get(g.row) else { return };
        let pk: Vec<(String, Option<String>)> = tt.pk.iter().map(|k| (k.clone(), cols.iter().position(|c| &c.name == k).and_then(|i| row.get(i)).filter(|v| !v.is_null()).map(crate::grid::cell_text))).collect();
        let (st, conn, db, target, schema, table) = (self.state.clone(), conn.clone(), db.clone(), target.clone(), tt.schema.clone(), tt.table.clone());
        self.spawn(async move { st.similar_rows_sql(target, schema, table, vc, pk).await.map_err(|e| e.to_string()) }, move |a, r| match r {
            Ok(sql) => {
                a.open_console(&conn, Some(db));
                if let Some(Tab { kind: TabKind::Console(c), .. }) = a.tabs.get_mut(a.active) {
                    c.editor.insert_str(sql);
                }
                a.status = "Similar-rows query is in a new console — F5 to run".into();
            }
            Err(e) => a.status = e,
        });
    }

    pub fn open_console(&mut self, conn: &str, db: Option<String>) {
        let db = db.unwrap_or_else(|| self.default_db(conn));
        let name = self.conn(conn).map(|c| c.config.name.clone()).unwrap_or_default();
        let n = self.tabs.iter().filter(|t| t.conn == conn && matches!(t.kind, TabKind::Console(_))).count();
        let title = if n == 0 { format!("console [{name}]") } else { format!("console_{} [{name}]", n + 1) };
        let mut editor = TextArea::default();
        editor.set_cursor_line_style(ratatui::style::Style::default());
        let manual = self.conn(conn).map(|c| c.config.options.tx_manual).unwrap_or(false);
        let session = uuid::Uuid::new_v4().to_string();
        let id = self.push_tab(conn, db, title, TabKind::Console(Console { editor, session: session.clone(), grid: None, result_title: String::new(), output: vec![], show_output: true, running: false, tx_manual: manual, in_tx: false, complete: None }));
        let target = self.tabs[self.tab_index(id).unwrap()].target.clone();
        self.load_completion(target);
        if manual {
            let st = self.state.clone();
            let target = self.tabs[self.tab_index(id).unwrap()].target.clone();
            self.spawn(async move { st.set_tx_mode(target, session, true).await.map_err(err) }, |a, r| {
                if let Err(e) = r {
                    a.status = e;
                }
            });
        }
    }

    /// Tab title: schema.table if several table tabs share the name, db.schema.table if the schema matches too (DataGrip)
    pub fn tab_label(&self, t: &Tab) -> String {
        let key = |t: &Tab| match &t.kind {
            TabKind::Table(o) => Some(qurry_core::labels::TableTab { id: t.id.to_string(), db: t.db.clone(), schema: o.schema.clone(), table: o.table.clone() }),
            _ => None,
        };
        let Some(me) = key(t) else { return t.title.clone() };
        let all: Vec<_> = self.tabs.iter().filter_map(key).collect();
        qurry_core::labels::label(&me, &all).unwrap_or_else(|| t.title.clone())
    }
}
