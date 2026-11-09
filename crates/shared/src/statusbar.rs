//! The status bar's items for the focused grid (#132), the same words in the GUI (through WASM) and the TUI: which items there
//! are and which are on (settings.json "statusBarItems", shared by both apps), and the text of each. Position carries the selection
//! ("Rows 5–26 (22) · Col 4"); Not null, Sum, Avg, Min and Max are over the selected cells; Size is the current cell's bytes as
//! sent (UTF-8; binary: its length) and its column's declared limit when the table's metadata gives it.

use crate::DbKind;
use crate::format::{self, ValueClass};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An item the user can turn on or off (the ids are what settings.json keeps)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Item {
    NotNull,
    Sum,
    Avg,
    Min,
    Max,
    Size,
    ShownColumns,
    Position,
}

/// How the checklists (GUI status bar menu and Settings, TUI ⌃k › Status Bar Items…) show an item
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ItemInfo {
    pub id: Item,
    pub label: &'static str,
    /// What it shows, after the label (empty: the label says it)
    pub hint: &'static str,
    pub group: &'static str,
}

/// Every item, in the order the checklists list them and the status bar shows them (left to right)
pub const ITEMS: [ItemInfo; 8] = [
    ItemInfo { id: Item::NotNull, label: "Not null", hint: "cells with a value", group: "Selection" },
    ItemInfo { id: Item::Sum, label: "Sum", hint: "", group: "Selection" },
    ItemInfo { id: Item::Avg, label: "Avg", hint: "", group: "Selection" },
    ItemInfo { id: Item::Min, label: "Min", hint: "", group: "Selection" },
    ItemInfo { id: Item::Max, label: "Max", hint: "", group: "Selection" },
    ItemInfo { id: Item::Size, label: "Size", hint: "bytes, column limit", group: "Current cell" },
    ItemInfo { id: Item::ShownColumns, label: "Shown columns", hint: "cols 3–8 of 14", group: "Grid" },
    ItemInfo { id: Item::Position, label: "Position", hint: "Row 3, Col 4 · Rows 5–26 (22)", group: "Grid" },
];

/// On until the user changes them (and after Restore Defaults)
pub const DEFAULTS: [Item; 2] = [Item::ShownColumns, Item::Position];

/// The items on, from settings.json's value: the defaults when there is none (or it isn't a list); ids this version doesn't know
/// are skipped. In ITEMS order
pub fn items(v: Option<&Value>) -> Vec<Item> {
    match v {
        Some(Value::Array(a)) => sorted(a.iter().filter_map(|x| serde_json::from_value(x.clone()).ok())),
        _ => DEFAULTS.to_vec(),
    }
}

/// `on` with `item` turned on or off, in ITEMS order
pub fn toggled(on: &[Item], item: Item, show: bool) -> Vec<Item> {
    sorted(on.iter().copied().filter(|&i| i != item).chain(show.then_some(item)))
}

fn sorted(it: impl Iterator<Item = Item>) -> Vec<Item> {
    let mut v: Vec<Item> = it.collect();
    v.sort();
    v.dedup();
    v
}

/// One part of the status bar: a muted label (none for Position and Shown columns) and its value
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    pub id: Part,
    pub label: Option<String>,
    pub value: String,
}

/// What a segment shows: an item, or the column limit that goes with Size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    NotNull,
    Sum,
    Avg,
    Min,
    Max,
    Size,
    Limit,
    ShownColumns,
    Position,
}

/// The most selected cells Not null / Sum / Avg / Min / Max go over (a whole column of a large result stays quick); past it they cover
/// the first ones in grid order
pub const MAX_CELLS: usize = 200_000;

/// Which segment goes first when the bar is too narrow (the GUI's UTF-8 before them all); Position never goes
pub const DROP: [Part; 8] = [Part::Limit, Part::Max, Part::Min, Part::Avg, Part::Sum, Part::NotNull, Part::Size, Part::ShownColumns];

/// Rows or columns of a selection, 1-based: the first, the last and how many (they are one range when count = last − first + 1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Span {
    pub first: usize,
    pub last: usize,
    pub count: usize,
}

impl Span {
    /// Of 0-based indexes in ascending order, each once; None when there are none
    pub fn of(sorted: impl IntoIterator<Item = usize>) -> Option<Span> {
        let mut it = sorted.into_iter();
        let first = it.next()?;
        let (last, n) = it.fold((first, 1), |(_, n), x| (x, n + 1));
        Some(Span { first: first + 1, last: last + 1, count: n })
    }

