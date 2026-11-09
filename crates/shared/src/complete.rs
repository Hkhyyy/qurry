//! Console autocompletion, shared by the TUI and the GUI (through WASM: CompletionIndex in qurry-shared-wasm, used by src/lib/completion.ts)
//! - what the caret's spot takes (scope.rs reads the statement's blocks, sources, CTEs and select lists):
//!   a statement start → statement keywords and templates (`sel`, `ins`, `upd`, `del`) only; after FROM / JOIN → CTEs, tables
//!   (after JOIN those with a foreign key to the statement's tables first); after a joined table → its ON conditions from foreign
//!   keys; in a select list, WHERE, ON … → the columns of the sources in scope at the caret, then functions, tables, keywords;
//!   GROUP BY → the select list's non-aggregated expressions first; ORDER BY → its aliases first; INSERT's column list → the
//!   columns not listed yet; `*` asked for → the column list it stands for
//! - `alias.` / `table.` / `cte.` → that source's columns, `schema.` → that schema's tables
//! - value spots (`status = '`, `data->>'` …) → values · json keys read from the DB (hint_at, then hint_items with what the DB returned)
//! - [`signature`]: the function call around the caret and its parameter; [`marks`]: tables and columns the statement's scope lacks
//!
//! Where the two front ends complete differently, they say so with [`Options`] (Options::GUI, Options::TUI).
use crate::scope::{K, Scope, Source, is_kw};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A table (or view) with its columns (name, type), as the backend reads them for autocomplete. A row with `routine` is a function
/// (its name and signature; no columns)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<(String, String)>,
    /// Foreign keys this table holds
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fks: Vec<ForeignKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<Routine>,
}

/// `columns` reference `ref_schema.ref_table (ref_columns)`
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

/// A user function's arguments and result, as the DB writes them (`a integer, b text`, `bigint`)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub args: String,
    pub returns: String,
}

/// A value (or json key) the DB has for a column, with how many sampled rows have it
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hint {
    pub value: String,
    pub count: i64,
}

/// Where the GUI and the TUI complete differently (D2). Each front end passes its own: [`Options::GUI`], [`Options::TUI`]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// The editor filters and ranks by the typed word itself (Monaco matches fuzzily, so `uname` finds user_name): every candidate,
    /// with columns, schema tables and keywords sorted by label. Off: only candidates that start with the typed word, without the
    /// fully typed word itself (so Enter goes to a new line), in statement order
    pub editor_filters: bool,
    /// Keywords also before anything is typed. Off: they wait for a character (dozens of keywords on an empty prefix get in the way)
    pub keywords_on_empty: bool,
    /// Inside the quotes of a value spot that has no value suggestions (bytea, a column the statement's tables lack …): no
    /// candidates at all, since keywords and tables don't fit inside a string
    pub quiet_in_value_quotes: bool,
    /// After a value typed inside quotes, add the closing quote when none follows the cursor. Off: Monaco closes quotes as they are typed
    pub close_quotes: bool,
}

impl Options {
    /// Monaco (src/lib/completion.ts)
    pub const GUI: Options = Options { editor_filters: true, keywords_on_empty: true, quiet_in_value_quotes: true, close_quotes: false };
    /// The terminal console's own popup
    pub const TUI: Options = Options { editor_filters: false, keywords_on_empty: false, quiet_in_value_quotes: false, close_quotes: true };
}

/// A foreign key seen from one of its two tables: the other table, this side's columns and the other side's
#[derive(Debug, Clone)]
struct Link {
    other: usize,
    mine: Vec<String>,
    theirs: Vec<String>,
    /// This table holds the foreign key
    out: bool,
}

/// A function: a built-in one or a user one from the metadata
#[derive(Debug, Clone, PartialEq)]
pub struct Func {
    pub schema: String,
    pub name: String,
    pub args: String,
    pub returns: String,
    pub aggregate: bool,
}

/// The completion metadata's column limit (meta.rs completion_meta): at it, tables past it are missing, so none is marked unknown
pub const COLUMN_LIMIT: usize = 30_000;

/// Autocomplete metadata: lowercase names, a per-name index and the sorted orders are built once on load (not on every key)
pub struct Meta {
    pub tables: Vec<CompletionTable>,
    /// (schema, table) lowercase
    lower: Vec<(String, String)>,
    /// Lowercase table name → positions in tables
    by_name: HashMap<String, Vec<usize>>,
    /// Positions in tables, by table name (ties in metadata order)
    by_label: Vec<usize>,
    /// Distinct schemas, sorted
    schemas: Vec<String>,
    /// Foreign keys per table, both ways
    links: Vec<Vec<Link>>,
    /// User functions, by name
    functions: Vec<Func>,
    /// The metadata stopped at its limit (or nothing loaded): unknown names can't be told
    partial: bool,
}

impl Meta {
    pub fn new(rows: Vec<CompletionTable>) -> Self {
        let (routines, tables): (Vec<_>, Vec<_>) = rows.into_iter().partition(|t| t.routine.is_some());
        let mut functions: Vec<Func> = routines.into_iter().map(|t| {
            let r = t.routine.unwrap_or_default();
            Func { schema: t.schema, name: t.name, args: r.args, returns: r.returns, aggregate: false }
        }).collect();
        functions.sort_by(|a, b| a.name.cmp(&b.name));
        let lower: Vec<(String, String)> = tables.iter().map(|t| (t.schema.to_lowercase(), t.name.to_lowercase())).collect();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, (_, n)) in lower.iter().enumerate() {
            by_name.entry(n.clone()).or_default().push(i);
        }
        let mut by_label: Vec<usize> = (0..tables.len()).collect();
        by_label.sort_by(|&a, &b| tables[a].name.cmp(&tables[b].name));
        let mut schemas: Vec<String> = tables.iter().map(|t| t.schema.clone()).collect();
        schemas.sort();
        schemas.dedup();
        let mut links: Vec<Vec<Link>> = vec![vec![]; tables.len()];
        for (i, t) in tables.iter().enumerate() {
            for fk in &t.fks {
                let (rs, rt) = (fk.ref_schema.to_lowercase(), fk.ref_table.to_lowercase());
                let Some(j) = by_name.get(&rt).and_then(|v| v.iter().copied().find(|&j| lower[j].0 == rs)) else { continue };
                links[i].push(Link { other: j, mine: fk.columns.clone(), theirs: fk.ref_columns.clone(), out: true });
                // A table referencing itself (employees.manager_id → employees.id) gets both: a self join goes either way
                links[j].push(Link { other: i, mine: fk.ref_columns.clone(), theirs: fk.columns.clone(), out: false });
            }
        }
        let partial = tables.is_empty() || tables.iter().map(|t| t.columns.len()).sum::<usize>() >= COLUMN_LIMIT;
        Meta { tables, lower, by_name, by_label, schemas, links, functions, partial }
    }
}

/// Keywords offered while typing (functions such as count come as functions)
const KEYWORDS: &str = "select from where and or not in is null like ilike between exists group by order having limit offset join left right inner outer full cross on using as \
distinct union all intersect except insert into values update set delete returning create table view index alter drop truncate with case when then else end \
asc desc nulls first last current_date current_timestamp interval true false begin commit rollback explain analyze natural lateral show grant revoke call";

/// Keywords a statement starts with: the only ones at a statement start (with the templates)
const STATEMENT_KEYWORDS: [&str; 18] = ["select", "insert", "update", "delete", "with", "create", "alter", "drop", "truncate", "explain", "analyze", "begin", "commit", "rollback", "set", "show", "values", "call"];

/// Templates at a statement start: (label, what it writes, the text as a snippet: `${n:placeholder}`, `$0` the caret)
const TEMPLATES: [(&str, &str, &str); 4] = [
    ("sel", "select * from …", "select * from $0"),
    ("ins", "insert into … (columns) values (…)", "insert into $0"),
    ("upd", "update … set … where …", "update ${1:table} set ${2:column} = ${3:value} where ${4:condition}"),
    ("del", "delete from … where …", "delete from ${1:table} where ${2:condition}"),
];

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Column,
    Table,
    Schema,
    Keyword,
    /// Column values · json keys (read from the DB)
    Value,
    /// A join's ON condition from a foreign key
    Join,
    /// A function: `insert` is a snippet (the caret inside its parentheses)
    Function,
    /// A template, an INSERT's column list, `*` expanded: `insert` is a snippet (`${1:placeholder}`, `$0`; see [`snippet_text`])
    Snippet,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub insert: String,
    pub kind: ItemKind,
}

impl Item {
    fn new(label: impl Into<String>, detail: impl Into<String>, insert: impl Into<String>, kind: ItemKind) -> Self {
        Item { label: label.into(), detail: detail.into(), insert: insert.into(), kind }
    }
}

/// A snippet's text without its markers, and its stops in order (char ranges in the text; `$0`, the final caret, last).
/// `\$`, `\}` and `\\` are the characters themselves
pub fn snippet_text(s: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut n = 0;
    let mut stops: Vec<(u32, usize, usize)> = vec![];
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' if it.peek().is_some_and(|c| matches!(c, '$' | '}' | '\\')) => {
                out.push(it.next().unwrap());
                n += 1;
            }
            '$' if it.peek().is_some_and(|c| c.is_ascii_digit() || *c == '{') => {
                let braced = it.peek() == Some(&'{');
                if braced {
                    it.next();
                }
                let mut num = String::new();
                while let Some(d) = it.peek().filter(|d| d.is_ascii_digit()) {
                    num.push(*d);
                    it.next();
                }
                let start = n;
                if braced && it.peek() == Some(&':') {
                    it.next();
                    for c in it.by_ref() {
                        if c == '}' {
                            break;
                        }
                        out.push(c);
                        n += 1;
                    }
                } else if braced {
                    it.next();
                }
                let k: u32 = num.parse().unwrap_or(0);
                stops.push((if k == 0 { u32::MAX } else { k }, start, n));
            }
            _ => {
                out.push(c);
                n += 1;
            }
        }
    }
    stops.sort_by_key(|s| s.0);
    (out, stops.into_iter().map(|(_, a, b)| (a, b)).collect())
}

/// A name inside a snippet, its `$ } \` escaped
fn snip(s: &str) -> String {
    s.replace('\\', "\\\\").replace('$', "\\$").replace('}', "\\}")
}

fn keywords() -> impl Iterator<Item = &'static str> {
    KEYWORDS.split_whitespace()
}

/// Keywords and the built-in function names, sorted (the aliases check)
static RESERVED: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
    let mut v: Vec<&str> = keywords().chain(["count", "sum", "avg", "min", "max", "coalesce", "now"]).collect();
    v.sort();
    v.dedup();
    v
});

/// Keywords by label (the GUI's order)
static SORTED_KEYWORDS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
    let mut v: Vec<&str> = keywords().collect();
    v.sort();
    v
});

/// A keyword, in any letter case (binary search: a long script checks an alias for every table ref)
pub(crate) fn is_keyword(w: &str) -> bool {
    RESERVED.binary_search_by(|k| k.bytes().cmp(w.bytes().map(|b| b.to_ascii_lowercase()))).is_ok()
}

/// Identifier characters: letters and digits of any script (Korean …) and `_`
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Keywords a table reference follows
const REF_KEYWORDS: [&str; 5] = ["from", "join", "straight_join", "update", "into"];

fn is_ref_keyword(w: &str) -> bool {
    REF_KEYWORDS.iter().any(|k| w.eq_ignore_ascii_case(k))
}

/// The tab's DB as the schema its bare names live in: only in MySQL, where the schema is the DB. A PostgreSQL schema named
/// after the database isn't on the default search_path, so its tables keep their schema
fn db_schema(db: &str, mysql: bool) -> &str {
    if mysql { db } else { "" }
}

/// The table a reference names: `schema.table`, or a bare name (in `public` or the tab's DB first, then any schema)
fn find_table<'a>(m: &'a Meta, r: &str, db: &str) -> Option<&'a CompletionTable> {
    find_idx(m, r, db).map(|i| &m.tables[i])
}

/// [`find_table`]'s position in the metadata
fn find_idx(m: &Meta, r: &str, db: &str) -> Option<usize> {
    let r = r.to_lowercase();
    let (schema, name) = match r.split_once('.') {
        Some((s, n)) => (Some(s), n),
        None => (None, r.as_str()),
    };
    let hits = m.by_name.get(name)?;
    match schema {
        Some(s) => hits.iter().copied().find(|&i| m.lower[i].0 == s),
        None => hits.iter().copied().find(|&i| m.tables[i].schema == "public" || m.tables[i].schema == db).or(hits.first().copied()),
    }
}

/// How a table is written in the console: bare in `public` or the tab's DB, else `schema.table`
fn table_insert(t: &CompletionTable, db: &str) -> String {
    if t.schema == "public" || t.schema == db { t.name.clone() } else { format!("{}.{}", t.schema, t.name) }
}

// ───────── Built-in functions ─────────

const AGG: u8 = 1;
const PG: u8 = 2;
const MY: u8 = 4;
const BOTH: u8 = PG | MY;

