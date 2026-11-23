//! Result grid: cell movement·horizontal/vertical scroll·edit markers (TUI version of DataGrip's data editor)
use qurry_core::db::QueryResult;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use unicode_width::UnicodeWidthStr;

/// Columns have two numberings: the result's (a data index: `col`, `widths`, `edits`, `cut`, `sort`, `col_x`, and every method that
/// takes a column) and the display's (a position in `view.view`, pinned first, hidden ones left out: `col_off`, `max_off`, `shown`).
/// `cols()` counts the result's columns, `visible_cols()` the displayed ones. The cursor is always on a displayed column
pub struct Grid {
    pub result: QueryResult,
    pub row: usize,
    /// The cursor's column (a data index)
    pub col: usize,
    pub row_off: usize,
    /// The first scrollable display position drawn after the pinned ones (at least `view.pinned`)
    pub col_off: usize,
    /// Server cursor session (console) — scrolling to the end loads more
    pub more_session: Option<String>,
    pub loading_more: bool,
    /// Loading more failed, why and how to run it again (App::fetch_more): wrapped on the grid's last lines, no more loads until it runs again
    pub more_error: Option<String>,
    /// Tells grids apart: a load-more answer only goes to the grid it was asked for (a run replaces the grid)
    pub id: u64,
    /// Edits: (row, col) → new value (None = NULL)
    pub edits: BTreeMap<(usize, usize), Option<String>>,
    pub deleted: BTreeSet<usize>,
    /// Number of newly added rows (appended to the end of result.rows)
    pub added: usize,
    /// Each column's width as drawn: the user's (`user_widths`), else the one computed from the header and the first rows
    pub widths: Vec<u16>,
    /// Widths the user set (< > =, or the GUI for a table tab) by column name (`id#2` for a name the result repeats), in cells (qurry_shared::format::ColumnWidths)
    pub user_widths: ColumnWidths,
    /// A column's set width before `=` fitted it (0: none, the computed one), so `=` on the fitted column puts it back (the GUI's double-click)
    before_fit: ColumnWidths,
    /// Fractional-second digits of time columns (aligned within a column, 0 = no alignment)
    pub(crate) frac: Vec<usize>,
    /// Last rendered height (PgUp/PgDn)
    pub page: usize,
    /// Last rendered layout (mouse clicks): body area, (column index, x start, x end) of visible columns
    pub body: Rect,
    pub col_x: Vec<(usize, u16, u16)>,
    /// Horizontal scrollbar (bottom border line). Empty when all columns are visible
    pub hbar: Rect,
    /// Header + separator (wheel here = horizontal scroll)
    pub header: Rect,
    /// First column at which the last column reaches the right edge (don't scroll further)
    pub max_off: usize,
    /// Sort indicator: (column, descending)
    pub sort: Option<(usize, bool)>,
    /// Start cell (row, col) of a range selection. The end is the current cell
    pub anchor: Option<(usize, usize)>,
    /// Rows added to the selection one by one (m: the GUI's ⌘-click on a row number). Moving keeps them, Esc clears them
    pub marked: BTreeSet<usize>,
    /// The columns the last frame showed: first, last, total (1-based; the status line's Shown columns)
    pub shown: Option<(usize, usize, usize)>,
    /// Original position of each row while sorted on screen (restored when sorting is cleared)
    orig: Vec<usize>,
    /// Cells whose value came cut at gridCutChars (QueryResult::cut, moved with their rows): the row holds only the start of the
    /// value until App::fetch_full reads it whole by the table's key
    pub cut: BTreeSet<(usize, usize)>,
    /// A read of whole values is on its way (App::fetch_full)
    pub reading: bool,
    /// The cell under the cursor that a read left cut (its row is gone, or the read failed) and why: the value panel doesn't ask
    /// for it again while the cursor stays on it (cleared when rows move)
    pub unread: Option<((usize, usize), String)>,
    /// Hidden and pinned columns (a table tab's saved one, settings.json "columnLayout"; a console result's until it runs again)
    pub layout: ColumnLayout,
    /// Saving `layout` failed (App::save_layout): a table tab's reload keeps it rather than the saved one
    pub layout_unsaved: bool,
    /// `layout` over this result's columns (set_layout)
    pub view: ColumnView,
}

/// Text placed in a cell `w` columns wide (display width, so wide characters line up too); text wider than the cell is left as is
fn pad(text: String, w: usize, a: Align) -> String {
    let gap = w.saturating_sub(text.width());
    match a {
        // Column never reaches a cell (cell_align resolves it)
        Align::Left | Align::Column => text,
        Align::Right => format!("{}{text}", " ".repeat(gap)),
        Align::Center => format!("{}{text}", " ".repeat(gap / 2)),
    }
}

/// The start of `text` that fits `w` columns (display width)
fn fit(text: String, w: usize) -> String {
    if text.width() <= w {
        return text;
    }
    let mut used = 0;
    text.chars().take_while(|ch| {
        used += unicode_width::UnicodeWidthChar::width(*ch).unwrap_or(0);
        used <= w
    }).collect()
}

/// A JSON cell in colors (qurry_core::format::json_spans, the GUI's tokenizer): `text` is the cell as drawn, `lead` spaces of
/// alignment and then the start of `raw` as Grid::shown writes it (↵ → for newline and tab: one char for one, so pieces line up)
fn json_line(raw: &str, text: &str, lead: usize) -> Line<'static> {
    let chars: Vec<char> = text[lead..].chars().collect();
    let piece = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mut out = vec![Span::raw(text[..lead].to_string())];
    // Byte offsets in `raw` → char indexes, walking forward (the pieces come in order)
    let (mut byte, mut ch, mut at) = (0, 0, 0);
    let mut to_char = |b: usize| {
        ch += raw[byte..b].chars().count();
        byte = b;
        ch
    };
    for (s, e, kind) in json_spans(raw, chars.len()) {
        let (s, e) = (to_char(s), to_char(e));
        if s > at {
            out.push(Span::raw(piece(at, s)));
        }
        out.push(Span::styled(piece(s, e), Style::default().fg(json_color(kind))));
        at = e;
    }
    out.push(Span::raw(piece(at, chars.len())));
    Line::from(out)
}

/// Color of a piece of JSON (GUI --ui-json-*, dark values), in the grid and the value panel
fn json_color(kind: JsonKind) -> Color {
    match kind {
        JsonKind::Key => crate::kit::JSON_KEY,
        JsonKind::String => crate::kit::JSON_STRING,
        JsonKind::Number => crate::kit::NUMBER,
        JsonKind::Keyword => crate::kit::JSON_KEYWORD,
        JsonKind::Punct => crate::kit::JSON_PUNCT,
    }
}

/// Max rows kept in the result grid. Beyond that, export to CSV
pub const MAX_ROWS: usize = 100_000;

pub fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "<null>".into(),
        Value::String(s) => s.replace('\n', "↵").replace('\t', "→"),
        other => other.to_string(),
    }
}
pub(crate) use qurry_core::format::{is_time_type, pad_frac};
use qurry_core::format::{cell_align, cell_class, column_class, column_keys, column_view, column_width, is_json_type, json_spans, text_width, type_class, Align, Alignments, ColumnLayout, ColumnView, ColumnWidths, JsonKind, ValueClass};
use qurry_core::extract::{self, Format, Rows};
use qurry_core::store::DbKind;

