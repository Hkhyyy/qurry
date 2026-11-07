//! Display formatting shared by the result grids: fractional seconds aligned within a time column, digit grouping,
//! the value class of a cell and its alignment (Settings › Appearance › Alignment in grid; overrides kept in settings.json),
//! the colors of a JSON cell, the widths a user gave columns (resize). And the SQL editors' formatting, the same in the GUI console
//! (WASM) and the TUI editor: Reformat Code and the keyword case setting (as you type and in Reformat Code).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Columns whose fractional seconds are aligned: time types except INTERVAL
pub fn is_time_type(type_name: &str) -> bool {
    let t = type_name.to_uppercase();
    t.contains("TIME") && !t.contains("INTERVAL")
}

/// End of "hh:mm:ss" and the fractional-second digit count of a time value: `[dddd-dd-dd( |T)]hh:mm:ss[.f…]` at the start
fn time_frac(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let digits = |from: usize| b[from.min(b.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    let two = |i: usize| b.get(i..i + 2).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let time = |i: usize| two(i) && at(i + 2, b':') && two(i + 3) && at(i + 5, b':') && two(i + 6);
    let y = digits(0);
    let dated = y >= 4 && at(y, b'-') && two(y + 1) && at(y + 3, b'-') && two(y + 4) && (at(y + 6, b' ') || at(y + 6, b'T')) && time(y + 7);
    let start = if dated { y + 7 } else if time(0) { 0 } else { return None };
    let end = start + 8;
    Some((end, if at(end, b'.') { digits(end + 1) } else { 0 }))
}

/// The longest fractional-second digit count among a column's values (0 if none)
pub fn frac_width<'a>(values: impl IntoIterator<Item = &'a str>) -> usize {
    values.into_iter().filter_map(time_frac).map(|x| x.1).max().unwrap_or(0)
}

/// 03:19:15 → 03:19:15.000000 (pads with 0 up to `width` digits; other text unchanged)
pub fn pad_frac(s: &str, width: usize) -> String {
    match time_frac(s) {
        Some((end, d)) if d < width => {
            let frac = if d > 0 { &s[end + 1..end + 1 + d] } else { "" };
            let rest = &s[end + if d > 0 { 1 + d } else { 0 }..];
            format!("{}.{frac:0<width$}{rest}", &s[..end])
        }
        _ => s.to_string(),
    }
}

/// 1234567 → "1,234,567" (the TUI). Not shared with the GUI: its row and key counts keep Number.toLocaleString,
/// which groups digits as the user's locale does (1.234.567 in de-DE), so there is no WASM facade or golden for it
pub fn thousands(n: u64) -> String {
    n.to_string().as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(",")
}

/// The result grid's shown columns, 1-based and inclusive ("cols 3–8 of 14": the GUI status bar, the TUI grid's bottom border),
/// or None when every column fits
pub fn shown_cols(first: usize, last: usize, total: usize) -> Option<String> {
    (first > 1 || last < total).then(|| format!("cols {first}–{last} of {total}"))
}

/// What kind of value a grid cell holds, for its alignment (and the GUI's number / NULL colors)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueClass {
    String,
    Number,
    DateTime,
    Boolean,
    Null,
    JsonBinary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Right,
    Center,
    /// NULL only: where its column's type would sit (a NULL in a text column on the left, in a number column on the right)
    Column,
}

impl ValueClass {
    pub const ALL: [ValueClass; 6] = [ValueClass::String, ValueClass::Number, ValueClass::DateTime, ValueClass::Boolean, ValueClass::Null, ValueClass::JsonBinary];

    /// Numbers on the right, NULL where its column's type sits (in line with the values above it), everything else on the left
    pub fn default_align(self) -> Align {
        match self {
            ValueClass::Number => Align::Right,
            ValueClass::Null => Align::Column,
            _ => Align::Left,
        }
    }

    /// What Settings offers for this class: "Column type" for NULL only
    pub fn aligns(self) -> &'static [Align] {
        match self {
            ValueClass::Null => &[Align::Column, Align::Left, Align::Center, Align::Right],
            _ => &[Align::Left, Align::Center, Align::Right],
        }
    }
}

