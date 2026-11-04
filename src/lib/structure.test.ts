import { afterEach, describe, expect, it, vi } from "vitest";
import { constraintLabel, ddlView, fkSide, hasStructure, noneNote, setDdlView } from "./structure";

afterEach(() => vi.unstubAllGlobals());

describe("DDL view choice", () => {
  it("is SQL until Structure is picked, then remembered", () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
    expect(ddlView()).toBe("sql");
    setDdlView("structure");
    expect(store.get("qurry.ddlView")).toBe("structure");
    expect(ddlView()).toBe("structure");
    store.set("qurry.ddlView", "something old");
    expect(ddlView()).toBe("sql");
  });
  it("falls back to SQL when browser storage is blocked", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(() => setDdlView("structure")).not.toThrow();
    expect(ddlView()).toBe("sql");
  });
});

describe("structure labels", () => {
  it("only tables have a Structure view", () => {
    expect(["table", "partitioned", "view", "matview", "foreign", "sequence", "routine", "key"].filter(hasStructure)).toEqual(["table", "partitioned"]);
  });
  it("names constraint kinds", () => {
    expect((["p", "f", "u", "c"] as const).map(constraintLabel)).toEqual(["Primary key", "Foreign key", "Unique", "Check"]);
  });
  it("writes either side of a foreign key as schema.table (columns)", () => {
    const fk = { name: "f", schema: "public", table: "orders", columns: ["user_id", "shop_id"], refSchema: "public", refTable: "users", refColumns: ["id", "shop_id"], onUpdate: "NO ACTION", onDelete: "CASCADE" };
    expect(fkSide(fk, "from")).toBe("public.orders (user_id, shop_id)");
    expect(fkSide(fk, "to")).toBe("public.users (id, shop_id)");
  });
});

describe("empty Referenced by / Triggers", () => {
  it("is None, or on MySQL only what this account can see", () => {
    expect(noneNote(false)).toBe("None");
    expect(noneNote(true)).toBe("None visible to this account (MySQL lists only what it has privileges on)");
  });
});