pub fn raw_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// Same comparison as the GUI: NULL always last, numeric if both are numbers, otherwise text. desc flips the result, so the NULL side is pre-flipped
fn cmp_cell(a: Option<&Value>, b: Option<&Value>, desc: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let null = |v: Option<&Value>| matches!(v, None | Some(Value::Null));
    match (null(a), null(b)) {
        (true, true) => return Equal,
        (true, false) => return if desc { Less } else { Greater },
        (false, true) => return if desc { Greater } else { Less },
        _ => {}
    }
    let (a, b) = (raw_text(a.unwrap()).unwrap_or_default(), raw_text(b.unwrap()).unwrap_or_default());
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) if !a.is_empty() && !b.is_empty() => x.partial_cmp(&y).unwrap_or(Equal),
        _ => a.cmp(&b),
    }
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Grid {
    pub fn new(result: QueryResult) -> Self {
        let mut g = Grid { result, row: 0, col: 0, row_off: 0, col_off: 0, more_session: None, loading_more: false, more_error: None, id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed), edits: BTreeMap::new(), deleted: BTreeSet::new(), added: 0, widths: vec![], user_widths: ColumnWidths::new(), before_fit: ColumnWidths::new(), frac: vec![], page: 10, body: Rect::default(), col_x: vec![], hbar: Rect::default(), header: Rect::default(), max_off: usize::MAX, sort: None, anchor: None, marked: BTreeSet::new(), shown: None, orig: vec![], cut: BTreeSet::new(), reading: false, unread: None, layout: ColumnLayout::default(), layout_unsaved: false, view: ColumnView { view: vec![], pinned: 0, hidden: 0 } };
        g.set_layout(ColumnLayout::default());
        g.cut = g.result.cut.iter().map(|&[r, c]| (r, c)).collect();
        g.compute_widths();
        g
    }

    pub fn compute_widths(&mut self) {
        let r = &self.result;
        let keys = self.width_keys();
        self.widths = r
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let body = r.rows.iter().take(300).map(|row| row.get(i).map(cell_text).unwrap_or_default().width()).max().unwrap_or(0);
                // +2: room for the sort indicator (▲▼)
                self.user_widths.get(&keys[i]).copied().unwrap_or(((c.name.width() + 2).max(body).clamp(4, 40) + 1) as u16)
            })
            .collect();
        self.compute_frac();
    }

    /// The keys of `user_widths`: each column's name, `id#2` for a name the result repeats (qurry_shared::format::column_keys)
    fn width_keys(&self) -> Vec<String> {
        column_keys(self.result.columns.iter().map(|c| c.name.as_str()))
    }

    /// Widths the user set (a table tab's saved ones, or none for Reset Widths) replace the computed ones by column key
    pub fn set_user_widths(&mut self, w: ColumnWidths) {
        self.user_widths = w;
        self.before_fit.clear();
        self.compute_widths();
    }

    /// Fit: the cells column `c` needs for its header (with the sort mark) and its longest value, over the rows loaded so far
    pub fn fit_width(&self, c: usize) -> u16 {
        let name = self.result.columns[c].name.width() + 2;
        let body = (0..self.rows()).map(|r| self.shown(r, c, usize::MAX).map_or(6, |s| text_width(&s)) + usize::from(self.is_cut(r, c))).max().unwrap_or(0);
        column_width(name.max(body) as i64, 0)
    }

    /// `=`: fit column `c`; on a column fitted to that width, back to the width it had set before (or none: its computed width).
    /// Returns the column's key and its width to save, 0 to forget it (Store::set_column_widths)
    pub fn toggle_fit(&mut self, c: usize) -> (String, u16) {
        let key = self.width_keys().swap_remove(c);
        let fit = self.fit_width(c);
        let set = self.user_widths.get(&key).copied();
        if set != Some(fit) {
            self.before_fit.insert(key, set.unwrap_or(0));
            return self.set_width(c, fit);
        }
        match self.before_fit.remove(&key).unwrap_or(0) {
            0 => {
                self.user_widths.remove(&key);
                self.compute_widths();
                (key, 0)
            }
            w => self.set_width(c, w),
        }
    }

    /// Give column `c` the width `w` (in range); returns the width and its column's key, for saving
    pub fn set_width(&mut self, c: usize, w: u16) -> (String, u16) {
        let key = self.width_keys().swap_remove(c);
        let w = column_width(w.into(), 0);
        self.user_widths.insert(key.clone(), w);
        self.compute_widths();
        (key, w)
    }

    /// Time columns: longest fractional-second digits among loaded values
    fn compute_frac(&mut self) {
        let r = &self.result;
        self.frac = r
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if !is_time_type(&c.type_name) {
                    return 0;
                }
                qurry_core::format::frac_width(r.rows.iter().filter_map(|row| row.get(i)?.as_str()))
            })
            .collect();
    }

    /// Loaded rows go before the added ones (Submit inserts the last `added` rows, Revert drops them): the added rows, their edits,
    /// marks and the cursor on them move down with them
    pub fn append(&mut self, more: QueryResult) {
        let (n, k) = (self.result.rows.len(), more.rows.len());
        let at = n - self.added;
        self.result.rows.splice(at..at, more.rows);
        self.result.truncated = more.truncated;
        self.compute_frac();
        if self.added > 0 {
            let mv = |r: usize| if r >= at { r + k } else { r };
            self.edits = std::mem::take(&mut self.edits).into_iter().map(|((r, c), v)| ((mv(r), c), v)).collect();
            self.cut = std::mem::take(&mut self.cut).into_iter().map(|(r, c)| (mv(r), c)).collect();
            self.unread = None;
            self.marked = std::mem::take(&mut self.marked).into_iter().map(mv).collect();
            self.row = mv(self.row);
            self.anchor = self.anchor.map(|(r, c)| (mv(r), c));
        }
        // The page's cut cells are of its own rows, now after the loaded ones
        self.cut.extend(more.cut.iter().map(|&[r, c]| (r + at, c)));
        if !self.orig.is_empty() {
            // The loaded rows, in the order they came (orig covers them only: added rows stay last, unsorted)
            self.orig.extend(at..at + k);
            self.apply_local_sort();
        }
    }

    /// On-screen sort (loaded rows only): ↑ → ↓ → off. Editable results (table tabs) use server ORDER BY
    pub fn toggle_local_sort(&mut self) {
        if self.cols() == 0 {
            return;
        }
        let col = self.col;
        self.sort = match self.sort {
            Some((c, false)) if c == col => Some((col, true)),
            Some((c, true)) if c == col => None,
            _ => Some((col, false)),
        };
        if self.orig.is_empty() {
            self.orig = (0..self.rows() - self.added).collect();
        }
        self.apply_local_sort();
        if self.sort.is_none() {
            self.orig.clear();
        }
    }

    /// Sorts the loaded rows (orig: where each came in the result); the added rows stay last, where Submit and Revert find them.
    /// Edits and deletes move with their rows
    fn apply_local_sort(&mut self) {
        // Marks are row positions: they would point at other rows once sorted
        self.marked.clear();
        let base = self.rows() - self.added;
        let loaded: Vec<Vec<Value>> = self.result.rows.drain(..base).collect();
        // (place in the result, row now, values)
        let mut rows: Vec<(usize, usize, Vec<Value>)> = std::mem::take(&mut self.orig).into_iter().zip(loaded).enumerate().map(|(now, (o, v))| (o, now, v)).collect();
        match self.sort {
            Some((c, desc)) => rows.sort_by(|a, b| {
                let o = cmp_cell(a.2.get(c), b.2.get(c), desc);
                if desc { o.reverse() } else { o }
            }),
            None => rows.sort_by_key(|r| r.0),
        }
        let mut to = vec![0; base];
        for (i, r) in rows.iter().enumerate() {
            to[r.1] = i;
        }
        let mv = |r: usize| if r < base { to[r] } else { r };
        self.edits = std::mem::take(&mut self.edits).into_iter().map(|((r, c), v)| ((mv(r), c), v)).collect();
        self.cut = std::mem::take(&mut self.cut).into_iter().map(|(r, c)| (mv(r), c)).collect();
        self.unread = None;
        self.deleted = std::mem::take(&mut self.deleted).into_iter().map(mv).collect();
        let (orig, sorted): (Vec<usize>, Vec<Vec<Value>>) = rows.into_iter().map(|(o, _, v)| (o, v)).unzip();
        self.orig = orig;
        self.result.rows.splice(0..0, sorted);
    }

    pub fn rows(&self) -> usize {
        self.result.rows.len()
    }
    pub fn cols(&self) -> usize {
        self.result.columns.len()
    }

    /// Columns on display (pinned ones included, hidden ones not)
    pub fn visible_cols(&self) -> usize {
        self.view.view.len()
    }

    /// Display position of column `c` (None: hidden)
    fn pos(&self, c: usize) -> Option<usize> {
        self.view.view.iter().position(|&v| v == c)
    }

    /// The cursor's display position
    fn cur_pos(&self) -> usize {
        self.pos(self.col).unwrap_or(0)
    }

    /// The columns at display positions `a..=b`, in display order
    fn cols_at(&self, a: usize, b: usize) -> Vec<usize> {
        self.view.view.get(a..=b.min(self.visible_cols().saturating_sub(1))).map_or(vec![], <[usize]>::to_vec)
    }

    /// The first and the last column on display (Home / End)
    pub fn first_col(&self) -> usize {
        self.view.view.first().copied().unwrap_or(0)
    }
    pub fn last_col(&self) -> usize {
        self.view.view.last().copied().unwrap_or(0)
    }

    /// Show the columns as `layout` says (qurry_shared::format::column_view over this result's column keys). A cursor on a column
    /// it hides moves to the one now at its place
    pub fn set_layout(&mut self, layout: ColumnLayout) {
        let at = self.pos(self.col).unwrap_or(0);
        self.layout = layout;
        self.view = column_view(&self.width_keys(), &self.layout);
        // A saved layout that hides every column (the GUI's, or one from columns renamed since) shows them all instead, and is kept
        // so (later hides start from what is shown)
        if self.view.view.is_empty() {
            self.layout.hidden.clear();
            self.view = column_view(&self.width_keys(), &self.layout);
        }
        if self.pos(self.col).is_none() && !self.view.view.is_empty() {
            self.col = self.view.view[at.min(self.visible_cols() - 1)];
        }
        if self.anchor.is_some_and(|(_, c)| self.pos(c).is_none()) {
            self.anchor = None;
        }
    }

    pub fn is_pinned(&self, c: usize) -> bool {
        self.pos(c).is_some_and(|p| p < self.view.pinned)
    }

    /// Column `c` shown or hidden; the last shown column stays (false: refused)
    pub fn set_hidden(&mut self, c: usize, hide: bool) -> bool {
        if hide && self.pos(c).is_some() && self.visible_cols() <= 1 {
            return false;
        }
        let key = self.width_keys().swap_remove(c);
        let mut l = self.layout.clone();
        l.hidden.retain(|k| *k != key);
        l.pinned.retain(|k| !(hide && *k == key));
        if hide {
            l.hidden.push(key);
        }
        self.set_layout(l);
        true
    }

    /// Column `c` pinned at the left (after the ones pinned before it) or not
    pub fn set_pinned(&mut self, c: usize, pin: bool) {
        let key = self.width_keys().swap_remove(c);
        let mut l = self.layout.clone();
        l.pinned.retain(|k| *k != key);
        if pin {
            l.hidden.retain(|k| *k != key);
            l.pinned.push(key);
        }
        self.set_layout(l);
    }

    /// The columns Hide Column and Pin Column act on (#216, the GUI's selected columns): the selection's, in grid order, else the cursor's
    pub fn selected_cols(&self) -> Vec<usize> {
        self.selection().map_or_else(|| vec![self.col], |(_, cols)| cols)
    }

    /// Show All: every column on display again, the pinned ones still pinned
    pub fn show_all(&mut self) {
        let l = ColumnLayout { hidden: vec![], pinned: self.layout.pinned.clone() };
        self.set_layout(l);
    }

    /// Value with edits applied
    pub fn value(&self, r: usize, c: usize) -> Option<String> {
        if let Some(v) = self.edits.get(&(r, c)) {
            return v.clone();
        }
        self.result.rows.get(r).and_then(|row| row.get(c)).and_then(raw_text)
    }

    /// Cell (r, c) holds only the start of its value: cut, and not changed (a change is a whole value: typed on the whole value,
    /// pasted, NULL)
    pub fn is_cut(&self, r: usize, c: usize) -> bool {
        self.cut.contains(&(r, c)) && !self.edits.contains_key(&(r, c))
    }

    /// The cut cells `y` copies: the selection's, else the current cell's
    pub fn cut_in_copy(&self) -> Vec<(usize, usize)> {
        match self.selection() {
            None => vec![(self.row, self.col)],
            Some((rows, cols)) => rows.iter().flat_map(|&r| cols.iter().map(move |&c| (r, c))).filter(|&(r, c)| self.selected(r, c)).collect(),
        }
        .into_iter()
        .filter(|&(r, c)| self.is_cut(r, c))
        .collect()
    }

    /// The cut cells `in_list` lists
    pub fn cut_in_list(&self) -> Vec<(usize, usize)> {
        let cells: Vec<(usize, usize)> = if !self.marked.is_empty() {
            self.marked.iter().map(|&r| (r, self.col)).collect()
        } else {
            let p = self.cur_pos();
            let (r0, p0, r1, p1) = self.sel_box().unwrap_or((0, p, self.rows().saturating_sub(1), p));
            let cols = self.cols_at(p0, p1);
            (r0..=r1).flat_map(|r| cols.iter().map(move |&c| (r, c))).collect()
        };
        cells.into_iter().filter(|&(r, c)| self.is_cut(r, c)).collect()
    }

    /// One cell with edits applied, as the shared IN list reads it (an edit is text; NULL is Null)
    fn cell(&self, r: usize, c: usize) -> Value {
        match self.edits.get(&(r, c)) {
            Some(v) => v.clone().map_or(Value::Null, Value::String),
            None => self.result.rows.get(r).and_then(|row| row.get(c)).cloned().unwrap_or(Value::Null),
        }
    }

    /// Rows × columns as IN-list text and value count. One column gives ('a', 'b'), several give row tuples (('a', 1), …). Duplicates·NULL
    /// are dropped. Numeric columns keep number text bare (numeric·bigint arrive as text); NaN and the like are quoted. Strings as `kind` reads them
    fn in_list_rows(&self, rows: impl Iterator<Item = usize>, cols: &[usize], kind: DbKind) -> (String, usize) {
        let rows: Vec<Vec<Value>> = rows.filter(|&r| r < self.rows()).map(|r| cols.iter().map(|&c| self.cell(r, c)).collect()).collect();
        let types: Vec<String> = cols.iter().map(|&c| self.result.columns.get(c).map(|x| x.type_name.clone()).unwrap_or_default()).collect();
        extract::in_list(&rows, &types, kind, extract::Options::default())
    }

    /// Whole column as an IN list
    pub fn column_in_list(&self, c: usize, kind: DbKind) -> (String, usize) {
        self.in_list_rows(0..self.rows(), &[c], kind)
    }

    /// The current column of the marked rows (in grid order), else the selected range, else the whole current column, as an IN list
    /// (the GUI's Copy as IN List of ⌘-clicked rows lists only their cells)
    pub fn in_list(&self, kind: DbKind) -> (String, usize) {
        if !self.marked.is_empty() {
            return self.in_list_rows(self.marked.iter().copied(), &[self.col], kind);
        }
        match self.sel_box() {
            Some((r0, p0, r1, p1)) => self.in_list_rows(r0..=r1, &self.cols_at(p0, p1), kind),
            None => self.column_in_list(self.col, kind),
        }
    }

    /// Selected range (first row, first display position, last row, last display position). Selected with Shift+arrows·dragging
    fn sel_box(&self) -> Option<(usize, usize, usize, usize)> {
        let (ar, ac) = self.anchor?;
        let (a, p) = (self.pos(ac)?, self.cur_pos());
        Some((ar.min(self.row), a.min(p), ar.max(self.row), a.max(p)))
    }

    /// The selected rows and columns (in grid order) the status line's items count: the marked rows (and a range's rows) in the
    /// cursor's column, else the range, else the cursor's cell
    pub fn status_cells(&self) -> (Vec<usize>, Vec<usize>) {
        let rect = self.sel_box();
        if !self.marked.is_empty() {
            let mut rows = self.marked.clone();
            if let Some((r0, _, r1, _)) = rect {
                rows.extend(r0..=r1);
            }
            return (rows.into_iter().filter(|&r| r < self.rows()).collect(), vec![self.col]);
        }
        match rect {
            Some((r0, p0, r1, p1)) => ((r0..=r1).collect(), self.cols_at(p0, p1)),
            None => (vec![self.row], vec![self.col]),
        }
    }

    /// Extend the range (starting from the current cell the first time)
    pub fn extend(&mut self, dr: isize, dc: isize) {
        let anchor = self.anchor.unwrap_or((self.row, self.col));
        self.move_by(dr, dc);
        self.anchor = Some(anchor);
    }

    /// m: the current row into the marked rows, or out of them
    pub fn toggle_mark(&mut self) {
        if !self.marked.remove(&self.row) && self.row < self.rows() {
            self.marked.insert(self.row);
        }
    }

    /// Whether (r, c) is selected: in the range, or in a marked row
    fn selected(&self, r: usize, c: usize) -> bool {
        let p = self.pos(c);
        p.is_some() && (self.marked.contains(&r) || self.sel_box().zip(p).is_some_and(|((r0, p0, r1, p1), p)| r >= r0 && r <= r1 && p >= p0 && p <= p1))
    }

    /// Rows and columns the selection (marked rows and the range) touches, in grid order. None when nothing is selected
    pub fn selection(&self) -> Option<(Vec<usize>, Vec<usize>)> {
        let rect = self.sel_box();
        if rect.is_none() && self.marked.is_empty() {
            return None;
        }
        let mut rows = self.marked.clone();
        let cols: Vec<usize> = match rect {
            Some((r0, p0, r1, p1)) if self.marked.is_empty() => {
                rows.extend(r0..=r1);
                self.cols_at(p0, p1)
            }
            Some((r0, _, r1, _)) => {
                rows.extend(r0..=r1);
                self.view.view.clone()
            }
            None => self.view.view.clone(),
        };
        Some((rows.into_iter().collect(), cols))
    }

    /// The selection as TSV (values only, NULL as empty): pastes into a sheet as cells. Same rule as the GUI's copy (gridSelection.ts
    /// copyTable): rows and columns in grid order, the union of their columns, a cell outside the selection empty. Nothing selected: the current cell.
    /// One cell is its value as it is (NULL empty), as the GUI copies one cell: it pastes back as one value
    pub fn selection_tsv(&self) -> String {
        let one = |r: usize, c: usize| self.value(r, c).unwrap_or_default();
        let Some((rows, cols)) = self.selection() else { return one(self.row, self.col) };
        if let ([r], [c]) = (&rows[..], &cols[..]) {
            return one(*r, *c);
        }
        let cell = |r: usize, c: usize| if self.selected(r, c) { self.value(r, c).map_or(Value::Null, Value::String) } else { Value::Null };
        let vals = rows.iter().map(|&r| cols.iter().map(|&c| cell(r, c)).collect()).collect();
        extract::rows(Format::Tsv, &Rows { vals, ..Default::default() }, DbKind::Postgres, extract::Options::default())
    }

    /// d: the selected rows (marked rows and the range's), or else the current row, deleted; when all of them are deleted already, back.
    /// Added rows go away instead
    pub fn delete_rows(&mut self) {
        let rows = self.selection().map_or(vec![self.row], |(rows, _)| rows);
        let base = self.rows() - self.added;
        let (new, old): (Vec<usize>, Vec<usize>) = rows.into_iter().filter(|&r| r < self.rows()).partition(|&r| r >= base);
        if old.iter().all(|r| self.deleted.contains(r)) {
            for r in &old {
                self.deleted.remove(r);
            }
        } else {
            self.deleted.extend(old);
        }
        for &r in new.iter().rev() {
            self.result.rows.remove(r);
            self.added -= 1;
            // Edits (and cut cells) of the added rows below move up with them
            let up = |er: usize| if er > r { er - 1 } else { er };
            self.edits = std::mem::take(&mut self.edits).into_iter().filter(|((er, _), _)| *er != r).map(|((er, c), v)| ((up(er), c), v)).collect();
            self.cut = std::mem::take(&mut self.cut).into_iter().filter(|(er, _)| *er != r).map(|(er, c)| (up(er), c)).collect();
            self.unread = None;
        }
        if !new.is_empty() {
            self.marked.retain(|&r| r < base);
            self.anchor = None;
            self.row = self.row.min(self.rows().saturating_sub(1));
        }
    }

    pub fn dirty(&self) -> bool {
        !self.edits.is_empty() || !self.deleted.is_empty() || self.added > 0
    }

    /// Pending edits: changed cells, deleted rows, added rows (0 when clean)
    pub fn pending(&self) -> usize {
        if self.dirty() { self.edits.len() + self.deleted.len() + self.added } else { 0 }
    }

    pub fn revert(&mut self) {
        self.edits.clear();
        self.deleted.clear();
        let n = self.result.rows.len() - self.added;
        self.marked.retain(|&r| r < n);
        self.result.rows.truncate(n);
        self.added = 0;
        self.row = self.row.min(n.saturating_sub(1));
    }

    pub fn add_row(&mut self) {
        self.result.rows.push(vec![Value::Null; self.cols()]);
        self.added += 1;
        self.row = self.rows() - 1;
    }

    /// Change a cell value (clears the edit if equal to the original)
    pub fn set_value(&mut self, r: usize, c: usize, v: String) {
        self.set_cell(r, c, Some(v));
    }

    /// Change a cell to a value or NULL (None); clears the edit if equal to the original. An added row keeps a NULL: Submit sends
    /// it, where an untouched cell is left out (its default)
    fn set_cell(&mut self, r: usize, c: usize, v: Option<String>) {
        let orig = self.result.rows.get(r).and_then(|row| row.get(c)).and_then(raw_text);
        if orig == v && !self.is_added(r) {
            self.edits.remove(&(r, c));
        } else {
            self.edits.insert((r, c), v);
        }
    }

    /// The cells a paste of `text` writes from the current cell (rows past the last one would be added)
    pub fn paste_spots(&self, text: &str) -> Vec<(usize, usize)> {
        let rows = extract::paste_cells(text, extract::PasteFormat::Auto);
        let cols = self.paste_cols(&rows);
        rows.iter().enumerate().flat_map(|(i, cells)| cols.iter().take(cells.len()).map(move |&c| (self.row + i, c))).collect()
    }

    /// The columns a paste of `rows` fills: from the cursor's to the right in display order, hidden ones skipped
    fn paste_cols<T>(&self, rows: &[Vec<T>]) -> Vec<usize> {
        let p = self.cur_pos();
        self.cols_at(p, p + rows.iter().map(Vec::len).max().unwrap_or(0).saturating_sub(1))
    }

    /// Paste: fill from the current cell (paste_cols), `text` read as TSV or CSV (extract::paste_cells, as the GUI reads it; an empty
    /// field is NULL). Missing rows are added, overflowing columns dropped
    pub fn paste(&mut self, text: &str) {
        let (row, col) = (self.row, self.col);
        let rows = extract::paste_cells(text, extract::PasteFormat::Auto);
        let cols = self.paste_cols(&rows);
        for (i, cells) in rows.into_iter().enumerate() {
            let r = row + i;
            while r >= self.rows() {
                self.add_row();
            }
            for (&c, v) in cols.iter().zip(cells) {
                self.set_cell(r, c, v);
            }
        }
        (self.row, self.col) = (row, col);
    }

    pub fn is_added(&self, r: usize) -> bool {
        r >= self.rows() - self.added
    }

    /// Screen coordinates → (row, col)
    pub fn hit(&self, x: u16, y: u16) -> Option<(usize, Option<usize>)> {
        let b = self.body;
        if x < b.x || x >= b.x + b.width || y < b.y || y >= b.y + b.height {
            return None;
        }
        let r = self.row_off + (y - b.y) as usize;
        if r >= self.rows() {
            return None;
        }
        Some((r, self.col_x.iter().find(|(_, x0, x1)| x >= *x0 && x <= *x1).map(|c| c.0)))
    }

    pub fn move_by(&mut self, dr: isize, dc: isize) {
        self.anchor = None;
        let rows = self.rows() as isize;
        let cols = self.visible_cols() as isize;
        if rows > 0 {
            self.row = (self.row as isize + dr).clamp(0, rows - 1) as usize;
        }
        if cols > 0 {
            self.col = self.view.view[(self.cur_pos() as isize + dc).clamp(0, cols - 1) as usize];
        }
    }

    /// Horizontal scroll (trackpad·Shift+wheel): move the first scrollable column and the selection with it (pinned ones stay)
    pub fn hscroll(&mut self, d: isize) {
        let (n, pinned) = (self.visible_cols(), self.view.pinned);
        if n <= pinned {
            return;
        }
        self.col_off = (self.col_off.max(pinned) as isize + d).clamp(pinned as isize, self.max_off.clamp(pinned, n - 1) as isize) as usize;
        self.col = self.view.view[self.col_off];
    }

    /// Jump to the clicked·dragged scrollbar position (the track covers the scrollable columns)
    pub fn hscroll_to(&mut self, x: u16) {
        let (b, n, pinned) = (self.hbar, self.visible_cols(), self.view.pinned);
        if b.width == 0 || n <= pinned {
            return;
        }
        let p = (pinned + x.saturating_sub(b.x) as usize * (n - pinned) / b.width as usize).min(self.max_off).min(n - 1);
        self.col_off = p;
        self.col = self.view.view[p];
    }

    /// When near the end, time to load more (only up to MAX_ROWS: beyond that, export to CSV)
    pub fn wants_more(&self) -> bool {
        self.result.truncated && self.more_session.is_some() && !self.loading_more && self.more_error.is_none() && self.row + 5 >= self.rows() && self.rows() < MAX_ROWS
    }

    /// Text of a cell to draw: built only up to the width (max chars) (so large text·jsonb isn't copied whole every frame)
    fn shown(&self, r: usize, c: usize, max: usize) -> Option<String> {
        let clip = |s: &str| -> String { s.chars().take(max).map(|ch| match ch { '\n' => '↵', '\t' => '→', ch => ch }).collect() };
        if let Some(v) = self.edits.get(&(r, c)) {
            return v.as_deref().map(clip);
        }
        match self.result.rows.get(r)?.get(c)? {
            Value::Null => None,
            Value::String(s) if self.frac.get(c).is_some_and(|&w| w > 0) => Some(clip(&pad_frac(s, self.frac[c]))),
            Value::String(s) => Some(clip(s)),
            other => Some(clip(&other.to_string())),
        }
    }

    /// Value class of a cell with edits applied, `column` = its column's type_class (the shared rule reads only the value's kind, so an edit is any text)
    fn class(&self, r: usize, c: usize, column: ValueClass) -> ValueClass {
        match self.edits.get(&(r, c)) {
            Some(e) => cell_class(column, &e.as_ref().map_or(Value::Null, |_| Value::String(String::new()))),
            None => cell_class(column, self.result.rows.get(r).and_then(|row| row.get(c)).unwrap_or(&Value::Null)),
        }
    }

    /// Horizontal scroll in `avail` cells: the pinned columns first, always, then the scrollable ones from `col_off`, keeping the
    /// cursor's column in view. Returns the drawn columns, their widths (a last one that doesn't fit is cut to the rest), how many
    /// of them are whole and how many are pinned ones
    fn place_cols(&mut self, avail: u16) -> (Vec<usize>, Vec<u16>, usize, usize) {
        let (n, pinned) = (self.visible_cols(), self.view.pinned);
        let dw: Vec<u16> = self.view.view.iter().map(|&c| self.widths[c]).collect();
        let room = avail.saturating_sub(dw[..pinned].iter().map(|w| w + 1).sum());
        let cur = self.cur_pos();
        // Pinned columns that leave no room for the cursor's (whole, or at least half the width) scroll away with the others while it
        // is there, as DataGrip's frozen area does: the cursor's column is always drawn
        let pins = if cur >= pinned && room <= dw[cur].min((avail / 2).max(3)) { 0 } else { pinned };
        // So the right side isn't empty when scrolled to the end: don't scroll past the first column at which the last column reaches the right edge
        self.max_off = {
            let mut off = n;
            let mut w = 0u16;
            while off > pinned && w + dw[off - 1] < room {
                off -= 1;
                w += dw[off] + 1;
            }
            off.min(n - 1).max(pinned)
        };
        self.col_off = self.col_off.clamp(pinned, self.max_off);
        if cur >= pinned {
            let room = if pins == 0 { avail } else { room };
            self.col_off = self.col_off.min(cur);
            while self.col_off < cur && dw[self.col_off..=cur].iter().map(|w| w + 1).sum::<u16>() > room {
                self.col_off += 1;
            }
        }
        let (mut cols, mut shown_w, mut used, mut full) = (vec![], vec![], 0u16, 0usize);
        for p in (0..pins).chain(self.col_off..n) {
            let (c, w) = (self.view.view[p], dw[p]);
            if used + w + 1 > avail && !cols.is_empty() {
                let rest = avail.saturating_sub(used + 1);
                if rest >= 3 {
                    cols.push(c);
                    shown_w.push(rest);
                }
                break;
            }
            used += w + 1;
            cols.push(c);
            shown_w.push(w);
            full += 1;
        }
        (cols, shown_w, full, pins)
    }

    /// `aligns` = App::align (settings.json "gridAlign", shared with the GUI)
    pub fn render(&mut self, f: &mut Frame, area: Rect, title: String, focused: bool, color: crate::theme::ColorMode, aligns: &Alignments) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            // Thick when focused, like kit::panel: focus shows without colors too (NO_COLOR)
            .border_type(if focused { ratatui::widgets::BorderType::Thick } else { ratatui::widgets::BorderType::Plain })
            .border_style(if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) });
        let mut inner = block.inner(area);
        f.render_widget(block, area);
        // Why no more rows came, in the grid so it stays in sight (the status line is overwritten by the next message): wrapped on
        // its last lines, up to half of them, so the reason and how to run it again show at usual widths
        if let Some(e) = &self.more_error
            && inner.height > 3
        {
            let lines = ratatui::text::Text::styled(format!("✗ {e}"), Style::default().fg(crate::kit::DANGER)).lines;
            let h = crate::kit::wrapped_height(&lines, inner.width).clamp(1, inner.height / 2);
            inner.height -= h;
            f.render_widget(ratatui::widgets::Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), Rect { y: inner.y + inner.height, height: h, ..inner });
        }
        if inner.height < 2 || self.visible_cols() == 0 {
            return;
        }
        // Body below the header (rows are separated by underlines, not separate lines)
        let body_h = (inner.height - 1) as usize;
        self.page = body_h.max(1);
        // Vertical scroll
        if self.row < self.row_off {
            self.row_off = self.row;
        } else if self.row >= self.row_off + body_h {
            self.row_off = self.row + 1 - body_h;
        }
        let num_w = (self.rows().max(1).to_string().len() + 1) as u16;
        let (cols, shown_w, full, pins) = self.place_cols(inner.width.saturating_sub(num_w + 1));
        // Record positions for click hit-testing
        self.body = Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: inner.height - 1 };
        self.header = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
        self.col_x.clear();
        let mut x = inner.x + num_w + 1;
        for (&c, &w) in cols.iter().zip(&shown_w) {
            self.col_x.push((c, x, x + w));
            x += w + 1;
        }
        let header = Row::new(
            std::iter::once(Cell::from("")).chain(cols.iter().map(|&c| {
                let col = &self.result.columns[c];
                let st = if c == self.col { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().add_modifier(Modifier::BOLD) };
                let mark = match self.sort {
                    Some((sc, desc)) if sc == c => if desc { " ▼" } else { " ▲" },
                    _ => "",
                };
                Cell::from(format!("{}{mark}", col.name)).style(st)
            })),
        )
        .style(Style::default().bg(crate::kit::SURFACE));
        let tclass: Vec<ValueClass> = cols.iter().map(|&c| type_class(&self.result.columns[c].type_name)).collect();
        // What a NULL on "Column type" follows (an untyped column goes by its values)
        let nclass: Vec<ValueClass> = cols.iter().map(|&c| column_class(&self.result.columns[c].type_name, self.result.rows.iter().filter_map(|row| row.get(c)))).collect();
        let tjson: Vec<bool> = cols.iter().map(|&c| is_json_type(&self.result.columns[c].type_name)).collect();
        let sel = self.sel_box();
        let end = (self.row_off + body_h).min(self.rows());
        let rows: Vec<Row> = (self.row_off..end)
            .map(|r| {
                let deleted = self.deleted.contains(&r);
                let added = self.is_added(r);
                let edited_row = self.edits.range((r, 0)..(r + 1, 0)).next().is_some();
                // Pending changes are marked next to the row number too, so they show without colors (NO_COLOR): + added · - deleted · ~ edited
                let (mark, num_c) = if added {
                    ('+', crate::kit::OK)
                } else if deleted {
                    ('-', crate::kit::DANGER)
                } else if edited_row {
                    ('~', Color::Yellow)
                } else {
                    (' ', Color::DarkGray)
                };
                // A marked row's number in reverse video (shows without colors too)
                let num_st = Style::default().fg(num_c);
                let num = Cell::from(format!("{:>w$}{mark}", r + 1, w = num_w as usize - 1)).style(if self.marked.contains(&r) { num_st.add_modifier(Modifier::REVERSED) } else { num_st });
                Row::new(std::iter::once(num).chain(cols.iter().zip(&shown_w).zip(&tclass).zip(&nclass).zip(&tjson).map(|((((&c, &w), &tc), &nc), &json)| {
                    let edited = self.edits.contains_key(&(r, c));
                    let v = self.shown(r, c, self.widths[c] as usize + 1);
                    let text = match &v {
                        None if added && !edited => "<default>".to_string(),
                        None => "<null>".to_string(),
                        Some(s) => s.clone(),
                    };
                    let class = self.class(r, c, tc);
                    // A cut value ends with a dim … (the GUI's mark) right after what fits of its text
                    let cut = self.is_cut(r, c);
                    let w = w as usize - usize::from(cut);
                    let text = if cut { fit(text, w) } else { text };
                    let shown_len = text.len();
                    let text = pad(text, w, cell_align(nc, class, aligns));
                    // JSON in colors unless the row's changes color the cell (edited · deleted · added)
                    let raw = self.result.rows.get(r).and_then(|row| row.get(c)).and_then(Value::as_str).filter(|_| json && class == ValueClass::JsonBinary && !edited && !deleted && !added);
                    let mut line = match raw {
                        Some(raw) => json_line(raw, &text, text.len() - shown_len),
                        None => Line::from(text),
                    };
                    if cut {
                        line.push_span(Span::styled("…", Style::default().fg(crate::kit::DIM)));
                    }
                    let mut st = match class {
                        ValueClass::Null => Style::default().fg(crate::kit::NULL_TEXT).add_modifier(Modifier::ITALIC),
                        ValueClass::Number => Style::default().fg(crate::kit::NUMBER),
                        _ => Style::default(),
                    };
                    if edited {
                        st = st.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                    }
                    if deleted {
                        st = st.fg(crate::kit::DANGER).add_modifier(Modifier::CROSSED_OUT);
                    }
                    if added {
                        st = st.fg(crate::kit::OK);
                    }
                    let in_sel = self.selected(r, c);
                    let cursor = r == self.row && c == self.col;
                    // Where the selection and the cursor are both reverse video, the cursor is the one cell of it left plain (like a spreadsheet)
                    let hole = cursor && in_sel && color.reverses_selection() && (!self.marked.is_empty() || sel.is_some_and(|(r0, c0, r1, c1)| r0 != r1 || c0 != c1));
                    if cursor && !hole {
                        st = st.bg(crate::kit::selected(focused));
                    } else if in_sel && !cursor {
                        st = st.bg(crate::kit::CELL_EDITED);
                    } else if r == self.row {
                        st = st.bg(crate::kit::CELL_RANGE);
                    }
                    Cell::from(line).style(st)
                })))
            })
            .collect();
        let widths: Vec<Constraint> = std::iter::once(Constraint::Length(num_w)).chain(shown_w.iter().map(|&w| Constraint::Length(w))).collect();
        let shown_rows = (end - self.row_off) as u16;
        f.render_widget(Table::new(rows, widths).header(header).column_spacing(1), inner);
        // Separators: vertical lines between columns (in the one-cell gap), dim underlines between header·rows.
        // Drawing horizontal rules as separate lines halves the visible rows, and some terminals (Orca etc.) don't show ─
        let line_c = crate::kit::GRID_LINE;
        // No underline on vertical-line cells (where │ and underline overlap, some terminals show a bump at the crossing and misalign the height)
        let line = Style::default().fg(line_c).remove_modifier(Modifier::UNDERLINED);
        let under = Style::default().add_modifier(Modifier::UNDERLINED).underline_color(line_c);
        let buf = f.buffer_mut();
        // No line after a cut last column (it overlaps the border)
        let seps: Vec<u16> = std::iter::once(inner.x + num_w).chain(self.col_x.iter().take(full).map(|c| c.2)).filter(|x| *x < inner.right()).collect();
        // Underline runs to the end of the last column (including a cut one)
        let last = self.col_x.last().map(|c| c.2).max(seps.last().map(|x| x + 1)).unwrap_or(inner.x);
        // The line after the pinned columns is double: what stays when scrolling (shows in NO_COLOR too; not the focused border's ┃)
        let pin_edge = pins.checked_sub(1).filter(|&p| p < full).map(|p| self.col_x[p].2);
        let rows_end = (inner.y + 1 + shown_rows).min(inner.bottom());
        for y in inner.y..rows_end {
            for x in inner.x..inner.right().min(last) {
                buf[(x, y)].set_style(under);
            }
            for &x in &seps {
                buf[(x, y)].set_symbol(if Some(x) == pin_edge { "║" } else { "│" }).set_style(line);
            }
        }
        // Horizontal scrollbar on the bottom border (GUI: "cols 3–8 of 14" in the status bar): the border line itself is the bar, a dim
        // thin track with a heavier bold thumb over the shown columns (the same focused or not, so it shows in NO_COLOR too), then the
        // shown range at its right end. Click or drag the track to move
        // In display positions over the shown columns (hidden ones not counted); the pinned ones count as shown from 1 until scrolled
        self.hbar = Rect::default();
        let (n, pinned) = (self.visible_cols(), self.view.pinned);
        let scrolled = full.saturating_sub(pins).max(1);
        let first = if self.col_off == pinned && pins == pinned { 1 } else { self.col_off + 1 };
        let last = (self.col_off + scrolled).min(n);
        self.shown = Some((first, last, n));
        if let Some(text) = qurry_shared::format::shown_cols(first, last, n) {
            let accent = if focused { Color::Cyan } else { Color::Gray };
            let label = format!(" ◀ {text} ▶ ");
            let room = inner.width;
            // A narrow grid keeps the track and drops the range
            let label_w = if room >= label.width() as u16 + 10 { label.width() as u16 } else { 0 };
            let track = Rect { x: inner.x, y: area.bottom().saturating_sub(1), width: room - label_w, height: 1 };
            // The track covers the scrollable columns
            let n = (n - pinned).max(1) as u32;
            let tw = track.width as u32;
            let start = ((self.col_off - pinned) as u32 * tw / n) as u16;
            let len = ((scrolled as u32 * tw).div_ceil(n) as u16).clamp(2, track.width - start);
            for x in track.left()..track.right() {
                let on = (track.x + start..track.x + start + len).contains(&x);
                let st = if on { Style::default().fg(accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::kit::DIM) };
                buf[(x, track.y)].set_symbol(if on { "━" } else { "─" }).set_style(st);
            }
            if label_w > 0 {
                buf.set_string(track.right(), track.y, &label, Style::default().fg(accent));
            }
            self.hbar = track;
        }
    }

    /// The value panel (v) in `area` beside the grid: the current cell's whole value, follows the cursor. JSON (a json / jsonb column, or
    /// a JSON object or array) pretty-printed and coloured as the GUI's Value Editor shows it (qurry_shared::value). `close` = its keys.
    /// `note`: a cut value's line above its start (App::cut_note: loading the whole value, or why it can't be read)
    pub fn render_value_panel(&self, f: &mut Frame, area: Rect, close: String, note: Option<String>) {
        let col = self.result.columns.get(self.col);
        let title = col.map(|c| format!(" {} : {} ", c.name, c.type_name.to_lowercase())).unwrap_or_else(|| " Value ".into());
        let block = Block::default().borders(Borders::ALL).title(title).title_bottom(Line::styled(close, Style::default().fg(crate::kit::DIM))).border_style(Style::default().fg(Color::DarkGray));
        let mut lines: Vec<Line> = match (col, self.value(self.row, self.col)) {
            (None, _) => vec![],
            (Some(_), None) => vec![Line::styled("<null>", Style::default().fg(crate::kit::NULL_TEXT).add_modifier(Modifier::ITALIC))],
            // Only the start of the value: as it came (no JSON to pretty-print), ending with the grid's …
            (Some(_), Some(v)) if self.is_cut(self.row, self.col) => {
                let mut l: Vec<Line> = v.lines().map(|l| Line::raw(l.to_string())).collect();
                if let Some(last) = l.last_mut() {
                    last.push_span(Span::styled("…", Style::default().fg(crate::kit::DIM)));
                }
                l
            }
            (Some(c), Some(v)) => {
                let p = qurry_shared::value::panel_text(&c.type_name, &v);
                if p.json { json_lines(&p.text) } else { p.text.lines().map(|l| Line::raw(l.to_string())).collect() }
            }
        };
        if let Some(n) = note {
            lines.splice(0..0, [Line::styled(n, Style::default().fg(crate::kit::WARN)), Line::raw("")]);
        }
        f.render_widget(ratatui::widgets::Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }).block(block), area);
    }
}

