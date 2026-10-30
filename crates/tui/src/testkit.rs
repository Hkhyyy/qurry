//! For tests: launch App without a DB, feed keys, and check state and screen
use crate::app::{Apply, App, TabKind, TableTab};
use crate::grid::Grid;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use qurry_core::db::{ColumnInfo, QueryResult};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct Harness {
    pub app: App,
    rx: UnboundedReceiver<Apply>,
    _dir: TempDir,
}

/// Config folder deleted when done
struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Harness {
    /// Launch with an empty config folder (inside a tokio runtime: uses background tasks)
    pub async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("qurry-tui-test-{}", uuid::Uuid::new_v4()));
        let state = Arc::new(qurry_core::open_state(Some(dir.clone())).unwrap());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let app = App::new(state, tx);
        let mut h = Harness { app, rx, _dir: TempDir(dir) };
        h.settle().await;
        h
    }

    /// Apply background task results until no more arrive
    pub async fn settle(&mut self) {
        while let Ok(Some(a)) = tokio::time::timeout(std::time::Duration::from_millis(100), self.rx.recv()).await {
            a(&mut self.app);
        }
    }

    /// One key: "a", "Ctrl+A", "Alt+Shift+Left", "Enter", "Tab", "Esc", "Down", "F1" …
    pub fn key(&mut self, spec: &str) {
        let mut m = KeyModifiers::NONE;
        let mut parts: Vec<&str> = spec.split('+').collect();
        let name = if spec.ends_with("++") { parts.truncate(parts.len() - 2); "+" } else { parts.pop().unwrap() };
        for p in parts {
            m |= match p {
                "Ctrl" => KeyModifiers::CONTROL,
                "Alt" => KeyModifiers::ALT,
                "Shift" => KeyModifiers::SHIFT,
                "Cmd" => KeyModifiers::SUPER,
                _ => panic!("unknown modifier {p}"),
            };
        }
        let code = match name {
            "Enter" => KeyCode::Enter,
            "Tab" => KeyCode::Tab,
            "Esc" => KeyCode::Esc,
            "Backspace" => KeyCode::Backspace,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "Space" => KeyCode::Char(' '),
            f if f.len() > 1 && f.starts_with('F') => KeyCode::F(f[1..].parse().unwrap()),
            c if c.chars().count() == 1 => {
                let ch = c.chars().next().unwrap();
                KeyCode::Char(if m.contains(KeyModifiers::CONTROL) { ch.to_ascii_lowercase() } else { ch })
            }
            _ => panic!("unknown key {name}"),
        };
        crate::input::on_event(&mut self.app, Event::Key(KeyEvent::new(code, m)));
    }

    /// Type text
    pub fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            crate::input::on_event(&mut self.app, Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)));
        }
    }

    /// Open a table tab filled with results, without a DB (editable if pk is set)
    pub fn table_tab(&mut self, table: &str, cols: &[&str], rows: &[&[&str]], pk: &[&str]) -> u64 {
        let result = QueryResult {
            columns: cols.iter().map(|c| ColumnInfo { name: c.to_string(), type_name: "text".into() }).collect(),
            rows: rows.iter().map(|r| r.iter().map(|v| serde_json::Value::String(v.to_string())).collect()).collect(),
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
        };
        let tt = TableTab {
            schema: "public".into(),
            table: table.into(),
            kind: "table".into(),
            where_: String::new(),
            order: String::new(),
            session: uuid::Uuid::new_v4().to_string(),
            grid: Some(Grid::new(result)),
            running: false,
            pk: pk.iter().map(|s| s.to_string()).collect(),
            error: None,
        };
        self.app.push_tab("test", "postgres".into(), table.into(), TabKind::Table(tt))
    }

    /// Open a Redis key browser tab filled with a key list, without a DB
    pub fn redis_tab(&mut self, keys: &[&str]) -> u64 {
        let r = crate::app::RedisTab {
            pattern: "*".into(),
            keys: keys.iter().map(|k| qurry_core::redisdb::KeyRow { key: k.to_string(), ty: "string".into(), ttl: -1 }).collect(),
            cursor: "0".into(),
            sel: 0,
            off: 0,
            open: Default::default(),
            detail: None,
            text: None,
            value: None,
            focus_value: false,
            scanning: false,
            want: None,
        };
        self.app.push_tab("test", "db0".into(), "redis".into(), TabKind::Redis(r))
    }

    /// Open an empty query console and focus the editor
    pub fn console_tab(&mut self) -> u64 {
        self.console_with("", 0, 0)
    }

    /// Open a query console with content and put the cursor at (line, column)
    pub fn console_with(&mut self, sql: &str, row: u16, col: u16) -> u64 {
        let id = self.app.restore_console("test", "postgres".into(), "console".into(), sql).unwrap();
        let i = self.app.tab_index(id).unwrap();
        self.app.activate(i);
        if let Some(TabKind::Console(c)) = self.app.tabs.get_mut(i).map(|t| &mut t.kind) {
            c.editor.move_cursor(tui_textarea::CursorMove::Jump(row, col));
        }
        id
    }

    /// Output lines of the current console
    pub fn console_output(&self) -> Vec<String> {
        match self.app.tabs.get(self.app.active).map(|t| &t.kind) {
            Some(TabKind::Console(c)) => c.output.iter().map(|x| x.1.clone()).collect(),
            _ => panic!("active tab is not a console"),
        }
    }

    /// Editor content if the current tab is a console
    pub fn console_text(&self) -> String {
        match self.app.tabs.get(self.app.active).map(|t| &t.kind) {
            Some(TabKind::Console(c)) => c.editor.lines().join("\n"),
            _ => panic!("active tab is not a console"),
        }
    }

    /// Left mouse button: "down" / "drag" / "up"
    pub fn mouse(&mut self, kind: &str, x: u16, y: u16) {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let kind = match kind {
            "down" => MouseEventKind::Down(MouseButton::Left),
            "drag" => MouseEventKind::Drag(MouseButton::Left),
            "up" => MouseEventKind::Up(MouseButton::Left),
            _ => panic!("unknown mouse {kind}"),
        };
        crate::input::on_event(&mut self.app, Event::Mouse(MouseEvent { kind, column: x, row: y, modifiers: KeyModifiers::NONE }));
    }

    /// (cursor, selected text) of the current console editor
    pub fn console_cursor(&self) -> ((usize, usize), Option<String>) {
        match self.app.tabs.get(self.app.active).map(|t| &t.kind) {
            Some(TabKind::Console(c)) => {
                let sel = c.editor.selection_range().map(|((r0, c0), (r1, c1))| crate::app::selected_text(c.editor.lines(), r0, c0, r1, c1));
                (c.editor.cursor(), sel)
            }
            _ => panic!("active tab is not a console"),
        }
    }

    /// Terminal paste (bracketed paste)
    pub fn paste(&mut self, s: &str) {
        crate::input::on_event(&mut self.app, Event::Paste(s.into()));
    }

    pub fn table(&self, id: u64) -> &TableTab {
        match self.app.tabs.iter().find(|t| t.id == id).map(|t| &t.kind) {
            Some(TabKind::Table(tt)) => tt,
            _ => panic!("tab {id} is not a table"),
        }
    }

    /// Draw the screen and return only the text (\n per line)
    pub fn screen(&mut self, w: u16, h: u16) -> String {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut self.app)).unwrap();
        let buf = term.backend().buffer();
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }
}
