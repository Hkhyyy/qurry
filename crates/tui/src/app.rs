//! TUI 상태와 동작. 실제 DB 작업은 전부 qurry_core::app::AppState (GUI 와 같은 엔진).
use crate::complete::{self, Item};
use crate::grid::{raw_text, Grid};
use crate::tree::{Node, NodeKind, Tree};
use qurry_core::app::{AppState, ConnectionView};
use qurry_core::db::{ColumnInfo, QueryResult};
use qurry_core::meta::{IndexEntry, SchemaObjects};
use qurry_core::redisdb::{KeyDetail, KeyRow};
use qurry_core::sql;
use qurry_core::store::{ConnectionConfig, DbKind, RedisMode, SshAuth, SshConfig, SslConfig};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tui_textarea::TextArea;

pub type Apply = Box<dyn FnOnce(&mut App) + Send>;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    Tree,
    Editor,
    Grid,
}

pub struct Console {
    pub editor: TextArea<'static>,
    pub session: String,
    pub grid: Option<Grid>,
    pub result_title: String,
    pub output: Vec<(bool, String)>,
    pub show_output: bool,
    pub running: bool,
    pub tx_manual: bool,
    pub in_tx: bool,
    /// 열린 자동완성 목록
    pub complete: Option<Completion>,
}

/// 콘솔 Output 로그에 남기는 최대 줄 수 (GUI 와 같다)
const OUTPUT_MAX: usize = 1000;

impl Console {
    /// Output 로그에 한 줄 (오래된 줄은 버린다)
    pub fn log(&mut self, err: bool, s: String) {
        self.output.push((err, s));
        if self.output.len() > OUTPUT_MAX {
            self.output.drain(..self.output.len() - OUTPUT_MAX);
        }
    }
}

pub struct Completion {
    pub items: Vec<Item>,
    pub sel: usize,
    /// 바꿀 접두어(커서 앞 단어)의 글자 수
    pub prefix: usize,
}

/// 에디터 화면 분할 (GUI 의 Split Right / Split Down). 칸(그룹)마다 탭 바가 따로 있다
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SplitDir {
    Right,
    Down,
}

/// 마우스로 끄는 것
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drag {
    /// 콘솔 에디터/결과 경계선
    Console,
    /// 칸 사이 경계선 (Hits.splits 의 순번)
    Split(usize),
    /// 그리드 가로 스크롤바
    HBar,
    /// 그리드 범위 선택 (누른 칸부터)
    Grid,
    /// Database Explorer 오른쪽 경계선
    Explorer,
    /// 탭 (moved = 실제로 끌었는지. 그냥 클릭이면 false)
    Tab { id: u64, moved: bool },
    /// 콘솔에서 글 선택 (anchor = 누른 곳, selecting = 끌기 시작했는지)
    Editor { anchor: (usize, usize), selecting: bool },
}

/// 탭을 놓을 곳
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DropTarget {
    /// 그 칸으로 옮긴다 (before = 그 탭 앞에 끼운다)
    Group(usize, Option<u64>),
    /// 그 칸을 나누고 새 칸으로
    NewSplit(usize, SplitDir),
}

pub struct TableTab {
    pub schema: String,
    pub table: String,
    /// table / view / matview / foreign (Ctrl+B DDL 용)
    pub kind: String,
    pub where_: String,
    pub order: String,
    pub session: String,
    pub grid: Option<Grid>,
    pub running: bool,
    pub pk: Vec<String>,
    pub error: Option<String>,
}

pub struct TextTab {
    pub text: String,
    pub scroll: u16,
}

/// DBA 리포트 (세션·락, 느린 쿼리, 용량·인덱스, 유지보수, 서버, 권한). 세션·락은 3초마다 다시 불러온다
pub struct DbaTab {
    pub report: String,
    pub info: Option<qurry_core::dba::ReportInfo>,
    pub grid: Option<Grid>,
    pub error: Option<String>,
    pub running: bool,
    pub auto: bool,
    /// 고른 세션이 다시 불러온 뒤 사라졌다 (그 pid). 이때 c / K 는 아무것도 끊지 않는다
    pub gone: Option<String>,
}

/// Medis 식 Redis 브라우저: 왼쪽 키 목록, 오른쪽 값
pub struct RedisTab {
    pub pattern: String,
    pub keys: Vec<KeyRow>,
    pub cursor: String,
    /// 보이는 줄(폴더·키, rows()) 에서 고른 것
    pub sel: usize,
    pub off: usize,
    /// 펼친 폴더 ("dev:sessions")
    pub open: HashSet<String>,
    pub detail: Option<(String, KeyDetail)>,
    /// 문자열 값을 보기 좋게 정리한 것 (JSON 은 들여쓰기). 그릴 때마다 다시 파싱하지 않게 불러올 때 만든다
    pub text: Option<String>,
    pub value: Option<Grid>,
    pub focus_value: bool,
    pub scanning: bool,
    /// 열 때 지정한 키 (스캔이 끝나면 선택)
    pub want: Option<String>,
}

/// 키 브라우저의 한 줄: ":" 로 묶은 폴더, 또는 키 (keys 의 번호)
#[derive(Debug, Clone, PartialEq)]
pub enum KRow {
    Folder { path: String, name: String, depth: usize, count: usize },
    Key { idx: usize, depth: usize },
}

impl RedisTab {
    /// 보이는 줄: 폴더 먼저(이름순), 그다음 키. 펼친 폴더만 안을 보인다 (탐색기 tree::key_folders 와 같은 규칙)
    pub fn rows(&self) -> Vec<KRow> {
        fn level(r: &RedisTab, idx: Vec<usize>, depth: usize, prefix: &str, out: &mut Vec<KRow>) {
            let mut folders: std::collections::BTreeMap<&str, Vec<usize>> = Default::default();
            let mut keys = vec![];
            for i in idx {
                match r.keys[i].key.split(':').nth(depth) {
                    Some(seg) if r.keys[i].key.split(':').count() > depth + 1 => folders.entry(seg).or_default().push(i),
                    _ => keys.push(i),
                }
            }
            for (seg, kids) in folders {
                let path = if prefix.is_empty() { seg.to_string() } else { format!("{prefix}:{seg}") };
                out.push(KRow::Folder { path: path.clone(), name: seg.to_string(), depth, count: kids.len() });
                if r.open.contains(&path) {
                    level(r, kids, depth + 1, &path, out);
                }
            }
            out.extend(keys.into_iter().map(|idx| KRow::Key { idx, depth }));
        }
        let mut out = vec![];
        level(self, (0..self.keys.len()).collect(), 0, "", &mut out);
        out
    }

    /// 고른 줄이 키면 그 키
    pub fn sel_key(&self) -> Option<String> {
        match self.rows().get(self.sel) {
            Some(KRow::Key { idx, .. }) => Some(self.keys[*idx].key.clone()),
            _ => None,
        }
    }

    /// 키가 보이도록 위 폴더를 펼치고 고른다
    pub fn reveal(&mut self, key: &str) -> bool {
        let parts: Vec<&str> = key.split(':').collect();
        for n in 1..parts.len() {
            self.open.insert(parts[..n].join(":"));
        }
        match self.rows().iter().position(|r| matches!(r, KRow::Key { idx, .. } if self.keys[*idx].key == key)) {
            Some(i) => {
                self.sel = i;
                true
            }
            None => false,
        }
    }
}

pub enum TabKind {
    Console(Console),
    Table(TableTab),
    Text(TextTab),
    Redis(RedisTab),
    Dba(DbaTab),
}

pub struct Tab {
    pub id: u64,
    pub title: String,
    pub conn: String,
    pub db: String,
    pub target: String,
    pub kind: TabKind,
    /// 분할된 화면에서 속한 칸 (panes 의 잎)
    pub group: usize,
    /// 되살린 탭: 아직 불러오지 않았다 (처음 보일 때 불러온다)
    pub stale: bool,
}

// ───────── 팝업 ─────────

#[derive(Clone)]
pub enum PAction {
    Act(Action),
    /// DBA 리포트 열기 (id)
    Dba(String),
    /// 히스토리·스니펫에서 고른 쿼리를 콘솔에
    InsertSql(String),
    /// 여러 단계 고르기 (비교·마이그레이션): 고른 값
    Pick(String),
    /// DBA 리포트 행 동작 (템플릿 SQL → 콘솔)
    DbaSql(String),
    /// 내보내기: 형식을 골랐다 → 대상 고르기
    ExportFormat(qurry_core::transfer::Format),
    /// 내보내기: (형식, 클립보드)
    ExportTo(qurry_core::transfer::Format, bool),
    Object { conn: String, db: String, e: IndexEntryLite },
    Tab(u64),
    /// 실행 선택 팝업에서 고른 문장들
    Run { stmts: Vec<String>, explain: bool },
    /// 실행 설정 바꾸기
    Exec(crate::exec::ExecSettings),
}
#[derive(Clone)]
pub struct IndexEntryLite {
    pub schema: String,
    pub name: String,
    pub kind: String,
}
#[derive(Clone)]
pub struct PItem {
    pub label: String,
    pub hint: String,
    pub action: PAction,
}
pub struct Palette {
    pub title: String,
    pub input: TextArea<'static>,
    pub items: Vec<PItem>,
    pub sel: usize,
    pub loading: usize,
    /// 거른 결과 캐시: (검색어, 항목 수) 가 같으면 다시 거르고 정렬하지 않는다 (그릴 때마다·키마다 불린다)
    pub cache: std::cell::RefCell<Option<(String, usize, Vec<usize>)>>,
}