/// (name, arguments, result, flags): what signature help and completion offer for built-in functions
const BUILTINS: &[(&str, &str, &str, u8)] = &[
    ("count", "expression", "bigint", BOTH | AGG),
    ("sum", "expression", "numeric", BOTH | AGG),
    ("avg", "expression", "numeric", BOTH | AGG),
    ("min", "expression", "same as input", BOTH | AGG),
    ("max", "expression", "same as input", BOTH | AGG),
    ("array_agg", "expression", "array", PG | AGG),
    ("string_agg", "expression, delimiter", "text", PG | AGG),
    ("json_agg", "expression", "json", PG | AGG),
    ("jsonb_agg", "expression", "jsonb", PG | AGG),
    ("jsonb_object_agg", "key, value", "jsonb", PG | AGG),
    ("bool_and", "expression", "boolean", PG | AGG),
    ("bool_or", "expression", "boolean", PG | AGG),
    ("stddev", "expression", "numeric", BOTH | AGG),
    ("variance", "expression", "numeric", BOTH | AGG),
    ("group_concat", "expression", "text", MY | AGG),
    ("json_arrayagg", "expression", "json", MY | AGG),
    ("json_objectagg", "key, value", "json", MY | AGG),
    ("coalesce", "value, …", "first non-null value", BOTH),
    ("nullif", "value1, value2", "value1, or null when equal", BOTH),
    ("greatest", "value, …", "largest value", BOTH),
    ("least", "value, …", "smallest value", BOTH),
    ("lower", "string", "text", BOTH),
    ("upper", "string", "text", BOTH),
    ("length", "string", "integer", BOTH),
    ("char_length", "string", "integer", BOTH),
    ("substring", "string, start, count", "text", BOTH),
    ("trim", "string", "text", BOTH),
    ("replace", "string, from, to", "text", BOTH),
    ("concat", "value, …", "text", BOTH),
    ("concat_ws", "separator, value, …", "text", BOTH),
    ("left", "string, n", "text", BOTH),
    ("right", "string, n", "text", BOTH),
    ("lpad", "string, length, fill", "text", BOTH),
    ("rpad", "string, length, fill", "text", BOTH),
    ("reverse", "string", "text", BOTH),
    ("repeat", "string, count", "text", BOTH),
    ("abs", "x", "same as input", BOTH),
    ("round", "x, digits", "numeric", BOTH),
    ("ceil", "x", "numeric", BOTH),
    ("floor", "x", "numeric", BOTH),
    ("mod", "y, x", "numeric", BOTH),
    ("power", "a, b", "numeric", BOTH),
    ("now", "", "timestamp", BOTH),
    ("row_number", "", "bigint", BOTH),
    ("rank", "", "bigint", BOTH),
    ("dense_rank", "", "bigint", BOTH),
    ("lag", "value, offset, default", "same as value", BOTH),
    ("lead", "value, offset, default", "same as value", BOTH),
    ("first_value", "value", "same as value", BOTH),
    ("last_value", "value", "same as value", BOTH),
    ("date_trunc", "field, source", "timestamp", PG),
    ("date_part", "field, source", "double precision", PG),
    ("to_char", "value, format", "text", PG),
    ("to_date", "text, format", "date", PG),
    ("to_timestamp", "text, format", "timestamp with time zone", PG),
    ("age", "timestamp, timestamp", "interval", PG),
    ("make_date", "year, month, day", "date", PG),
    ("generate_series", "start, stop, step", "setof", PG),
    ("jsonb_build_object", "key, value, …", "jsonb", PG),
    ("json_build_object", "key, value, …", "json", PG),
    ("jsonb_set", "target, path, new_value, create_missing", "jsonb", PG),
    ("jsonb_array_length", "jsonb", "integer", PG),
    ("jsonb_typeof", "jsonb", "text", PG),
    ("to_jsonb", "value", "jsonb", PG),
    ("split_part", "string, delimiter, n", "text", PG),
    ("regexp_replace", "string, pattern, replacement, flags", "text", PG),
    ("string_to_array", "string, delimiter", "text[]", PG),
    ("array_length", "array, dimension", "integer", PG),
    ("unnest", "array", "setof", PG),
    ("gen_random_uuid", "", "uuid", PG),
    ("md5", "string", "text", BOTH),
    ("random", "", "double precision", PG),
    ("format", "formatstr, value, …", "text", PG),
    ("initcap", "string", "text", PG),
    ("starts_with", "string, prefix", "boolean", PG),
    ("pg_size_pretty", "bigint", "text", PG),
    ("nextval", "regclass", "bigint", PG),
    ("ifnull", "expr1, expr2", "expr1, or expr2 when null", MY),
    ("if", "condition, then, else", "then or else", MY),
    ("date_format", "date, format", "varchar", MY),
    ("str_to_date", "string, format", "datetime", MY),
    ("datediff", "expr1, expr2", "integer", MY),
    ("timestampdiff", "unit, datetime1, datetime2", "bigint", MY),
    ("from_unixtime", "unix_timestamp, format", "datetime", MY),
    ("unix_timestamp", "date", "bigint", MY),
    ("curdate", "", "date", MY),
    ("date", "expr", "date", MY),
    ("json_extract", "json_doc, path, …", "json", MY),
    ("json_unquote", "json_val", "text", MY),
    ("json_object", "key, value, …", "json", MY),
    ("json_array", "value, …", "json", MY),
    ("json_contains", "target, candidate, path", "integer", MY),
    ("instr", "string, substring", "integer", MY),
    ("locate", "substring, string, position", "integer", MY),
    ("uuid", "", "varchar", MY),
    ("rand", "", "double", MY),
    ("last_insert_id", "", "bigint", MY),
    ("find_in_set", "string, string_list", "integer", MY),
];

/// Built-in functions of the dialect
fn builtins(mysql: bool) -> impl Iterator<Item = Func> {
    BUILTINS.iter().filter(move |b| b.3 & if mysql { MY } else { PG } != 0).map(|&(n, a, r, f)| Func { schema: String::new(), name: n.into(), args: a.into(), returns: r.into(), aggregate: f & AGG != 0 })
}

/// An aggregate function's name
fn is_aggregate(name: &str) -> bool {
    BUILTINS.iter().any(|b| b.3 & AGG != 0 && b.0.eq_ignore_ascii_case(name))
}

impl Func {
    /// `name(args) → result`
    pub fn signature(&self) -> String {
        let schema = if self.schema.is_empty() { String::new() } else { format!("{}.", self.schema) };
        format!("{schema}{}({}) → {}", self.name, self.args, self.returns)
    }

    /// The arguments one by one (split at the commas outside parentheses)
    pub fn params(&self) -> Vec<String> {
        let mut out = vec![];
        let (mut depth, mut cur) = (0, String::new());
        for c in self.args.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(cur.trim().to_string());
                    cur.clear();
                    continue;
                }
                _ => {}
            }
            cur.push(c);
        }
        if !cur.trim().is_empty() {
            out.push(cur.trim().to_string());
        }
        out
    }

    fn item(&self) -> Item {
        let insert = if self.args.is_empty() { format!("{}()", snip(&self.name)) } else { format!("{}($0)", snip(&self.name)) };
        Item::new(self.name.clone(), self.signature(), insert, ItemKind::Function)
    }
}

// ───────── Where the caret is ─────────

/// The caret's statement as the editor passes it to [`suggest`] (and [`signature`]): its text, trimmed at the start, and the caret's byte
/// offset in it. Right after a `;` (a statement ended, the next not started): an empty statement
pub fn at_caret(script: &str, offset: usize, kind: crate::DbKind) -> (String, usize) {
    let Some((s, e)) = crate::sql::statement_span(script, offset, kind) else { return (String::new(), 0) };
    let head = script.get(s..offset).unwrap_or("").trim_start();
    let stmt = script[s..e].trim();
    if head.len() <= stmt.len() && stmt.starts_with(head) {
        return (stmt.to_string(), head.len());
    }
    // Past the statement's end (spaces, a new line after it)
    if head.get(stmt.len()..).is_some_and(|rest| rest.contains(';')) || head.is_empty() {
        return (String::new(), 0);
    }
    (head.to_string(), head.len())
}

/// What the spot at the caret takes
#[derive(Debug, Clone, PartialEq)]
enum Spot {
    /// Nothing before the caret in the statement
    Start,
    /// A word starting a line after a statement that could end there (`select * from orders⏎se`): statement keywords first
    StatementFirst,
    /// After FROM / JOIN / UPDATE / INTO / a `,` of a FROM list: a table
    TableRef { join: bool },
    /// Right after a source (`from users u ‸`): its ON condition when it's a join without one yet; INSERT INTO t: its column list
    AfterSource,
    /// After ON (its first word: the FK conditions too)
    On { first: bool },
    SelectList,
    GroupBy,
    OrderBy,
    /// Inside INSERT INTO t (…)
    InsertColumns,
    /// Right after an operand: keywords only
    AfterOperand,
    /// After a keyword that only one of a few words can follow (`left` → join, `order` → by, `is` → null …): only those
    Follows(&'static [&'static str]),
    /// Any other expression (WHERE, SET, HAVING, function arguments …)
    Expr,
}

/// The words that can come after `prev` (and `prev2` before it), when there are only a few: nothing else is offered there.
/// `in_sources`: in a FROM / JOIN clause, where `left` / `right` start a join rather than a call
fn follows(prev: &str, prev2: &str, at: At, in_sources: bool, mysql: bool) -> Option<&'static [&'static str]> {
    // A statement's first word: at its start (a data-modifying CTE's too) or after a CTE's `)` (`with x as (…) delete`)
    let starts = prev2.is_empty() || at.after_paren;
    Some(match prev {
        "left" | "right" if in_sources => &["join", "outer join"],
        "full" if in_sources && !mysql => &["join", "outer join"],
        "inner" | "cross" | "outer" if in_sources => &["join"],
        "natural" if in_sources => if mysql { &["join", "left join", "right join"] } else { &["join", "left join", "right join", "full join", "inner join"] },
        "order" | "group" => &["by"],
        // `partition` is a common column name: `by` only at the start of `over (…)` / `window w as (…)` or after DDL's `)`
        "partition" if prev2.is_empty() && at.window || at.after_paren => &["by"],
        "not" if prev2 == "is" => &["null", "true", "false", "distinct from"],
        "is" => &["null", "not null", "true", "false", "not", "distinct from"],
        "nulls" => &["first", "last"],
        // Not `on delete` (a foreign key's action), `after insert` (a trigger), `grant insert`, MERGE's `then delete` …
        "insert" if starts => if mysql { &["into", "ignore", "low_priority", "high_priority"] } else { &["into"] },
        "delete" if starts => if mysql { &["from", "low_priority", "quick", "ignore"] } else { &["from"] },
        "union" | "intersect" | "except" => if mysql { &["select", "all", "distinct", "values"] } else { &["select", "all", "distinct", "values", "table"] },
        "create" if prev2.is_empty() => if mysql {
            &["table", "view", "index", "unique index", "schema", "database", "function", "procedure", "trigger", "sequence", "temporary", "role", "user", "event", "or replace"]
        } else {
            &[
                "table", "view", "index", "unique index", "schema", "database", "function", "procedure", "trigger", "sequence", "materialized view", "temporary", "type", "extension", "role", "user", "policy", "domain", "or replace",
            ]
        },
        "alter" if prev2.is_empty() && mysql => &["table", "view", "index", "schema", "database", "function", "procedure", "trigger", "sequence", "user", "event"],
        "drop" if prev2.is_empty() && mysql => &["table", "view", "index", "schema", "database", "function", "procedure", "trigger", "sequence", "temporary", "role", "user", "event"],
        "alter" | "drop" if prev2.is_empty() => &["table", "view", "index", "schema", "database", "function", "procedure", "trigger", "sequence", "materialized view", "type", "extension", "role", "user", "policy", "domain"],
        _ => return None,
    })
}

/// Where the word before the caret's word is, for [`follows`]
#[derive(Clone, Copy)]
struct At {
    /// The caret is inside `over (` or `window w as (`
    window: bool,
    /// The word comes right after a `)`
    after_paren: bool,
}

/// Words a statement can't end with: a word after them continues the statement
const CONTINUES: [&str; 31] = [
    "select", "from", "where", "and", "or", "not", "on", "join", "by", "set", "values", "into", "as", "in", "is", "like", "ilike", "between", "when", "then", "else", "case", "having", "limit", "offset", "with", "union", "all",
    "distinct", "update", "delete",
];

struct Caret {
    sc: Scope,
    /// The caret's block
    block: usize,
    /// Tokens before the typed word at the caret's parenthesis depth, nearest first
    back: Vec<usize>,
    /// The `(` the caret is inside (in the caret's block), if any
    open: Option<usize>,
    spot: Spot,
    mysql: bool,
}

impl Caret {
    /// `text` with the word being typed (`word` bytes before `caret`) blanked, so it is never read as an alias or a name
    fn new(text: &str, caret: usize, word: usize, mysql: bool) -> Self {
        let start = caret - word;
        let blanked = format!("{}{}{}", &text[..start], " ".repeat(word), &text[caret..]);
        let sc = Scope::new(&blanked, mysql);
        let block = sc.block_at(start);
        let before: Vec<usize> = (0..sc.toks.len()).take_while(|&i| sc.toks[i].e <= start).collect();
        let mut back = vec![];
        let mut open = None;
        let mut depth = 0;
        for &i in before.iter().rev() {
            if sc.punct(i, ")") {
                depth += 1;
            } else if sc.punct(i, "(") {
                if depth == 0 {
                    open = Some(i);
                    break;
                }
                depth -= 1;
            } else if depth == 0 {
                back.push(i);
            }
        }
        let mut c = Caret { sc, block, back, open, spot: Spot::Expr, mysql };
        c.spot = c.spot(text, start);
        c
    }

    fn at(&self) -> At {
        let sc = &self.sc;
        let window = self.open.is_some_and(|o| o > 0 && (sc.is(o - 1, "over") || sc.is(o - 1, "as")));
        let after_paren = self.back.first().is_some_and(|&i| i > 0 && sc.punct(i - 1, ")"));
        At { window, after_paren }
    }

    fn word_at(&self, k: usize) -> String {
        self.back.get(k).map(|&i| self.sc.t(i).to_lowercase()).unwrap_or_default()
    }

    fn spot(&self, text: &str, start: usize) -> Spot {
        let sc = &self.sc;
        let b = &sc.blocks[self.block];
        if self.back.is_empty() && self.open.is_none() {
            return Spot::Start;
        }
        // INSERT INTO t (‸
        if let (Some(o), Some((a, _))) = (self.open, b.insert_cols)
            && o + 1 == a
        {
            return Spot::InsertColumns;
        }
        let prev = self.back.first().copied();
        let prev_word = self.word_at(0);
        let prev_kind = prev.map(|i| sc.toks[i].k);
        // A word starting its own line after something a statement can end with
        let line_start = text[..start].rfind('\n').is_some_and(|nl| text[nl + 1..start].trim().is_empty());
        let ends = match prev_kind {
            Some(K::Word) => !is_kw(&prev_word, &CONTINUES),
            Some(K::Quoted | K::Num | K::Str) => true,
            Some(K::Punct) => prev_word == ")" || prev_word == "*",
            None => false,
        };
        let typed = text[start..].chars().take_while(|c| is_word(*c)).collect::<String>().to_lowercase();
        // Only when the word can't go on the statement: `an` (and), `de` (desc) and `or` keep the clause's keywords first
        let statement_only = STATEMENT_KEYWORDS.iter().any(|k| k.starts_with(&typed)) && !keywords().any(|k| !STATEMENT_KEYWORDS.contains(&k) && k.starts_with(&typed));
        if self.open.is_none() && line_start && ends && !typed.is_empty() && statement_only {
            return Spot::StatementFirst;
        }
        // `from` inside a call's parentheses (`extract(year from x)`, `substring(x from 2)`, `trim(both from x)`) or in
        // `is [not] distinct from` is part of an expression, not a FROM clause
        let in_call = self.open.is_some_and(|o| !sc.is_query(o));
        let expr_from = |i: usize| sc.is(i, "from") && (in_call || i > 0 && sc.is(i - 1, "distinct"));
        // The clause the caret is in: the nearest clause word at its depth
        let clause = self.back.iter().map(|&i| (i, sc.t(i).to_lowercase())).find(|(i, w)| {
            sc.toks[*i].k == K::Word
                && !expr_from(*i)
                && matches!(w.as_str(), "select" | "from" | "join" | "straight_join" | "on" | "using" | "where" | "by" | "having" | "set" | "values" | "into" | "update" | "returning" | "limit" | "when" | "then" | "else" | "case")
        });
        // Right after an operand (a name, a number, a string, `)`, `null` …) or after `is` / `is not` in an expression: only
        // keywords come there (`and`, `desc`, `null` …), not another column or a function
        let operand = match prev_kind {
            Some(K::Word) => !is_keyword(&prev_word) || matches!(prev_word.as_str(), "null" | "true" | "false" | "current_date" | "current_timestamp"),
            Some(K::Quoted | K::Num | K::Str) => true,
            Some(K::Punct) => prev_word == ")",
            None => false,
        };
        let in_sources = clause.as_ref().is_some_and(|(_, w)| matches!(w.as_str(), "from" | "join" | "straight_join" | "update" | "into"));
        if prev_kind == Some(K::Word)
            && let Some(words) = follows(&prev_word, &self.word_at(1), self.at(), in_sources, self.mysql)
        {
            return Spot::Follows(words);
        }
        if operand && !in_sources {
            return Spot::AfterOperand;
        }
        let Some((ci, mut cw)) = clause else { return Spot::Expr };
        if cw == "straight_join" {
            cw = "join".into();
        }
        let is_ref = prev.is_some_and(|i| sc.toks[i].k == K::Word && !expr_from(i) && is_ref_keyword(&prev_word) && (prev_word != "update" || self.back.len() == 1 || sc.is(self.back[1], "with") || sc.punct(self.back[1], ")")));
        if is_ref {
            return Spot::TableRef { join: prev_word == "join" || prev_word == "straight_join" };
        }
        match cw.as_str() {
            "from" | "join" | "update" | "into" if prev_word == "," && cw == "from" => Spot::TableRef { join: false },
            "from" | "join" | "update" | "into" => {
                if matches!(prev_kind, Some(K::Word | K::Quoted)) && !is_keyword(&prev_word) || prev_word == ")" {
                    Spot::AfterSource
                } else {
                    Spot::Expr
                }
            }
            "on" => Spot::On { first: prev == Some(ci) },
            "select" => Spot::SelectList,
            "by" => match self.back.iter().position(|&i| i == ci).and_then(|p| self.back.get(p + 1)).map(|&i| sc.t(i).to_lowercase()) {
                Some(w) if w == "group" => Spot::GroupBy,
                Some(w) if w == "order" => Spot::OrderBy,
                _ => Spot::Expr,
            },
            _ => Spot::Expr,
        }
    }

