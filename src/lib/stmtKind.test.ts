import { describe, expect, it, vi } from "vitest";
import { isDdl, isRead } from "./stmtKind";

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

  it("without the WASM module every statement is a write and not DDL (Production then asks)", async () => {
    vi.resetModules();
    const fresh = await import("./stmtKind");
    expect(fresh.isRead("postgres", "select 1")).toBe(false);
    expect(fresh.isDdl("postgres", "drop table t")).toBe(false);
  });
});