    fn one(self) -> bool {
        self.count == 1
    }

    fn range(self) -> bool {
        self.last + 1 - self.first == self.count
    }
}

/// Position: one cell "Row 3, Col 4"; else the rows ("Row 3", "Rows 5–26 (22)", or "22 rows" when they aren't one range) and the
/// columns ("Col 4", "Cols 5–7", "3 cols") the selection touches
pub fn position(rows: Span, cols: Span) -> String {
    if rows.one() && cols.one() {
        return format!("Row {}, Col {}", rows.first, cols.first);
    }
    let r = if rows.one() {
        format!("Row {}", rows.first)
    } else if rows.range() {
        format!("Rows {}–{} ({})", rows.first, rows.last, rows.count)
    } else {
        format!("{} rows", rows.count)
    };
    let c = if cols.one() {
        format!("Col {}", cols.first)
    } else if cols.range() {
        format!("Cols {}–{}", cols.first, cols.last)
    } else {
        format!("{} cols", cols.count)
    };
    format!("{r} · {c}")
}

/// Not null, Sum, Avg, Min and Max of the selected cells: the cells with a value, and the numbers among them (a JSON number, or
/// text that is one: -12, 3.5, 1e3)
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub not_null: u64,
    pub nums: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

impl Stats {
    pub fn add(&mut self, v: &Value) {
        let n = match v {
            Value::Null => return,
            Value::Number(n) => n.as_f64(),
            Value::String(s) => number(s),
            _ => None,
        };
        self.not_null += 1;
        if let Some(n) = n {
            (self.min, self.max) = if self.nums == 0 { (n, n) } else { (self.min.min(n), self.max.max(n)) };
            self.nums += 1;
            self.sum += n;
        }
    }
}