    /// The sources visible at the caret
    fn sources(&self) -> impl Iterator<Item = (usize, &Source)> {
        visible(&self.sc, self.block)
    }
}

/// The sources visible in block `b`: its own, then those of the enclosing blocks it sees (a CTE body or a FROM subquery that
/// isn't LATERAL sees none of them)
fn visible(sc: &Scope, b: usize) -> impl Iterator<Item = (usize, &Source)> {
    sc.sees(b).flat_map(move |b| sc.blocks[b].sources.iter().map(move |s| (b, s)))
}

/// The source a qualifier (an alias, a table's name, a CTE) names among those visible in block `b`, innermost first
fn named_in<'s>(sc: &'s Scope, b: usize, q: &str) -> Option<(usize, &'s Source)> {
    let q = q.to_lowercase();
    visible(sc, b).find(|(_, s)| s.alias.as_ref().is_some_and(|a| a.to_lowercase() == q)).or_else(|| visible(sc, b).find(|(_, s)| s.alias.is_none() && s.name().is_some_and(|n| n.to_lowercase() == q)))
}

type Columns = Vec<(String, String)>;

/// The columns of a statement's sources and query blocks, each block's worked out once: a chain of CTEs or subqueries reads
/// every block once, not once per select item of each block above it
struct Resolver<'a> {
    m: &'a Meta,
    sc: &'a Scope,
    db: &'a str,
    /// Per block: not asked yet (None), or its columns (Some(None) while they are being worked out, so a CTE reading itself is unknown)
    memo: std::cell::RefCell<Vec<Option<Option<Columns>>>>,
}

impl<'a> Resolver<'a> {
    fn new(m: &'a Meta, sc: &'a Scope, db: &'a str) -> Self {
        Resolver { m, sc, db, memo: std::cell::RefCell::new(vec![None; sc.blocks.len()]) }
    }

    /// A source's columns: a table's, a subquery's or a CTE's select list (`*` expanded), its column aliases. None when they
    /// can't be told (a function, a table that isn't loaded, an expression without a name in a select list …). `block` holds it
    fn source(&self, block: usize, s: &Source) -> Option<Columns> {
        if s.opaque {
            return None;
        }
        let mut cols = if let Some(sub) = s.sub {
            // `(values …) t(a, b)`: the column aliases name its columns
            if self.sc.blocks[sub].head == "values" && !s.cols.is_empty() {
                return Some(s.cols.iter().map(|n| (n.clone(), String::new())).collect());
            }
            self.block(sub)?
        } else {
            let r = s.table.as_deref()?;
            match (!r.contains('.')).then(|| self.sc.cte(block, r)).flatten() {
                Some(c) if !c.cols.is_empty() => c.cols.iter().map(|n| (n.clone(), String::new())).collect(),
                Some(c) => self.block(c.block?)?,
                None => find_table(self.m, r, self.db)?.columns.clone(),
            }
        };
        for (i, n) in s.cols.iter().enumerate() {
            if let Some(c) = cols.get_mut(i) {
                c.0 = n.clone();
            }
        }
        Some(cols)
    }

    /// The columns a query block returns: its select list's names, `*` as its sources' columns (a VALUES list's: unknown)
    fn block(&self, b: usize) -> Option<Columns> {
        if let Some(done) = &self.memo.borrow()[b] {
            return done.clone();
        }
        self.memo.borrow_mut()[b] = Some(None);
        let out = self.work_out(b);
        self.memo.borrow_mut()[b] = Some(out.clone());
        out
    }

    fn work_out(&self, b: usize) -> Option<Columns> {
        let (sc, blk) = (self.sc, &self.sc.blocks[b]);
        if blk.head == "values" {
            return None;
        }
        let srcs: Vec<Option<Columns>> = blk.sources.iter().map(|s| self.source(b, s)).collect();
        let mut out = vec![];
        for it in &blk.items {
            match &it.star {
                Some(None) => {
                    for cs in &srcs {
                        out.extend(cs.clone()?);
                    }
                }
                Some(Some(q)) => {
                    let k = blk.sources.iter().position(|s| s.name().is_some_and(|n| n.eq_ignore_ascii_case(q)))?;
                    out.extend(srcs[k].clone()?);
                }
                None => {
                    let name = it.alias.clone().or_else(|| {
                        // A column (`x`, `t.x`) keeps its name
                        let (x, y) = it.toks;
                        let last = y - 1;
                        let plain = matches!(sc.toks[last].k, K::Word | K::Quoted) && (y - x == 1 || y - x == 3 && sc.punct(x + 1, "."));
                        plain.then(|| sc.name(last))
                    })?;
                    let ty = srcs.iter().flatten().find_map(|cs| cs.iter().find(|c| c.0.eq_ignore_ascii_case(&name))).map(|c| c.1.clone()).unwrap_or_default();
                    out.push((name, ty));
                }
            }
        }
        Some(out)
    }
}

/// The ON conditions a joined source can take from foreign keys to the sources before it in its block: `o.user_id = u.id`
fn fk_conditions(m: &Meta, c: &Caret, db: &str) -> Vec<(String, String)> {
    let blk = &c.sc.blocks[c.block];
    let Some((j, joined)) = blk.sources.iter().enumerate().rev().find(|(_, s)| s.join) else { return vec![] };
    let Some(ji) = joined.table.as_deref().and_then(|r| find_idx(m, r, db)) else { return vec![] };
    let jt = &m.tables[ji];
    let jn = joined.name().unwrap_or_default().to_string();
    let mut out = vec![];
    for s in &blk.sources[..j] {
        let Some(ti) = s.table.as_deref().and_then(|r| find_idx(m, r, db)) else { continue };
        let t = &m.tables[ti];
        let sn = s.name().unwrap_or_default();
        for l in m.links[ji].iter().filter(|l| l.other == ti) {
            let cond: Vec<String> = l.mine.iter().zip(&l.theirs).map(|(a, b)| format!("{sn}.{b} = {jn}.{a}")).collect();
            let (from, to) = if l.out { (&jt.name, &t.name) } else { (&t.name, &jt.name) };
            out.push((cond.join(" and "), format!("fk {from} → {to}")));
        }
    }
    out
}

/// Build candidates at the caret. `text` is the caret's statement ([`at_caret`]: aliases and table refs resolve in it, not in the
/// rest of the script), `caret` the byte offset in it, `db` the tab's DB (in MySQL, schema = DB; "" for none), `mysql` reads
/// MySQL json paths in value spots and its functions. Returns: (length of the prefix to replace in chars, candidates)
pub fn suggest(m: &Meta, text: &str, caret: usize, db: &str, mysql: bool, o: Options) -> (usize, Vec<Item>) {
    let (n, mut items, rest) = suggest_lead(m, text, caret, db, mysql, o);
    if let Some(r) = rest {
        items.extend(rest_items(m, &r, o));
    }
    (n, items)
}

/// Which keywords come in the rest
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kw {
    None,
    /// Statement keywords and templates only
    Statement,
    /// Statement keywords and templates, then the others
    StatementFirst,
    /// The others, then statement keywords (right after an operand: `id de` → desc before delete)
    StatementLast,
    All,
}

/// What the candidates after the leading ones (functions, tables, schemas, keywords) depend on. In the GUI it stays the same
/// while typing, so the WASM index keeps their JS array instead of building thousands of items on every key
#[derive(Clone, Debug, PartialEq)]
pub struct Rest {
    /// The typed word, lowercase, when candidates are filtered by it here ("" when the editor filters)
    filter: String,
    db: String,
    keywords: Kw,
    tables: bool,
    functions: bool,
    mysql: bool,
    /// Tables already in the leading candidates (FK-related ones after JOIN)
    skip: Vec<usize>,
}

