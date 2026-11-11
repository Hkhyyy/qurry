import { describe, expect, it, vi } from "vitest";
import { DEFAULT_PORT, defaultDbOf, KIND_LABEL, redisDb, type DbKind } from "./api";
import { shared } from "./shared";

const KINDS = Object.keys(KIND_LABEL) as DbKind[];

// The database a target without "::db" connects to: qurry_shared::kind::default_db through WASM (core, the TUI and MCP use the same)
describe("defaultDbOf", () => {
  it("follows the shared rule per kind", () => {
    expect(defaultDbOf({ kind: "postgres", database: "" })).toBe("postgres");
    expect(defaultDbOf({ kind: "postgres", database: "shop" })).toBe("shop");
    expect(defaultDbOf({ kind: "mysql", database: "" })).toBe("");
    expect(defaultDbOf({ kind: "mysql", database: "shop" })).toBe("shop");
    expect(defaultDbOf({ kind: "redis", database: "0" })).toBe("db0");
    expect(defaultDbOf({ kind: "redis", database: "" })).toBe("db0");
    expect(defaultDbOf({ kind: "redis", database: "3" })).toBe("db3");
    expect(defaultDbOf({ kind: "sqlite", database: "/tmp/a.db" })).toBe("main");
    expect(defaultDbOf({ kind: "duckdb", database: "" })).toBe("main");
    expect(defaultDbOf({ kind: "mssql", database: "app" })).toBe("app");
    expect(defaultDbOf({ kind: "qdrant", database: "" })).toBe("qdrant");
  });

  it("before the module has loaded: the PostgreSQL rule, without throwing", async () => {
    vi.resetModules();
    vi.doMock("./shared", () => ({ sharedOr: <T,>(fallback: T) => fallback }));
    const { defaultDbOf: early } = await import("./api");
    expect(early({ kind: "postgres", database: "" })).toBe("postgres");
    expect(early({ kind: "mysql", database: "" })).toBe("");
    expect(early({ kind: "redis", database: "0" })).toBe("0");
    vi.doUnmock("./shared");
  });
});

// A Redis database name from a tree node, a tab or the Database field: the same rule as defaultDbOf
describe("redisDb", () => {
  it("reads a name or a number as core does", () => {
    for (const [d, want] of [["", "db0"], ["3", "db3"], ["db3", "db3"], [" 3 ", "db3"], ["x", "db0"], [null, "db0"]] as const) expect([d, redisDb(d)]).toEqual([d, want]);
  });
});

// The connection dialog reads DEFAULT_PORT synchronously; it must stay the table of qurry_shared::kind::DbKind::default_port
describe("DEFAULT_PORT", () => {
  it("equals the shared default port for every kind", () => {
    expect(Object.keys(DEFAULT_PORT).sort()).toEqual([...KINDS].sort());
    for (const k of KINDS) expect([k, DEFAULT_PORT[k]]).toEqual([k, shared().defaultPort(k)]);
  });
});
