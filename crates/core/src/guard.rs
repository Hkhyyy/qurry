//! Statement checks (read-only, risky statements, MCP execute): the code is qurry_shared::guard, re-exported here so every caller
//! keeps using qurry_core::guard. Only impact_count stays in core, since it reads a QueryResult.
//! The GUI's WASM copy of the checks is for UI decisions only; enforcement (read-only Data Sources, MCP) runs these in the backend.
pub use qurry_shared::guard::*;

/// The number in the first cell of the count query's result (drivers such as MySQL may return it as text)
pub fn impact_count(r: &crate::db::QueryResult) -> Option<i64> {
    r.rows.first().and_then(|x| x.first()).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DbKind;

    /// The count comes back as a number (PostgreSQL) or as text (MySQL and some drivers)
    #[test]
    fn impact_count_reads_numbers_and_text() {
        let r = |v: serde_json::Value| crate::db::QueryResult { columns: vec![], rows: vec![vec![v]], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None };
        assert_eq!(impact_count(&r(12408.into())), Some(12408));
        assert_eq!(impact_count(&r("12408".into())), Some(12408));
        assert_eq!(impact_count(&r(serde_json::Value::Null)), None);
        assert_eq!(impact_count(&crate::db::QueryResult { rows: vec![], ..r(1.into()) }), None);
    }

    /// Statements is_read must not call reads: under a server setting that changes where strings end, they delete.
    /// QURRY_TEST_MY=host:port:user:password:db · QURRY_TEST_PG=host:port:user:db (each uses its own stmt_kind_exec database / schema)
    #[tokio::test]
    #[ignore]
    async fn backslash_reads_that_write() {
        use sqlx::Connection;
        let my = "with a as (select 'x\\') delete from stmt_kind_exec.t -- ') select 1";
        let pg = "with a as (select 'x\\' as c, '), d as (delete from stmt_kind_exec.t returning 1) select 1 -- ' as e) select 1";
        // ANSI_QUOTES: \ escapes in '…' but not in "…" (a name)
        let ansi = r#"with a as (select 'p\' "  ' as "a\") delete from stmt_kind_exec.t -- ""#;
        assert!(!is_read(my, DbKind::Mysql) && !is_read(pg, DbKind::Postgres) && !is_read(ansi, DbKind::Mysql));
        let fixture = "create table stmt_kind_exec.t (a int); insert into stmt_kind_exec.t values (1), (2), (3)";
        let count = "select count(*) from stmt_kind_exec.t";
        if let Ok(e) = std::env::var("QURRY_TEST_MY") {
            let p: Vec<&str> = e.split(':').collect();
            let mut c = sqlx::MySqlConnection::connect(&format!("mysql://{}:{}@{}:{}/{}", p[2], p[3], p[0], p[1], p[4])).await.unwrap();
            for (mode, sql) in [("NO_BACKSLASH_ESCAPES", my), ("ANSI_QUOTES", ansi)] {
                sqlx::raw_sql("set session sql_mode = default; drop database if exists stmt_kind_exec; create database stmt_kind_exec").execute(&mut c).await.unwrap();
                sqlx::raw_sql(fixture).execute(&mut c).await.unwrap();
                sqlx::raw_sql(&format!("set session sql_mode = '{mode}'")).execute(&mut c).await.unwrap();
                sqlx::raw_sql(sql).execute(&mut c).await.unwrap();
                let (n,): (i64,) = sqlx::query_as(count).fetch_one(&mut c).await.unwrap();
                sqlx::raw_sql("drop database stmt_kind_exec").execute(&mut c).await.unwrap();
                assert_eq!(n, 0, "MySQL ran the DELETE under {mode}");
            }
        }
        if let Ok(e) = std::env::var("QURRY_TEST_PG") {
            let p: Vec<&str> = e.split(':').collect();
            let mut c = sqlx::PgConnection::connect(&format!("postgres://{}@{}:{}/{}", p[2], p[0], p[1], p[3])).await.unwrap();
            sqlx::raw_sql("drop schema if exists stmt_kind_exec cascade; create schema stmt_kind_exec").execute(&mut c).await.unwrap();
            sqlx::raw_sql(fixture).execute(&mut c).await.unwrap();
            sqlx::raw_sql("set standard_conforming_strings = off").execute(&mut c).await.unwrap();
            sqlx::raw_sql(pg).execute(&mut c).await.unwrap();
            let (n,): (i64,) = sqlx::query_as(count).fetch_one(&mut c).await.unwrap();
            sqlx::raw_sql("drop schema stmt_kind_exec cascade").execute(&mut c).await.unwrap();
            assert_eq!(n, 0, "PostgreSQL ran the DELETE");
        }
    }

    /// SELECTs that change data on the server are not reads: EXPLAIN ANALYZE of SELECT … INTO creates the table, and nextval /
    /// lo_create persist although the statement is a SELECT. QURRY_TEST_PG=host:port:user:db (schema stmt_kind_exec_into)
    #[tokio::test]
    #[ignore]
    async fn select_reads_that_write() {
        use sqlx::Connection;
        let into = "select * into stmt_kind_exec_into.n from stmt_kind_exec_into.t";
        let seq = "select nextval('stmt_kind_exec_into.s')";
        let lo = "select lo_create(0)";
        assert!(!is_read(into, DbKind::Postgres) && !is_read(seq, DbKind::Postgres) && !is_read(lo, DbKind::Postgres));
        let Ok(e) = std::env::var("QURRY_TEST_PG") else { return };
        let p: Vec<&str> = e.split(':').collect();
        let mut c = sqlx::PgConnection::connect(&format!("postgres://{}@{}:{}/{}", p[2], p[0], p[1], p[3])).await.unwrap();
        sqlx::raw_sql("drop schema if exists stmt_kind_exec_into cascade; create schema stmt_kind_exec_into; create table stmt_kind_exec_into.t (a int); create sequence stmt_kind_exec_into.s")
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::raw_sql(&format!("explain analyze {into}")).execute(&mut c).await.unwrap();
        let (made,): (bool,) = sqlx::query_as("select to_regclass('stmt_kind_exec_into.n') is not null").fetch_one(&mut c).await.unwrap();
        sqlx::raw_sql(seq).execute(&mut c).await.unwrap();
        let (called,): (bool,) = sqlx::query_as("select is_called from stmt_kind_exec_into.s").fetch_one(&mut c).await.unwrap();
        let (oid,): (sqlx::postgres::types::Oid,) = sqlx::query_as(lo).fetch_one(&mut c).await.unwrap();
        let (kept,): (bool,) = sqlx::query_as("select exists (select 1 from pg_largeobject_metadata where oid = $1)").bind(oid).fetch_one(&mut c).await.unwrap();
        sqlx::raw_sql(&format!("select lo_unlink({}); drop schema stmt_kind_exec_into cascade", oid.0)).execute(&mut c).await.unwrap();
        assert!(made, "EXPLAIN ANALYZE of SELECT … INTO created no table");
        assert!(called && kept, "nextval / lo_create changed nothing");
    }

    /// Core's guard, sql and DbKind are the shared items, not copies: two glob imports of the same name only compile
    /// (instead of failing as ambiguous, E0659) when both paths name the same item
    #[test]
    fn guard_sql_and_kind_are_the_shared_items() {
        // rustc reports the second import of an item as unused: that is the point
        #[allow(unused_imports)]
        mod both {
            pub use crate::guard::*;
            pub use crate::sql::*;
            pub use crate::store::*;
            pub use qurry_shared::guard::*;
            pub use qurry_shared::kind::*;
            pub use qurry_shared::sql::*;
        }
        let read_only_violation: fn(&str, DbKind, bool) -> Option<String> = both::read_only_violation;
        let kind: both::DbKind = DbKind::Postgres;
        assert!(read_only_violation("delete from t", kind, true).is_some());
        assert_eq!(both::split_statements("select 1; select 2", kind).len(), 2);
        assert!(both::unsafe_statement("drop table t", kind).is_some());
    }
}
