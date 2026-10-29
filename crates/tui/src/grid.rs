//! Result grid: cell movement·horizontal/vertical scroll·edit markers (TUI version of DataGrip's data editor)
use qurry_core::db::QueryResult;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use unicode_width::UnicodeWidthStr;

pub struct Grid {
    pub result: QueryResult,
    pub row: usize,
    pub col: usize,
    pub row_off: usize,
    pub col_off: usize,
    /// Server cursor session (console) — scrolling to the end loads more
    pub more_session: Option<String>,
    pub loading_more: bool,
    /// Edits: (row, col) → new value (None = NULL)
    pub edits: BTreeMap<(usize, usize), Option<String>>,
    pub deleted: BTreeSet<usize>,
    /// Number of newly added rows (appended to the end of result.rows)
    pub added: usize,
    pub widths: Vec<u16>,
    /// Fractional-second digits of time columns (aligned within a column, 0 = no alignment)
    frac: Vec<usize>,
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
    /// Original position of each row while sorted on screen (restored when sorting is cleared)
    orig: Vec<usize>,
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
/// End position of "hh:mm:ss" and fractional-second digits of a time value ([date ]hh:mm:ss[.ffffff]…)
fn time_frac(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let start = match b.iter().position(|&c| c == b' ' || c == b'T') {
        Some(i) if i >= 10 && b[i - 3] == b'-' => i + 1,
        _ => 0,
    };
    let t = b.get(start..start + 8)?;
    let ok = t.iter().enumerate().all(|(i, &c)| if i == 2 || i == 5 { c == b':' } else { c.is_ascii_digit() });
    if !ok {
        return None;
    }
    let end = start + 8;
    let digits = if b.get(end) == Some(&b'.') { b[end + 1..].iter().take_while(|c| c.is_ascii_digit()).count() } else { 0 };
    Some((end, digits))
}

/// 03:19:15 → 03:19:15.000000 (pad with 0 up to width digits)
pub fn pad_frac(s: &str, width: usize) -> String {
    match time_frac(s) {
        Some((end, d)) if d < width => {
            let rest = &s[end + if d > 0 { 1 + d } else { 0 }..];
            let frac = if d > 0 { &s[end + 1..end + 1 + d] } else { "" };
            format!("{}.{:0<width$}{rest}", &s[..end], frac)
        }
        _ => s.to_string(),
    }
}

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

impl Grid {
    pub fn new(result: QueryResult) -> Self {
        let mut g = Grid { result, row: 0, col: 0, row_off: 0, col_off: 0, more_session: None, loading_more: false, edits: BTreeMap::new(), deleted: BTreeSet::new(), added: 0, widths: vec![], frac: vec![], page: 10, body: Rect::default(), col_x: vec![], hbar: Rect::default(), header: Rect::default(), max_off: usize::MAX, sort: None, anchor: None, orig: vec![] };
        g.compute_widths();
        g
    }

