//! Grid cells as text: the IN list and DataGrip's Data Extractors (copy formats). Cells are JSON values as the grids hold them.
use crate::{sql, DbKind};
use serde::Deserialize;
use serde_json::Value;

/// The value Set DEFAULT puts in a GUI cell (written as DEFAULT, never as a string)
pub const DEFAULT_TOKEN: &str = "\u{0}DEFAULT";

/// A cell of the copied table no selected range holds (⌘-clicked ranges in different columns), not a NULL: CSV, TSV and Markdown
/// write an empty field; SQL Inserts, SQL Updates, Where Clause and JSON leave the column out of that row; SQL IN List writes
/// every selected cell as its own item
pub const HOLE_TOKEN: &str = "\u{0}HOLE";

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

fn is_hole(v: &Value) -> bool {
    v.as_str() == Some(HOLE_TOKEN)
}

/// A row's cells of `n` columns with their column index (a short row as NULL), holes left out
fn held(v: &[Value], n: usize) -> impl Iterator<Item = (usize, Value)> + '_ {
    (0..n).map(|j| (j, v.get(j).cloned().unwrap_or(Value::Null))).filter(|(_, c)| !is_hole(c))
}

/// A cell as plain text: NULL is empty, DEFAULT is DEFAULT
fn raw(v: &Value, opts: Options) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) if s == HOLE_TOKEN => String::new(),
        Value::String(s) if s == DEFAULT_TOKEN => "DEFAULT".into(),
        other => raw_text(other, opts),
    }
}

/// A string literal of `kind` (sql::literal): '…' with quotes doubled, \ doubled where the server reads it as an escape
/// (MySQL, ClickHouse), and PostgreSQL E'…' when the value has a \ (read the same whatever standard_conforming_strings is)
fn quote(kind: DbKind, s: &str) -> String {
    sql::literal(kind, Some(s))
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

/// Number text as SQL reads it bare. strict: digits with an optional fraction and exponent only (no sign but -, no `.5`)
pub(crate) fn number_text(s: &str, strict: bool) -> bool {
    let t = s.strip_prefix('-').unwrap_or(s);
    if !strict {
        // One sign only: --1 would start a line comment
        return !t.is_empty() && (t.len() == s.len() || !t.starts_with(['-', '+'])) && t.parse::<f64>().is_ok() && t.chars().all(|c| c.is_ascii_digit() || ".eE+-".contains(c));
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
    let lit = |c: usize, v: &Value| in_item(v, types.get(c), kind, opts);
    let items = rows
        .iter()
        .filter(|r| r.iter().any(|v| !v.is_null()))
        .map(|r| if r.len() == 1 { lit(0, &r[0]) } else { format!("({})", r.iter().enumerate().map(|(c, v)| lit(c, v)).collect::<Vec<_>>().join(", ")) });
    in_items(items)
}

/// One value of an IN list: number text bare in a numeric column of type `ty`
fn in_item(v: &Value, ty: Option<&String>, kind: DbKind, opts: Options) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Number(_) | Value::Bool(_) => raw_text(v, opts),
        Value::String(s) if ty.is_some_and(|t| numeric_type(t)) && number_text(s, opts.strict_numbers) => s.clone(),
        other => quote(kind, &raw_text(other, opts)),
    }
}

/// Items as (a, b), duplicates left out, and how many there are
fn in_items(items: impl Iterator<Item = String>) -> (String, usize) {
    let mut seen = std::collections::HashSet::new();
    let items: Vec<String> = items.filter(|x| seen.insert(x.clone())).collect();
    let n = items.len();
    (format!("({})", if n == 0 { "NULL".into() } else { items.join(", ") }), n)
}

/// A CSV field: quoted when it has a comma, a quote, a tab (paste would read TSV) or a line break, or is an empty string ("" reads back as '', an empty field as NULL)
fn csv_field(s: &str) -> String {
    if s.is_empty() || s.contains([',', '"', '\t', '\n', '\r']) { quoted(s) } else { s.to_string() }
}

