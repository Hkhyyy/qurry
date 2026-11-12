//! Whether a console result can be edited the way a table tab is, and where each edit goes. A plain SELECT from one table, with
//! the table's whole primary key in the result, edits like a table tab (rows can be added and deleted too). A join edits per
//! table (#102): a column of a base table whose whole primary key is in the result writes to that table's row, found by its key;
//! every other column is read-only with the reason. The server says where each column comes from (qurry_core::editable); the
//! statement check and the rules live here, so the reasons read the same in the GUI and the TUI.
//!
//! The statement check matters even where the server reports column origins: a self-join (`t a JOIN t b`) gives columns of one
//! table but of two different rows, and only the select list says which occurrence of the table a column is.
use crate::errors as e;
use crate::guard::{Tok, scan};
use crate::kind::DbKind;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Where an editable console result writes. Result column i is the table's column `columns[i]`; `pk` is the table's primary key
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTarget {
    pub schema: String,
    pub table: String,
    pub pk: Vec<String>,
    pub columns: Vec<String>,
}

/// A join result's edits (#102): per result column, the table occurrence it writes to or why it can't be edited. Rows can't be
/// added or deleted (a result row isn't a row of one table)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinEdit {
    /// The table occurrences whose rows can be edited, each found by its own primary key
    pub tables: Vec<JoinTable>,
    /// One per result column
    pub columns: Vec<ColumnEdit>,
}

/// A table occurrence of a join that can be edited: `pk_at[j]` is the result column holding its key column `pk[j]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinTable {
    pub schema: String,
    pub table: String,
    pub pk: Vec<String>,
    pub pk_at: Vec<usize>,
}

/// Where an edit of one result column goes: `table` (an index into JoinEdit::tables) and its column there, or why it can't be edited
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum ColumnEdit {
    To { table: usize, column: String },
    ReadOnly {
        #[serde(rename = "readOnly")]
        read_only: String,
    },
}

impl JoinEdit {
    /// Where an edit of result column `c` goes in a row, or why it can't be edited there. null(i): result column i is NULL in
    /// that row. A key is never NULL, so a NULL in the table's key means no row of it matched (the outer side of a join)
    pub fn cell(&self, c: usize, null: impl Fn(usize) -> bool) -> Result<(&JoinTable, &str), String> {
        match self.columns.get(c) {
            Some(ColumnEdit::To { table, column }) => {
                let t = &self.tables[*table];
                match t.pk_at.iter().any(|&i| null(i)) {
                    true => Err(e::edit_no_row(&format!("{}.{}", t.schema, t.table))),
                    false => Ok((t, column)),
                }
            }
            Some(ColumnEdit::ReadOnly { read_only }) => Err(read_only.clone()),
            None => Err(e::EDIT_UNMATCHED.into()),
        }
    }
}

/// Flattened into the result: `edit` (one table), `editJoin` (a join) or `readOnlyReason`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Editable {
    #[serde(rename = "edit")]
    Table(EditTarget),
    #[serde(rename = "editJoin")]
    Join(JoinEdit),
    #[serde(rename = "readOnlyReason")]
    ReadOnly(String),
}

/// A select-list item as written. `of` = the table or alias it is qualified with (`u.name`, `u.*`), as the server reads it
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `*` or `t.*`
    All { of: Option<String> },
    /// A bare or qualified column name (quotes stripped), and its alias
    Column { of: Option<String>, name: String, alias: Option<String> },
    /// Anything else: an expression, a literal, a function call
    Other,
}

/// A table in FROM: `[schema.]table [[AS] alias]`
#[derive(Debug, Clone, PartialEq)]
pub struct FromTable {
    pub schema: Option<String>,
    pub table: String,
    pub alias: Option<String>,
}

impl FromTable {
    /// What the select list qualifies its columns with
    fn key(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.table)
    }
}

/// A plain SELECT from one table or a join of tables
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub items: Vec<Item>,
    /// The tables in FROM, in order (one for a plain SELECT from one table)
    pub from: Vec<FromTable>,
    /// A join with USING or NATURAL: its join columns are merged, so `*` doesn't list each table's columns in turn
    pub using: bool,
}

/// What follows FROM
const AFTER_FROM: &[&str] = &["where", "order", "limit", "offset", "fetch", "for", "lock"];
/// The words a join starts with (a function such as left(…) is followed by a parenthesis)
const JOIN_WORDS: &[&str] = &["natural", "inner", "cross", "left", "right", "full", "join", "straight_join"];
/// Words that end a FROM item, so they aren't its alias
const NOT_ALIAS: &[&str] = &[
    "where", "order", "limit", "offset", "fetch", "for", "lock", "join", "inner", "left", "right", "full", "cross", "natural", "straight_join", "group", "having", "union",
    "intersect", "except", "window", "tablesample", "partition", "use", "force", "ignore", "on", "using",
];
/// Bare words that are values, not columns (MySQL reads `current_date` as the function even when a column has that name)
const VALUE_WORDS: &[&str] = &[
    "null", "true", "false", "unknown", "default", "current_date", "current_time", "current_timestamp", "current_user", "current_role", "localtime", "localtimestamp", "utc_date",
    "utc_time", "utc_timestamp",
];

