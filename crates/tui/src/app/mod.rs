//! TUI state and actions. All real DB work goes through qurry_core::app::AppState (same engine as the GUI).
use qurry_core::complete::{self, Item, Options};
use crate::grid::{raw_text, Grid};
use crate::tree::{Node, NodeKind, Tree};
use qurry_core::app::{AppState, ConnectionView};
use qurry_core::db::{ColumnInfo, QueryResult};
use qurry_core::meta::{IndexEntry, SchemaObjects};
use qurry_core::redisdb::{KeyDetail, KeyRow};
use qurry_core::sql;
use qurry_core::store::{ConnectionConfig, DbKind, RedisMode, SshAuth, SshConfig, SsmConfig, SslConfig};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tui_textarea::TextArea;

mod actions;
mod completion;
mod conns;
mod explorer;
mod palette;
mod redis;
mod run;
mod splits;
mod tables;
mod tabs;

pub use actions::{title, ACTIONS};
#[cfg(test)]
pub(crate) use run::explain_analyze_ask_text;

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
    /// The statement the grid shows the result of (run again after its edits are submitted)
    pub result_sql: String,
    pub output: Vec<(bool, String)>,
    pub show_output: bool,
    pub running: bool,
    pub tx_manual: bool,
    pub in_tx: bool,
    /// Open autocomplete list
    pub complete: Option<Completion>,
    /// Gutter marks of the statements runs executed
    pub marks: Marks,
}

/// How a run statement's gutter mark shows it (DataGrip): … running, ✓ done, ✗ failed (symbols, so NO_COLOR reads them too)
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MarkState {
    Running,
    Ok,
    Err,
}

impl MarkState {
    pub fn symbol(self) -> char {
        match self {
            MarkState::Running => '…',
            MarkState::Ok => '✓',
            MarkState::Err => '✗',
        }
    }
}

/// A run statement's editor rows (first, last) and its state (None until it starts). `stmt`: its index in the latest run
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mark {
    pub rows: (usize, usize),
    pub state: Option<MarkState>,
    stmt: Option<usize>,
}

/// The console's gutter marks and the editor lines their rows refer to. Edits move them along (`follow`): marks below the changed
/// lines shift, marks on them are dropped (their statement was edited)
#[derive(Default)]
pub struct Marks {
    list: Vec<Mark>,
    lines: Vec<String>,
}

impl Marks {
    /// Follow the edits from the lines the marks refer to, to `now`
    pub fn follow(&mut self, now: &[String]) {
        if self.list.is_empty() || self.lines == now {
            return;
        }
        let old = &self.lines;
        let same = old.iter().zip(now).take_while(|(a, b)| a == b).count();
        let tail = old[same..].iter().rev().zip(now[same..].iter().rev()).take_while(|(a, b)| a == b).count();
        let (changed_end, added) = (old.len() - tail, now.len() as isize - old.len() as isize);
        self.list.retain_mut(|m| match m.rows {
            (_, last) if last < same => true,
            (first, last) if first >= changed_end => {
                m.rows = ((first as isize + added) as usize, (last as isize + added) as usize);
                true
            }
            _ => false,
        });
        self.lines = now.to_vec();
    }

    /// A run is about to start over the statements on these rows of `now` (in order): they are marked as they run
    pub fn start(&mut self, now: &[String], rows: &[(usize, usize)]) {
        self.follow(now);
        self.list.retain(|m| m.state.is_some());
        self.list.iter_mut().for_each(|m| m.stmt = None);
        self.list.extend(rows.iter().enumerate().map(|(i, &rows)| Mark { rows, state: None, stmt: Some(i) }));
        self.lines = now.to_vec();
    }

