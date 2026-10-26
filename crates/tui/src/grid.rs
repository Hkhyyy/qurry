//! 결과 그리드: 셀 이동·가로/세로 스크롤·편집 표시 (DataGrip 데이터 에디터의 TUI 판)
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
    /// 서버 커서 세션 (콘솔) — 끝까지 내리면 더 불러온다
    pub more_session: Option<String>,
    pub loading_more: bool,
    /// 편집: (행, 열) → 새 값 (None = NULL)
    pub edits: BTreeMap<(usize, usize), Option<String>>,
    pub deleted: BTreeSet<usize>,
    /// 새로 추가한 행 수 (result.rows 끝에 붙어 있다)
    pub added: usize,
    pub widths: Vec<u16>,
    /// 마지막 렌더 높이 (PgUp/PgDn)
    pub page: usize,
    /// 마지막 렌더 위치 (마우스 클릭): 본문 영역, 보이는 열의 (열 번호, x 시작, x 끝)
    pub body: Rect,
    pub col_x: Vec<(usize, u16, u16)>,
    /// 가로 스크롤바 (아래 테두리 줄). 열이 다 보이면 비어 있다
    pub hbar: Rect,
    /// 헤더 + 구분선 (여기서 휠 = 가로 스크롤)
    pub header: Rect,
    /// 마지막 열이 오른쪽 끝에 닿는 첫 열 (그보다 더 밀지 않는다)
    pub max_off: usize,
    /// 정렬 표시: (열, 내림차순)
    pub sort: Option<(usize, bool)>,
    /// 화면 정렬 중이면 각 행의 원래 위치 (정렬 해제 때 되돌린다)
    orig: Vec<usize>,
}

/// 결과 그리드에 쌓아 두는 최대 행 수. 그 이상은 CSV 로 내보낸다
pub const MAX_ROWS: usize = 100_000;

