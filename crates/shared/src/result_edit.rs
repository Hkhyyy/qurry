//! Whether a console result can be edited the way a table tab is: a plain SELECT from one table whose every result column is a
//! column of that table, with the table's whole primary key in the result. The server says where each column comes from
//! (qurry_core::editable); the statement check and the rules live here, so the reasons read the same in the GUI and the TUI.
//!
//! The statement check matters even where the server reports column origins: a self-join (`t a JOIN t b`) gives columns of one
//! table but of two different rows, and editing such a row by its key would write to the wrong one.
use crate::errors as e;
use crate::guard::{Tok, scan};
use crate::kind::DbKind;
use serde::Serialize;

/// Where an editable console result writes. Result column i is the table's column `columns[i]`; `pk` is the table's primary key
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTarget {
    pub schema: String,
    pub table: String,
    pub pk: Vec<String>,
    pub columns: Vec<String>,
}

/// Flattened into the result: `edit` or `readOnlyReason`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Editable {
    #[serde(rename = "edit")]
    Table(EditTarget),
    #[serde(rename = "readOnlyReason")]
    ReadOnly(String),
}

/// A select-list item as written
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `*` or `t.*`
    All,
    /// A bare or qualified column name (quotes stripped), and its alias
    Column { name: String, alias: Option<String> },
    /// Anything else: an expression, a literal, a function call
    Other,
}

/// A plain SELECT from one table
#[derive(Debug, Clone, PartialEq)]
pub struct OneTable {
    pub schema: Option<String>,
    pub table: String,
    pub items: Vec<Item>,
}

/// What follows the table (and its alias) in a plain SELECT
const AFTER_TABLE: &[&str] = &["where", "order", "limit", "offset", "fetch", "for", "lock"];
const JOINS: &[&str] = &["join", "inner", "left", "right", "full", "cross", "natural", "straight_join"];
/// Words that end the FROM item, so they aren't its alias
const NOT_ALIAS: &[&str] = &[
    "where", "order", "limit", "offset", "fetch", "for", "lock", "join", "inner", "left", "right", "full", "cross", "natural", "straight_join", "group", "having", "union",
    "intersect", "except", "window", "tablesample", "partition", "use", "force", "ignore", "on", "using",
];
/// Bare words that are values, not columns (MySQL reads `current_date` as the function even when a column has that name)
const VALUE_WORDS: &[&str] = &[
    "null", "true", "false", "unknown", "default", "current_date", "current_time", "current_timestamp", "current_user", "current_role", "localtime", "localtimestamp", "utc_date",
    "utc_time", "utc_timestamp",
];

