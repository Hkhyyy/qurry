//! TUI state and actions. All real DB work goes through qurry_core::app::AppState (same engine as the GUI).
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
    /// Open autocomplete list
    pub complete: Option<Completion>,
}

/// Max lines kept in the console Output log (same as the GUI)
const OUTPUT_MAX: usize = 1000;

impl Console {
    /// One line in the Output log (old lines are dropped)
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
    /// Length of the prefix to replace (the word before the cursor)
    pub prefix: usize,
}

/// Editor split (the GUI's Split Right / Split Down). Each pane (group) has its own tab bar
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SplitDir {
    Right,
    Down,
}

/// What is being dragged with the mouse
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drag {
    /// Console editor/result divider
    Console,
    /// Divider between panes (index into Hits.splits)
    Split(usize),
    /// Grid horizontal scrollbar
    HBar,
    /// Grid range selection (from the pressed cell)
    Grid,
    /// Database Explorer right divider
    Explorer,
    /// Tab (moved = whether it was actually dragged; false for a plain click)
    Tab { id: u64, moved: bool },
    /// Text selection in the console (anchor = pressed position, selecting = whether dragging started)
    Editor { anchor: (usize, usize), selecting: bool },
}

/// Where to drop a tab
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DropTarget {
    /// Move into that pane (before = insert before that tab)
    Group(usize, Option<u64>),
    /// Split that pane and move into the new one
    NewSplit(usize, SplitDir),
}

pub struct TableTab {
    pub schema: String,
    pub table: String,
    /// table / view / matview / foreign (for Ctrl+B DDL)
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

/// DBA reports (sessions·locks, slow queries, size·indexes, maintenance, server, privileges). Sessions·locks reload every 3 seconds
pub struct DbaTab {
    pub report: String,
    pub info: Option<qurry_core::dba::ReportInfo>,
    pub grid: Option<Grid>,
    pub error: Option<String>,
    pub running: bool,
    pub auto: bool,
    /// The selected session disappeared after a reload (its pid). c / K then terminate nothing
    pub gone: Option<String>,
}

/// Medis-style Redis browser: key list on the left, value on the right
pub struct RedisTab {
    pub pattern: String,
    pub keys: Vec<KeyRow>,
    pub cursor: String,
    /// Selection among the visible rows (folders·keys, rows())
    pub sel: usize,
    pub off: usize,
    /// Expanded folders ("dev:sessions")
    pub open: HashSet<String>,
    pub detail: Option<(String, KeyDetail)>,
    /// String value prettified (JSON indented). Built on load so it isn't reparsed on every draw
    pub text: Option<String>,
    pub value: Option<Grid>,
    pub focus_value: bool,
    pub scanning: bool,
    /// Key given on open (selected once the scan finishes)
    pub want: Option<String>,
}

/// A row in the key browser: a folder grouped by ":", or a key (index into keys)
#[derive(Debug, Clone, PartialEq)]
pub enum KRow {
    Folder { path: String, name: String, depth: usize, count: usize },
    Key { idx: usize, depth: usize },
}

impl RedisTab {
    /// Visible rows: folders first (by name), then keys. Only expanded folders show contents (same rules as the explorer's tree::key_folders)
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

    /// The key, if the selected row is a key
    pub fn sel_key(&self) -> Option<String> {
        match self.rows().get(self.sel) {
            Some(KRow::Key { idx, .. }) => Some(self.keys[*idx].key.clone()),
            _ => None,
        }
    }

