import { describe, expect, it, vi } from "vitest";
import { askedStatements, explainAnalyzeAsk, explainAnalyzeNote, impactLine, isDdl, isRead, unsafeReason } from "./stmtKind";

describe("stmtKind (core guard through WASM)", () => {
  it("reads the statement as the TUI does (D1)", () => {
    expect(isRead("postgres", "select 'delete' as action")).toBe(true);
    expect(isRead("postgres", "(select 1) union (select 2)")).toBe(true);
    expect(isRead("mysql", "select 1 # then delete")).toBe(true);
    expect(isRead("postgres", 'select "update" from t')).toBe(false);
    expect(isRead("postgres", "with d as (delete from t returning *) select * from d")).toBe(false);
    expect(isDdl("mysql", "# wipe\ntruncate t")).toBe(true);
    expect(isDdl("postgres", "# wipe\ntruncate t")).toBe(false);
  });

  it("unsafeReason gives the core guard's reason for a dangerous statement, and nothing for a safe one (qurry_shared::guard::unsafe_statement)", () => {
    expect(unsafeReason("postgres", "delete from t")).toMatch(/WHERE/);
    expect(unsafeReason("mysql", "drop table t")).toBeTruthy();
    expect(unsafeReason("redis", "FLUSHALL")).toMatch(/FLUSHALL/);
    expect(unsafeReason("postgres", "delete from t where id = 1")).toBeUndefined();
    expect(unsafeReason("postgres", "select 1")).toBeUndefined();
    expect(unsafeReason("redis", "GET a")).toBeUndefined();
  });

  it("SELECT … INTO and sequence writers change data", () => {
    expect(isRead("postgres", "select * into newtab from t")).toBe(false);
    expect(isRead("mysql", "select * from t into outfile '/tmp/x'")).toBe(false);
    expect(isRead("postgres", "select nextval('s')")).toBe(false);
  });

  it("text with several statements is a read only if every one is", () => {
    expect(isRead("postgres", "select 1; do $$ begin delete from orders; end $$")).toBe(false);
    expect(isRead("mysql", "select 1; prepare s from 'delete from orders'; execute s")).toBe(false);
    expect(isRead("postgres", "select 1; select 2;")).toBe(true);
  });

  it("Explain Analyse asks with the TUI's words: what a rollback can't undo, a DELETE without WHERE; not a function it doesn't know", () => {
    expect(explainAnalyzeAsk("postgres", "delete from t where id = 1")).toBe(undefined);
    expect(explainAnalyzeAsk("postgres", "delete from t")).toBe("DELETE without WHERE changes every row, holding their locks until the rollback undoes it");
    expect(explainAnalyzeAsk("postgres", "update t set a = f(1) where id = 1")).toBe(undefined);
    expect(explainAnalyzeAsk("postgres", "select nextval('s')")).toMatch(/^a rollback can't undo what it does outside the transaction/);
    expect(explainAnalyzeAsk("postgres", "select dblink_exec('dbname=x', 'delete from t')")).toMatch(/^a rollback can't undo/);
    expect(explainAnalyzeAsk("mysql", "select * from t into outfile '/tmp/x'")).toMatch(/^a rollback can't undo/);
    expect(explainAnalyzeAsk("postgres", "select archive_old_orders()")).toBe(undefined);
    expect(explainAnalyzeAsk("postgres", "select count(*) from t")).toBe(undefined);
    expect(explainAnalyzeAsk("sqlite", "delete from t where id = 1")).toBe("it changes data");
    expect(explainAnalyzeNote("postgres", "select 1")).toBe(undefined);
    expect(explainAnalyzeNote("postgres", "update t set a = 1")).toEqual({ level: "info", text: expect.stringContaining("its table changes were undone") });
    expect(explainAnalyzeNote("postgres", "select archive()")).toBe(undefined);
    expect(explainAnalyzeNote("postgres", "select nextval('s')")).toEqual({ level: "warn", text: expect.stringContaining("what it did outside the transaction stays") });
  });

  it("with a backslash in a string, a read must be a read under both of the server's string rules", () => {
    // MySQL NO_BACKSLASH_ESCAPES / PostgreSQL standard_conforming_strings = off end the string early and run the DELETE
    expect(isRead("mysql", "with a as (select 'x\\') delete from t -- ') select 1")).toBe(false);
    expect(isRead("postgres", "with a as (select 'x\\' as c, '), d as (delete from t returning 1) select 1 -- ' as e) select 1")).toBe(false);
    expect(isRead("mysql", "select 'C:\\path', 'delete' as action")).toBe(true);
  });

  it("the Production confirmation asks about the writes only, in the TUI's words and thresholds", () => {
    expect(askedStatements("mysql", ["select 1 # then delete", "update t set a = 1", "select 'drop' as x"], false)).toEqual(["update t set a = 1"]);
    expect(askedStatements("postgres", ["delete from t"], true)).toEqual([]);
    expect(impactLine("postgres", "delete from t", { Ok: 0 })).toEqual({ value: "0 rows", tone: undefined });
    expect(impactLine("postgres", "delete from t", { Ok: 1 })).toEqual({ value: "1 row", tone: "warn" });
    expect(impactLine("postgres", "delete from t", { Ok: 99 })).toEqual({ value: "99 rows", tone: "warn" });
    expect(impactLine("postgres", "delete from t", { Ok: 12408 })).toEqual({ value: "12,408 rows", tone: "danger" });
    expect(impactLine("postgres", "create table x (a int)", { Ok: null })).toEqual({ value: "schema change", tone: "danger" });
    expect(impactLine("postgres", "insert into t values (1)", { Ok: null })).toEqual({ value: "can't count", tone: "danger" });
    expect(impactLine("postgres", "delete from t", { Err: "Error: refused\nmore" })).toEqual({ value: "can't count: Error: refused", tone: "danger" });
  });

  it("without the WASM module every statement is a write that may be DDL (Production asks, the explorer reloads)", async () => {
    vi.resetModules();
    const fresh = await import("./stmtKind");
    expect(fresh.isRead("postgres", "select 1")).toBe(false);
    expect(fresh.isDdl("postgres", "drop table t")).toBe(null);
    expect(fresh.explainAnalyzeAsk("postgres", "delete from t")).toBe("it can't be checked without the statement splitter");
    expect(fresh.explainAnalyzeNote("postgres", "select 1")?.text).toMatch(/couldn't be checked/);
    expect(fresh.askedStatements("postgres", ["select 1", "delete from t"], false)).toEqual(["select 1", "delete from t"]);
    expect(fresh.impactLine("postgres", "create table x (a int)", { Ok: null })).toEqual({ value: "can't count · may change the schema", tone: "danger" });
    // A count keeps its tone by number: 0 normal, 1~99 warning, 100+ danger
    expect(fresh.impactLine("postgres", "delete from t", { Ok: 0 })).toEqual({ value: "0 rows", tone: undefined });
    expect(fresh.impactLine("postgres", "delete from t", { Ok: 5 })).toEqual({ value: "5 rows", tone: "warn" });
    expect(fresh.impactLine("postgres", "delete from t", { Ok: 12408 })).toEqual({ value: "12,408 rows", tone: "danger" });
  });
});