/// A TSV field: as it is unless paste_cells would read it otherwise (a tab or line break, a leading quote, an empty string). Quotes
/// elsewhere stay as they are, so JSON text copies as JSON
fn tsv_field(s: &str) -> String {
    if s.is_empty() || s.starts_with('"') || s.contains(['\t', '\n', '\r']) { quoted(s) } else { s.to_string() }
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// A cell as CSV or TSV writes it: NULL (and a hole) as an empty field, anything else through `field`
fn delimited(v: &Value, opts: Options, field: fn(&str) -> String) -> String {
    if v.is_null() || is_hole(v) { String::new() } else { field(&raw(v, opts)) }
}

/// How paste_cells reads the text: by its own rules, or as CSV / TSV
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
pub enum PasteFormat {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "CSV")]
    Csv,
    #[serde(rename = "TSV")]
    Tsv,
}

/// Pasted text as grid cells, one Vec per row; None is NULL. The reverse of the CSV and TSV above, and what spreadsheets copy:
/// - one line without a tab is one value as it is (pasting `a, b` or `{"k": 1}` into a cell), and no text is no cells
/// - otherwise TSV when a tab is outside quotes, else CSV, with RFC 4180 quotes: a field that starts with `"` runs to the next lone `"`
///   (`""` inside is one quote, delimiters and line breaks inside are text); a quote elsewhere is text
/// - an empty field is NULL, `""` an empty string; CRLF reads as a line break and the text's last line break ends the last row
///
/// `format` Csv or Tsv reads the text as that, one line too: the caller knows it wrote the text so (the GUI's last grid copy)
pub fn paste_cells(text: &str, format: PasteFormat) -> Vec<Vec<Option<String>>> {
    match format {
        PasteFormat::Csv => return split_delimited(text, ',').0,
        PasteFormat::Tsv => return split_delimited(text, '\t').0,
        PasteFormat::Auto => {}
    }
    let line = text.strip_suffix('\n').map_or(text, |t| t.strip_suffix('\r').unwrap_or(t));
    if !line.contains(['\n', '\r', '\t']) {
        return if line.is_empty() { vec![] } else { vec![vec![Some(line.to_string())]] };
    }
    // A tab is outside quotes when reading the text as CSV leaves one in a field's text (a CSV quote around a tab keeps it CSV)
    let (csv, tab) = split_delimited(text, ',');
    if tab { split_delimited(text, '\t').0 } else { csv }
}

