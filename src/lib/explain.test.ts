import { describe, expect, it } from "vitest";
import { explainSql } from "./explain";

describe("explainSql (qurry_shared::explain through WASM)", () => {
  it("builds each kind's statement, without the trailing ;", () => {
    expect(explainSql("select 1;", "postgres", false, true)).toEqual({ sql: "EXPLAIN (FORMAT JSON) select 1" });
    expect(explainSql("select 1", "postgres", true, true)).toEqual({ sql: "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select 1" });
    expect(explainSql("select 1", "postgres", false, false)).toEqual({ sql: "EXPLAIN (FORMAT TEXT) select 1" });
    expect(explainSql("select 1;", "mysql", true, true)).toEqual({ sql: "EXPLAIN ANALYZE select 1" });
    expect(explainSql("select 1", "sqlite", false, true)).toEqual({ sql: "EXPLAIN QUERY PLAN select 1" });
    expect(explainSql("select 1", "duckdb", true, true)).toEqual({ sql: "EXPLAIN ANALYZE select 1" });
    expect(explainSql("select 1", "clickhouse", false, true)).toEqual({ sql: "EXPLAIN select 1" });
  });

  it("says why a kind has none", () => {
    expect(explainSql("select 1", "sqlite", true, true)).toEqual({ err: "SQLite has no Explain that runs the statement: use Explain Plan" });
    expect(explainSql("GET k", "redis", false, true)).toEqual({ err: "Explain isn't available for Redis: run the statement without Explain" });
    expect(explainSql("select 1", "mssql", false, true)).toEqual({ err: "Explain isn't available for SQL Server: run the statement without Explain" });
  });

  it("puts the same text before every statement", () => {
    const pre = explainSql("", "postgres", true, true);
    expect(pre).toEqual({ sql: "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) " });
  });
});
