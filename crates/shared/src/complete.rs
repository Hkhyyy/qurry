//! Console autocompletion, shared by the TUI and the GUI (through WASM: CompletionIndex in qurry-shared-wasm, used by src/lib/completion.ts)
//! - `alias.` / `table.` → that table's columns, `schema.` → that schema's tables
//! - otherwise: columns of tables in the statement → tables → schemas → keywords
//! - value spots (`status = '`, `data->>'` …) → values · json keys read from the DB (hint_at, then hint_items with what the DB returned)
//!
//! Where the two front ends complete differently, they say so with [`Options`] (Options::GUI, Options::TUI).
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A table (or view) with its columns (name, type), as the backend reads them for autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<(String, String)>,
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
}

impl Options {
    /// Monaco (src/lib/completion.ts)
    pub const GUI: Options = Options { editor_filters: true, keywords_on_empty: true, quiet_in_value_quotes: true };
    /// The terminal console's own popup
    pub const TUI: Options = Options { editor_filters: false, keywords_on_empty: false, quiet_in_value_quotes: false };
}

/// Autocomplete metadata: lowercase names and a per-name index are built once on load (not rebuilt on every key)
pub struct Meta {
    pub tables: Vec<CompletionTable>,
    /// (schema, table) lowercase
    lower: Vec<(String, String)>,
    /// Lowercase table name → positions in tables
    by_name: HashMap<String, Vec<usize>>,
}

impl Meta {
    pub fn new(tables: Vec<CompletionTable>) -> Self {
        let lower: Vec<(String, String)> = tables.iter().map(|t| (t.schema.to_lowercase(), t.name.to_lowercase())).collect();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, (_, n)) in lower.iter().enumerate() {
            by_name.entry(n.clone()).or_default().push(i);
        }
        Meta { tables, lower, by_name }
    }
}

const KEYWORDS: &str = "select from where and or not in is null like ilike between exists group by order having limit offset join left right inner outer full cross on using as \
distinct union all intersect except insert into values update set delete returning create table view index alter drop truncate with case when then else end \
asc desc nulls first last count sum avg min max coalesce now current_date current_timestamp interval true false begin commit rollback explain analyze";

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Column,
    Table,
    Schema,
    Keyword,
    /// Column values · json keys (read from the DB)
    Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub insert: String,
    pub kind: ItemKind,
}

fn keywords() -> impl Iterator<Item = &'static str> {
    KEYWORDS.split_whitespace()
}

/// Identifier characters: letters and digits of any script (Korean …) and `_`
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Find `from|join|update|into <ref> [as] [alias]`
pub fn tables_in_statement(text: &str) -> Vec<(String, Option<String>)> {
    let toks: Vec<String> = tokenize(text);
    let mut out = vec![];
    for (i, t) in toks.iter().enumerate() {
        if !["from", "join", "update", "into"].contains(&t.to_lowercase().as_str()) {
            continue;
        }
        let Some(r) = toks.get(i + 1).filter(|r| r.chars().next().is_some_and(|c| is_word(c) || c == '"')) else { continue };
        let mut j = i + 2;
        if toks.get(j).is_some_and(|t| t.eq_ignore_ascii_case("as")) {
            j += 1;
        }
        let alias = toks.get(j).filter(|a| a.chars().all(is_word) && !keywords().any(|k| k.eq_ignore_ascii_case(a))).cloned();
        out.push((r.replace('"', ""), alias));
    }
    out
}

/// Split into words (including `schema.table`, `"quoted"`) and symbols
fn tokenize(text: &str) -> Vec<String> {
    let cs: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if is_word(c) || c == '"' {
            let start = i;
            loop {
                if i < cs.len() && cs[i] == '"' {
                    i += 1;
                    while i < cs.len() && cs[i] != '"' {
                        i += 1;
                    }
                    i = (i + 1).min(cs.len());
                } else {
                    while i < cs.len() && is_word(cs[i]) {
                        i += 1;
                    }
                }
                // schema.table is one token
                if i + 1 < cs.len() && cs[i] == '.' && (is_word(cs[i + 1]) || cs[i + 1] == '"') {
                    i += 1;
                    continue;
                }
                break;
            }
            out.push(cs[start..i].iter().collect());
        } else {
            if !c.is_whitespace() {
                out.push(c.to_string());
            }
            i += 1;
        }
    }
    out
}