#[derive(Clone, PartialEq)]
pub enum FieldKind {
    Text,
    Secret,
    Toggle,
    Choice(Vec<&'static str>),
}
pub struct Field {
    pub label: &'static str,
    pub value: String,
    pub kind: FieldKind,
}
pub struct ConnForm {
    pub edit: Option<ConnectionConfig>,
    pub fields: Vec<Field>,
    pub sel: usize,
    pub message: Option<(bool, String)>,
}

pub enum Popup {
    /// F1 키맵. 위쪽 Commands 는 고를 수 있다 (sel). record = 새 키를 기다리는 중 (true = 기존 키에 더하기)
    /// sel 은 위쪽 Commands 와 아래 고정 키 목록을 이어서 센다 (Commands 만 바꿀 수 있다)
    Help { sel: usize, scroll: u16, record: Option<bool>, message: Option<String> },
    Message { title: String, text: String },
    /// 한 줄 입력. words = 자동완성 후보 (테이블의 WHERE/ORDER BY 는 컬럼·키워드), complete = 열린 목록 (후보, 고른 것, 바꿀 접두어 글자 수)
    Input { title: String, input: TextArea<'static>, on_ok: Box<dyn FnOnce(&mut App, String)>, words: Vec<String>, complete: Option<(Vec<String>, usize, usize)> },
    Confirm { text: String, on_yes: Box<dyn FnOnce(&mut App)> },
    Palette(Palette),
    ConnForm(ConnForm),
    /// Manage Shown Schemas (트리에 보일 DB·스키마)
    Scope(crate::scope::ScopeEditor),
    /// 편집기 찾기/바꾸기 바 (⌃F)
    Find(crate::find::FindBar),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    NewConsole,
    NewDataSource,
    EditDataSource,
    DeleteDataSource,
    Connect,
    Disconnect,
    Refresh,
    Execute,
    Explain,
    ExplainAnalyze,
    Cancel,
    TxToggle,
    Commit,
    Rollback,
    CloseTab,
    NextTab,
    PrevTab,
    GoToObject,
    SearchEverywhere,
    RecentTabs,
    ExecSettings,
    ClearConsole,
    Ddl,
    EditData,
    Submit,
    Revert,
    AddRow,
    DeleteRow,
    SetNull,
    ExportCsv,
    CopyInList,
    Find,
    DbaReports,
    BackupDb,
    RestoreDb,
    CompareSchemas,
    CompareData,
    RunMigrations,
    ShowDiagram,
    QueryHistory,
    Snippets,
    SaveSnippet,
    ImportData,
    CancelTransfer,
    ToggleExplorer,
    ToggleOutput,
    SplitRight,
    SplitDown,
    Unsplit,
    OtherSplit,
    Complete,
    TrustSshHost,
    Palette,
    CycleFocus,
    ShrinkEditor,
    GrowEditor,
    PaneUp,
    PaneDown,
    PaneLeft,
    PaneRight,
    SplitShrink,
    SplitGrow,
    ExplorerNarrower,
    ExplorerWider,
    ManageSchemas,
    CheckUpdate,
    InstallUpdate,
    Help,
    Quit,
}

/// 액션 팔레트 목록: (액션, 이름, 힌트). 바꿀 수 있는 명령(keys::DEFAULT_KEYS)의 힌트는 현재 키맵에서 가져온다
pub const ACTIONS: &[(Action, &str, &str)] = &[
    (Action::NewConsole, "New Query Console", ""),
    (Action::Execute, "Execute (asks what to run — see Query Execution Settings)", ""),
    (Action::Explain, "Explain Plan", ""),
    (Action::ExplainAnalyze, "Explain Analyze (runs the statement: real times and row counts)", ""),
    (Action::Cancel, "Cancel Running Statement", ""),
    (Action::TxToggle, "Toggle Transaction Mode (Auto/Manual)", ""),
    (Action::Commit, "Commit", ""),
    (Action::Rollback, "Roll Back", ""),
    (Action::ExecSettings, "Query Execution Settings (what Ctrl+Enter runs)", ""),
    (Action::ClearConsole, "Clear Console (delete all text)", "⌘A ⌫ (console)"),
    (Action::SearchEverywhere, "Search Everywhere (objects · columns · tabs · actions)", ""),
    (Action::GoToObject, "Go to Database Object", ""),
    (Action::RecentTabs, "Recent Tabs / Switcher", ""),
    (Action::EditData, "Edit Data (open table)", "F4 / Enter"),
    (Action::Ddl, "Go to DDL", ""),
    (Action::Submit, "Submit Changes", ""),
    (Action::Revert, "Revert Changes", "Ctrl+Z (grid)"),
    (Action::AddRow, "Add Row", "Insert / a"),
    (Action::DeleteRow, "Delete Row", "Delete / d"),
    (Action::SetNull, "Set NULL", "n (grid)"),
    (Action::ExportCsv, "Export Data… (CSV, TSV, JSON, SQL INSERT, Markdown)", "x (grid)"),
    (Action::CopyInList, "Copy Column as IN List  ('a', 'b', …)", "Y (grid)"),
    (Action::Find, "Find / Replace in Editor (regex)", ""),
    (Action::QueryHistory, "Query History… (shared with the GUI)", ""),
    (Action::Snippets, "Snippets…", ""),
    (Action::SaveSnippet, "Save Query as Snippet…", ""),
    (Action::ShowDiagram, "Show Diagram (ERD): foreign keys of a schema, as text + Mermaid", ""),
    (Action::CompareSchemas, "Compare Schemas… (DDL to make one schema look like another)", ""),
    (Action::CompareData, "Compare Table Data… (sync SQL by primary key)", ""),
    (Action::RunMigrations, "Run Migrations… (folder of V1__name.sql files)", ""),
    (Action::BackupDb, "Back Up Database… (pg_dump / mysqldump)", ""),
    (Action::RestoreDb, "Restore Database… (from a .dump / .sql backup)", ""),
    (Action::DbaReports, "Database Admin… (sessions & locks, slow queries, sizes, indexes, vacuum, server, privileges)", ""),
    (Action::ImportData, "Import Data from File… (CSV, TSV, JSON)", ""),
    (Action::CancelTransfer, "Cancel Export/Import", ""),
    (Action::NewDataSource, "New Data Source…", "n (explorer)"),
    (Action::EditDataSource, "Data Source Properties…", "e (explorer)"),
    (Action::DeleteDataSource, "Remove Data Source", "D (explorer)"),
    (Action::Connect, "Connect", "c (explorer)"),
    (Action::Disconnect, "Disconnect", ""),
    (Action::Refresh, "Refresh", "r (explorer · table · Redis)"),
    (Action::ManageSchemas, "Manage Shown Schemas… (databases · schemas in the explorer)", "s (explorer)"),
    (Action::CloseTab, "Close Tab", ""),
    (Action::NextTab, "Next Tab", ""),
    (Action::PrevTab, "Previous Tab", ""),
    (Action::ToggleExplorer, "Database Explorer", ""),
    (Action::ToggleOutput, "Toggle Output / Result", ""),
    (Action::Complete, "Code Completion", ""),
    (Action::SplitRight, "Split Right (focused pane, up to 4)", ""),
    (Action::SplitDown, "Split Down (focused pane, up to 4)", ""),
    (Action::Unsplit, "Unsplit All", ""),
    (Action::OtherSplit, "Go to the Next Split", ""),
    (Action::SplitShrink, "Move Split Divider Left / Up", ""),
    (Action::SplitGrow, "Move Split Divider Right / Down", ""),
    (Action::ExplorerNarrower, "Database Explorer Narrower", ""),
    (Action::ExplorerWider, "Database Explorer Wider", ""),
    (Action::ShrinkEditor, "Shrink Editor (console)", ""),
    (Action::GrowEditor, "Grow Editor (console)", ""),
    (Action::PaneUp, "Go to the Split Above", ""),
    (Action::PaneDown, "Go to the Split Below", ""),
    (Action::PaneLeft, "Go to the Split on the Left", ""),
    (Action::PaneRight, "Go to the Split on the Right", ""),
    (Action::CycleFocus, "Cycle Focus (explorer → editor → results)", ""),
    (Action::Palette, "Actions Palette", ""),
    (Action::TrustSshHost, "Trust SSH Host Key (after checking its fingerprint)", ""),
    (Action::CheckUpdate, "Check for Updates", ""),
    (Action::InstallUpdate, "Update Qurry & Restart (download the latest release)", ""),
    (Action::Help, "Keymap", ""),
    (Action::Quit, "Quit", ""),
];

fn object_items(conn: &str, name: &str, db: &str, es: Vec<IndexEntry>, tag: &str) -> Vec<PItem> {
    es.into_iter()
        .map(|e| PItem {
            label: e.name.clone(),
            hint: format!("{} {} · {name}/{db}{tag}", e.kind, if e.kind == "schema" { String::new() } else { e.schema.clone() }),
            action: PAction::Object { conn: conn.into(), db: db.into(), e: IndexEntryLite { schema: e.schema, name: e.name, kind: e.kind } },
        })
        .collect()
}

/// 컬럼 → 고르면 그 테이블을 연다 (Ctrl+B 는 그 테이블 DDL)
fn column_items(conn: &str, name: &str, db: &str, ts: Vec<qurry_core::meta::CompletionTable>, tag: &str) -> Vec<PItem> {
    ts.into_iter()
        .flat_map(|t| {
            t.columns.into_iter().map(move |(col, ty)| PItem {
                label: col,
                hint: format!("column {ty} · {}.{} · {name}/{db}{tag}", t.schema, t.name),
                action: PAction::Object { conn: conn.into(), db: db.into(), e: IndexEntryLite { schema: t.schema.clone(), name: t.name.clone(), kind: "table".into() } },
            })
        })
        .collect()
}


/// 마지막으로 그린 화면 영역 (마우스 클릭 판정)
#[derive(Default)]
pub struct Hits {
    pub tree: ratatui::layout::Rect,
    /// (탭 id, x 시작, x 끝, y)
    pub tabs: Vec<(u64, u16, u16, u16)>,
    /// 칸마다: (칸, 탭 바, 본문)
    pub groups: Vec<(usize, ratatui::layout::Rect, ratatui::layout::Rect)>,
    /// 나눈 곳마다: (가지 위치, 나눈 영역, 경계선, 방향). 위아래로 나눈 곳은 아래 첫 칸 탭 바의 빈 곳도 경계선으로 잡는다 (grab)
    pub splits: Vec<(crate::panes::Path, ratatui::layout::Rect, ratatui::layout::Rect, ratatui::layout::Rect, SplitDir)>,
    /// Database Explorer 오른쪽 경계선과 화면 본문 (끌어서 폭 조절)
    pub explorer_divider: ratatui::layout::Rect,
    pub main: ratatui::layout::Rect,
    pub editor: ratatui::layout::Rect,
    /// 찾기 바 자리 (편집기 위, 바가 열려 있을 때만)
    pub find: ratatui::layout::Rect,
    pub results: ratatui::layout::Rect,
    pub redis_keys: ratatui::layout::Rect,
    pub redis_value: ratatui::layout::Rect,
    pub palette: ratatui::layout::Rect,
    pub palette_off: usize,
    /// 콘솔 본문(에디터+결과)과 그 경계선 (드래그로 비율 조절)
    pub console: ratatui::layout::Rect,
    pub divider: ratatui::layout::Rect,
    /// 더블클릭 판정: (시각, x, y)
    pub last_click: Option<(std::time::Instant, u16, u16)>,
    /// 콘솔 커서를 그린 화면 칸과 그때 커서 (줄, 칸) — 마우스 위치를 글자 위치로 바꿀 때 (tui-textarea 는 스크롤 위치를 공개하지 않는다)
    pub caret: Option<(u16, u16, usize, usize)>,
}

pub struct App {
    pub hits: Hits,
    pub state: Arc<AppState>,
    pub tx: UnboundedSender<Apply>,
    pub conns: Vec<ConnectionView>,
    pub tree: Tree,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub mru: Vec<u64>,
    /// 도는 내보내기·가져오기 (작업 id, 이름, 진행). 상태줄에 진행 막대
    pub jobs: Vec<(String, String, Arc<qurry_core::transfer::Job>)>,
    /// 다음 Explain 은 ANALYZE (실제로 실행해서 시간·행 수)
    pub analyze_next: bool,
    /// 여러 단계 고르기 중 (비교·마이그레이션)
    pub flow: Option<crate::migrate::Flow>,
    pub focus: Focus,
    pub explorer: bool,
    pub popup: Option<Popup>,
    pub status: String,
    pub quit: bool,
    /// 끝낸 뒤 다시 켠다 (앱 안 업데이트를 마친 뒤)
    pub restart: bool,
    /// 화면 분할 (최대 4칸)
    pub panes: crate::panes::Pane,
    /// 칸마다 보이는 탭
    pub group_active: [Option<u64>; crate::panes::MAX],
    /// 콘솔에서 에디터가 차지하는 비율(%)
    pub console_pct: u16,
    /// Database Explorer 폭 (None = 화면 폭에 맞춰 자동)
    pub explorer_w: Option<u16>,
    /// 마우스로 끄는 중인 것과 지금 위치
    pub drag: Option<Drag>,
    pub drag_pos: (u16, u16),
    /// 자동완성 메타 (target → 테이블·컬럼)
    pub completion: HashMap<String, Arc<complete::Meta>>,
    completion_loading: HashSet<String>,
    /// 명령 단축키 (keymap.json 의 "tui")
    pub keys: crate::keys::Keymap,
    /// 새 버전이 있으면 그 버전 (상태줄에 알린다)
    pub update: Option<String>,
    next_id: u64,
    /// ⌃Enter 로 무엇을 실행할지 (Query Execution Settings, workspace 에 저장)
    pub exec: crate::exec::ExecSettings,
    /// 지난번 탭을 되살렸는지 (Data Source 목록을 처음 읽은 뒤 한 번), 마지막으로 저장한 내용
    restored: bool,
    saved_ws: String,
}

pub fn fuzzy(q: &str, text: &str) -> Option<i64> {
    if q.is_empty() {
        return Some(0);
    }
    let (q, t) = (q.to_lowercase(), text.to_lowercase());
    if t == q {
        return Some(1000);
    }
    if t.starts_with(&q) {
        return Some(800 - t.len() as i64);
    }
    if let Some(i) = t.find(&q) {
        return Some(500 - i as i64 - t.len() as i64 / 10);
    }
    let mut score = 200;
    let mut it = t.char_indices().peekable();
    let mut last = 0usize;
    for qc in q.chars().filter(|c| !c.is_whitespace()) {
        loop {
            match it.next() {
                Some((i, c)) if c == qc => {
                    score -= (i.saturating_sub(last)).min(10) as i64;
                    last = i + 1;
                    break;
                }
                Some(_) => continue,
                None => return None,
            }
        }
    }
    Some(score - t.len() as i64 / 10)
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn color_of(c: &ConnectionView) -> Option<String> {
    c.config.color.clone()
}

impl App {
    pub fn new(state: Arc<AppState>, tx: UnboundedSender<Apply>) -> Self {
        let (user_keys, keys_err) = match state.store.keymap("tui") {
            Ok(k) => (k, None),
            Err(e) => (Default::default(), Some(format!("keymap.json ignored: {e}"))),
        };
        let mut a = App {
            hits: Hits::default(),
            state,
            tx,
            conns: vec![],
            tree: Tree::default(),
            tabs: vec![],
            active: 0,
            mru: vec![],
            jobs: vec![],
            analyze_next: false,
            flow: None,
            focus: Focus::Tree,
            explorer: true,
            popup: None,
            status: "Ready — not sure? Ctrl+P searches everything · F1 keymap".into(),
            quit: false,
            panes: crate::panes::Pane::Leaf(0),
            group_active: [None; crate::panes::MAX],
            console_pct: 42,
            explorer_w: None,
            drag: None,
            drag_pos: (0, 0),
            completion: HashMap::new(),
            completion_loading: HashSet::new(),
            keys: crate::keys::Keymap::new(&user_keys),
            update: None,
            restart: false,
            restored: false,
            saved_ws: String::new(),
            next_id: 1,
            exec: Default::default(),
        };
        if let Some(e) = keys_err {
            a.status = e;
        }
        a.reload_conns();
        a
    }

    pub fn spawn<T: Send + 'static>(&self, fut: impl Future<Output = T> + Send + 'static, apply: impl FnOnce(&mut App, T) + Send + 'static) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let v = fut.await;
            let _ = tx.send(Box::new(move |a: &mut App| apply(a, v)));
        });
    }

    pub fn conn(&self, id: &str) -> Option<&ConnectionView> {
        self.conns.iter().find(|c| c.config.id == id)
    }
    pub fn kind_of(&self, conn: &str) -> DbKind {
        self.conn(conn).map(|c| c.config.kind).unwrap_or(DbKind::Postgres)
    }
    pub fn conn_color(&self, conn: &str) -> Option<String> {
        self.conn(conn).and_then(color_of)
    }

    // ───────── Data Sources ─────────

    pub fn reload_conns(&mut self) {
        let st = self.state.clone();
        self.spawn(async move { st.list_connections().await.map_err(err) }, |a, r| match r {
            Ok(cs) => {
                a.tree.roots = cs
                    .iter()
                    .map(|c| Node {
                        key: c.config.id.clone(),
                        label: c.config.name.clone(),
                        hint: format!("{:?}", c.config.kind).to_lowercase() + if c.config.active_ssh().is_some() { " · ssh" } else { "" },
                        kind: NodeKind::Conn,
                        conn: c.config.id.clone(),
                        db: None,
                        schema: None,
                        name: None,
                        expandable: true,
                    })
                    .collect();
                // 방금 접속된 Data Source: 접속 실패로 비어 있던 테이블 탭을 다시 불러온다
                let back: Vec<String> = cs.iter().filter(|c| c.connected && a.conn(&c.config.id).is_some_and(|o| !o.connected)).map(|c| c.config.id.clone()).collect();
                a.conns = cs;
                a.tree.clamp();
                for c in back {
                    a.retry_failed_tables(&c);
                }
                if !a.restored {
                    a.restored = true;
                    if let Some(v) = a.state.store.workspace("tui") {
                        crate::workspace::restore(a, &v);
                    }
                    a.saved_ws = crate::workspace::to_json(a).to_string();
                }
            }
            Err(e) => a.status = format!("Failed to load data sources: {e}"),
        });
    }

