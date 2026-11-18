//! User parameters in console SQL (#173): `:name` in code, asked for before a run and put in as typed (no quoting, as DataGrip).
//! Only code is read, with the server's rules for its kind (sql.rs), so strings, quoted names, comments and dollar quotes are
//! never scanned (ClickHouse `\'` and `# `, SQL Server / SQLite `[names]` included). Oracle triggers' `:new.x` / `:old.x` stay.
//! Both apps substitute before
//! any check (read-only, Production, dangerous statements), so those see the final SQL. MCP doesn't: an AI writes its SQL itself.
use crate::DbKind;
use crate::scope::{K, lex};
use std::collections::BTreeMap;

/// One `:name` in the text: `start..end` is the byte range from the `:` to the end of the name
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// Every `:name` in code, in text order. Not `x::int`, `{p:UInt8}`, `a[1:3]`, `:=`, `@v`. None for a console that isn't SQL
pub fn find_params(sql: &str, kind: DbKind) -> Vec<Param> {
    if matches!(kind, DbKind::Redis | DbKind::Mongodb) || kind.is_vector() {
        return vec![];
    }
    let name_byte = |c: char| crate::complete::is_word(c) || c == '$';
    // Non-code blanked first: scope::lex only knows PostgreSQL and MySQL quoting. Offsets are the same in both texts
    let code = crate::sql::blank_non_code(sql, kind);
    let sql = code.as_str();
    let toks = lex(sql, kind == DbKind::Mysql);
    let mut out = vec![];
    for w in toks.windows(2) {
        let (colon, name) = (w[0], w[1]);
        if colon.k != K::Punct || &sql[colon.s..colon.e] != ":" || name.k != K::Word || name.s != colon.e {
            continue;
        }
        // Lexed words start with a letter, `_` or another non-digit name character; digits are numbers (a[1:3])
        let glued = sql[..colon.s].chars().next_back().is_some_and(|c| name_byte(c) || c == ':');
        // `=` or one `:` after the name (`:a:=`, `:a:b`); `::` is a cast of the value (`:id::int`)
        let after = &sql[name.e..];
        let assigns = after.starts_with('=') || (after.starts_with(':') && !after.starts_with("::"));
        // Oracle trigger bodies: `:new.id`, `:old.id` are the row, not something to ask for
        let row = kind == DbKind::Oracle && after.starts_with('.') && ["new", "old"].iter().any(|r| sql[name.s..name.e].eq_ignore_ascii_case(r));
        if !glued && !assigns && !row {
            out.push(Param { name: sql[name.s..name.e].to_string(), start: colon.s, end: name.e });
        }
    }
    out
}

/// The parameters' names in order of first appearance, each once: what a run asks for
pub fn names(sql: &str, kind: DbKind) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for p in find_params(sql, kind) {
        if !out.contains(&p.name) {
            out.push(p.name);
        }
    }
    out
}

