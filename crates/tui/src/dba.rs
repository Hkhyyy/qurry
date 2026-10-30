//! DBA report tab (⌃K › Database Admin…): sessions and locks reload every 3 seconds; c cancel query · K kill session · Enter query to console · g generate SQL
use crate::app::{App, DbaTab, Focus, PAction, PItem, Palette, Popup, Tab, TabKind};
use crate::grid::Grid;
use crate::tree::NodeKind;
use qurry_core::dba::{Action as DAction, reports};
use qurry_core::sql;
use std::collections::HashMap;
use tui_textarea::TextArea;

/// Reports that reload automatically (views of the current state)
const LIVE: [&str; 3] = ["sessions", "locks", "vacuum_progress"];

impl App {
    /// Connection target for the report: the Data Source / DB selected in the tree, otherwise the current tab's target
    fn dba_target(&self) -> Option<(String, String)> {
        if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected().filter(|n| matches!(n.kind, NodeKind::Conn | NodeKind::Db | NodeKind::Schema))
        {
            let db = n.db.clone().unwrap_or_else(|| self.conn(&n.conn).map(|c| sql::default_db(&c.config)).unwrap_or_default());
            return Some((n.conn.clone(), db));
        }
        let t = self.tabs.get(self.active)?;
        Some((t.conn.clone(), t.db.clone()))
    }