/// The tables and select list of a plain `SELECT … FROM t [alias] {[NATURAL] [INNER | CROSS | LEFT | RIGHT | FULL [OUTER]] JOIN
/// u [alias] [ON … | USING (…)] | , u} [WHERE …] [ORDER BY …] [LIMIT …]`, or why the statement isn't one (errors::EDIT_*).
/// Subqueries inside ON and WHERE are fine: they don't add result columns
pub fn select_shape(sql: &str, kind: DbKind) -> Result<Shape, String> {
    let sc = scan(sql, kind);
    if sc.exec_comment {
        return Err(e::EDIT_NOT_PLAIN.into());
    }
    let end = sc.toks.iter().position(|x| x.text == ";" && x.depth == 0).unwrap_or(sc.toks.len());
    if end + 1 < sc.toks.len() {
        return Err(e::EDIT_NOT_PLAIN.into());
    }
    let t = &sc.toks[..end];
    let kw = |i: usize, w: &str| t.get(i).is_some_and(|x| x.word && !x.quoted && x.depth == 0 && x.text == w);
    if !kw(0, "select") {
        return Err(e::EDIT_NOT_PLAIN.into());
    }
    for (i, x) in t.iter().enumerate() {
        if !(x.word && !x.quoted && x.depth == 0) {
            continue;
        }
        match x.text.as_str() {
            "union" | "intersect" | "except" => return Err(e::edit_combined(&x.text.to_uppercase())),
            "group" => return Err(e::edit_combined("GROUP BY")),
            "having" => return Err(e::edit_combined("HAVING")),
            "into" | "lateral" => return Err(e::EDIT_NOT_PLAIN.into()),
            // FOR UPDATE / SHARE / NO KEY UPDATE / KEY SHARE, MySQL LOCK IN SHARE MODE: in Auto mode the cursor's transaction keeps
            // the row locks while the result is open, and the edits, saved on another connection, would wait on them
            "for" | "lock" => return Err(e::EDIT_LOCKS.into()),
            "distinct" | "distinctrow" if i == 1 => return Err(e::edit_combined("DISTINCT")),
            _ => {}
        }
    }
    let from_at = (1..t.len()).find(|&i| kw(i, "from")).ok_or_else(|| e::EDIT_NO_TABLE.to_string())?;
    // A word that starts a join here (not a name after a dot, not a function such as left(…))
    let joins_at = |i: usize| {
        t.get(i).is_some_and(|x| x.word && !x.quoted && x.depth == 0 && JOIN_WORDS.contains(&x.text.as_str()))
            && t.get(i + 1).is_none_or(|n| n.text != "(")
            && !(i > 0 && t[i - 1].text == ".")
    };
    let mut from = Vec::new();
    let mut using = false;
    let mut i = from_at + 1;
    loop {
        // The table: name or schema.name, not a subquery, a function, parentheses or ONLY
        let mut parts = Vec::new();
        loop {
            let Some(x) = t.get(i).filter(|x| x.word && x.depth == 0 && !kw(i, "only")) else { return Err(e::EDIT_NOT_PLAIN.into()) };
            parts.push(name(x, sql, kind));
            i += 1;
            if t.get(i).is_some_and(|x| x.text == ".") {
                i += 1;
            } else {
                break;
            }
        }
        let (schema, table) = match parts.len() {
            1 => (None, parts.remove(0)),
            2 => (Some(parts.remove(0)), parts.remove(0)),
            _ => return Err(e::EDIT_NOT_PLAIN.into()),
        };
        if t.get(i).is_some_and(|x| x.text == "(") {
            return Err(e::EDIT_NOT_PLAIN.into());
        }
        // Its alias
        let alias_at = if kw(i, "as") { i + 1 } else { i };
        let alias = match t.get(alias_at) {
            Some(x) if x.word && x.depth == 0 && (x.quoted || !NOT_ALIAS.contains(&x.text.as_str())) => {
                i = alias_at + 1;
                Some(name(x, sql, kind))
            }
            _ if alias_at != i => return Err(e::EDIT_NOT_PLAIN.into()),
            _ => None,
        };
        from.push(FromTable { schema, table, alias });
        // ON … / USING (…): read past it to the next join or what follows FROM
        if kw(i, "using") {
            using = true;
            i += 1;
            if t.get(i).is_none_or(|x| x.text != "(") {
                return Err(e::EDIT_NOT_PLAIN.into());
            }
            i += 1;
            while t.get(i).is_some_and(|x| x.depth > 0) {
                i += 1;
            }
        } else if kw(i, "on") {
            i += 1;
            while i < t.len() && !(t[i].depth == 0 && (t[i].text == "," || joins_at(i) || AFTER_FROM.iter().any(|w| kw(i, w)))) {
                i += 1;
            }
        }
        match t.get(i) {
            None => break,
            Some(_) if AFTER_FROM.iter().any(|w| kw(i, w)) => break,
            Some(x) if x.text == "," && x.depth == 0 => i += 1,
            Some(_) if joins_at(i) => {
                // [NATURAL] [INNER | CROSS | LEFT | RIGHT | FULL] [OUTER] JOIN, STRAIGHT_JOIN
                using |= kw(i, "natural");
                while !(kw(i, "join") || kw(i, "straight_join")) {
                    if !(joins_at(i) || kw(i, "outer")) {
                        return Err(e::EDIT_NOT_PLAIN.into());
                    }
                    i += 1;
                }
                i += 1;
            }
            Some(_) => return Err(e::EDIT_NOT_PLAIN.into()),
        }
    }
    let items = t[1..from_at].split(|x| x.text == "," && x.depth == 0).map(|it| item(it, sql, kind)).collect();
    Ok(Shape { items, from, using })
}

/// A name as the server reads it: quotes stripped, unquoted PostgreSQL names lowercased
fn name(x: &Tok, sql: &str, kind: DbKind) -> String {
    let raw = &sql[x.start..x.end];
    match raw.chars().next() {
        Some(q @ ('"' | '`' | '\'')) if raw.len() >= 2 => raw[1..raw.len() - 1].replace(&format!("{q}{q}"), &q.to_string()),
        _ if kind == DbKind::Postgres => x.text.clone(),
        _ => raw.to_string(),
    }
}

fn item(it: &[Tok], sql: &str, kind: DbKind) -> Item {
    let is = |i: usize, s: &str| it.get(i).is_some_and(|x| x.text == s && !x.quoted);
    // A part of a column name: a quoted name (in MySQL "…" is a string unless ANSI_QUOTES is on), or a bare word that isn't a number or a value
    let part = |x: &Tok| match x.quoted {
        true => !(kind == DbKind::Mysql && sql[x.start..].starts_with('"')),
        false => x.word && !x.text.starts_with(|c: char| c.is_ascii_digit()) && !VALUE_WORDS.contains(&x.text.as_str()),
    };
    // The part before the last one: the table or alias (schema.table.column has the table there too)
    let of = |i: usize| (i >= 3).then(|| name(&it[i - 3], sql, kind));
    // name(.name)*, or (name.)* followed by *
    let mut i = 0;
    loop {
        if is(i, "*") {
            return if i + 1 == it.len() { Item::All { of: (i >= 2).then(|| name(&it[i - 2], sql, kind)) } } else { Item::Other };
        }
        match it.get(i) {
            Some(x) if part(x) => i += 1,
            _ => return Item::Other,
        }
        if !is(i, ".") {
            break;
        }
        i += 1;
    }
    let col = name(&it[i - 1], sql, kind);
    // A string right after the word is a literal with a prefix, not an alias: N'…' / n'…' (national), _utf8mb4'…' or
    // _utf8mb4 '…' (charset introducer, a space allowed)
    if i == 1 && it.get(1).is_some_and(|a| a.text == "'" && (a.start == it[0].end || it[0].text.starts_with('_'))) {
        return Item::Other;
    }
    let of = of(i);
    let alias_at = if is(i, "as") { i + 1 } else { i };
    match it.get(alias_at) {
        None if alias_at == i => Item::Column { of, name: col, alias: None },
        Some(a) if alias_at + 1 == it.len() && (a.word || a.text == "'") => Item::Column { of, name: col, alias: Some(name(a, sql, kind)) },
        _ => Item::Other,
    }
}

/// A table a result comes from, as the catalog has it
#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    pub schema: String,
    pub table: String,
    /// Not a base table: "view", "materialized view", "foreign table" …
    pub not_table: Option<&'static str>,
    /// All columns, in table order
    pub columns: Vec<String>,
    pub pk: Vec<String>,
}