/// Class of a column from its type name (PostgreSQL, MySQL, SQLite, SQL Server, Oracle, ClickHouse …); arrays and unknown types are strings
pub fn type_class(type_name: &str) -> ValueClass {
    let mut t = type_name.trim().to_uppercase();
    // ClickHouse wrappers: Nullable(Int32) is an Int32
    for w in ["NULLABLE(", "LOWCARDINALITY("] {
        if let Some(inner) = t.strip_prefix(w) {
            t = inner.trim_end_matches(')').to_string();
        }
    }
    // Arrays, ranges and containers show as text: _int4, int4[], int4range, Array(Int32), Map(String, Int32), Tuple(…)
    if t.starts_with('_') || t.contains('[') || t.contains("RANGE") || ["ARRAY(", "MAP(", "TUPLE("].iter().any(|p| t.starts_with(p)) {
        return ValueClass::String;
    }
    // numeric(8,2) → NUMERIC; "timestamp with time zone" and "bigint unsigned" keep their words
    let name = t.split('(').next().unwrap_or_default().trim();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    // Oracle's BINARY_FLOAT / BINARY_DOUBLE (and PL/SQL BINARY_INTEGER) are numbers, not binary
    let binary = has(&["BINARY"]) && !name.starts_with("BINARY_");
    if has(&["JSON", "BYTEA", "BLOB"]) || binary || name == "RAW" || name == "IMAGE" {
        ValueClass::JsonBinary
    } else if has(&["DATE", "TIME", "INTERVAL"]) {
        ValueClass::DateTime
    } else if has(&["BOOL"]) {
        ValueClass::Boolean
    } else if (has(&["INT"]) && !has(&["POINT"])) || has(&["SERIAL", "NUMERIC", "DECIMAL", "FLOAT", "DOUBLE", "REAL", "MONEY", "OID", "NUMBER"]) || name == "DEC" {
        ValueClass::Number
    } else {
        ValueClass::String
    }
}

/// Class of one cell: NULL, then the value's own kind (a JSON number or boolean, whatever the column says), then the column's type.
/// Only the value's kind matters, not its content, so a caller may ask once per column and kind
pub fn value_class(type_name: &str, value: &Value) -> ValueClass {
    cell_class(type_class(type_name), value)
}

/// `value_class` with the column's `type_class` already worked out (the TUI grid works it out once per column, not per cell)
pub fn cell_class(column: ValueClass, value: &Value) -> ValueClass {
    match value {
        Value::Null => ValueClass::Null,
        Value::Bool(_) => ValueClass::Boolean,
        Value::Number(_) => ValueClass::Number,
        _ => column,
    }
}

/// A JSON column (json, jsonb, MySQL / Oracle / ClickHouse JSON): its text cells are colored. Binary types share its value class, not its colors
pub fn is_json_type(type_name: &str) -> bool {
    type_class(type_name) == ValueClass::JsonBinary && type_name.to_uppercase().contains("JSON")
}

/// What a piece of a JSON cell is, for its color (DataGrip's JSON colors; true, false and null are keywords).
/// The GUI gets it as an index in this order (src/lib/jsonSpans.ts)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JsonKind {
    Key,
    String,
    Number,
    Keyword,
    Punct,
}

