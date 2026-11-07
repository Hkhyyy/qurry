//! Display formatting shared by the result grids: fractional seconds aligned within a time column, digit grouping,
//! the value class of a cell and its alignment (Settings › Appearance › Alignment in grid; overrides kept in settings.json),
//! the colors of a JSON cell.

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
}
