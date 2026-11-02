//! Grid cells as text: the IN list and DataGrip's Data Extractors (copy formats). Cells are JSON values as the grids hold them.
use crate::{sql, DbKind};
use serde::Deserialize;
use serde_json::Value;

/// The value Set DEFAULT puts in a GUI cell (written as DEFAULT, never as a string)
pub const DEFAULT_TOKEN: &str = "\u{0}DEFAULT";

/// Where the GUI writes values differently from the TUI (D2; the inList rows of crates/shared/tests/DIVERGENCES.md and the GUI's literals)
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// IN list: in a numeric column only -?digits[.digits][e±digits] text stays bare (the GUI). Otherwise any number text does,
    /// such as +4 or .5 (the TUI)
    pub strict_numbers: bool,
    /// Extractors: literals as the GUI's dialects write them: '…' with only quotes doubled (a backslash stays as it is, in
    /// PostgreSQL and ClickHouse too), MySQL also doubles \, Redis gets JSON strings. Otherwise sql::literal (PostgreSQL E'…' for a \)
    pub plain_literals: bool,
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
fn raw(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) if s == DEFAULT_TOKEN => "DEFAULT".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A cell as an SQL literal of `kind`
fn literal(kind: DbKind, v: &Value, opts: Options) -> String {
    let text = (!v.is_null()).then(|| raw_text(v));
    if !opts.plain_literals {
        return sql::literal(kind, text.as_deref());
    }
    match (kind, text) {
        (DbKind::Redis, None) => "\"\"".into(),
        (DbKind::Redis, Some(t)) => Value::from(t).to_string(),
        (_, None) => "NULL".into(),
        (DbKind::Mysql, Some(t)) => sql::literal(DbKind::Mysql, Some(&t)),
        (_, Some(t)) => quote(&t),
    }
}

/// Text of a non-NULL cell (a DEFAULT token as it is)
fn raw_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Numeric and boolean column types (PostgreSQL, MySQL); not interval and the like
fn numeric_type(t: &str) -> bool {
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

/// Rows as a list to put after IN, and how many items it has: ('a', 'b'), or one tuple per row with several columns
/// (('a', 1), ('b', 2)). Rows of only NULL and duplicates are left out. In numeric columns (`types`, one per column) number text stays bare
/// (numeric and bigint arrive as text); elsewhere it is quoted (010 in a text column)
pub fn in_list(rows: &[Vec<Value>], types: &[String], opts: Options) -> (String, usize) {
    let lit = |c: usize, v: &Value| match v {
        Value::Null => "NULL".to_string(),
        Value::Number(_) | Value::Bool(_) => v.to_string(),
        Value::String(s) if types.get(c).is_some_and(|t| numeric_type(t)) && number_text(s, opts.strict_numbers) => s.clone(),
        Value::String(s) => quote(s),
        other => quote(&other.to_string()),
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
fn json_rows(x: &Rows) -> String {
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
            let c = v.get(j).filter(|c| !is_default(c)).unwrap_or(&Value::Null);
            format!("    {}: {}", Value::from(x.names[j].as_str()), c)
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
        Format::Tsv => lines(x.header.then(|| x.names.join("\t")), x.vals.iter().map(|v| v.iter().map(raw).collect::<Vec<_>>().join("\t")).collect()),
        Format::Csv => lines(
            x.header.then(|| x.names.iter().map(|n| csv_field(n)).collect::<Vec<_>>().join(",")),
            x.vals.iter().map(|v| v.iter().map(|c| csv_field(&raw(c))).collect::<Vec<_>>().join(",")).collect(),
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
            in_list(&vals, &x.types, opts).0
        }
        Format::Json => json_rows(x),
        Format::Markdown => lines(
            Some(format!("| {} |\n| {} |", x.names.join(" | "), x.names.iter().map(|_| "---").collect::<Vec<_>>().join(" | "))),
            x.vals.iter().map(|v| format!("| {} |", v.iter().map(|c| raw(c).replace('|', "\\|")).collect::<Vec<_>>().join(" | "))).collect(),
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
        let one = |v: Value, ty: &str, o: Options| in_list(&[vec![v]], &t(ty), o).0;
        assert_eq!(in_list(&[vec![json!("1.2.3.4")], vec![json!("it's")], vec![Value::Null], vec![json!("1.2.3.4")]], &[], Options::default()), ("('1.2.3.4', 'it''s')".into(), 2));
        assert_eq!(one(json!("010"), "varchar", Options::default()), "('010')");
        assert_eq!(one(json!("NaN"), "numeric", Options::default()), "('NaN')");
        assert_eq!(one(json!("+4"), "int8", Options::default()), "(+4)");
        assert_eq!(one(json!("+4"), "int8", Options { strict_numbers: true, ..Default::default() }), "('+4')");
        assert_eq!(one(json!("1e-5"), "float8", Options { strict_numbers: true, ..Default::default() }), "(1e-5)");
        assert!(numeric_type("BIGINT UNSIGNED") && numeric_type("double precision") && numeric_type("numeric(10,2)"));
        assert!(!numeric_type("interval") && !numeric_type("int4range") && !numeric_type("_int4"));
        assert_eq!(in_list(&[vec![Value::Null]], &[], Options::default()), ("(NULL)".into(), 0));
    }

    #[test]
    fn literals_follow_the_option() {
        let plain = Options { plain_literals: true, ..Default::default() };
        let v = json!("a\\'b");
        assert_eq!(literal(DbKind::Postgres, &v, Options::default()), "E'a\\\\''b'");
        assert_eq!(literal(DbKind::Postgres, &v, plain), "'a\\''b'");
        assert_eq!(literal(DbKind::Mysql, &v, plain), "'a\\\\''b'");
        assert_eq!(literal(DbKind::Redis, &v, plain), "\"a\\\\'b\"");
        assert_eq!(literal(DbKind::Redis, &Value::Null, plain), "\"\"");
    }

    #[test]
    fn json_keeps_javascript_key_order() {
        let x = Rows { names: vec!["b".into(), "2".into(), "a".into(), "b".into()], vals: vec![vec![json!(1), json!(2), json!(DEFAULT_TOKEN), json!(4)]], ..Default::default() };
        assert_eq!(json_rows(&x), "[\n  {\n    \"2\": 2,\n    \"b\": 4,\n    \"a\": null\n  }\n]");
        assert_eq!(json_rows(&Rows::default()), "[]");
    }
}
