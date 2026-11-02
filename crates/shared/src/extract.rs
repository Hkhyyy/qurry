//! Grid cells as text: the IN list and DataGrip's Data Extractors (copy formats). Cells are JSON values as the grids hold them.
use crate::{sql, DbKind};
use serde::Deserialize;
use serde_json::Value;

/// The value Set DEFAULT puts in a GUI cell (written as DEFAULT, never as a string)
pub const DEFAULT_TOKEN: &str = "\u{0}DEFAULT";

/// Where the GUI writes values differently from the TUI (D2; the inList rows of crates/shared/tests/DIVERGENCES.md and number cells)
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// IN list: in a numeric column only -?digits[.digits][e±digits] text stays bare (the GUI). Otherwise any number text does,
    /// such as +4 or .5 (the TUI)
    pub strict_numbers: bool,
    /// Number cells as JavaScript's String() writes them, as the GUI's grid shows them (1e16 as 10000000000000000).
    /// Otherwise as serde_json writes them, as the TUI's grid shows them
    pub js_numbers: bool,
}

/// Data Extractor formats, named as the GUI's menu shows them
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum Format {
    #[serde(rename = "CSV")]
    Csv,
    #[serde(rename = "TSV")]
    Tsv,
    #[serde(rename = "SQL Inserts")]
    SqlInserts,
    #[serde(rename = "SQL Updates")]
    SqlUpdates,
    #[serde(rename = "Where Clause")]
    WhereClause,
    #[serde(rename = "SQL IN List")]
    SqlInList,
    #[serde(rename = "JSON")]
    Json,
    #[serde(rename = "Markdown")]
    Markdown,
}

impl Format {
    pub const ALL: [Format; 8] = [Format::Csv, Format::Tsv, Format::SqlInserts, Format::SqlUpdates, Format::WhereClause, Format::SqlInList, Format::Json, Format::Markdown];

    pub fn label(self) -> &'static str {
        match self {
            Format::Csv => "CSV",
            Format::Tsv => "TSV",
            Format::SqlInserts => "SQL Inserts",
            Format::SqlUpdates => "SQL Updates",
            Format::WhereClause => "Where Clause",
            Format::SqlInList => "SQL IN List",
            Format::Json => "JSON",
            Format::Markdown => "Markdown",
        }
    }
}

/// The selected cells and what the formats need around them
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rows {
    /// Selected columns
    pub names: Vec<String>,
    pub types: Vec<String>,
    /// Selected cells, one array per row
    pub vals: Vec<Vec<Value>>,
    /// SQL Updates: (key column, value) pairs per row, empty when the table has no key
    pub keys: Vec<Vec<(String, Value)>>,
    pub table_ref: String,
    /// A line of column names at the top of CSV and TSV
    pub header: bool,
}

fn is_default(v: &Value) -> bool {
    v.as_str() == Some(DEFAULT_TOKEN)
}

/// A cell as plain text: NULL is empty, DEFAULT is DEFAULT
fn raw(v: &Value, opts: Options) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) if s == DEFAULT_TOKEN => "DEFAULT".into(),
        other => raw_text(other, opts),
    }
}

/// A string literal of `kind`: '…' with quotes doubled, and \ doubled where the server reads it as an escape (MySQL,
/// ClickHouse). Elsewhere a \ stays as it is (PostgreSQL: standard_conforming_strings, on by default)
fn quote(kind: DbKind, s: &str) -> String {
    if sql::backslash_escapes(kind) { sql::literal(kind, Some(s)) } else { format!("'{}'", s.replace('\'', "''")) }
}

/// A cell as an SQL literal of `kind` (Redis: a JSON string)
fn literal(kind: DbKind, v: &Value, opts: Options) -> String {
    match (kind, v) {
        (DbKind::Redis, Value::Null) => "\"\"".into(),
        (DbKind::Redis, v) => Value::from(raw_text(v, opts)).to_string(),
        (_, Value::Null) => "NULL".into(),
        (_, v) => quote(kind, &raw_text(v, opts)),
    }
}

/// Text of a non-NULL cell (a DEFAULT token as it is)
fn raw_text(v: &Value, opts: Options) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) if opts.js_numbers => js_number(n),
        other => other.to_string(),
    }
}