/// The table and select list of a plain `SELECT … FROM one_table [alias] [WHERE …] [ORDER BY …] [LIMIT …] [FOR UPDATE]`, or why
/// the statement isn't one (errors::EDIT_*). Subqueries inside WHERE are fine: they don't add result columns
pub fn one_table_select(sql: &str, kind: DbKind) -> Result<OneTable, String> {
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
            "join" | "straight_join" => return Err(e::EDIT_MANY_TABLES.into()),
            "into" => return Err(e::EDIT_NOT_PLAIN.into()),
            // FOR UPDATE / SHARE / NO KEY UPDATE / KEY SHARE, MySQL LOCK IN SHARE MODE: in Auto mode the cursor's transaction keeps
            // the row locks while the result is open, and the edits, saved on another connection, would wait on them
            "for" | "lock" => return Err(e::EDIT_LOCKS.into()),
            "distinct" | "distinctrow" if i == 1 => return Err(e::edit_combined("DISTINCT")),
            _ => {}
        }
    }
    let from = (1..t.len()).find(|&i| kw(i, "from")).ok_or_else(|| e::EDIT_NO_TABLE.to_string())?;
    // The table: name or schema.name, not a subquery, a function or ONLY / LATERAL
    let mut i = from + 1;
    let mut parts = Vec::new();
    loop {
        let Some(x) = t.get(i).filter(|x| x.word && x.depth == 0 && !(kw(i, "only") || kw(i, "lateral"))) else { return Err(e::EDIT_NOT_PLAIN.into()) };
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
    if kw(i, "as") {
        i += 2;
    } else if t.get(i).is_some_and(|x| x.word && (x.quoted || !NOT_ALIAS.contains(&x.text.as_str()))) {
        i += 1;
    }
    if let Some(x) = t.get(i) {
        if x.text == "," || JOINS.iter().any(|w| kw(i, w)) {
            return Err(e::EDIT_MANY_TABLES.into());
        }
        if !AFTER_TABLE.iter().any(|w| kw(i, w)) {
            return Err(e::EDIT_NOT_PLAIN.into());
        }
    }
    let items = t[1..from].split(|x| x.text == "," && x.depth == 0).map(|it| item(it, sql, kind)).collect();
    Ok(OneTable { schema, table, items })
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
    // name(.name)*, or (name.)* followed by *
    let mut i = 0;
    loop {
        if is(i, "*") {
            return if i + 1 == it.len() { Item::All } else { Item::Other };
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
    let alias_at = if is(i, "as") { i + 1 } else { i };
    match it.get(alias_at) {
        None if alias_at == i => Item::Column { name: col, alias: None },
        Some(a) if alias_at + 1 == it.len() && (a.word || a.text == "'") => Item::Column { name: col, alias: Some(name(a, sql, kind)) },
        _ => Item::Other,
    }
}

/// The table a result comes from, as the catalog has it
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
            Item::All => out.extend(t.columns.iter().map(|c| (Some(c.clone()), Some(c.as_str())))),
            Item::Column { name, alias } => {
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

/// The rules, once the table and the table column of each result column (None: computed) are known
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
        return ro(e::edit_missing_pk(&t.pk));
    }
    Editable::Table(EditTarget { schema: t.schema.clone(), table: t.table.clone(), pk: t.pk.clone(), columns: cols })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: DbKind = DbKind::Postgres;
    const MY: DbKind = DbKind::Mysql;

    fn col(n: &str) -> Item {
        Item::Column { name: n.into(), alias: None }
    }

    #[test]
    fn plain_selects_from_one_table() {
        let ok = |q: &str, k: DbKind| one_table_select(q, k).unwrap_or_else(|e| panic!("{q}: {e}"));
        let r = ok("select * from users", PG);
        assert_eq!(r, OneTable { schema: None, table: "users".into(), items: vec![Item::All] });
        let r = ok("SELECT u.id, u.\"Name\" AS n, email e FROM Public.Users u WHERE id IN (SELECT 1 UNION SELECT 2) ORDER BY 1 LIMIT 5;", PG);
        assert_eq!(r.schema.as_deref(), Some("public"));
        assert_eq!(r.table, "users");
        assert_eq!(r.items, vec![col("id"), Item::Column { name: "Name".into(), alias: Some("n".into()) }, Item::Column { name: "email".into(), alias: Some("e".into()) }]);
        let r = ok("select `Id`, t.* , shop.t.name from shop.`My T` as t limit 10", MY);
        assert_eq!((r.schema.as_deref(), r.table.as_str()), (Some("shop"), "My T"));
        assert_eq!(r.items, vec![col("Id"), Item::All, col("name")]);
        // -- comment and /* */ before it, a trailing comment after the ;
        ok("-- top\n/* x */ select id from t; -- done", PG);
        // Expressions are items of their own (the origins or the table decide later)
        let r = ok("select id, lower(name), 1, id + 1 as x, id::text, current_date, \"s\", null, count(*) from t", MY);
        assert_eq!(r.items, vec![col("id"), Item::Other, Item::Other, Item::Other, Item::Other, Item::Other, Item::Other, Item::Other, Item::Other]);
        // A string alias (MySQL)
        assert_eq!(ok("select id 'the id' from t", MY).items, vec![Item::Column { name: "id".into(), alias: Some("the id".into()) }]);
        // IS DISTINCT FROM in WHERE isn't DISTINCT
        ok("select id from t where a is distinct from b", PG);
    }

    #[test]
    fn refuses_what_isnt_one_table() {
        let why = |q: &str, k: DbKind| one_table_select(q, k).expect_err(q);
        for q in ["select a.id from t a join t b on a.p = b.id", "select * from t, u", "select * from t a, u b", "select * from t left join u using (id)", "select * from t natural join u", "select * from t cross join u"] {
            assert_eq!(why(q, PG), e::EDIT_MANY_TABLES, "{q}");
        }
        assert_eq!(why("select * from t straight_join u", MY), e::EDIT_MANY_TABLES);
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
            "select * from generate_series(1, 3) g",
            "select * from only t",
            "select * into t2 from t",
            "select id from t; select id from u",
            "table t",
            "values (1)",
            "update t set a = 1",
            "select * from db.s.t",
            "select * from t tablesample system (10)",
            "select row_number() over w, id from t window w as (order by id)",
        ] {
            assert_eq!(why(q, PG), e::EDIT_NOT_PLAIN, "{q}");
        }
        // MySQL executable comments, partitions
        assert_eq!(why("select /*! sql_no_cache */ id from t", MY), e::EDIT_NOT_PLAIN);
        assert_eq!(why("select id from t partition (p0)", MY), e::EDIT_NOT_PLAIN);
        // Words inside strings or quotes don't count
        one_table_select("select id, \"group\" from t where note = 'a join b union c'", PG).unwrap();
        // Locking reads: in Auto mode the cursor's transaction holds the locks while edits save on another connection
        for q in ["select * from t for update", "select * from t where id = 1 for share", "select * from t for no key update", "select * from t for key share", "select * from t a for update of a nowait"] {
            assert_eq!(why(q, PG), e::EDIT_LOCKS, "{q}");
        }
        for q in ["select * from t lock in share mode", "select * from t where id > 0 for share", "select * from t for update skip locked"] {
            assert_eq!(why(q, MY), e::EDIT_LOCKS, "{q}");
        }
    }

    fn table(cols: &[&str], pk: &[&str]) -> TableInfo {
        TableInfo { schema: "public".into(), table: "users".into(), not_table: None, columns: cols.iter().map(|c| c.to_string()).collect(), pk: pk.iter().map(|c| c.to_string()).collect() }
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
        assert_eq!(ro(&["name"], some(&["name"]), &t), e::edit_missing_pk(&s(&["id"])));
        assert_eq!(ro(&["id", "upper"], vec![Some("id".into()), None], &t), e::edit_computed("upper"));
        assert_eq!(ro(&["id", "id"], some(&["id", "id"]), &t), e::edit_duplicate("id"));
        assert_eq!(ro(&["id", "name", "n2"], some(&["id", "name", "name"]), &t), e::edit_duplicate("name"));
        assert_eq!(ro(&["id"], some(&["id"]), &table(&["id"], &[])), e::edit_no_pk("public.users"));
        let v = TableInfo { not_table: Some("view"), ..t.clone() };
        assert_eq!(ro(&["id"], some(&["id"]), &v), e::edit_not_table("public.users", "view"));
        // A composite key needs all its columns
        let c = table(&["a", "b", "x"], &["a", "b"]);
        assert_eq!(ro(&["a", "x"], some(&["a", "x"]), &c), e::edit_missing_pk(&s(&["a", "b"])));
        assert!(matches!(decide(&c, &s(&["b", "x", "a"]), &some(&["b", "x", "a"])), Editable::Table(_)));
    }

    #[test]
    fn mysql_lines_items_up_with_the_table() {
        let t = table(&["id", "Name", "email"], &["id"]);
        let q = |sql: &str| one_table_select(sql, MY).unwrap().items;
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
        let col_n = Item::Column { name: "n".into(), alias: Some("zzz".into()) };
        assert_eq!(q("select id, N'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, n'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, _utf8mb4'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, _utf8mb4 'zzz' from users"), vec![col("id"), Item::Other]);
        assert_eq!(q("select id, n 'zzz' from users"), vec![col("id"), col_n]);
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
    }
}