    /// Statement i of the latest run is running, done or failed. Once one runs, the marks of earlier runs go: the gutter shows
    /// only the latest run (not when it starts, which may still be cancelled)
    pub fn set(&mut self, now: &[String], i: usize, state: MarkState) {
        self.follow(now);
        if !self.list.iter().any(|m| m.stmt == Some(i)) {
            return;
        }
        self.list.retain(|m| m.stmt.is_some());
        if let Some(m) = self.list.iter_mut().find(|m| m.stmt == Some(i)) {
            m.state = Some(state);
        }
    }

    /// The mark drawn on an editor row (a statement's first)
    pub fn at(&self, row: usize) -> Option<MarkState> {
        self.list.iter().filter(|m| m.rows.0 == row).find_map(|m| m.state)
    }
}

/// Max lines kept in the console Output log (same as the GUI)
const OUTPUT_MAX: usize = 1000;
/// Output line prefix of a warning (drawn in the warn color, like the GUI): it ran, so not an error to run again
pub const WARNING: &str = "Warning: ";

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

/// What a table tab's WHERE / ORDER BY input completes against (complete::suggest_filter): the console's metadata of the tab's
/// target, or only the tab's own columns while that loads
pub struct FilterSource {
    pub meta: Arc<complete::Meta>,
    pub schema: String,
    pub table: String,
    pub order_by: bool,
    pub db: String,
    pub mysql: bool,
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

#[derive(Default)]
pub struct TextTab {
    pub text: String,
    /// DDL: tree kind of the object (for the tab glyph); empty for other text (diagram)
    pub kind: String,
    pub scroll: u16,
    /// DDL: the object (schema, name), which r fetches again; a table's also has the Structure view (v)
    pub ddl_of: Option<(String, String)>,
    /// Showing the Structure view instead of the SQL
    pub structure_view: bool,
    /// The loaded structure, or why it failed (None: not loaded yet)
    pub structure: Option<Result<qurry_core::meta::TableStructure, String>>,
}

impl TextTab {
    /// A table's DDL: has the Structure view (v)
    pub fn has_structure(&self) -> bool {
        self.ddl_of.is_some() && crate::structure::has_structure(&self.kind)
    }
}

/// DBA reports (sessions·locks, slow queries, size·indexes, maintenance, server, privileges). Sessions·locks reload every 3 seconds
pub struct DbaTab {
    pub report: String,
    pub info: Option<qurry_core::dba::ReportInfo>,
    pub grid: Option<Grid>,
    /// When the grid was loaded (a failed refresh keeps the grid and says how old it is)
    pub loaded: Option<std::time::Instant>,
    pub error: Option<String>,
    pub running: bool,
    /// Bumped by each load; only the latest load's result is put in (an older run of the same report must not clear running)
    pub run: u64,
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
    /// DBA report row action: EXPLAIN of the row's query → console
    DbaExplain,
    /// Export: format picked → pick target
    ExportFormat(qurry_core::transfer::Format),
    /// Export: (format, clipboard)
    ExportTo(qurry_core::transfer::Format, bool),
    Object { conn: String, db: String, e: IndexEntryLite },
    Tab(u64),
    /// Statements picked in the execution picker popup
    Run { stmts: Vec<String>, rows: Vec<(usize, usize)>, explain: bool },
    /// Change execution settings
    Exec(qurry_core::exec::ExecSettings),
    /// Update channel picked
    Channel(qurry_core::update::Channel),
    /// Grid alignment: a value class picked (→ pick where it sits)
    AlignClass(qurry_core::format::ValueClass),
    /// Grid alignment: where a value class sits, picked
    Align(qurry_core::format::ValueClass, qurry_core::format::Align),
    /// Theme picked
    Theme(crate::theme::Theme),
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
    /// The Test Connection running now: when it started, and how to cancel it (dropping it stops its SSH / SSM tunnel)
    pub testing: Option<(std::time::Instant, tokio::task::AbortHandle)>,
}

/// The Tunnel choice a form field belongs to (None: shown with every choice)
fn tunnel_of(label: &str) -> Option<&'static str> {
    if label.starts_with("SSH ") {
        Some("ssh")
    } else if label.starts_with("AWS ") || label == "SSM target" {
        Some("aws ssm")
    } else {
        None
    }
}

