//! 테스트용: DB 없이 App 을 띄우고 키를 넣어 상태·화면을 확인한다
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

/// 끝나면 지우는 설정 폴더
struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Harness {
    /// 빈 설정 폴더로 띄운다 (tokio 런타임 안에서: 백그라운드 작업을 쓴다)
    pub async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("qurry-tui-test-{}", uuid::Uuid::new_v4()));
        let state = Arc::new(qurry_core::open_state(Some(dir.clone())).unwrap());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let app = App::new(state, tx);
        let mut h = Harness { app, rx, _dir: TempDir(dir) };
        h.settle().await;
        h
    }

    /// 백그라운드 작업 결과를 더 오지 않을 때까지 반영한다
    pub async fn settle(&mut self) {
        while let Ok(Some(a)) = tokio::time::timeout(std::time::Duration::from_millis(100), self.rx.recv()).await {
            a(&mut self.app);
        }
    }

    /// 키 하나: "a", "Ctrl+A", "Alt+Shift+Left", "Enter", "Tab", "Esc", "Down", "F1" …
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

    /// 글자를 친다
    pub fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            crate::input::on_event(&mut self.app, Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)));
        }
    }

    /// DB 없이 결과가 채워진 테이블 탭을 연다 (pk 가 있으면 편집 가능)
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

    /// DB 없이 키 목록이 채워진 Redis 키 브라우저 탭을 연다
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

    /// 빈 쿼리 콘솔을 열고 편집기에 포커스
    pub fn console_tab(&mut self) -> u64 {
        self.console_with("", 0, 0)
    }

    /// 내용이 든 쿼리 콘솔을 열고 커서를 (줄, 칸) 에 둔다
    pub fn console_with(&mut self, sql: &str, row: u16, col: u16) -> u64 {
        let id = self.app.restore_console("test", "postgres".into(), "console".into(), sql).unwrap();
        let i = self.app.tab_index(id).unwrap();
        self.app.activate(i);
        if let Some(TabKind::Console(c)) = self.app.tabs.get_mut(i).map(|t| &mut t.kind) {
            c.editor.move_cursor(tui_textarea::CursorMove::Jump(row, col));
        }
        id
    }

    /// 지금 콘솔의 Output 줄들
    pub fn console_output(&self) -> Vec<String> {
        match self.app.tabs.get(self.app.active).map(|t| &t.kind) {
            Some(TabKind::Console(c)) => c.output.iter().map(|x| x.1.clone()).collect(),
            _ => panic!("active tab is not a console"),
        }
    }

    /// 지금 탭이 콘솔이면 편집기 내용
    pub fn console_text(&self) -> String {
        match self.app.tabs.get(self.app.active).map(|t| &t.kind) {
            Some(TabKind::Console(c)) => c.editor.lines().join("\n"),
            _ => panic!("active tab is not a console"),
        }
    }

    /// 마우스 왼쪽 버튼: "down" / "drag" / "up"
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

    /// 지금 콘솔 편집기의 (커서, 선택한 글)
    pub fn console_cursor(&self) -> ((usize, usize), Option<String>) {
        match self.app.tabs.get(self.app.active).map(|t| &t.kind) {
            Some(TabKind::Console(c)) => {
                let sel = c.editor.selection_range().map(|((r0, c0), (r1, c1))| crate::app::selected_text(c.editor.lines(), r0, c0, r1, c1));
                (c.editor.cursor(), sel)
            }
            _ => panic!("active tab is not a console"),
        }
    }

    /// 터미널 붙여넣기 (bracketed paste)
    pub fn paste(&mut self, s: &str) {
        crate::input::on_event(&mut self.app, Event::Paste(s.into()));
    }

    pub fn table(&self, id: u64) -> &TableTab {
        match self.app.tabs.iter().find(|t| t.id == id).map(|t| &t.kind) {
            Some(TabKind::Table(tt)) => tt,
            _ => panic!("tab {id} is not a table"),
        }
    }

    /// 화면을 그려 글자만 돌려준다 (줄마다 \n)
    pub fn screen(&mut self, w: u16, h: u16) -> String {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut self.app)).unwrap();
        let buf = term.backend().buffer();
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }
}