/// Colored pieces of the first `max_chars` characters of a JSON text: byte ranges in order, the whitespace between them left out.
/// Reads tokens, not a whole document, so a value cut anywhere (what a cell shows of a large jsonb) still colors what is there;
/// from the first thing that isn't JSON on, the rest is plain
pub fn json_spans(text: &str, max_chars: usize) -> Vec<(usize, usize, JsonKind)> {
    let text = text.char_indices().nth(max_chars).map_or(text, |(b, _)| &text[..b]);
    let b = text.as_bytes();
    let mut out = Vec::new();
    // Open brackets, and whether the next string in the innermost object is a key
    let (mut open, mut key) = (Vec::new(), false);
    let mut i = 0;
    while i < b.len() {
        let start = i;
        i += 1;
        let kind = match b[start] {
            b' ' | b'\t' | b'\n' | b'\r' => continue,
            c @ (b'{' | b'[') => {
                open.push(c);
                key = c == b'{';
                JsonKind::Punct
            }
            c @ (b'}' | b']') => {
                if open.pop() != Some(if c == b'}' { b'{' } else { b'[' }) {
                    break;
                }
                JsonKind::Punct
            }
            b':' => {
                key = false;
                JsonKind::Punct
            }
            b',' => {
                key = open.last() == Some(&b'{');
                JsonKind::Punct
            }
            b'"' => {
                // '"' and '\' never occur inside a multi-byte character, so stepping bytes is safe; a cut string ends at the cut
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                if key { JsonKind::Key } else { JsonKind::String }
            }
            b'-' | b'0'..=b'9' => {
                while i < b.len() && matches!(b[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
                    i += 1;
                }
                JsonKind::Number
            }
            b'a'..=b'z' => {
                while i < b.len() && b[i].is_ascii_lowercase() {
                    i += 1;
                }
                // A keyword cut at the end counts too
                let w = &text[start..i];
                let words = ["true", "false", "null"];
                if !(words.contains(&w) || (i == b.len() && words.iter().any(|k| k.starts_with(w)))) {
                    break;
                }
                JsonKind::Keyword
            }
            _ => break,
        };
        out.push((start, i, kind));
    }
    out
}

/// The classes the user changed (settings.json "gridAlign"); a class not in it keeps its default
pub type Alignments = BTreeMap<ValueClass, Align>;

/// Reads saved overrides, skipping entries this version doesn't know (a class or alignment added later) or that the class doesn't
/// offer ("column" outside NULL), so the rest still apply
pub fn alignments(v: &Value) -> Alignments {
    let Some(m) = v.as_object() else { return Alignments::new() };
    m.iter()
        .filter_map(|(k, a)| Some((serde_json::from_value::<ValueClass>(Value::from(k.as_str())).ok()?, serde_json::from_value(a.clone()).ok()?)))
        .filter(|(c, a)| c.aligns().contains(a))
        .collect()
}

/// The class's setting (Settings › Appearance › Alignment shows it); NULL's may be `Align::Column`, which `cell_align` resolves
pub fn align(class: ValueClass, overrides: &Alignments) -> Align {
    overrides.get(&class).copied().unwrap_or(class.default_align())
}

/// The class a NULL on "Column type" follows: the column's `type_class`, or for a column without a type name (SQLite expressions,
/// MongoDB) the class of its first non-null value; with none, Number, so such a NULL stays on the right as in 0.3.0
pub fn column_class<'a>(type_name: &str, mut values: impl Iterator<Item = &'a Value>) -> ValueClass {
    if !type_name.trim().is_empty() {
        return type_class(type_name);
    }
    values.find(|v| !v.is_null()).map_or(ValueClass::Number, |v| cell_class(ValueClass::String, v))
}

/// Where a cell of `class` sits in a column of `column` (its `column_class`): its class's setting, or for a NULL on "Column type"
/// the column's (a `type_class` is never NULL, so this never returns `Align::Column`)
pub fn cell_align(column: ValueClass, class: ValueClass, overrides: &Alignments) -> Align {
    match align(class, overrides) {
        Align::Column => align(column, overrides),
        a => a,
    }
}

/// Column widths a user set (drag, fit, ⌃⌥← / ⌃⌥→ in the GUI, < > = in the TUI), by column name, in characters: the TUI draws a
/// column that many cells wide, the GUI that many grid characters plus its cell padding. A column not in it keeps its computed width
pub type ColumnWidths = BTreeMap<String, u16>;

/// The narrowest and widest a set width can be, and how much one narrow / widen step changes it
pub const COLUMN_WIDTH_MIN: u16 = 3;
pub const COLUMN_WIDTH_MAX: u16 = 200;
pub const COLUMN_WIDTH_STEP: i64 = 4;

/// `width` (characters) moved by `steps` narrow (-) / widen (+) steps, kept between the narrowest and the widest
pub fn column_width(width: i64, steps: i64) -> u16 {
    width.saturating_add(steps.saturating_mul(COLUMN_WIDTH_STEP)).clamp(COLUMN_WIDTH_MIN as i64, COLUMN_WIDTH_MAX as i64) as u16
}

/// The characters `s` takes in a grid, as a terminal draws it (East Asian wide characters and emoji count 2): what Fit sizes a column
/// to, so a width means the same in both apps (the GUI draws a wide character in about 1.4 grid characters, within 2)
pub fn text_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// The keys the set widths of a result's columns are kept under: the name, and for a name the result repeats (a join's two `id`
/// columns, `select 1 a, 2 a`) the name and its occurrence, `id#2`, so each column resizes on its own
pub fn column_keys<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    names
        .into_iter()
        .map(|n| {
            let k = seen.entry(n).or_default();
            *k += 1;
            if *k == 1 { n.to_string() } else { format!("{n}#{k}") }
        })
        .collect()
}

/// Reads saved widths (settings.json "columnWidths" › "widths"), skipping what isn't a whole number and keeping the rest in range
pub fn column_widths(v: &Value) -> ColumnWidths {
    let Some(m) = v.as_object() else { return ColumnWidths::new() };
    m.iter().filter_map(|(k, w)| Some((k.clone(), column_width(w.as_i64()?, 0)))).collect()
}

/// Settings.json "keywordCase" (GUI Settings › Appearance › Keyword case, TUI ⌃K › Keyword Case…): the letter case SQL keywords
/// get as you type and in Reformat Code. Off leaves typing alone, and Reformat Code writes keywords in upper case as before 0.3.2
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeywordCase {
    #[default]
    Off,
    Upper,
    Lower,
}