/// `suggest` in two parts: (prefix length, the leading candidates, what the rest comes from). None = no rest (after a `.`, in quotes)
pub fn suggest_lead(m: &Meta, text: &str, caret: usize, db: &str, mysql: bool, o: Options) -> (usize, Vec<Item>, Option<Rest>) {
    let db = db_schema(db, mysql);
    let all = &m.tables;
    let caret = caret.min(text.len());
    let before = &text[text[..caret].rfind('\n').map_or(0, |i| i + 1)..caret];
    let prefix: String = before.chars().rev().take_while(|c| is_word(*c)).collect::<Vec<_>>().into_iter().rev().collect();
    let head = &before[..before.len() - prefix.len()];
    let p = prefix.to_lowercase();
    let n = prefix.chars().count();
    if o.quiet_in_value_quotes && context(before, mysql).is_some_and(|c| c.quoted) {
        return (n, vec![], None);
    }
    let starts = |lower: &str| kept(lower, &p, o);
    let matches = |label: &str| keeps_label(label, &p, o);
    let sorted = |mut v: Vec<Item>| {
        if o.editor_filters {
            v.sort_by(|a, b| a.label.cmp(&b.label));
        }
        v
    };
    let c = Caret::new(text, caret, prefix.len(), mysql);
    let rc = Resolver::new(m, &c.sc, db);
    let col = |name: &str, ty: &str, from: &str| Item::new(name, if from.is_empty() { ty.to_string() } else if ty.is_empty() { from.to_string() } else { format!("{ty} · {from}") }, name, ItemKind::Column);

    if let Some(h) = head.strip_suffix('.') {
        // The qualifier: a quoted name back to its opening quote (it may hold spaces, hyphens, emoji), else the word before the dot
        let start = match h.strip_suffix(['"', '`']) {
            Some(inner) => inner.rfind(['"', '`']).unwrap_or(h.len()),
            None => h.trim_end_matches(is_word).len(),
        };
        let q = h[start..].trim_matches(['"', '`']).to_lowercase();
        if q.is_empty() {
            return (n, vec![], None);
        }
        // A schema of that name wins where a table name goes (`from sales.`) and over a table the statement doesn't name
        let schema = m.lower.iter().any(|(ls, _)| *ls == q);
        let at_ref = h[..start].trim_end().rsplit(|c: char| !is_word(c)).next().is_some_and(is_ref_keyword);
        // alias. / table. / cte. → columns
        let hit = if schema && at_ref { None } else { named_in(&c.sc, c.block, &q) };
        let cols = match hit {
            Some((b, s)) => rc.source(b, s),
            None if schema => None,
            None => c.sc.cte(c.block, &q).and_then(|x| if x.cols.is_empty() { rc.block(x.block?) } else { Some(x.cols.iter().map(|n| (n.clone(), String::new())).collect()) }).or_else(|| find_table(m, &q, db).map(|t| t.columns.clone())),
        };
        if let Some(cols) = cols {
            let items = cols.iter().filter(|(c, _)| matches(c)).map(|(c, ty)| col(c, ty, "")).collect();
            return (n, items, None);
        }
        // schema. → tables
        let items = all
            .iter()
            .zip(&m.lower)
            .filter(|(_, (ls, ln))| *ls == q && starts(ln))
            .map(|(t, _)| Item::new(t.name.clone(), format!("{} cols", t.columns.len()), t.name.clone(), ItemKind::Table))
            .collect();
        return (n, sorted(items), None);
    }

    let filter = if o.editor_filters { String::new() } else { p.clone() };
    let any_kw = !p.is_empty() || o.keywords_on_empty;
    let rest = |keywords: Kw, tables: bool, functions: bool, skip: Vec<usize>| Some(Rest { filter: filter.clone(), db: db.to_string(), keywords: if any_kw { keywords } else { Kw::None }, tables, functions: functions && any_kw, mysql, skip });

    // `*` (or `x.*`) in a select list, asked for: the columns it stands for, in the order the database returns them. None when a
    // source it covers has columns that can't be told: a list missing them would return other columns
    if p.is_empty() && c.spot == Spot::SelectList && before.ends_with('*') {
        let blk = &c.sc.blocks[c.block];
        let it = blk.items.iter().find(|it| c.sc.toks.get(it.toks.1 - 1).is_some_and(|t| t.e == caret) && it.star.is_some());
        if let Some(it) = it {
            let bare = it.star == Some(None);
            let covered: Vec<&Source> = blk.sources.iter().filter(|s| match &it.star {
                Some(Some(q)) => s.name().is_some_and(|x| x.eq_ignore_ascii_case(q)),
                _ => true,
            }).collect();
            let known: Option<Vec<Columns>> = covered.iter().map(|s| rc.source(c.block, s)).collect();
            // (qualifier, column)
            let mut names: Vec<(Option<String>, String)> = vec![];
            for (s, cols) in covered.iter().zip(known.unwrap_or_default()) {
                let q = match &it.star {
                    Some(Some(q)) => Some(q.clone()),
                    _ if blk.sources.len() == 1 => None,
                    _ => s.name().map(str::to_string),
                };
                let mine: Vec<(Option<String>, String)> = cols.into_iter().map(|(cn, _)| (q.clone(), cn)).collect();
                // `*` over USING / NATURAL: the join columns once (unqualified) and first, then the other columns of each side
                let joined: Vec<String> = match () {
                    _ if !bare || !s.join => vec![],
                    _ if !s.using.is_empty() => s.using.clone(),
                    _ if s.natural => mine.iter().filter(|(_, n)| names.iter().any(|(_, x)| x.eq_ignore_ascii_case(n))).map(|(_, n)| n.clone()).collect(),
                    _ => vec![],
                };
                let is_joined = |n: &str| joined.iter().any(|j| j.eq_ignore_ascii_case(n));
                let mut next: Vec<(Option<String>, String)> = joined.iter().map(|j| (None, j.clone())).collect();
                next.extend(names.drain(..).filter(|(_, n)| !is_joined(n)));
                next.extend(mine.into_iter().filter(|(_, n)| !is_joined(n)));
                names = next;
            }
            if !names.is_empty() {
                let star = c.sc.text_of(it.toks.0, it.toks.1);
                let list = names.iter().map(|(q, n)| q.as_ref().map_or(n.clone(), |q| format!("{q}.{n}"))).collect::<Vec<_>>().join(", ");
                return (star.chars().count(), vec![Item::new(format!("expand {star}"), list.clone(), snip(&list), ItemKind::Snippet)], None);
            }
        }
    }

    // The columns of the sources in scope at the caret (its block's first)
    let mut seen: HashSet<String> = HashSet::new();
    let mut columns: Vec<Item> = vec![];
    let scope_columns = |columns: &mut Vec<Item>, seen: &mut HashSet<String>| {
        for (b, s) in c.sources() {
            let Some(cols) = rc.source(b, s) else { continue };
            let from = s.table.as_deref().map(|t| t.rsplit('.').next().unwrap_or(t).to_string()).or(s.alias.clone()).unwrap_or_default();
            for (cn, ty) in cols {
                if matches(&cn) && seen.insert(cn.clone()) {
                    columns.push(col(&cn, &ty, &from));
                }
            }
        }
    };
    let blk = &c.sc.blocks[c.block];
    match c.spot.clone() {
        Spot::Start => (n, vec![], rest(Kw::Statement, false, false, vec![])),
        Spot::StatementFirst => (n, vec![], rest(Kw::StatementFirst, false, false, vec![])),
        Spot::AfterOperand => (n, vec![], rest(Kw::StatementLast, false, false, vec![])),
        Spot::Follows(words) => (n, words.iter().filter(|w| starts(w)).map(|w| Item::new(*w, "keyword", *w, ItemKind::Keyword)).collect(), None),
        Spot::TableRef { join } => {
            let mut lead: Vec<Item> = vec![];
            for b in c.sc.chain(c.block) {
                for cte in &c.sc.blocks[b].ctes {
                    if matches(&cte.name) && !lead.iter().any(|i| i.label == cte.name) {
                        lead.push(Item::new(cte.name.clone(), "cte", cte.name.clone(), ItemKind::Table));
                    }
                }
            }
            let mut skip = vec![];
            if join {
                // Tables with a foreign key to (or from) the statement's tables first
                let mut related: Vec<(usize, String)> = vec![];
                for s in &blk.sources {
                    let Some(ti) = s.table.as_deref().and_then(|r| find_idx(m, r, db)) else { continue };
                    let t = &m.tables[ti];
                    for l in &m.links[ti] {
                        if !related.iter().any(|r| r.0 == l.other) {
                            related.push((l.other, t.name.clone()));
                        }
                    }
                }
                for (i, via) in related {
                    let t = &m.tables[i];
                    if starts(&m.lower[i].1) {
                        lead.push(Item::new(t.name.clone(), format!("{} · fk {via}", t.schema), table_insert(t, db), ItemKind::Table));
                        skip.push(i);
                    }
                }
            }
            skip.sort_unstable();
            (n, lead, rest(Kw::All, true, false, skip))
        }
        Spot::AfterSource => {
            let mut lead = vec![];
            if blk.sources.last().is_some_and(|s| s.join && !s.cond) {
                for (cond, detail) in fk_conditions(m, &c, db) {
                    let label = format!("on {cond}");
                    if keeps_label(&label, &p, o) {
                        lead.push(Item::new(label.clone(), detail, label, ItemKind::Join));
                    }
                }
            }
            // INSERT INTO t ‸ → (its columns) values (…)
            if blk.head == "insert" && blk.insert_cols.is_none()
                && let Some(t) = blk.sources.first().and_then(|s| s.table.as_deref()).and_then(|r| find_table(m, r, db))
                && !t.columns.is_empty()
            {
                let names: Vec<&str> = t.columns.iter().map(|c| c.0.as_str()).collect();
                let label = format!("({}) values (…)", names.join(", "));
                let stops: Vec<String> = names.iter().enumerate().map(|(i, c)| format!("${{{}:{}}}", i + 1, snip(c))).collect();
                if p.is_empty() || o.editor_filters {
                    lead.push(Item::new(label, format!("{} columns", names.len()), format!("({}) values ({})", names.iter().map(|c| snip(c)).collect::<Vec<_>>().join(", "), stops.join(", ")), ItemKind::Snippet));
                }
            }
            (n, lead, rest(Kw::All, false, false, vec![]))
        }
        Spot::InsertColumns => {
            let used: HashSet<String> = blk.insert_cols.map(|(a, b)| (a..b).filter(|&i| matches!(c.sc.toks[i].k, K::Word | K::Quoted)).map(|i| c.sc.name(i).to_lowercase()).collect()).unwrap_or_default();
            let cols = blk.sources.first().and_then(|s| rc.source(c.block, s)).unwrap_or_default();
            let lead = cols.iter().filter(|(cn, _)| matches(cn) && !used.contains(&cn.to_lowercase())).map(|(cn, ty)| col(cn, ty, "")).collect();
            (n, lead, None)
        }
        spot => {
            let mut lead = vec![];
            match spot {
                Spot::On { first: true } => {
                    for (cond, detail) in fk_conditions(m, &c, db) {
                        if keeps_label(&cond, &p, o) {
                            lead.push(Item::new(cond.clone(), detail, cond, ItemKind::Join));
                        }
                    }
                }
                Spot::GroupBy => {
                    // The select list's expressions that aren't aggregates (or `*`)
                    for it in &blk.items {
                        let (x, y) = it.toks;
                        let end = if it.alias.is_some() { y - if c.sc.is(y.saturating_sub(2), "as") { 2 } else { 1 } } else { y };
                        if it.star.is_some() || end <= x || (x..end).any(|i| c.sc.toks[i].k == K::Word && is_aggregate(c.sc.t(i)) && c.sc.punct(i + 1, "(")) {
                            continue;
                        }
                        let expr = c.sc.text_of(x, end).to_string();
                        if matches(&expr) && seen.insert(expr.clone()) {
                            lead.push(Item::new(expr.clone(), "select list", expr, ItemKind::Column));
                        }
                    }
                }
                Spot::OrderBy => {
                    for a in blk.items.iter().filter_map(|it| it.alias.as_ref()) {
                        if matches(a) && seen.insert(a.clone()) {
                            lead.push(Item::new(a.clone(), "alias", a.clone(), ItemKind::Column));
                        }
                    }
                }
                _ => {}
            }
            scope_columns(&mut columns, &mut seen);
            lead.extend(sorted(columns));
            (n, lead, rest(Kw::All, true, true, vec![]))
        }
    }
}

/// The templates the typed word `p` keeps (a template typed out in full stays: `sel` is how it's picked)
fn templates(p: &str, o: Options) -> Vec<Item> {
    TEMPLATES.iter().filter(|t| o.editor_filters || p.is_empty() || t.0.starts_with(p)).map(|&(l, d, s)| Item::new(l, d, s, ItemKind::Snippet)).collect()
}

/// The candidates after the leading ones: functions, tables (schema.table outside public and the tab's DB), schemas, keywords
pub fn rest_items(m: &Meta, r: &Rest, o: Options) -> Vec<Item> {
    let (p, db) = (r.filter.as_str(), r.db.as_str());
    let mut items: Vec<Item> = vec![];
    if r.keywords == Kw::StatementFirst || r.keywords == Kw::Statement {
        let mut kws: Vec<&str> = STATEMENT_KEYWORDS.iter().copied().filter(|k| keeps_label(k, p, o)).collect();
        if o.editor_filters {
            kws.sort();
        }
        items.extend(kws.into_iter().map(|k| Item::new(k, "keyword", k, ItemKind::Keyword)));
        items.extend(templates(p, o));
    }
    if r.functions {
        let mut fs: Vec<Item> = m.functions.iter().filter(|f| keeps_label(&f.name, p, o)).map(Func::item).collect();
        fs.extend(builtins(r.mysql).filter(|f| keeps_label(&f.name, p, o)).map(|f| f.item()));
        if o.editor_filters {
            fs.sort_by(|a, b| a.label.cmp(&b.label));
        }
        items.extend(fs);
    }
    if r.tables {
        items.extend(m.by_label.iter().filter(|&&i| kept(&m.lower[i].1, p, o) && r.skip.binary_search(&i).is_err()).map(|&i| {
            let t = &m.tables[i];
            Item::new(t.name.clone(), t.schema.clone(), table_insert(t, db), ItemKind::Table)
        }));
        items.extend(m.schemas.iter().filter(|s| keeps_label(s, p, o)).map(|s| Item::new(s.clone(), "schema", s.clone(), ItemKind::Schema)));
    }
    let statement = |k: &&str| STATEMENT_KEYWORDS.contains(k);
    let kws: Vec<&str> = match r.keywords {
        Kw::None | Kw::Statement => vec![],
        Kw::StatementFirst => keywords().filter(|k| !statement(k) && keeps_label(k, p, o)).collect(),
        Kw::All if o.editor_filters => SORTED_KEYWORDS.clone(),
        Kw::All => keywords().filter(|k| keeps_label(k, p, o)).collect(),
        Kw::StatementLast => {
            let order: Vec<&str> = if o.editor_filters { SORTED_KEYWORDS.clone() } else { keywords().collect() };
            let (stmt, other): (Vec<&str>, Vec<&str>) = order.into_iter().filter(|k| keeps_label(k, p, o)).partition(statement);
            other.into_iter().chain(stmt).collect()
        }
    };
    items.extend(kws.into_iter().map(|k| Item::new(k, "keyword", k, ItemKind::Keyword)));
    items
}

/// The query a table tab's WHERE / ORDER BY field filters, with the field's text in it: `SELECT * FROM "schema"."table" WHERE <field>`
/// (or `ORDER BY <field>`), and the byte offset where the field's text starts in it. The names are quoted for [`suggest`] only
/// (it reads `"…"` refs in any dialect), not to be run
pub fn filter_query(schema: &str, table: &str, order_by: bool, field: &str) -> (String, usize) {
    let q = |s: &str| format!("\"{}\"", s.replace('"', ""));
    let from = if schema.is_empty() { q(table) } else { format!("{}.{}", q(schema), q(table)) };
    let head = format!("SELECT * FROM {from} {} ", if order_by { "ORDER BY" } else { "WHERE" });
    (format!("{head}{field}"), head.len())
}

/// Completion in a table tab's WHERE / ORDER BY field (the GUI's filter bar, the TUI's w / o input): the field completes as
/// [`filter_query`] with the caret (a byte offset in `field`) mapped into it, so the rules are the console's and the table's
/// columns come first. A list without an editor to filter it, so the TUI console's [`Options`]. Never inside a string literal, and
/// never table or schema names: the field belongs to its table, so only its columns, keywords and functions.
/// While typing (`explicit` off): candidates only while typing a word (not a number) or right after a `.`, and none once the word
/// is a whole keyword or column of the table: there ↩ applies the filter rather than picking a longer word (`is null` must not
/// become `is nulls`). Asked for (⌃Space, the TUI's ⇥): the console's candidates at any spot, keywords too at an empty word.
/// Returns (length of the word before the caret to replace, in chars; candidates)
#[allow(clippy::too_many_arguments)]
pub fn suggest_filter(m: &Meta, schema: &str, table: &str, order_by: bool, field: &str, caret: usize, db: &str, mysql: bool, explicit: bool) -> (usize, Vec<Item>) {
    let before = &field[..caret];
    if before.matches('\'').count() % 2 == 1 {
        return (0, vec![]);
    }
    if !explicit {
        let head = before.trim_end_matches(is_word);
        let typed = &before[head.len()..];
        let word = !typed.is_empty() && !typed.starts_with(|c: char| c.is_ascii_digit());
        if !(word || head.ends_with('.')) {
            return (0, vec![]);
        }
        let named = if schema.is_empty() { table.to_string() } else { format!("{schema}.{table}") };
        let column = || find_table(m, &named, db_schema(db, mysql)).is_some_and(|t| t.columns.iter().any(|(c, _)| c.eq_ignore_ascii_case(typed)));
        if is_keyword(typed) || column() {
            return (0, vec![]);
        }
    }
    let (text, at) = filter_query(schema, table, order_by, field);
    // The field's `"…"` names are PostgreSQL's quoting, so the query is read as PostgreSQL's; MySQL's functions still come
    let o = Options { keywords_on_empty: explicit, ..Options::TUI };
    let (n, mut items, rest) = suggest_lead(m, &text, at + caret, db, false, o);
    if let Some(r) = rest {
        items.extend(rest_items(m, &Rest { mysql, ..r }, o));
    }
    items.retain(|i| !matches!(i.kind, ItemKind::Table | ItemKind::Schema));
    // A field is plain text: a function writes its name and `(` (`()` without arguments)
    for i in items.iter_mut().filter(|i| i.kind == ItemKind::Function) {
        i.insert = snippet_text(&i.insert).0.trim_end_matches(')').to_string() + if i.insert.ends_with("()") { ")" } else { "" };
    }
    (n, items)
}

/// A candidate (lowercase) for the typed word `p` (lowercase): all when the editor filters or nothing is typed, else those that
/// start with it, without the fully typed word itself (so Enter goes to a newline)
fn kept(lower: &str, p: &str, o: Options) -> bool {
    o.editor_filters || p.is_empty() || (lower.starts_with(p) && lower != p)
}

/// `kept` for a label in any letter case (lowercased only when it is checked)
fn keeps_label(label: &str, p: &str, o: Options) -> bool {
    o.editor_filters || p.is_empty() || kept(&label.to_lowercase(), p, o)
}

// ───────── Signature help ─────────

/// The function call around the caret: its signatures (a user function's overloads, or the built-in one) and the parameter the
/// caret is at (0-based; may be past the last one for `…` lists)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Signature {
    pub signatures: Vec<SignatureItem>,
    pub active: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignatureItem {
    pub label: String,
    pub params: Vec<String>,
}

/// Words before `(` that aren't a function call
const NOT_CALLS: [&str; 16] = ["in", "exists", "values", "any", "all", "some", "over", "filter", "using", "on", "as", "from", "join", "into", "and", "or"];