impl ConnForm {
    pub fn value(&self, label: &str) -> &str {
        self.fields.iter().find(|f| f.label == label).map_or("", |f| f.value.as_str())
    }

    /// Indices of the fields shown: only the chosen tunnel's fields (the other tunnel's keep their values for switching back)
    pub fn visible(&self) -> Vec<usize> {
        let tunnel = self.value("Tunnel");
        (0..self.fields.len()).filter(|&i| tunnel_of(self.fields[i].label).is_none_or(|t| t == tunnel)).collect()
    }

    /// The most fields any Tunnel choice shows: the popup keeps that height, so choosing another tunnel doesn't resize it
    pub fn max_visible(&self) -> usize {
        let count = |t: &str| self.fields.iter().filter(|f| tunnel_of(f.label) == Some(t)).count();
        self.fields.len() - count("ssh").min(count("aws ssm"))
    }

    /// Cancels the running Test Connection (its tunnel and aws CLI stop with it)
    pub fn cancel_test(&mut self) -> bool {
        let Some((_, h)) = self.testing.take() else { return false };
        h.abort();
        self.message = Some((false, "Test cancelled".into()));
        true
    }

    /// Moves the selection by `d` shown fields (wrapping around)
    pub fn step(&mut self, d: isize) {
        let v = self.visible();
        let pos = v.iter().position(|&i| i == self.sel).unwrap_or(0) as isize;
        self.sel = v[(pos + d).rem_euclid(v.len() as isize) as usize];
    }
}

