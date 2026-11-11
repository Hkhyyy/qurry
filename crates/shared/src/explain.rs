//! The EXPLAIN statement for a kind: one builder for the GUI console, the TUI console and the MCP explain tool.
use crate::DbKind;

/// The statement that shows `sql`'s plan on `kind`; `analyze` runs it (callers roll it back where they can).
/// PostgreSQL: `json` picks FORMAT JSON (the consoles draw its tree) over FORMAT TEXT (MCP); other kinds ignore it.
/// SQLite: EXPLAIN QUERY PLAN (plain EXPLAIN is the bytecode), with no form that runs it. A trailing `;` goes, once.
/// Err: the words to show when the kind has no such form (nothing should be sent)
pub fn explain_sql(sql: &str, kind: DbKind, analyze: bool, json: bool) -> Result<String, String> {
    let body = sql.trim_end();
    let body = body.strip_suffix(';').unwrap_or(body);
    let prefix = match (kind, analyze) {
        (DbKind::Postgres, false) => format!("EXPLAIN (FORMAT {}) ", if json { "JSON" } else { "TEXT" }),
        (DbKind::Postgres, true) => format!("EXPLAIN (FORMAT {}, ANALYZE, BUFFERS) ", if json { "JSON" } else { "TEXT" }),
        (DbKind::Mysql | DbKind::Duckdb, false) | (DbKind::Clickhouse, false) => "EXPLAIN ".into(),
        (DbKind::Mysql | DbKind::Duckdb, true) => "EXPLAIN ANALYZE ".into(),
        (DbKind::Sqlite, false) => "EXPLAIN QUERY PLAN ".into(),
        (DbKind::Sqlite | DbKind::Clickhouse, true) => return Err(format!("{} has no Explain that runs the statement: use Explain without Analyze", kind.label())),
        (DbKind::Redis | DbKind::Mssql | DbKind::Oracle | DbKind::Mongodb | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus, _) => {
            return Err(format!("Explain isn't available for {}: run the statement without Explain", kind.label()));
        }
    };
    Ok(prefix + body)
}

/// How many bytes explain_sql puts before the statement (an error position in what was sent, minus this, is one in the statement)
pub fn explain_prefix_len(kind: DbKind, analyze: bool, json: bool) -> usize {
    explain_sql("", kind, analyze, json).map_or(0, |s| s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use DbKind::*;

    #[test]
    fn postgres_json_or_text_with_or_without_analyze() {
        assert_eq!(explain_sql("select 1", Postgres, false, true).unwrap(), "EXPLAIN (FORMAT JSON) select 1");
        assert_eq!(explain_sql("select 1", Postgres, true, true).unwrap(), "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select 1");
        assert_eq!(explain_sql("select 1", Postgres, false, false).unwrap(), "EXPLAIN (FORMAT TEXT) select 1");
        assert_eq!(explain_sql("select 1", Postgres, true, false).unwrap(), "EXPLAIN (FORMAT TEXT, ANALYZE, BUFFERS) select 1");
    }

    #[test]
    fn mysql_duckdb_clickhouse() {
        assert_eq!(explain_sql("select 1", Mysql, false, true).unwrap(), "EXPLAIN select 1");
        assert_eq!(explain_sql("select 1", Mysql, true, true).unwrap(), "EXPLAIN ANALYZE select 1");
        assert_eq!(explain_sql("select 1", Duckdb, false, true).unwrap(), "EXPLAIN select 1");
        assert_eq!(explain_sql("select 1", Duckdb, true, false).unwrap(), "EXPLAIN ANALYZE select 1");
        assert_eq!(explain_sql("select 1", Clickhouse, false, true).unwrap(), "EXPLAIN select 1");
        assert_eq!(explain_sql("select 1", Clickhouse, true, true).unwrap_err(), "ClickHouse has no Explain that runs the statement: use Explain without Analyze");
    }

    #[test]
    fn sqlite_is_the_query_plan_and_has_no_analyze() {
        assert_eq!(explain_sql("select 1", Sqlite, false, true).unwrap(), "EXPLAIN QUERY PLAN select 1");
        assert_eq!(explain_sql("select 1", Sqlite, true, true).unwrap_err(), "SQLite has no Explain that runs the statement: use Explain without Analyze");
    }

    #[test]
    fn kinds_without_explain_say_so() {
        for (k, name) in [(Redis, "Redis"), (Mssql, "SQL Server"), (Oracle, "Oracle"), (Mongodb, "MongoDB"), (Qdrant, "Qdrant"), (Chroma, "Chroma"), (Milvus, "Milvus")] {
            for analyze in [false, true] {
                assert_eq!(explain_sql("select 1", k, analyze, true).unwrap_err(), format!("Explain isn't available for {name}: run the statement without Explain"));
            }
        }
    }

    #[test]
    fn a_trailing_semicolon_goes_once() {
        assert_eq!(explain_sql("select 1;", Mysql, false, true).unwrap(), "EXPLAIN select 1");
        assert_eq!(explain_sql("select 1; \n", Mysql, false, true).unwrap(), "EXPLAIN select 1");
        assert_eq!(explain_sql("select 1;;", Mysql, false, true).unwrap(), "EXPLAIN select 1;");
        assert_eq!(explain_sql("select ';'", Mysql, false, true).unwrap(), "EXPLAIN select ';'");
    }

    #[test]
    fn prefix_len_is_what_comes_before_the_statement() {
        let q = explain_sql("select 1;", Postgres, true, true).unwrap();
        assert_eq!(&q[explain_prefix_len(Postgres, true, true)..], "select 1");
        assert_eq!(explain_prefix_len(Redis, false, true), 0);
    }
}
