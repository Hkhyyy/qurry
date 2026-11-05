import { describe, expect, it } from "vitest";
import { currentFirst, dbShown, defaultDbOf, isSystemDb, type ConnectionView } from "./api";

const conn = (c: Partial<ConnectionView>) => ({ id: "c", name: "c", kind: "postgres", database: "", ...c }) as ConnectionView;
const shown = (c: ConnectionView, dbs: string[]) => dbs.filter((d) => dbShown(c, d, defaultDbOf(c)));

// The databases the explorer and the DBA report's database picker list
describe("dbShown", () => {
  it("PostgreSQL: all, or the shown-database scope; the default is postgres when blank", () => {
    expect(defaultDbOf(conn({}))).toBe("postgres");
    expect(shown(conn({}), ["postgres", "shop", "billing"])).toEqual(["postgres", "shop", "billing"]);
    expect(shown(conn({ database: "shop", scope: { databases: ["billing", "@default"] } }), ["postgres", "shop", "billing"])).toEqual(["shop", "billing"]);
  });
  it("MySQL: system schemas hidden, no default schema when blank", () => {
    const my = conn({ kind: "mysql" });
    expect(defaultDbOf(my)).toBe("");
    expect(shown(my, ["information_schema", "mysql", "shop", "sys"])).toEqual(["shop"]);
    expect(["mysql", "shop"].filter((d) => dbShown(my, d, "", true))).toEqual(["mysql", "shop"]);
  });
  it("one fixed database: only the default", () => {
    expect(shown(conn({ database: "shop", options: { singleDb: true } as ConnectionView["options"] }), ["postgres", "shop", "billing"])).toEqual(["shop"]);
  });
});

// The database / schema pickers (console, Redis browser, Database Admin report): every one of the server (Manage Shown Schemas only filters
// the explorer), the current one first, the rest in the server's order
describe("currentFirst", () => {
  it("current first, then the others once", () => {
    expect(currentFirst("shop", ["postgres", "shop", "billing"])).toEqual(["shop", "postgres", "billing"]);
    expect(currentFirst("gone", ["postgres", "shop"])).toEqual(["gone", "postgres", "shop"]);
    expect(currentFirst("", ["", "mysql", "shop"])).toEqual(["", "mysql", "shop"]);
  });
});

describe("isSystemDb", () => {
  it("MySQL's system schemas; none for PostgreSQL", () => {
    expect(["information_schema", "mysql", "performance_schema", "sys", "shop"].filter((d) => isSystemDb("mysql", d))).toEqual(["information_schema", "mysql", "performance_schema", "sys"]);
    expect(isSystemDb("postgres", "information_schema")).toBe(false);
  });
});