pub enum Popup {
    /// F1 keymap. The Commands section at the top is selectable (sel). record = waiting for a new key (true = add to existing keys)
    /// sel counts the top Commands and the fixed key list below as one sequence (only Commands can be changed)
    Help { sel: usize, scroll: u16, record: Option<bool>, message: Option<String> },
    Message { title: String, text: String },
    /// A Message over another popup (Production Log's note over the log): closing it goes back there, as it was
    Note { title: String, text: String, back: Box<Popup> },
    /// Single-line input. filter = a table tab's WHERE / ORDER BY completes like the console, complete = its open list
    /// body = app-written explanation shown in a box under the input (never interpolated names, so a name can't pose as it)
    Input { title: String, body: String, input: TextArea<'static>, on_ok: Box<dyn FnOnce(&mut App, String)>, filter: Option<FilterSource>, complete: Option<Completion> },
    /// `opened` is set when the confirm comes up on its own (open_after_notices, or a notice over it closing): then, as for
    /// AllowPlaintext, y / Enter count only after TLS_ASK_GRACE without any key, so typing meant for the editor or a form never answers it
    Confirm { text: String, on_yes: Box<dyn FnOnce(&mut App)>, opened: Option<std::time::Instant> },
    /// The TLS opt-in (offer_tls_opt_in). It opens on its own when a background load fails, so only a y pressed after TLS_ASK_GRACE
    /// without any key (`opened` restarts at each other key) allows plaintext: never Enter, never a key that was meant for the editor or the tree
    AllowPlaintext { text: String, on_yes: Box<dyn FnOnce(&mut App)>, opened: std::time::Instant },
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
    MarkNode,
    DropObject,
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
    MarkRow,
    ValueEditor,
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
    GridAlign,
    Theme,
    CheckUpdate,
    UpdateChannel,
    InstallUpdate,
    Help,
    Quit,
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
    /// The open confirm's question or warnings didn't fit the window: y / Enter don't answer it
    pub confirm_unseen: bool,
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
    /// Confirms an async result opened while the user works in another popup (a form, a palette): the next one opens when that closes
    pub(crate) waiting: Vec<Popup>,
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
    /// The value panel beside result and table grids (v in the grid, as the GUI's Value Editor panel): the current cell's whole value
    pub value_panel: bool,
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
    pub exec: qurry_core::exec::ExecSettings,
    /// DDL tabs of tables open in the Structure view (the last choice of v, saved in the workspace)
    pub ddl_structure: bool,
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
    /// What the terminal can show (set from the environment at startup; drawing maps the frame to it)
    pub color: crate::theme::ColorMode,
    /// The Theme setting (settings.json, read at startup and by ⌃k › Theme…) and what Auto picks for this terminal (COLORFGBG, set at startup)
    pub theme: crate::theme::Theme,
    pub auto_light: bool,
    /// Grid alignment per value class, shared with the GUI's Settings › Appearance › Alignment (settings.json, read at startup and by ⌃k › Grid Alignment…)
    pub align: qurry_core::format::Alignments,
    /// The last frame was the 'terminal too small' message: keys and clicks would act on a screen that isn't drawn
    pub too_small: bool,
    /// Tests only (testkit): answers console statements and the Production row counts instead of the database
    #[cfg(test)]
    pub fake_db: Option<FakeDb>,
    /// Tests only: every command run_action ran, in order (tests/actions.rs checks keys reach the right one)
    #[cfg(test)]
    pub ran: Vec<Action>,
}

/// Tests only (testkit): stands in for the database, answering each query by its text
pub type FakeDb = fn(&str) -> Result<QueryResult, String>;

/// Palette search score (qurry_shared::fuzzy with the TUI's scoring); None = no match
pub fn fuzzy(q: &str, text: &str) -> Option<i64> {
    qurry_core::fuzzy::score(q, text, Default::default()).map(|s| s as i64)
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
            // The file stays as it is (saving a key refuses to overwrite it): say how to get the user's keys back
            Err(e) => (Default::default(), Some(format!("keymap.json ignored ({e}) — default keys in use; fix or delete {} and restart", state.store.dir().join("keymap.json").display()))),
        };
        let align = state.store.grid_align();
        let theme = crate::theme::Theme::parse(state.store.tui_theme().as_deref());
        let mut a = App {
            align,
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
            waiting: vec![],
            status: "Ready — not sure? ⌃p searches everything · F1 keymap".into(),
            quit: false,
            panes: crate::panes::Pane::Leaf(0),
            group_active: [None; crate::panes::MAX],
            console_pct: 42,
            value_panel: false,
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
            color: Default::default(),
            theme,
            auto_light: false,
            too_small: false,
            #[cfg(test)]
            fake_db: None,
            #[cfg(test)]
            ran: vec![],
            next_id: 1,
            exec: Default::default(),
            ddl_structure: false,
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
        self.spawn_abortable(fut, apply);
    }

    /// spawn that can be cancelled: abort() drops the future, and `apply` doesn't run
    pub fn spawn_abortable<T: Send + 'static>(&self, fut: impl Future<Output = T> + Send + 'static, apply: impl FnOnce(&mut App, T) + Send + 'static) -> tokio::task::AbortHandle {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let v = fut.await;
            let _ = tx.send(Box::new(move |a: &mut App| apply(a, v)));
        })
        .abort_handle()
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

    pub fn run_action(&mut self, a: Action) {
        #[cfg(test)]
        self.ran.push(a);
        (actions::def(a).run)(self)
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
            || matches!(&self.popup, Some(Popup::ConnForm(f)) if f.testing.is_some())
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

/// The statements' text (the TUI doesn't highlight what runs, so the ranges aren't needed)
fn sqls(v: Vec<qurry_core::exec::Stmt>) -> Vec<String> {
    v.into_iter().map(|s| s.sql).collect()
}

pub fn selected_text(lines: &[String], r0: usize, c0: usize, r1: usize, c1: usize) -> String {
    let full = lines.join("\n");
    let a = byte_offset(lines, r0, c0);
    let b = byte_offset(lines, r1, c1);
    full[a.min(b)..a.max(b)].to_string()
}