/// MySQL doesn't report the column origins through sqlx: line the select list up with the table's columns.
/// → the table column of each result column (None: computed)
pub fn mysql_sources(items: &[Item], result: &[String], t: &TableInfo) -> Result<Vec<Option<String>>, String> {
    let mut out: Vec<(Option<String>, Option<&str>)> = Vec::new();
    for it in items {
        match it {
            Item::All { .. } => out.extend(t.columns.iter().map(|c| (Some(c.clone()), Some(c.as_str())))),
            Item::Column { name, alias, .. } => {
                let c = t.columns.iter().find(|c| c.eq_ignore_ascii_case(name)).ok_or_else(|| e::EDIT_UNMATCHED.to_string())?;
                out.push((Some(c.clone()), Some(alias.as_deref().unwrap_or(name))));
            }
            Item::Other => out.push((None, None)),
        }
    }
    // The server's names must be the ones read here, or the two disagree on what the statement selects
    let lines_up = out.len() == result.len() && out.iter().zip(result).all(|((_, want), got)| want.is_none_or(|w| w.eq_ignore_ascii_case(got)));
    match lines_up {
        true => Ok(out.into_iter().map(|(c, _)| c).collect()),
        false => Err(e::EDIT_UNMATCHED.into()),
    }
}

/// The rules for one table, once the table column of each result column (None: computed) is known
pub fn decide(t: &TableInfo, result: &[String], sources: &[Option<String>]) -> Editable {
    let ro = Editable::ReadOnly;
    let full = format!("{}.{}", t.schema, t.table);
    if let Some(what) = t.not_table {
        return ro(e::edit_not_table(&full, what));
    }
    if let Some(i) = sources.iter().position(Option::is_none) {
        return ro(e::edit_computed(&result[i]));
    }
    let cols: Vec<String> = sources.iter().flatten().cloned().collect();
    let twice = |v: &[String]| v.iter().enumerate().find(|(i, c)| v[..*i].contains(c)).map(|(_, c)| c.clone());
    if let Some(c) = twice(result).or_else(|| twice(&cols)) {
        return ro(e::edit_duplicate(&c));
    }
    if t.pk.is_empty() {
        return ro(e::edit_no_pk(&full));
    }
    if !t.pk.iter().all(|k| cols.contains(k)) {
        return ro(e::edit_missing_pk(&full, &t.pk));
    }
    Editable::Table(EditTarget { schema: t.schema.clone(), table: t.table.clone(), pk: t.pk.clone(), columns: cols })
}

/// Where a join result's column comes from
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Column `column` of the table in FROM at `from`
    Col { from: usize, column: String },
    /// An expression, an aggregate, a cast, a system column
    Computed,
    /// Not known for sure: the reason it is read-only
    Unknown(String),
}

/// The select list lined up with the result: each column's source and the name the server should give it (None: computed).
/// cols[k] = the columns of the table in FROM at k, None when unknown. None when the items can't be lined up with the result
/// (a `*` over a table whose columns aren't known, or with USING / NATURAL, whose merged columns come once)
pub fn align(shape: &Shape, cols: &[Option<Vec<String>>], kind: DbKind, result: usize) -> Option<Vec<(Source, Option<String>)>> {
    // PostgreSQL names are as the server reads them already (lowercased unless quoted); MySQL column names ignore case
    let same = |a: &str, b: &str| if kind == DbKind::Mysql { a.eq_ignore_ascii_case(b) } else { a == b };
    let keyed = |q: &str| {
        let hits: Vec<usize> = (0..shape.from.len()).filter(|&k| same(shape.from[k].key(), q)).collect();
        (hits.len() == 1).then(|| hits[0])
    };
    let column = |k: usize, n: &str| cols.get(k)?.as_ref()?.iter().find(|c| same(c, n)).cloned();
    let mut out = Vec::new();
    for it in &shape.items {
        match it {
            Item::All { of: None } if shape.using => return None,
            Item::All { of: None } => {
                for (k, c) in cols.iter().enumerate() {
                    out.extend(c.as_ref()?.iter().map(|n| (Source::Col { from: k, column: n.clone() }, Some(n.clone()))));
                }
            }
            Item::All { of: Some(q) } => {
                let k = keyed(q)?;
                out.extend(cols.get(k)?.as_ref()?.iter().map(|n| (Source::Col { from: k, column: n.clone() }, Some(n.clone()))));
            }
            Item::Column { of, name, alias } => {
                let shown = Some(alias.clone().unwrap_or_else(|| name.clone()));
                // Qualified: that table's. Bare: the one table that has it (the server refuses one that more than one has)
                let ks: Vec<usize> = match of {
                    Some(q) => keyed(q).into_iter().collect(),
                    None => (0..shape.from.len()).filter(|&k| column(k, name).is_some()).collect(),
                };
                out.push(match ks[..] {
                    [k] => match column(k, name) {
                        Some(c) => (Source::Col { from: k, column: c }, shown),
                        None => (Source::Unknown(e::EDIT_UNMATCHED.into()), shown),
                    },
                    _ => (Source::Unknown(e::EDIT_UNMATCHED.into()), shown),
                });
            }
            Item::Other => out.push((Source::Computed, None)),
        }
    }
    (out.len() == result).then_some(out)
}

/// The rules for a join, once the tables in FROM (Err: why that one can't be edited, such as a temporary table) and the source
/// of each result column are known. A table's columns can be edited when it is a base table whose whole primary key is in the
/// result; when none can, the result is read-only with the first column's reason
pub fn decide_join(tables: &[Result<TableInfo, String>], result: &[String], sources: &[Source]) -> Editable {
    let full = |t: &TableInfo| format!("{}.{}", t.schema, t.table);
    // Each table's targets: its key columns' result columns, or why its columns are read-only
    let mut target: Vec<Result<usize, String>> = Vec::new();
    let mut targets = Vec::new();
    for (k, t) in tables.iter().enumerate() {
        let at = |c: &str| sources.iter().position(|s| matches!(s, Source::Col { from, column } if *from == k && column == c));
        target.push(match t {
            Err(why) => Err(why.clone()),
            Ok(t) if t.not_table.is_some() => Err(e::edit_not_table(&full(t), t.not_table.unwrap_or_default())),
            Ok(t) if t.pk.is_empty() => Err(e::edit_no_pk(&full(t))),
            Ok(t) => match t.pk.iter().map(|c| at(c)).collect::<Option<Vec<usize>>>() {
                None => Err(e::edit_missing_pk(&full(t), &t.pk)),
                Some(pk_at) => {
                    targets.push(JoinTable { schema: t.schema.clone(), table: t.table.clone(), pk: t.pk.clone(), pk_at });
                    Ok(targets.len() - 1)
                }
            },
        });
    }
    let columns: Vec<ColumnEdit> = sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ro = |read_only: String| ColumnEdit::ReadOnly { read_only };
            match s {
                // The same table column twice: an edit in one cell would leave the other showing the old value
                Source::Col { from, column } if sources[..i].contains(s) || sources[i + 1..].contains(s) => match &target[*from] {
                    Ok(_) => ro(e::edit_duplicate(column)),
                    Err(why) => ro(why.clone()),
                },
                Source::Col { from, column } => match &target[*from] {
                    Ok(t) => ColumnEdit::To { table: *t, column: column.clone() },
                    Err(why) => ro(why.clone()),
                },
                Source::Computed => ro(e::edit_computed(&result[i])),
                Source::Unknown(why) => ro(why.clone()),
            }
        })
        .collect();
    if !columns.iter().any(|c| matches!(c, ColumnEdit::To { .. })) {
        return match columns.into_iter().next() {
            Some(ColumnEdit::ReadOnly { read_only }) => Editable::ReadOnly(read_only),
            _ => Editable::ReadOnly(e::EDIT_NO_TABLE.into()),
        };
    }
    Editable::Join(JoinEdit { tables: targets, columns })
}