    /// Expand the parent folders so the key is visible, and select it
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
    /// Pane it belongs to in the split view (a leaf of panes)
    pub group: usize,
    /// Restored tab: not loaded yet (loads when first shown)
    pub stale: bool,
}

// ───────── Popups ─────────

#[derive(Clone)]
pub enum PAction {
    Act(Action),
    /// Open a DBA report (id)
    Dba(String),
    /// Query picked from history·snippets into the console
    InsertSql(String),
    /// Picked from the production audit log: undo SQL into a new console for that Data Source (or the reason if none)
    OpenUndo { conn: String, db: Option<String>, undo: Option<String>, note: String },
    /// Multi-step pick (compare·migration): the picked value
    Pick(String),
    /// DBA report row action (template SQL → console)
    DbaSql(String),
    /// Export: format picked → pick target
    ExportFormat(qurry_core::transfer::Format),
    /// Export: (format, clipboard)
    ExportTo(qurry_core::transfer::Format, bool),
    Object { conn: String, db: String, e: IndexEntryLite },
    Tab(u64),
    /// Statements picked in the execution picker popup
    Run { stmts: Vec<String>, explain: bool },
    /// Change execution settings
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
    /// Filtered result cache: same (query, item count) skips re-filtering and sorting (called on every draw·key)
    pub cache: std::cell::RefCell<Option<(String, usize, Vec<usize>)>>,
}

#[derive(Clone, PartialEq)]
pub enum FieldKind {
    Text,
    Secret,
    Toggle,
    Choice(Vec<String>),
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
    /// F1 keymap. The Commands section at the top is selectable (sel). record = waiting for a new key (true = add to existing keys)
    /// sel counts the top Commands and the fixed key list below as one sequence (only Commands can be changed)
    Help { sel: usize, scroll: u16, record: Option<bool>, message: Option<String> },
    Message { title: String, text: String },
    /// Single-line input. words = autocomplete candidates (columns·keywords for table WHERE/ORDER BY), complete = open list (candidates, selection, prefix length to replace)
    Input { title: String, input: TextArea<'static>, on_ok: Box<dyn FnOnce(&mut App, String)>, words: Vec<String>, complete: Option<(Vec<String>, usize, usize)> },
    Confirm { text: String, on_yes: Box<dyn FnOnce(&mut App)> },
    Palette(Palette),
    ConnForm(ConnForm),
    /// Manage Shown Schemas (DBs·schemas shown in the tree)
    Scope(crate::scope::ScopeEditor),
    /// Editor find/replace bar (⌃F)
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
    FindSimilar,
    RagSearch,
    RagBuild,
    RagNew,
    RagKey,
    WsSwitch,
    WsNew,
    WsRename,
    WsDelete,
    WsMoveTab,
    WsAddConn,
    WsMoveConn,
    WsRemoveConn,
    Find,
    DbaReports,
    BackupDb,
    RestoreDb,
    CompareSchemas,
    CompareData,
    RunMigrations,
    ShowDiagram,
    QueryHistory,
    ProductionLog,
    Snippets,
    SaveSnippet,
    ImportData,
    GenerateTestData,
    CopyRelatedRows,
    ChartResult,
    NewTable,
    ModifyTable,
    NewView,
    NewSchema,
    NewDatabase,
    NewSequence,
    RenameObject,
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
    PasswordKey,
    CheckUpdate,
    InstallUpdate,
    Help,
    Quit,
}

/// Action palette list: (action, name, hint). Hints for rebindable commands (keys::DEFAULT_KEYS) come from the current keymap
pub const ACTIONS: &[(Action, &str, &str)] = &[
    (Action::NewConsole, "New Query Console", ""),
    (Action::Execute, "Execute (asks what to run — see Query Execution Settings)", ""),
    (Action::Explain, "Explain Plan", ""),
    (Action::ExplainAnalyze, "Explain Analyze (runs the statement: real times and row counts)", ""),
    (Action::Cancel, "Cancel Running Statement", ""),
    (Action::TxToggle, "Toggle Transaction Mode (Auto/Manual)", ""),
    (Action::Commit, "Commit", ""),
    (Action::Rollback, "Roll Back", ""),
    (Action::ExecSettings, "Query Execution Settings (what ⌃Enter runs)", ""),
    (Action::ClearConsole, "Clear Console (delete all text)", "⌘A ⌫ (console)"),
    (Action::SearchEverywhere, "Search Everywhere (objects · columns · tabs · actions)", ""),
    (Action::GoToObject, "Go to Database Object", ""),
    (Action::RecentTabs, "Recent Tabs / Switcher", ""),
    (Action::EditData, "Edit Data (open table)", "F4 / Enter"),
    (Action::Ddl, "Go to DDL", ""),
    (Action::Submit, "Submit Changes", ""),
    (Action::Revert, "Revert Changes", "⌃z (grid)"),
    (Action::AddRow, "Add Row", "Insert / a"),
    (Action::DeleteRow, "Delete Row", "Delete / d"),
    (Action::SetNull, "Set NULL", "n (grid)"),
    (Action::ExportCsv, "Export Data… (CSV, TSV, JSON, SQL INSERT, Markdown)", "x (grid)"),
    (Action::CopyInList, "Copy Column as IN List  ('a', 'b', …)", "Y (grid)"),
    (Action::FindSimilar, "Find Similar Rows (pgvector, by the vector column)", ""),
    (Action::RagSearch, "AI Search (RAG)… — ask a table in plain words", ""),
    (Action::RagBuild, "Build RAG Index… (embed the rows)", ""),
    (Action::RagNew, "New RAG Index from the Selected Table…", ""),
    (Action::RagKey, "Set OpenAI API Key (RAG)…", ""),
    (Action::WsSwitch, "Switch Workspace…", "⌥1…9"),
    (Action::WsNew, "New Workspace…", ""),
    (Action::WsRename, "Rename Workspace…", ""),
    (Action::WsDelete, "Delete Workspace…", ""),
    (Action::WsMoveTab, "Move Tab to Workspace…", ""),
    (Action::WsAddConn, "Add Data Source to This Workspace…", ""),
    (Action::WsMoveConn, "Move Data Source to Another Workspace…", "w (explorer)"),
    (Action::WsRemoveConn, "Remove Data Source from This Workspace", ""),
    (Action::Find, "Find / Replace in Editor (regex)", ""),
    (Action::QueryHistory, "Query History… (shared with the GUI)", ""),
    (Action::ProductionLog, "Production Log… (changes run on Production data sources, undo SQL)", ""),
    (Action::Snippets, "Snippets…", ""),
    (Action::SaveSnippet, "Save Query as Snippet…", ""),
    (Action::ShowDiagram, "Show Diagram (ERD): foreign keys of a schema, as text + Mermaid", ""),
    (Action::CompareSchemas, "Compare Schemas… (DDL to make one schema look like another)", ""),
    (Action::CompareData, "Compare Table Data… (sync SQL by primary key)", ""),
    (Action::RunMigrations, "Run Migrations… (folder of V1__name.sql files)", ""),
    (Action::BackupDb, "Back Up Database… (pg_dump / mysqldump)", ""),
    (Action::RestoreDb, "Restore Database… (from a .dump / .sql backup)", ""),
    (Action::DbaReports, "Database Admin… (sessions & locks, slow queries, sizes, indexes, vacuum, server, privileges)", ""),
    (Action::ImportData, "Import Data from File… (CSV, TSV, JSON, Excel)", ""),
    (Action::GenerateTestData, "Generate Test Data… (fill the selected table with realistic rows)", ""),
    (Action::NewTable, "New Table… (columns, keys, indexes, foreign keys — SQL shown before it runs)", ""),
    (Action::ModifyTable, "Modify Table… (change columns, keys, indexes, foreign keys → ALTER)", ""),
    (Action::NewView, "New View…", ""),
    (Action::NewSchema, "New Schema… (PostgreSQL)", ""),
    (Action::NewDatabase, "New Database…", ""),
    (Action::NewSequence, "New Sequence… (PostgreSQL)", ""),
    (Action::RenameObject, "Rename… (table, view, sequence, schema, database)", ""),
    (Action::ChartResult, "Chart Result (bars: sum of a number column by the first text column)", ""),
    (Action::CopyRelatedRows, "Copy Related Rows To… (rows + foreign-key related rows into another data source, personal data masked)", ""),
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
    (Action::PasswordKey, "Password Key… (keep it in the config folder or the system keychain)", ""),
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

/// Column → picking it opens its table (Ctrl+B opens that table's DDL)
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


/// Screen areas from the last draw (for mouse hit testing)
#[derive(Default)]
pub struct Hits {
    pub tree: ratatui::layout::Rect,
    /// (tab id, x start, x end, y)
    pub tabs: Vec<(u64, u16, u16, u16)>,
    /// Per pane: (pane, tab bar, body)
    pub groups: Vec<(usize, ratatui::layout::Rect, ratatui::layout::Rect)>,
    /// Per split: (branch position, split area, divider, direction). For vertical splits, the empty part of the lower first pane's tab bar also counts as the divider (grab)
    pub splits: Vec<(crate::panes::Path, ratatui::layout::Rect, ratatui::layout::Rect, ratatui::layout::Rect, SplitDir)>,
    /// Database Explorer right divider and the screen body (drag to resize width)
    pub explorer_divider: ratatui::layout::Rect,
    pub main: ratatui::layout::Rect,
    pub editor: ratatui::layout::Rect,
    /// Find bar position (above the editor, only while the bar is open)
    pub find: ratatui::layout::Rect,
    pub results: ratatui::layout::Rect,
    pub redis_keys: ratatui::layout::Rect,
    pub redis_value: ratatui::layout::Rect,
    pub palette: ratatui::layout::Rect,
    pub palette_off: usize,
    /// Console body (editor+results) and its divider (drag to adjust the ratio)
    pub console: ratatui::layout::Rect,
    pub divider: ratatui::layout::Rect,
    /// Double-click detection: (time, x, y)
    pub last_click: Option<(std::time::Instant, u16, u16)>,
    /// Screen cell where the console cursor was drawn and the cursor then (row, col) — for mapping mouse position to text position (tui-textarea doesn't expose scroll position)
    pub caret: Option<(u16, u16, usize, usize)>,
    /// [■ Cancel] in the running console's title bar (click to cancel)
    pub cancel: ratatui::layout::Rect,
    /// Text tab (DDL·diagram) body — scroll with the wheel
    pub text: ratatui::layout::Rect,
    /// Workspace strip: (number, area)
    pub workspaces: Vec<(usize, ratatui::layout::Rect)>,
}

/// Tabs·split of a workspace other than the current one (tab objects kept as is: console sessions·transactions·results survive)
pub struct WsSaved {
    /// This workspace's explorer (expansion state·selection)
    pub tree: Tree,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub mru: Vec<u64>,
    pub panes: crate::panes::Pane,
    pub group_active: [Option<u64>; crate::panes::MAX],
}

impl Default for WsSaved {
    fn default() -> Self {
        WsSaved { tree: Tree::default(), tabs: vec![], active: 0, mru: vec![], panes: crate::panes::Pane::Leaf(0), group_active: [None; crate::panes::MAX] }
    }
}

/// Workspace (crate::workspace): name + stashed tabs when not current
pub struct Workspace {
    /// id in the list shared with the GUI (workspaces.json)
    pub id: String,
    pub name: String,
    /// Data Source ids shown in this workspace's explorer
    pub conns: Vec<String>,
    pub saved: Option<WsSaved>,
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
    /// Running exports·imports (job id, name, progress). Progress bar in the status bar
    pub jobs: Vec<(String, String, Arc<qurry_core::transfer::Job>)>,
    /// The next Explain is ANALYZE (actually runs it for time·row counts)
    pub analyze_next: bool,
    /// Multi-step pick in progress (compare·migration)
    pub flow: Option<crate::migrate::Flow>,
    /// Table create · modify window (crate::designer). Input · confirm popups open over it
    pub designer: Option<crate::designer::Designer>,
    pub focus: Focus,
    pub explorer: bool,
    pub popup: Option<Popup>,
    pub status: String,
    pub quit: bool,
    /// Relaunch after quitting (after an in-app update)
    pub restart: bool,
    /// Screen split (up to 4 panes)
    pub panes: crate::panes::Pane,
    /// Tab shown in each pane
    pub group_active: [Option<u64>; crate::panes::MAX],
    /// Share of the console taken by the editor (%)
    pub console_pct: u16,
    /// Database Explorer width (None = auto, fit to screen width)
    pub explorer_w: Option<u16>,
    /// What is being dragged and the current position
    pub drag: Option<Drag>,
    pub drag_pos: (u16, u16),
    /// Autocomplete metadata (target → tables·columns)
    pub completion: HashMap<String, Arc<complete::Meta>>,
    completion_loading: HashSet<String>,
    /// Value · json key suggestions (per spot, kept briefly) · spot currently loading
    hints: HashMap<String, (std::time::Instant, Vec<qurry_core::hints::Hint>)>,
    hints_loading: HashSet<String>,
    /// When the running tab started (synced at draw time)
    run_since: HashMap<u64, std::time::Instant>,
    /// Command shortcuts ("tui" in keymap.json)
    pub keys: crate::keys::Keymap,
    /// The new version, if one is available (announced in the status bar)
    pub update: Option<String>,
    next_id: u64,
    /// What ⌃Enter runs (Query Execution Settings, saved in the workspace)
    pub exec: crate::exec::ExecSettings,
    /// Workspaces and the current one (the current one's tabs are the tabs · panes … above)
    pub workspaces: Vec<Workspace>,
    pub cur_ws: usize,
    /// Whether last session's tabs were restored (once, after the first Data Source list load), and the last saved content
    restored: bool,
    saved_ws: String,
    /// Last sync with the shared workspace list (None = not yet)
    pub ws_shared: Option<Vec<qurry_core::workspaces::WsInfo>>,
    /// The save file has synced with the shared list before (missing workspaces were deleted elsewhere)
    pub ws_synced: bool,
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
            designer: None,
            focus: Focus::Tree,
            explorer: true,
            popup: None,
            status: "Ready — not sure? ⌃p searches everything · F1 keymap".into(),
            quit: false,
            panes: crate::panes::Pane::Leaf(0),
            group_active: [None; crate::panes::MAX],
            console_pct: 42,
            explorer_w: None,
            drag: None,
            drag_pos: (0, 0),
            completion: HashMap::new(),
            completion_loading: HashSet::new(),
            hints: HashMap::new(),
            hints_loading: HashSet::new(),
            run_since: HashMap::new(),
            keys: crate::keys::Keymap::new(&user_keys),
            update: None,
            restart: false,
            restored: false,
            saved_ws: String::new(),
            ws_shared: None,
            ws_synced: false,
            next_id: 1,
            exec: Default::default(),
            workspaces: vec![Workspace { id: uuid::Uuid::new_v4().to_string(), name: "Default".into(), conns: vec![], saved: None }],
            cur_ws: 0,
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
                // Data Source just connected: reload table tabs left empty by the failed connection
                let back: Vec<String> = cs.iter().filter(|c| c.connected && a.conn(&c.config.id).is_some_and(|o| !o.connected)).map(|c| c.config.id.clone()).collect();
                a.conns = cs;
                let first = !a.restored;
                if first {
                    a.restored = true;
                    if let Some(v) = a.state.store.workspace("tui") {
                        crate::workspace::restore(a, &v);
                    }
                    a.ws_adopt_shared();
                }
                a.ws_adopt_orphans();
                a.ws_push();
                a.rebuild_roots();
                if first {
                    a.saved_ws = crate::workspace::to_json(a).to_string();
                }
                for c in back {
                    a.retry_failed_tables(&c);
                }
            }
            Err(e) => a.status = format!("Failed to load data sources: {e}"),
        });
    }

    /// Top of the explorer: only the current workspace's Data Sources (in the order added to the workspace)
    pub fn rebuild_roots(&mut self) {
        let mine = &self.workspaces[self.cur_ws].conns;
        self.tree.roots = mine
            .iter()
            .filter_map(|id| self.conns.iter().find(|c| &c.config.id == id))
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
        self.tree.clamp();
    }

    /// Data Sources in no workspace (just created, or created in the GUI) → the current workspace
    pub fn ws_adopt_orphans(&mut self) {
        let known: HashSet<String> = self.workspaces.iter().flat_map(|w| w.conns.iter().cloned()).collect();
        let orphans: Vec<String> = self.conns.iter().map(|c| c.config.id.clone()).filter(|id| !known.contains(id)).collect();
        let cur = self.cur_ws;
        self.workspaces[cur].conns.extend(orphans);
    }

    pub(crate) fn default_db(&self, conn: &str) -> String {
        self.conn(conn).map(|c| sql::default_db(&c.config)).unwrap_or_default()
    }

    // ───────── Tree ─────────

    pub fn toggle_node(&mut self, n: &Node) {
        if !n.expandable {
            return;
        }
        if self.tree.expanded.contains(&n.key) {
            self.tree.expanded.remove(&n.key);
        } else {
            self.tree.expanded.insert(n.key.clone());
            self.load_children(n.clone());
            // If the connection fell back to plaintext, say so in the status bar (same wording as the GUI)
            if matches!(n.kind, NodeKind::Conn) && matches!(self.kind_of(&n.conn), DbKind::Postgres | DbKind::Mysql) {
                let (st, id) = (self.state.clone(), n.conn.clone());
                self.spawn(async move { st.connect(id).await.map_err(|e| e.to_string()) }, |a, r| {
                    if let Ok(v) = r
                        && let Some((_, note)) = v.split_once(" — ⚠")
                    {
                        a.status = format!("⚠{note}");
                    }
                });
            }
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
                    // If the driver is missing, ask whether to download it (expand again afterwards)
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
                    a.status = if e.contains(qurry_core::tunnel::UNKNOWN_HOST) { format!("{e} — ⌃k › Trust SSH Host Key") } else { e };
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
                        // Connected: fill the search cache in the background so lookups work next time without connecting
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
                        // DB to auto-expand: the only visible DB if there's one, otherwise the default DB (only if visible)
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

    /// Schema objects → folders (tables/views/…/keys) + nodes under them. The first folder is expanded
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
            // Redis keys are grouped into folders by ":"
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

    /// Enter / F4 / double-click
    pub fn activate_node(&mut self, n: &Node, f4: bool) {
        let kind = self.kind_of(&n.conn);
        // Connection·DB·schema·folder: Enter/double-click = expand (regardless of DB kind). F4 / Ctrl+↓ = console, key browser for Redis
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
    /// Also search tabs stashed in other workspaces (so query results finished meanwhile aren't lost)
    pub fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        if let Some(i) = self.tab_index(id) {
            return self.tabs.get_mut(i);
        }
        self.workspaces.iter_mut().filter_map(|w| w.saved.as_mut()).flat_map(|s| s.tabs.iter_mut()).find(|t| t.id == id)
    }

    /// Console tabs with uncommitted changes
    fn pending_tabs(&self, conn: Option<&str>) -> Vec<u64> {
        let saved = self.workspaces.iter().filter_map(|w| w.saved.as_ref()).flat_map(|s| s.tabs.iter());
        self.tabs.iter().chain(saved).filter(|t| conn.is_none_or(|c| t.conn == c) && matches!(&t.kind, TabKind::Console(c) if c.in_tx)).map(|t| t.id).collect()
    }

    /// Close a tab. Ask first if there are uncommitted changes (closing rolls them back)
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

    /// Unsplit: all tabs into one pane
    pub fn unsplit(&mut self) {
        for t in &mut self.tabs {
            t.group = 0;
        }
        self.panes = crate::panes::Pane::Leaf(0);
        self.group_active = [None; crate::panes::MAX];
        self.group_active[0] = self.tabs.get(self.active).map(|t| t.id);
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
    fn find_similar(&mut self) {
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
        let TabKind::Table(me) = &t.kind else { return t.title.clone() };
        let same: Vec<&TableTab> = self
            .tabs
            .iter()
            .filter(|x| x.id != t.id)
            .filter_map(|x| match &x.kind {
                TabKind::Table(o) if o.table == me.table => Some(o),
                _ => None,
            })
            .collect();
        if same.is_empty() {
            t.title.clone()
        } else if same.iter().any(|o| o.schema == me.schema) {
            format!("{}.{}.{}", t.db, me.schema, me.table)
        } else {
            format!("{}.{}", me.schema, me.table)
        }
    }

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

    /// Reload table tabs still sitting on a failed query (with no unsaved edits)
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
                        // If the first row is a key, show it right away (if a folder, expand it to pick)
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

    /// Re-read keys after running a Redis command
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

    /// Execution settings (DataGrip Settings › Database › Query Execution). Picking applies immediately and the window stays open
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

    /// confirmed = whether a dangerous statement (UPDATE/DELETE without WHERE, DROP, TRUNCATE) was confirmed
    pub fn run_stmts(&mut self, stmts: Vec<String>, explain: bool, confirmed: bool) {
        self.run_stmts_checked(stmts, explain, confirmed, confirmed)
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
            self.popup = Some(Popup::Confirm { text: format!("{why}:\n  {preview}\nRun it anyway?"), on_yes: Box::new(move |a| a.run_stmts_checked(stmts, explain, true, prod_ok)) });
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
                                    Ok(None) if qurry_core::guard::is_ddl(s, kind) => "schema change".into(),
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
        // EXPLAIN ANALYZE actually runs the statement. PostgreSQL·MySQL run changing statements inside a transaction and roll back (below).
        // Drivers may not support transactions, so ask
        let wraps = matches!(kind, DbKind::Postgres | DbKind::Mysql);
        if analyze
            && !wraps
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
        // While running, show Output (the SQL being run instead of the previous results)
        c.show_output = true;
        let session = c.session.clone();
        let in_tx0 = c.in_tx;
        let st = self.state.clone();
        let stmts2 = stmts.clone();
        let tx = self.tx.clone();
        let echo_name = conn_name.clone();
        self.status = "Executing…".into();
        self.spawn(
            async move {
                let mut out = vec![];
                for s in stmts2 {
                    // Put the running statement in Output right away (on one line, truncated if too long)
                    let echo: String = s.replace('\n', " ").chars().take(300).collect();
                    let line = format!("{echo_name}> {echo}{}", if s.chars().count() > 300 { "…" } else { "" });
                    let _ = tx.send(Box::new(move |a: &mut App| {
                        if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            c.log(false, line);
                        }
                    }));
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
                    // Explain Analyze of a changing statement: run inside a transaction (a savepoint if one is already open) and always roll back
                    let dry = analyze && wraps && !qurry_core::guard::is_read(&s, kind);
                    if dry {
                        let begin = if in_tx0 { "SAVEPOINT qurry_explain" } else { "BEGIN" };
                        if let Err(e) = st.run_query(target.clone(), begin.into(), None, Some(session.clone())).await {
                            out.push((s, Err(err(e)), false));
                            break;
                        }
                    }
                    let r = st.run_query(target.clone(), q, Some(500), Some(session.clone())).await.map_err(err);
                    let r = if dry {
                        let back = if in_tx0 { "ROLLBACK TO SAVEPOINT qurry_explain" } else { "ROLLBACK" };
                        match st.run_query(target.clone(), back.into(), None, Some(session.clone())).await {
                            Ok(_) => r.map(|res| qurry_core::db::QueryResult { in_transaction: in_tx0, ..res }),
                            Err(e) => Err(format!("Roll back after Explain Analyze failed: {}", err(e))),
                        }
                    } else {
                        r
                    };
                    let stop = r.is_err();
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
                let mut failed = false;
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
                            if dry {
                                c.log(false, "Explain Analyze ran the statement in a transaction and rolled it back — nothing was changed".into());
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
                // If tables·columns were created, changed, or dropped, show it in the explorer right away
                if ddl {
                    a.schema_changed(&hist_conn, &hist_db);
                }
                // Even on error the session may have an open transaction (e.g. first Manual statement failed) → sync the Commit/Roll back display
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

    // ───────── Table editing ─────────

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

    // ───────── Connections ─────────

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

    /// After a structure change in the console: re-read the expanded nodes under that database (expansion kept)
    pub fn schema_changed(&mut self, conn: &str, db: &str) {
        self.forget_completion(conn);
        let Some(dbn) = self.tree.children.values().flatten().find(|n| matches!(n.kind, NodeKind::Db) && n.conn == conn && n.db.as_deref() == Some(db)).cloned() else { return };
        let under = |k: &str| k == dbn.key || k.starts_with(&format!("{}|", dbn.key));
        let reload: Vec<Node> = std::iter::once(&dbn)
            .chain(self.tree.children.values().flatten())
            .filter(|n| under(&n.key) && self.tree.expanded.contains(&n.key))
            .cloned()
            .collect();
        self.tree.forget(&dbn.key);
        let mut seen = std::collections::HashSet::new();
        for n in reload {
            if seen.insert(n.key.clone()) {
                self.load_children(n);
            }
        }
    }

    pub fn refresh_node(&mut self, n: &Node) {
        let key = n.key.clone();
        // The schema may have changed, so re-read autocomplete metadata too
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
        let ch = |xs: &[&str]| FieldKind::Choice(xs.iter().map(|x| x.to_string()).collect());
        let saved = |b: bool| if b { "<saved>".to_string() } else { String::new() };
        let color = crate::ui::color_name(c.color.as_deref());
        let mut colors: Vec<&'static str> = std::iter::once("none").chain(crate::ui::COLOR_PRESETS.iter().map(|p| p.0)).collect();
        if color == "custom" {
            colors.push("custom");
        }
        self.popup = Some(Popup::ConnForm(ConnForm {
            fields: vec![
                f("Name", c.name.clone(), FieldKind::Text),
                f("Color", color.into(), ch(&colors)),
                // Workspaces where this Data Source is shown (moved on save if changed)
                f("Workspace", self.ws_label(edit.as_ref().map(|e| self.ws_of_conn(&e.id)).unwrap_or(self.cur_ws)), FieldKind::Choice((0..self.workspaces.len()).map(|i| self.ws_label(i)).collect())),
                f("Type", kind, ch(&["postgres", "mysql", "redis", "sqlite", "duckdb", "mssql", "clickhouse", "mongodb", "oracle", "qdrant", "chroma", "milvus"])),
                f("Host", c.host.clone(), FieldKind::Text),
                f("Port", c.port.to_string(), FieldKind::Text),
                f("User", c.user.clone(), FieldKind::Text),
                f("Password", saved(c.password_saved), FieldKind::Secret),
                f("Database", c.database.clone(), FieldKind::Text),
                f("Read-only", if c.read_only { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("Production", if c.options.confirm_changes { "on" } else { "off" }.into(), FieldKind::Toggle),
                // MySQL: whether plaintext is allowed for old remote servers that can't do TLS
                f("Unencrypted OK", if c.options.allow_plaintext { "on" } else { "off" }.into(), FieldKind::Toggle),
                // Whether it's visible to qurry-mcp (AI tools). New sources start off
                f("MCP", if edit.is_some() { mcp_label(&c) } else { "off" }.into(), ch(&["off", "read", "write"])),
                f("SSH tunnel", if ssh.enabled { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("SSH host", ssh.host.clone(), FieldKind::Text),
                f("SSH port", ssh.port.to_string(), FieldKind::Text),
                f("SSH user", ssh.user.clone(), FieldKind::Text),
                f("SSH auth", if ssh.auth == SshAuth::Key { "key" } else { "password" }.into(), ch(&["key", "password"])),
                f("SSH key file", ssh.key_path.clone(), FieldKind::Text),
                f("SSH secret", saved(c.ssh_secret_saved), FieldKind::Secret),
                f("SSL", if c.active_ssl().is_some() { "on" } else { "off" }.into(), FieldKind::Toggle),
                // Newly enabled SSL is verify-full (require only encrypts and doesn't verify the certificate)
                f("SSL mode", c.ssl.as_ref().map(|x| x.mode().to_string()).unwrap_or_else(|| "verify-full".into()), ch(&["require", "verify-ca", "verify-full"])),
                f("CA file", c.ssl.as_ref().map(|x| x.ca_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Client cert", c.ssl.as_ref().map(|x| x.cert_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Client key", c.ssl.as_ref().map(|x| x.key_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Redis mode", c.redis_mode().into(), ch(&["standalone", "sentinel", "cluster"])),
                f("Sentinel master", c.redis.as_ref().map(|x| x.sentinel_master.clone()).unwrap_or_default(), FieldKind::Text),
                f("Redis nodes", c.redis.as_ref().map(|x| x.nodes.clone()).unwrap_or_default(), FieldKind::Text),
            ],
            edit,
            sel: 0,
            message: None,
        }));
    }

    /// Form → (config, password, SSH secret)
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
        c.options.allow_plaintext = v("Unencrypted OK") == "on";
        // Save the shown value as is (leaving it unset would reopen it automatically after turning Production off)
        c.options.mcp = Some(v("MCP"));
        // custom = a color outside the presets picked in the GUI: leave it
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

    // ───────── Popup: palette ─────────

    pub fn palette_actions(&mut self) {
        let items = ACTIONS
            .iter()
            .filter(|(a, ..)| *a != Action::Palette)
            .map(|(a, n, k)| PItem { label: n.to_string(), hint: if k.is_empty() { self.keys.label(*a) } else { k.to_string() }, action: PAction::Act(*a) })
            .collect();
        self.popup = Some(Popup::Palette(Palette { title: "Actions".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    pub fn palette_tabs(&mut self) {
        // Recently viewed tabs → then not-yet-viewed tabs (restored but never selected)
        let seen = self.mru.iter().filter_map(|id| self.tabs.iter().find(|t| t.id == *id));
        let rest = self.tabs.iter().filter(|t| !self.mru.contains(&t.id));
        let items = seen.chain(rest).map(|t| PItem { label: self.tab_label(t), hint: t.db.clone(), action: PAction::Tab(t.id) }).collect();
        self.popup = Some(Popup::Palette(Palette { title: "Recent Tabs".into(), input: TextArea::default(), items, sel: 1, loading: 0, cache: Default::default() }));
    }

    /// Search Everywhere (the GUI's ⇧⇧): recent tabs + actions + DB objects in one list
    pub fn palette_everywhere(&mut self) {
        let mut items: Vec<PItem> = self.mru.iter().filter_map(|id| self.tabs.iter().find(|t| t.id == *id)).map(|t| PItem { label: self.tab_label(t), hint: format!("tab · {}", t.db), action: PAction::Tab(t.id) }).collect();
        items.extend(ACTIONS.iter().filter(|(a, ..)| *a != Action::SearchEverywhere).map(|(a, n, k)| {
            let key = if k.is_empty() { self.keys.label(*a) } else { k.to_string() };
            PItem { label: n.to_string(), hint: format!("action · {key}"), action: PAction::Act(*a) }
        }));
        self.palette_objects("Search Everywhere", items);
    }

    /// Go to Database Object: default DB of connected Data Sources + expanded DBs
    /// Go to Database Object / Search Everywhere: like the GUI, objects of the visible DBs (at most 8) of each connected Data Source.
    /// Redis and single DB: default DB only
    pub fn palette_objects(&mut self, title: &str, items: Vec<PItem>) {
        let conns: Vec<ConnectionConfig> = self.conns.iter().filter(|c| c.connected).map(|c| c.config.clone()).collect();
        // Unconnected Data Sources use the list read last time (connects when picked)
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
                // Default DB first (it's searched even if the DB list can't be read)
                dbs.retain(|d| d != &def);
                dbs.insert(0, def);
                dbs.truncate(8);
                a.index_into_palette(&cfg, dbs);
            });
        }
    }

    /// Add DB objects to the open palette (turns one Data Source's loading count into the number of DBs)
    fn index_into_palette(&mut self, cfg: &ConnectionConfig, dbs: Vec<String>) {
        // Search Everywhere also finds columns (the GUI's All / Symbols)
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

    /// Column → picking it opens its table (Ctrl+B opens that table's DDL)
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
            // Columns go after tables·actions with the same name (same as the GUI's All)
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
            PAction::OpenUndo { conn, db, undo, note } => self.open_undo(conn, db, undo, note),
            PAction::Pick(v) => self.flow_pick(v),
            PAction::DbaSql(t) => self.dba_sql(t),
            PAction::ExportTo(f, clipboard) => self.export_to(f, clipboard),
            PAction::Exec(e) => {
                self.exec = e;
                self.palette_exec();
                // Stay on the selected row
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

    /// Single-line input with autocomplete (typed word filters words, ⌃Space / Tab opens it right away)
    pub fn input_words(&mut self, title: impl Into<String>, init: &str, words: Vec<String>, on_ok: impl FnOnce(&mut App, String) + 'static) {
        let mut t = TextArea::new(vec![init.to_string()]);
        t.move_cursor(tui_textarea::CursorMove::End);
        t.set_cursor_line_style(ratatui::style::Style::default());
        self.popup = Some(Popup::Input { title: title.into(), input: t, on_ok: Box::new(on_ok), words, complete: None });
    }

    /// Autocomplete candidates for the table tab's WHERE / ORDER BY: columns + common keywords
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
                // In a table data tab, that table's DDL
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
            Action::ProductionLog => self.production_log(),
            Action::GenerateTestData => self.generate_data_start(),
            Action::CopyRelatedRows => self.copy_rows_start(),
            Action::ChartResult => self.chart_result(),
            Action::NewTable => self.new_table(),
            Action::ModifyTable => self.modify_table(),
            Action::NewView => self.new_object("view"),
            Action::NewSchema => self.new_object("schema"),
            Action::NewDatabase => self.new_object("database"),
            Action::NewSequence => self.new_object("sequence"),
            Action::RenameObject => self.rename_object(),
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
            Action::FindSimilar => self.find_similar(),
            Action::RagSearch => self.rag_search_start(),
            Action::RagBuild => self.rag_build_start(),
            Action::RagNew => self.rag_new_start(),
            Action::RagKey => self.rag_key_start(),
            Action::WsSwitch => self.ws_pick(false),
            Action::WsNew => self.ws_new(),
            Action::WsRename => self.ws_rename(),
            Action::WsDelete => self.ws_delete(),
            Action::WsMoveTab => self.ws_pick(true),
            Action::WsAddConn => self.ws_add_conn_pick(),
            Action::WsMoveConn => self.ws_move_conn_pick(),
            Action::WsRemoveConn => self.ws_remove_conn(),
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
                // To the next pane (cycles from left/top in order)
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
                // Move the left/right divider next to the focused pane (even if the nearest one is a top/bottom divider). Top/bottom if there's no left/right
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
            Action::PasswordKey => self.password_key(),
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

    /// Next/previous tab (within the focused pane)
    fn cycle_tab(&mut self, d: isize) {
        let g = self.focused_group();
        let idx: Vec<usize> = (0..self.tabs.len()).filter(|&i| self.tabs[i].group == g).collect();
        let Some(p) = idx.iter().position(|&i| i == self.active) else { return };
        let n = idx.len() as isize;
        self.activate(idx[(p as isize + d).rem_euclid(n) as usize]);
    }

    /// Split the focused pane (up to 4 panes): move the current tab into the new pane (right/below)
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
        // If the pane has only one tab, open a new console on the same DB in the right/lower pane
        let (conn, db) = (self.tabs[self.active].conn.clone(), self.tabs[self.active].db.clone());
        if self.kind_of(&conn) == DbKind::Redis {
            self.status = "Open another tab to split".into();
            return;
        }
        self.open_console(&conn, Some(db));
        let id = self.tabs[self.active].id;
        self.drop_tab(id, DropTarget::NewSplit(g, dir));
    }

    /// Database Explorer width: 1/4 of the screen width if unset (26~48)
    pub fn explorer_width(&self, total: u16) -> u16 {
        let auto = (total / 4).clamp(26, 48);
        self.explorer_w.unwrap_or(auto).clamp(12, total.saturating_sub(20).max(12))
    }

    pub fn set_explorer_width(&mut self, w: i16) {
        let max = self.hits.main.width.saturating_sub(20).max(12) as i16;
        self.explorer_w = Some(w.clamp(12, max) as u16);
    }

    /// Adjust the console editor/result ratio
    /// To the adjacent pane: axis = Down means up(back)/down, Right means left(back)/right.
    /// Based on the last drawn pane positions, the nearest overlapping pane in that direction
    fn focus_pane(&mut self, axis: SplitDir, back: bool) {
        let cur = self.focused_group();
        // (pane, left, right, top, bottom)
        let rect = |&(g, bar, body): &(usize, ratatui::layout::Rect, ratatui::layout::Rect)| (g, bar.x, bar.x + bar.width, bar.y, body.y + body.height);
        let Some((_, x0, x1, top, bottom)) = self.hits.groups.iter().map(rect).find(|r| r.0 == cur) else { return };
        let next = self
            .hits
            .groups
            .iter()
            .map(rect)
            .filter_map(|(g, a0, a1, t, b)| {
                // Dividers (borders) may overlap by about one cell, so allow 1 cell of slack
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

    // ───────── Autocomplete ─────────

    /// Drop a Data Source's autocomplete metadata (disconnect·refresh)
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

    /// Recompute candidates at the cursor position. explicit = Ctrl+Space (otherwise '.' typed·typing with the list open)
    /// Manage Shown Schemas: read the DB list and open the window (db = expand that DB if opened from a DB row)
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

    /// Read schemas of the DB expanded in the window
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

    /// Save the chosen scope to the Data Source and redraw the tree.
    /// Connection settings are unchanged, so write to the store directly (save_connection closes the pool and drops open transactions)
    pub fn apply_scope(&mut self, e: crate::scope::ScopeEditor) {
        // Re-read the saved config and change only the scope (so read-only·host etc. changed meanwhile in the GUI·MCP aren't reverted)
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

    /// Check for a new version right now (⌃K › Check for Updates)
    /// Master key for the password file: config folder ↔ OS keychain (asks, then moves. Runs separately since a keychain popup may appear)
    fn password_key(&mut self) {
        use qurry_core::secrets::KeyLocation;
        let to_keychain = self.state.secret_key_location() == KeyLocation::File;
        let text = if to_keychain {
            "The password key is in the config folder (secret.key).\n\nMove it into the system keychain? The GUI and the MCP server use the same key, so each asks for keychain access once (on macOS, again after updates while Qurry isn't code-signed)."
        } else {
            "The password key is in the system keychain.\n\nMove it back to the config folder (no keychain prompts)?"
        };
        let st = self.state.clone();
        self.popup = Some(Popup::Confirm {
            text: text.into(),
            on_yes: Box::new(move |a| {
                a.status = "Waiting for the system keychain…".into();
                a.spawn(async move { tokio::task::spawn_blocking(move || st.set_secret_key_location(to_keychain)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string()) }, |a, r| {
                    a.status = match r {
                        Ok(KeyLocation::Keychain) => "The password key is now in the system keychain".into(),
                        Ok(KeyLocation::File) => "The password key is now in the config folder".into(),
                        Err(e) => e,
                    };
                });
            }),
        });
    }

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

    /// New version window: changes since the current version (per version) + how to update
    pub fn show_update(&mut self, r: qurry_core::update::Release) {
        let cur = env!("CARGO_PKG_VERSION");
        let how = format!("{} › Update Qurry & Restart", self.keys.label(Action::Palette));
        let changes = qurry_core::update::changes_text(&r.changes_since(cur));
        // How to update goes at the top (with many versions the bottom of the window gets cut off)
        let text = format!("You have {cur} — {how}\n\n{}", if changes.is_empty() { "See the release page for what changed.\n".into() } else { changes });
        self.status = format!("Qurry {} is available — {how}", r.version);
        self.popup = Some(Popup::Message { title: format!("Qurry {} is available", r.version), text });
        self.update = Some(r.version);
    }

    /// Download the latest release, swap it in, and relaunch when done (open tabs are saved on quit)
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
            self.load_completion(target.clone());
        }
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let (row, col) = c.editor.cursor();
        let lines = c.editor.lines();
        let before: String = lines.get(row).map(|l| l.chars().take(col).collect()).unwrap_or_default();
        // Called on every key, so look only at lines around the cursor (don't split a long script every time)
        let (from, to) = (row.saturating_sub(200), (row + 50).min(lines.len()));
        let window = &lines[from..to];
        let text = window.join("\n");
        let stmt = sql::statement_at(&text, byte_offset(window, row - from, col), kind).unwrap_or_default();
        let empty = complete::Meta::new(vec![]);
        // Value · json key spot: use already-read values if any, otherwise fetch and reopen
        if let Some(h) = meta.as_deref().and_then(|m| complete::hint_at(m, &stmt, &before, &db, kind == DbKind::Mysql)) {
            use complete::HintMode;
            let after = lines.get(row).and_then(|l| l.chars().nth(col));
            let key = |mode: HintMode| format!("{target}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{mode:?}", h.schema, h.table, h.column, h.path.join("\u{2}"));
            let hints_cache = &self.hints;
            let fresh = |k: &str| hints_cache.get(k).filter(|(at, _)| at.elapsed() < std::time::Duration::from_secs(120)).map(|(_, v)| v.clone());
            let mut want = (h.mode, fresh(&key(h.mode)));
            // JSON without keys (array of objects etc.): instead of keys, frequent key: value pairs as @> conditions (replacing including the operator)
            if h.mode == HintMode::Keys && want.1.as_ref().is_some_and(|v| v.is_empty()) && kind != DbKind::Mysql {
                want = (HintMode::Pairs, fresh(&key(HintMode::Pairs)));
            }
            match want {
                (mode, Some(hints)) => {
                    let as_pairs = mode == HintMode::Pairs && h.mode == HintMode::Keys;
                    let items = complete::hint_items(&h, &hints, h.quoted && after != Some('\''), as_pairs);
                    let prefix = if as_pairs { h.op.chars().count() } else { h.typed.chars().count() };
                    c.complete = if items.is_empty() { None } else { Some(Completion { items, sel: 0, prefix }) };
                }
                (mode, None) => {
                    c.complete = None;
                    let k = key(mode);
                    if self.hints_loading.insert(k.clone()) {
                        let st = self.state.clone();
                        let req = qurry_core::hints::HintReq { schema: h.schema, table: h.table, column: h.column, path: h.path, keys: mode == HintMode::Keys, pairs: mode == HintMode::Pairs };
                        self.spawn(async move { st.column_hints(target, req).await.map_err(err) }, move |a, r| {
                            a.hints_loading.remove(&k);
                            match r {
                                Ok(v) => {
                                    a.hints.insert(k, (std::time::Instant::now(), v));
                                    a.update_completion(explicit);
                                }
                                Err(e) => a.status = format!("Value suggestions: {e}"),
                            }
                        });
                    }
                }
            }
            return;
        }
        let (prefix, items) = complete::suggest(meta.as_deref().unwrap_or(&empty), &stmt, &before, &db);
        // Auto-open only after '.' or while typing a word
        let keep = explicit || prefix > 0 || before.ends_with('.');
        c.complete = if items.is_empty() || !keep { None } else { Some(Completion { items, sel: 0, prefix }) };
        if explicit && c.complete.is_none() {
            self.status = if meta.is_none() { "Loading completion metadata… try again in a moment".into() } else { "No suggestions".into() };
        }
    }

    /// Replace the word before the cursor with the picked candidate
    pub fn accept_completion(&mut self) {
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let Some(cmp) = c.complete.take() else { return };
        let Some(item) = cmp.items.get(cmp.sel) else { return };
        for _ in 0..cmp.prefix {
            c.editor.delete_char();
        }
        c.editor.insert_str(&item.insert);
    }

    /// Save the keymap to keymap.json
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

/// 12.3 s · 1 m 05 s (same as the GUI's RunningBar)
pub fn elapsed_text(ms: u64) -> String {
    if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        let s = ms / 1000;
        format!("{} m {:02} s", s / 60, s % 60)
    }
}

impl App {
    /// Elapsed time if this tab is running · loading (since first seen)
    pub fn run_elapsed(&mut self, idx: usize) -> Option<std::time::Duration> {
        let t = self.tabs.get(idx)?;
        let running = match &t.kind {
            TabKind::Console(c) => c.running,
            TabKind::Table(tt) => tt.running,
            _ => false,
        };
        if !running {
            self.run_since.remove(&t.id);
            return None;
        }
        Some(self.run_since.entry(t.id).or_insert_with(std::time::Instant::now).elapsed())
    }

    /// If a tab is running, the elapsed time needs redrawing
    pub fn any_running(&self) -> bool {
        self.tabs.iter().any(|t| matches!(&t.kind, TabKind::Console(c) if c.running) || matches!(&t.kind, TabKind::Table(tt) if tt.running))
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

/// Redis value → grid (None for string: shown as text)
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

/// MCP exposure shown in the form (old sources without a setting keep current behavior)
fn mcp_label(c: &ConnectionConfig) -> &'static str {
    match c.options.mcp.as_deref() {
        Some("off") => "off",
        Some("read") => "read",
        Some(_) => "write",
        None if c.options.confirm_changes => "off",
        None => "write",
    }
}
