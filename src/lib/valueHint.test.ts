import { describe, expect, it } from "vitest";
import { hintable, hintContext, hintInsert } from "./valueHint";

describe("hintContext", () => {
  it("finds column values after a comparison", () => {
    expect(hintContext("select * from orders where status = '")).toMatchObject({ column: "status", keys: false, quoted: true, typed: "" });
    expect(hintContext("where o.status = 'pa")).toMatchObject({ ref: "o", column: "status", quoted: true, typed: "pa" });
    expect(hintContext("where status <> ")).toMatchObject({ column: "status", quoted: false, typed: "" });
    expect(hintContext("where code = 12")).toMatchObject({ column: "code", quoted: false, typed: "12" });
    expect(hintContext("where status in ('paid', '")).toMatchObject({ column: "status", quoted: true, typed: "" });
    expect(hintContext("where status not in (1, ")).toMatchObject({ column: "status", quoted: false });
    expect(hintContext("where name ILIKE 'ki")).toMatchObject({ column: "name", typed: "ki" });
    expect(hintContext('where "Status" = \'')).toMatchObject({ column: "Status" });
  });
  it("finds json keys and values", () => {
    expect(hintContext("where data->>'")).toMatchObject({ column: "data", path: [], keys: true, typed: "" });
    expect(hintContext("where t.data -> 'meta' ->> 'sr")).toMatchObject({ ref: "t", column: "data", path: ["meta"], keys: true, typed: "sr" });
    expect(hintContext("where data->>'kind' = '")).toMatchObject({ column: "data", path: ["kind"], keys: false, quoted: true });
    expect(hintContext("where data->'items'->0->>'")).toMatchObject({ path: ["items", "0"], keys: true });
  });
  it("reads MySQL json paths", () => {
    expect(hintContext("where data->>'$.", true)).toMatchObject({ column: "data", path: [], keys: true, typed: "" });
    expect(hintContext("where data->>'$.meta.s", true)).toMatchObject({ path: ["meta"], keys: true, typed: "s" });
    expect(hintContext("where data->>'$.kind' = '", true)).toMatchObject({ column: "data", path: ["kind"], keys: false, quoted: true });
    expect(hintContext("where data->>'$.", false)).toBeNull();
  });
  it("ignores other places", () => {
    expect(hintContext("select * from orders where ")).toBeNull();
    expect(hintContext("select 'abc")).toBeNull();
    expect(hintContext("where status = 'paid' and ")).toBeNull();
  });
});

describe("hintInsert / hintable", () => {
  const ctx = (quoted: boolean, path: string[] = []) => ({ column: "c", path, keys: false, quoted, typed: "" });
  it("quotes as needed", () => {
    expect(hintInsert("it's", ctx(true), "text")).toBe("it''s");
    expect(hintInsert("paid", ctx(false), "varchar")).toBe("'paid'");
    expect(hintInsert("12", ctx(false), "int4")).toBe("12");
    expect(hintInsert("12", ctx(false, ["n"]), "jsonb")).toBe("'12'");
  });
  it("skips binary and whole json", () => {
    expect(hintable(ctx(true), "bytea")).toBe(false);
    expect(hintable(ctx(true), "jsonb")).toBe(false);
    expect(hintable(ctx(true, ["a"]), "jsonb")).toBe(true);
    expect(hintable({ ...ctx(true), keys: true }, "jsonb")).toBe(true);
    expect(hintable({ ...ctx(true), keys: true }, "text")).toBe(false);
  });
});