/// Rows of `text` split at `d`, and whether a tab was read outside quotes
fn split_delimited(text: &str, d: char) -> (Vec<Vec<Option<String>>>, bool) {
    let (mut rows, mut row, mut field) = (vec![], vec![], String::new());
    // `start`: nothing of the field read yet; `fresh`: nothing of the row (the text's last line break adds no empty row)
    let (mut quoted, mut start, mut fresh, mut tab) = (false, true, true, false);
    let end = |field: &mut String, quoted: &mut bool| {
        let f = std::mem::take(field);
        if std::mem::take(quoted) || !f.is_empty() { Some(f) } else { None }
    };
    let mut cs = text.chars().peekable();
    while let Some(c) = cs.next() {
        fresh = false;
        if start && c == '"' {
            (quoted, start) = (true, false);
            while let Some(q) = cs.next() {
                match q {
                    '"' if cs.peek() == Some(&'"') => {
                        cs.next();
                        field.push('"');
                    }
                    '"' => break,
                    q => field.push(q),
                }
            }
            continue;
        }
        start = false;
        match c {
            '\r' if cs.peek() == Some(&'\n') => {}
            '\n' => {
                row.push(end(&mut field, &mut quoted));
                rows.push(std::mem::take(&mut row));
                (start, fresh) = (true, true);
            }
            c if c == d => {
                row.push(end(&mut field, &mut quoted));
                start = true;
            }
            c => {
                tab |= c == '\t';
                field.push(c);
            }
        }
    }
    if !fresh {
        row.push(end(&mut field, &mut quoted));
        rows.push(row);
    }
    (rows, tab)
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
        if order.iter().all(|&j| v.get(j).is_some_and(is_hole)) {
            return "  {}".to_string();
        }
        let fields = order.iter().filter(|&&j| !v.get(j).is_some_and(is_hole)).map(|&j| {
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
    let lit = |v: &Value| if is_default(v) { "DEFAULT".to_string() } else { literal(kind, v, opts) };
    let lines = |head: Option<String>, body: Vec<String>| head.into_iter().chain(body).collect::<Vec<_>>().join("\n");
    match fmt {
        Format::Tsv => lines(
            x.header.then(|| x.names.iter().map(|n| tsv_field(n)).collect::<Vec<_>>().join("\t")),
            x.vals.iter().map(|v| v.iter().map(|c| delimited(c, opts, tsv_field)).collect::<Vec<_>>().join("\t")).collect(),
        ),
        Format::Csv => lines(
            x.header.then(|| x.names.iter().map(|n| csv_field(n)).collect::<Vec<_>>().join(",")),
            x.vals.iter().map(|v| v.iter().map(|c| delimited(c, opts, csv_field)).collect::<Vec<_>>().join(",")).collect(),
        ),
        Format::SqlInserts => x
            .vals
            .iter()
            .map(|v| {
                let (cols, vals): (Vec<_>, Vec<_>) = held(v, x.names.len()).map(|(j, c)| (q(&x.names[j]), lit(&c))).unzip();
                format!("INSERT INTO {} ({}) VALUES ({});", x.table_ref, cols.join(", "), vals.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Format::SqlUpdates => x
            .vals
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = x.keys.get(i).filter(|k| !k.is_empty());
                let w = key.map_or("/* no key */".into(), |k| k.iter().map(|(n, c)| format!("{} = {}", q(n), literal(kind, c, opts))).collect::<Vec<_>>().join(" AND "));
                let set = held(v, x.names.len()).map(|(j, c)| format!("{} = {}", q(&x.names[j]), lit(&c))).collect::<Vec<_>>().join(", ");
                format!("UPDATE {} SET {set} WHERE {w};", x.table_ref)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Format::WhereClause => x
            .vals
            .iter()
            .map(|v| {
                let parts = held(v, x.names.len()).map(|(j, c)| match c {
                    Value::Null => format!("{} IS NULL", q(&x.names[j])),
                    c => format!("{} = {}", q(&x.names[j]), lit(&c)),
                });
                format!("({})", parts.collect::<Vec<_>>().join(" AND "))
            })
            .collect::<Vec<_>>()
            .join(" OR\n"),
        Format::SqlInList => {
            let vals: Vec<Vec<Value>> = x.vals.iter().map(|v| v.iter().map(|c| if is_default(c) { Value::Null } else { c.clone() }).collect()).collect();
            if !vals.iter().flatten().any(is_hole) {
                return in_list(&vals, &x.types, kind, opts).0;
            }
            // Ranges in different columns: no tuple fits every row, so each selected cell is an item (NULLs left out as above)
            in_items(vals.iter().flat_map(|v| v.iter().enumerate()).filter(|(_, c)| !c.is_null() && !is_hole(c)).map(|(j, c)| in_item(c, x.types.get(j), kind, opts))).0
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
        for s in ["--1", "-+1", "--1e5"] {
            assert_eq!(one(json!(s), "int8", Options::default()), format!("('{s}')"));
        }
        assert_eq!(one(json!("-1"), "int8", Options::default()), "(-1)");
        assert_eq!(one(json!(".5"), "numeric", Options::default()), "(.5)");
    }

    /// A \ ends a string early where the server reads it as an escape: x\') OR 1=1 -- must stay one value after IN
    #[test]
    fn in_list_escapes_backslashes_where_the_server_does() {
        let rows = [vec![json!("x\\') OR 1=1 -- ")]];
        assert_eq!(in_list(&rows, &[], DbKind::Mysql, Options::default()).0, "('x\\\\'') OR 1=1 -- ')");
        assert_eq!(in_list(&rows, &[], DbKind::Clickhouse, Options::default()).0, "('x\\\\'') OR 1=1 -- ')");
        // PostgreSQL: E'' reads the same with standard_conforming_strings off; SQL Server and the like keep \ as it is
        assert_eq!(in_list(&rows, &[], DbKind::Postgres, Options::default()).0, "(E'x\\\\'') OR 1=1 -- ')");
        assert_eq!(in_list(&rows, &[], DbKind::Mssql, Options::default()).0, "('x\\'') OR 1=1 -- ')");
    }

    /// ClickHouse also reads \ as an escape inside "names": a column name can't end the identifier early
    #[test]
    fn clickhouse_names_escape_backslashes() {
        let x = Rows { names: vec!["a\\".into(), ") VALUES (1); DROP TABLE t; -- ".into()], types: vec![], vals: vec![vec![json!("1"), json!("2")]], keys: vec![], table_ref: "t".into(), header: false };
        assert_eq!(rows(Format::SqlInserts, &x, DbKind::Clickhouse, Options::default()), "INSERT INTO t (\"a\\\\\", \") VALUES (1); DROP TABLE t; -- \") VALUES ('1', '2');");
        assert_eq!(crate::sql::quote_ident(DbKind::Postgres, "a\\"), "\"a\\\"", "PostgreSQL names keep \\ as it is");
    }

    #[test]
    fn literals_follow_the_kind() {
        let v = json!("a\\'b");
        assert_eq!(literal(DbKind::Postgres, &v, Options::default()), "E'a\\\\''b'");
        assert_eq!(literal(DbKind::Postgres, &json!("it's"), Options::default()), "'it''s'");
        assert_eq!(literal(DbKind::Sqlite, &v, Options::default()), "'a\\''b'");
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

    /// ⌘-clicked cells in different columns: a hole is not a NULL, so no format writes or matches a value nobody selected
    #[test]
    fn holes_are_left_out_not_written_as_null() {
        let h = json!(HOLE_TOKEN);
        let x = Rows {
            names: vec!["user_name".into(), "amount".into()],
            types: vec!["text".into(), "numeric".into()],
            vals: vec![vec![json!("user_1"), h.clone()], vec![h.clone(), json!("2469.00")], vec![json!("user_3"), Value::Null]],
            keys: vec![vec![("id".into(), json!(1))], vec![("id".into(), json!(2))], vec![("id".into(), json!(3))]],
            table_ref: "t".into(),
            header: true,
        };
        let f = |fmt| rows(fmt, &x, DbKind::Postgres, Options::default());
        assert_eq!(f(Format::SqlUpdates), "UPDATE t SET user_name = 'user_1' WHERE id = '1';\nUPDATE t SET amount = '2469.00' WHERE id = '2';\nUPDATE t SET user_name = 'user_3', amount = NULL WHERE id = '3';");
        assert_eq!(f(Format::WhereClause), "(user_name = 'user_1') OR\n(amount = '2469.00') OR\n(user_name = 'user_3' AND amount IS NULL)");
        assert_eq!(f(Format::SqlInserts), "INSERT INTO t (user_name) VALUES ('user_1');\nINSERT INTO t (amount) VALUES ('2469.00');\nINSERT INTO t (user_name, amount) VALUES ('user_3', NULL);");
        assert_eq!(f(Format::SqlInList), "('user_1', 2469.00, 'user_3')");
        assert_eq!(f(Format::Json), "[\n  {\n    \"user_name\": \"user_1\"\n  },\n  {\n    \"amount\": \"2469.00\"\n  },\n  {\n    \"user_name\": \"user_3\",\n    \"amount\": null\n  }\n]");
        assert_eq!(f(Format::Csv), "user_name,amount\nuser_1,\n,2469.00\nuser_3,");
        assert_eq!(f(Format::Tsv), "user_name\tamount\nuser_1\t\n\t2469.00\nuser_3\t");
        assert_eq!(f(Format::Markdown), "| user_name | amount |\n| --- | --- |\n| user_1 |  |\n|  | 2469.00 |\n| user_3 |  |");
    }

    fn auto(text: &str) -> Vec<Vec<Option<String>>> {
        paste_cells(text, PasteFormat::Auto)
    }

    fn cells(rows: &[&[Option<&str>]]) -> Vec<Vec<Option<String>>> {
        rows.iter().map(|r| r.iter().map(|c| c.map(String::from)).collect()).collect()
    }

    #[test]
    fn paste_reads_csv_quotes() {
        let n: Option<&str> = None;
        // Doubled quotes, a comma and a line break inside quotes, an empty field (NULL) and "" (an empty string)
        assert_eq!(auto("1,\"{\"\"k\"\": \"\"a, b\"\"}\",\n2,\"x\ny\",\"\"\n"), cells(&[&[Some("1"), Some("{\"k\": \"a, b\"}"), n], &[Some("2"), Some("x\ny"), Some("")]]));
        // CRLF, and only the last line break is dropped: an empty line is a row with one NULL
        assert_eq!(auto("a,b\r\n\r\nc,\r\n"), cells(&[&[Some("a"), Some("b")], &[n], &[Some("c"), n]]));
        // A quote that doesn't start the field is text; text after a closing quote joins the field
        assert_eq!(auto("a\"b,\"c\"d\ne"), cells(&[&[Some("a\"b"), Some("cd")], &[Some("e")]]));
        // An unclosed quote runs to the end
        assert_eq!(auto("x\n\"a,\nb"), cells(&[&[Some("x")], &[Some("a,\nb")]]));
    }

    #[test]
    fn paste_reads_tsv_when_a_tab_is_outside_quotes() {
        let n: Option<&str> = None;
        assert_eq!(auto("a\t\t{\"k\": 1, \"j\": 2}\n\"x\ty\"\t\"\"\t,"), cells(&[&[Some("a"), n, Some("{\"k\": 1, \"j\": 2}")], &[Some("x\ty"), Some(""), Some(",")]]));
        assert_eq!(auto("a\tb"), cells(&[&[Some("a"), Some("b")]]));
        // A tab only inside quotes: CSV
        assert_eq!(auto("\"a\tb\",c\nd"), cells(&[&[Some("a\tb"), Some("c")], &[Some("d")]]));
    }

    /// One line without a tab is one value: `a, b` or a JSON value pasted into one cell stays whole, quotes and all
    #[test]
    fn paste_keeps_one_line_whole() {
        assert_eq!(auto("a, b\n"), cells(&[&[Some("a, b")]]));
        assert_eq!(auto("\"quoted\", {\"k\": 1}"), cells(&[&[Some("\"quoted\", {\"k\": 1}")]]));
        assert_eq!(auto(""), cells(&[]));
        assert_eq!(auto("\r\n"), cells(&[]));
    }

    /// What CSV and TSV copy writes, paste reads back: NULL, '', quotes, commas, tabs and line breaks in values
    #[test]
    fn paste_reads_back_csv_and_tsv_copy() {
        let vals = vec![
            vec![Value::Null, json!(""), json!(42), json!("{\"k\": \"a, b\"}")],
            vec![json!("\"lead"), json!("x\ty"), json!("l1\nl2"), json!("cr\r\nlf")],
            vec![json!("plain"), json!(" sp "), Value::Null, json!("a\"b")],
        ];
        let want: Vec<Vec<Option<String>>> = vals.iter().map(|r| r.iter().map(|v| (!v.is_null()).then(|| raw_text(v, Options::default()))).collect()).collect();
        for fmt in [Format::Csv, Format::Tsv] {
            let x = Rows { names: vec!["a".into(), "b".into(), "c".into(), "d".into()], vals: vals.clone(), ..Default::default() };
            let text = rows(fmt, &x, DbKind::Postgres, Options::default());
            assert_eq!(auto(&text), want, "{fmt:?}: {text}");
            assert_eq!(auto(&format!("{text}\n")), want, "{fmt:?} with a final line break");
            let head = rows(fmt, &Rows { header: true, ..x }, DbKind::Postgres, Options::default());
            assert_eq!(auto(&head)[1..], want[..], "{fmt:?} with a header");
            // Read as the format it was copied in (the GUI's last grid copy): the same, and one row splits into its cells too
            let forced = if fmt == Format::Csv { PasteFormat::Csv } else { PasteFormat::Tsv };
            assert_eq!(paste_cells(&text, forced), want, "{fmt:?} forced");
            for (i, row) in vals.iter().enumerate() {
                let one = rows(fmt, &Rows { vals: vec![row.clone()], ..Default::default() }, DbKind::Postgres, Options::default());
                assert_eq!(paste_cells(&one, forced), want[i..=i], "{fmt:?} row {i}: {one}");
            }
        }
        // One copied CSV row is one value by the text's own rules, cells when read as CSV
        assert_eq!(auto("1,,\"a, b\""), cells(&[&[Some("1,,\"a, b\"")]]));
        assert_eq!(paste_cells("1,,\"a, b\"", PasteFormat::Csv), cells(&[&[Some("1"), None, Some("a, b")]]));
        // TSV leaves JSON's quotes as they are
        let json = Rows { vals: vec![vec![json!("{\"k\": 1}"), Value::Null]], ..Default::default() };
        assert_eq!(rows(Format::Tsv, &json, DbKind::Postgres, Options::default()), "{\"k\": 1}\t");
    }

    #[test]
    fn json_keeps_javascript_key_order() {
        let x = Rows { names: vec!["b".into(), "2".into(), "a".into(), "b".into()], vals: vec![vec![json!(1), json!(2), json!(DEFAULT_TOKEN), json!(4)]], ..Default::default() };
        assert_eq!(json_rows(&x, Options::default()), "[\n  {\n    \"2\": 2,\n    \"b\": 4,\n    \"a\": null\n  }\n]");
        assert_eq!(json_rows(&Rows::default(), Options::default()), "[]");
    }
}
