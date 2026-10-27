//! 쿼리 히스토리·스니펫 (GUI 와 같은 파일): ⌃K › Query History… · Snippets… · Save Query as Snippet…
use crate::app::{App, PAction, PItem, Palette, Popup, Tab, TabKind, byte_offset, selected_text};
use qurry_core::history::Snippet;
use tui_textarea::TextArea;

/// 한 줄로, 길면 앞부분만
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
    /// 지금 탭의 Data Source
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

    /// 최근 실행한 쿼리 (이 Data Source). 고르면 콘솔에
    pub fn history_palette(&mut self) {
        let conn = self.snip_conn();
        let items = self.state.history.list(conn.as_deref(), "", 1000).into_iter().map(|h| PItem { label: one_line(&h.sql, 160), hint: ago(h.ts), action: PAction::InsertSql(h.sql) }).collect();
        self.chooser_sql("Query History (type to filter)", items, "No queries yet");
    }

    pub fn snippets_palette(&mut self) {
        let conn = self.snip_conn();
        let items = self.state.history.snippets(conn.as_deref()).into_iter().map(|s| PItem { label: s.name, hint: one_line(&s.sql, 80), action: PAction::InsertSql(s.sql) }).collect();
        self.chooser_sql("Snippets (type to filter)", items, "No snippets yet — Ctrl+K › Save Query as Snippet");
    }

    /// 골라 둔 글, 아니면 커서의 문장을 이름 붙여 저장 (이 Data Source 에서만 보인다)
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
                Ok(s) => format!("Snippet \"{}\" saved (Ctrl+K › Snippets…)", s.name),
                Err(e) => e.to_string(),
            };
        });
    }

    /// 고른 쿼리를 콘솔에 넣는다 (지금 탭이 콘솔이 아니면 새 콘솔)
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