/// Why Submit can't save the edits of a table without a primary key (rows are found by it)
pub const EDIT_NO_KEY: &str = "Table has no primary key — cannot update/delete rows";

/// A value as the SQL Submit writes: None = NULL, the GUI's Set DEFAULT token = DEFAULT, else a string literal `kind` reads
fn value(kind: DbKind, v: &Option<String>) -> String {
    match v.as_deref() {
        Some(crate::extract::DEFAULT_TOKEN) => "DEFAULT".into(),
        v => crate::sql::literal(kind, v),
    }
}

/// Submit's statements for one table (the GUI and the TUI), without a trailing `;`, in the order they run: DELETE, then UPDATE, then
/// INSERT, so a deleted row frees a unique value an update or insert may take; a row both edited and deleted is only deleted.
/// rows = the result rows' raw text (only the edited and deleted rows' key values are read), edits = (row, column) → value of those
/// rows (None = NULL), added = per new row the cells the user set (an unset cell is left out: the column default applies; one set to
/// NULL is NULL). A new row with no cell set inserts the defaults. Err: there are rows to update or delete and the table has no key
pub fn edit_statements(
    kind: DbKind,
    to: &EditTarget,
    rows: &[Vec<Option<String>>],
    edits: &BTreeMap<(usize, usize), Option<String>>,
    deleted: &BTreeSet<usize>,
    added: &[BTreeMap<usize, Option<String>>],
) -> Result<Vec<String>, String> {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    let table = format!("{}.{}", q(&to.schema), q(&to.table));
    // The table column of each result column (a console result's aliases): what the SQL names
    let col = |i: usize| q(&to.columns[i]);
    let pk_at: Vec<usize> = to.pk.iter().filter_map(|p| to.columns.iter().position(|c| c == p)).collect();
    let at = |r: usize, i: usize| rows.get(r).and_then(|row| row.get(i)).cloned().flatten();
    let key = |r: usize| -> Result<String, String> {
        if pk_at.is_empty() || pk_at.len() != to.pk.len() {
            return Err(EDIT_NO_KEY.into());
        }
        Ok(pk_at.iter().map(|&i| format!("{} = {}", col(i), crate::sql::literal(kind, at(r, i).as_deref()))).collect::<Vec<_>>().join(" AND "))
    };
    let mut out = vec![];
    for &r in deleted {
        out.push(format!("DELETE FROM {table} WHERE {}", key(r)?));
    }
    let mut sets: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for ((r, c), v) in edits.iter().filter(|((r, _), _)| !deleted.contains(r)) {
        sets.entry(*r).or_default().push(format!("{} = {}", col(*c), value(kind, v)));
    }
    for (r, set) in sets {
        out.push(format!("UPDATE {table} SET {} WHERE {}", set.join(", "), key(r)?));
    }
    for row in added {
        out.push(match (row.is_empty(), kind) {
            (true, DbKind::Mysql) => format!("INSERT INTO {table} () VALUES ()"),
            (true, _) => format!("INSERT INTO {table} DEFAULT VALUES"),
            _ => format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                row.keys().map(|&c| col(c)).collect::<Vec<_>>().join(", "),
                row.values().map(|v| value(kind, v)).collect::<Vec<_>>().join(", ")
            ),
        });
    }
    Ok(out)
}

