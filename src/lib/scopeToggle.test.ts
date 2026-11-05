import { describe, expect, it } from "vitest";
import { SCOPE_ALL, SCOPE_DEFAULT } from "./api";
import { toggleScoped } from "./scopeToggle";

const listed = ["pg_catalog", "public", "billing"];
const toggle = (cur: string[], name: string) => toggleScoped(cur, name, listed, "public");

// Manage Shown Schemas: what one click on a schema (or database) line stores
describe("toggleScoped", () => {
  it("All on + uncheck one: All turns off, the other listed ones stay checked, Default kept", () => {
    expect(toggle([SCOPE_ALL], "billing")).toEqual(["pg_catalog", "public"]);
    expect(toggle([SCOPE_ALL, SCOPE_DEFAULT], "pg_catalog")).toEqual([SCOPE_DEFAULT, "public", "billing"]);
    // a name stored next to All isn't listed twice
    expect(toggle([SCOPE_ALL, "public"], "billing")).toEqual(["pg_catalog", "public"]);
  });
  it("checking the last unchecked one turns All back on (not the long list)", () => {
    expect(toggle(["pg_catalog", "public"], "billing")).toEqual([SCOPE_ALL]);
    // the default counts as checked through Default
    expect(toggle([SCOPE_DEFAULT, "billing"], "pg_catalog")).toEqual([SCOPE_DEFAULT, SCOPE_ALL]);
  });
  it("otherwise one just flips", () => {
    expect(toggle(["public"], "billing")).toEqual(["public", "billing"]);
    expect(toggle(["public", "billing"], "billing")).toEqual(["public"]);
  });
  it("All and Default flip as before", () => {
    expect(toggle([SCOPE_ALL], SCOPE_ALL)).toEqual([]);
    expect(toggle(["public"], SCOPE_ALL)).toEqual(["public", SCOPE_ALL]);
    expect(toggle([SCOPE_ALL], SCOPE_DEFAULT)).toEqual([SCOPE_ALL, SCOPE_DEFAULT]);
  });
  it("callers list only user schemas / databases: switching from All leaves system ones out, and all user ones checked is All", () => {
    const user = ["public", "billing", "sales"];
    expect(toggleScoped([SCOPE_ALL], "billing", user, "public")).toEqual(["public", "sales"]);
    expect(toggleScoped(["public", "sales"], "billing", user, "public")).toEqual([SCOPE_ALL]);
  });
  it("databases: Default database stands for the source's default one", () => {
    const dbs = ["postgres", "shop", "billing"];
    expect(toggleScoped([SCOPE_ALL], "billing", dbs, "shop")).toEqual(["postgres", "shop"]);
    expect(toggleScoped([SCOPE_DEFAULT, "postgres"], "billing", dbs, "shop")).toEqual([SCOPE_DEFAULT, SCOPE_ALL]);
  });
});