impl KeywordCase {
    pub const ALL: [KeywordCase; 3] = [KeywordCase::Off, KeywordCase::Upper, KeywordCase::Lower];

    fn apply(self, word: &str) -> String {
        match self {
            KeywordCase::Off => word.to_string(),
            KeywordCase::Upper => word.to_ascii_uppercase(),
            KeywordCase::Lower => word.to_ascii_lowercase(),
        }
    }
}

/// The words the keyword case changes, sorted: SQL keywords that are rarely names. Words tables often have as columns (`user`, `name`,
/// `type`, `key`, `first`, `range`, `filter` …) and function names stay as written, so typing them never changes them
const CASE_KEYWORDS: &[&str] = &[
    "add", "all", "alter", "analyze", "and", "any", "as", "asc", "begin", "between", "by", "cascade", "case", "cast", "check", "commit",
    "conflict", "constraint", "create", "cross", "current_date", "current_timestamp", "database", "default", "delete", "desc", "distinct",
    "do", "drop", "else", "end", "except", "exists", "explain", "false", "fetch", "for", "foreign", "from", "full", "grant", "group",
    "having", "if", "ilike", "in", "index", "inner", "insert", "intersect", "interval", "into", "is", "join", "lateral", "left", "like",
    "limit", "materialized", "not", "nothing", "null", "nulls", "offset", "on", "or", "order", "outer", "over", "partition", "primary",
    "recursive", "references", "rename", "returning", "revoke", "right", "rollback", "savepoint", "schema", "select", "set", "table",
    "then", "to", "true", "truncate", "union", "unique", "update", "using", "values", "view", "when", "where", "window", "with",
];

/// Whether toks[i] is a keyword the keyword case changes: a bare word (not quoted, not in a string or comment) of CASE_KEYWORDS that
/// isn't part of a name (`t.order`, `order.id`) or a variable (`@from`, `:from`); `key` only in PRIMARY KEY / FOREIGN KEY
fn cased_keyword(toks: &[crate::guard::Tok], i: usize) -> bool {
    let t = &toks[i];
    let prev = i.checked_sub(1).map(|p| &toks[p]);
    let next = toks.get(i + 1);
    let known = CASE_KEYWORDS.binary_search(&t.text.as_str()).is_ok() || (t.text == "key" && prev.is_some_and(|p| p.word && !p.quoted && matches!(p.text.as_str(), "primary" | "foreign")));
    t.word
        && !t.quoted
        && known
        && !prev.is_some_and(|p| p.text == "." || (p.end == t.start && matches!(p.text.as_str(), "@" | ":")))
        && !next.is_some_and(|n| n.text == ".")
}

/// `sql` with its keywords in `case` (Off: as written)
pub fn case_keywords(sql: &str, kind: crate::DbKind, case: KeywordCase) -> String {
    if case == KeywordCase::Off {
        return sql.to_string();
    }
    let toks = crate::guard::scan(sql, kind).toks;
    let (mut out, mut at) = (String::with_capacity(sql.len()), 0);
    for i in (0..toks.len()).filter(|&i| cased_keyword(&toks, i)) {
        let t = &toks[i];
        out.push_str(&sql[at..t.start]);
        out.push_str(&case.apply(&sql[t.start..t.end]));
        at = t.end;
    }
    out + &sql[at..]
}

/// Keyword case as you type: `end` (bytes) is where the character just typed sits (a space, a newline, punctuation). When it ends a
/// keyword written in another case (outside strings, comments and quoted names, not before a `.`): the keyword's start and new text
pub fn keyword_case_at(text: &str, end: usize, kind: crate::DbKind, case: KeywordCase) -> Option<(usize, String)> {
    if case == KeywordCase::Off || !holds_sql(kind) || !text.is_char_boundary(end) {
        return None;
    }
    // With the typed character: after a `.` the word is a name, and a letter would have continued it
    let stop = end + text[end..].chars().next()?.len_utf8();
    let toks = crate::guard::scan(&text[..stop], kind).toks;
    let i = toks.iter().rposition(|t| t.end <= end)?;
    let t = &toks[i];
    let word = &text[t.start..t.end];
    let cased = case.apply(word);
    (t.end == end && cased != word && cased_keyword(&toks, i)).then_some((t.start, cased))
}

/// Reformat Code's change: `text[start..end]` (bytes) becomes `text`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reformatted {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Databases whose consoles hold SQL (not Redis commands, the MongoDB shell or vector database requests)
pub fn holds_sql(kind: crate::DbKind) -> bool {
    !matches!(kind, crate::DbKind::Redis | crate::DbKind::Mongodb) && !kind.is_vector()
}