/// Pretty-printed JSON in colors, line by line (the value panel): one json_spans pass over the whole text, as the grid's
/// tokenizer reads it, so a key is a key whichever line it is on
fn json_lines(text: &str) -> Vec<Line<'static>> {
    let spans = json_spans(text, usize::MAX);
    let (mut i, mut start, mut out) = (0, 0, vec![]);
    for line in text.split('\n') {
        let end = start + line.len();
        let (mut parts, mut at) = (vec![], start);
        // Pieces on this line; one that goes on past it (an unclosed string) is cut here and continues on the next
        while let Some(&(s, e, kind)) = spans.get(i).filter(|p| p.0 < end) {
            let (s, cut) = (s.max(start), e.min(end));
            if s > at {
                parts.push(Span::raw(text[at..s].to_string()));
            }
            parts.push(Span::styled(text[s..cut].to_string(), Style::default().fg(json_color(kind))));
            at = cut;
            if e > end {
                break;
            }
            i += 1;
        }
        if at < end {
            parts.push(Span::raw(text[at..end].to_string()));
        }
        out.push(Line::from(parts));
        start = end + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Time columns align fractional-second digits within the column (values unchanged)
    #[test]
    fn time_fraction_is_padded_per_column() {
        assert_eq!(pad_frac("2026-06-06 03:19:15 +09:00", 6), "2026-06-06 03:19:15.000000 +09:00");
        assert_eq!(pad_frac("2026-06-08 10:52:05.27 +09:00", 6), "2026-06-08 10:52:05.270000 +09:00");
        assert_eq!(pad_frac("12:00:01", 3), "12:00:01.000");
        assert_eq!(pad_frac("2026-06-08T10:52:05.123456", 6), "2026-06-08T10:52:05.123456");
        assert_eq!(pad_frac("hello", 6), "hello");
        let col = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("2026-06-06 03:19:15 +09:00"), json!("03:19:15")], vec![json!("2026-06-08 09:38:06.521101 +09:00"), json!("09:38:06.5")], vec![Value::Null, Value::Null]];
        let g = Grid::new(QueryResult { columns: vec![col("at", "TIMESTAMPTZ"), col("note", "TEXT")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        assert_eq!(g.shown(0, 0, 100).unwrap(), "2026-06-06 03:19:15.000000 +09:00");
        assert_eq!(g.shown(1, 0, 100).unwrap(), "2026-06-08 09:38:06.521101 +09:00");
        assert_eq!(g.shown(2, 0, 100), None);
        assert_eq!(g.shown(0, 1, 100).unwrap(), "03:19:15", "not a time column");
        assert_eq!(g.result.rows[0][0], json!("2026-06-06 03:19:15 +09:00"), "value unchanged");
    }

    /// JSON columns color keys, strings, numbers, keywords and punctuation (the GUI's tokenizer); binary columns of the same
    /// value class and changed cells don't; a newline drawn as ↵ keeps the pieces in line; NO_COLOR has no colors
    #[test]
    fn json_cells_are_colored() {
        use crate::kit::{JSON_KEY, JSON_KEYWORD, JSON_PUNCT, JSON_STRING, NUMBER};
        use crate::theme::ColorMode;
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![
            vec![json!("{\"이름\":\"김\",\"n\":-1,\"ok\":true}"), json!("0x7B22"), json!("{\"e\":1}")],
            vec![json!("{\n\"a\":[null]}"), Value::Null, json!("{\"e\":1}")],
        ];
        let mut g = Grid::new(QueryResult { columns: vec![ty("meta", "JSONB"), ty("bin", "BYTEA"), ty("edited", "JSON")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        g.set_value(0, 2, "{\"e\":2}".into());
        (g.row, g.col) = (1, 1);
        let draw = |g: &mut Grid, mode: ColorMode| {
            let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 6)).unwrap();
            term.draw(|f| g.render(f, f.area(), "r".into(), true, mode, &Alignments::new())).unwrap();
            let mut buf = term.backend().buffer().clone();
            mode.apply(&mut buf, &crate::theme::Look::dark());
            buf
        };
        // Colors of the cells from the first `from` on row y, one per symbol (the second half of a wide char skipped)
        let colors = |buf: &ratatui::buffer::Buffer, y: u16, from: &str, n: usize| -> Vec<(String, Color)> {
            let mut row = vec![];
            let mut x = 0;
            while x < 80 {
                row.push(&buf[(x, y)]);
                x += buf[(x, y)].symbol().width().max(1) as u16;
            }
            let at = row.iter().position(|c| c.symbol() == from).unwrap_or_else(|| panic!("no {from} on row {y}"));
            row[at..at + n].iter().map(|c| (c.symbol().to_string(), c.fg)).collect()
        };
        let buf = draw(&mut g, ColorMode::TrueColor);
        let want = |s: &str, c: Color| s.chars().map(|ch| (ch.to_string(), c)).collect::<Vec<_>>();
        let row0 = [want("{", JSON_PUNCT), want("\"이름\"", JSON_KEY), want(":", JSON_PUNCT), want("\"김\"", JSON_STRING), want(",", JSON_PUNCT), want("\"n\"", JSON_KEY), want(":", JSON_PUNCT), want("-1", NUMBER), want(",", JSON_PUNCT), want("\"ok\"", JSON_KEY), want(":", JSON_PUNCT), want("true", JSON_KEYWORD), want("}", JSON_PUNCT)].concat();
        assert_eq!(colors(&buf, 2, "{", row0.len()), row0);
        let row1 = [want("{", JSON_PUNCT), want("↵", Color::Reset), want("\"a\"", JSON_KEY), want(":[", JSON_PUNCT), want("null", JSON_KEYWORD), want("]}", JSON_PUNCT)].concat();
        assert_eq!(colors(&buf, 3, "{", row1.len()), row1);
        // Binary: plain; the edited cell: bold yellow as any edit
        assert_eq!(colors(&buf, 2, "0", 6), want("0x7B22", Color::Reset));
        let yellow: String = (0..80).map(|x| &buf[(x, 2)]).filter(|c| c.fg == Color::Yellow).map(|c| c.symbol()).collect();
        assert_eq!(yellow.trim_end(), "1~{\"e\":2}", "row mark and the edited cell");
        // NO_COLOR: no colors at all; 16 colors: the five kinds stay apart
        let buf = draw(&mut g, ColorMode::NoColor);
        assert!(colors(&buf, 2, "{", row0.len()).iter().all(|c| c.1 == Color::Reset));
        let c16 = [JSON_KEY, JSON_STRING, NUMBER, JSON_KEYWORD, JSON_PUNCT].map(|c| ColorMode::Ansi16.color(c, false));
        assert_eq!(c16, [Color::LightMagenta, Color::LightGreen, Color::LightCyan, Color::LightRed, Color::DarkGray]);
    }

    /// The value panel colors pretty-printed JSON with the grid's tokenizer and palette: a key is a key on its own line, and a
    /// piece that runs past a line (an unclosed string in a broken value) goes on in color on the next one
    #[test]
    fn value_panel_json_uses_the_grid_colors() {
        use crate::kit::{JSON_KEY, JSON_KEYWORD, JSON_PUNCT, JSON_STRING, NUMBER};
        let runs = |text: &str| -> Vec<Vec<(String, Color)>> { json_lines(text).iter().map(|l| l.spans.iter().map(|s| (s.content.to_string(), s.style.fg.unwrap_or(Color::Reset))).collect()).collect() };
        let r = |s: &str, c: Color| (s.to_string(), c);
        assert_eq!(
            runs("{\n  \"a\": [\n    1\n  ],\n  \"b\": null\n}"),
            [
                vec![r("{", JSON_PUNCT)],
                vec![r("  ", Color::Reset), r("\"a\"", JSON_KEY), r(":", JSON_PUNCT), r(" ", Color::Reset), r("[", JSON_PUNCT)],
                vec![r("    ", Color::Reset), r("1", NUMBER)],
                vec![r("  ", Color::Reset), r("]", JSON_PUNCT), r(",", JSON_PUNCT)],
                vec![r("  ", Color::Reset), r("\"b\"", JSON_KEY), r(":", JSON_PUNCT), r(" ", Color::Reset), r("null", JSON_KEYWORD)],
                vec![r("}", JSON_PUNCT)],
            ]
        );
        assert_eq!(runs("[\"a\nb\"]"), [vec![r("[", JSON_PUNCT), r("\"a", JSON_STRING)], vec![r("b\"", JSON_STRING), r("]", JSON_PUNCT)]]);
    }

    #[test]
    fn local_sort_cycles_and_restores() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let rows = vec![vec![json!(10)], vec![Value::Null], vec![json!("9")], vec![json!(100)]];
        let mut g = Grid::new(QueryResult { columns: vec![col("a")], rows, rows_affected: 0, truncated: true, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        let vals = |g: &Grid| g.result.rows.iter().map(|r| cell_text(&r[0])).collect::<Vec<_>>();
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["9", "10", "100", "<null>"]);
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["100", "10", "9", "<null>"]);
        // Rows loaded while sorted are sorted too
        g.append(QueryResult { columns: vec![], rows: vec![vec![json!(50)]], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        assert_eq!(vals(&g), ["100", "50", "10", "9", "<null>"]);
        g.toggle_local_sort();
        assert_eq!(g.sort, None);
        assert_eq!(vals(&g), ["10", "<null>", "9", "100", "50"]);
    }

    /// Rows loaded after Add Row go before the added one: Submit still inserts only the added row, Revert drops only it, and its
    /// edit and the cursor on it move with it
    #[test]
    fn load_more_keeps_added_rows_last() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let page = |v: &[i64], more| QueryResult { columns: vec![col("a")], rows: v.iter().map(|x| vec![json!(x)]).collect(), rows_affected: 0, truncated: more, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] };
        let mut g = Grid::new(page(&[1, 2], true));
        g.add_row();
        g.set_value(2, 0, "new".into());
        g.append(page(&[3, 4], false));
        let vals = |g: &Grid| g.result.rows.iter().map(|r| cell_text(&r[0])).collect::<Vec<_>>();
        assert_eq!(vals(&g), ["1", "2", "3", "4", "<null>"]);
        assert!(g.is_added(4) && !g.is_added(2) && g.row == 4);
        assert_eq!(g.edits.keys().copied().collect::<Vec<_>>(), [(4, 0)]);
        g.revert();
        assert_eq!(vals(&g), ["1", "2", "3", "4"]);
    }

    /// The same with the local sort on: the loaded rows are sorted, the added row stays last with its edit, and edits and deletes
    /// of loaded rows move with them when the sort changes
    #[test]
    fn local_sort_keeps_added_rows_last_and_edits_on_their_rows() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let page = |v: &[i64], more| QueryResult { columns: vec![col("a")], rows: v.iter().map(|x| vec![json!(x)]).collect(), rows_affected: 0, truncated: more, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] };
        let vals = |g: &Grid| g.result.rows.iter().map(|r| cell_text(&r[0])).collect::<Vec<_>>();
        let mut g = Grid::new(page(&[3, 1], true));
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["1", "3"]);
        g.add_row();
        g.set_value(2, 0, "9".into());
        g.append(page(&[2], false));
        assert_eq!(vals(&g), ["1", "2", "3", "<null>"]);
        assert!(g.is_added(3) && !g.is_added(2) && g.added == 1);
        assert_eq!(g.edits.iter().map(|(k, v)| (*k, v.as_deref())).collect::<Vec<_>>(), [((3, 0), Some("9"))]);
        // Sorting again: an edit and a delete on loaded rows stay on them
        g.set_value(2, 0, "30".into());
        g.deleted.insert(0);
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["3", "2", "1", "<null>"]);
        assert_eq!(g.edits.iter().map(|(k, v)| (*k, v.as_deref())).collect::<Vec<_>>(), [((0, 0), Some("30")), ((3, 0), Some("9"))]);
        assert_eq!(g.deleted.iter().copied().collect::<Vec<_>>(), [2]);
        // Off: the result's order, the added row still last; Revert drops only it
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["3", "1", "2", "<null>"]);
        g.revert();
        assert_eq!(vals(&g), ["3", "1", "2"]);
        // Add Row while sorted, then sort the other way: it stays last
        g.toggle_local_sort();
        g.add_row();
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["3", "2", "1", "<null>"]);
        assert!(g.is_added(3));
    }
    /// Whole column as an IN list: ('a', 'b'). Numbers as is, duplicates·NULL dropped (same as GUI lib/inList.ts)
    #[test]
    fn column_as_in_list() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let rows = vec![vec![json!("1.2.3.4"), json!(1)], vec![json!("it's"), json!(2)], vec![Value::Null, json!(2)], vec![json!("1.2.3.4"), Value::Null]];
        let g = Grid::new(QueryResult { columns: vec![col("ip"), col("n")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        assert_eq!(g.column_in_list(0, DbKind::Postgres), ("('1.2.3.4', 'it''s')".to_string(), 2));
        assert_eq!(g.column_in_list(1, DbKind::Postgres), ("(1, 2)".to_string(), 2));
        // numeric·bigint arrive as text but go unquoted based on column type. Number-like values in text columns (010) are quoted
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("1234.50"), json!("010"), json!("NaN")]];
        let g = Grid::new(QueryResult { columns: vec![ty("a", "NUMERIC"), ty("b", "VARCHAR"), ty("c", "NUMERIC")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        assert_eq!(g.column_in_list(0, DbKind::Postgres).0, "(1234.50)");
        assert_eq!(g.column_in_list(1, DbKind::Postgres).0, "('010')");
        assert_eq!(g.column_in_list(2, DbKind::Postgres).0, "('NaN')");
        // Strings as the tab's kind reads them: MySQL doubles a backslash, PostgreSQL writes E''
        let rows = vec![vec![json!(r"a\b")]];
        let g = Grid::new(QueryResult { columns: vec![ty("s", "TEXT")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        assert_eq!(g.column_in_list(0, DbKind::Mysql).0, r"('a\\b')");
        assert_eq!(g.column_in_list(0, DbKind::Postgres).0, r"(E'a\\b')");
    }
    /// = fits a column; = again on the fitted column puts back the width before (a set one, else the computed one), as the GUI's double-click
    #[test]
    fn fit_toggles_back() {
        let rows = vec![vec![json!("a value much longer than the header")]];
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let mut g = Grid::new(QueryResult { columns: vec![ty("s", "TEXT")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        let computed = g.widths[0];
        let fit = g.fit_width(0);
        // No set width: fit, then forget it (0 to save), the computed width again
        assert_eq!(g.toggle_fit(0).1, fit);
        assert_eq!(g.toggle_fit(0), ("s".to_string(), 0));
        assert_eq!((g.widths[0], g.user_widths.len()), (computed, 0));
        // A set width: fit, then that width again
        g.set_width(0, 5);
        assert_eq!(g.toggle_fit(0).1, fit);
        assert_eq!(g.toggle_fit(0).1, 5);
        // Fitted without a known width before (a saved width): back to the computed one
        g.set_user_widths([("s".to_string(), fit)].into());
        assert_eq!(g.toggle_fit(0).1, 0);
        assert_eq!(g.widths[0], computed);
    }
    /// Shift+arrow range: rectangle, copy as TSV (values only), IN list only the selected cells (row tuples for multiple columns). Plain movement clears it
    #[test]
    fn range_selection_copies_tsv_and_in_list() {
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("1.2.3.4"), json!(1)], vec![json!("5.6.7.8"), json!(2)], vec![json!("9.9.9.9"), json!(3)]];
        let mut g = Grid::new(QueryResult { columns: vec![ty("ip", "TEXT"), ty("n", "INT4")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        assert_eq!(g.sel_box(), None);
        g.extend(1, 0);
        assert_eq!(g.sel_box(), Some((0, 0, 1, 0)));
        assert_eq!(g.in_list(DbKind::Postgres), ("('1.2.3.4', '5.6.7.8')".to_string(), 2));
        g.extend(0, 1);
        assert_eq!(g.selection_tsv(), "1.2.3.4\t1\n5.6.7.8\t2");
        assert_eq!(g.in_list(DbKind::Postgres), ("(('1.2.3.4', 1), ('5.6.7.8', 2))".to_string(), 2));
        // Plain movement clears it, and the IN list is the whole current column
        g.move_by(1, 0);
        assert_eq!(g.sel_box(), None);
        assert_eq!(g.in_list(DbKind::Postgres), ("(1, 2, 3)".to_string(), 3));
    }

    /// One row copied (y: TSV) pastes back as its cells on a new row, its NULL as NULL and a value with quotes as it was (#160)
    #[test]
    fn one_copied_row_pastes_back_as_cells() {
        let ty = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "TEXT".into() };
        let rows = vec![vec![json!(7), Value::Null, json!("{\"k\": \"a, b\"}"), json!("\"q\"")]];
        let mut g = Grid::new(QueryResult { columns: vec![ty("n"), ty("m"), ty("j"), ty("s")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        g.extend(0, 3);
        let text = g.selection_tsv();
        assert_eq!(text, "7\t\t{\"k\": \"a, b\"}\t\"\"\"q\"\"\"");
        g.move_by(0, -3);
        g.add_row();
        g.paste(&text);
        assert_eq!((1..4).map(|c| g.edits.get(&(1, c)).cloned()).collect::<Vec<_>>(), [Some(None), Some(Some("{\"k\": \"a, b\"}".into())), Some(Some("\"q\"".into()))]);
        assert_eq!(g.value(1, 0).as_deref(), Some("7"));
    }

    /// Focus shows in the border's shape, not only its color (NO_COLOR has none). The bottom border is the horizontal scrollbar (#107):
    /// a dim thin track, a bold heavier thumb over the shown columns (the same focused or not), the shown range at its right end
    #[test]
    fn grid_bottom_border_is_a_scrollbar_with_the_shown_columns() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let cols: Vec<_> = (0..12).map(|i| col(&format!("column_{i}"))).collect();
        let rows = vec![(0..12).map(|i| json!(i)).collect()];
        let mut g = Grid::new(QueryResult { columns: cols, rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        let draw = |g: &mut Grid, focused: bool, w: u16| {
            let mut term = Terminal::new(ratatui::backend::TestBackend::new(w, 6)).unwrap();
            term.draw(|f| g.render(f, f.area(), "t".into(), focused, Default::default(), &Default::default())).unwrap();
            let buf = term.backend().buffer().clone();
            let bottom: Vec<_> = (0..w).map(|x| buf[(x, 5)].clone()).collect();
            (buf[(0, 0)].symbol().to_string(), bottom)
        };
        let text = |cells: &[ratatui::buffer::Cell]| cells.iter().map(|c| c.symbol()).collect::<String>();
        for focused in [true, false] {
            let (corner, bottom) = draw(&mut g, focused, 60);
            assert_eq!(corner, if focused { "┏" } else { "┌" });
            let line = text(&bottom);
            let end = if focused { " ◀ cols 1–4 of 12 ▶ ┛" } else { " ◀ cols 1–4 of 12 ▶ ┘" };
            assert!(line.starts_with(if focused { "┗━━" } else { "└━━" }) && line.ends_with(end), "{line}");
            // The thumb: heavier and bold over the first 4 of 12 columns; the rest of the track thin and dim
            let track = &bottom[1..bottom.len() - 1 - " ◀ cols 1–4 of 12 ▶ ".chars().count()];
            let thumb = track.iter().take_while(|c| c.symbol() == "━").count();
            assert!(thumb >= 2 && track[..thumb].iter().all(|c| c.modifier.contains(Modifier::BOLD)), "{line}");
            assert!(track[thumb..].iter().all(|c| c.symbol() == "─" && c.fg == crate::kit::DIM && !c.modifier.contains(Modifier::BOLD)), "{line}");
            assert_eq!(g.hbar.width as usize, track.len());
        }
        // Scrolled to the end: the thumb at the right end of the track, the range follows
        g.col = 11;
        let (_, bottom) = draw(&mut g, true, 60);
        let line = text(&bottom);
        assert!(line.contains("━ ◀ cols 9–12 of 12 ▶ ┛"), "{line}");
        // Every column fits: a plain border, no range
        let (_, bottom) = draw(&mut g, true, 200);
        assert!(!text(&bottom).contains("cols") && g.hbar.width == 0, "{}", text(&bottom));
        // Too narrow for the range: only the track
        g.col = 0;
        let (_, bottom) = draw(&mut g, true, 26);
        assert!(!text(&bottom).contains("cols") && text(&bottom).contains('━') && g.hbar.width == 24, "{}", text(&bottom));
    }

    /// A 3×3 grid of "r{row}c{col}" values
    fn grid3() -> Grid {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let rows = (0..3).map(|r| (0..3).map(|c| json!(format!("r{r}c{c}"))).collect()).collect();
        Grid::new(QueryResult { columns: vec![col("a"), col("b"), col("c")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] })
    }

    /// Draw focused in a color mode: the screen's lines and its buffer
    fn draw_in(g: &mut Grid, mode: crate::theme::ColorMode) -> (Vec<String>, ratatui::buffer::Buffer) {
        let mut term = Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
        term.draw(|f| {
            g.render(f, f.area(), "t".into(), true, mode, &Default::default());
            mode.apply(f.buffer_mut(), &crate::theme::Look::dark());
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        ((0..8).map(|y| (0..40).map(|x| buf[(x, y)].symbol()).collect()).collect(), buf)
    }

    /// The cell at the first character of a drawn value
    fn cell_of(lines: &[String], buf: &ratatui::buffer::Buffer, v: &str) -> ratatui::buffer::Cell {
        let (y, x) = lines.iter().enumerate().find_map(|(y, l)| l.find(v).map(|i| (y, l[..i].chars().count()))).unwrap_or_else(|| panic!("{v} not drawn"));
        buf[(x as u16, y as u16)].clone()
    }

    /// Unsaved changes don't rely on color (NO_COLOR drops it): edited cells are bold, the row number says + added · - deleted · ~ edited
    #[test]
    fn pending_changes_show_without_colors() {
        use crate::theme::ColorMode;
        let mut g = grid3();
        g.set_value(0, 1, "e0c1".into());
        g.deleted.insert(1);
        g.add_row();
        g.set_value(3, 0, "n3c0".into());
        g.row = 2;
        let (lines, buf) = draw_in(&mut g, ColorMode::NoColor);
        let (edited, plain) = (cell_of(&lines, &buf, "e0c1"), cell_of(&lines, &buf, "r0c2"));
        assert_eq!((edited.fg, edited.bg), (plain.fg, plain.bg));
        assert!(edited.modifier.contains(Modifier::BOLD) && !plain.modifier.contains(Modifier::BOLD), "{:?} vs {:?}", edited.modifier, plain.modifier);
        let num = |v: &str| {
            let l: Vec<char> = lines.iter().find(|l| l.contains(v)).unwrap().chars().collect();
            l[1..3].iter().collect::<String>()
        };
        assert_eq!(num("r0c2"), "1~");
        assert_eq!(num("r1c2"), "2-");
        assert_eq!(num("r2c2"), "3 ");
        assert_eq!(num("n3c0"), "4+");
        // Colored modes have the marks too (one rule)
        let (lines, _) = draw_in(&mut g, ColorMode::TrueColor);
        assert!(lines.iter().any(|l| l.contains("1~")), "{lines:?}");
    }

    /// The cursor inside a range stays apart from the range in every color mode
    #[test]
    fn cursor_stands_out_of_a_range_in_every_color_mode() {
        use crate::theme::ColorMode;
        let mut g = grid3();
        g.anchor = Some((0, 0));
        (g.row, g.col) = (1, 1);
        let look = |c: ratatui::buffer::Cell| (c.bg, c.modifier.contains(Modifier::REVERSED));
        for mode in [ColorMode::TrueColor, ColorMode::Ansi256, ColorMode::Ansi16, ColorMode::NoColor] {
            let (lines, buf) = draw_in(&mut g, mode);
            let [cur, range, out] = ["r1c1", "r0c0", "r2c2"].map(|v| look(cell_of(&lines, &buf, v)));
            assert_ne!(cur, range, "{mode:?}");
            assert_ne!(range, out, "{mode:?}: the range shows");
            if mode.reverses_selection() {
                assert!(range.1 && !cur.1, "{mode:?}: the range is reversed, the cursor is the plain cell in it");
            }
        }
        // A single cell is just the cursor, reversed where selections are
        g.anchor = None;
        let (lines, buf) = draw_in(&mut g, ColorMode::NoColor);
        assert!(look(cell_of(&lines, &buf, "r1c1")).1);
    }

    /// Cells sit where Settings › Appearance › Data grid › Alignment says (qurry_shared::format, the GUI's rule): by default numbers on the
    /// right and a NULL where its column's type sits, so it lines up with the values above it; an override moves only its own class
    #[test]
    fn cells_follow_the_alignment_setting() {
        use qurry_core::format::{Align, Alignments, ValueClass};
        let col = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("abc"), json!(12345), json!(true)], vec![Value::Null, Value::Null, json!(false)], vec![json!("하나"), json!("7.5"), Value::Null]];
        let mut g = Grid::new(QueryResult { columns: vec![col("name", "TEXT"), col("qty_total", "NUMERIC"), col("flag", "BOOL")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        // Each body cell's text with its padding: (leading spaces, text) per row of a column
        let cells = |g: &mut Grid, a: &Alignments, c: usize| -> Vec<(usize, String)> {
            let mut term = Terminal::new(ratatui::backend::TestBackend::new(60, 8)).unwrap();
            term.draw(|f| g.render(f, f.area(), "t".into(), true, Default::default(), a)).unwrap();
            let buf = term.backend().buffer().clone();
            let &(_, x0, x1) = g.col_x.iter().find(|x| x.0 == c).unwrap();
            (0..3)
                .map(|r| {
                    let s: String = (x0..x1).map(|x| buf[(x, g.body.y + r)].symbol().to_string()).collect();
                    (s.len() - s.trim_start().len(), s.trim().to_string())
                })
                .collect()
        };
        let w = |g: &Grid, c: usize| g.widths[c] as usize;
        let none = Alignments::new();
        // Number column: values and NULL end at the column's right edge
        let qty = cells(&mut g, &none, 1);
        assert_eq!(qty.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["12345", "<null>", "7.5"]);
        assert!(qty.iter().all(|(lead, t)| lead + t.len() == w(&g, 1)), "{qty:?}");
        // Text and boolean columns: values and NULL on the left
        assert_eq!(cells(&mut g, &none, 0).iter().map(|x| x.0).collect::<Vec<_>>(), [0, 0, 0]);
        assert_eq!(cells(&mut g, &none, 2).iter().map(|x| x.0).collect::<Vec<_>>(), [0, 0, 0]);
        // NULL on the right (as in 0.3.0): in the text column too; wide characters count as two columns
        let right = Alignments::from([(ValueClass::Null, Align::Right)]);
        let name = cells(&mut g, &right, 0);
        assert_eq!((name[0].0, name[2].0), (0, 0));
        assert_eq!(name[1].0 + "<null>".len(), w(&g, 0), "{name:?}");
        // NULL on the left, numbers in the center: booleans stay where they were
        let back = Alignments::from([(ValueClass::Null, Align::Left), (ValueClass::Number, Align::Center)]);
        assert_eq!(cells(&mut g, &back, 0)[1].0, 0);
        let qty = cells(&mut g, &back, 1);
        assert_eq!(qty[1].0, 0, "NULL on the left");
        assert_eq!(qty[0].0, (w(&g, 1) - 5) / 2, "centered: {qty:?}");
        assert_eq!(cells(&mut g, &back, 2).iter().map(|x| x.0).collect::<Vec<_>>(), [0, 0, 0]);
        // Columns without a type name (SQLite `sum(x)`, MongoDB): a NULL follows the values, so it lines up with numbers
        let rows = vec![vec![json!("a"), Value::Null], vec![Value::Null, json!(12345)], vec![json!("b"), json!(7)]];
        let mut g = Grid::new(QueryResult { columns: vec![col("g", ""), col("sum(x)", "")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![] });
        let sum = cells(&mut g, &none, 1);
        assert!(sum.iter().all(|(lead, t)| lead + t.len() == w(&g, 1)), "{sum:?}");
        assert_eq!(cells(&mut g, &none, 0).iter().map(|x| x.0).collect::<Vec<_>>(), [0, 0, 0]);
    }

    /// A page of (id, body) rows whose bodies are cut at `cut`
    fn cut_page(ids: &[i64], cut: &[usize], more: bool) -> QueryResult {
        let col = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = ids.iter().map(|i| vec![json!(i), json!(format!("body{i}"))]).collect();
        QueryResult { columns: vec![col("id", "INT4"), col("body", "TEXT")], rows, rows_affected: 0, truncated: more, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: cut.iter().map(|&r| [r, 1]).collect() }
    }

    fn cut_rows(g: &Grid) -> Vec<String> {
        g.cut.iter().map(|&(r, _)| raw_text(&g.result.rows[r][1]).unwrap()).collect()
    }

    /// #183: a cut cell ends with a dim … right after its text; value() is the cut text; an edit (a whole value) has no mark
    #[test]
    fn cut_cell_has_a_dim_mark() {
        use crate::theme::ColorMode;
        let mut g = Grid::new(cut_page(&[1, 2], &[0], false));
        let (lines, buf) = draw_in(&mut g, ColorMode::TrueColor);
        let row = |id: &str| lines.iter().find(|l| l.contains(id)).unwrap().clone();
        assert!(row("body1").contains("body1…"), "{lines:?}");
        assert!(!row("body2").contains('…'), "{lines:?}");
        assert_eq!(cell_of(&lines, &buf, "…").fg, crate::kit::DIM);
        assert_eq!(g.value(0, 1).as_deref(), Some("body1"));
        assert!(g.is_cut(0, 1) && g.fit_width(1) >= 6);
        g.set_value(0, 1, "whole".into());
        assert!(!g.is_cut(0, 1));
        let (lines, _) = draw_in(&mut g, ColorMode::TrueColor);
        assert!(!lines.iter().any(|l| l.contains('…')), "{lines:?}");
    }

    /// The cut set follows its rows: a load-more's cells after the loaded rows (and before added ones), a local sort, and an
    /// added row deleted above nothing cut
    #[test]
    fn cut_cells_move_with_rows() {
        let mut g = Grid::new(cut_page(&[3, 1], &[1], true));
        g.add_row();
        g.append(cut_page(&[2, 4], &[0], false));
        assert_eq!(g.rows(), 5);
        assert_eq!(cut_rows(&g), ["body1", "body2"]);
        assert!(g.is_added(4));
        g.col = 0;
        g.toggle_local_sort();
        assert_eq!(g.result.rows.iter().take(4).map(|r| r[0].clone()).collect::<Vec<_>>(), [json!(1), json!(2), json!(3), json!(4)]);
        assert_eq!(g.cut, BTreeSet::from([(0, 1), (1, 1)]));
        assert_eq!(cut_rows(&g), ["body1", "body2"]);
        // Sorted back: the order they came in
        g.toggle_local_sort();
        g.toggle_local_sort();
        assert_eq!(cut_rows(&g), ["body1", "body2"]);
        assert_eq!(g.cut, BTreeSet::from([(1, 1), (2, 1)]));
        // Deleting the added row (the last) keeps them; deleting a loaded row only marks it
        g.row = 4;
        g.delete_rows();
        g.row = 1;
        g.delete_rows();
        assert_eq!((g.rows(), g.cut.clone()), (4, BTreeSet::from([(1, 1), (2, 1)])));
    }

    /// The value panel on a cut cell: the note over the cut text, which ends with …
    #[test]
    fn value_panel_shows_a_cut_note() {
        let mut g = Grid::new(cut_page(&[1], &[0], false));
        g.col = 1;
        let mut term = Terminal::new(ratatui::backend::TestBackend::new(100, 8)).unwrap();
        let note ="Cut at 5 characters; the whole value needs a primary key (public.t has none)";
        term.draw(|f| g.render_value_panel(f, f.area(), String::new(), Some(note.into()))).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = (0..8).map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n").collect();
        assert!(text.contains("Cut at 5 characters; the whole value needs a primary key") && text.contains("body1…"), "{text}");
    }
}