/// Submit's statements for a join (#102): per edited row, one UPDATE for each table its edits go to, found by that table's key
/// (never by a column's name), without a trailing `;`. rows and edits as edit_statements reads them; an edit of a cell that can't
/// be edited (JoinEdit::cell) is left out
pub fn join_statements(kind: DbKind, j: &JoinEdit, rows: &[Vec<Option<String>>], edits: &BTreeMap<(usize, usize), Option<String>>) -> Vec<String> {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    let at = |r: usize, i: usize| rows.get(r).and_then(|row| row.get(i)).cloned().flatten();
    let mut sets: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    for ((r, c), v) in edits {
        if let (Ok((_, column)), Some(ColumnEdit::To { table, .. })) = (j.cell(*c, |i| at(*r, i).is_none()), j.columns.get(*c)) {
            sets.entry((*r, *table)).or_default().push(format!("{} = {}", q(column), value(kind, v)));
        }
    }
    sets.into_iter()
        .map(|((r, ti), set)| {
            let t = &j.tables[ti];
            let key = t.pk.iter().zip(&t.pk_at).map(|(k, &i)| format!("{} = {}", q(k), crate::sql::literal(kind, at(r, i).as_deref()))).collect::<Vec<_>>().join(" AND ");
            format!("UPDATE {}.{} SET {} WHERE {key}", q(&t.schema), q(&t.table), set.join(", "))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: DbKind = DbKind::Postgres;
    const MY: DbKind = DbKind::Mysql;

    fn col(n: &str) -> Item {
        Item::Column { of: None, name: n.into(), alias: None }
    }
    fn qcol(of: &str, n: &str) -> Item {
        Item::Column { of: Some(of.into()), name: n.into(), alias: None }
    }
    fn all() -> Item {
        Item::All { of: None }
    }
    fn tab(t: &str, alias: Option<&str>) -> FromTable {
        FromTable { schema: None, table: t.into(), alias: alias.map(String::from) }
    }
    fn one(q: &str, k: DbKind) -> Shape {
        let s = select_shape(q, k).unwrap_or_else(|e| panic!("{q}: {e}"));
        assert_eq!(s.from.len(), 1, "{q}");
        s
    }

    #[test]
    fn plain_selects_from_one_table() {
        let r = one("select * from users", PG);
        assert_eq!(r, Shape { items: vec![all()], from: vec![tab("users", None)], using: false });
        let r = one("SELECT u.id, u.\"Name\" AS n, email e FROM Public.Users u WHERE id IN (SELECT 1 UNION SELECT 2) ORDER BY 1 LIMIT 5;", PG);
        assert_eq!(r.from, vec![FromTable { schema: Some("public".into()), table: "users".into(), alias: Some("u".into()) }]);
        assert_eq!(r.items, vec![qcol("u", "id"), Item::Column { of: Some("u".into()), name: "Name".into(), alias: Some("n".into()) }, Item::Column { of: None, name: "email".into(), alias: Some("e".into()) }]);
        let r = one("select `Id`, t.* , shop.t.name from shop.`My T` as t limit 10", MY);
        assert_eq!((r.from[0].schema.as_deref(), r.from[0].table.as_str()), (Some("shop"), "My T"));
        assert_eq!(r.items, vec![col("Id"), Item::All { of: Some("t".into()) }, qcol("t", "name")]);
        // -- comment and /* */ before it, a trailing comment after the ;
        one("-- top\n/* x */ select id from t; -- done", PG);
        // Expressions are items of their own (the origins or the table decide later)
        let r = one("select id, lower(name), 1, id + 1 as x, id::text, current_date, \"s\", null, count(*) from t", MY);
        assert_eq!(r.items, vec![col("id"), Item::Other, Item::Other, Item::Other, Item::Other, Item::Other, Item::Other, Item::Other, Item::Other]);
        // A string alias (MySQL)
        assert_eq!(one("select id 'the id' from t", MY).items, vec![Item::Column { of: None, name: "id".into(), alias: Some("the id".into()) }]);
        // IS DISTINCT FROM in WHERE isn't DISTINCT
        one("select id from t where a is distinct from b", PG);
    }

    #[test]
    fn joins_list_their_tables() {
        let ok = |q: &str, k: DbKind| select_shape(q, k).unwrap_or_else(|e| panic!("{q}: {e}"));
        let r = ok("select * from crm.voc as vo left join public.users as pu on vo.user_idx = pu.idx where vo.idx = 1", PG);
        assert_eq!(r.from, vec![FromTable { schema: Some("crm".into()), table: "voc".into(), alias: Some("vo".into()) }, FromTable { schema: Some("public".into()), table: "users".into(), alias: Some("pu".into()) }]);
        assert!(!r.using);
        // Every join form; ON with subqueries, functions named like join words, a column named like one after a dot
        let r = ok("select a.id, b.* from t a inner join u b on b.p = a.id and left(a.x, 1) = right(b.y, 1) and a.full = (select 1) natural left outer join v cross join w, x straight_join y order by 1", MY);
        assert_eq!(r.from.iter().map(|f| f.table.as_str()).collect::<Vec<_>>(), ["t", "u", "v", "w", "x", "y"]);
        assert_eq!(r.items, vec![qcol("a", "id"), Item::All { of: Some("b".into()) }]);
        assert!(r.using, "NATURAL merges columns");
        assert!(ok("select * from t join u using (id, k)", PG).using);
        assert!(!ok("select * from t full outer join u on t.id = u.id, v", PG).using);
        // Self-join: two occurrences
        let r = ok("select a.id, b.id from t a join t b on a.parent = b.id", PG);
        assert_eq!(r.from, vec![tab("t", Some("a")), tab("t", Some("b"))]);
        // Unaliased, then an alias with AS and a quoted alias
        assert_eq!(ok("select 1 from t join u as \"U\" on true", PG).from, vec![tab("t", None), tab("u", Some("U"))]);
    }

    #[test]
    fn refuses_what_isnt_a_plain_select() {
        let why = |q: &str, k: DbKind| select_shape(q, k).expect_err(q);
        assert_eq!(why("select a, count(*) from t group by a", PG), e::edit_combined("GROUP BY"));
        assert_eq!(why("select distinct a from t", PG), e::edit_combined("DISTINCT"));
        assert_eq!(why("select distinct on (a) a, id from t", PG), e::edit_combined("DISTINCT"));
        assert_eq!(why("select id from t union all select id from t", PG), e::edit_combined("UNION"));
        assert_eq!(why("select id from t where x = 1 except select id from u", PG), e::edit_combined("EXCEPT"));
        assert_eq!(why("select 1", PG), e::EDIT_NO_TABLE);
        assert_eq!(why("select now()", MY), e::EDIT_NO_TABLE);
        for q in [
            "with x as (select * from t) select * from x",
            "select * from (select * from t) s",
            "select * from t join (select * from u) s on true",
            "select * from t, lateral (select 1) s",
            "select * from t join lateral f(t.id) on true",
            "select * from (t join u on true)",
            "select * from generate_series(1, 3) g",
            "select * from t join generate_series(1, 3) g on true",
            "select * from only t",
            "select * into t2 from t",
            "select id from t; select id from u",
            "table t",
            "values (1)",
            "update t set a = 1",
            "select * from db.s.t",
            "select * from t tablesample system (10)",
            "select row_number() over w, id from t window w as (order by id)",
            "select * from t left u on true",
            "select * from t join u using id",
        ] {
            assert_eq!(why(q, PG), e::EDIT_NOT_PLAIN, "{q}");
        }
        // MySQL executable comments, partitions, index hints
        assert_eq!(why("select /*! sql_no_cache */ id from t", MY), e::EDIT_NOT_PLAIN);
        assert_eq!(why("select id from t partition (p0)", MY), e::EDIT_NOT_PLAIN);
        assert_eq!(why("select * from t join u use index (i) on true", MY), e::EDIT_NOT_PLAIN);
        // Words inside strings or quotes don't count
        select_shape("select id, \"group\" from t where note = 'a join b union c'", PG).unwrap();
        // Locking reads: in Auto mode the cursor's transaction holds the locks while edits save on another connection
        for q in ["select * from t for update", "select * from t where id = 1 for share", "select * from t for no key update", "select * from t for key share", "select * from t a join u b on true for update of a nowait"] {
            assert_eq!(why(q, PG), e::EDIT_LOCKS, "{q}");
        }
        for q in ["select * from t lock in share mode", "select * from t where id > 0 for share", "select * from t for update skip locked"] {
            assert_eq!(why(q, MY), e::EDIT_LOCKS, "{q}");
        }
    }

    fn table(cols: &[&str], pk: &[&str]) -> TableInfo {
        TableInfo { schema: "public".into(), table: "users".into(), not_table: None, columns: cols.iter().map(|c| c.to_string()).collect(), pk: pk.iter().map(|c| c.to_string()).collect() }
    }
    fn named(t: &str, cols: &[&str], pk: &[&str]) -> TableInfo {
        TableInfo { table: t.into(), ..table(cols, pk) }
    }
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|c| c.to_string()).collect()
    }
    fn some(v: &[&str]) -> Vec<Option<String>> {
        v.iter().map(|c| Some(c.to_string())).collect()
    }

    #[test]
    fn decide_rules() {
        let t = table(&["id", "name", "email"], &["id"]);
        let target = |cols: &[&str]| Editable::Table(EditTarget { schema: "public".into(), table: "users".into(), pk: s(&["id"]), columns: s(cols) });
        assert_eq!(decide(&t, &s(&["id", "name", "email"]), &some(&["id", "name", "email"])), target(&["id", "name", "email"]));
        // A subset with the key, aliased
        assert_eq!(decide(&t, &s(&["user_id", "n"]), &some(&["id", "name"])), target(&["id", "name"]));
        let ro = |r: &[&str], src: Vec<Option<String>>, t: &TableInfo| match decide(t, &s(r), &src) {
            Editable::ReadOnly(why) => why,
            other => panic!("{other:?}"),
        };
        assert_eq!(ro(&["name"], some(&["name"]), &t), e::edit_missing_pk("public.users", &s(&["id"])));
        assert_eq!(ro(&["id", "upper"], vec![Some("id".into()), None], &t), e::edit_computed("upper"));
        assert_eq!(ro(&["id", "id"], some(&["id", "id"]), &t), e::edit_duplicate("id"));
        assert_eq!(ro(&["id", "name", "n2"], some(&["id", "name", "name"]), &t), e::edit_duplicate("name"));
        assert_eq!(ro(&["id"], some(&["id"]), &table(&["id"], &[])), e::edit_no_pk("public.users"));
        let v = TableInfo { not_table: Some("view"), ..t.clone() };
        assert_eq!(ro(&["id"], some(&["id"]), &v), e::edit_not_table("public.users", "view"));
        // A composite key needs all its columns
        let c = table(&["a", "b", "x"], &["a", "b"]);
        assert_eq!(ro(&["a", "x"], some(&["a", "x"]), &c), e::edit_missing_pk("public.users", &s(&["a", "b"])));
        assert!(matches!(decide(&c, &s(&["b", "x", "a"]), &some(&["b", "x", "a"])), Editable::Table(_)));
    }

    #[test]
    fn mysql_lines_items_up_with_the_table() {
        let t = table(&["id", "Name", "email"], &["id"]);
        let q = |sql: &str| one(sql, MY).items;
        assert_eq!(mysql_sources(&q("select * from users"), &s(&["id", "Name", "email"]), &t), Ok(some(&["id", "Name", "email"])));
        // Case-insensitive names give the table's spelling; aliases are checked against the server's names
        assert_eq!(mysql_sources(&q("select NAME as n, ID from users"), &s(&["n", "ID"]), &t), Ok(some(&["Name", "id"])));
        assert_eq!(mysql_sources(&q("select id, now() from users"), &s(&["id", "now()"]), &t), Ok(vec![Some("id".into()), None]));
        assert_eq!(mysql_sources(&q("select id, *, id from users"), &s(&["id", "id", "Name", "email", "id"]), &t), Ok(some(&["id", "id", "Name", "email", "id"])));
        // Not a column of the table, or the server's result doesn't line up
        assert_eq!(mysql_sources(&q("select nope from users"), &s(&["nope"]), &t), Err(e::EDIT_UNMATCHED.into()));
        assert_eq!(mysql_sources(&q("select id from users"), &s(&["id", "x"]), &t), Err(e::EDIT_UNMATCHED.into()));
        assert_eq!(mysql_sources(&q("select id as a from users"), &s(&["b"]), &t), Err(e::EDIT_UNMATCHED.into()));
        // A string right after a word is a literal with a prefix (N'…' national, _utf8mb4'…' charset), not a column with an alias
        let col_n = Item::Column { of: None, name: "n".into(), alias: Some("zzz".into()) };
        assert_eq!(q("select id, N'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, n'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, _utf8mb4'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, _utf8mb4 'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, n 'zzz' from users"), vec![col("id"), col_n]);
    }

    /// The voc / users tables of #102: voc (idx, user_idx, status), users (idx, role, status)
    fn voc_users() -> [Option<Vec<String>>; 2] {
        [Some(s(&["idx", "user_idx", "status"])), Some(s(&["idx", "role", "status"]))]
    }
    fn src(from: usize, c: &str) -> Source {
        Source::Col { from, column: c.into() }
    }

    #[test]
    fn align_lines_a_join_up() {
        let shape = |q: &str, k| select_shape(q, k).unwrap();
        let cols = voc_users();
        let names = |v: Vec<(Source, Option<String>)>| v.into_iter().map(|x| x.1).collect::<Vec<_>>();
        // `*`: each table's columns in turn; duplicate names (idx, status) come apart by their place
        let q = shape("select * from voc vo left join users pu on vo.user_idx = pu.idx", PG);
        let a = align(&q, &cols, PG, 6).unwrap();
        assert_eq!(a.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), [src(0, "idx"), src(0, "user_idx"), src(0, "status"), src(1, "idx"), src(1, "role"), src(1, "status")]);
        assert_eq!(names(a), [Some("idx".to_string()), Some("user_idx".into()), Some("status".into()), Some("idx".into()), Some("role".into()), Some("status".into())]);
        // alias.*, qualified and bare columns (role is only in users), aliases swapping the names, expressions
        let q = shape("select pu.*, vo.status as role, role as status, upper(vo.status), user_idx from voc vo join users pu on true", MY);
        let a = align(&q, &cols, MY, 7).unwrap();
        assert_eq!(a.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), [src(1, "idx"), src(1, "role"), src(1, "status"), src(0, "status"), src(1, "role"), Source::Computed, src(0, "user_idx")]);
        assert_eq!(a[3].1.as_deref(), Some("role"));
        // A bare column both tables have, or none: not known (the server would refuse the first)
        let q = shape("select status, nope, x.idx from voc join users on true", PG);
        let a = align(&q, &cols, PG, 3).unwrap();
        assert!(a.iter().all(|x| x.0 == Source::Unknown(e::EDIT_UNMATCHED.into())), "{a:?}");
        // MySQL names ignore case; PostgreSQL's are as the server read them
        let q = shape("select VO.IDX from voc vo join users pu on true", MY);
        assert_eq!(align(&q, &cols, MY, 1).unwrap()[0].0, src(0, "idx"));
        // Self-join: each occurrence by its alias, `*` by its place
        let tt = [Some(s(&["id", "parent", "name"])), Some(s(&["id", "parent", "name"]))];
        let q = shape("select b.name, a.id, b.id, a.name from t a join t b on a.parent = b.id", PG);
        assert_eq!(align(&q, &tt, PG, 4).unwrap().into_iter().map(|x| x.0).collect::<Vec<_>>(), [src(1, "name"), src(0, "id"), src(1, "id"), src(0, "name")]);
        let q = shape("select * from t a join t b on a.parent = b.id", PG);
        assert_eq!(align(&q, &tt, PG, 6).unwrap()[3].0, src(1, "id"));
        // Can't be lined up: * with USING, a table whose columns aren't known, a count the server disagrees with
        assert_eq!(align(&shape("select * from voc join users using (idx)", PG), &cols, PG, 5), None);
        assert!(align(&shape("select users.* from voc join users using (idx)", PG), &cols, PG, 3).is_some());
        assert_eq!(align(&shape("select * from voc join users on true", PG), &[cols[0].clone(), None], PG, 6), None);
        assert_eq!(align(&shape("select * from voc join users on true", PG), &cols, PG, 5), None);
        // Two occurrences with the same name and no alias: alias.column can't tell them apart
        let q = shape("select t.id from s1.t join s2.t on true", PG);
        assert_eq!(align(&q, &tt, PG, 1).unwrap()[0].0, Source::Unknown(e::EDIT_UNMATCHED.into()));
    }

    fn join_ro(e: &Editable) -> &str {
        match e {
            Editable::ReadOnly(why) => why,
            other => panic!("{other:?}"),
        }
    }
    fn to(table: usize, c: &str) -> ColumnEdit {
        ColumnEdit::To { table, column: c.into() }
    }
    fn ro(why: String) -> ColumnEdit {
        ColumnEdit::ReadOnly { read_only: why }
    }

    #[test]
    fn decide_join_per_table() {
        let voc = named("voc", &["idx", "user_idx", "status"], &["idx"]);
        let users = named("users", &["idx", "role", "status"], &["idx"]);
        let result = s(&["idx", "user_idx", "status", "idx", "role", "status"]);
        let all = vec![src(0, "idx"), src(0, "user_idx"), src(0, "status"), src(1, "idx"), src(1, "role"), src(1, "status")];
        let jt = |t: &str, at: usize| JoinTable { schema: "public".into(), table: t.into(), pk: s(&["idx"]), pk_at: vec![at] };
        // Both keys in the result: each column to its own table, duplicate names apart by origin
        let r = decide_join(&[Ok(voc.clone()), Ok(users.clone())], &result, &all);
        assert_eq!(r, Editable::Join(JoinEdit { tables: vec![jt("voc", 0), jt("users", 3)], columns: vec![to(0, "idx"), to(0, "user_idx"), to(0, "status"), to(1, "idx"), to(1, "role"), to(1, "status")] }));
        // The users key missing: only voc's columns, users' say why
        let r = decide_join(&[Ok(voc.clone()), Ok(users.clone())], &s(&["idx", "role"]), &[src(0, "idx"), src(1, "role")]);
        let missing = e::edit_missing_pk("public.users", &s(&["idx"]));
        assert_eq!(r, Editable::Join(JoinEdit { tables: vec![jt("voc", 0)], columns: vec![to(0, "idx"), ro(missing.clone())] }));
        // Neither key: read-only, saying why for the first column
        let r = decide_join(&[Ok(voc.clone()), Ok(users.clone())], &s(&["status", "role"]), &[src(0, "status"), src(1, "role")]);
        assert_eq!(join_ro(&r), e::edit_missing_pk("public.voc", &s(&["idx"])));
        // A view, a table without a key, a temporary table (Err), computed and unknown columns
        let v = TableInfo { not_table: Some("view"), ..users.clone() };
        let r = decide_join(&[Ok(voc.clone()), Ok(v)], &s(&["idx", "role", "n", "x"]), &[src(0, "idx"), src(1, "role"), Source::Computed, Source::Unknown("why".into())]);
        let Editable::Join(j) = r else { panic!() };
        assert_eq!(j.columns[1..], [ro(e::edit_not_table("public.users", "view")), ro(e::edit_computed("n")), ro("why".into())]);
        let nopk = named("users", &["idx", "role"], &[]);
        let Editable::Join(j) = decide_join(&[Ok(voc.clone()), Ok(nopk)], &s(&["idx", "role"]), &[src(0, "idx"), src(1, "role")]) else { panic!() };
        assert_eq!(j.columns[1], ro(e::edit_no_pk("public.users")));
        let Editable::Join(j) = decide_join(&[Ok(voc.clone()), Err("temp".into())], &s(&["idx", "role"]), &[src(0, "idx"), src(1, "role")]) else { panic!() };
        assert_eq!(j.columns[1], ro("temp".into()));
        // The same column twice: both cells read-only, the key still finds the row
        let Editable::Join(j) = decide_join(&[Ok(voc.clone()), Ok(users.clone())], &s(&["idx", "status", "s2", "idx"]), &[src(0, "idx"), src(0, "status"), src(0, "status"), src(1, "idx")]) else { panic!() };
        assert_eq!(j.columns, [to(0, "idx"), ro(e::edit_duplicate("status")), ro(e::edit_duplicate("status")), to(1, "idx")]);
        // Self-join: each occurrence by its own key
        let t = named("t", &["id", "name"], &["id"]);
        let Editable::Join(j) = decide_join(&[Ok(t.clone()), Ok(t.clone())], &s(&["name", "id", "id", "name"]), &[src(1, "name"), src(0, "id"), src(1, "id"), src(0, "name")]) else { panic!() };
        assert_eq!(j.tables.iter().map(|t| t.pk_at.clone()).collect::<Vec<_>>(), [vec![1], vec![2]]);
        assert_eq!(j.columns, [to(1, "name"), to(0, "id"), to(1, "id"), to(0, "name")]);
        // A composite key needs all its columns
        let c = named("c", &["a", "b", "x"], &["a", "b"]);
        let Editable::Join(j) = decide_join(&[Ok(voc), Ok(c)], &s(&["idx", "b", "x", "a"]), &[src(0, "idx"), src(1, "b"), src(1, "x"), src(1, "a")]) else { panic!() };
        assert_eq!(j.tables[1].pk_at, [3, 1]);
    }

    #[test]
    fn join_cell_on_the_outer_side_is_read_only() {
        let j = JoinEdit {
            tables: vec![JoinTable { schema: "crm".into(), table: "voc".into(), pk: s(&["idx"]), pk_at: vec![0] }, JoinTable { schema: "public".into(), table: "users".into(), pk: s(&["idx"]), pk_at: vec![2] }],
            columns: vec![to(0, "idx"), to(0, "status"), to(1, "idx"), to(1, "role"), ro("why".into())],
        };
        // No users row matched (LEFT JOIN): its key is NULL there
        let row_null = |i: usize| i >= 2;
        assert_eq!(j.cell(1, row_null).map(|(t, c)| (t.table.as_str(), c)), Ok(("voc", "status")));
        assert_eq!(j.cell(3, row_null).unwrap_err(), e::edit_no_row("public.users"));
        assert_eq!(j.cell(3, |_| false).map(|(t, c)| (t.table.as_str(), c)), Ok(("users", "role")));
        assert_eq!(j.cell(4, |_| false).unwrap_err(), "why");
        assert_eq!(j.cell(9, |_| false).unwrap_err(), e::EDIT_UNMATCHED);
    }

    #[test]
    fn serializes_as_edit_or_reason() {
        #[derive(Serialize)]
        struct R {
            #[serde(flatten)]
            e: Option<Editable>,
        }
        let t = EditTarget { schema: "s".into(), table: "t".into(), pk: s(&["id"]), columns: s(&["id"]) };
        assert_eq!(serde_json::to_value(R { e: Some(Editable::Table(t)) }).unwrap(), serde_json::json!({"edit": {"schema": "s", "table": "t", "pk": ["id"], "columns": ["id"]}}));
        assert_eq!(serde_json::to_value(R { e: Some(Editable::ReadOnly("why".into())) }).unwrap(), serde_json::json!({"readOnlyReason": "why"}));
        assert_eq!(serde_json::to_value(R { e: None }).unwrap(), serde_json::json!({}));
        let j = JoinEdit { tables: vec![JoinTable { schema: "s".into(), table: "t".into(), pk: s(&["id"]), pk_at: vec![0] }], columns: vec![to(0, "id"), ro("why".into())] };
        assert_eq!(
            serde_json::to_value(R { e: Some(Editable::Join(j)) }).unwrap(),
            serde_json::json!({"editJoin": {"tables": [{"schema": "s", "table": "t", "pk": ["id"], "pkAt": [0]}], "columns": [{"table": 0, "column": "id"}, {"readOnly": "why"}]}})
        );
    }

    mod submit {
        use super::*;

        const SQLITE: DbKind = DbKind::Sqlite;

        fn users() -> EditTarget {
            EditTarget { schema: "public".into(), table: "users".into(), pk: s(&["id"]), columns: s(&["id", "name", "memo"]) }
        }
        fn rows() -> Vec<Vec<Option<String>>> {
            vec![some(&["1", "a", "x"]), some(&["2", "b", "y"]), some(&["3", "c", "z"])]
        }
        fn cells(v: &[(usize, usize, Option<&str>)]) -> BTreeMap<(usize, usize), Option<String>> {
            v.iter().map(|&(r, c, x)| ((r, c), x.map(String::from))).collect()
        }
        fn new_row(v: &[(usize, Option<&str>)]) -> BTreeMap<usize, Option<String>> {
            v.iter().map(|&(c, x)| (c, x.map(String::from))).collect()
        }
        fn none() -> BTreeSet<usize> {
            BTreeSet::new()
        }
        fn add(kind: DbKind, added: &[BTreeMap<usize, Option<String>>]) -> Vec<String> {
            edit_statements(kind, &users(), &rows(), &cells(&[]), &none(), added).unwrap()
        }

        #[test]
        fn all_default_row_per_kind() {
            assert_eq!(add(MY, &[new_row(&[])]), ["INSERT INTO public.users () VALUES ()"]);
            assert_eq!(add(PG, &[new_row(&[])]), ["INSERT INTO public.users DEFAULT VALUES"]);
            assert_eq!(add(SQLITE, &[new_row(&[])]), ["INSERT INTO public.users DEFAULT VALUES"]);
        }

        #[test]
        fn untouched_cell_is_left_out_and_a_null_one_is_sent() {
            for k in [PG, MY, SQLITE] {
                // name set to NULL, memo never touched: the column default applies to memo only
                assert_eq!(add(k, &[new_row(&[(1, None)])]), ["INSERT INTO public.users (name) VALUES (NULL)"], "{k:?}");
                assert_eq!(add(k, &[new_row(&[(0, Some("9")), (2, Some("m"))])]), ["INSERT INTO public.users (id, memo) VALUES ('9', 'm')"], "{k:?}");
                let up = edit_statements(k, &users(), &rows(), &cells(&[(0, 2, None)]), &none(), &[]).unwrap();
                assert_eq!(up, ["UPDATE public.users SET memo = NULL WHERE id = '1'"], "{k:?}");
            }
        }

        #[test]
        fn delete_then_update_then_insert_and_an_edited_deleted_row_is_only_deleted() {
            let edits = cells(&[(0, 1, Some("a2")), (0, 2, Some("x2")), (1, 1, Some("gone")), (2, 1, Some("c2"))]);
            let deleted: BTreeSet<usize> = [1].into();
            let got = edit_statements(PG, &users(), &rows(), &edits, &deleted, &[new_row(&[(1, Some("n"))]), new_row(&[])]).unwrap();
            assert_eq!(
                got,
                [
                    "DELETE FROM public.users WHERE id = '2'",
                    "UPDATE public.users SET name = 'a2', memo = 'x2' WHERE id = '1'",
                    "UPDATE public.users SET name = 'c2' WHERE id = '3'",
                    "INSERT INTO public.users (name) VALUES ('n')",
                    "INSERT INTO public.users DEFAULT VALUES",
                ]
            );
        }

        #[test]
        fn no_key_is_an_error_for_updates_and_deletes_only() {
            let mut to = users();
            to.pk = s(&["id", "tenant"]); // a key column the result doesn't have
            let rows = rows();
            assert_eq!(edit_statements(PG, &to, &rows, &cells(&[(0, 1, Some("z"))]), &none(), &[]), Err(EDIT_NO_KEY.into()));
            assert_eq!(edit_statements(PG, &to, &rows, &cells(&[]), &[0].into(), &[]), Err(EDIT_NO_KEY.into()));
            to.pk = vec![];
            assert_eq!(edit_statements(MY, &to, &rows, &cells(&[(0, 1, Some("z"))]), &none(), &[]), Err(EDIT_NO_KEY.into()));
            assert_eq!(edit_statements(MY, &to, &rows, &cells(&[]), &none(), &[new_row(&[])]), Ok(vec!["INSERT INTO public.users () VALUES ()".to_string()]));
        }

        #[test]
        fn names_and_values_quoted_per_kind() {
            let to = EditTarget { schema: "S".into(), table: "My T".into(), pk: s(&["Id"]), columns: s(&["Id", "select"]) };
            let rows = vec![some(&["it's", "a"])];
            let edits = cells(&[(0, 1, Some("x\\y'z")), (0, 0, Some(crate::extract::DEFAULT_TOKEN))]);
            let got = |k| edit_statements(k, &to, &rows, &edits, &none(), &[]).unwrap();
            assert_eq!(got(PG), [r#"UPDATE "S"."My T" SET "Id" = DEFAULT, "select" = E'x\\y''z' WHERE "Id" = 'it''s'"#]);
            assert_eq!(got(MY), [r"UPDATE S.`My T` SET Id = DEFAULT, `select` = 'x\\y''z' WHERE Id = 'it''s'"]);
            assert_eq!(got(SQLITE), [r#"UPDATE "S"."My T" SET "Id" = DEFAULT, "select" = 'x\y''z' WHERE "Id" = 'it''s'"#]);
        }

        #[test]
        fn join_edits_one_update_per_table_by_its_key() {
            // voc.idx, voc.status, users.idx, users.role, a read-only expression
            let j = JoinEdit {
                tables: vec![JoinTable { schema: "crm".into(), table: "voc".into(), pk: s(&["idx"]), pk_at: vec![0] }, JoinTable { schema: "public".into(), table: "users".into(), pk: s(&["idx"]), pk_at: vec![2] }],
                columns: vec![to(0, "idx"), to(0, "status"), to(1, "idx"), to(1, "role"), ro("why".into())],
            };
            let rows = vec![some(&["10", "open", "7", "admin", "e"]), vec![Some("11".into()), Some("new".into()), None, None, None]];
            let edits = cells(&[(0, 1, Some("closed")), (0, 3, None), (0, 4, Some("ignored")), (1, 1, Some("done")), (1, 3, Some("outer side"))]);
            assert_eq!(
                join_statements(MY, &j, &rows, &edits),
                ["UPDATE crm.voc SET status = 'closed' WHERE idx = '10'", "UPDATE public.users SET role = NULL WHERE idx = '7'", "UPDATE crm.voc SET status = 'done' WHERE idx = '11'"]
            );
        }
    }
}