    pub fn compute_widths(&mut self) {
        let r = &self.result;
        self.widths = r
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let body = r.rows.iter().take(300).map(|row| row.get(i).map(cell_text).unwrap_or_default().width()).max().unwrap_or(0);
                // +2: room for the sort indicator (▲▼)
                ((c.name.width() + 2).max(body).clamp(4, 40) + 1) as u16
            })
            .collect();
        self.compute_frac();
    }

    /// Time columns: longest fractional-second digits among loaded values
    fn compute_frac(&mut self) {
        let r = &self.result;
        self.frac = r
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let t = c.type_name.to_uppercase();
                if !t.contains("TIME") || t.contains("INTERVAL") {
                    return 0;
                }
                r.rows.iter().filter_map(|row| row.get(i)?.as_str().and_then(time_frac)).map(|x| x.1).max().unwrap_or(0)
            })
            .collect();
    }

    pub fn append(&mut self, more: QueryResult) {
        let n = self.result.rows.len();
        self.result.rows.extend(more.rows);
        self.result.truncated = more.truncated;
        self.compute_frac();
        if !self.orig.is_empty() {
            self.orig.extend(n..self.result.rows.len());
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
            self.orig = (0..self.rows()).collect();
        }
        self.apply_local_sort();
        if self.sort.is_none() {
            self.orig.clear();
        }
    }

    fn apply_local_sort(&mut self) {
        let mut rows: Vec<(usize, Vec<Value>)> = std::mem::take(&mut self.orig).into_iter().zip(std::mem::take(&mut self.result.rows)).collect();
        match self.sort {
            Some((c, desc)) => rows.sort_by(|a, b| {
                let o = cmp_cell(a.1.get(c), b.1.get(c), desc);
                if desc { o.reverse() } else { o }
            }),
            None => rows.sort_by_key(|r| r.0),
        }
        (self.orig, self.result.rows) = rows.into_iter().unzip();
    }

    pub fn rows(&self) -> usize {
        self.result.rows.len()
    }
    pub fn cols(&self) -> usize {
        self.result.columns.len()
    }

    /// Value with edits applied
    pub fn value(&self, r: usize, c: usize) -> Option<String> {
        if let Some(v) = self.edits.get(&(r, c)) {
            return v.clone();
        }
        self.result.rows.get(r).and_then(|row| row.get(c)).and_then(raw_text)
    }

    /// One cell as an SQL value (NULL is None). Numeric columns keep the value as is (numeric·bigint arrive as text), only when it looks numeric (NaN etc. are quoted)
    fn sql_value(&self, r: usize, c: usize) -> Option<String> {
        let ty = self.result.columns.get(c).map(|x| x.type_name.to_lowercase()).unwrap_or_default();
        let head: String = ty.chars().take_while(|ch| ch.is_ascii_alphanumeric()).collect();
        let numeric_col = ["int", "int2", "int4", "int8", "integer", "tinyint", "smallint", "mediumint", "bigint", "serial", "serial4", "serial8", "smallserial", "bigserial", "numeric", "decimal", "real", "float", "float4", "float8", "double", "bool", "boolean", "oid"].contains(&head.as_str());
        let num = |s: &str| {
            let t = s.strip_prefix('-').unwrap_or(s);
            !t.is_empty() && t.parse::<f64>().is_ok() && t.chars().all(|ch| ch.is_ascii_digit() || ".eE+-".contains(ch))
        };
        let lit = |s: String| if numeric_col && num(&s) { s } else { format!("'{}'", s.replace('\'', "''")) };
        match self.edits.get(&(r, c)) {
            Some(v) => v.clone().map(lit),
            None => match self.result.rows.get(r)?.get(c)? {
                Value::Null => None,
                v @ (Value::Number(_) | Value::Bool(_)) => Some(v.to_string()),
                v => raw_text(v).map(lit),
            },
        }
    }

    /// Range as IN-list text and value count. One column gives ('a', 'b'), several give row tuples (('a', 1), …). Duplicates·NULL are dropped
    fn in_list_of(&self, (r0, c0, r1, c1): (usize, usize, usize, usize)) -> (String, usize) {
        let mut seen = std::collections::HashSet::new();
        let items: Vec<String> = (r0..=r1.min(self.rows().saturating_sub(1)))
            .filter_map(|r| {
                let vals: Vec<Option<String>> = (c0..=c1).map(|c| self.sql_value(r, c)).collect();
                if vals.iter().all(|v| v.is_none()) {
                    return None;
                }
                let vals: Vec<String> = vals.into_iter().map(|v| v.unwrap_or_else(|| "NULL".into())).collect();
                Some(if c0 == c1 { vals[0].clone() } else { format!("({})", vals.join(", ")) })
            })
            .filter(|x| seen.insert(x.clone()))
            .collect();
        let n = items.len();
        (format!("({})", if n == 0 { "NULL".into() } else { items.join(", ") }), n)
    }

    /// Whole column as an IN list
    pub fn column_in_list(&self, c: usize) -> (String, usize) {
        self.in_list_of((0, c, self.rows().saturating_sub(1), c))
    }

    /// Selected range, or the whole current column if none, as an IN list
    pub fn in_list(&self) -> (String, usize) {
        match self.sel_rect() {
            Some(r) => self.in_list_of(r),
            None => self.column_in_list(self.col),
        }
    }

    /// Selected range (first row, first col, last row, last col). Selected with Shift+arrows·dragging
    pub fn sel_rect(&self) -> Option<(usize, usize, usize, usize)> {
        let (ar, ac) = self.anchor?;
        Some((ar.min(self.row), ac.min(self.col), ar.max(self.row), ac.max(self.col)))
    }

    /// Extend the range (starting from the current cell the first time)
    pub fn extend(&mut self, dr: isize, dc: isize) {
        let anchor = self.anchor.unwrap_or((self.row, self.col));
        self.move_by(dr, dc);
        self.anchor = Some(anchor);
    }

    /// Selected range as TSV (values only, NULL as empty): pastes into a sheet as cells
    pub fn selection_tsv(&self) -> String {
        let Some((r0, c0, r1, c1)) = self.sel_rect() else { return self.value(self.row, self.col).unwrap_or_default() };
        (r0..=r1).map(|r| (c0..=c1).map(|c| self.value(r, c).unwrap_or_default()).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n")
    }

    pub fn dirty(&self) -> bool {
        !self.edits.is_empty() || !self.deleted.is_empty() || self.added > 0
    }

    pub fn revert(&mut self) {
        self.edits.clear();
        self.deleted.clear();
        let n = self.result.rows.len() - self.added;
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
        let orig = self.result.rows.get(r).and_then(|row| row.get(c)).and_then(raw_text);
        if orig.as_deref() == Some(v.as_str()) {
            self.edits.remove(&(r, c));
        } else {
            self.edits.insert((r, c), Some(v));
        }
    }

    /// Paste: fill from the current cell. Tabs for multiple cells, lines for multiple rows (spreadsheet·DataGrip copy). Missing rows are added, overflowing columns dropped
    pub fn paste(&mut self, text: &str) {
        let (row, col) = (self.row, self.col);
        let mut lines: Vec<&str> = text.split('\n').collect();
        if lines.len() > 1 && lines.last() == Some(&"") {
            lines.pop();
        }
        for (i, line) in lines.into_iter().enumerate() {
            let r = row + i;
            while r >= self.rows() {
                self.add_row();
            }
            let cells: Vec<&str> = if line.contains('\t') { line.split('\t').collect() } else { vec![line] };
            let n = self.cols();
            for (j, v) in cells.into_iter().enumerate().filter(|(j, _)| col + j < n) {
                self.set_value(r, col + j, v.to_string());
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
        let cols = self.cols() as isize;
        if rows > 0 {
            self.row = (self.row as isize + dr).clamp(0, rows - 1) as usize;
        }
        if cols > 0 {
            self.col = (self.col as isize + dc).clamp(0, cols - 1) as usize;
        }
    }

    /// Horizontal scroll (trackpad·Shift+wheel): move the first visible column and the selection with it
    pub fn hscroll(&mut self, d: isize) {
        if self.cols() == 0 {
            return;
        }
        self.col_off = (self.col_off as isize + d).clamp(0, self.max_off.min(self.cols() - 1) as isize) as usize;
        self.col = self.col_off;
    }

    /// Jump to the clicked·dragged scrollbar position
    pub fn hscroll_to(&mut self, x: u16) {
        let b = self.hbar;
        if b.width == 0 || self.cols() == 0 {
            return;
        }
        let c = (x.saturating_sub(b.x) as usize * self.cols() / b.width as usize).min(self.max_off).min(self.cols() - 1);
        self.col_off = c;
        self.col = c;
    }

    /// When near the end, time to load more (only up to MAX_ROWS: beyond that, export to CSV)
    pub fn wants_more(&self) -> bool {
        self.result.truncated && self.more_session.is_some() && !self.loading_more && self.row + 5 >= self.rows() && self.rows() < MAX_ROWS
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

    pub fn render(&mut self, f: &mut Frame, area: Rect, title: String, focused: bool) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) });
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 2 || self.cols() == 0 {
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
        // Horizontal scroll: keep the selected column visible
        let num_w = (self.rows().max(1).to_string().len() + 1) as u16;
        let avail = inner.width.saturating_sub(num_w + 1);
        // So the right side isn't empty when scrolled to the end: don't scroll past the first column at which the last column reaches the right edge
        self.max_off = {
            let mut off = self.cols();
            let mut w = 0u16;
            while off > 0 && w + self.widths[off - 1] + 1 <= avail {
                off -= 1;
                w += self.widths[off] + 1;
            }
            off.min(self.cols() - 1)
        };
        if self.col_off > self.max_off {
            self.col_off = self.max_off;
        }
        if self.col < self.col_off {
            self.col_off = self.col;
        }
        loop {
            let used: u16 = self.widths[self.col_off..=self.col].iter().map(|w| w + 1).sum();
            if used <= avail || self.col_off == self.col {
                break;
            }
            self.col_off += 1;
        }
        // Visible columns and their widths. A last column that doesn't fit is shown cut to the remaining width
        let mut cols: Vec<usize> = vec![];
        let mut shown_w: Vec<u16> = vec![];
        let mut used = 0u16;
        let mut full = 0usize;
        for c in self.col_off..self.cols() {
            let w = self.widths[c];
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
        let numeric: Vec<bool> = self
            .result
            .columns
            .iter()
            .map(|c| {
                let t = c.type_name.to_uppercase();
                ["INT", "NUMERIC", "DECIMAL", "FLOAT", "DOUBLE", "REAL", "SERIAL", "OID"].iter().any(|k| t.contains(k))
            })
            .collect();
        let sel = self.sel_rect();
        let end = (self.row_off + body_h).min(self.rows());
        let rows: Vec<Row> = (self.row_off..end)
            .map(|r| {
                let deleted = self.deleted.contains(&r);
                let added = self.is_added(r);
                let num = Cell::from(format!("{:>w$}", r + 1, w = num_w as usize - 1)).style(Style::default().fg(if added { crate::kit::OK } else if deleted { crate::kit::DANGER } else { Color::DarkGray }));
                Row::new(std::iter::once(num).chain(cols.iter().map(|&c| {
                    let edited = self.edits.contains_key(&(r, c));
                    let v = self.shown(r, c, self.widths[c] as usize + 1);
                    let text = match &v {
                        None if added && !edited => "<default>".to_string(),
                        None => "<null>".to_string(),
                        Some(s) => s.clone(),
                    };
                    let w = self.widths[c] as usize;
                    let text = if numeric[c] && v.is_some() { format!("{text:>w$}") } else { text };
                    let mut st = if v.is_none() { Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC) } else if numeric[c] { Style::default().fg(crate::kit::NUMBER) } else { Style::default() };
                    if edited {
                        st = st.fg(Color::Yellow);
                    }
                    if deleted {
                        st = st.fg(crate::kit::DANGER).add_modifier(Modifier::CROSSED_OUT);
                    }
                    if added {
                        st = st.fg(crate::kit::OK);
                    }
                    let in_sel = sel.is_some_and(|(r0, c0, r1, c1)| r >= r0 && r <= r1 && c >= c0 && c <= c1);
                    if r == self.row && c == self.col {
                        st = st.bg(crate::kit::selected(focused));
                    } else if in_sel {
                        st = st.bg(crate::kit::CELL_EDITED);
                    } else if r == self.row {
                        st = st.bg(crate::kit::CELL_RANGE);
                    }
                    Cell::from(text).style(st)
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
        let rows_end = (inner.y + 1 + shown_rows).min(inner.bottom());
        for y in inner.y..rows_end {
            for x in inner.x..inner.right().min(last) {
                buf[(x, y)].set_style(under);
            }
            for &x in &seps {
                buf[(x, y)].set_symbol("│").set_style(line);
            }
        }
        // Horizontal scrollbar: visible range in bold on the bottom border (click or drag to move)
        self.hbar = Rect::default();
        if self.col_off > 0 || self.col_off + full < self.cols() {
            let track = Rect { x: inner.x, y: area.bottom().saturating_sub(1), width: inner.width, height: 1 };
            let n = self.cols() as u32;
            let tw = track.width as u32;
            let start = (self.col_off as u32 * tw / n) as u16;
            let len = ((full.max(1) as u32 * tw).div_ceil(n) as u16).clamp(2, track.width - start);
            let thumb = Style::default().fg(if focused { Color::Cyan } else { Color::Gray });
            for x in track.x + start..track.x + start + len {
                buf[(x, track.y)].set_symbol("━").set_style(thumb);
            }
            self.hbar = track;
        }
    }
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
        let g = Grid::new(QueryResult { columns: vec![col("at", "TIMESTAMPTZ"), col("note", "TEXT")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(g.shown(0, 0, 100).unwrap(), "2026-06-06 03:19:15.000000 +09:00");
        assert_eq!(g.shown(1, 0, 100).unwrap(), "2026-06-08 09:38:06.521101 +09:00");
        assert_eq!(g.shown(2, 0, 100), None);
        assert_eq!(g.shown(0, 1, 100).unwrap(), "03:19:15", "not a time column");
        assert_eq!(g.result.rows[0][0], json!("2026-06-06 03:19:15 +09:00"), "value unchanged");
    }

    #[test]
    fn local_sort_cycles_and_restores() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let rows = vec![vec![json!(10)], vec![Value::Null], vec![json!("9")], vec![json!(100)]];
        let mut g = Grid::new(QueryResult { columns: vec![col("a")], rows, rows_affected: 0, truncated: true, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        let vals = |g: &Grid| g.result.rows.iter().map(|r| cell_text(&r[0])).collect::<Vec<_>>();
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["9", "10", "100", "<null>"]);
        g.toggle_local_sort();
        assert_eq!(vals(&g), ["100", "10", "9", "<null>"]);
        // Rows loaded while sorted are sorted too
        g.append(QueryResult { columns: vec![], rows: vec![vec![json!(50)]], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(vals(&g), ["100", "50", "10", "9", "<null>"]);
        g.toggle_local_sort();
        assert_eq!(g.sort, None);
        assert_eq!(vals(&g), ["10", "<null>", "9", "100", "50"]);
    }
    /// Whole column as an IN list: ('a', 'b'). Numbers as is, duplicates·NULL dropped (same as GUI lib/inList.ts)
    #[test]
    fn column_as_in_list() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let rows = vec![vec![json!("1.2.3.4"), json!(1)], vec![json!("it's"), json!(2)], vec![Value::Null, json!(2)], vec![json!("1.2.3.4"), Value::Null]];
        let g = Grid::new(QueryResult { columns: vec![col("ip"), col("n")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(g.column_in_list(0), ("('1.2.3.4', 'it''s')".to_string(), 2));
        assert_eq!(g.column_in_list(1), ("(1, 2)".to_string(), 2));
        // numeric·bigint arrive as text but go unquoted based on column type. Number-like values in text columns (010) are quoted
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("1234.50"), json!("010"), json!("NaN")]];
        let g = Grid::new(QueryResult { columns: vec![ty("a", "NUMERIC"), ty("b", "VARCHAR"), ty("c", "NUMERIC")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(g.column_in_list(0).0, "(1234.50)");
        assert_eq!(g.column_in_list(1).0, "('010')");
        assert_eq!(g.column_in_list(2).0, "('NaN')");
    }
    /// Shift+arrow range: rectangle, copy as TSV (values only), IN list only the selected cells (row tuples for multiple columns). Plain movement clears it
    #[test]
    fn range_selection_copies_tsv_and_in_list() {
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("1.2.3.4"), json!(1)], vec![json!("5.6.7.8"), json!(2)], vec![json!("9.9.9.9"), json!(3)]];
        let mut g = Grid::new(QueryResult { columns: vec![ty("ip", "TEXT"), ty("n", "INT4")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(g.sel_rect(), None);
        g.extend(1, 0);
        assert_eq!(g.sel_rect(), Some((0, 0, 1, 0)));
        assert_eq!(g.in_list(), ("('1.2.3.4', '5.6.7.8')".to_string(), 2));
        g.extend(0, 1);
        assert_eq!(g.selection_tsv(), "1.2.3.4\t1\n5.6.7.8\t2");
        assert_eq!(g.in_list(), ("(('1.2.3.4', 1), ('5.6.7.8', 2))".to_string(), 2));
        // Plain movement clears it, and the IN list is the whole current column
        g.move_by(1, 0);
        assert_eq!(g.sel_rect(), None);
        assert_eq!(g.in_list(), ("(1, 2, 3)".to_string(), 3));
    }
}