pub fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "<null>".into(),
        Value::String(s) => s.replace('\n', "↵").replace('\t', "→"),
        other => other.to_string(),
    }
}
pub fn raw_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// GUI 와 같은 비교: NULL 은 항상 끝, 둘 다 숫자면 숫자로, 아니면 글자로. desc 면 결과가 뒤집히므로 NULL 쪽도 미리 뒤집는다
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
        let mut g = Grid { result, row: 0, col: 0, row_off: 0, col_off: 0, more_session: None, loading_more: false, edits: BTreeMap::new(), deleted: BTreeSet::new(), added: 0, widths: vec![], page: 10, body: Rect::default(), col_x: vec![], hbar: Rect::default(), header: Rect::default(), max_off: usize::MAX, sort: None, orig: vec![] };
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
                // +2: 정렬 표시(▲▼) 자리
                ((c.name.width() + 2).max(body).clamp(4, 40) + 1) as u16
            })
            .collect();
    }

    pub fn append(&mut self, more: QueryResult) {
        let n = self.result.rows.len();
        self.result.rows.extend(more.rows);
        self.result.truncated = more.truncated;
        if !self.orig.is_empty() {
            self.orig.extend(n..self.result.rows.len());
            self.apply_local_sort();
        }
    }

    /// 화면 정렬 (불러온 행만): ↑ → ↓ → 해제. 편집 결과(테이블 탭)는 서버 ORDER BY 를 쓴다
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

    /// 편집 반영된 값
    pub fn value(&self, r: usize, c: usize) -> Option<String> {
        if let Some(v) = self.edits.get(&(r, c)) {
            return v.clone();
        }
        self.result.rows.get(r).and_then(|row| row.get(c)).and_then(raw_text)
    }

    /// 열 전체를 IN 목록 글과 값 개수로
    pub fn column_in_list(&self, c: usize) -> (String, usize) {
        // 숫자 컬럼은 값 그대로 (numeric·bigint 는 글로 온다). 숫자 모양일 때만 (NaN 등은 따옴표)
        let ty = self.result.columns.get(c).map(|x| x.type_name.to_lowercase()).unwrap_or_default();
        let head: String = ty.chars().take_while(|ch| ch.is_ascii_alphanumeric()).collect();
        let numeric_col = ["int", "int2", "int4", "int8", "integer", "tinyint", "smallint", "mediumint", "bigint", "serial", "serial4", "serial8", "smallserial", "bigserial", "numeric", "decimal", "real", "float", "float4", "float8", "double", "bool", "boolean", "oid"].contains(&head.as_str());
        let num = |s: &str| {
            let t = s.strip_prefix('-').unwrap_or(s);
            !t.is_empty() && t.parse::<f64>().is_ok() && t.chars().all(|ch| ch.is_ascii_digit() || ".eE+-".contains(ch))
        };
        let lit = |s: String| if numeric_col && num(&s) { s } else { format!("'{}'", s.replace('\'', "''")) };
        let mut seen = std::collections::HashSet::new();
        let items: Vec<String> = (0..self.result.rows.len())
            .filter_map(|r| match self.edits.get(&(r, c)) {
                Some(v) => v.clone().map(lit),
                None => match self.result.rows[r].get(c)? {
                    Value::Null => None,
                    v @ (Value::Number(_) | Value::Bool(_)) => Some(v.to_string()),
                    v => raw_text(v).map(lit),
                },
            })
            .filter(|x| seen.insert(x.clone()))
            .collect();
        let n = items.len();
        (format!("({})", if n == 0 { "NULL".into() } else { items.join(", ") }), n)
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

    /// 칸 값을 바꾼다 (원래 값과 같으면 편집을 지운다)
    pub fn set_value(&mut self, r: usize, c: usize, v: String) {
        let orig = self.result.rows.get(r).and_then(|row| row.get(c)).and_then(raw_text);
        if orig.as_deref() == Some(v.as_str()) {
            self.edits.remove(&(r, c));
        } else {
            self.edits.insert((r, c), Some(v));
        }
    }

    /// 붙여넣기: 현재 칸부터 채운다. 탭으로 여러 칸, 줄로 여러 행 (스프레드시트·DataGrip 복사). 모자라는 행은 새로 추가, 넘치는 열은 버린다
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

    /// 화면 좌표 → (행, 열)
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
        let rows = self.rows() as isize;
        let cols = self.cols() as isize;
        if rows > 0 {
            self.row = (self.row as isize + dr).clamp(0, rows - 1) as usize;
        }
        if cols > 0 {
            self.col = (self.col as isize + dc).clamp(0, cols - 1) as usize;
        }
    }

    /// 가로 스크롤 (트랙패드·Shift+휠): 보이는 첫 열을 옮기고 선택도 그 열로
    pub fn hscroll(&mut self, d: isize) {
        if self.cols() == 0 {
            return;
        }
        self.col_off = (self.col_off as isize + d).clamp(0, self.max_off.min(self.cols() - 1) as isize) as usize;
        self.col = self.col_off;
    }

    /// 스크롤바를 누른·끈 위치로
    pub fn hscroll_to(&mut self, x: u16) {
        let b = self.hbar;
        if b.width == 0 || self.cols() == 0 {
            return;
        }
        let c = (x.saturating_sub(b.x) as usize * self.cols() / b.width as usize).min(self.max_off).min(self.cols() - 1);
        self.col_off = c;
        self.col = c;
    }

    /// 끝 근처면 더 불러올 때 (MAX_ROWS 까지만: 그 이상은 CSV 로 내보낸다)
    pub fn wants_more(&self) -> bool {
        self.result.truncated && self.more_session.is_some() && !self.loading_more && self.row + 5 >= self.rows() && self.rows() < MAX_ROWS
    }

    /// 그릴 칸의 글자: 폭(max 글자)까지만 만든다 (큰 text·jsonb 를 프레임마다 통째로 복사하지 않게)
    fn shown(&self, r: usize, c: usize, max: usize) -> Option<String> {
        let clip = |s: &str| -> String { s.chars().take(max).map(|ch| match ch { '\n' => '↵', '\t' => '→', ch => ch }).collect() };
        if let Some(v) = self.edits.get(&(r, c)) {
            return v.as_deref().map(clip);
        }
        match self.result.rows.get(r)?.get(c)? {
            Value::Null => None,
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
        // 헤더 아래가 본문 (행 구분은 줄을 따로 쓰지 않고 밑줄로 긋는다)
        let body_h = (inner.height - 1) as usize;
        self.page = body_h.max(1);
        // 세로 스크롤
        if self.row < self.row_off {
            self.row_off = self.row;
        } else if self.row >= self.row_off + body_h {
            self.row_off = self.row + 1 - body_h;
        }
        // 가로 스크롤: 선택 열이 보이도록
        let num_w = (self.rows().max(1).to_string().len() + 1) as u16;
        let avail = inner.width.saturating_sub(num_w + 1);
        // 끝까지 밀었을 때 오른쪽이 비지 않도록: 마지막 열이 오른쪽 끝에 닿는 첫 열보다 더 밀지 않는다
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
        // 보이는 열과 그 폭. 다 들어가지 않는 마지막 열은 남은 폭만큼 잘라서 보인다
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
        // 클릭 판정용 위치 기록
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
        .style(Style::default().bg(Color::Rgb(43, 45, 48)));
        let numeric: Vec<bool> = self
            .result
            .columns
            .iter()
            .map(|c| {
                let t = c.type_name.to_uppercase();
                ["INT", "NUMERIC", "DECIMAL", "FLOAT", "DOUBLE", "REAL", "SERIAL", "OID"].iter().any(|k| t.contains(k))
            })
            .collect();
        let end = (self.row_off + body_h).min(self.rows());
        let rows: Vec<Row> = (self.row_off..end)
            .map(|r| {
                let deleted = self.deleted.contains(&r);
                let added = self.is_added(r);
                let num = Cell::from(format!("{:>w$}", r + 1, w = num_w as usize - 1)).style(Style::default().fg(if added { Color::Green } else if deleted { Color::Red } else { Color::DarkGray }));
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
                    let mut st = if v.is_none() { Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC) } else if numeric[c] { Style::default().fg(Color::Rgb(42, 172, 184)) } else { Style::default() };
                    if edited {
                        st = st.fg(Color::Yellow);
                    }
                    if deleted {
                        st = st.fg(Color::Red).add_modifier(Modifier::CROSSED_OUT);
                    }
                    if added {
                        st = st.fg(Color::Green);
                    }
                    if r == self.row && c == self.col {
                        st = st.bg(if focused { Color::Rgb(46, 67, 110) } else { Color::Rgb(50, 52, 56) });
                    } else if r == self.row {
                        st = st.bg(Color::Rgb(38, 40, 46));
                    }
                    Cell::from(text).style(st)
                })))
            })
            .collect();
        let widths: Vec<Constraint> = std::iter::once(Constraint::Length(num_w)).chain(shown_w.iter().map(|&w| Constraint::Length(w))).collect();
        let shown_rows = (end - self.row_off) as u16;
        f.render_widget(Table::new(rows, widths).header(header).column_spacing(1), inner);
        // 구분선: 열 사이는 세로줄(열 사이 한 칸 간격 자리), 헤더·행 사이는 흐린 밑줄.
        // 가로줄을 줄로 따로 그리면 보이는 행이 절반이 되고, 일부 터미널(Orca 등)에서는 ─ 가 안 보인다
        let line_c = Color::Rgb(96, 99, 106);
        // 세로줄 칸에는 밑줄을 긋지 않는다 (│ 와 밑줄이 겹치면 터미널에 따라 교차점에 돌기가 생기고 높이도 어긋난다)
        let line = Style::default().fg(line_c).remove_modifier(Modifier::UNDERLINED);
        let under = Style::default().add_modifier(Modifier::UNDERLINED).underline_color(line_c);
        let buf = f.buffer_mut();
        // 잘린 마지막 열 뒤에는 선을 긋지 않는다 (테두리와 겹친다)
        let seps: Vec<u16> = std::iter::once(inner.x + num_w).chain(self.col_x.iter().take(full).map(|c| c.2)).filter(|x| *x < inner.right()).collect();
        // 밑줄은 (잘린 열까지) 마지막 열 끝까지
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
        // 가로 스크롤바: 아래 테두리에 보이는 범위를 굵게 (누르거나 끌어서 옮긴다)
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
        // 정렬 중에 더 불러온 행도 정렬된다
        g.append(QueryResult { columns: vec![], rows: vec![vec![json!(50)]], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(vals(&g), ["100", "50", "10", "9", "<null>"]);
        g.toggle_local_sort();
        assert_eq!(g.sort, None);
        assert_eq!(vals(&g), ["10", "<null>", "9", "100", "50"]);
    }
    /// 열 전체를 IN 목록으로: ('a', 'b'). 숫자는 그대로, 중복·NULL 은 뺀다 (GUI lib/inList.ts 와 같다)
    #[test]
    fn column_as_in_list() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: String::new() };
        let rows = vec![vec![json!("1.2.3.4"), json!(1)], vec![json!("it's"), json!(2)], vec![Value::Null, json!(2)], vec![json!("1.2.3.4"), Value::Null]];
        let g = Grid::new(QueryResult { columns: vec![col("ip"), col("n")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(g.column_in_list(0), ("('1.2.3.4', 'it''s')".to_string(), 2));
        assert_eq!(g.column_in_list(1), ("(1, 2)".to_string(), 2));
        // numeric·bigint 는 글로 와도 컬럼 타입을 보고 따옴표 없이. 문자 컬럼의 숫자 모양(010)은 따옴표
        let ty = |n: &str, t: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: t.into() };
        let rows = vec![vec![json!("1234.50"), json!("010"), json!("NaN")]];
        let g = Grid::new(QueryResult { columns: vec![ty("a", "NUMERIC"), ty("b", "VARCHAR"), ty("c", "NUMERIC")], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false });
        assert_eq!(g.column_in_list(0).0, "(1234.50)");
        assert_eq!(g.column_in_list(1).0, "('010')");
        assert_eq!(g.column_in_list(2).0, "('NaN')");
    }
}