    /// ⌃K › Database Admin…: this DB's report list
    pub fn dba_palette(&mut self) {
        let Some((conn, _)) = self.dba_target() else {
            self.status = "Select a data source (or open a tab) first".into();
            return;
        };
        let list = reports(self.kind_of(&conn));
        if list.is_empty() {
            self.status = "No admin reports for this data source".into();
            return;
        }
        let items = list.into_iter().map(|r| PItem { label: format!("{} › {}", r.group, r.title), hint: r.help.chars().take(80).collect(), action: PAction::Dba(r.id.into()) }).collect();
        self.popup = Some(Popup::Palette(Palette { title: "Database Admin".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// Open a report tab (if a DBA tab for the same target exists, switch it there)
    pub fn dba_open(&mut self, report: String) {
        let Some((conn, db)) = self.dba_target() else { return };
        let info = reports(self.kind_of(&conn)).into_iter().find(|r| r.id == report);
        let title = info.as_ref().map(|i| i.title).unwrap_or("Admin").to_string();
        let target = sql::target_id(&conn, Some(&db));
        let id = match self.tabs.iter().position(|t| t.target == target && matches!(t.kind, TabKind::Dba(_))) {
            Some(i) => {
                let t = &mut self.tabs[i];
                t.title = title;
                if let TabKind::Dba(d) = &mut t.kind {
                    d.report = report;
                    d.info = info;
                    d.grid = None;
                    d.error = None;
                }
                let id = t.id;
                self.activate(i);
                id
            }
            None => {
                let d = DbaTab { report, info, grid: None, error: None, running: false, auto: true, gone: None };
                let id = self.push_tab(&conn, db, title, TabKind::Dba(d));
                self.dba_ticker(id);
                id
            }
        };
        self.dba_reload(id);
    }

    /// Sessions and locks: reload every 3 seconds while this tab is visible and auto is on (stops when the tab closes)
    pub(crate) fn dba_ticker(&self, id: u64) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
                let a2 = alive.clone();
                let sent = tx.send(Box::new(move |a: &mut App| {
                    let Some(i) = a.tab_index(id) else {
                        a2.store(false, std::sync::atomic::Ordering::Relaxed);
                        return;
                    };
                    let shown = a.active == i || a.group_tab(a.tabs[i].group) == Some(i);
                    if let TabKind::Dba(d) = &a.tabs[i].kind
                        && shown
                        && d.auto
                        && !d.running
                        && LIVE.contains(&d.report.as_str())
                    {
                        a.dba_reload(id);
                    }
                }));
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if sent.is_err() || !alive.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
        });
    }

    pub fn dba_reload(&mut self, id: u64) {
        let Some(conn) = self.tab_mut(id).map(|t| t.conn.clone()) else { return };
        let kind = self.kind_of(&conn);
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Dba(d) = &mut t.kind else { return };
        d.running = true;
        let report = d.report.clone();
        if d.info.is_none() {
            d.info = reports(kind).into_iter().find(|r| r.id == report);
        }
        let st = self.state.clone();
        self.spawn(async move { st.dba_report(target, report).await.map_err(|e| e.to_string()) }, move |a, r| a.dba_apply(id, r));
    }

    /// Put in the loaded results. The selected row follows the same pid (so a different session isn't selected when the order changes)
    pub fn dba_apply(&mut self, id: u64, r: Result<qurry_core::db::QueryResult, String>) {
        let Some(Tab { kind: TabKind::Dba(d), .. }) = self.tab_mut(id) else { return };
        d.running = false;
        match r {
            Ok(res) => {
                let pid_col = |g: &Grid| g.result.columns.iter().position(|c| c.name == "pid");
                let (row, col, picked) = d.grid.as_ref().map(|g| (g.row, g.col, pid_col(g).and_then(|c| g.value(g.row, c)))).unwrap_or((0, 0, None));
                let mut g = Grid::new(res);
                g.col = col.min(g.cols().saturating_sub(1));
                g.row = row.min(g.rows().saturating_sub(1));
                let picked = picked.or(d.gone.take());
                if let (Some(p), Some(c)) = (picked, pid_col(&g)) {
                    match (0..g.rows()).find(|&r| g.value(r, c).as_deref() == Some(p.as_str())) {
                        Some(r) => g.row = r,
                        None => d.gone = Some(p),
                    }
                }
                d.grid = Some(g);
                d.error = None;
            }
            Err(e) => {
                d.grid = None;
                d.error = Some(e);
            }
        }
    }

    /// Current row: column name → value
    fn dba_row(&self) -> Option<HashMap<String, String>> {
        let Some(Tab { kind: TabKind::Dba(d), .. }) = self.tabs.get(self.active) else { return None };
        let g = d.grid.as_ref()?;
        Some(g.result.columns.iter().enumerate().map(|(c, col)| (col.name.clone(), g.value(g.row, c).unwrap_or_default())).collect())
    }

    /// c / K: cancel the query / kill the connection of the current row's session (pid). Asks first
    pub fn dba_kill(&mut self, terminate: bool) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let (id, target) = (t.id, t.target.clone());
        let TabKind::Dba(d) = &t.kind else { return };
        if !d.info.as_ref().is_some_and(|i| i.actions.iter().any(|a| matches!(a, DAction::Kill { .. }))) {
            self.status = "This report has no sessions to stop".into();
            return;
        }
        // Vacuum Progress only allows cancel (K does nothing in reports without a kill-connection action)
        if terminate && !d.info.as_ref().is_some_and(|i| i.actions.iter().any(|a| matches!(a, DAction::Kill { terminate: true, .. }))) {
            self.status = "This report can only cancel (c)".into();
            return;
        }
        if let Some(p) = &d.gone {
            self.status = format!("Session {p} is gone — select a session again");
            return;
        }
        let Some(row) = self.dba_row() else { return };
        let Some(pid) = row.get("pid").and_then(|p| p.parse::<i64>().ok()) else {
            self.status = "No pid in this row".into();
            return;
        };
        let what = if terminate { "Terminate session" } else { "Cancel the query of session" };
        let q: String = row.get("query").or(row.get("blocking_query")).cloned().unwrap_or_default().chars().take(200).collect();
        let st = self.state.clone();
        self.popup = Some(Popup::Confirm {
            text: format!("{what} {pid}?\n\n{q}"),
            on_yes: Box::new(move |a| {
                a.spawn(async move { st.kill_session(target, pid, terminate).await.map_err(|e| e.to_string()) }, move |a, r| {
                    a.status = match r {
                        Ok(true) => format!("{what} {pid}: done"),
                        Ok(false) => format!("{what} {pid}: not found (already gone?)"),
                        Err(e) => e,
                    };
                    a.dba_reload(id);
                });
            }),
        });
    }