/// The call the caret (a byte offset in the statement `text`) is inside, if its function is known
pub fn signature(m: &Meta, text: &str, caret: usize, mysql: bool) -> Option<Signature> {
    let caret = caret.min(text.len());
    let sc = Scope::new(text, mysql);
    let mut depth = 0;
    let mut commas = 0;
    let before: Vec<usize> = (0..sc.toks.len()).take_while(|&i| sc.toks[i].e <= caret).collect();
    // Inside a string or a quoted name: no call
    if sc.toks.iter().any(|t| t.s < caret && caret < t.e) {
        return None;
    }
    for &i in before.iter().rev() {
        if sc.punct(i, ")") {
            depth += 1;
        } else if sc.punct(i, "(") {
            if depth > 0 {
                depth -= 1;
                continue;
            }
            let name = i.checked_sub(1).filter(|&j| sc.toks[j].k == K::Word || sc.toks[j].k == K::Quoted)?;
            if sc.toks[name].k == K::Word && is_kw(sc.t(name), &NOT_CALLS) || sc.is_query(i) {
                return None;
            }
            let qual = name.checked_sub(2).filter(|&j| sc.punct(j + 1, ".")).map(|j| sc.name(j).to_lowercase());
            let fname = sc.name(name).to_lowercase();
            let user: Vec<&Func> = m.functions.iter().filter(|f| f.name.to_lowercase() == fname && qual.as_ref().is_none_or(|q| f.schema.to_lowercase() == *q)).collect();
            let fs: Vec<Func> = if user.is_empty() { builtins(mysql).filter(|f| f.name == fname && qual.is_none()).collect() } else { user.into_iter().cloned().collect() };
            if fs.is_empty() {
                return None;
            }
            let signatures = fs.iter().map(|f| SignatureItem { label: f.signature(), params: f.params() }).collect();
            return Some(Signature { signatures, active: commas });
        } else if depth == 0 && sc.punct(i, ",") {
            commas += 1;
        } else if depth == 0 && sc.toks[i].k == K::Punct && sc.t(i) == ";" {
            return None;
        }
    }
    None
}

// ───────── Unknown names ─────────

/// A table or column the statement's scope doesn't have, with the closest names that would fix it
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mark {
    /// Byte range in the text
    pub start: usize,
    pub end: usize,
    pub message: String,
    /// What to write instead, closest first
    pub fixes: Vec<String>,
}

/// Words that are never a column where a column could be (keywords, type names, date parts …)
const NOT_NAMES: &str = "over partition rows range groups unbounded preceding following current row filter within window exclude ties others no only \
array any some escape similar to overlaps at time zone year month day hour minute second epoch dow doy week quarter isodow microseconds milliseconds \
date timestamp timestamptz time interval varchar char character varying text int integer bigint smallint numeric decimal real double precision float \
boolean bool json jsonb uuid bytea leading trailing both for collate symmetric asymmetric isnull notnull unknown current_user session_user user \
localtime localtimestamp current_time current_schema current_catalog current_role div mod xor regexp rlike binary separator dual unsigned signed \
nowait skip locked share of conflict do nothing recursive materialized ordinality default excluded new old row_number value values key \
fetch next percent grouping sets cube rollup lock mode boolean language query expansion primary index \
distinctrow high_priority straight_join sql_small_result sql_big_result sql_buffer_result sql_no_cache sql_cache sql_calc_found_rows \
ctid xmin xmax cmin cmax tableoid oid";

static NOT_NAME_SET: std::sync::LazyLock<HashSet<&'static str>> = std::sync::LazyLock::new(|| NOT_NAMES.split_whitespace().chain(RESERVED.iter().copied()).collect());

/// Calls whose arguments aren't all expressions (`extract(year from x)`, `cast(x as int)`): no marks inside
const SPECIAL_CALLS: [&str; 12] = ["extract", "cast", "convert", "trim", "substring", "position", "overlay", "timestampdiff", "timestampadd", "date_add", "date_sub", "json_table"];

/// Unknown tables and columns of every statement of a script (byte offsets in it). See [`marks`]
pub fn script_marks(m: &Meta, script: &str, kind: crate::DbKind, db: &str) -> Vec<Mark> {
    let mysql = kind == crate::DbKind::Mysql;
    let created = created_names(script, mysql);
    let mut out = vec![];
    for (s, e) in crate::sql::split_script(script, kind) {
        let stmt = &script[s..e];
        out.extend(marks(m, stmt, db, mysql, &created).into_iter().map(|mk| Mark { start: mk.start + s, end: mk.end + s, ..mk }));
    }
    out
}

/// Tables and views the script creates (lowercase names, without their schema): not unknown though the metadata lacks them
fn created_names(script: &str, mysql: bool) -> HashSet<String> {
    let sc = Scope::new(script, mysql);
    let mut out = HashSet::new();
    for i in 0..sc.toks.len() {
        if !sc.is(i, "create") {
            continue;
        }
        let mut j = i + 1;
        while ["or", "replace", "temp", "temporary", "unlogged", "materialized", "global", "local"].iter().any(|w| sc.is(j, w)) {
            j += 1;
        }
        if !(sc.is(j, "table") || sc.is(j, "view")) {
            continue;
        }
        j += 1;
        if sc.is(j, "if") {
            j += 3;
        }
        let mut name = sc.name(j);
        while sc.punct(j + 1, ".") {
            j += 2;
            name = sc.name(j);
        }
        out.insert(name.to_lowercase());
    }
    out
}

/// Tables and columns of one statement that its scope doesn't have, as far as the metadata tells: nothing when it isn't loaded
/// (or stopped at its limit), for a schema it doesn't hold (another database), for CTEs, aliases, functions, select-list aliases,
/// and for columns of a block reading something whose columns aren't known. When unsure, no mark. `created`: tables the script creates
pub fn marks(m: &Meta, text: &str, db: &str, mysql: bool, created: &HashSet<String>) -> Vec<Mark> {
    if m.partial {
        return vec![];
    }
    let db = db_schema(db, mysql);
    let sc = Scope::new(text, mysql);
    let rc = Resolver::new(m, &sc, db);
    if !sc.blocks.first().is_some_and(|b| matches!(b.head.as_str(), "select" | "insert" | "update" | "delete" | "with" | "values")) {
        return vec![];
    }
    let mut out = vec![];
    let tok = |a: usize, z: usize| (sc.toks[a].s, sc.toks[z - 1].e);
    for (bi, b) in sc.blocks.iter().enumerate() {
        // Tables
        for s in &b.sources {
            let Some(r) = s.table.as_deref().filter(|_| !s.opaque) else { continue };
            let parts: Vec<&str> = r.split('.').collect();
            let name = parts.last().copied().unwrap_or(r);
            let created = created.contains(&name.to_lowercase());
            let unknown = match parts.as_slice() {
                [_] => sc.cte(bi, name).is_none() && !created && find_table(m, name, db).is_none() && !name.to_lowercase().starts_with("pg_") && !name.eq_ignore_ascii_case("dual"),
                // Another schema the metadata holds: only then can it tell (a schema it lacks may be another database's)
                [schema, _] => m.schemas.iter().any(|x| x.eq_ignore_ascii_case(schema)) && find_table(m, r, db).is_none() && !created,
                _ => false,
            };
            if unknown {
                let fixes = if let [schema, _] = parts.as_slice() {
                    closest(name, m.tables.iter().filter(|t| t.schema.eq_ignore_ascii_case(schema)).map(|t| t.name.as_str())).into_iter().map(|f| format!("{schema}.{f}")).collect()
                } else {
                    let ctes = sc.chain(bi).flat_map(|x| sc.blocks[x].ctes.iter().map(|c| c.name.clone()));
                    let names: Vec<String> = m.tables.iter().map(|t| table_insert(t, db)).chain(ctes).collect();
                    closest(name, names.iter().map(String::as_str))
                };
                let (start, end) = tok(s.toks.0, s.toks.1);
                out.push(Mark { start, end, message: format!("Unknown table \"{r}\""), fixes });
            }
        }
        // Columns: those the block's sources are known to lack. A source whose columns can't be told makes the block's bare names unsure
        type Cols = Option<Vec<(String, String)>>;
        let cols: Vec<(&Source, Cols)> = visible(&sc, bi).map(|(sb, s)| (s, rc.source(sb, s))).collect();
        let all_known = !cols.is_empty() && cols.iter().all(|c| c.1.is_some());
        let has = |cs: &[(String, String)], n: &str| cs.iter().any(|c| c.0.eq_ignore_ascii_case(n));
        let aliases: HashSet<String> = sc.chain(bi).flat_map(|x| sc.blocks[x].items.iter().filter_map(|it| it.alias.as_ref().map(|a| a.to_lowercase()))).collect();
        // INSERT INTO t (columns)
        if let (Some((x, y)), Some((target, Some(cs)))) = (b.insert_cols, cols.first()) {
            for i in (x..y).filter(|&i| matches!(sc.toks[i].k, K::Word | K::Quoted)) {
                let w = sc.name(i);
                if !has(cs, &w) {
                    let (start, end) = tok(i, i + 1);
                    out.push(Mark { start, end, message: format!("Unknown column \"{w}\" in {}", target.name().unwrap_or_default()), fixes: closest(&w, cs.iter().map(|c| c.0.as_str())) });
                }
            }
        }
        // The block's own tokens (nested blocks are checked as themselves)
        let (a, z) = b.toks;
        let mut i = a;
        let mut in_values = false;
        while i < z {
            if b.sources.iter().any(|s| s.span.0 <= i && i < s.span.1) || b.insert_cols.is_some_and(|(x, y)| x <= i && i < y) || b.skip.iter().any(|&(x, y)| x <= i && i < y) {
                i += 1;
                continue;
            }
            if sc.punct(i, "(") {
                let special = i > 0 && sc.toks[i - 1].k == K::Word && is_kw(sc.t(i - 1), &SPECIAL_CALLS);
                if sc.is_query(i) || special || in_values {
                    i = sc.close[i].unwrap_or(z) + 1;
                    continue;
                }
                i += 1;
                continue;
            }
            let tk = sc.toks[i];
            if sc.is(i, "values") {
                in_values = true;
            }
            if !matches!(tk.k, K::Word | K::Quoted) || in_values {
                i += 1;
                continue;
            }
            // A dotted name: q.col (three parts and more are left alone)
            let mut j = i;
            while sc.punct(j + 1, ".") && sc.toks.get(j + 2).is_some_and(|t| matches!(t.k, K::Word | K::Quoted)) {
                j += 2;
            }
            let prev = i.checked_sub(1).map(|p| sc.t(p)).unwrap_or("");
            // A call, a typed literal (`date '…'`), a cast's type, a variable, an alias being named, a window's name
            let skip = sc.punct(j + 1, "(") || sc.toks.get(j + 1).is_some_and(|t| t.k == K::Str) || sc.punct(j + 1, ".") || matches!(prev, "::" | "@" | ":" | "$" | ".") || i > 0 && ["as", "over", "window", "collate"].iter().any(|w| sc.is(i - 1, w))
                // `window w as (…)`, `, w2 as (…)`
                || sc.is(j + 1, "as") && sc.punct(j + 2, "(");
            let item_alias = b.items.iter().any(|it| it.alias.is_some() && it.toks.1 == j + 1 && it.toks.0 < i);
            if skip || item_alias {
                i = j + 1;
                continue;
            }
            if j == i + 2 {
                let (q, col) = (sc.name(i), sc.name(j));
                if let Some((sb, s)) = named_in(&sc, bi, &q)
                    && let Some(cs) = rc.source(sb, s)
                    && !has(&cs, &col)
                {
                    let (start, end) = tok(j, j + 1);
                    out.push(Mark { start, end, message: format!("Unknown column \"{col}\" in {q}"), fixes: closest(&col, cs.iter().map(|c| c.0.as_str())) });
                }
            } else if j == i && all_known {
                let w = sc.name(i);
                let lw = w.to_lowercase();
                let known = tk.k == K::Word && NOT_NAME_SET.contains(lw.as_str())
                    || aliases.contains(&lw)
                    || cols.iter().any(|(s, cs)| cs.as_ref().is_some_and(|cs| has(cs, &w)) || s.name().is_some_and(|n| n.eq_ignore_ascii_case(&w)))
                    || sc.cte(bi, &w).is_some();
                if !known {
                    let pool: Vec<&str> = cols.iter().flat_map(|c| c.1.iter().flatten().map(|c| c.0.as_str())).collect();
                    let (start, end) = tok(i, i + 1);
                    out.push(Mark { start, end, message: format!("Unknown column \"{w}\""), fixes: closest(&w, pool.into_iter()) });
                }
            }
            i = j + 1;
        }
    }
    out.sort_by_key(|m| m.start);
    out
}

/// The closest of `names` to `w` (case-insensitive edit distance; one holding the other's letters in order counts as close), closest first, at most 5
fn closest<'a>(w: &str, names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let lw = w.to_lowercase();
    let max = (lw.chars().count() / 3).max(2);
    let mut v: Vec<(usize, &str)> = names
        .filter_map(|n| {
            let ln = n.to_lowercase();
            let d = edit_distance(&lw, &ln);
            let within = |big: &str, small: &str| small.chars().count() > 2 && { let mut it = big.chars(); small.chars().all(|c| it.any(|x| x == c)) };
            (d <= max || within(&ln, &lw) || within(&lw, &ln)).then_some((d, n))
        })
        .collect();
    v.sort();
    v.dedup_by(|a, b| a.1 == b.1);
    v.into_iter().take(5).map(|(_, n)| n.to_string()).collect()
}

/// Levenshtein distance with transpositions (optimal string alignment), over chars
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

// ───────── Value · json key suggestions ─────────

/// Value suggestion spots: `status = '` · `in ('a', '` · `like '` → that column's values, `data->>'` · `data->'a'->>'` (MySQL `->>'$.a.`) → json keys
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HintAt {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub type_name: String,
    pub path: Vec<String>,
    pub mode: HintMode,
    /// Typing inside quotes
    pub quoted: bool,
    /// Characters already typed
    pub typed: String,
    /// keys: from the operator to the cursor (`->> '` + typed) — for replacing with an @> condition
    pub op: String,
    /// MySQL reads a backslash in a string literal as an escape
    pub mysql: bool,
}

/// values = column values, keys = json keys, pairs = @> conditions for frequent `key: value` pairs (PostgreSQL)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HintMode {
    Values,
    Keys,
    Pairs,
}

/// A value spot read from the text before the cursor alone (no metadata yet)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ctx {
    /// Alias or table (if absent, the column is looked up among the statement's tables)
    pub r#ref: Option<String>,
    pub column: String,
    pub path: Vec<String>,
    pub mode: HintMode,
    pub quoted: bool,
    #[serde(default)]
    pub typed: String,
    /// keys: from the operator to the cursor (`->> '` + typed); empty otherwise
    #[serde(default)]
    pub op: String,
}

