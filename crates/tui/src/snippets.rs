//! Query history·snippets (same file as the GUI): ⌃K › Query History… · Snippets… · Save Query as Snippet…
//! Production log (⌃K › Production Log…): changes run on Production Data Sources and their undo SQL
use crate::app::{App, PAction, PItem, Palette, Popup, Tab, TabKind, byte_offset, selected_text};
use qurry_core::history::Snippet;
use tui_textarea::TextArea;

/// On one line, only the beginning if long
fn one_line(s: &str, n: usize) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
}

fn ago(ts: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(ts);
    let s = (now - ts).max(0) / 1000;
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

impl App {
    /// Data Source of the current tab
    fn snip_conn(&self) -> Option<String> {
        self.tabs.get(self.active).map(|t| t.conn.clone())
    }

    fn chooser_sql(&mut self, title: &str, items: Vec<PItem>, empty: &str) {
        if items.is_empty() {
            self.status = empty.into();
            return;
        }
        self.popup = Some(Popup::Palette(Palette { title: title.into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// Recently run queries (this Data Source). Picking one puts it in the console
    pub fn history_palette(&mut self) {
        let conn = self.snip_conn();
        let items = self.state.history.list(conn.as_deref(), "", 1000).into_iter().map(|h| PItem { label: one_line(&h.sql, 160), hint: ago(h.ts), action: PAction::InsertSql(h.sql) }).collect();
        self.chooser_sql("Query History (type to filter)", items, "No queries yet");
    }

    pub fn snippets_palette(&mut self) {
        let conn = self.snip_conn();
        let items = self.state.history.snippets(conn.as_deref()).into_iter().map(|s| PItem { label: s.name, hint: one_line(&s.sql, 80), action: PAction::InsertSql(s.sql) }).collect();
        self.chooser_sql("Snippets (type to filter)", items, "No snippets yet — ⌃k › Save Query as Snippet");
    }

    /// Save the selected text, or the statement at the cursor, under a name (visible only in this Data Source)
    pub fn save_snippet(&mut self) {
        let Some(Tab { kind: TabKind::Console(c), conn, .. }) = self.tabs.get(self.active) else {
            self.status = "Open a query console first".into();
            return;
        };
        let kind = self.kind_of(conn);
        let sql = match c.editor.selection_range() {
            Some(((r0, c0), (r1, c1))) => selected_text(c.editor.lines(), r0, c0, r1, c1),
            None => {
                let (row, col) = c.editor.cursor();
                qurry_core::sql::statement_at(&c.editor.lines().join("\n"), byte_offset(c.editor.lines(), row, col), kind).unwrap_or_default()
            }
        };
        if sql.trim().is_empty() {
            self.status = "Nothing to save — write or select a query".into();
            return;
        }
        let conn = conn.clone();
        self.input("Snippet name", &one_line(&sql, 40), move |a, name| {
            if name.trim().is_empty() {
                return;
            }
            a.status = match a.state.history.save_snippet(Snippet { id: String::new(), name: name.trim().into(), sql: sql.trim().into(), conn_id: Some(conn) }) {
                Ok(s) => format!("Snippet \"{}\" saved (⌃k › Snippets…)", s.name),
                Err(e) => e.to_string(),
            };
        });
    }

    /// Production log (all Data Sources, newest first). Picking one opens its undo SQL in a new console
    pub fn production_log(&mut self) {
        let items = self
            .state
            .audit_log(None, String::new(), 1000)
            .into_iter()
            .map(|e| {
                let status = if e.error.is_some() {
                    "failed".to_string()
                } else if let Some(n) = e.rows {
                    format!("{n} row(s){}", if e.in_tx { ", not committed yet" } else { "" })
                } else {
                    String::new()
                };
                let place = match &e.database {
                    Some(db) => format!("{} · {db}", e.conn_name),
                    None => e.conn_name.clone(),
                };
                let undo = if e.undo.is_some() { "↶ undo".to_string() } else { String::new() };
                let (conn, db) = qurry_core::app::split_target(&e.conn_id);
                PItem {
                    label: one_line(&e.sql, 120),
                    // When narrow the end is cut → important things (undo · row count) first
                    hint: [undo, status, ago(e.ts), place].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
                    action: PAction::OpenUndo { conn: conn.into(), db: db.map(String::from), note: e.error.clone().or(e.note.clone()).unwrap_or_else(|| "No undo for this statement".into()), undo: e.undo },
                }
            })
            .collect();
        self.chooser_sql("Production Log (type to filter · ↩ opens the undo SQL in a new console)", items, "Nothing yet — changes you run on a Production data source show up here");
    }

    /// Undo SQL into a new console of that Data Source (run it yourself — it goes through the Production confirmation)
    pub fn open_undo(&mut self, conn: String, db: Option<String>, undo: Option<String>, note: String) {
        let Some(undo) = undo else {
            self.status = note;
            return;
        };
        if self.conn(&conn).is_none() {
            self.status = "That data source no longer exists".into();
            return;
        }
        self.open_console(&conn, db);
        if let Some(Tab { kind: TabKind::Console(c), .. }) = self.tabs.get_mut(self.active) {
            c.editor.insert_str(undo);
            self.focus = crate::app::Focus::Editor;
        }
        self.status = "Undo SQL opened — check it, then run it".into();
    }

    /// Put the picked query in the console (a new console if the current tab isn't a console)
    pub fn insert_sql(&mut self, sql: String) {
        if !matches!(self.tabs.get(self.active).map(|t| &t.kind), Some(TabKind::Console(_))) {
            let Some(t) = self.tabs.get(self.active) else { return };
            let (conn, db) = (t.conn.clone(), t.db.clone());
            self.open_console(&conn, Some(db));
        }
        if let Some(Tab { kind: TabKind::Console(c), .. }) = self.tabs.get_mut(self.active) {
            c.editor.insert_str(sql);
            self.focus = crate::app::Focus::Editor;
        }
    }
}