/// A number as JavaScript's Number.prototype.toString writes it: integers below 1e21 in full, other exponents as e+21 and
/// e-7. Integers serde_json holds exactly (i64, u64) print the same either way
fn js_number(n: &serde_json::Number) -> String {
    let Some(f) = n.as_f64().filter(|_| n.is_f64()) else { return n.to_string() };
    if f == 0.0 {
        return "0".into();
    }
    // Shortest round-trip digits (as JavaScript picks them) and the decimal point's place: 0.ddd × 10^point
    let e = format!("{:e}", f.abs());
    let (mantissa, exp) = e.split_once('e').unwrap();
    let digits = mantissa.replace('.', "");
    let (k, point) = (digits.len() as i32, exp.parse::<i32>().unwrap() + 1);
    let body = if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat(-point as usize))
    } else {
        let frac = if k > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{frac}e{}{}", &digits[..1], if point > 0 { "+" } else { "-" }, (point - 1).abs())
    };
    if f < 0.0 { format!("-{body}") } else { body }
}

/// Numeric and boolean column types (PostgreSQL, MySQL); not interval and the like
pub fn numeric_type(t: &str) -> bool {
    const TYPES: [&str; 25] = [
        "int", "int2", "int4", "int8", "integer", "tinyint", "smallint", "mediumint", "bigint", "serial", "serial2", "serial4", "serial8", "smallserial", "bigserial", "numeric", "decimal", "real", "float", "float4", "float8",
        "double", "bool", "boolean", "oid",
    ];
    let t = t.trim().to_lowercase();
    TYPES.iter().any(|w| t.strip_prefix(w).is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')))
}

fn number_text(s: &str, strict: bool) -> bool {
    let t = s.strip_prefix('-').unwrap_or(s);
    if !strict {
        return !t.is_empty() && t.parse::<f64>().is_ok() && t.chars().all(|c| c.is_ascii_digit() || ".eE+-".contains(c));
    }
    let digits = |s: &str| s.len() - s.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let n = digits(t);
    let mut rest = &t[n..];
    if n == 0 {
        return false;
    }
    if let Some(r) = rest.strip_prefix('.') {
        if digits(r) == 0 {
            return false;
        }
        rest = &r[digits(r)..];
    }
    if let Some(r) = rest.strip_prefix(['e', 'E']) {
        let r = r.strip_prefix(['+', '-']).unwrap_or(r);
        return digits(r) > 0 && digits(r) == r.len();
    }
    rest.is_empty()
}

/// Rows as a list to put after IN (strings as `kind` reads them), and how many items it has: ('a', 'b'), or one tuple per row with several columns
/// (('a', 1), ('b', 2)). Rows of only NULL and duplicates are left out. In numeric columns (`types`, one per column) number text stays bare
/// (numeric and bigint arrive as text); elsewhere it is quoted (010 in a text column)
pub fn in_list(rows: &[Vec<Value>], types: &[String], kind: DbKind, opts: Options) -> (String, usize) {
    let lit = |c: usize, v: &Value| match v {
        Value::Null => "NULL".to_string(),
        Value::Number(_) | Value::Bool(_) => raw_text(v, opts),
        Value::String(s) if types.get(c).is_some_and(|t| numeric_type(t)) && number_text(s, opts.strict_numbers) => s.clone(),
        other => quote(kind, &raw_text(other, opts)),
    };
    let mut seen = std::collections::HashSet::new();
    let items: Vec<String> = rows
        .iter()
        .filter(|r| r.iter().any(|v| !v.is_null()))
        .map(|r| if r.len() == 1 { lit(0, &r[0]) } else { format!("({})", r.iter().enumerate().map(|(c, v)| lit(c, v)).collect::<Vec<_>>().join(", ")) })
        .filter(|x| seen.insert(x.clone()))
        .collect();
    let n = items.len();
    (format!("({})", if n == 0 { "NULL".into() } else { items.join(", ") }), n)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// JSON.stringify(rows, null, 2): an object per row with the column names as keys (a repeated name keeps its first place and
/// its last value; names that are array indices come first, ascending, as in a JavaScript object)
fn json_rows(x: &Rows, opts: Options) -> String {
    let index = |k: &str| k.parse::<u32>().ok().filter(|n| *n < u32::MAX && n.to_string() == k);
    let mut order: Vec<usize> = vec![];
    for (j, n) in x.names.iter().enumerate() {
        match order.iter().position(|&o| x.names[o] == *n) {
            Some(p) => order[p] = j,
            None => order.push(j),
        }
    }
    order.sort_by_key(|&j| index(&x.names[j]).map_or((1, 0), |n| (0, n)));
    if x.vals.is_empty() {
        return "[]".into();
    }
    let obj = |v: &Vec<Value>| {
        if order.is_empty() {
            return "  {}".to_string();
        }
        let fields = order.iter().map(|&j| {
            let c = match v.get(j).filter(|c| !is_default(c)) {
                Some(Value::Number(n)) if opts.js_numbers => js_number(n),
                Some(c) => c.to_string(),
                None => "null".into(),
            };
            format!("    {}: {c}", Value::from(x.names[j].as_str()))
        });
        format!("  {{\n{}\n  }}", fields.collect::<Vec<_>>().join(",\n"))
    };
    format!("[\n{}\n]", x.vals.iter().map(obj).collect::<Vec<_>>().join(",\n"))
}

/// Rows as text in a Data Extractor format; identifiers and literals as `kind` writes them
pub fn rows(fmt: Format, x: &Rows, kind: DbKind, opts: Options) -> String {
    let q = |s: &str| sql::quote_ident(kind, s);
    let cell = |v: &Vec<Value>, j: usize| v.get(j).cloned().unwrap_or(Value::Null);
    let lit = |v: &Value| if is_default(v) { "DEFAULT".to_string() } else { literal(kind, v, opts) };
    let lines = |head: Option<String>, body: Vec<String>| head.into_iter().chain(body).collect::<Vec<_>>().join("\n");
    let names = x.names.iter();
    match fmt {
        Format::Tsv => lines(x.header.then(|| x.names.join("\t")), x.vals.iter().map(|v| v.iter().map(|c| raw(c, opts)).collect::<Vec<_>>().join("\t")).collect()),
        Format::Csv => lines(
            x.header.then(|| x.names.iter().map(|n| csv_field(n)).collect::<Vec<_>>().join(",")),
            x.vals.iter().map(|v| v.iter().map(|c| csv_field(&raw(c, opts))).collect::<Vec<_>>().join(",")).collect(),
        ),
        Format::SqlInserts => {
            let cols = names.map(|n| q(n)).collect::<Vec<_>>().join(", ");
            x.vals.iter().map(|v| format!("INSERT INTO {} ({cols}) VALUES ({});", x.table_ref, v.iter().map(lit).collect::<Vec<_>>().join(", "))).collect::<Vec<_>>().join("\n")
        }
        Format::SqlUpdates => x
            .vals
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = x.keys.get(i).filter(|k| !k.is_empty());
                let w = key.map_or("/* no key */".into(), |k| k.iter().map(|(n, c)| format!("{} = {}", q(n), literal(kind, c, opts))).collect::<Vec<_>>().join(" AND "));
                let set = x.names.iter().enumerate().map(|(j, n)| format!("{} = {}", q(n), lit(&cell(v, j)))).collect::<Vec<_>>().join(", ");
                format!("UPDATE {} SET {set} WHERE {w};", x.table_ref)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Format::WhereClause => x
            .vals
            .iter()
            .map(|v| {
                let parts = x.names.iter().enumerate().map(|(j, n)| match cell(v, j) {
                    Value::Null => format!("{} IS NULL", q(n)),
                    c => format!("{} = {}", q(n), lit(&c)),
                });
                format!("({})", parts.collect::<Vec<_>>().join(" AND "))
            })
            .collect::<Vec<_>>()
            .join(" OR\n"),
        Format::SqlInList => {
            let vals: Vec<Vec<Value>> = x.vals.iter().map(|v| v.iter().map(|c| if is_default(c) { Value::Null } else { c.clone() }).collect()).collect();
            in_list(&vals, &x.types, kind, opts).0
        }
        Format::Json => json_rows(x, opts),
        Format::Markdown => lines(
            Some(format!("| {} |\n| {} |", x.names.join(" | "), x.names.iter().map(|_| "---").collect::<Vec<_>>().join(" | "))),
            x.vals.iter().map(|v| format!("| {} |", v.iter().map(|c| raw(c, opts).replace('|', "\\|")).collect::<Vec<_>>().join(" | "))).collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn in_list_keeps_numbers_in_numeric_columns() {
        let t = |s: &str| vec![s.to_string()];
        let one = |v: Value, ty: &str, o: Options| in_list(&[vec![v]], &t(ty), DbKind::Postgres, o).0;
        assert_eq!(in_list(&[vec![json!("1.2.3.4")], vec![json!("it's")], vec![Value::Null], vec![json!("1.2.3.4")]], &[], DbKind::Postgres, Options::default()), ("('1.2.3.4', 'it''s')".into(), 2));
        assert_eq!(one(json!("010"), "varchar", Options::default()), "('010')");
        assert_eq!(one(json!("NaN"), "numeric", Options::default()), "('NaN')");
        assert_eq!(one(json!("+4"), "int8", Options::default()), "(+4)");
        assert_eq!(one(json!("+4"), "int8", Options { strict_numbers: true, ..Default::default() }), "('+4')");
        assert_eq!(one(json!("1e-5"), "float8", Options { strict_numbers: true, ..Default::default() }), "(1e-5)");
        assert!(numeric_type("BIGINT UNSIGNED") && numeric_type("double precision") && numeric_type("numeric(10,2)"));
        assert!(!numeric_type("interval") && !numeric_type("int4range") && !numeric_type("_int4"));
        assert_eq!(in_list(&[vec![Value::Null]], &[], DbKind::Postgres, Options::default()), ("(NULL)".into(), 0));
    }

    /// A \ ends a string early where the server reads it as an escape: x\') OR 1=1 -- must stay one value after IN
    #[test]
    fn in_list_escapes_backslashes_where_the_server_does() {
        let rows = [vec![json!("x\\') OR 1=1 -- ")]];
        assert_eq!(in_list(&rows, &[], DbKind::Mysql, Options::default()).0, "('x\\\\'') OR 1=1 -- ')");
        assert_eq!(in_list(&rows, &[], DbKind::Clickhouse, Options::default()).0, "('x\\\\'') OR 1=1 -- ')");
        assert_eq!(in_list(&rows, &[], DbKind::Postgres, Options::default()).0, "('x\\'') OR 1=1 -- ')");
    }

    #[test]
    fn literals_follow_the_kind() {
        let v = json!("a\\'b");
        assert_eq!(literal(DbKind::Postgres, &v, Options::default()), "'a\\''b'");
        assert_eq!(literal(DbKind::Mysql, &v, Options::default()), "'a\\\\''b'");
        assert_eq!(literal(DbKind::Clickhouse, &v, Options::default()), "'a\\\\''b'");
        assert_eq!(literal(DbKind::Redis, &v, Options::default()), "\"a\\\\'b\"");
        assert_eq!(literal(DbKind::Redis, &Value::Null, Options::default()), "\"\"");
        assert_eq!(literal(DbKind::Mysql, &Value::Null, Options::default()), "NULL");
    }

    #[test]
    fn numbers_as_javascript_writes_them() {
        let js = |v: Value| raw_text(&v, Options { js_numbers: true, ..Default::default() });
        assert_eq!(js(json!(9007199254740992.0)), "9007199254740992");
        assert_eq!(js(json!(1e16)), "10000000000000000");
        assert_eq!(js(json!(1e20)), "100000000000000000000");
        assert_eq!(js(json!(1e21)), "1e+21");
        assert_eq!(js(json!(1152921504606846976.0)), "1152921504606847000");
        assert_eq!(js(json!(-1.5)), "-1.5");
        assert_eq!(js(json!(0.000001)), "0.000001");
        assert_eq!(js(json!(1.5e-7)), "1.5e-7");
        assert_eq!(js(json!(123.456)), "123.456");
        assert_eq!(js(json!(-0.0)), "0");
        assert_eq!(js(json!(u64::MAX)), "18446744073709551615");
        assert_eq!(raw_text(&json!(1e16), Options::default()), "1e+16");
    }

    #[test]
    fn json_keeps_javascript_key_order() {
        let x = Rows { names: vec!["b".into(), "2".into(), "a".into(), "b".into()], vals: vec![vec![json!(1), json!(2), json!(DEFAULT_TOKEN), json!(4)]], ..Default::default() };
        assert_eq!(json_rows(&x, Options::default()), "[\n  {\n    \"2\": 2,\n    \"b\": 4,\n    \"a\": null\n  }\n]");
        assert_eq!(json_rows(&Rows::default(), Options::default()), "[]");
    }
}