/// `sql` with each `:name` replaced by its value as typed (the user writes the quotes); a name without a value stays as is
pub fn substitute(sql: &str, kind: DbKind, values: &BTreeMap<String, String>) -> String {
    let (mut out, mut at) = (String::with_capacity(sql.len()), 0);
    for p in find_params(sql, kind) {
        if let Some(v) = values.get(&p.name) {
            out.push_str(&sql[at..p.start]);
            out.push_str(v);
            at = p.end;
        }
    }
    out.push_str(&sql[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: DbKind = DbKind::Postgres;
    const MY: DbKind = DbKind::Mysql;

    fn found(sql: &str, kind: DbKind) -> Vec<&str> {
        find_params(sql, kind).into_iter().map(|p| &sql[p.start..p.end]).collect()
    }

    #[test]
    fn finds_a_name_with_its_range() {
        let sql = "select * from t where id = :id";
        assert_eq!(find_params(sql, PG), vec![Param { name: "id".into(), start: 27, end: 30 }]);
        assert_eq!(found("where a = (:a) and b in (:b,:c)", PG), [":a", ":b", ":c"]);
    }

    #[test]
    fn postgres_casts_are_not_parameters() {
        assert!(found("select x::int, a::text::int, '1'::jsonb", PG).is_empty());
        assert_eq!(found("select :v::int", PG), [":v"]);
        assert!(found("select x:::a", PG).is_empty());
    }

    #[test]
    fn assignment_is_not_a_parameter() {
        assert!(found("do $$ begin x := 1; end $$", PG).is_empty());
        assert!(found("select @v := 1", MY).is_empty());
        assert!(found("select a :b:= 1, :c=1", MY).is_empty());
    }

    #[test]
    fn strings_are_not_scanned() {
        assert!(found("select ':a', E':a\\' :b', 'it''s :c'", PG).is_empty());
        assert!(found("select ':a', 'x\\' :b'", MY).is_empty());
    }

    #[test]
    fn quoted_names_are_not_scanned() {
        assert!(found(r#"select "a:b", "x :y" from t"#, PG).is_empty());
    }

    #[test]
    fn comments_are_not_scanned() {
        assert!(found("select 1 -- :a\n/* :b */ /* x /* :c */ :d */", PG).is_empty());
        assert!(found("select 1 # :a\n-- :b\n/* :c */", MY).is_empty());
        assert_eq!(found("select 1 -- :a\n, :b", PG), [":b"]);
    }

    #[test]
    fn dollar_quotes_are_not_scanned() {
        assert!(found("do $$ begin perform :a; end $$", PG).is_empty());
        assert!(found("create function f() returns int as $fn$ select :a $fn$ language sql", PG).is_empty());
        assert_eq!(found("select $1, :a", PG), [":a"]);
    }

    #[test]
    fn mysql_variables_are_not_parameters() {
        assert!(found("select @v, @@sess, @@session.sql_mode", MY).is_empty());
        assert_eq!(found("select @v, :a", MY), [":a"]);
    }

    #[test]
    fn mysql_backticks_and_double_quoted_strings_are_not_scanned() {
        assert!(found(r#"select `a:b`, "x :y" from t"#, MY).is_empty());
        assert_eq!(found(r#"select `a`, ":x", :y"#, MY), [":y"]);
    }

    #[test]
    fn clickhouse_query_parameters_are_not_user_parameters() {
        assert!(found("select {p:UInt8}, {name:String}", DbKind::Clickhouse).is_empty());
    }

    #[test]
    fn clickhouse_backslash_strings_and_hash_comments_are_not_scanned() {
        const CH: DbKind = DbKind::Clickhouse;
        assert!(found(r"select 'it\'s :a', 1", CH).is_empty());
        assert!(found("select 1 # :a\n, 2", CH).is_empty());
        assert_eq!(found(r"select 'x\'', :b", CH), [":b"]);
    }

    #[test]
    fn bracket_names_are_not_scanned() {
        for kind in [DbKind::Mssql, DbKind::Sqlite] {
            assert!(found("select [x :y] from t", kind).is_empty(), "{kind:?}");
            assert_eq!(found("select [a]] :b], :c", kind), [":c"], "{kind:?}");
        }
    }

    #[test]
    fn oracle_trigger_rows_are_not_parameters() {
        const OR: DbKind = DbKind::Oracle;
        assert!(found("create trigger t before insert on x for each row begin :new.id := :OLD.id; end;", OR).is_empty());
        assert_eq!(found("select :new, :old_id from t where a = :b", OR), [":new", ":old_id", ":b"]);
        assert_eq!(found("select :new.id", PG), [":new"]);
    }

    #[test]
    fn non_ascii_strings_keep_offsets() {
        let sql = "select '이름 :x', :id";
        assert_eq!(find_params(sql, PG), vec![Param { name: "id".into(), start: 20, end: 23 }]);
    }

    #[test]
    fn array_slice_is_not_a_parameter() {
        assert!(found("select a[1:3], a[:3]", PG).is_empty());
        assert!(found("select a[lo:hi]", PG).is_empty());
    }

    #[test]
    fn unicode_names() {
        assert_eq!(names("select :이름, :na_me2", PG), ["이름", "na_me2"]);
    }

    #[test]
    fn same_name_twice_is_asked_once() {
        let sql = "select :a, :b where x = :a";
        assert_eq!(found(sql, PG), [":a", ":b", ":a"]);
        assert_eq!(names(sql, PG), ["a", "b"]);
    }

    #[test]
    fn substitutes_names_across_statements() {
        let values = BTreeMap::from([("id".to_string(), "42".to_string()), ("who".to_string(), "'ann'".to_string())]);
        assert_eq!(
            substitute("select * from t where id = :id and n = :who; delete from t where id = :id and s = ':id'", PG, &values),
            "select * from t where id = 42 and n = 'ann'; delete from t where id = 42 and s = ':id'"
        );
    }

    #[test]
    fn a_name_without_a_value_is_kept() {
        let values = BTreeMap::from([("a".to_string(), "1".to_string())]);
        assert_eq!(substitute("select :a, :b, x::int", PG, &values), "select 1, :b, x::int");
    }

    #[test]
    fn consoles_that_are_not_sql_have_none() {
        for kind in [DbKind::Redis, DbKind::Mongodb, DbKind::Qdrant, DbKind::Chroma, DbKind::Milvus] {
            assert!(find_params("GET :a", kind).is_empty(), "{kind:?}");
        }
        for kind in [DbKind::Sqlite, DbKind::Mssql, DbKind::Duckdb, DbKind::Oracle, DbKind::Clickhouse] {
            assert_eq!(found("select :a", kind), [":a"], "{kind:?}");
        }
    }
}