/// Text that is a plain number: [-]digits[.digits][e[+-]digits], spaces around it allowed
fn number(s: &str) -> Option<f64> {
    let t = s.trim();
    let b = t.strip_prefix('-').unwrap_or(t).as_bytes();
    let digits = |i: usize| b[i.min(b.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut i = digits(0);
    if i == 0 {
        return None;
    }
    if b.get(i) == Some(&b'.') {
        let d = digits(i + 1);
        if d == 0 {
            return None;
        }
        i += 1 + d;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1 + usize::from(matches!(b.get(i + 1), Some(b'+' | b'-')));
        let d = digits(i);
        if d == 0 {
            return None;
        }
        i += d;
    }
    // 1e400 parses as infinity: not a number to add up
    (i == b.len()).then(|| t.parse().ok()).flatten().filter(|n: &f64| n.is_finite())
}

/// A number as the status bar shows it: digits grouped with commas, up to 4 decimals ("1,284,930.5")
pub fn num(n: f64) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let s = format!("{:.4}", n);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    let (neg, s) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let (int, frac) = s.split_once('.').map_or((s, None), |(i, f)| (i, Some(f)));
    let grouped = int.as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(",");
    // -0.00001 rounds to 0, not -0
    let neg = neg && (int != "0" || frac.is_some());
    format!("{}{grouped}{}", if neg { "-" } else { "" }, frac.map(|f| format!(".{f}")).unwrap_or_default())
}

fn plural(n: usize, one: &str) -> String {
    format!("{} {one}{}", num(n as f64), if n == 1 { "" } else { "s" })
}

/// The Size item's value for a cell of a column of type `type_name`: "11 bytes (7 chars)" (UTF-8, as sent), a binary value's
/// length ("2,048 bytes"; past the 64 bytes a grid receives, "over 64 bytes"), "NULL". None for numbers, dates / times and booleans
/// (their text isn't what is stored)
pub fn size(type_name: &str, value: &Value) -> Option<String> {
    let class = format::value_class(type_name, value);
    match (class, value) {
        (ValueClass::Null, _) => Some("NULL".into()),
        (ValueClass::Number | ValueClass::DateTime | ValueClass::Boolean, _) => None,
        (ValueClass::JsonBinary, Value::String(s)) if !format::is_json_type(type_name) && s.starts_with("0x") => {
            // The core sends binary as 0x and the hex of its first 64 bytes, then … when there is more
            let hex = s[2..].trim_end_matches('…');
            let n = hex.len() / 2;
            Some(if hex.len() < s.len() - 2 { format!("over {}", plural(n, "byte")) } else { plural(n, "byte") })
        }
        (_, Value::String(s)) => {
            let (b, c) = (s.len(), s.chars().count());
            Some(if b == c { plural(b, "byte") } else { format!("{} ({})", plural(b, "byte"), plural(c, "char")) })
        }
        (_, v) => Some(plural(v.to_string().len(), "byte")),
    }
}

/// The most bytes one character takes in a MySQL character set (the ones Qurry knows)
fn mysql_char_bytes(charset: &str) -> Option<usize> {
    let one = [
        "latin1", "latin2", "latin5", "latin7", "ascii", "binary", "cp1250", "cp1251", "cp1256", "cp1257", "cp850", "cp852", "cp866", "dec8", "hp8", "koi8r", "koi8u", "greek", "hebrew", "armscii8", "geostd8",
        "keybcs2", "macce", "macroman", "swe7", "tis620",
    ];
    Some(match charset {
        c if one.contains(&c) => 1,
        "ucs2" | "gbk" | "gb2312" | "big5" | "sjis" | "cp932" | "euckr" => 2,
        "utf8" | "utf8mb3" | "ujis" | "eucjpms" => 3,
        "utf8mb4" | "utf16" | "utf16le" | "utf32" | "gb18030" => 4,
        _ => return None,
    })
}

/// A column's declared limit, from its type as the table's metadata gives it (PostgreSQL's format_type, MySQL's COLUMN_TYPE) and,
/// for MySQL, its character set: (the type, what it allows), e.g. ("varchar(8)", "up to 8 chars (32 bytes in utf8mb4)"). None for
/// a type without one Qurry knows. PostgreSQL's bytes assume a UTF-8 database
pub fn limit(kind: DbKind, data_type: &str, charset: Option<&str>) -> Option<(String, String)> {
    let t = data_type.trim().to_lowercase().replace("character varying", "varchar").replace("character", "char");
    if t.contains('[') {
        return None;
    }
    let name = t.split('(').next().unwrap_or_default().trim().to_string();
    let n: Option<usize> = t.split_once('(').and_then(|(_, a)| a.split([')', ',']).next()).and_then(|a| a.trim().parse().ok());
    let label = n.map_or(name.clone(), |n| format!("{name}({n})"));
    let cs = charset.map(str::to_lowercase);
    let per = match kind {
        DbKind::Postgres => Some((4, "UTF-8".to_string())),
        DbKind::Mysql => cs.and_then(|c| mysql_char_bytes(&c).map(|m| (m, c))),
        _ => None,
    };
    let my = kind == DbKind::Mysql;
    let pg = kind == DbKind::Postgres;
    let up_to = |b: u64| format!("up to {} bytes", num(b as f64));
    let text = match (name.as_str(), n) {
        ("varchar" | "nvarchar" | "varchar2" | "nvarchar2", Some(n)) => match &per {
            Some((m, cs)) => format!("up to {n} chars ({} bytes in {cs})", num((n * m) as f64)),
            None => format!("up to {n} chars"),
        },
        ("char" | "nchar" | "bpchar", Some(n)) => match &per {
            Some((1, cs)) if my => format!("always {n} chars ({n} bytes in {cs})"),
            Some((m, cs)) if my => format!("always {n} chars ({n}–{} bytes in {cs})", num((n * m) as f64)),
            _ => format!("always {n} chars (padded)"),
        },
        ("varchar" | "text" | "bytea", None) if pg => "no limit (up to 1 GB)".into(),
        ("varbinary", Some(n)) => up_to(n as u64),
        ("binary", Some(n)) => format!("always {n} bytes (padded)"),
        ("tinyblob" | "tinytext", None) if my => up_to(255),
        ("blob" | "text", None) if my => up_to(65_535),
        ("mediumblob" | "mediumtext", None) if my => up_to(16_777_215),
        ("longblob" | "longtext", None) if my => up_to(4_294_967_295),
        _ => return None,
    };
    Some((label, text))
}

/// The current cell, for Size: its column's result type, its value, and the table column it is (when known)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub type_name: String,
    pub value: Value,
    #[serde(default)]
    pub column: Option<ColumnType>,
}