static RES: std::sync::LazyLock<[regex::Regex; 4]> = std::sync::LazyLock::new(|| {
    let id = r#"(?:"[^"]+"|\w+)"#;
    let expr = format!(r"(?:({id})\.)?({id})((?:\s*->>?\s*(?:'[^']*'|\d+))*)");
    let op = r"\s*(=|<>|!=|@>|(?:not\s+)?i?like|(?:not\s+)?in\s*\((?:\s*(?:'[^']*'|[\w.-]+)\s*,)*)\s*";
    let my_val = format!(r#"(?:({id})\.)?({id})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)'"#);
    [
        regex::Regex::new(&format!(r#"(?:({id})\.)?({id})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)\.(\w*)$"#)).unwrap(),
        regex::Regex::new(&format!(r"(?i){my_val}{op}(?:'([^']*)|([\w.-]*))$")).unwrap(),
        regex::Regex::new(&format!(r"{expr}(\s*->>?\s*(?:'([^']*)|(\w*)))$")).unwrap(),
        regex::Regex::new(&format!(r"(?i){expr}{op}(?:'([^']*)|([\w.-]*))$")).unwrap(),
    ]
});

fn unq(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn path_of(seg: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"->>?\s*(?:'([^']*)'|(\d+))").unwrap());
    RE.captures_iter(seg).map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default()).collect()
}

fn my_path(p: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#"\.(?:"([^"]*)"|(\w+))"#).unwrap());
    RE.captures_iter(p).map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default()).collect()
}

const NOT_COLUMN: [&str; 13] = ["where", "and", "or", "not", "set", "on", "select", "by", "when", "then", "else", "case", "from"];

/// The value spot the text before the cursor ends in, if any
pub fn context(before: &str, mysql: bool) -> Option<Ctx> {
    let g = |c: &regex::Captures, i: usize| c.get(i).map(|m| m.as_str().to_string());
    let not_col = |s: &str| NOT_COLUMN.iter().any(|k| s.eq_ignore_ascii_case(k));
    let [my_keys, my_vals, keys, vals] = &*RES;
    if mysql {
        if let Some(c) = my_keys.captures(before) {
            return Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: my_path(&c[3]), mode: HintMode::Keys, quoted: true, typed: c[4].to_string(), op: String::new() });
        }
        if let Some(c) = my_vals.captures(before) {
            if &c[4] == "@>" {
                return None;
            }
            return Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: my_path(&c[3]), mode: HintMode::Values, quoted: c.get(5).is_some(), typed: g(&c, 5).or(g(&c, 6)).unwrap_or_default(), op: String::new() });
        }
    }
    if let Some(c) = keys.captures(before) {
        // MySQL '$.…' is handled above. A '$…' reaching here is not a complete path
        if c.get(5).is_some_and(|m| m.as_str().starts_with('$')) || not_col(&c[2]) {
            return None;
        }
        return Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: path_of(&c[3]), mode: HintMode::Keys, quoted: c.get(5).is_some(), typed: g(&c, 5).or(g(&c, 6)).unwrap_or_default(), op: c[4].to_string() });
    }
    let c = vals.captures(before)?;
    // Things after keywords (where = …) aren't columns
    if not_col(&c[2]) {
        return None;
    }
    let mode = if &c[4] == "@>" { HintMode::Pairs } else { HintMode::Values };
    if mysql && mode == HintMode::Pairs {
        return None;
    }
    Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: path_of(&c[3]), mode, quoted: c.get(5).is_some(), typed: g(&c, 5).or(g(&c, 6)).unwrap_or_default(), op: String::new() })
}

/// Columns worth suggesting: excluding binary · spatial · arrays; json only with a path (values) or when looking at keys · conditions
pub fn hintable(mode: HintMode, path: &[String], type_name: &str) -> bool {
    let ty = type_name.to_lowercase();
    let json = ty.contains("json");
    if mode != HintMode::Values {
        return json;
    }
    !["bytea", "blob", "binary", "geometry", "geography", "tsvector", "[]"].iter().any(|k| ty.contains(k)) && !ty.starts_with('_') && (!json || !path.is_empty())
}


/// Clause keywords that end a JOIN's ON condition (or come before one)
const NOT_ON: [&str; 8] = ["select", "from", "join", "where", "group", "having", "order", "set"];

/// Whether the end of `head` is in a JOIN's ON condition: the nearest clause keyword before it, at its parenthesis depth, is ON
fn in_join_on(head: &str, mysql: bool) -> bool {
    // One flag per open parenthesis, starting as the enclosing one's (a subquery's SELECT resets it); words in string literals don't count
    let mut on = vec![false];
    for t in crate::scope::lex(head, mysql) {
        let w = &head[t.s..t.e];
        match t.k {
            K::Punct if w == "(" => on.push(on[on.len() - 1]),
            K::Punct if w == ")" && on.len() > 1 => _ = on.pop(),
            K::Word if w.eq_ignore_ascii_case("on") => *on.last_mut().unwrap() = true,
            K::Word if NOT_ON.iter().any(|k| w.eq_ignore_ascii_case(k)) => *on.last_mut().unwrap() = false,
            _ => {}
        }
    }
    on.last() == Some(&true)
}

/// If the cursor is at a value · json key spot, that table · column (only for columns worth suggesting). `text` is the caret's
/// statement, `head` the statement up to the cursor ([`crate::sql::statement_head`]; its last line is the spot). The column is
/// looked up among the sources in scope at the caret. None in a JOIN's ON condition: the other side of `=` there is usually a
/// column, so the console's columns complete instead
pub fn hint_at(m: &Meta, text: &str, head: &str, db: &str, mysql: bool) -> Option<HintAt> {
    let db = db_schema(db, mysql);
    let before = &head[head.rfind('\n').map_or(0, |i| i + 1)..];
    let ctx = context(before, mysql)?;
    if in_join_on(head, mysql) {
        return None;
    }
    let sc = Scope::new(text, mysql);
    let block = sc.block_at(head.trim_start().len().min(text.len()));
    let candidates: Vec<&Source> = match &ctx.r#ref {
        Some(q) => named_in(&sc, block, q).map(|(_, s)| s).into_iter().collect(),
        None => visible(&sc, block).map(|(_, s)| s).collect(),
    };
    let lc = ctx.column.to_lowercase();
    let (t, (col, ty)) = candidates.iter().filter_map(|s| find_table(m, s.table.as_deref()?, db)).find_map(|t| t.columns.iter().find(|(c, _)| c.to_lowercase() == lc).map(|c| (t, c)))?;
    hintable(ctx.mode, &ctx.path, ty).then(|| HintAt { schema: t.schema.clone(), table: t.name.clone(), column: col.clone(), type_name: ty.clone(), path: ctx.path, mode: ctx.mode, quoted: ctx.quoted, typed: ctx.typed, op: ctx.op, mysql })
}


/// Text to insert for a value: inside quotes only doubled ' (and doubled \ on MySQL, where it escapes) plus the closing quote
/// when `close`; outside, a number or true/false in a numeric · boolean column as it is, the rest as sql::literal writes it
/// (PostgreSQL E'…' for a backslash, so standard_conforming_strings = off can't end the string early).
/// Bare needs both: the type is one by its name (a domain such as realm_name is not real), and the value itself is a plain literal
/// (a value is data from the DB, so anything else would be SQL typed into the statement)
pub fn hint_insert(value: &str, mode: HintMode, path: &[String], type_name: &str, quoted: bool, close: bool, mysql: bool) -> String {
    let esc = if mysql { value.replace('\\', "\\\\") } else { value.to_string() }.replace('\'', "''");
    let literal = crate::extract::number_text(value, true) || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false");
    let bare = mode == HintMode::Values && path.is_empty() && crate::extract::numeric_type(type_name) && literal;
    if quoted {
        if close { format!("{esc}'") } else { esc }
    } else if bare {
        value.to_string()
    } else {
        crate::sql::literal(if mysql { crate::DbKind::Mysql } else { crate::DbKind::Postgres }, Some(value))
    }
}

/// For keys spots with no keys (arrays of objects): text replacing the operator and what follows with `@> '[{…}]'`
pub fn pairs_insert(value: &str, op: &str) -> String {
    let lead = if op.starts_with(char::is_whitespace) { " " } else { "" };
    format!("{lead}@> '{}'", value.replace('\'', "''"))
}

/// `1 row`, `1,234 rows`
fn rows(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(d);
    }
    format!("{}{grouped} row{}", if n < 0 { "-" } else { "" }, if n == 1 { "" } else { "s" })
}

