import { describe, expect, it } from "vitest";
import { hintable, hintContext, hintInsert, pairsForKeys } from "./valueHint";

describe("hintContext", () => {
  it("finds column values after a comparison", () => {
    expect(hintContext("select * from orders where status = '")).toMatchObject({ column: "status", mode: "values", quoted: true, typed: "" });
    expect(hintContext("where o.status = 'pa")).toMatchObject({ ref: "o", column: "status", quoted: true, typed: "pa" });
    expect(hintContext("where status <> ")).toMatchObject({ column: "status", quoted: false, typed: "" });
    expect(hintContext("where code = 12")).toMatchObject({ column: "code", quoted: false, typed: "12" });
    expect(hintContext("where status in ('paid', '")).toMatchObject({ column: "status", quoted: true, typed: "" });
    expect(hintContext("where status not in (1, ")).toMatchObject({ column: "status", quoted: false });
    expect(hintContext("where name ILIKE 'ki")).toMatchObject({ column: "name", typed: "ki" });
    expect(hintContext('where "Status" = \'')).toMatchObject({ column: "Status" });
  });
  it("finds json keys and values", () => {
    expect(hintContext("where data->>'")).toMatchObject({ column: "data", path: [], mode: "keys", typed: "" });
    expect(hintContext("where t.data -> 'meta' ->> 'sr")).toMatchObject({ ref: "t", column: "data", path: ["meta"], mode: "keys", typed: "sr" });
    expect(hintContext("where data->>'kind' = '")).toMatchObject({ column: "data", path: ["kind"], mode: "values", quoted: true });
    expect(hintContext("where data->'items'->0->>'")).toMatchObject({ path: ["items", "0"], mode: "keys" });
  });
  it("finds json keys before the quote and @> conditions", () => {
    expect(hintContext("select * from order_risk_assessment where reasons ->> ")).toMatchObject({ column: "reasons", mode: "keys", quoted: false, typed: "", op: " ->> " });
    expect(hintContext("where reasons->>co")).toMatchObject({ column: "reasons", mode: "keys", quoted: false, typed: "co", op: "->>co" });
    expect(hintContext("where reasons @> '")).toMatchObject({ column: "reasons", mode: "pairs", quoted: true, typed: "" });
    expect(hintContext("where reasons @> ")).toMatchObject({ mode: "pairs", quoted: false });
    expect(hintContext("where reasons @> '", true)).toBeNull();
  });
  it("reads MySQL json paths", () => {
    expect(hintContext("where data->>'$.", true)).toMatchObject({ column: "data", path: [], mode: "keys", typed: "" });
    expect(hintContext("where data->>'$.meta.s", true)).toMatchObject({ path: ["meta"], mode: "keys", typed: "s" });
    expect(hintContext("where data->>'$.kind' = '", true)).toMatchObject({ column: "data", path: ["kind"], mode: "values", quoted: true });
    expect(hintContext("where data->>'$.", false)).toBeNull();
  });
  it("ignores other places", () => {
    expect(hintContext("select * from orders where ")).toBeNull();
    expect(hintContext("select 'abc")).toBeNull();
    expect(hintContext("where status = 'paid' and ")).toBeNull();
  });
});

describe("hintInsert / hintable", () => {
  const ctx = (quoted: boolean, path: string[] = []) => ({ column: "c", path, mode: "values" as const, quoted, typed: "" });
  it("quotes as needed", () => {
    expect(hintInsert("it's", ctx(true), "text")).toBe("it''s");
    expect(hintInsert("paid", ctx(false), "varchar")).toBe("'paid'");
    expect(hintInsert("12", ctx(false), "int4")).toBe("12");
    expect(hintInsert("12", ctx(false, ["n"]), "jsonb")).toBe("'12'");
    expect(hintInsert('[{"code": "x"}]', { ...ctx(false), mode: "pairs" }, "jsonb")).toBe(`'[{"code": "x"}]'`);
    expect(pairsForKeys(`[{"code": "it's"}]`, { ...ctx(false), mode: "keys", op: " ->> " })).toBe(` @> '[{"code": "it''s"}]'`);
  });
  it("skips binary and whole json", () => {
    expect(hintable(ctx(true), "bytea")).toBe(false);
    expect(hintable(ctx(true), "jsonb")).toBe(false);
    expect(hintable(ctx(true, ["a"]), "jsonb")).toBe(true);
    expect(hintable({ ...ctx(true), mode: "keys" }, "jsonb")).toBe(true);
    expect(hintable({ ...ctx(true), mode: "pairs" }, "jsonb")).toBe(true);
    expect(hintable({ ...ctx(true), mode: "keys" }, "text")).toBe(false);
  });
});