    fn default_db(&self, conn: &str) -> String {
        self.conn(conn).map(|c| sql::default_db(&c.config)).unwrap_or_default()
    }

    // ───────── 트리 ─────────

    pub fn toggle_node(&mut self, n: &Node) {
        if !n.expandable {
            return;
        }
        if self.tree.expanded.contains(&n.key) {
            self.tree.expanded.remove(&n.key);
        } else {
            self.tree.expanded.insert(n.key.clone());
            self.load_children(n.clone());
        }
        self.tree.clamp();
    }

    pub fn load_children(&mut self, n: Node) {
        let key = n.key.clone();
        if self.tree.children.contains_key(&key) || self.tree.loading.contains(&key) {
            return;
        }
        self.tree.loading.insert(key.clone());
        let st = self.state.clone();
        let conn = n.conn.clone();
        let kind = self.kind_of(&conn);
        let cfg = self.conn(&conn).map(|c| c.config.clone());
        let done = move |a: &mut App, key: String, r: Result<Vec<Node>, String>| {
            a.tree.loading.remove(&key);
            match r {
                Ok(nodes) => {
                    a.tree.children.insert(key, nodes);
                }
                Err(e) => {
                    a.tree.children.insert(key.clone(), vec![Node::info(&key, "", format!("⚠ {e}"))]);
                    // 드라이버가 없으면 받을지 묻는다 (받은 뒤 다시 펼친다)
                    if e.contains(qurry_core::plugin::MISSING) && let Some(d) = kind.driver() {
                        let k2 = key.clone();
                        a.popup = Some(Popup::Confirm {
                            text: format!("The {d} driver isn't installed.\n\nDownload it from this Qurry version's GitHub release (its signature is checked) and connect?"),
                            on_yes: Box::new(move |a| {
                                a.status = format!("Installing the {d} driver…");
                                a.spawn(async move { qurry_core::plugin::install(d).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                                    Ok(_) => {
                                        a.status = format!("{d} driver installed");
                                        a.tree.children.remove(&k2);
                                        a.tree.expanded.remove(&k2);
                                        if let Some(n) = a.tree.roots.iter().find(|n| n.key == k2).cloned() {
                                            a.toggle_node(&n);
                                        }
                                    }
                                    Err(e) => a.status = e,
                                });
                            }),
                        });
                    }
                    a.status = if e.contains(qurry_core::tunnel::UNKNOWN_HOST) { format!("{e} — Ctrl+K › Trust SSH Host Key") } else { e };
                }
            }
            a.tree.clamp();
        };
        match n.kind {
            NodeKind::Conn => {
                let cfg = cfg.unwrap();
                let def = sql::default_db(&cfg);
                self.spawn(
                    async move {
                        let dbs = st.list_databases(conn.clone()).await.map_err(err)?;
                        // 접속됨: 다음에 접속하지 않고도 찾도록 검색 캐시를 백그라운드로 채운다
                        let st2 = st.clone();
                        tokio::spawn(async move { st2.warm_search_cache(conn).await });
                        Ok::<_, String>((dbs, def))
                    },
                    move |a, r: Result<(Vec<String>, String), String>| {
                        let r = r.map(|(dbs, def)| {
                            qurry_core::app::visible_dbs(&cfg, dbs, &def)
                                .into_iter()
                                .map(|d| Node {
                                    key: format!("{}|{d}", cfg.id),
                                    label: d.clone(),
                                    hint: if d == def { "default".into() } else { String::new() },
                                    kind: NodeKind::Db,
                                    conn: cfg.id.clone(),
                                    db: Some(d),
                                    schema: None,
                                    name: None,
                                    expandable: true,
                                })
                                .collect::<Vec<_>>()
                        });
                        // 자동으로 펼칠 DB: 보이는 DB 가 하나면 그것, 아니면 기본 DB (보일 때만)
                        let auto = r.as_ref().ok().and_then(|ns| if ns.len() == 1 { ns.first().cloned() } else { ns.iter().find(|n| n.hint == "default").cloned() });
                        done(a, key, r);
                        a.reload_conns();
                        if let Some(n) = auto {
                            a.tree.expanded.insert(n.key.clone());
                            a.load_children(n);
                        }
                    },
                );
            }
            NodeKind::Db => {
                let db = n.db.clone().unwrap();
                let target = sql::target_id(&conn, Some(&db));
                let flat = kind.flat();
                self.spawn(
                    async move {
                        let schemas = st.list_schemas(target.clone()).await.map_err(err)?;
                        if flat {
                            let s = schemas.first().map(|s| s.name.clone()).unwrap_or_else(|| db.clone());
                            let objs = st.list_objects(target, s.clone()).await.map_err(err)?;
                            Ok::<_, String>((schemas, Some((s, objs))))
                        } else {
                            Ok((schemas, None))
                        }
                    },
                    move |a, r| match r {
                        Err(e) => done(a, key, Err(e)),
                        Ok((_, Some((schema, objs)))) => {
                            let groups = a.groups(&key, &n.conn, n.db.as_deref().unwrap(), &schema, objs);
                            done(a, key, Ok(groups));
                        }
                        Ok((schemas, None)) => {
                            let scope = a.conn(&n.conn).map(|c| c.config.scope.clone()).unwrap_or_default();
                            let db = n.db.clone().unwrap_or_default();
                            let user: Vec<_> = schemas.iter().filter(|s| scope.schema_visible(&db, &s.name, s.is_system, false)).collect();
                            let nodes: Vec<Node> = user
                                .iter()
                                .map(|s| Node {
                                    key: format!("{key}|{}", s.name),
                                    label: s.name.clone(),
                                    hint: String::new(),
                                    kind: NodeKind::Schema,
                                    conn: n.conn.clone(),
                                    db: n.db.clone(),
                                    schema: Some(s.name.clone()),
                                    name: None,
                                    expandable: true,
                                })
                                .collect();
                            let auto = if nodes.len() == 1 { nodes.first().cloned() } else { nodes.iter().find(|x| x.label == "public").cloned() };
                            done(a, key, Ok(nodes));
                            if let Some(s) = auto {
                                a.tree.expanded.insert(s.key.clone());
                                a.load_children(s);
                            }
                        }
                    },
                );
            }
            NodeKind::Schema => {
                let (db, schema) = (n.db.clone().unwrap(), n.schema.clone().unwrap());
                let target = sql::target_id(&conn, Some(&db));
                let s2 = schema.clone();
                self.spawn(async move { st.list_objects(target, s2).await.map_err(err) }, move |a, r| {
                    let r = r.map(|objs| a.groups(&key, &n.conn, &db, &schema, objs));
                    done(a, key, r);
                });
            }
            NodeKind::Object(_) if n.is_relation() => {
                let target = sql::target_id(&conn, n.db.as_deref());
                let (schema, name) = (n.schema.clone().unwrap(), n.name.clone().unwrap());
                self.spawn(async move { st.table_detail(target, schema, name).await.map_err(err) }, move |a, r| {
                    let r = r.map(|d| {
                        d.columns
                            .into_iter()
                            .map(|c| Node {
                                key: format!("{key}|c:{}", c.name),
                                label: c.name.clone(),
                                hint: format!("{}{}", c.data_type, if c.nullable { "" } else { " not null" }),
                                kind: NodeKind::Column { ty: c.data_type, pk: c.is_pk },
                                conn: n.conn.clone(),
                                db: n.db.clone(),
                                schema: n.schema.clone(),
                                name: Some(c.name),
                                expandable: false,
                            })
                            .collect()
                    });
                    done(a, key, r);
                });
            }
            _ => {
                self.tree.loading.remove(&key);
            }
        }
    }

    /// 스키마 객체 → 폴더(tables/views/…/keys) + 그 아래 노드. 첫 폴더는 펼친다
    fn groups(&mut self, parent: &str, conn: &str, db: &str, schema: &str, objs: SchemaObjects) -> Vec<Node> {
        let defs: [(&str, &[&str]); 5] = [
            ("tables", &["table", "partitioned", "foreign"]),
            ("views", &["view"]),
            ("materialized views", &["matview"]),
            ("sequences", &["sequence"]),
            ("keys", &["key"]),
        ];
        let mut out = vec![];
        let mk_obj = |gkey: &str, name: &str, kind: &str, hint: String| Node {
            key: format!("{gkey}|o:{name}"),
            label: name.to_string(),
            hint,
            kind: NodeKind::Object(kind.to_string()),
            conn: conn.into(),
            db: Some(db.into()),
            schema: Some(schema.into()),
            name: Some(name.into()),
            expandable: ["table", "view", "matview", "foreign", "partitioned"].contains(&kind),
        };
        for (label, kinds) in defs {
            let items: Vec<_> = objs.relations.iter().filter(|r| kinds.contains(&r.kind.as_str())).collect();
            if items.is_empty() {
                continue;
            }
            let gkey = format!("{parent}|g:{label}");
            let kids = items
                .iter()
                .map(|r| mk_obj(&gkey, &r.name, if r.kind == "key" { "key" } else { &r.kind }, if r.kind == "key" { r.comment.clone().unwrap_or_default() } else { r.comment.clone().unwrap_or_default() }))
                .collect();
            // Redis 키는 ":" 로 폴더를 만든다
            let kids = if label == "keys" { crate::tree::key_folders(&mut self.tree.children, &gkey, kids) } else { kids };
            self.tree.children.insert(gkey.clone(), kids);
            out.push(Node { key: gkey, label: label.into(), hint: items.len().to_string(), kind: NodeKind::Group, conn: conn.into(), db: Some(db.into()), schema: Some(schema.into()), name: None, expandable: true });
        }
        if !objs.routines.is_empty() {
            let gkey = format!("{parent}|g:routines");
            let kids = objs
                .routines
                .iter()
                .map(|r| {
                    let mut n = mk_obj(&gkey, &r.name, &r.kind, format!("({}){}", r.args, r.returns.as_ref().map(|x| format!(" → {x}")).unwrap_or_default()));
                    n.key = format!("{gkey}|o:{}({})", r.name, r.args);
                    n
                })
                .collect();
            self.tree.children.insert(gkey.clone(), kids);
            out.push(Node { key: gkey, label: "routines".into(), hint: objs.routines.len().to_string(), kind: NodeKind::Group, conn: conn.into(), db: Some(db.into()), schema: Some(schema.into()), name: None, expandable: true });
        }
        if let Some(first) = out.first() {
            self.tree.expanded.insert(first.key.clone());
        }
        out
    }

    /// Enter / F4 / 더블클릭
    pub fn activate_node(&mut self, n: &Node, f4: bool) {
        let kind = self.kind_of(&n.conn);
        // 연결·DB·스키마·폴더는 Enter/더블클릭 = 펼치기 (DB 종류와 상관없이). F4 / Ctrl+↓ = 콘솔, Redis 는 키 브라우저
        match &n.kind {
            NodeKind::Conn | NodeKind::Db if f4 && kind == DbKind::Redis => {
                let db = n.db.clone().unwrap_or_else(|| self.default_db(&n.conn));
                self.open_redis(&n.conn, db, None)
            }
            NodeKind::Conn | NodeKind::Db if f4 => self.open_console(&n.conn, n.db.clone()),
            NodeKind::Object(k) if k == "key" => self.open_redis(&n.conn, n.db.clone().unwrap(), n.name.clone()),
            NodeKind::Object(k) if n.is_relation() => self.open_table(&n.conn, n.db.clone().unwrap(), n.schema.clone().unwrap(), n.name.clone().unwrap(), k.clone()),
            NodeKind::Object(k) => {
                let k = if k == "function" || k == "procedure" { "routine".to_string() } else { k.clone() };
                self.open_ddl(&n.conn, n.db.clone().unwrap(), n.schema.clone().unwrap(), n.name.clone().unwrap(), k)
            }
            NodeKind::Column { .. } => {}
            _ => self.toggle_node(n),
        }
    }

    // ───────── 탭 ─────────

    pub(crate) fn push_tab(&mut self, conn: &str, db: String, title: String, kind: TabKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let target = sql::target_id(conn, Some(&db));
        // 새 탭은 포커스가 있는 칸에 연다
        let group = self.focused_group();
        self.tabs.push(Tab { id, title, conn: conn.into(), db, target, kind, group, stale: false });
        self.activate(self.tabs.len() - 1);
        id
    }

    /// 포커스가 있는 칸
    pub fn focused_group(&self) -> usize {
        self.tabs.get(self.active).map(|t| t.group).unwrap_or_else(|| self.panes.leaves()[0])
    }

    /// 칸에 보이는 탭 (index)
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

    /// 되살린 탭을 처음 볼 때: 테이블은 데이터, Redis 는 키 목록, 콘솔은 자동완성·트랜잭션 모드
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

    // ───────── 다시 켤 때 되살리기 (workspace.rs) ─────────

    /// 바뀌었으면 저장 (주기적으로·끝낼 때)
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

    pub fn restore_text(&mut self, conn: &str, db: String, title: String, text: &str) -> Option<u64> {
        self.restored_tab(conn, db, title, TabKind::Text(TextTab { text: text.into(), scroll: 0 }))
    }

    pub fn restore_dba(&mut self, conn: &str, db: String, title: String, report: &str) -> Option<u64> {
        let d = DbaTab { report: report.into(), info: None, grid: None, error: None, running: false, auto: true, gone: None };
        self.restored_tab(conn, db, title, TabKind::Dba(d))
    }