    /// Enter: current row's query to the console. g: choose an SQL action for this report
    pub fn dba_actions(&mut self, open_query: bool) {
        let Some(Tab { kind: TabKind::Dba(d), .. }) = self.tabs.get(self.active) else { return };
        let sqls: Vec<(String, String)> = d.info.iter().flat_map(|i| i.actions.iter()).filter_map(|a| match a {
            DAction::Sql { label, template } => Some((label.to_string(), template.to_string())),
            _ => None,
        }).collect();
        if open_query {
            match sqls.iter().find(|s| s.1 == "{=query}") {
                Some((_, t)) => self.dba_sql(t.clone()),
                None => self.status = "This report has no query to open".into(),
            }
            return;
        }
        if sqls.is_empty() {
            self.status = "This report has no SQL actions".into();
            return;
        }
        let items = sqls.into_iter().map(|(label, t)| PItem { label, hint: t.clone(), action: PAction::DbaSql(t) }).collect();
        self.popup = Some(Popup::Palette(Palette { title: "Generate SQL from this row".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// Fill a template with the current row's values into a new console
    pub fn dba_sql(&mut self, template: String) {
        let Some(row) = self.dba_row() else { return };
        let Some(t) = self.tabs.get(self.active) else { return };
        let (conn, db) = (t.conn.clone(), t.db.clone());
        let q = qurry_core::dba::action_sql(self.kind_of(&conn), &template, &row);
        self.open_console(&conn, Some(db));
        if let Some(Tab { kind: TabKind::Console(c), .. }) = self.tabs.get_mut(self.active) {
            c.editor.insert_str(q);
        }
    }

    /// ⌃K › Show Diagram (ERD): relationships of the schema selected in the tree (or all user schemas of the DB) as text + Mermaid (copy with y)
    pub fn erd_open(&mut self) {
        let Some((conn, db)) = self.dba_target() else {
            self.status = "Select a schema or a database first".into();
            return;
        };
        // From a Data Source row: instead of drawing the default DB (usually postgres — empty), choose which DB
        if self.focus == Focus::Tree && self.tree.selected().is_some_and(|n| n.kind == NodeKind::Conn) {
            let st = self.state.clone();
            let c2 = conn.clone();
            self.status = "Reading databases…".into();
            self.spawn(async move { st.list_databases(c2).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                Ok(dbs) if dbs.len() > 1 => {
                    let items = dbs.into_iter().map(|d| crate::app::PItem { label: d.clone(), hint: String::new(), action: crate::app::PAction::Pick(d) }).collect();
                    a.status.clear();
                    a.flow = Some(crate::migrate::Flow::ErdDb(conn));
                    a.popup = Some(crate::app::Popup::Palette(crate::app::Palette { title: "Diagram of which database?".into(), input: tui_textarea::TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
                }
                Ok(dbs) => a.erd_run(conn, dbs.into_iter().next().unwrap_or(db), None),
                Err(e) => a.status = format!("Diagram: {e}"),
            });
            return;
        }
        let picked = self.tree.selected().filter(|n| n.kind == NodeKind::Schema && self.focus == Focus::Tree).and_then(|n| n.schema);
        self.erd_run(conn, db, picked);
    }

    /// This DB's diagram (picked = only those schemas, otherwise all non-system schemas)
    pub fn erd_run(&mut self, conn: String, db: String, picked: Option<String>) {
        let kind = self.kind_of(&conn);
        let target = sql::target_id(&conn, Some(&db));
        let st = self.state.clone();
        self.status = "Reading tables and foreign keys…".into();
        let db2 = db.clone();
        self.spawn(
            async move {
                let schemas = match (picked, kind) {
                    (Some(s), _) => vec![s],
                    (None, qurry_core::store::DbKind::Mysql) => vec![db2],
                    (None, _) => st.list_schemas(target.clone()).await.map_err(|e| e.to_string())?.into_iter().filter(|s| !s.is_system).map(|s| s.name).collect(),
                };
                let e = st.erd(target, schemas.clone()).await.map_err(|e| e.to_string())?;
                Ok::<_, String>((schemas, e))
            },
            move |a, r| match r {
                Ok((schemas, e)) if e.tables.is_empty() => {
                    a.status = format!("No tables in {db} / {} — pick the database (or schema) that has them in the explorer", schemas.join(", "));
                }
                Ok((schemas, e)) => {
                    a.status = format!("{} tables, {} foreign keys", e.tables.len(), e.relations.len());
                    let text = format!("{}\n── Mermaid (y copies the whole text; paste into docs, issues, mermaid.live) ──\n\n{}", e.text(), e.mermaid());
                    let title = format!("Diagram: {}", if schemas.len() == 1 { schemas[0].clone() } else { format!("{} schemas", schemas.len()) });
                    a.push_tab(&conn, db, title, TabKind::Text(crate::app::TextTab { text, scroll: 0 }));
                }
                Err(e) => a.status = format!("Diagram: {e}"),
            },
        );
    }

    /// a: toggle auto reload
    pub fn dba_toggle_auto(&mut self) {
        if let Some(Tab { kind: TabKind::Dba(d), .. }) = self.tabs.get_mut(self.active) {
            d.auto = !d.auto;
            self.status = format!("Auto-refresh {}", if d.auto { "on (3s)" } else { "off" });
        }
    }
}
