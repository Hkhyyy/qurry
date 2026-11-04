import { describe, expect, it } from "vitest";
import { SCOPE_ALL, SCOPE_DEFAULT } from "./api";
import { toggleSchema } from "./schemaScope";

const listed = ["pg_catalog", "public", "billing"];

// Manage Shown Schemas: what one click on a schema line stores for that database
describe("toggleSchema", () => {
  it("All on + uncheck a schema: All turns off, the other listed schemas stay checked, Default schema kept", () => {
    expect(toggleSchema([SCOPE_ALL], "billing", listed)).toEqual(["pg_catalog", "public"]);
    expect(toggleSchema([SCOPE_ALL, SCOPE_DEFAULT], "pg_catalog", listed)).toEqual([SCOPE_DEFAULT, "public", "billing"]);
    // a name stored next to All isn't listed twice
    expect(toggleSchema([SCOPE_ALL, "public"], "billing", listed)).toEqual(["pg_catalog", "public"]);
  });
  it("checking the last unchecked schema turns All back on (not the long list)", () => {
    expect(toggleSchema(["pg_catalog", "public"], "billing", listed)).toEqual([SCOPE_ALL]);
    // public counts as checked through Default schema
    expect(toggleSchema([SCOPE_DEFAULT, "billing"], "pg_catalog", listed)).toEqual([SCOPE_DEFAULT, SCOPE_ALL]);
  });
  it("otherwise a schema just flips", () => {
    expect(toggleSchema(["public"], "billing", listed)).toEqual(["public", "billing"]);
    expect(toggleSchema(["public", "billing"], "billing", listed)).toEqual(["public"]);
  });
  it("All schemas and Default schema flip as before", () => {
    expect(toggleSchema([SCOPE_ALL], SCOPE_ALL, listed)).toEqual([]);
    expect(toggleSchema(["public"], SCOPE_ALL, listed)).toEqual(["public", SCOPE_ALL]);
    expect(toggleSchema([SCOPE_ALL], SCOPE_DEFAULT, listed)).toEqual([SCOPE_ALL, SCOPE_DEFAULT]);
  });
});