/// Fetched values → candidates (filtered by typed text unless the editor filters). `after` = the character after the cursor (a ' there is the closing quote).
/// as_pairs = a keys spot with no keys (array of objects), so @> conditions were fetched → replace including the operator (prefix = op length)
pub fn hint_items(h: &HintAt, hints: &[Hint], after: Option<char>, as_pairs: bool, o: Options) -> Vec<Item> {
    let p = h.typed.to_lowercase();
    let close = o.close_quotes && h.quoted && after != Some('\'');
    hints
        .iter()
        .filter(|x| as_pairs || o.editor_filters || p.is_empty() || (x.value.to_lowercase().starts_with(&p) && x.value.to_lowercase() != p))
        .map(|x| {
            if as_pairs {
                return Item { label: format!("@> '{}'", x.value), detail: rows(x.count), insert: pairs_insert(&x.value, &h.op), kind: ItemKind::Value };
            }
            let insert = hint_insert(&x.value, h.mode, &h.path, &h.type_name, h.quoted, close, h.mysql);
            Item { label: x.value.clone(), detail: format!("{}{}", rows(x.count), if h.mode == HintMode::Keys { " · key" } else { "" }), insert, kind: ItemKind::Value }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUI: Options = Options::TUI;

    fn meta() -> Meta {
        let t = |s: &str, n: &str, cols: &[&str]| CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|c| (c.to_string(), "text".to_string())).collect(), ..Default::default() };
        Meta::new(vec![t("public", "users", &["id", "user_name", "email"]), t("public", "orders", &["id", "user_id", "total"]), t("billing", "invoices", &["id", "amount"])])
    }

    /// `suggest` with the caret at the end of `before`, the start of the statement `text` up to the caret
    fn sg(m: &Meta, text: &str, before: &str, db: &str, mysql: bool, o: Options) -> (usize, Vec<Item>) {
        assert!(text.starts_with(before), "{text:?} doesn't start with {before:?}");
        suggest(m, text, before.len(), db, mysql, o)
    }

    fn labels(v: &[Item]) -> Vec<&str> {
        v.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn alias_dot_gives_columns() {
        let text = "select u. from users u join orders o on o.user_id = u.id";
        let (n, v) = sg(&meta(), text, "select u.", "app", false, TUI);
        assert_eq!(n, 0);
        assert_eq!(labels(&v), ["id", "user_name", "email"]);
        let text = "select u.us from users u join orders o on o.user_id = u.id";
        let (n, v) = sg(&meta(), text, "select u.us", "app", false, TUI);
        assert_eq!(n, 2);
        assert_eq!(labels(&v), ["user_name"]);
    }

    #[test]
    fn table_dot_and_schema_dot() {
        let (_, v) = sg(&meta(), "select orders. from orders", "select orders.", "app", false, TUI);
        assert_eq!(labels(&v), ["id", "user_id", "total"]);
        let (_, v) = sg(&meta(), "select * from billing.", "select * from billing.", "app", false, TUI);
        assert_eq!(labels(&v), ["invoices"]);
    }

    #[test]
    fn plain_prefix_orders_columns_tables_schemas_keywords() {
        let (n, v) = sg(&meta(), "select * from users where u", "select * from users where u", "app", false, TUI);
        assert_eq!(n, 1);
        assert_eq!(labels(&v), ["user_name", "upper", "unnest", "users", "using", "union", "update"], "columns, functions, tables, keywords");
        let (_, v) = sg(&meta(), "select * from inv", "select * from inv", "app", false, TUI);
        assert_eq!(v[0].insert, "billing.invoices");
    }

    /// A keyword only a few words can follow offers just those: `left j` → join (no json_agg …), `order` → by, `is` → null …
    #[test]
    fn fixed_next_words_only() {
        let m = meta();
        let cases: [(&str, bool, &[&str]); 12] = [
            ("select * from orders p left j", false, &["join"]),
            ("select * from orders p left ", false, &["join", "outer join"]),
            ("select * from orders p left outer ", false, &["join"]),
            ("select * from orders p full ", true, &[]),
            ("select * from users u inner ", false, &["join"]),
            ("select * from users order ", false, &["by"]),
            ("select count(*) from users group b", false, &["by"]),
            ("select * from users where email is n", false, &["null", "not null", "not"]),
            ("select * from users where email is not ", false, &["null", "true", "false", "distinct from"]),
            ("insert ", false, &["into"]),
            ("select 1 union ", false, &["select", "all", "distinct", "values", "table"]),
            ("create t", false, &["table", "trigger", "temporary", "type"]),
        ];
        for (text, mysql, want) in cases {
            let (_, v) = sg(&m, text, text, "app", mysql, TUI);
            if want.is_empty() {
                // MySQL has no FULL JOIN: `full` is a name there, so the usual candidates come
                assert!(!v.is_empty() && !labels(&v).contains(&"outer join"), "{text:?}: {:?}", labels(&v));
            } else {
                assert_eq!(labels(&v), want, "{text:?}");
            }
            // The GUI (Monaco filters) gets the same few, not every function and keyword
            let (_, g) = sg(&m, text, text, "app", mysql, Options::GUI);
            if !want.is_empty() {
                assert!(g.len() <= 20 && g.iter().all(|i| i.kind == ItemKind::Keyword), "{text:?}: {:?}", labels(&g));
            }
        }
        // `left(` in a select list is a call, not a join
        let (_, v) = sg(&m, "select left", "select left", "app", false, TUI);
        assert!(!labels(&v).contains(&"outer join"));
    }

    /// `partition`, `insert` and `delete` have a fixed next word only where they start a clause or a statement
    #[test]
    fn fixed_next_words_need_their_spot() {
        let m = meta();
        let only: [(&str, bool, &[&str]); 21] = [
            ("select sum(total) over (partition ", false, &["by"]),
            ("select sum(total) over w from orders window w as (partition ", false, &["by"]),
            ("create table t (id int) partition ", false, &["by"]),
            ("select * from users order by ", false, &[]),
            ("delete ", false, &["from"]),
            ("delete fr", false, &["from"]),
            ("with x as (select 1) delete fr", false, &["from"]),
            ("with x as (select 1) insert ", false, &["into"]),
            ("with x as (delete ", false, &["from"]),
            ("insert ", true, &["into", "ignore", "low_priority", "high_priority"]),
            ("insert i", true, &["into", "ignore"]),
            ("delete ", true, &["from", "low_priority", "quick", "ignore"]),
            ("select 1 union ", false, &["select", "all", "distinct", "values", "table"]),
            ("select 1 except v", false, &["values"]),
            ("select 1 intersect t", false, &["table"]),
            ("select 1 union ", true, &["select", "all", "distinct", "values"]),
            ("create m", false, &["materialized view"]),
            ("create e", true, &["event"]),
            ("create e", false, &["extension"]),
            ("drop t", true, &["table", "trigger", "temporary"]),
            ("alter t", false, &["table", "trigger", "type"]),
        ];
        for (text, mysql, want) in only {
            let (_, v) = sg(&m, text, text, "app", mysql, TUI);
            if want.is_empty() {
                assert!(!labels(&v).contains(&"by"), "{text:?}: {:?}", labels(&v));
            } else {
                assert_eq!(labels(&v), want, "{text:?}");
            }
        }
        // A column named `partition`, a foreign key's action, a trigger's event, a grant, MERGE's actions: the usual candidates
        let usual: [(&str, bool, &str); 6] = [
            ("select topic, partition fr", false, "from"),
            ("select * from t order by partition de", false, "desc"),
            ("create trigger tr after insert o", false, "on"),
            ("create trigger tr after delete o", false, "on"),
            ("create trigger tr after insert o", false, "or"),
            ("grant insert o", false, "on"),
        ];
        for (text, mysql, has) in usual {
            let (_, v) = sg(&m, text, text, "app", mysql, TUI);
            let l = labels(&v);
            assert!(l.contains(&has), "{text:?}: {l:?}");
        }
        for text in ["create table o (u int references users(id) on delete ", "merge into t using s on t.id = s.id when not matched then insert ", "merge into t using s on t.id = s.id when matched then delete "] {
            let l = labels(&sg(&m, text, text, "app", false, TUI).1).join(" ");
            assert!(l != "from" && l != "into", "{text:?}: {l}");
        }
        let (_, v) = sg(&m, "select * from t order by partition de", "select * from t order by partition de", "app", false, TUI);
        assert_eq!(labels(&v), ["desc", "delete"]);
        let (_, v) = sg(&m, "select topic, partition fr", "select topic, partition fr", "app", false, TUI);
        assert_eq!(labels(&v), ["from"]);
    }

    #[test]
    fn typed_word_is_not_suggested_again() {
        let (_, v) = sg(&meta(), "select * from users", "select * from users", "app", false, TUI);
        assert!(v.is_empty(), "{:?}", labels(&v));
        let (_, v) = sg(&meta(), "sel", "sel", "app", false, TUI);
        assert_eq!(labels(&v), ["select", "sel"], "a template typed out in full stays: that's how it's picked");
    }

    /// The GUI leaves filtering to Monaco: everything, sorted by label in each group, keywords on an empty prefix too
    #[test]
    fn gui_returns_everything_for_the_editor_to_filter() {
        let text = "select * from users where u";
        let (n, v) = sg(&meta(), text, text, "", false, Options::GUI);
        assert_eq!(n, 1);
        assert_eq!(&labels(&v)[..4], ["email", "id", "user_name", "abs"], "columns, then functions");
        let tables: Vec<&str> = v.iter().filter(|i| matches!(i.kind, ItemKind::Table | ItemKind::Schema)).map(|i| i.label.as_str()).collect();
        assert_eq!(tables, ["invoices", "orders", "users", "billing", "public"]);
        let fs: Vec<&str> = v.iter().filter(|i| i.kind == ItemKind::Function).map(|i| i.label.as_str()).collect();
        assert!(fs.is_sorted() && fs.contains(&"date_trunc") && !fs.contains(&"ifnull"), "PostgreSQL's functions, by label");
        assert_eq!(v.iter().filter(|i| i.kind == ItemKind::Keyword).count(), keywords().count());
        let kws: Vec<&str> = v.iter().filter(|i| i.kind == ItemKind::Keyword).map(|i| i.label.as_str()).collect();
        assert!(kws.is_sorted());
        let (_, v) = sg(&meta(), "select * from ", "select * from ", "", false, Options::GUI);
        assert!(v.iter().any(|i| i.kind == ItemKind::Keyword));
        let (_, v) = sg(&meta(), "select u.us from users u", "select u.us", "", false, Options::GUI);
        assert_eq!(labels(&v), ["id", "user_name", "email"], "columns after alias. keep table order");
    }

    #[test]
    fn gui_is_quiet_inside_value_quotes() {
        let before = "select * from users where email = 'us";
        assert!(sg(&meta(), before, before, "", false, Options::GUI).1.is_empty());
        assert!(!sg(&meta(), before, before, "", false, TUI).1.is_empty());
        // Not a value spot: a string anywhere else still completes
        assert!(!sg(&meta(), "select 'us", "select 'us", "", false, Options::GUI).1.is_empty());
    }

    #[test]
    fn filter_query_puts_the_field_after_its_clause() {
        assert_eq!(filter_query("public", "users", false, "id = 1"), (r#"SELECT * FROM "public"."users" WHERE id = 1"#.to_string(), 37));
        assert_eq!(filter_query("", "users", true, "id"), (r#"SELECT * FROM "users" ORDER BY id"#.to_string(), 31));
    }

    /// The field's text and caret map into the query: the table's columns first, then the console's rules
    #[test]
    fn filter_field_completes_as_its_query() {
        let m = meta();
        let f = |order: bool, field: &str, caret: usize| {
            let (n, v) = suggest_filter(&m, "public", "users", order, field, caret, "", false, false);
            (n, v.into_iter().map(|i| i.label).collect::<Vec<_>>())
        };
        // Only the filtered table's columns, not other tables' (orders.user_id)
        assert_eq!(f(false, "us", 2), (2, vec!["user_name".into(), "using".into()]));
        // The caret in the middle: only the text before it counts
        assert_eq!(f(false, "em = 1 and id > 2", 2).1[0], "email");
        assert_eq!(f(false, "id = 1 an", 9), (2, vec!["and".into(), "analyze".into()]));
        assert_eq!(f(true, "id de", 5).1, ["desc", "delete"], "after a column: keywords only, statement keywords last");
        assert_eq!(f(true, "users.", 6).1, ["id", "user_name", "email"]);
        // Not while typing a number, after a space or an operator, or in a string literal
        assert!(f(false, "id = 12", 7).1.is_empty());
        assert!(f(false, "id = ", 5).1.is_empty());
        assert!(f(false, "email like 'us", 14).1.is_empty());
        assert!(!f(false, "email = 'a''b' and us", 21).1.is_empty(), "a doubled quote closes nothing");
        // A whole keyword or column: ↩ applies the filter instead of picking a longer word
        assert!(f(false, "email is not null", 17).1.is_empty());
        assert!(f(false, "user_id = 1 or ID", 17).1.is_empty());
        assert_eq!(f(false, "user", 4).1[0], "user_name");
    }

    /// Asked for (⌃Space, ⇥): the table's columns first at an empty word, then the console's rest with keywords; longer words at a whole one
    #[test]
    fn filter_field_lists_when_asked() {
        let m = meta();
        let ask = |field: &str| suggest_filter(&m, "public", "users", false, field, field.len(), "", false, true);
        for field in ["", "id = 1 and "] {
            let (n, v) = ask(field);
            assert_eq!((n, &labels(&v)[..3]), (0, &["id", "user_name", "email"][..]), "{field:?}");
            assert!(v.iter().any(|i| i.kind == ItemKind::Keyword && i.label == "and"), "{field:?}");
        }
        assert_eq!(labels(&ask("user").1)[0], "user_name");
        assert_eq!(labels(&ask("us").1)[0], "user_name");
        // Not inside a string literal, asked or not
        assert!(ask("email like 'us").1.is_empty());
    }

    /// The field belongs to one table: its columns, keywords and functions, never other tables or schemas (typed or asked)
    #[test]
    fn filter_field_lists_no_tables_or_schemas() {
        let m = meta();
        for explicit in [false, true] {
            let f = |field: &str| suggest_filter(&m, "public", "users", false, field, field.len(), "", false, explicit).1;
            assert_eq!(labels(&f("orde")), ["order"], "explicit {explicit}");
            assert!(f("bil").is_empty(), "explicit {explicit}");
            assert!(f("billing.").is_empty(), "explicit {explicit}");
            assert_eq!(labels(&f("co")), ["count", "coalesce", "concat", "concat_ws", "commit"], "explicit {explicit}");
        }
        let all = suggest_filter(&m, "public", "users", false, "", 0, "", false, true).1;
        assert!(all.iter().all(|i| !matches!(i.kind, ItemKind::Table | ItemKind::Schema)));
        // A field is plain text: a function writes its name and `(`; MySQL's functions in a MySQL field
        let f = |field: &str, mysql: bool| suggest_filter(&m, "public", "users", false, field, field.len(), "", mysql, false).1.into_iter().map(|i| i.insert).collect::<Vec<_>>();
        assert_eq!(f("low", false), ["lower("]);
        assert_eq!(f("no", false), ["now()", "not"]);
        assert_eq!(f("ifn", true), ["ifnull("]);
        assert!(f("ifn", false).is_empty());
    }

    #[test]
    fn snippets_lose_their_markers() {
        assert_eq!(snippet_text("select * from $0"), ("select * from ".to_string(), vec![(14, 14)]));
        assert_eq!(snippet_text("update ${1:table} set ${2:col} = 1$0"), ("update table set col = 1".to_string(), vec![(7, 12), (17, 20), (24, 24)]));
        assert_eq!(snippet_text(&format!("({}) $0", snip("a$b}"))), ("(a$b}) ".to_string(), vec![(7, 7)]));
    }

    /// The caret's statement as the front ends pass it: trimmed at its start; right after a `;`, an empty one
    #[test]
    fn at_caret_finds_the_statement_and_caret() {
        let pg = crate::DbKind::Postgres;
        let t = "select 1;\n  select * from users u where u.";
        assert_eq!(at_caret(t, t.len(), pg), ("select * from users u where u.".to_string(), 30));
        assert_eq!(at_caret(t, 3, pg), ("select 1".to_string(), 3));
        assert_eq!(at_caret("select 1;\n", 10, pg), (String::new(), 0));
        assert_eq!(at_caret("select 1   ", 11, pg), ("select 1   ".to_string(), 11), "spaces after the statement stay before the caret");
        assert_eq!(at_caret("", 0, pg), (String::new(), 0));
    }

    #[test]
    fn filter_field_in_mysql_finds_its_table_in_the_db() {
        let m = Meta::new(vec![CompletionTable { schema: "shop".into(), name: "orders".into(), columns: vec![("status".into(), "varchar".into())], ..Default::default() }]);
        assert_eq!(suggest_filter(&m, "shop", "orders", false, "st", 2, "shop", true, false).1[0].label, "status");
    }

    fn hmeta() -> Meta {
        let t = |s: &str, n: &str, cols: &[(&str, &str)]| CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|(c, ty)| (c.to_string(), ty.to_string())).collect(), ..Default::default() };
        Meta::new(vec![t("public", "orders", &[("id", "int8"), ("status", "varchar"), ("data", "jsonb"), ("raw", "bytea")]), t("public", "users", &[("id", "int8"), ("name", "text")])])
    }

    #[test]
    fn hint_places() {
        let m = hmeta();
        let at = |before: &str| hint_at(&m, before, before, "app", false);
        let h = at("select * from orders o where o.status = 'pa").unwrap();
        assert_eq!((h.table.as_str(), h.column.as_str(), h.mode, h.quoted, h.typed.as_str()), ("orders", "status", HintMode::Values, true, "pa"));
        let h = at("select * from orders where status in ('paid', '").unwrap();
        assert_eq!((h.column.as_str(), h.quoted), ("status", true));
        let h = at("select * from orders where data -> 'meta' ->> 'sr").unwrap();
        assert_eq!((h.path.clone(), h.mode, h.typed.as_str()), (vec!["meta".to_string()], HintMode::Keys, "sr"));
        let h = at("select * from orders where data->>'kind' = '").unwrap();
        assert_eq!((h.path.clone(), h.mode), (vec!["kind".to_string()], HintMode::Values));
        assert!(!at("select * from orders where id = ").unwrap().quoted);
        assert!(at("select * from orders where raw = '").is_none(), "bytea");
        assert!(at("select * from orders where data = '").is_none(), "whole json");
        assert!(at("select * from orders where ").is_none());
        assert!(at("select * from orders where status = 'paid' and ").is_none());
        assert!(at("select * from users where status = '").is_none(), "not a column of users");
        let h = hint_at(&m, "select * from orders where data->>'$.meta.", "select * from orders where data->>'$.meta.", "app", true).unwrap();
        assert_eq!((h.path.clone(), h.mode), (vec!["meta".to_string()], HintMode::Keys));
        let h = at("select * from orders where data ->> ").unwrap();
        assert_eq!((h.mode, h.quoted, h.op.as_str()), (HintMode::Keys, false, " ->> "));
        let h = at("select * from orders where data @> '").unwrap();
        assert_eq!((h.mode, h.quoted), (HintMode::Pairs, true));
        assert!(hint_at(&m, "select * from orders where data @> '", "select * from orders where data @> '", "app", true).is_none(), "no @> on MySQL");
    }

    /// A JOIN's ON condition compares columns: no value hints there (the console's columns instead); WHERE keeps them
    #[test]
    fn no_hints_in_join_on() {
        let m = hmeta();
        let at = |head: &str| hint_at(&m, head, head, "app", false);
        assert!(at("select * from orders as o\nleft join users as u on o.id = ").is_none());
        assert!(at("select * from orders o join users u on u.id = o.id and o.status = '").is_none(), "AND stays in the ON");
        assert!(at("select * from orders o join users u on (o.id = ").is_none(), "a parenthesis inside the ON");
        assert!(at("select * from orders o join users u on u.id = o.id\nwhere o.status = '").is_some());
        assert!(at("select * from orders o join users u on u.id = o.id where o.id = 1 or o.status = ").is_some());
        assert!(at("select * from users u where u.id in (select o.id from orders o where o.status = '").is_some());
        assert!(at("select * from orders o where o.id in (select u.id from users u join orders x on x.status = ").is_none(), "the subquery's own ON");
        assert!(at("select * from orders o join users u on u.id = o.id where o.id in (select id from users) and o.status = '").is_some());
        assert!(at("select * from orders o join users u on u.name = 'on (' where o.status = '").is_some(), "parentheses in a string");
    }

    #[test]
    fn hint_inserts() {
        let m = hmeta();
        let hs = |v: &[&str]| v.iter().map(|x| Hint { value: x.to_string(), count: 2 }).collect::<Vec<_>>();
        let h = hint_at(&m, "select * from orders where status = 'p", "select * from orders where status = 'p", "app", false).unwrap();
        let v = hint_items(&h, &hs(&["paid", "pa'y", "ready"]), None, false, TUI);
        assert_eq!(v.iter().map(|i| i.insert.as_str()).collect::<Vec<_>>(), ["paid'", "pa''y'"]);
        let v = hint_items(&h, &hs(&["paid", "pa'y", "ready"]), None, false, Options::GUI);
        assert_eq!(v.iter().map(|i| i.insert.as_str()).collect::<Vec<_>>(), ["paid", "pa''y", "ready"], "Monaco filters and closes quotes");
        assert_eq!(hint_items(&h, &hs(&["paid"]), Some('\''), false, TUI)[0].insert, "paid", "closing quote already there");
        let h = hint_at(&m, "select * from orders where status = ", "select * from orders where status = ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["paid"]), None, false, TUI)[0].insert, "'paid'");
        // A backslash makes it E'…' (sql::literal): with standard_conforming_strings = off, '\'' would end the string early
        assert_eq!(hint_items(&h, &hs(&[r"x\' or 1=1 -- "]), None, false, TUI)[0].insert, r"E'x\\'' or 1=1 -- '", "PostgreSQL: the value stays one literal");
        let my = hint_at(&m, "select * from orders where status = ", "select * from orders where status = ", "app", true).unwrap();
        assert_eq!(hint_items(&my, &hs(&[r"x\' or 1=1 -- "]), None, false, TUI)[0].insert, r"'x\\'' or 1=1 -- '", "MySQL: the value stays one literal");
        let h = hint_at(&m, "select * from orders where id = ", "select * from orders where id = ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["7"]), None, false, TUI)[0].insert, "7");
        // Bare only for a plain number or true/false in a column whose type is numeric · boolean by name
        let bare = |v: &str, ty: &str| hint_insert(v, HintMode::Values, &[], ty, false, false, false);
        assert_eq!(bare("x' or true; delete from realm_users; --", "realm_name"), "'x'' or true; delete from realm_users; --'", "a domain named *real*");
        assert_eq!(bare("7", "realm_name"), "'7'");
        assert_eq!(bare("1; drop table t", "int4"), "'1; drop table t'", "not a number, even in a numeric column");
        assert_eq!([bare("-1.5e3", "numeric(10,2)"), bare("TRUE", "boolean"), bare("1", "tinyint(1)"), bare("2", "interval")], ["-1.5e3", "TRUE", "1", "'2'"]);
        let h = hint_at(&m, "select * from orders where data ->> ", "select * from orders where data ->> ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["kind"]), None, false, TUI)[0].insert, "'kind'");
        let v = hint_items(&h, &hs(&[r#"[{"code": "x"}]"#]), None, true, TUI);
        assert_eq!((v[0].insert.as_str(), v[0].label.as_str()), (r#" @> '[{"code": "x"}]'"#, r#"@> '[{"code": "x"}]'"#));
    }

    #[test]
    fn row_counts_are_grouped() {
        assert_eq!([1, 2, 999, 1000, 1234567].map(rows), ["1 row", "2 rows", "999 rows", "1,000 rows", "1,234,567 rows"]);
    }

    #[test]
    fn mysql_schema_is_db() {
        let all = Meta::new(vec![CompletionTable { schema: "shop".into(), name: "items".into(), columns: vec![], ..Default::default() }]);
        let (_, v) = sg(&all, "select * from it", "select * from it", "shop", true, TUI);
        assert_eq!(v[0].insert, "items");
    }

    #[test]
    fn postgres_db_is_not_a_schema() {
        // Database `app` with schemas `app` and `public`: bare names resolve on search_path ("$user", public), not in `app`
        let t = |schema: &str, name: &str, col: &str| CompletionTable { schema: schema.into(), name: name.into(), columns: vec![(col.into(), "text".into())], ..Default::default() };
        let m = Meta::new(vec![t("app", "orders", "o"), t("app", "users", "a_only"), t("public", "users", "p_only")]);
        let (_, v) = sg(&m, "select * from ord", "select * from ord", "app", false, TUI);
        assert_eq!(v.iter().find(|i| i.label == "orders").map(|i| i.insert.as_str()), Some("app.orders"));
        let text = "select * from users u where u.";
        let (_, v) = sg(&m, text, text, "app", false, TUI);
        assert!(v.iter().any(|i| i.label == "p_only") && !v.iter().any(|i| i.label == "a_only"), "{v:?}");
    }

    #[test]
    fn scope_reads_refs_and_aliases() {
        type Refs<'a> = &'a [(&'a str, Option<&'a str>)];
        let cases: [(&str, Refs); 16] = [
            ("select * from users u join orders o on o.user_id = u.id", &[("users", Some("u")), ("orders", Some("o"))]),
            ("select * from billing.invoices as i where i.", &[("billing.invoices", Some("i"))]),
            (r#"update "public"."users" set email = null"#, &[("public.users", None)]),
            ("insert into orders (id) values (1)", &[("orders", None)]),
            ("select * from 주문 where 상태 = 'a'", &[("주문", None)]),
            ("select * from users where id = 1", &[("users", None)]),
            ("select * from users left join orders on true", &[("users", None), ("orders", None)]),
            ("delete from users u where u.id = 1", &[("users", Some("u"))]),
            ("select * from (select 1) as sub", &[]),
            (r#"select * from "My Table" t"#, &[("My Table", Some("t"))]),
            ("select * from users\n join orders o2 using (id)", &[("users", None), ("orders", Some("o2"))]),
            ("select * from users a, orders b", &[("users", Some("a")), ("orders", Some("b"))]),
            ("select * from orders 주 where 주.status = 'x'", &[("orders", Some("주"))]),
            ("select 'from users x' from orders", &[("orders", None)]),
            ("SELECT * FROM Users AS U JOIN Orders O ON TRUE", &[("Users", Some("U")), ("Orders", Some("O"))]),
            // An unclosed quote runs to the end; emoji and a ref at the very end
            ("select 😀 from \"un closed", &[("un closed", None)]),
        ];
        for (text, want) in cases {
            let want: Vec<(String, Option<String>)> = want.iter().map(|(r, a)| (r.to_string(), a.map(str::to_string))).collect();
            let sc = Scope::new(text, false);
            let got: Vec<(String, Option<String>)> = sc.blocks[0].sources.iter().filter_map(|s| Some((s.table.clone()?, s.alias.clone()))).collect();
            assert_eq!(got, want, "{text}");
        }
    }

    #[test]
    fn find_table_by_ref() {
        let t = |s: &str, n: &str| CompletionTable { schema: s.into(), name: n.into(), columns: vec![], ..Default::default() };
        let m = Meta::new(vec![t("public", "users"), t("public", "orders"), t("billing", "invoices"), t("billing", "users"), t("sales", "주문"), t("public", "My Table")]);
        let cases = [
            ("users", Some("public.users")),
            ("USERS", Some("public.users")),
            ("billing.users", Some("billing.users")),
            ("BILLING.INVOICES", Some("billing.invoices")),
            ("invoices", Some("billing.invoices")),
            ("public.invoices", None),
            ("nope", None),
            ("sales.주문", Some("sales.주문")),
            ("주문", Some("sales.주문")),
            ("My Table", Some("public.My Table")),
            ("a.b.c", None),
        ];
        for (r, want) in cases {
            assert_eq!(find_table(&m, r, "").map(|t| format!("{}.{}", t.schema, t.name)).as_deref(), want, "{r}");
        }
    }

    /// A long text (as if a whole script were passed): 300 statements over tables with 20 distinct columns each, ~6k columns.
    /// Timing only means something in a release build: `cargo test --release -p qurry-shared long_script -- --ignored`
    #[test]
    #[ignore = "timing: run with --release"]
    fn long_script_suggest_is_fast() {
        let tables: Vec<CompletionTable> = (0..2000)
            .map(|i| CompletionTable { schema: format!("s{i}"), name: format!("t{i}"), columns: (0..20).map(|c| (format!("c{i}_{c}"), "text".into())).collect(), ..Default::default() })
            .collect();
        let m = Meta::new(tables);
        let mut text: String = (0..300).map(|i| format!("select * from s{i}.t{i} where id = 1;\n")).collect();
        text.push_str("select ");
        let mut times: Vec<std::time::Duration> = (0..50)
            .map(|_| {
                let t = std::time::Instant::now();
                let (_, v) = suggest(&m, &text, text.len(), "", false, Options::GUI);
                assert_eq!(v.iter().filter(|i| i.kind == ItemKind::Column).count(), 6000);
                t.elapsed()
            })
            .collect();
        times.sort();
        let p95 = times[times.len() * 95 / 100];
        assert!(p95 < std::time::Duration::from_millis(5), "p95 {p95:?}");
    }

    /// The TUI's completion over tests/corpus/suggest.json (`‸` = cursor), as its update_completion builds it, compared with
    /// tests/goldens/completion/tui.json. `UPDATE_GOLDENS=1 cargo test -p qurry-shared complete` rewrites the golden.
    /// The GUI side of the same corpus is src/lib/completion.test.ts (goldens/completion/gui.json)
    #[test]
    fn corpus_matches_tui_golden() {
        use serde_json::{Value, json};
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
        let corpus: Value = serde_json::from_str(&std::fs::read_to_string(root.join("corpus/suggest.json")).unwrap()).unwrap();
        let st = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let tables = corpus["tables"].as_array().unwrap().iter().map(|t| CompletionTable { schema: st(&t["schema"]), name: st(&t["name"]), columns: t["columns"].as_array().unwrap().iter().map(|p| (st(&p[0]), st(&p[1]))).collect(), ..Default::default() }).collect();
        let m = Meta::new(tables);
        let items = |v: Vec<Item>| v.into_iter().map(|i| json!({ "label": i.label, "detail": i.detail, "insert": i.insert, "kind": format!("{:?}", i.kind) })).collect::<Vec<_>>();
        let mut out = serde_json::Map::new();
        for c in corpus["cases"].as_array().unwrap() {
            let raw = st(&c["text"]);
            let at = raw.find('‸').unwrap();
            let text = raw.replacen('‸', "", 1);
            let after = text[at..].chars().next();
            let (db, mysql) = (st(&c["db"]), c["mysql"].as_bool().unwrap_or(false));
            // As update_completion: the caret's statement
            let kind = if mysql { crate::DbKind::Mysql } else { crate::DbKind::Postgres };
            let head = crate::sql::statement_head(&text, at, kind).to_string();
            let text = crate::sql::statement_at(&text, at, kind).unwrap_or_default();
            let hints = |mode: &str| c["hints"].get(mode).map(|v| v.as_array().unwrap().iter().map(|p| Hint { value: st(&p[0]), count: p[1].as_i64().unwrap() }).collect::<Vec<_>>());
            let o = if let Some(h) = hint_at(&m, &text, &head, &db, mysql) {
                let name = |m: HintMode| format!("{m:?}").to_lowercase();
                let mut asked = vec![name(h.mode)];
                let mut got = (h.mode, hints(&name(h.mode)).unwrap_or_default());
                if h.mode == HintMode::Keys && got.1.is_empty() && !mysql {
                    asked.push("pairs".into());
                    got = (HintMode::Pairs, hints("pairs").unwrap_or_default());
                }
                let as_pairs = got.0 == HintMode::Pairs && h.mode == HintMode::Keys;
                let prefix = if as_pairs { h.op.chars().count() } else { h.typed.chars().count() };
                let hint = json!({ "schema": h.schema, "table": h.table, "column": h.column, "path": h.path, "asked": asked });
                json!({ "hint": hint, "prefix": prefix, "items": items(hint_items(&h, &got.1, after, as_pairs, TUI)) })
            } else {
                let (stmt, caret) = at_caret(&raw.replacen('‸', "", 1), at, kind);
                let (prefix, v) = suggest(&m, &stmt, caret, &db, mysql, TUI);
                json!({ "prefix": prefix, "items": items(v) })
            };
            out.insert(st(&c["id"]), o);
        }
        // One item per line, so a diff of the golden shows which ones changed
        let lines: Vec<String> = out
            .iter()
            .map(|(id, o)| {
                let its: Vec<String> = o["items"].as_array().unwrap().iter().map(|i| format!("\n      {i}")).collect();
                let hint = o.get("hint").map(|h| format!("\n    \"hint\": {h},")).unwrap_or_default();
                format!("  {}: {{{hint}\n    \"prefix\": {},\n    \"items\": [{}{}]\n  }}", json!(id), o["prefix"], its.join(","), if its.is_empty() { "" } else { "\n    " })
            })
            .collect();
        let got = format!("{{\n{}\n}}\n", lines.join(",\n"));
        let golden = root.join("goldens/completion/tui.json");
        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            std::fs::write(&golden, &got).unwrap();
        }
        let want = std::fs::read_to_string(&golden).expect("missing golden: run with UPDATE_GOLDENS=1");
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap(), serde_json::from_str::<Value>(&want).unwrap());
    }

    /// tests/corpus/assist.json (`‸` = the caret) as both front ends get it, compared with tests/goldens/completion/assist.json:
    /// suggest → the TUI's whole list and the GUI's first candidates; signature → the call's signatures and parameter; marks → the
    /// unknown names of the script. `UPDATE_GOLDENS=1 cargo test -p qurry-shared assist` rewrites the golden
    #[test]
    fn assist_corpus_matches_golden() {
        use serde_json::{Value, json};
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
        let corpus: Value = serde_json::from_str(&std::fs::read_to_string(root.join("corpus/assist.json")).unwrap()).unwrap();
        let st = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let m = Meta::new(serde_json::from_value(corpus["tables"].clone()).unwrap());
        let item = |i: &Item| {
            let insert = if i.insert == i.label { String::new() } else { format!(" => {}", i.insert) };
            let detail = if i.detail.is_empty() { String::new() } else { format!(" · {}", i.detail) };
            format!("{:?} {}{insert}{detail}", i.kind, i.label)
        };
        let mut out = serde_json::Map::new();
        for c in corpus["cases"].as_array().unwrap() {
            let raw = st(&c["text"]);
            let at = raw.find('‸').unwrap_or(raw.len());
            let text = raw.replacen('‸', "", 1);
            let (db, mysql) = (st(&c["db"]), c["mysql"].as_bool().unwrap_or(false));
            let kind = if mysql { crate::DbKind::Mysql } else { crate::DbKind::Postgres };
            let (stmt, caret) = at_caret(&text, at, kind);
            let o = match c["what"].as_str().unwrap_or("suggest") {
                "signature" => json!(signature(&m, &stmt, caret, mysql)),
                "marks" => json!(script_marks(&m, &text, kind, &db).iter().map(|mk| json!({ "at": &text[mk.start..mk.end], "message": mk.message, "fixes": mk.fixes })).collect::<Vec<_>>()),
                _ => {
                    let (n, tui) = suggest(&m, &stmt, caret, &db, mysql, TUI);
                    let (gn, gui) = suggest(&m, &stmt, caret, &db, mysql, Options::GUI);
                    json!({ "tui": { "prefix": n, "items": tui.iter().map(item).collect::<Vec<_>>() }, "gui": { "prefix": gn, "first": gui.iter().take(8).map(item).collect::<Vec<_>>() } })
                }
            };
            out.insert(st(&c["id"]), o);
        }
        let got = serde_json::to_string_pretty(&Value::Object(out)).unwrap() + "\n";
        let golden = root.join("goldens/completion/assist.json");
        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            std::fs::write(&golden, &got).unwrap();
        }
        let want = std::fs::read_to_string(&golden).expect("missing golden: run with UPDATE_GOLDENS=1");
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap(), serde_json::from_str::<Value>(&want).unwrap());
    }
}