/// A table column's declared type (and MySQL character set), from the table's metadata
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnType {
    pub data_type: String,
    #[serde(default)]
    pub charset: Option<String>,
}

/// What a focused grid tells the status bar. `pos`: the selected rows and columns, or the current cell; `stats`: when 2+ cells are
/// selected; `cell`: the current cell when it is the only one; `shown`: the shown columns (first, last, total), 1-based
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub kind: Option<DbKind>,
    pub pos: Option<(Span, Span)>,
    pub stats: Option<Stats>,
    pub cell: Option<Cell>,
    pub shown: Option<(usize, usize, usize)>,
}

/// The segments to show, left to right, for the items on: the selection's, the cell's, the grid's (Position last, in the corner)
pub fn segments(on: &[Item], s: &Status) -> Vec<Segment> {
    let mut out = vec![];
    let mut add = |id: Part, label: Option<&str>, value: String| out.push(Segment { id, label: label.map(String::from), value });
    if let Some(st) = s.stats {
        if on.contains(&Item::NotNull) {
            add(Part::NotNull, Some("Not null"), num(st.not_null as f64));
        }
        if st.nums > 0 {
            let avg = st.sum / st.nums as f64;
            for (item, part, label, v) in [(Item::Sum, Part::Sum, "Sum", st.sum), (Item::Avg, Part::Avg, "Avg", avg), (Item::Min, Part::Min, "Min", st.min), (Item::Max, Part::Max, "Max", st.max)] {
                if on.contains(&item) {
                    add(part, Some(label), num(v));
                }
            }
        }
    } else if let Some(c) = s.cell.as_ref().filter(|_| on.contains(&Item::Size))
        && let Some(sz) = size(&c.type_name, &c.value)
    {
        add(Part::Size, Some("Size"), sz);
        let lim = c.column.as_ref().filter(|_| !c.value.is_null()).and_then(|t| limit(s.kind?, &t.data_type, t.charset.as_deref()));
        if let Some((ty, text)) = lim {
            add(Part::Limit, Some(&ty), text);
        }
    }
    if let Some(text) = s.shown.filter(|_| on.contains(&Item::ShownColumns)).and_then(|(f, l, t)| format::shown_cols(f, l, t)) {
        add(Part::ShownColumns, None, text);
    }
    if let Some((r, c)) = s.pos.filter(|_| on.contains(&Item::Position)) {
        add(Part::Position, None, position(r, c));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn span(first: usize, last: usize, count: usize) -> Span {
        Span { first, last, count }
    }

    #[test]
    fn items_from_settings() {
        assert_eq!(items(None), DEFAULTS);
        assert_eq!(items(Some(&json!("x"))), DEFAULTS, "not a list: the defaults");
        assert_eq!(items(Some(&json!([]))), [], "all turned off stays off");
        assert_eq!(items(Some(&json!(["position", "sum", "later", "sum", "notNull"]))), [Item::NotNull, Item::Sum, Item::Position], "unknown skipped, in ITEMS order, once");
        assert_eq!(toggled(&DEFAULTS, Item::Size, true), [Item::Size, Item::ShownColumns, Item::Position]);
        assert_eq!(toggled(&DEFAULTS, Item::Position, false), [Item::ShownColumns]);
        assert_eq!(toggled(&DEFAULTS, Item::Position, true), DEFAULTS);
        assert_eq!(serde_json::to_value(DEFAULTS).unwrap(), json!(["shownColumns", "position"]));
        // ITEMS lists each item once, in Item order (the status bar's order follows it)
        assert!(ITEMS.windows(2).all(|w| w[0].id < w[1].id));
    }

    #[test]
    fn position_words() {
        let cases = [
            ((span(3, 3, 1), span(4, 4, 1)), "Row 3, Col 4"),
            ((span(5, 26, 22), span(4, 4, 1)), "Rows 5–26 (22) · Col 4"),
            ((span(3, 42, 40), span(5, 7, 3)), "Rows 3–42 (40) · Cols 5–7"),
            ((span(1, 30, 22), span(3, 3, 1)), "22 rows · Col 3"),
            ((span(7, 7, 1), span(2, 9, 8)), "Row 7 · Cols 2–9"),
            ((span(2, 9, 2), span(1, 5, 2)), "2 rows · 2 cols"),
        ];
        for ((r, c), want) in cases {
            assert_eq!(position(r, c), want);
        }
        assert_eq!(Span::of([4, 5, 6]), Some(span(5, 7, 3)));
        assert_eq!(Span::of([0, 3]), Some(span(1, 4, 2)));
        assert_eq!(Span::of([]), None);
    }

    #[test]
    fn numbers() {
        for (n, want) in [(1284930.5, "1,284,930.5"), (10707.754166, "10,707.7542"), (0.0, "0"), (-0.00001, "0"), (-1234.5, "-1,234.5"), (98400.0, "98,400"), (91867108876892.0, "91,867,108,876,892"), (0.1 + 0.2, "0.3")] {
            assert_eq!(num(n), want, "{n}");
        }
        for (s, want) in [("12", Some(12.0)), (" -3.5 ", Some(-3.5)), ("1e3", Some(1000.0)), ("2E-2", Some(0.02)), ("1.", None), (".5", None), ("1e", None), ("0x10", None), ("12a", None), ("", None), ("-", None), ("NaN", None), ("inf", None), ("1e400", None)] {
            assert_eq!(number(s), want, "{s:?}");
        }
    }

    /// Not null counts every cell with a value (NULL left out); Sum / Avg / Min / Max only the numbers among them
    #[test]
    fn stats_over_the_selection() {
        let mut st = Stats::default();
        for v in [json!(null), json!(5), json!("7.5"), json!("abc"), json!(true), json!(null), json!(-2)] {
            st.add(&v);
        }
        assert_eq!(st, Stats { not_null: 5, nums: 3, sum: 10.5, min: -2.0, max: 7.5 });
        let on = [Item::NotNull, Item::Sum, Item::Avg, Item::Min, Item::Max, Item::Position];
        let s = Status { stats: Some(st), pos: Some((span(1, 7, 7), span(2, 2, 1))), ..Default::default() };
        let text: Vec<_> = segments(&on, &s).into_iter().map(|g| (g.label, g.value)).collect();
        let l = |s: &str| Some(s.to_string());
        assert_eq!(text, [(l("Not null"), "5".into()), (l("Sum"), "10.5".into()), (l("Avg"), "3.5".into()), (l("Min"), "-2".into()), (l("Max"), "7.5".into()), (None, "Rows 1–7 (7) · Col 2".into())]);
        // Only NULLs: Not null 0, no number items
        let s = Status { stats: Some(Stats::default()), ..Default::default() };
        assert_eq!(segments(&on, &s).iter().map(|g| g.id).collect::<Vec<_>>(), [Part::NotNull]);
        // Turned off: not shown
        let s = Status { stats: Some(st), ..Default::default() };
        assert!(segments(&DEFAULTS, &s).is_empty());
    }

    /// The Size item per type: bytes as sent, a binary value's length, nothing for numbers and dates
    #[test]
    fn size_per_type() {
        let cases: [(&str, Value, Option<&str>); 11] = [
            ("VARCHAR", json!("Grüße ☕"), Some("11 bytes (7 chars)")),
            ("text", json!("ab"), Some("2 bytes")),
            ("TEXT", json!("a"), Some("1 byte")),
            ("TEXT", json!(""), Some("0 bytes")),
            ("VARCHAR", json!(null), Some("NULL")),
            ("BYTEA", json!("0xDEADBEEF"), Some("4 bytes")),
            ("VARBINARY", json!(format!("0x{}…", "AB".repeat(64))), Some("over 64 bytes")),
            ("JSONB", json!("{\"a\": 1}"), Some("8 bytes")),
            ("INT4", json!(18), None),
            ("NUMERIC", json!("12.50"), None),
            ("TIMESTAMPTZ", json!("2024-01-02 03:04:05+00"), None),
        ];
        for (t, v, want) in cases {
            assert_eq!(size(t, &v).as_deref(), want, "{t} {v}");
        }
        assert_eq!(size("BOOL", &json!(true)), None);
        assert_eq!(size("TEXT", &json!(true)), None, "a JSON boolean whatever the column says");
    }

    /// The limit wording per type and engine (the design's frame: the type, then what it allows)
    #[test]
    fn limit_per_type_and_engine() {
        use DbKind::*;
        let cases: [(DbKind, &str, Option<&str>, Option<(&str, &str)>); 18] = [
            (Postgres, "character varying(8)", None, Some(("varchar(8)", "up to 8 chars (32 bytes in UTF-8)"))),
            (Postgres, "character(8)", None, Some(("char(8)", "always 8 chars (padded)"))),
            (Postgres, "text", None, Some(("text", "no limit (up to 1 GB)"))),
            (Postgres, "character varying", None, Some(("varchar", "no limit (up to 1 GB)"))),
            (Postgres, "bytea", None, Some(("bytea", "no limit (up to 1 GB)"))),
            (Postgres, "character varying(8)[]", None, None),
            (Postgres, "jsonb", None, None),
            (Mysql, "varchar(8)", Some("utf8mb4"), Some(("varchar(8)", "up to 8 chars (32 bytes in utf8mb4)"))),
            (Mysql, "varchar(8)", Some("latin1"), Some(("varchar(8)", "up to 8 chars (8 bytes in latin1)"))),
            (Mysql, "varchar(8)", None, Some(("varchar(8)", "up to 8 chars"))),
            (Mysql, "char(8)", Some("utf8mb4"), Some(("char(8)", "always 8 chars (8–32 bytes in utf8mb4)"))),
            (Mysql, "char(8)", Some("ascii"), Some(("char(8)", "always 8 chars (8 bytes in ascii)"))),
            (Mysql, "varbinary(16)", None, Some(("varbinary(16)", "up to 16 bytes"))),
            (Mysql, "binary(16)", None, Some(("binary(16)", "always 16 bytes (padded)"))),
            (Mysql, "blob", None, Some(("blob", "up to 65,535 bytes"))),
            (Mysql, "longtext", Some("utf8mb4"), Some(("longtext", "up to 4,294,967,295 bytes"))),
            (Mysql, "text", None, Some(("text", "up to 65,535 bytes"))),
            (Sqlite, "varchar(20)", None, Some(("varchar(20)", "up to 20 chars"))),
        ];
        for (k, t, cs, want) in cases {
            let got = limit(k, t, cs);
            assert_eq!(got.as_ref().map(|(a, b)| (a.as_str(), b.as_str())), want, "{k:?} {t} {cs:?}");
        }
        assert_eq!(limit(Mysql, "int(11)", None), None);
    }

    /// Size and its limit as two segments; the limit only with a known column and a value; nothing for 2+ cells (their stats instead)
    #[test]
    fn size_segments() {
        let on = [Item::Size, Item::Position];
        let cell = |v: Value, col: Option<ColumnType>| Some(Cell { type_name: "VARCHAR".into(), value: v, column: col });
        let col = || Some(ColumnType { data_type: "varchar(8)".into(), charset: Some("utf8mb4".into()) });
        let s = Status { kind: Some(DbKind::Mysql), cell: cell(json!("Grüße ☕"), col()), pos: Some((span(3, 3, 1), span(4, 4, 1))), ..Default::default() };
        let got: Vec<_> = segments(&on, &s).into_iter().map(|g| format!("{}{}", g.label.map(|l| format!("{l}: ")).unwrap_or_default(), g.value)).collect();
        assert_eq!(got, ["Size: 11 bytes (7 chars)", "varchar(8): up to 8 chars (32 bytes in utf8mb4)", "Row 3, Col 4"]);
        // A console column not mapped to a table column: the bytes only
        let s = Status { kind: Some(DbKind::Mysql), cell: cell(json!("ab"), None), ..Default::default() };
        assert_eq!(segments(&on, &s).iter().map(|g| g.value.as_str()).collect::<Vec<_>>(), ["2 bytes"]);
        // NULL: no size, no limit
        let s = Status { kind: Some(DbKind::Mysql), cell: cell(json!(null), col()), ..Default::default() };
        assert_eq!(segments(&on, &s).iter().map(|g| g.value.as_str()).collect::<Vec<_>>(), ["NULL"]);
        // 2+ cells selected: Size isn't shown
        let s = Status { stats: Some(Stats::default()), cell: cell(json!("ab"), col()), ..Default::default() };
        assert!(segments(&on, &s).is_empty());
        // Shown columns only when some column is out of view
        let s = Status { shown: Some((1, 5, 5)), ..Default::default() };
        assert!(segments(&DEFAULTS, &s).is_empty());
        let s = Status { shown: Some((3, 8, 14)), ..Default::default() };
        assert_eq!(segments(&DEFAULTS, &s)[0].value, "cols 3–8 of 14");
    }
}