    pub fn restore_redis(&mut self, conn: &str, db: String, title: String, pattern: &str) -> Option<u64> {
        let r = RedisTab { pattern: pattern.into(), keys: vec![], cursor: "0".into(), sel: 0, off: 0, open: HashSet::new(), detail: None, text: None, value: None, focus_value: false, scanning: false, want: None };
        self.restored_tab(conn, db, title, TabKind::Redis(r))
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }
    pub fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// 커밋 안 한 변경이 있는 콘솔 탭들
    fn pending_tabs(&self, conn: Option<&str>) -> Vec<u64> {
        self.tabs.iter().filter(|t| conn.is_none_or(|c| t.conn == c) && matches!(&t.kind, TabKind::Console(c) if c.in_tx)).map(|t| t.id).collect()
    }

    /// 탭 닫기. 커밋 안 한 변경이 있으면 먼저 묻는다 (닫으면 롤백된다)
    pub fn close_tab(&mut self, i: usize) {
        let Some(id) = self.tabs.get(i).map(|t| t.id) else { return };
        if self.pending_tabs(None).contains(&id) {
            self.popup = Some(Popup::Confirm {
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
        if i >= self.tabs.len() {
            return;
        }
        let t = self.tabs.remove(i);
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
        self.mru.retain(|x| *x != t.id);
        if self.group_active[t.group] == Some(t.id) {
            self.group_active[t.group] = None;
        }
        if self.tabs.is_empty() {
            self.active = 0;
            self.focus = Focus::Tree;
            self.panes = crate::panes::Pane::Leaf(0);
            self.group_active = [None; crate::panes::MAX];
            return;
        }
        // 칸이 비면 그 칸을 없앤다
        let emptied = !self.tabs.iter().any(|x| x.group == t.group);
        // 같은 칸에서 최근에 쓴 탭으로. 없으면 (되살린 탭은 최근 목록에 없다) 같은 칸의 옆 탭
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
    }

    /// 분할 해제: 모든 탭을 한 칸으로
    pub fn unsplit(&mut self) {
        for t in &mut self.tabs {
            t.group = 0;
        }
        self.panes = crate::panes::Pane::Leaf(0);
        self.group_active = [None; crate::panes::MAX];
        self.group_active[0] = self.tabs.get(self.active).map(|t| t.id);
    }

    /// 탭을 다른 칸으로 옮기거나 (before 앞으로) 순서를 바꾸고, 새로 나눠 그 칸으로 보낸다
    pub fn drop_tab(&mut self, id: u64, target: DropTarget) {
        let Some(i) = self.tab_index(id) else { return };
        let from = self.tabs[i].group;
        let (g, before) = match target {
            DropTarget::NewSplit(g, dir) => {
                if self.tabs.len() < 2 {
                    // 탭이 하나면 새 콘솔을 만들어 나눈다
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
            // 떠난 칸에는 그 칸에서 최근에 쓴 탭을 보인다
            self.group_active[from] = self.mru.iter().copied().find(|x| *x != id && self.tab_index(*x).is_some_and(|j| self.tabs[j].group == from));
        }
        self.activate(at);
        if !self.tabs.iter().any(|x| x.group == from) {
            self.panes.remove(from);
        }
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

    pub fn open_table(&mut self, conn: &str, db: String, schema: String, table: String, kind: String) {
        if let Some(i) = self.tabs.iter().position(|t| t.conn == conn && t.db == db && matches!(&t.kind, TabKind::Table(tt) if tt.schema == schema && tt.table == table)) {
            return self.activate(i);
        }
        let id = self.push_tab(conn, db, table.clone(), TabKind::Table(TableTab { schema, table, kind, where_: String::new(), order: String::new(), session: uuid::Uuid::new_v4().to_string(), grid: None, running: false, pk: vec![], error: None }));
        self.reload_table(id);
    }

    /// 다시 불러오기 전에: 저장 안 한 편집이 있으면 버릴지 묻는다 (그냥 불러오면 말없이 사라진다)
    pub fn reload_table_checked(&mut self, id: u64) {
        let n = match self.tab_mut(id).map(|t| &t.kind) {
            Some(TabKind::Table(tt)) => tt.grid.as_ref().filter(|g| g.dirty()).map(|g| g.edits.len() + g.deleted.len() + g.added).unwrap_or(0),
            _ => 0,
        };
        if n == 0 {
            return self.reload_table(id);
        }
        self.popup = Some(Popup::Confirm {
            text: format!("{n} pending change(s) are not saved to the database ({} submits them). Discard them and reload?", self.keys.label(Action::Submit)),
            on_yes: Box::new(move |a| a.reload_table(id)),
        });
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
                // 같이 보낸다 (접속이 안 되면 둘 다 기다리느라 두 배로 걸린다)
                let (cols, r) = tokio::join!(st.table_detail(target.clone(), schema, table), st.run_query(target, sqls, Some(500), Some(session)));
                let cols = cols.map(|d| d.columns).unwrap_or_default();
                let pk = cols.iter().filter(|c| c.is_pk).map(|c| c.name.clone()).collect::<Vec<_>>();
                let cols: Vec<(String, String)> = cols.into_iter().map(|c| (c.name, c.data_type)).collect();
                let r = r.map_err(err);
                (pk, cols, r)
            },
            move |a, (pk, cols, r)| {
                // 검색 캐시에서 연 테이블이면 지금 막 접속한 것: 탐색기 표시를 맞춘다
                let conn = a.tab_mut(id).map(|t| t.conn.clone()).unwrap_or_default();
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
                        // ORDER BY 가 한 열 ASC/DESC 면 헤더에 표시
                        g.sort = g.result.columns.iter().enumerate().find_map(|(i, c)| {
                            let q = sql::quote_ident(kind, &c.name);
                            if order == format!("{q} ASC") { Some((i, false)) } else if order == format!("{q} DESC") { Some((i, true)) } else { None }
                        });
                        tt.grid = Some(g);
                        a.status = format!("{n}{} rows", if more { "+" } else { "" });
                    }
                    Err(e) => {
                        // 결과가 없으니 빈 표를 보인다 (컬럼은 그대로). 저장 안 한 편집이 있으면 그 표를 그대로 둔다
                        if !tt.grid.as_ref().is_some_and(|g| g.dirty()) {
                            let columns = match &tt.grid {
                                Some(g) => g.result.columns.clone(),
                                None => cols.into_iter().map(|(name, type_name)| qurry_core::db::ColumnInfo { name, type_name }).collect(),
                            };
                            let empty = qurry_core::db::QueryResult { columns, rows: vec![], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false };
                            tt.grid = Some(Grid::new(empty));
                        }
                        tt.error = Some(e.clone());
                        a.status = e;
                    }
                }
            },
        );
    }

    /// 쿼리가 실패한 채로 있는 테이블 탭(저장 안 한 편집 없음)을 다시 불러온다
    fn retry_failed_tables(&mut self, conn: &str) {
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

    /// 테이블 탭 헤더 정렬: 선택 열로 ORDER BY ↑ → ↓ → 해제 (GUI 의 Sort via ORDER BY)
    pub fn sort_table(&mut self, id: u64) {
        let Some(conn) = self.tab_mut(id).map(|t| t.conn.clone()) else { return };
        let kind = self.kind_of(&conn);
        let Some(TabKind::Table(tt)) = self.tab_mut(id).map(|t| &mut t.kind) else { return };
        let Some(g) = &tt.grid else { return };
        if g.cols() == 0 {
            return;
        }
        if g.dirty() {
            self.status = "Save (Ctrl+S) or revert (Ctrl+Z) pending changes before sorting".into();
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
        self.spawn(async move { st.object_ddl(target, schema, name, kind).await.map_err(err) }, move |a, r| match r {
            Ok(text) => {
                if let Some(i) = a.tabs.iter().position(|t| t.title == title && t.conn == conn) {
                    a.activate(i);
                } else {
                    a.push_tab(&conn, db2, title, TabKind::Text(TextTab { text, scroll: 0 }));
                }
            }
            Err(e) => a.status = format!("DDL failed: {e}"),
        });
    }

    pub fn open_redis(&mut self, conn: &str, db: String, key: Option<String>) {
        let id = match self.tabs.iter().position(|t| t.conn == conn && t.db == db && matches!(t.kind, TabKind::Redis(_))) {
            Some(i) => {
                self.activate(i);
                self.tabs[i].id
            }
            None => {
                let name = self.conn(conn).map(|c| c.config.name.clone()).unwrap_or_default();
                let id = self.push_tab(conn, db.clone(), format!("{name} [{db}]"), TabKind::Redis(RedisTab { pattern: "*".into(), keys: vec![], cursor: "0".into(), sel: 0, off: 0, open: HashSet::new(), detail: None, text: None, value: None, focus_value: false, scanning: false, want: key.clone() }));
                self.redis_scan(id, true);
                id
            }
        };
        if let Some(k) = key {
            if let Some(t) = self.tab_mut(id) {
                if let TabKind::Redis(r) = &mut t.kind {
                    if !r.reveal(&k) {
                        r.want = Some(k.clone());
                    }
                }
            }
            self.redis_load_key(id, k);
        }
    }

    pub fn redis_scan(&mut self, id: u64, reset: bool) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Redis(r) = &mut t.kind else { return };
        r.scanning = true;
        let (cursor, pattern) = (if reset { "0".to_string() } else { r.cursor.clone() }, r.pattern.clone());
        let st = self.state.clone();
        self.spawn(async move { st.redis_scan(target, cursor, pattern, 500, None).await.map_err(err) }, move |a, res| {
            let Some(t) = a.tab_mut(id) else { return };
            let TabKind::Redis(r) = &mut t.kind else { return };
            r.scanning = false;
            match res {
                Ok(page) => {
                    if reset {
                        r.keys.clear();
                        r.sel = 0;
                        r.off = 0;
                    }
                    let mut seen: HashSet<String> = r.keys.iter().map(|k| k.key.clone()).collect();
                    for k in page.keys {
                        if seen.insert(k.key.clone()) {
                            r.keys.push(k);
                        }
                    }
                    r.keys.sort_by(|a, b| a.key.cmp(&b.key));
                    r.sel = r.sel.min(r.rows().len().saturating_sub(1));
                    r.cursor = page.cursor;
                    let n = r.keys.len();
                    let more = r.cursor != "0";
                    let first = match r.want.take() {
                        Some(w) => {
                            r.reveal(&w);
                            None
                        }
                        // 첫 줄이 키면 바로 보여 준다 (폴더면 펼쳐서 고르게)
                        None if reset && r.detail.is_none() => r.sel_key(),
                        None => None,
                    };
                    a.status = format!("{n}{} keys", if more { "+ (m: scan more)" } else { "" });
                    if let Some(k) = first {
                        a.redis_load_key(id, k);
                    }
                }
                Err(e) => a.status = e,
            }
        });
    }

    pub fn redis_load_key(&mut self, id: u64, key: String) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let st = self.state.clone();
        let k2 = key.clone();
        self.spawn(async move { st.redis_key(target, k2, Some(1000)).await.map_err(err) }, move |a, res| {
            let Some(t) = a.tab_mut(id) else { return };
            let TabKind::Redis(r) = &mut t.kind else { return };
            match res {
                Ok(d) => {
                    r.value = redis_grid(&d);
                    r.text = d.value.as_str().map(|s| match serde_json::from_str::<Value>(s) {
                        Ok(v) if v.is_object() || v.is_array() => serde_json::to_string_pretty(&v).unwrap_or_default(),
                        _ => s.to_string(),
                    });
                    r.detail = Some((key, d));
                }
                Err(e) => {
                    r.detail = None;
                    r.value = None;
                    a.status = e;
                }
            }
        });
    }

    /// Redis 명령 실행 후 키 다시 읽기
    pub fn redis_cmd(&mut self, id: u64, args: Vec<String>, rescan: bool) {
        if self.tab_mut(id).is_none() {
            return;
        }
        if self.conn(&self.tabs[self.tab_index(id).unwrap()].conn).map(|c| c.config.read_only).unwrap_or(false) {
            self.status = "Data source is read-only".into();
            return;
        }
        let t = self.tab_mut(id).unwrap();
        let target = t.target.clone();
        let key = match &t.kind {
            TabKind::Redis(r) => r.detail.as_ref().map(|d| d.0.clone()),
            _ => None,
        };
        let st = self.state.clone();
        let shown = args.join(" ");
        self.spawn(async move { st.redis_exec(target, args).await.map_err(err) }, move |a, res| match res {
            Ok(v) => {
                a.status = format!("{shown} → {}", crate::ui::format_reply(&v).lines().next().unwrap_or(""));
                if rescan {
                    a.redis_scan(id, true);
                }
                if let Some(k) = key {
                    a.redis_load_key(id, k);
                }
            }
            Err(e) => a.status = e,
        });
    }

    // ───────── 실행 ─────────

    pub fn execute(&mut self, explain: bool) {
        self.execute_checked(explain, false)
    }

    /// 무엇을 실행할지는 실행 설정(self.exec)을 따른다. 물어봐야 하면 고르는 창을 띄운다 (DataGrip)
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
        let plan = if let Some(((r0, c0), (r1, c1))) = c.editor.selection_range() {
            crate::exec::plan_selection(&selected_text(c.editor.lines(), r0, c0, r1, c1), kind, self.exec)
        } else {
            let (row, col) = c.editor.cursor();
            crate::exec::plan(&c.editor.lines().join("\n"), byte_offset(c.editor.lines(), row, col), kind, self.exec)
        };
        match plan {
            crate::exec::Plan::Nothing => {}
            crate::exec::Plan::Run(stmts) => self.run_stmts(stmts, explain, confirmed),
            crate::exec::Plan::Ask(opts) => {
                let items = opts.into_iter().map(|(label, stmts)| PItem { label, hint: String::new(), action: PAction::Run { stmts, explain } }).collect();
                self.popup = Some(Popup::Palette(Palette { title: "Execute".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
            }
        }
    }

    /// 실행 설정 (DataGrip Settings › Database › Query Execution). 고르면 바로 바뀌고 창은 그대로
    pub fn palette_exec(&mut self) {
        use crate::exec::{ExecSettings, Inside, Outside, Selection};
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

    /// confirmed = 위험 문장(WHERE 없는 UPDATE/DELETE, DROP, TRUNCATE) 확인을 받았는지
    pub fn run_stmts(&mut self, stmts: Vec<String>, explain: bool, confirmed: bool) {
        self.run_stmts_checked(stmts, explain, confirmed, confirmed)
    }

    /// unsafe_ok = 위험 문장 확인을 받았다, prod_ok = Production 확인(바뀔 행 수)을 받았다
    fn run_stmts_checked(&mut self, stmts: Vec<String>, explain: bool, confirmed: bool, prod_ok: bool) {
        let analyze = explain && self.analyze_next;
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let id = t.id;
        let target = t.target.clone();
        let conn_name = self.conn(&t.conn).map(|c| c.config.name.clone()).unwrap_or_default();
        // 히스토리 (GUI 와 같은 기록)
        let (hist_conn, hist_db, hist) = (t.conn.clone(), t.db.clone(), self.state.clone());
        let prod = self.tabs.get(self.active).and_then(|t| self.conn(&t.conn)).filter(|c| c.config.options.confirm_changes).map(|c| (c.config.name.clone(), self.tabs[self.active].target.clone()));
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
            self.popup = Some(Popup::Confirm { text: format!("{why}:\n  {preview}\nRun it anyway?"), on_yes: Box::new(move |a| a.run_stmts_checked(stmts, explain, true, prod_ok)) });
            return;
        }
        // Production Data Source: 바꿀 행 수를 세어 보여 주고 묻는다 (세는 것은 SELECT count(*) — 아무것도 바꾸지 않는다)
        if let Some((name, target)) = prod
            && !prod_ok
            && !explain
        {
            let writes: Vec<String> = stmts.iter().filter(|s| !qurry_core::guard::is_read(s, kind)).cloned().collect();
            if !writes.is_empty() {
                let st = self.state.clone();
                self.status = "Counting the rows this will change…".into();
                self.spawn(
                    async move {
                        let mut out = vec![];
                        for w in writes {
                            let n = st.preview_impact(target.clone(), w.clone()).await;
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
                                    Ok(None) => "can't count this statement".into(),
                                    Err(e) => format!("can't count: {}", e.to_string().lines().next().unwrap_or("")),
                                };
                                format!("  {head}\n    → {n}")
                            })
                            .collect();
                        a.status.clear();
                        a.popup = Some(Popup::Confirm {
                            text: format!("Production data source \"{name}\" — this changes data:\n\n{}\n\nRun it?", lines.join("\n")),
                            on_yes: Box::new(move |a| a.run_stmts_checked(stmts, explain, true, true)),
                        });
                    },
                );
                return;
            }
        }
        // EXPLAIN ANALYZE 는 문장을 실제로 실행한다: 데이터를 바꾸는 문장이면 묻는다
        if analyze
            && !confirmed
            && let Some(s) = stmts.iter().find(|s| !qurry_core::guard::is_read(s, kind))
        {
            let preview: String = s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
            self.popup = Some(Popup::Confirm {
                text: format!("Explain Analyze really runs the statement (it changes data):\n  {preview}\nRun it anyway?"),
                on_yes: Box::new(move |a| {
                    a.analyze_next = true;
                    a.run_stmts(stmts, explain, true);
                    a.analyze_next = false;
                }),
            });
            return;
        }
        c.running = true;
        let session = c.session.clone();
        let st = self.state.clone();
        let stmts2 = stmts.clone();
        self.status = "Executing…".into();
        self.spawn(
            async move {
                let mut out = vec![];
                for s in stmts2 {
                    let q = if explain {
                        let bare = s.trim_end_matches(';');
                        match (kind, analyze) {
                            // PG: JSON 으로 받아 트리 표 (% · 경고) 로 바꾼다
                            (DbKind::Postgres, false) => format!("EXPLAIN (FORMAT JSON) {bare}"),
                            (DbKind::Postgres, true) => format!("EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) {bare}"),
                            (DbKind::Mysql, false) => format!("EXPLAIN {bare}"),
                            (DbKind::Mysql, true) => format!("EXPLAIN ANALYZE {bare}"),
                            (DbKind::Redis, _) => s.clone(),
                            // 드라이버: 대부분 EXPLAIN 을 받는다 (SQL Server 는 SHOWPLAN 이라 그대로 실행된다)
                            (_, true) => format!("EXPLAIN ANALYZE {bare}"),
                            (_, false) => format!("EXPLAIN {bare}"),
                        }
                    } else {
                        s.clone()
                    };
                    let r = st.run_query(target.clone(), q, Some(500), Some(session.clone())).await.map_err(err);
                    let stop = r.is_err();
                    out.push((s, r));
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
                // Auto 모드에서 이미 확정된 변경 문장 수 (중간에 실패하면 알린다: 앞의 것들은 롤백되지 않는다)
                let mut committed = 0;
                let mut failed = false;
                for (i, (s, r)) in out.into_iter().enumerate() {
                    // 실행한 문장은 한 줄로, 너무 길면 앞부분만
                    let echo: String = s.replace('\n', " ").chars().take(300).collect();
                    c.log(false, format!("{conn_name}> {echo}{}", if s.chars().count() > 300 { "…" } else { "" }));
                    if r.is_ok() {
                        let _ = hist.history.add(&hist_conn, Some(&hist_db), &s);
                    }
                    match r {
                        Ok(res) => {
                            // PG 실행 계획: 들여쓴 연산 · 몫(%) · 경고 표
                            let res = match (explain && kind == DbKind::Postgres, res.rows.first().and_then(|r| r.first())) {
                                (true, Some(v)) => match qurry_core::plan::parse_pg(&crate::grid::raw_text(v).unwrap_or_default()) {
                                    Ok(p) => qurry_core::db::QueryResult { elapsed_ms: res.elapsed_ms, in_transaction: res.in_transaction, ..qurry_core::plan::to_result(&p) },
                                    Err(_) => res,
                                },
                                _ => res,
                            };
                            c.in_tx = res.in_transaction;
                            if !res.in_transaction && !qurry_core::guard::is_read(&s, kind) {
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
                                status = msg;
                                if i + 1 == n && c.grid.is_none() {
                                    c.show_output = true;
                                }
                            }
                        }
                        Err(e) => {
                            c.log(true, e.clone());
                            if committed > 0 {
                                c.log(true, format!("Note: {committed} earlier statement(s) of this run were already committed (Auto mode) and were not rolled back"));
                            }
                            c.show_output = true;
                            failed = true;
                            status = format!("Error: {}", e.lines().next().unwrap_or(""));
                        }
                    }
                }
                a.status = status;
                // 오류가 나도 세션에는 트랜잭션이 열려 있을 수 있다 (Manual 첫 문장 실패 등) → Commit/Roll back 표시를 맞춘다
                if failed {
                    let st = a.state.clone();
                    let session = a.tab_mut(id).and_then(|t| if let TabKind::Console(c) = &t.kind { Some(c.session.clone()) } else { None });
                    if let Some(session) = session {
                        a.spawn(async move { st.tx_state(session).await }, move |a, in_tx| {
                            if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                                c.in_tx = in_tx;
                            }
                        });
                    }
                }
            },
        );
    }

    pub fn fetch_more(&mut self) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let id = t.id;
        let g = match &mut t.kind {
            TabKind::Console(c) => c.grid.as_mut(),
            TabKind::Table(tt) => tt.grid.as_mut(),
            _ => None,
        };
        let Some(g) = g else { return };
        if !g.wants_more() {
            return;
        }
        g.loading_more = true;
        let session = g.more_session.clone().unwrap();
        let st = self.state.clone();
        self.spawn(async move { st.fetch_more(session, Some(500)).await.map_err(err) }, move |a, r| {
            let Some(t) = a.tab_mut(id) else { return };
            let g = match &mut t.kind {
                TabKind::Console(c) => c.grid.as_mut(),
                TabKind::Table(tt) => tt.grid.as_mut(),
                _ => None,
            };
            let Some(g) = g else { return };
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
                    g.result.truncated = false;
                    a.status = e;
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
                        Ok(()) => {
                            if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                                c.in_tx = false;
                                c.log(false, if commit { "Committed".into() } else { "Rolled back".into() });
                            }
                            a.status = if commit { "Committed".into() } else { "Rolled back".into() };
                        }
                        Err(e) => a.status = e,
                    },
                );
            }
            _ => {}
        }
    }

    // ───────── 테이블 편집 ─────────

    pub fn submit(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let ro = self.conn(&t.conn).map(|c| c.config.read_only).unwrap_or(false);
        let (id, target) = (t.id, t.target.clone());
        let TabKind::Table(tt) = &t.kind else { return };
        let Some(g) = &tt.grid else { return };
        if !g.dirty() {
            self.status = "No pending changes".into();
            return;
        }
        if ro {
            self.status = "Data source is read-only".into();
            return;
        }
        let q = |s: &str| sql::quote_ident(kind, s);
        let table = format!("{}.{}", q(&tt.schema), q(&tt.table));
        let cols = &g.result.columns;
        let pk_idx: Vec<usize> = tt.pk.iter().filter_map(|p| cols.iter().position(|c| &c.name == p)).collect();
        let where_of = |r: usize| -> Option<String> {
            if pk_idx.is_empty() || pk_idx.len() != tt.pk.len() {
                return None;
            }
            Some(pk_idx.iter().map(|&i| format!("{} = {}", q(&cols[i].name), sql::literal(kind, raw_text(&g.result.rows[r][i]).as_deref()))).collect::<Vec<_>>().join(" AND "))
        };
        let mut stmts = vec![];
        let base = g.rows() - g.added;
        let mut rows_with_edits: Vec<usize> = g.edits.keys().map(|(r, _)| *r).collect();
        rows_with_edits.dedup();
        for r in rows_with_edits.iter().copied().filter(|r| *r < base && !g.deleted.contains(r)) {
            let sets: Vec<String> = g.edits.iter().filter(|((rr, _), _)| *rr == r).map(|((_, c), v)| format!("{} = {}", q(&cols[*c].name), sql::literal(kind, v.as_deref()))).collect();
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
                    set.iter().map(|(c, _)| q(&cols[*c].name)).collect::<Vec<_>>().join(", "),
                    set.iter().map(|(_, v)| sql::literal(kind, v.as_deref())).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        let n = stmts.len();
        let st = self.state.clone();
        self.status = format!("Submitting {n} statement(s)…");
        self.spawn(async move { st.execute_batch(target, stmts, true).await.map_err(err) }, move |a, r| match r {
            Ok(rows) => {
                a.status = format!("{n} statement(s) submitted, {rows} row(s) affected");
                a.reload_table(id);
            }
            Err(e) => a.popup = Some(Popup::Message { title: "Submit failed".into(), text: e }),
        });
    }

    // ───────── 연결 ─────────

    pub fn connect(&mut self, conn: &str) {
        if let Some(n) = self.tree.roots.iter().find(|n| n.conn == conn).cloned() {
            if !self.tree.expanded.contains(&n.key) {
                self.toggle_node(&n);
            }
        }
    }

    pub fn disconnect(&mut self, conn: &str) {
        let n = self.pending_tabs(Some(conn)).len();
        if n > 0 {
            let c = conn.to_string();
            self.popup = Some(Popup::Confirm {
                text: format!("{n} console(s) on this data source have uncommitted changes. Roll them back and disconnect?"),
                on_yes: Box::new(move |a| a.disconnect_now(&c)),
            });
            return;
        }
        self.disconnect_now(conn);
    }

    pub fn disconnect_now(&mut self, conn: &str) {
        let st = self.state.clone();
        let c = conn.to_string();
        self.spawn(async move { st.disconnect(c).await.map_err(err) }, {
            let conn = conn.to_string();
            move |a, r| {
                a.forget_completion(&conn);
                a.tree.forget(&conn);
                a.tree.expanded.retain(|k| !(k == &conn || k.starts_with(&format!("{conn}|"))));
                a.tree.clamp();
                a.status = match r {
                    Ok(()) => "Disconnected".into(),
                    Err(e) => e,
                };
                a.reload_conns();
            }
        });
    }

    pub fn refresh_node(&mut self, n: &Node) {
        let key = n.key.clone();
        // 스키마가 바뀌었을 수 있으니 자동완성 메타도 다시 읽게 한다
        self.forget_completion(&n.conn.clone());
        self.tree.forget(&key);
        if self.tree.expanded.contains(&key) {
            self.load_children(n.clone());
        }
        self.reload_conns();
    }

    pub fn conn_form(&mut self, edit: Option<ConnectionConfig>) {
        let c = edit.clone().unwrap_or(ConnectionConfig {
            id: String::new(),
            name: String::new(),
            kind: DbKind::Postgres,
            host: "localhost".into(),
            port: 5432,
            user: String::new(),
            database: String::new(),
            scope: Default::default(),
            password_saved: false,
            ssh_secret_saved: false,
            ssh: None,
            color: None,
            read_only: false,
            options: Default::default(), ssl: None, redis: None,
        });
        let ssh = c.ssh.clone().unwrap_or(SshConfig { enabled: false, host: String::new(), port: 22, user: String::new(), auth: SshAuth::Key, key_path: "~/.ssh/id_ed25519".into() });
        let kind = serde_json::to_value(c.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "postgres".into());
        let f = |label, value: String, kind| Field { label, value, kind };
        let saved = |b: bool| if b { "<saved>".to_string() } else { String::new() };
        let color = crate::ui::color_name(c.color.as_deref());
        let mut colors: Vec<&'static str> = std::iter::once("none").chain(crate::ui::COLOR_PRESETS.iter().map(|p| p.0)).collect();
        if color == "custom" {
            colors.push("custom");
        }
        self.popup = Some(Popup::ConnForm(ConnForm {
            fields: vec![
                f("Name", c.name.clone(), FieldKind::Text),
                f("Color", color.into(), FieldKind::Choice(colors)),
                f("Type", kind, FieldKind::Choice(vec!["postgres", "mysql", "redis", "sqlite", "duckdb", "mssql", "clickhouse", "mongodb", "oracle"])),
                f("Host", c.host.clone(), FieldKind::Text),
                f("Port", c.port.to_string(), FieldKind::Text),
                f("User", c.user.clone(), FieldKind::Text),
                f("Password", saved(c.password_saved), FieldKind::Secret),
                f("Database", c.database.clone(), FieldKind::Text),
                f("Read-only", if c.read_only { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("Production", if c.options.confirm_changes { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("SSH tunnel", if ssh.enabled { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("SSH host", ssh.host.clone(), FieldKind::Text),
                f("SSH port", ssh.port.to_string(), FieldKind::Text),
                f("SSH user", ssh.user.clone(), FieldKind::Text),
                f("SSH auth", if ssh.auth == SshAuth::Key { "key" } else { "password" }.into(), FieldKind::Choice(vec!["key", "password"])),
                f("SSH key file", ssh.key_path.clone(), FieldKind::Text),
                f("SSH secret", saved(c.ssh_secret_saved), FieldKind::Secret),
                f("SSL", if c.active_ssl().is_some() { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("SSL mode", c.ssl.as_ref().map(|x| x.mode().to_string()).unwrap_or_else(|| "require".into()), FieldKind::Choice(vec!["require", "verify-ca", "verify-full"])),
                f("CA file", c.ssl.as_ref().map(|x| x.ca_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Client cert", c.ssl.as_ref().map(|x| x.cert_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Client key", c.ssl.as_ref().map(|x| x.key_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Redis mode", c.redis_mode().into(), FieldKind::Choice(vec!["standalone", "sentinel", "cluster"])),
                f("Sentinel master", c.redis.as_ref().map(|x| x.sentinel_master.clone()).unwrap_or_default(), FieldKind::Text),
                f("Redis nodes", c.redis.as_ref().map(|x| x.nodes.clone()).unwrap_or_default(), FieldKind::Text),
            ],
            edit,
            sel: 0,
            message: None,
        }));
    }

    /// 폼 → (설정, 비밀번호, SSH 비밀)
    pub fn form_config(form: &ConnForm) -> Result<(ConnectionConfig, Option<String>, Option<String>), String> {
        let v = |l: &str| form.fields.iter().find(|f| f.label == l).map(|f| f.value.clone()).unwrap_or_default();
        let mut c = form.edit.clone().unwrap_or(ConnectionConfig {
            id: String::new(),
            name: String::new(),
            kind: DbKind::Postgres,
            host: String::new(),
            port: 0,
            user: String::new(),
            database: String::new(),
            scope: Default::default(),
            password_saved: false,
            ssh_secret_saved: false,
            ssh: None,
            color: None,
            read_only: false,
            options: Default::default(), ssl: None, redis: None,
        });
        c.kind = serde_json::from_value(serde_json::Value::String(v("Type"))).unwrap_or(DbKind::Postgres);
        c.host = v("Host").trim().into();
        c.port = v("Port").trim().parse().map_err(|_| "Port must be a number".to_string())?;
        c.user = v("User").trim().into();
        c.database = v("Database").trim().into();
        c.name = if v("Name").trim().is_empty() { format!("{}@{}", c.user, c.host) } else { v("Name").trim().into() };
        c.read_only = v("Read-only") == "on";
        c.options.confirm_changes = v("Production") == "on";
        // custom = GUI 에서 고른 프리셋 밖의 색: 그대로 둔다
        match v("Color").as_str() {
            "custom" => {}
            name => c.color = crate::ui::COLOR_PRESETS.iter().find(|p| p.0 == name).map(|p| p.1.to_string()),
        }
        c.ssh = Some(SshConfig {
            enabled: v("SSH tunnel") == "on",
            host: v("SSH host").trim().into(),
            port: v("SSH port").trim().parse().unwrap_or(22),
            user: v("SSH user").trim().into(),
            auth: if v("SSH auth") == "password" { SshAuth::Password } else { SshAuth::Key },
            key_path: v("SSH key file").trim().into(),
        });
        c.ssl = Some(SslConfig { enabled: v("SSL") == "on", mode: v("SSL mode"), ca_file: v("CA file").trim().into(), cert_file: v("Client cert").trim().into(), key_file: v("Client key").trim().into() });
        c.redis = if c.kind == DbKind::Redis { Some(RedisMode { mode: v("Redis mode"), sentinel_master: v("Sentinel master").trim().into(), nodes: v("Redis nodes").trim().into() }) } else { None };
        let secret = |s: String| if s == "<saved>" { None } else { Some(s) };
        Ok((c, secret(v("Password")), secret(v("SSH secret"))))
    }

    // ───────── 팝업: 팔레트 ─────────

    pub fn palette_actions(&mut self) {
        let items = ACTIONS
            .iter()
            .filter(|(a, ..)| *a != Action::Palette)
            .map(|(a, n, k)| PItem { label: n.to_string(), hint: if k.is_empty() { self.keys.label(*a) } else { k.to_string() }, action: PAction::Act(*a) })
            .collect();
        self.popup = Some(Popup::Palette(Palette { title: "Actions".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    pub fn palette_tabs(&mut self) {
        // 최근에 본 탭 → 아직 안 본 탭(되살린 뒤 고르지 않은 탭) 순
        let seen = self.mru.iter().filter_map(|id| self.tabs.iter().find(|t| t.id == *id));
        let rest = self.tabs.iter().filter(|t| !self.mru.contains(&t.id));
        let items = seen.chain(rest).map(|t| PItem { label: t.title.clone(), hint: t.db.clone(), action: PAction::Tab(t.id) }).collect();
        self.popup = Some(Popup::Palette(Palette { title: "Recent Tabs".into(), input: TextArea::default(), items, sel: 1, loading: 0, cache: Default::default() }));
    }

    /// Search Everywhere (GUI 의 ⇧⇧): 최근 탭 + 액션 + DB 객체를 한 목록에서
    pub fn palette_everywhere(&mut self) {
        let mut items: Vec<PItem> = self.mru.iter().filter_map(|id| self.tabs.iter().find(|t| t.id == *id)).map(|t| PItem { label: t.title.clone(), hint: format!("tab · {}", t.db), action: PAction::Tab(t.id) }).collect();
        items.extend(ACTIONS.iter().filter(|(a, ..)| *a != Action::SearchEverywhere).map(|(a, n, k)| {
            let key = if k.is_empty() { self.keys.label(*a) } else { k.to_string() };
            PItem { label: n.to_string(), hint: format!("action · {key}"), action: PAction::Act(*a) }
        }));
        self.palette_objects("Search Everywhere", items);
    }

    /// Go to Database Object: 연결된 Data Source 의 기본 DB + 펼친 DB
    /// Go to Database Object / Search Everywhere: GUI 처럼 연결된 Data Source 마다 보이는 DB (최대 8개) 의 객체.
    /// Redis 와 single DB 는 기본 DB 만
    pub fn palette_objects(&mut self, title: &str, items: Vec<PItem>) {
        let conns: Vec<ConnectionConfig> = self.conns.iter().filter(|c| c.connected).map(|c| c.config.clone()).collect();
        // 접속하지 않은 Data Source 는 지난번에 읽어 둔 목록에서 (고르면 그때 접속한다)
        let mut items = items;
        let columns = title == "Search Everywhere";
        for c in self.state.cached_index().unwrap_or_default() {
            let (conn, db) = qurry_core::app::split_target(&c.target);
            let Some(cv) = self.conns.iter().find(|x| x.config.id == conn && !x.connected) else { continue };
            let (name, db) = (cv.config.name.clone(), db.unwrap_or_default().to_string());
            items.extend(object_items(conn, &name, &db, c.objects, " (cached)"));
            if columns {
                items.extend(column_items(conn, &name, &db, c.columns, " (cached)"));
            }
        }
        let empty = conns.is_empty() && items.iter().all(|i| !matches!(i.action, PAction::Object { .. }));
        self.popup = Some(Popup::Palette(Palette { title: title.into(), input: TextArea::default(), items, sel: 0, loading: conns.len(), cache: Default::default() }));
        if empty {
            self.status = "No connected data sources — connect one (Enter in the explorer) to search its objects".into();
        }
        for cfg in conns {
            let def = sql::default_db(&cfg);
            if cfg.kind == DbKind::Redis || cfg.options.single_db {
                self.index_into_palette(&cfg, vec![def]);
                continue;
            }
            let st = self.state.clone();
            let id = cfg.id.clone();
            self.spawn(async move { st.list_databases(id).await.map_err(err) }, move |a, r: Result<Vec<String>, String>| {
                let mut dbs = r.map(|dbs| qurry_core::app::visible_dbs(&cfg, dbs, &def)).unwrap_or_default();
                // 기본 DB 먼저 (DB 목록을 못 읽어도 기본 DB 는 찾는다)
                dbs.retain(|d| d != &def);
                dbs.insert(0, def);
                dbs.truncate(8);
                a.index_into_palette(&cfg, dbs);
            });
        }
    }

    /// 열린 팔레트에 DB 들의 객체를 더한다 (Data Source 하나 몫의 loading 을 DB 수로 바꾼다)
    fn index_into_palette(&mut self, cfg: &ConnectionConfig, dbs: Vec<String>) {
        // Search Everywhere 는 컬럼도 찾는다 (GUI 의 All / Symbols)
        let columns = match &mut self.popup {
            Some(Popup::Palette(p)) => {
                p.loading = (p.loading + dbs.len()).saturating_sub(1);
                p.title == "Search Everywhere" && cfg.kind != DbKind::Redis
            }
            _ => return,
        };
        for db in dbs {
            if columns {
                self.columns_into_palette(cfg, db.clone());
            }
            let st = self.state.clone();
            let target = sql::target_id(&cfg.id, Some(&db));
            let (conn, name) = (cfg.id.clone(), cfg.name.clone());
            self.spawn(async move { st.object_index(target).await.map_err(err) }, move |a, r: Result<Vec<IndexEntry>, String>| {
                if let Err(e) = &r {
                    a.status = format!("{name}/{db}: {e}");
                }
                if let Some(Popup::Palette(p)) = &mut a.popup {
                    p.loading = p.loading.saturating_sub(1);
                    if let Ok(es) = r {
                        p.items.extend(object_items(&conn, &name, &db, es, ""));
                    }
                }
            });
        }
    }

    /// 컬럼 → 고르면 그 테이블을 연다 (Ctrl+B 는 그 테이블 DDL)
    fn columns_into_palette(&mut self, cfg: &ConnectionConfig, db: String) {
        let st = self.state.clone();
        let target = sql::target_id(&cfg.id, Some(&db));
        let (conn, name) = (cfg.id.clone(), cfg.name.clone());
        self.spawn(async move { st.completion_meta(target).await.map_err(err) }, move |a, r| {
            let (Some(Popup::Palette(p)), Ok(ts)) = (&mut a.popup, r) else { return };
            p.items.extend(column_items(&conn, &name, &db, ts, ""));
        });
    }

    pub fn palette_matches(p: &Palette) -> Vec<&PItem> {
        let q = p.input.lines().join("");
        let mut cache = p.cache.borrow_mut();
        if !cache.as_ref().is_some_and(|(cq, n, _)| *cq == q && *n == p.items.len()) {
            // 컬럼은 같은 이름의 테이블·액션보다 뒤로 (GUI 의 All 과 같게)
            let mut v: Vec<(i64, usize)> = p.items.iter().enumerate().filter_map(|(i, it)| fuzzy(&q, &it.label).map(|s| (s - if it.hint.starts_with("column ") { 50 } else { 0 }, i))).collect();
            if !q.is_empty() {
                v.sort_by(|a, b| b.0.cmp(&a.0));
            }
            *cache = Some((q, p.items.len(), v.into_iter().take(300).map(|x| x.1).collect()));
        }
        cache.as_ref().map(|c| c.2.iter().map(|&i| &p.items[i]).collect()).unwrap_or_default()
    }

    pub fn run_palette(&mut self, item: PItem, ddl: bool) {
        self.popup = None;
        match item.action {
            PAction::Act(a) => self.run_action(a),
            PAction::Tab(id) => {
                if let Some(i) = self.tab_index(id) {
                    self.activate(i)
                }
            }
            PAction::Run { stmts, explain } => self.run_stmts(stmts, explain, false),
            PAction::ExportFormat(f) => self.export_dest(f),
            PAction::Dba(r) => self.dba_open(r),
            PAction::InsertSql(q) => self.insert_sql(q),
            PAction::Pick(v) => self.flow_pick(v),
            PAction::DbaSql(t) => self.dba_sql(t),
            PAction::ExportTo(f, clipboard) => self.export_to(f, clipboard),
            PAction::Exec(e) => {
                self.exec = e;
                self.palette_exec();
                // 고른 줄에 그대로 둔다
                if let Some(Popup::Palette(p)) = &mut self.popup {
                    p.sel = p.items.iter().position(|i| i.hint == item.hint && i.label.chars().skip(2).eq(item.label.chars().skip(2))).unwrap_or(0);
                }
            }
            PAction::Object { conn, db, e } => {
                let kind = match e.kind.as_str() {
                    "function" | "procedure" => "routine".to_string(),
                    k => k.to_string(),
                };
                if ddl && kind != "schema" {
                    return self.open_ddl(&conn, db, e.schema, e.name, kind);
                }
                match kind.as_str() {
                    "table" | "view" | "matview" | "foreign" => self.open_table(&conn, db, e.schema, e.name, kind),
                    "key" => self.open_redis(&conn, db, Some(e.name)),
                    "schema" => {
                        self.explorer = true;
                        self.focus = Focus::Tree;
                        let key = format!("{conn}|{db}|{}", e.schema);
                        self.tree.select_key(&key);
                    }
                    _ => self.open_ddl(&conn, db, e.schema, e.name, kind),
                }
            }
        }
    }

    pub fn input(&mut self, title: impl Into<String>, init: &str, on_ok: impl FnOnce(&mut App, String) + 'static) {
        self.input_words(title, init, vec![], on_ok);
    }

    /// 자동완성이 되는 한 줄 입력 (치는 단어로 words 를 거른다, ⌃Space / Tab 으로 바로 열기)
    pub fn input_words(&mut self, title: impl Into<String>, init: &str, words: Vec<String>, on_ok: impl FnOnce(&mut App, String) + 'static) {
        let mut t = TextArea::new(vec![init.to_string()]);
        t.move_cursor(tui_textarea::CursorMove::End);
        t.set_cursor_line_style(ratatui::style::Style::default());
        self.popup = Some(Popup::Input { title: title.into(), input: t, on_ok: Box::new(on_ok), words, complete: None });
    }

    /// 테이블 탭의 WHERE / ORDER BY 자동완성 후보: 컬럼 + 자주 쓰는 키워드
    pub fn table_words(&self, id: u64, order: bool) -> Vec<String> {
        let Some(t) = self.tabs.iter().find(|t| t.id == id) else { return vec![] };
        let kind = self.kind_of(&t.conn);
        let mut w: Vec<String> = match &t.kind {
            TabKind::Table(tt) => tt.grid.as_ref().map(|g| g.result.columns.iter().map(|c| sql::quote_ident(kind, &c.name)).collect()).unwrap_or_default(),
            _ => vec![],
        };
        let kw: &[&str] = if order { &["ASC", "DESC", "NULLS FIRST", "NULLS LAST"] } else { &["AND", "OR", "NOT", "IS NULL", "IS NOT NULL", "IN", "LIKE", "ILIKE", "BETWEEN", "TRUE", "FALSE", "NOW()"] };
        w.extend(kw.iter().map(|s| s.to_string()));
        w
    }

    pub fn run_action(&mut self, a: Action) {
        let sel = self.tree.selected();
        let tab_conn = self.tabs.get(self.active).map(|t| (t.conn.clone(), t.db.clone()));
        match a {
            Action::NewConsole => {
                if let Some(n) = sel.as_ref().filter(|_| self.focus == Focus::Tree).or(None) {
                    self.open_console(&n.conn.clone(), n.db.clone())
                } else if let Some((c, d)) = tab_conn {
                    self.open_console(&c, Some(d))
                } else if let Some(n) = sel {
                    self.open_console(&n.conn.clone(), n.db.clone())
                }
            }
            Action::NewDataSource => self.conn_form(None),
            Action::EditDataSource => {
                if let Some(c) = sel.and_then(|n| self.conn(&n.conn).map(|c| c.config.clone())) {
                    self.conn_form(Some(c))
                }
            }
            Action::DeleteDataSource => {
                if let Some(c) = sel.and_then(|n| self.conn(&n.conn).map(|c| c.config.clone())) {
                    self.popup = Some(Popup::Confirm {
                        text: format!("Remove data source \"{}\"?", c.name),
                        on_yes: Box::new(move |a| {
                            let st = a.state.clone();
                            let id = c.id.clone();
                            a.spawn(async move { st.delete_connection(id).await.map_err(err) }, |a, r| {
                                if let Err(e) = r {
                                    a.status = e;
                                }
                                a.reload_conns();
                            });
                        }),
                    });
                }
            }
            Action::Connect => {
                if let Some(n) = sel {
                    self.connect(&n.conn)
                }
            }
            Action::Disconnect => {
                if let Some(n) = sel {
                    self.disconnect(&n.conn)
                }
            }
            Action::Refresh => match (self.focus, sel) {
                (Focus::Tree, Some(n)) => self.refresh_node(&n),
                _ => {
                    if let Some(t) = self.tabs.get(self.active) {
                        let id = t.id;
                        match t.kind {
                            TabKind::Table(_) => self.reload_table_checked(id),
                            TabKind::Redis(_) => self.redis_scan(id, true),
                            _ => {}
                        }
                    }
                }
            },
            Action::Execute => self.execute(false),
            Action::Explain => self.execute(true),
            Action::ExplainAnalyze => {
                self.analyze_next = true;
                self.execute(true);
                self.analyze_next = false;
            }
            Action::Cancel => self.cancel(),
            Action::TxToggle | Action::Commit | Action::Rollback => self.tx(a),
            Action::CloseTab => self.close_tab(self.active),
            Action::NextTab => self.cycle_tab(1),
            Action::PrevTab => self.cycle_tab(-1),
            Action::GoToObject => self.palette_objects("Go to Database Object", vec![]),
            Action::SearchEverywhere => self.palette_everywhere(),
            Action::RecentTabs => self.palette_tabs(),
            Action::ExecSettings => self.palette_exec(),
            Action::ClearConsole => {
                if let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) {
                    c.editor.select_all();
                    c.editor.cut();
                    c.complete = None;
                    self.focus = Focus::Editor;
                }
            }
            Action::Ddl => {
                // 테이블 데이터 탭에서는 그 테이블의 DDL
                if self.focus != Focus::Tree {
                    if let Some(Tab { kind: TabKind::Table(tt), conn, db, .. }) = self.tabs.get(self.active) {
                        return self.open_ddl(&conn.clone(), db.clone(), tt.schema.clone(), tt.table.clone(), tt.kind.clone());
                    }
                    self.status = "Go to DDL: select an object in the explorer or open a table".into();
                    return;
                }
                if let Some(n) = sel.filter(|n| matches!(n.kind, NodeKind::Object(_))) {
                    let k = match &n.kind {
                        NodeKind::Object(k) if k == "function" || k == "procedure" => "routine".to_string(),
                        NodeKind::Object(k) => k.clone(),
                        _ => unreachable!(),
                    };
                    self.open_ddl(&n.conn, n.db.unwrap(), n.schema.unwrap(), n.name.unwrap(), k)
                }
            }
            Action::EditData => {
                if let Some(n) = sel {
                    self.activate_node(&n, true)
                }
            }
            Action::Submit => self.submit(),
            Action::Revert => {
                if let Some(g) = self.grid_mut() {
                    g.revert();
                }
            }
            Action::AddRow => {
                if let Some(Tab { kind: TabKind::Table(tt), .. }) = self.tabs.get_mut(self.active) {
                    if let Some(g) = &mut tt.grid {
                        g.add_row();
                    }
                }
            }
            Action::DeleteRow => {
                if let Some(Tab { kind: TabKind::Table(tt), .. }) = self.tabs.get_mut(self.active) {
                    if let Some(g) = &mut tt.grid {
                        let r = g.row;
                        if g.is_added(r) {
                            g.result.rows.remove(r);
                            g.added -= 1;
                            g.edits.retain(|(rr, _), _| *rr != r);
                            g.row = g.row.min(g.rows().saturating_sub(1));
                        } else if !g.deleted.remove(&r) {
                            g.deleted.insert(r);
                        }
                    }
                }
            }
            Action::SetNull => {
                if let Some(Tab { kind: TabKind::Table(tt), .. }) = self.tabs.get_mut(self.active) {
                    if let Some(g) = &mut tt.grid {
                        g.edits.insert((g.row, g.col), None);
                    }
                }
            }
            Action::ExportCsv => self.export_start(),
            Action::ImportData => self.import_start(),
            Action::Find => self.find_open(),
            Action::DbaReports => self.dba_palette(),
            Action::BackupDb => self.backup_start(),
            Action::RestoreDb => self.restore_start(),
            Action::CompareSchemas => self.compare_schemas_start(),
            Action::CompareData => self.compare_data_start(),
            Action::RunMigrations => self.migrations_start(),
            Action::ShowDiagram => self.erd_open(),
            Action::QueryHistory => self.history_palette(),
            Action::Snippets => self.snippets_palette(),
            Action::SaveSnippet => self.save_snippet(),
            Action::CopyInList => {
                let Some(g) = self.grid_mut() else {
                    self.status = "Open a result or table to copy a column".into();
                    return;
                };
                let (text, n) = g.in_list();
                crate::input::copy(self, &text);
                let short: String = text.chars().take(80).collect();
                self.status = format!("Copied {n} values as an IN list: {short}{}", if text.chars().count() > 80 { "…" } else { "" });
            }
            Action::CancelTransfer => {
                for (id, ..) in &self.jobs {
                    self.state.cancel_job(id);
                }
            }
            Action::ToggleExplorer => {
                if self.focus == Focus::Tree && self.explorer && !self.tabs.is_empty() {
                    self.explorer = false;
                    self.activate(self.active);
                } else {
                    self.explorer = true;
                    self.focus = Focus::Tree;
                }
            }
            Action::ToggleOutput => {
                if let Some(Tab { kind: TabKind::Console(c), .. }) = self.tabs.get_mut(self.active) {
                    c.show_output = !c.show_output;
                }
            }
            Action::SplitRight => self.split_focused(SplitDir::Right),
            Action::SplitDown => self.split_focused(SplitDir::Down),
            Action::Unsplit => self.unsplit(),
            Action::OtherSplit => {
                // 다음 칸으로 (왼쪽/위부터 차례로 돈다)
                let ls = self.panes.leaves();
                if ls.len() > 1 {
                    let p = ls.iter().position(|&g| g == self.focused_group()).unwrap_or(0);
                    if let Some(i) = self.group_tab(ls[(p + 1) % ls.len()]) {
                        self.activate(i);
                    }
                }
            }
            Action::ExplorerNarrower | Action::ExplorerWider => {
                let w = self.explorer_width(self.hits.main.width) as i16 + if a == Action::ExplorerWider { 4 } else { -4 };
                self.set_explorer_width(w);
            }
            Action::SplitShrink | Action::SplitGrow => {
                // 포커스 있는 칸 옆의 좌우 경계선을 옮긴다 (가장 가까운 것이 위아래 경계선이어도). 좌우가 없으면 위아래
                let g = self.focused_group();
                let path = self.panes.ancestor_with_dir(g, SplitDir::Right).or_else(|| self.panes.ancestor_with_dir(g, SplitDir::Down));
                if let Some(pct) = path.and_then(|p| self.panes.pct_mut(&p)) {
                    *pct = (*pct as i16 + if a == Action::SplitGrow { 5 } else { -5 }).clamp(15, 85) as u16;
                }
            }
            Action::Complete => {
                if matches!(self.tabs.get(self.active).map(|t| &t.kind), Some(TabKind::Console(_))) {
                    self.focus = Focus::Editor;
                    self.update_completion(true);
                }
            }
            Action::Palette => self.palette_actions(),
            Action::CycleFocus => {
                self.focus = match self.focus {
                    Focus::Tree if !self.tabs.is_empty() => match self.tabs[self.active].kind {
                        TabKind::Console(_) => Focus::Editor,
                        _ => Focus::Grid,
                    },
                    Focus::Editor => Focus::Grid,
                    _ => Focus::Tree,
                };
                if self.focus == Focus::Tree {
                    self.explorer = true;
                }
            }
            Action::ShrinkEditor => self.resize_console(-5),
            Action::GrowEditor => self.resize_console(5),
            Action::PaneUp => self.focus_pane(SplitDir::Down, true),
            Action::PaneDown => self.focus_pane(SplitDir::Down, false),
            Action::PaneLeft => self.focus_pane(SplitDir::Right, true),
            Action::PaneRight => self.focus_pane(SplitDir::Right, false),
            Action::Help => self.popup = Some(Popup::Help { sel: 0, scroll: 0, record: None, message: None }),
            Action::ManageSchemas => match sel {
                Some(n) if self.kind_of(&n.conn) != DbKind::Redis => self.manage_schemas(&n.conn, n.db.clone()),
                _ => self.status = "Select a PostgreSQL / MySQL data source in the explorer".into(),
            },
            Action::CheckUpdate => self.check_update(),
            Action::InstallUpdate => {
                let n = self.pending_tabs(None).len();
                if n > 0 {
                    self.popup = Some(Popup::Confirm { text: format!("{n} console(s) have uncommitted changes. Roll them back, update and restart?"), on_yes: Box::new(|a| a.install_update()) });
                } else {
                    self.install_update();
                }
            }
            Action::Quit => {
                let n = self.pending_tabs(None).len();
                if n > 0 {
                    self.popup = Some(Popup::Confirm { text: format!("{n} console(s) have uncommitted changes. Roll them back and quit?"), on_yes: Box::new(|a| a.quit = true) });
                } else {
                    self.quit = true;
                }
            }
            Action::TrustSshHost => {
                let ssh = sel.as_ref().and_then(|n| self.conn(&n.conn)).and_then(|c| c.config.active_ssh().cloned());
                match ssh {
                    Some(s) => self.status = self.state.trust_ssh_host(s.host, s.port).unwrap_or_else(|e| e.to_string()),
                    None => self.status = "Select a data source that uses an SSH tunnel".into(),
                }
            }
        }
    }

    /// 다음/이전 탭 (포커스가 있는 칸 안에서)
    fn cycle_tab(&mut self, d: isize) {
        let g = self.focused_group();
        let idx: Vec<usize> = (0..self.tabs.len()).filter(|&i| self.tabs[i].group == g).collect();
        let Some(p) = idx.iter().position(|&i| i == self.active) else { return };
        let n = idx.len() as isize;
        self.activate(idx[(p as isize + d).rem_euclid(n) as usize]);
    }

    /// 포커스 있는 칸을 나눈다 (최대 4칸): 지금 탭을 새 칸(오른쪽/아래)으로 옮긴다
    fn split_focused(&mut self, dir: SplitDir) {
        let Some(cur) = self.tabs.get(self.active).map(|t| t.id) else {
            self.status = "Open a tab to split".into();
            return;
        };
        if self.panes.leaves().len() >= crate::panes::MAX {
            self.status = format!("Up to {} panes — close or move a tab to merge one", crate::panes::MAX);
            return;
        }
        let g = self.focused_group();
        if self.tabs.iter().filter(|t| t.group == g).count() >= 2 {
            return self.drop_tab(cur, DropTarget::NewSplit(g, dir));
        }
        // 칸에 탭이 하나뿐이면 같은 DB 의 새 콘솔을 오른쪽/아래 칸에 연다
        let (conn, db) = (self.tabs[self.active].conn.clone(), self.tabs[self.active].db.clone());
        if self.kind_of(&conn) == DbKind::Redis {
            self.status = "Open another tab to split".into();
            return;
        }
        self.open_console(&conn, Some(db));
        let id = self.tabs[self.active].id;
        self.drop_tab(id, DropTarget::NewSplit(g, dir));
    }

    /// Database Explorer 폭: 정한 값이 없으면 화면 폭의 1/4 (26~48)
    pub fn explorer_width(&self, total: u16) -> u16 {
        let auto = (total / 4).clamp(26, 48);
        self.explorer_w.unwrap_or(auto).clamp(12, total.saturating_sub(20).max(12))
    }

    pub fn set_explorer_width(&mut self, w: i16) {
        let max = self.hits.main.width.saturating_sub(20).max(12) as i16;
        self.explorer_w = Some(w.clamp(12, max) as u16);
    }

    /// 콘솔 에디터/결과 비율 조절
    /// 옆 칸으로: axis = Down 이면 위(back)/아래, Right 면 왼쪽(back)/오른쪽.
    /// 마지막으로 그린 칸 위치 기준으로, 그 방향에 있고 겹치는 칸 중 가장 가까운 것
    fn focus_pane(&mut self, axis: SplitDir, back: bool) {
        let cur = self.focused_group();
        // (칸, 왼, 오른, 위, 아래)
        let rect = |&(g, bar, body): &(usize, ratatui::layout::Rect, ratatui::layout::Rect)| (g, bar.x, bar.x + bar.width, bar.y, body.y + body.height);
        let Some((_, x0, x1, top, bottom)) = self.hits.groups.iter().map(rect).find(|r| r.0 == cur) else { return };
        let next = self
            .hits
            .groups
            .iter()
            .map(rect)
            .filter_map(|(g, a0, a1, t, b)| {
                // 경계선(테두리) 한 칸 정도는 겹쳐 그려질 수 있어 1 칸 여유
                let gap = match (axis, back) {
                    (SplitDir::Down, true) if a0 < x1 && x0 < a1 && b <= top + 1 => top.saturating_sub(b),
                    (SplitDir::Down, false) if a0 < x1 && x0 < a1 && t + 1 >= bottom => t.saturating_sub(bottom),
                    (SplitDir::Right, true) if t < bottom && top < b && a1 <= x0 + 1 => x0.saturating_sub(a1),
                    (SplitDir::Right, false) if t < bottom && top < b && a0 + 1 >= x1 => a0.saturating_sub(x1),
                    _ => return None,
                };
                (g != cur).then_some((gap, if axis == SplitDir::Down { a0.abs_diff(x0) } else { t.abs_diff(top) }, g))
            })
            .min()
            .map(|r| r.2);
        let side = match (axis, back) {
            (SplitDir::Down, true) => "above",
            (SplitDir::Down, false) => "below",
            (SplitDir::Right, true) => "on the left",
            (SplitDir::Right, false) => "on the right",
        };
        match next.and_then(|g| self.group_tab(g)) {
            Some(i) => self.activate(i),
            None => self.status = format!("No split {side}"),
        }
    }

    pub fn resize_console(&mut self, d: i16) {
        self.console_pct = (self.console_pct as i16 + d).clamp(10, 90) as u16;
    }

    // ───────── 자동완성 ─────────

    /// Data Source 의 자동완성 메타를 버린다 (끊기·새로 고침)
    pub fn forget_completion(&mut self, conn: &str) {
        self.completion.retain(|t, _| qurry_core::app::split_target(t).0 != conn);
    }

    pub fn load_completion(&mut self, target: String) {
        if self.completion.contains_key(&target) || !self.completion_loading.insert(target.clone()) {
            return;
        }
        let st = self.state.clone();
        let t2 = target.clone();
        self.spawn(async move { st.completion_meta(t2).await.map_err(err) }, move |a, r| {
            a.completion_loading.remove(&target);
            match r {
                Ok(v) => {
                    a.completion.insert(target, Arc::new(complete::Meta::new(v)));
                }
                Err(e) => a.status = format!("Completion metadata: {e}"),
            }
        });
    }

    /// 커서 위치로 후보를 다시 계산한다. explicit = Ctrl+Space (아니면 '.' 입력·목록이 열린 채 타이핑)
    /// Manage Shown Schemas: DB 목록을 읽어 창을 연다 (db = DB 줄에서 열었으면 그 DB 를 펼친다)
    fn manage_schemas(&mut self, conn: &str, db: Option<String>) {
        self.status = "Loading databases…".into();
        let st = self.state.clone();
        let id = conn.to_string();
        self.spawn(async move { st.list_databases(id).await.map_err(err) }, {
            let conn = conn.to_string();
            move |a, r| match r {
                Ok(dbs) => {
                    a.status.clear();
                    a.open_scope(&conn, dbs, db.as_deref());
                }
                Err(e) => a.status = e,
            }
        });
    }

    pub fn open_scope(&mut self, conn: &str, dbs: Vec<String>, focus_db: Option<&str>) {
        let Some(c) = self.conn(conn) else { return };
        let e = crate::scope::ScopeEditor::new(conn.to_string(), dbs, self.default_db(conn), &c.config.scope, focus_db);
        self.popup = Some(Popup::Scope(e));
        if let Some(db) = focus_db {
            self.load_scope_schemas(db.to_string());
        }
    }

    /// 창에서 펼친 DB 의 스키마를 읽는다
    pub fn load_scope_schemas(&mut self, db: String) {
        let Some(Popup::Scope(e)) = &self.popup else { return };
        let conn = e.conn.clone();
        let st = self.state.clone();
        let target = sql::target_id(&conn, Some(&db));
        self.spawn(async move { st.list_schemas(target).await.map_err(err) }, move |a, r| {
            if let Some(Popup::Scope(e)) = &mut a.popup {
                if e.conn == conn {
                    e.set_schemas(&db, r);
                }
            }
        });
    }

    /// 고른 범위를 Data Source 에 저장하고 트리를 다시 그린다.
    /// 접속 설정은 그대로라 store 에 바로 쓴다 (save_connection 은 풀을 닫아 열린 트랜잭션이 끊긴다)
    pub fn apply_scope(&mut self, e: crate::scope::ScopeEditor) {
        // 저장된 설정을 다시 읽어 범위만 바꾼다 (그 사이 GUI·MCP 에서 바꾼 읽기 전용·호스트 등을 되돌리지 않게)
        let cfg = match self.state.store.get(&e.conn) {
            Ok(Some(mut cfg)) => {
                cfg.scope = e.scope();
                cfg
            }
            Ok(None) => return,
            Err(er) => {
                self.status = er.to_string();
                return;
            }
        };
        if let Err(er) = self.state.store.save(&cfg) {
            self.status = er.to_string();
            return;
        }
        if let Some(c) = self.conns.iter_mut().find(|c| c.config.id == cfg.id) {
            c.config = cfg.clone();
        }
        self.status = format!("Shown databases and schemas updated: {}", cfg.name);
        if let Some(root) = self.tree.roots.iter().find(|n| n.conn == cfg.id && n.kind == NodeKind::Conn).cloned() {
            self.refresh_node(&root);
        }
    }

    /// 지금 바로 새 버전을 확인한다 (⌃K › Check for Updates)
    fn check_update(&mut self) {
        self.status = "Checking for updates…".into();
        let dir = self.state.store.dir();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let r = qurry_core::update::check(&dir, env!("CARGO_PKG_VERSION"), true).await;
            let _ = tx.send(Box::new(move |a: &mut App| match r {
                Ok(Some(r)) => a.show_update(r),
                Ok(None) => a.status = format!("You have the latest version ({}).", env!("CARGO_PKG_VERSION")),
                Err(e) => a.status = e.to_string(),
            }));
        });
    }

    /// 새 버전 창: 지금 버전 이후 바뀐 것 (버전마다) + 업데이트 방법
    pub fn show_update(&mut self, r: qurry_core::update::Release) {
        let cur = env!("CARGO_PKG_VERSION");
        let how = format!("{} › Update Qurry & Restart", self.keys.label(Action::Palette));
        let changes = qurry_core::update::changes_text(&r.changes_since(cur));
        // 업데이트 방법은 맨 위에 (버전이 많으면 창 아래가 잘린다)
        let text = format!("You have {cur} — {how}\n\n{}", if changes.is_empty() { "See the release page for what changed.\n".into() } else { changes });
        self.status = format!("Qurry {} is available — {how}", r.version);
        self.popup = Some(Popup::Message { title: format!("Qurry {} is available", r.version), text });
        self.update = Some(r.version);
    }

    /// 최신 릴리스를 받아 바꿔 넣고, 끝나면 다시 켠다 (열어 둔 탭은 끝낼 때 저장된다)
    fn install_update(&mut self) {
        self.status = "Updating Qurry… (downloading)".into();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let r = crate::selfupdate::install(true).await;
            let _ = tx.send(Box::new(move |a: &mut App| match r {
                Ok(crate::selfupdate::Outcome::Installed { version, .. }) => {
                    a.status = format!("Qurry {version} installed — restarting…");
                    a.restart = true;
                    a.quit = true;
                }
                Ok(crate::selfupdate::Outcome::UpToDate) => {
                    a.update = None;
                    a.status = format!("You have the latest version ({}).", env!("CARGO_PKG_VERSION"));
                }
                Ok(crate::selfupdate::Outcome::Skipped(why)) => a.status = why,
                Err(e) => a.status = format!("Update failed: {e}"),
            }));
        });
    }

    pub fn update_completion(&mut self, explicit: bool) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let (target, db) = (t.target.clone(), t.db.clone());
        let meta = self.completion.get(&target).cloned();
        if meta.is_none() {
            self.load_completion(target);
        }
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let (row, col) = c.editor.cursor();
        let lines = c.editor.lines();
        let before: String = lines.get(row).map(|l| l.chars().take(col).collect()).unwrap_or_default();
        // 키마다 불리므로 커서 주변 줄만 본다 (긴 스크립트 전체를 매번 나누지 않게)
        let (from, to) = (row.saturating_sub(200), (row + 50).min(lines.len()));
        let window = &lines[from..to];
        let text = window.join("\n");
        let stmt = sql::statement_at(&text, byte_offset(window, row - from, col), kind).unwrap_or_default();
        let empty = complete::Meta::new(vec![]);
        let (prefix, items) = complete::suggest(meta.as_deref().unwrap_or(&empty), &stmt, &before, &db);
        // 자동으로 열 때는 '.' 뒤이거나 단어를 치는 중일 때만
        let keep = explicit || prefix > 0 || before.ends_with('.');
        c.complete = if items.is_empty() || !keep { None } else { Some(Completion { items, sel: 0, prefix }) };
        if explicit && c.complete.is_none() {
            self.status = if meta.is_none() { "Loading completion metadata… try again in a moment".into() } else { "No suggestions".into() };
        }
    }

    /// 고른 후보로 커서 앞 단어를 바꾼다
    pub fn accept_completion(&mut self) {
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let Some(cmp) = c.complete.take() else { return };
        let Some(item) = cmp.items.get(cmp.sel) else { return };
        for _ in 0..cmp.prefix {
            c.editor.delete_char();
        }
        c.editor.insert_str(&item.insert);
    }

    /// 키맵을 keymap.json 에 저장
    pub fn save_keys(&mut self) -> Result<(), String> {
        self.state.store.save_keymap("tui", &self.keys.overrides()).map_err(err)
    }

    pub fn grid_mut(&mut self) -> Option<&mut Grid> {
        match &mut self.tabs.get_mut(self.active)?.kind {
            TabKind::Console(c) => c.grid.as_mut(),
            TabKind::Table(t) => t.grid.as_mut(),
            TabKind::Redis(r) => r.value.as_mut(),
            TabKind::Dba(d) => d.grid.as_mut(),
            TabKind::Text(_) => None,
        }
    }
}

pub fn byte_offset(lines: &[String], row: usize, col: usize) -> usize {
    let mut off = 0;
    for l in lines.iter().take(row) {
        off += l.len() + 1;
    }
    let line = lines.get(row).map(|s| s.as_str()).unwrap_or("");
    off + line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len())
}

pub fn selected_text(lines: &[String], r0: usize, c0: usize, r1: usize, c1: usize) -> String {
    let full = lines.join("\n");
    let a = byte_offset(lines, r0, c0);
    let b = byte_offset(lines, r1, c1);
    full[a.min(b)..a.max(b)].to_string()
}

/// Redis 값 → 그리드 (string 은 None: 텍스트로 보여 준다)
pub fn redis_grid(d: &KeyDetail) -> Option<Grid> {
    let arr = d.value.as_array()?;
    let (names, rows): (Vec<&str>, Vec<Vec<Value>>) = match d.ty.as_str() {
        "hash" => (vec!["field", "value"], arr.iter().map(|x| vec![x[0].clone(), x[1].clone()]).collect()),
        "zset" => (vec!["member", "score"], arr.iter().map(|x| vec![x[0].clone(), x[1].clone()]).collect()),
        "list" => (vec!["index", "value"], arr.iter().enumerate().map(|(i, x)| vec![Value::from(i), x.clone()]).collect()),
        "set" => (vec!["member"], arr.iter().map(|x| vec![x.clone()]).collect()),
        "stream" => (vec!["id", "fields"], arr.iter().map(|x| vec![x[0].clone(), Value::from(x[1].to_string())]).collect()),
        _ => return None,
    };
    let result = QueryResult {
        columns: names.iter().map(|n| ColumnInfo { name: n.to_string(), type_name: "TEXT".into() }).collect(),
        rows,
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 0,
        has_result_set: true,
        in_transaction: false,
    };
    Some(Grid::new(result))
}