/// The table a reference names: `schema.table`, or a bare name (in `public` or the tab's DB first, then any schema)
pub fn find_table<'a>(m: &'a Meta, r: &str, db: &str) -> Option<&'a CompletionTable> {
    let r = r.to_lowercase();
    let (schema, name) = match r.split_once('.') {
        Some((s, n)) => (Some(s), n),
        None => (None, r.as_str()),
    };
    let hits = m.by_name.get(name)?;
    let t = |i: &usize| &m.tables[*i];
    match schema {
        Some(s) => hits.iter().find(|&&i| m.lower[i].0 == s).map(t),
        None => hits.iter().find(|&&i| m.tables[i].schema == "public" || m.tables[i].schema == db).or(hits.first()).map(t),
    }
}

/// Build candidates from the text before the cursor. Returns: (length of the prefix to replace in chars, candidates)
/// `text` is the statement (or the whole script), `before` is the part of the cursor's line before the cursor, `db` is the tab's DB
/// (in MySQL, schema = DB; "" for none), `mysql` reads MySQL json paths in value spots
pub fn suggest(m: &Meta, text: &str, before: &str, db: &str, mysql: bool, o: Options) -> (usize, Vec<Item>) {
    let all = &m.tables;
    let prefix: String = before.chars().rev().take_while(|c| is_word(*c)).collect::<Vec<_>>().into_iter().rev().collect();
    let head = &before[..before.len() - prefix.len()];
    let p = prefix.to_lowercase();
    let n = prefix.chars().count();
    if o.quiet_in_value_quotes && context(before, mysql).is_some_and(|c| c.quoted) {
        return (n, vec![]);
    }
    // Drop candidates identical to the fully typed word so Enter goes to a newline
    let starts = |lower: &str| o.editor_filters || p.is_empty() || (lower.starts_with(&p) && lower != p);
    let matches = |label: &str| starts(&label.to_lowercase());
    let sorted = |mut v: Vec<Item>| {
        if o.editor_filters {
            v.sort_by(|a, b| a.label.cmp(&b.label));
        }
        v
    };

    if let Some(h) = head.strip_suffix('.') {
        let q: String = h.chars().rev().take_while(|c| is_word(*c) || *c == '"').collect::<Vec<_>>().into_iter().rev().collect();
        let q = q.replace('"', "").to_lowercase();
        if q.is_empty() {
            return (n, vec![]);
        }
        // alias. or table. → columns
        let refs = tables_in_statement(text);
        let hit = refs
            .iter()
            .find(|(_, a)| a.as_ref().is_some_and(|a| a.to_lowercase() == q))
            .or_else(|| refs.iter().find(|(r, _)| r.rsplit('.').next().unwrap_or(r).to_lowercase() == q));
        let t = hit.and_then(|(r, _)| find_table(m, r, db)).or_else(|| find_table(m, &q, db));
        if let Some(t) = t {
            let items = t.columns.iter().filter(|(c, _)| matches(c)).map(|(c, ty)| Item { label: c.clone(), detail: ty.clone(), insert: c.clone(), kind: ItemKind::Column }).collect();
            return (n, items);
        }
        // schema. → tables
        let items = all
            .iter()
            .zip(&m.lower)
            .filter(|(_, (ls, ln))| *ls == q && starts(ln))
            .map(|(t, _)| Item { label: t.name.clone(), detail: format!("{} cols", t.columns.len()), insert: t.name.clone(), kind: ItemKind::Table })
            .collect();
        return (n, sorted(items));
    }

    let mut columns: Vec<Item> = vec![];
    for (r, _) in tables_in_statement(text) {
        if let Some(t) = find_table(m, &r, db) {
            for (c, ty) in &t.columns {
                if matches(c) && !columns.iter().any(|i| &i.label == c) {
                    columns.push(Item { label: c.clone(), detail: format!("{ty} · {}", t.name), insert: c.clone(), kind: ItemKind::Column });
                }
            }
        }
    }
    let mut items = sorted(columns);
    let mut tables: Vec<Item> = all
        .iter()
        .zip(&m.lower)
        .filter(|(_, (_, ln))| starts(ln))
        .map(|(t, _)| Item {
            label: t.name.clone(),
            detail: t.schema.clone(),
            insert: if t.schema == "public" || t.schema == db { t.name.clone() } else { format!("{}.{}", t.schema, t.name) },
            kind: ItemKind::Table,
        })
        .collect();
    tables.sort_by(|a, b| a.label.cmp(&b.label));
    items.extend(tables);
    let mut schemas: Vec<&str> = all.iter().map(|t| t.schema.as_str()).filter(|s| matches(s)).collect();
    schemas.sort();
    schemas.dedup();
    items.extend(schemas.into_iter().map(|s| Item { label: s.into(), detail: "schema".into(), insert: s.into(), kind: ItemKind::Schema }));
    if !p.is_empty() || o.keywords_on_empty {
        let kws = keywords().filter(|k| matches(k)).map(|k| Item { label: k.into(), detail: "keyword".into(), insert: k.into(), kind: ItemKind::Keyword }).collect();
        items.extend(sorted(kws));
    }
    (n, items)
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
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ctx {
    /// Alias or table (if absent, the column is looked up among the statement's tables)
    pub r#ref: Option<String>,
    pub column: String,
    pub path: Vec<String>,
    pub mode: HintMode,
    pub quoted: bool,
    pub typed: String,
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

/// If the cursor is at a value · json key spot, that table · column (only for columns worth suggesting)
pub fn hint_at(m: &Meta, text: &str, before: &str, db: &str, mysql: bool) -> Option<HintAt> {
    let ctx = context(before, mysql)?;
    let refs = tables_in_statement(text);
    let named: Vec<&(String, Option<String>)> = match &ctx.r#ref {
        Some(q) => {
            let q = q.to_lowercase();
            refs.iter()
                .find(|(_, a)| a.as_ref().is_some_and(|a| a.to_lowercase() == q))
                .or_else(|| refs.iter().find(|(r, _)| r.rsplit('.').next().unwrap_or(r).to_lowercase() == q))
                .into_iter()
                .collect()
        }
        None => refs.iter().collect(),
    };
    let lc = ctx.column.to_lowercase();
    let (t, (col, ty)) = named.iter().filter_map(|(r, _)| find_table(m, r, db)).find_map(|t| t.columns.iter().find(|(c, _)| c.to_lowercase() == lc).map(|c| (t, c)))?;
    hintable(ctx.mode, &ctx.path, ty).then(|| HintAt { schema: t.schema.clone(), table: t.name.clone(), column: col.clone(), type_name: ty.clone(), path: ctx.path, mode: ctx.mode, quoted: ctx.quoted, typed: ctx.typed, op: ctx.op })
}

/// Text to insert for a value: inside quotes only doubled ' (and the closing quote when `close`); outside, numeric · boolean
/// column values as they are, the rest '…'
pub fn hint_insert(value: &str, mode: HintMode, path: &[String], type_name: &str, quoted: bool, close: bool) -> String {
    let esc = value.replace('\'', "''");
    let bare = mode == HintMode::Values && path.is_empty() && ["int", "numeric", "decimal", "float", "double", "real", "serial", "bool", "bit", "money"].iter().any(|k| type_name.to_lowercase().contains(k));
    if quoted {
        if close { format!("{esc}'") } else { esc }
    } else if bare {
        value.to_string()
    } else {
        format!("'{esc}'")
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

/// Fetched values → candidates (filtered by typed text unless the editor filters). close = whether to close the quote (when no ' follows the cursor).
/// as_pairs = a keys spot with no keys (array of objects), so @> conditions were fetched → replace including the operator (prefix = op length)
pub fn hint_items(h: &HintAt, hints: &[Hint], close: bool, as_pairs: bool, o: Options) -> Vec<Item> {
    let p = h.typed.to_lowercase();
    hints
        .iter()
        .filter(|x| as_pairs || o.editor_filters || p.is_empty() || (x.value.to_lowercase().starts_with(&p) && x.value.to_lowercase() != p))
        .map(|x| {
            if as_pairs {
                return Item { label: format!("@> '{}'", x.value), detail: rows(x.count), insert: pairs_insert(&x.value, &h.op), kind: ItemKind::Value };
            }
            let insert = hint_insert(&x.value, h.mode, &h.path, &h.type_name, h.quoted, close);
            Item { label: x.value.clone(), detail: format!("{}{}", rows(x.count), if h.mode == HintMode::Keys { " · key" } else { "" }), insert, kind: ItemKind::Value }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUI: Options = Options::TUI;

    fn meta() -> Meta {
        let t = |s: &str, n: &str, cols: &[&str]| CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|c| (c.to_string(), "text".to_string())).collect() };
        Meta::new(vec![t("public", "users", &["id", "user_name", "email"]), t("public", "orders", &["id", "user_id", "total"]), t("billing", "invoices", &["id", "amount"])])
    }

    fn labels(v: &[Item]) -> Vec<&str> {
        v.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn alias_dot_gives_columns() {
        let text = "select u. from users u join orders o on o.user_id = u.id";
        let (n, v) = suggest(&meta(), text, "select u.", "app", false, TUI);
        assert_eq!(n, 0);
        assert_eq!(labels(&v), ["id", "user_name", "email"]);
        let (n, v) = suggest(&meta(), text, "select u.us", "app", false, TUI);
        assert_eq!(n, 2);
        assert_eq!(labels(&v), ["user_name"]);
    }

    #[test]
    fn table_dot_and_schema_dot() {
        let (_, v) = suggest(&meta(), "select orders. from orders", "select orders.", "app", false, TUI);
        assert_eq!(labels(&v), ["id", "user_id", "total"]);
        let (_, v) = suggest(&meta(), "select * from billing.", "select * from billing.", "app", false, TUI);
        assert_eq!(labels(&v), ["invoices"]);
    }

    #[test]
    fn plain_prefix_orders_columns_tables_schemas_keywords() {
        let (n, v) = suggest(&meta(), "select * from users where u", "select * from users where u", "app", false, TUI);
        assert_eq!(n, 1);
        assert_eq!(labels(&v), ["user_name", "users", "using", "union", "update"]);
        let (_, v) = suggest(&meta(), "select * from inv", "select * from inv", "app", false, TUI);
        assert_eq!(v[0].insert, "billing.invoices");
    }

    #[test]
    fn typed_word_is_not_suggested_again() {
        let (_, v) = suggest(&meta(), "select * from users", "select * from users", "app", false, TUI);
        assert!(v.is_empty(), "{:?}", labels(&v));
        let (_, v) = suggest(&meta(), "sel", "sel", "app", false, TUI);
        assert_eq!(labels(&v), ["select"]);
    }

    /// The GUI leaves filtering to Monaco: everything, sorted by label in each group, keywords on an empty prefix too
    #[test]
    fn gui_returns_everything_for_the_editor_to_filter() {
        let text = "select * from users where u";
        let (n, v) = suggest(&meta(), text, text, "", false, Options::GUI);
        assert_eq!(n, 1);
        assert_eq!(&labels(&v)[..7], ["email", "id", "user_name", "invoices", "orders", "users", "billing"]);
        assert_eq!(v.iter().filter(|i| i.kind == ItemKind::Keyword).count(), keywords().count());
        let kws: Vec<&str> = v.iter().filter(|i| i.kind == ItemKind::Keyword).map(|i| i.label.as_str()).collect();
        assert!(kws.is_sorted());
        let (_, v) = suggest(&meta(), "select * from ", "select * from ", "", false, Options::GUI);
        assert!(v.iter().any(|i| i.kind == ItemKind::Keyword));
        let (_, v) = suggest(&meta(), "select u.us from users u", "select u.us", "", false, Options::GUI);
        assert_eq!(labels(&v), ["id", "user_name", "email"], "columns after alias. keep table order");
    }

    #[test]
    fn gui_is_quiet_inside_value_quotes() {
        let before = "select * from users where email = 'us";
        assert!(suggest(&meta(), before, before, "", false, Options::GUI).1.is_empty());
        assert!(!suggest(&meta(), before, before, "", false, TUI).1.is_empty());
        // Not a value spot: a string anywhere else still completes
        assert!(!suggest(&meta(), "select 'us", "select 'us", "", false, Options::GUI).1.is_empty());
    }

    fn hmeta() -> Meta {
        let t = |s: &str, n: &str, cols: &[(&str, &str)]| CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|(c, ty)| (c.to_string(), ty.to_string())).collect() };
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

    #[test]
    fn hint_inserts() {
        let m = hmeta();
        let hs = |v: &[&str]| v.iter().map(|x| Hint { value: x.to_string(), count: 2 }).collect::<Vec<_>>();
        let h = hint_at(&m, "select * from orders where status = 'p", "select * from orders where status = 'p", "app", false).unwrap();
        let v = hint_items(&h, &hs(&["paid", "pa'y", "ready"]), true, false, TUI);
        assert_eq!(v.iter().map(|i| i.insert.as_str()).collect::<Vec<_>>(), ["paid'", "pa''y'"]);
        assert_eq!(hint_items(&h, &hs(&["paid", "pa'y", "ready"]), false, false, Options::GUI).len(), 3, "Monaco filters");
        assert_eq!(hint_items(&h, &hs(&["paid"]), false, false, TUI)[0].insert, "paid", "closing quote already there");
        let h = hint_at(&m, "select * from orders where status = ", "select * from orders where status = ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["paid"]), false, false, TUI)[0].insert, "'paid'");
        let h = hint_at(&m, "select * from orders where id = ", "select * from orders where id = ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["7"]), false, false, TUI)[0].insert, "7");
        let h = hint_at(&m, "select * from orders where data ->> ", "select * from orders where data ->> ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["kind"]), false, false, TUI)[0].insert, "'kind'");
        let v = hint_items(&h, &hs(&[r#"[{"code": "x"}]"#]), false, true, TUI);
        assert_eq!((v[0].insert.as_str(), v[0].label.as_str()), (r#" @> '[{"code": "x"}]'"#, r#"@> '[{"code": "x"}]'"#));
    }

    #[test]
    fn row_counts_are_grouped() {
        assert_eq!([1, 2, 999, 1000, 1234567].map(rows), ["1 row", "2 rows", "999 rows", "1,000 rows", "1,234,567 rows"]);
    }

    #[test]
    fn mysql_schema_is_db() {
        let all = Meta::new(vec![CompletionTable { schema: "shop".into(), name: "items".into(), columns: vec![] }]);
        let (_, v) = suggest(&all, "select * from it", "select * from it", "shop", true, TUI);
        assert_eq!(v[0].insert, "items");
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
        let tables = corpus["tables"].as_array().unwrap().iter().map(|t| CompletionTable { schema: st(&t["schema"]), name: st(&t["name"]), columns: t["columns"].as_array().unwrap().iter().map(|p| (st(&p[0]), st(&p[1]))).collect() }).collect();
        let m = Meta::new(tables);
        let items = |v: Vec<Item>| v.into_iter().map(|i| json!({ "label": i.label, "detail": i.detail, "insert": i.insert, "kind": format!("{:?}", i.kind) })).collect::<Vec<_>>();
        let mut out = serde_json::Map::new();
        for c in corpus["cases"].as_array().unwrap() {
            let raw = st(&c["text"]);
            let at = raw.find('‸').unwrap();
            let text = raw.replacen('‸', "", 1);
            let before = &text[text[..at].rfind('\n').map_or(0, |i| i + 1)..at];
            let after = text[at..].chars().next();
            let (db, mysql) = (st(&c["db"]), c["mysql"].as_bool().unwrap_or(false));
            let hints = |mode: &str| c["hints"].get(mode).map(|v| v.as_array().unwrap().iter().map(|p| Hint { value: st(&p[0]), count: p[1].as_i64().unwrap() }).collect::<Vec<_>>());
            let o = if let Some(h) = hint_at(&m, &text, before, &db, mysql) {
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
                json!({ "hint": hint, "prefix": prefix, "items": items(hint_items(&h, &got.1, h.quoted && after != Some('\''), as_pairs, TUI)) })
            } else {
                let (prefix, v) = suggest(&m, &text, before, &db, mysql, TUI);
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
}