/// Reformat Code (GUI ⌘⌥l, TUI ⌥⇧F): the selection (`sel`, bytes), or without one the statement at the caret, one clause per line
/// with 4-space indents and keywords in `case` (Off: upper case). None when that changes nothing
pub fn reformat(text: &str, sel: Option<(usize, usize)>, caret: usize, kind: crate::DbKind, case: KeywordCase) -> Option<Reformatted> {
    if !holds_sql(kind) {
        return None;
    }
    let (s, e) = match sel.filter(|(s, e)| s < e) {
        Some(r) => r,
        None => crate::sql::statement_span(text, caret, kind)?,
    };
    let part = text.get(s..e)?;
    // The statement's own text, not the blank lines around it
    let start = s + (part.len() - part.trim_start().len());
    let end = s + part.trim_end().len();
    if start >= end {
        return None;
    }
    let out = format_sql(&text[start..end], kind, if case == KeywordCase::Off { KeywordCase::Upper } else { case });
    (out != text[start..end]).then_some(Reformatted { start, end, text: out })
}

/// sqlformat's layout. Strings and quoted names go to it as stand-ins and come back as written: it would split E'…' and $$…$$
/// bodies, which it doesn't read as strings
fn format_sql(sql: &str, kind: crate::DbKind, case: KeywordCase) -> String {
    use crate::DbKind;
    let toks = crate::guard::scan(sql, kind).toks;
    let (mut masked, mut kept, mut at) = (String::with_capacity(sql.len()), Vec::new(), 0);
    for (i, t) in toks.iter().enumerate() {
        if !(t.quoted || t.text == "'") {
            continue;
        }
        // A prefix written against the quote belongs to the literal: E'…', X'0f', N'…', _utf8mb4'…'
        let start = match i.checked_sub(1).map(|p| &toks[p]) {
            Some(p) if p.word && !p.quoted && p.end == t.start && t.text == "'" => p.start,
            _ => t.start,
        };
        masked.push_str(&sql[at..start]);
        masked.push_str(&format!("'{STAND_IN}{}{STAND_IN}'", kept.len()));
        kept.push(&sql[start..t.end]);
        at = t.end;
    }
    masked.push_str(&sql[at..]);
    let dialect = match kind {
        DbKind::Postgres | DbKind::Duckdb => sqlformat::Dialect::PostgreSql,
        DbKind::Mssql => sqlformat::Dialect::SQLServer,
        _ => sqlformat::Dialect::Generic,
    };
    let o = sqlformat::FormatOptions { indent: sqlformat::Indent::Spaces(4), lines_between_queries: 2, dialect, ..Default::default() };
    let formatted = sqlformat::format(&masked, &sqlformat::QueryParams::None, &o);
    let (mut out, mut rest) = (String::with_capacity(formatted.len()), formatted.as_str());
    let open = format!("'{STAND_IN}");
    while let Some(p) = rest.find(&open) {
        let after = &rest[p + open.len()..];
        let back = after.split_once(STAND_IN).and_then(|(n, tail)| Some((*kept.get(n.parse::<usize>().ok()?)?, tail.strip_prefix('\'')?)));
        match back {
            Some((orig, tail)) => {
                out.push_str(&rest[..p]);
                out.push_str(orig);
                rest = tail;
            }
            None => {
                out.push_str(&rest[..p + open.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    case_keywords(out.trim_end(), kind, case)
}

/// Marks a string's stand-in while sqlformat lays the SQL out (a private-use character, not in SQL)
const STAND_IN: char = '\u{e000}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shown_cols_only_when_some_column_is_out_of_view() {
        assert_eq!(shown_cols(3, 8, 14).as_deref(), Some("cols 3–8 of 14"));
        assert_eq!(shown_cols(1, 13, 14).as_deref(), Some("cols 1–13 of 14"));
        assert_eq!(shown_cols(1, 14, 14), None);
    }

    #[test]
    fn column_widths_stay_in_range() {
        assert_eq!(column_width(16, 3), 28);
        assert_eq!(column_width(5, -1), COLUMN_WIDTH_MIN);
        assert_eq!(column_width(198, 1), COLUMN_WIDTH_MAX);
        let saved = serde_json::json!({ "memo": 53, "tiny": 1, "wide": 5000, "odd": "12", "half": 1.5 });
        assert_eq!(column_widths(&saved), ColumnWidths::from([("memo".into(), 53), ("tiny".into(), COLUMN_WIDTH_MIN), ("wide".into(), COLUMN_WIDTH_MAX)]));
        assert!(column_widths(&serde_json::json!([1])).is_empty());
    }

    #[test]
    fn column_widths_count_display_columns_and_repeated_names() {
        assert_eq!(text_width("memo"), 4);
        assert_eq!(text_width("고객 요청"), 9);
        assert_eq!(text_width("🙏"), 2);
        assert_eq!(column_keys(["id", "name", "id", "id"]), ["id", "name", "id#2", "id#3"]);
    }

    #[test]
    fn time_values_are_padded() {
        assert_eq!(pad_frac("2026-06-06 03:19:15 +09:00", 6), "2026-06-06 03:19:15.000000 +09:00");
        assert_eq!(pad_frac("2026-06-08 10:52:05.27 +09:00", 6), "2026-06-08 10:52:05.270000 +09:00");
        assert_eq!(pad_frac("12:00:01", 3), "12:00:01.000");
        assert_eq!(pad_frac("2026-06-08T10:52:05.123456", 6), "2026-06-08T10:52:05.123456");
        assert_eq!(pad_frac("hello", 6), "hello");
        // A date part must be digits (as the GUI's pattern): no padding after other text
        assert_eq!(pad_frac("abcd-ef-gh 10:00:00", 3), "abcd-ef-gh 10:00:00");
        assert_eq!(frac_width(["03:19:15.5", "x", "09:38:06.521101"]), 6);
        assert!(is_time_type("timestamp with time zone") && !is_time_type("interval"));
    }

    #[test]
    fn type_classes() {
        use ValueClass::*;
        let cases = [
            ("int4", Number), ("INT8", Number), ("bigint unsigned", Number), ("numeric(8,2)", Number), ("DOUBLE PRECISION", Number), ("float8", Number),
            ("money", Number), ("oid", Number), ("NUMBER", Number), ("BINARY_DOUBLE", Number), ("BINARY_FLOAT", Number), ("BINARY_INTEGER", Number), ("UInt64", Number), ("Nullable(Int32)", Number), ("Decimal(10, 2)", Number), ("serial", Number),
            ("timestamptz", DateTime), ("timestamp with time zone", DateTime), ("DATE", DateTime), ("DateTime64(3)", DateTime), ("interval", DateTime), ("timetz", DateTime),
            ("bool", Boolean), ("BOOLEAN", Boolean),
            ("jsonb", JsonBinary), ("JSON", JsonBinary), ("bytea", JsonBinary), ("LONGBLOB", JsonBinary), ("varbinary(16)", JsonBinary), ("RAW", JsonBinary),
            ("text", String), ("varchar(60)", String), ("uuid", String), ("order_status", String), ("", String),
            // Arrays and containers of numbers are text; POINT is not an INT
            ("_int4", String), ("int4[]", String), ("Array(Int32)", String), ("Map(String, Int32)", String), ("int4range", String), ("tstzrange", String), ("point", String), ("vector", String),
        ];
        for (t, want) in cases {
            assert_eq!(type_class(t), want, "{t}");
        }
    }

    #[test]
    fn value_class_reads_null_and_json_kind_first() {
        use serde_json::json;
        assert_eq!(value_class("int4", &Value::Null), ValueClass::Null);
        assert_eq!(value_class("text", &Value::Null), ValueClass::Null);
        // NUMERIC and big INT8 arrive as text, a typeless SQLite expression as a number
        assert_eq!(value_class("NUMERIC", &json!("12.50")), ValueClass::Number);
        assert_eq!(value_class("", &json!(3)), ValueClass::Number);
        assert_eq!(value_class("", &json!(true)), ValueClass::Boolean);
        assert_eq!(value_class("jsonb", &json!("{\"k\":1}")), ValueClass::JsonBinary);
    }

    #[test]
    fn defaults_put_numbers_on_the_right_and_null_where_its_column_sits() {
        let none = Alignments::new();
        let got: Vec<Align> = ValueClass::ALL.iter().map(|&c| align(c, &none)).collect();
        assert_eq!(got, [Align::Left, Align::Right, Align::Left, Align::Left, Align::Column, Align::Left]);
        let null_in = |t: &str, o: &Alignments| cell_align(type_class(t), value_class(t, &Value::Null), o);
        let types = ["text", "int4", "timestamptz", "bool", "jsonb"];
        assert_eq!(types.map(|t| null_in(t, &none)), [Align::Left, Align::Right, Align::Left, Align::Left, Align::Left]);
        // It follows the column's class as the user set it
        let number_centered = alignments(&serde_json::json!({ "number": "center" }));
        assert_eq!(null_in("int4", &number_centered), Align::Center);
        // NULL saved as Left or Right (0.3.0 and later) stays there in every column
        let left = alignments(&serde_json::json!({ "null": "left" }));
        assert_eq!(types.map(|t| null_in(t, &left)), [Align::Left; 5]);
        let right = alignments(&serde_json::json!({ "null": "right" }));
        assert_eq!(types.map(|t| null_in(t, &right)), [Align::Right; 5]);
        assert_eq!(cell_align(ValueClass::String, ValueClass::Number, &left), Align::Right);
        // A column without a type name (SQLite expression, MongoDB): a NULL follows the column's first non-null value
        use serde_json::json;
        let untyped = |vals: &[Value], o: &Alignments| cell_align(column_class("", vals.iter()), ValueClass::Null, o);
        assert_eq!(untyped(&[Value::Null, json!(3), json!("a")], &none), Align::Right);
        assert_eq!(untyped(&[json!("a"), json!(3)], &none), Align::Left);
        assert_eq!(untyped(&[json!(true)], &none), Align::Left);
        assert_eq!(untyped(&[Value::Null], &none), Align::Right);
        assert_eq!(untyped(&[json!(3)], &left), Align::Left);
        // A typed column goes by its type, whatever its values
        assert_eq!(column_class("text", [json!(3)].iter()), ValueClass::String);
    }

    #[test]
    fn saved_alignments_skip_what_this_version_does_not_know() {
        let a = alignments(&serde_json::json!({ "number": "center", "uuid": "left", "string": "justify", "boolean": "right" }));
        assert_eq!(a, Alignments::from([(ValueClass::Number, Align::Center), (ValueClass::Boolean, Align::Right)]));
        assert!(alignments(&serde_json::json!("left")).is_empty());
        // "column" is a NULL choice only
        let c = alignments(&serde_json::json!({ "null": "column", "string": "column" }));
        assert_eq!(c, Alignments::from([(ValueClass::Null, Align::Column)]));
        assert_eq!(serde_json::to_value(&a).unwrap(), serde_json::json!({ "number": "center", "boolean": "right" }));
    }

    #[test]
    fn json_types_are_json_columns_only() {
        for t in ["json", "jsonb", "JSON", "JSONB", "Nullable(JSON)"] {
            assert!(is_json_type(t), "{t}");
        }
        // Binary shares the value class; arrays of JSON are text
        for t in ["bytea", "LONGBLOB", "varbinary(16)", "_jsonb", "jsonb[]", "text", ""] {
            assert!(!is_json_type(t), "{t}");
        }
    }

    #[test]
    fn json_spans_never_panic_and_stay_in_order_on_any_cut() {
        // Every prefix and every max_chars of texts with multi-byte characters, escapes and broken JSON (the cases are in goldens/jsonspans.json)
        for text in ["{\"이름\":\"🎉\\\"x\",\"n\":[-1.5e3,true,null]}", "[\"a\\", "{\"a\":1}}]x", "\u{feff}{}", "\"\\🎉\""] {
            for (cut, _) in text.char_indices().chain([(text.len(), ' ')]) {
                let t = &text[..cut];
                for max in 0..=t.chars().count() + 1 {
                    let mut at = 0;
                    for (s, e, _) in json_spans(t, max) {
                        assert!(at <= s && s < e && e <= t.len() && t.is_char_boundary(s) && t.is_char_boundary(e), "{t:?} max {max}");
                        assert!(t[..e].chars().count() <= max, "{t:?} max {max}");
                        at = e;
                    }
                }
            }
        }
    }

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    use crate::DbKind::{Mysql, Postgres};
    use KeywordCase::{Lower, Off, Upper};

    #[test]
    fn case_keywords_are_sorted_lowercase_words() {
        assert!(CASE_KEYWORDS.windows(2).all(|w| w[0] < w[1]), "CASE_KEYWORDS must stay sorted (binary search)");
        assert!(CASE_KEYWORDS.iter().all(|k| k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')));
    }

    /// Keywords change case; strings, comments, quoted names, names after or before a dot, variables and look-alike names stay
    #[test]
    fn keyword_case_leaves_strings_comments_and_names() {
        let sql = "select u.user, t.order, \"from\", 'select from' as x -- where\nfrom users u /* join */ where order.id = @from and Name is not null";
        assert_eq!(
            case_keywords(sql, Postgres, Upper),
            "SELECT u.user, t.order, \"from\", 'select from' AS x -- where\nFROM users u /* join */ WHERE order.id = @from AND Name IS NOT NULL"
        );
        assert_eq!(case_keywords("SELECT `select` FROM t # FROM\nWHERE a = 'it\\'s FROM'", Mysql, Lower), "select `select` from t # FROM\nwhere a = 'it\\'s FROM'");
        assert_eq!(case_keywords("select $$ from $$, e'\\' from' from t", Postgres, Upper), "SELECT $$ from $$, e'\\' from' FROM t");
        // Columns named like non-reserved keywords, functions, and key outside PRIMARY / FOREIGN KEY
        assert_eq!(case_keywords("select key, value, type, count(*) from kv", Postgres, Upper), "SELECT key, value, type, count(*) FROM kv");
        assert_eq!(case_keywords("id int primary key, foreign key (a) references b", Postgres, Upper), "id int PRIMARY KEY, FOREIGN KEY (a) REFERENCES b");
        assert_eq!(case_keywords("select 1", Postgres, Off), "select 1");
    }

    /// As you type: the word the typed delimiter ends, when it's a keyword in another case, outside strings, comments and quoted names
    #[test]
    fn keyword_case_as_you_type() {
        let at = |text: &str, case| keyword_case_at(text, text.len() - 1, Postgres, case);
        assert_eq!(at("select ", Upper), Some((0, "SELECT".into())));
        assert_eq!(at("SELECT a\nFROM\n", Lower), Some((9, "from".into())));
        assert_eq!(at("select a from t where x in(", Upper), Some((24, "IN".into())));
        assert_eq!(at("select a,", Upper), None);
        assert_eq!(at("SELECT ", Upper), None);
        assert_eq!(at("select ", Off), None);
        // In a string, a comment, a quoted name; a name before a dot or after one
        assert_eq!(at("select 'a from ", Upper), None);
        assert_eq!(at("select 1 -- from ", Upper), None);
        assert_eq!(at("select 1 /* from ", Upper), None);
        assert_eq!(at("select \"from ", Upper), None);
        assert_eq!(at("select order.", Upper), None);
        assert_eq!(at("select t.order ", Upper), None);
        assert_eq!(at("select user ", Upper), None);
        // A string closed before the word: the word is code again
        assert_eq!(at("select 'x' from ", Upper), Some((11, "FROM".into())));
        // The typed character isn't the last one (typing in the middle), multi-byte text before it
        assert_eq!(keyword_case_at("select '한글' from t", 20, Postgres, Upper), Some((16, "FROM".into())));
        assert_eq!(keyword_case_at("select 1 from t", 9, Postgres, Upper), None);
        // Not SQL
        assert_eq!(keyword_case_at("select ", 6, crate::DbKind::Redis, Upper), None);
    }

    /// Reformat Code: the statement at the caret (not the others, the blank lines around it kept), or the selection; strings,
    /// E'…' and $$…$$ bodies as written
    #[test]
    fn reformat_lays_out_the_statement_at_the_caret_or_the_selection() {
        let text = "select 1;\n\nselect a, b from t where x = 'it''s' and y = 2\n\nselect 3";
        let r = reformat(text, None, 15, Postgres, Off).unwrap();
        assert_eq!((r.start, r.end), (11, 57));
        assert_eq!(r.text, "SELECT\n    a,\n    b\nFROM\n    t\nWHERE\n    x = 'it''s'\n    AND y = 2");
        assert_eq!(reformat(text, None, 15, Postgres, Lower).unwrap().text, "select\n    a,\n    b\nfrom\n    t\nwhere\n    x = 'it''s'\n    and y = 2");
        let sel = reformat(text, Some((0, 8)), 0, Postgres, Off).unwrap();
        assert_eq!((sel.start, sel.end, sel.text.as_str()), (0, 8, "SELECT\n    1"));
        let pg = "select e'a\\'b', $body$ select  1 from t $body$ as f from t";
        assert_eq!(reformat(pg, None, 0, Postgres, Upper).unwrap().text, "SELECT\n    e'a\\'b',\n    $body$ select  1 from t $body$ AS f\nFROM\n    t");
        let my = "select `order`, \"a\\\"b\", x'0f' from t # from\nwhere t.from = 1";
        assert_eq!(reformat(my, None, 0, Mysql, Off).unwrap().text, "SELECT\n    `order`,\n    \"a\\\"b\",\n    x'0f'\nFROM\n    t # from\nWHERE\n    t.from = 1");
        // Already formatted, nothing there, not SQL
        assert_eq!(reformat("SELECT\n    1", None, 0, Postgres, Off), None);
        assert_eq!(reformat("  \n", None, 1, Postgres, Off), None);
        assert_eq!(reformat("GET k", None, 0, crate::DbKind::Redis, Off), None);
    }
}
