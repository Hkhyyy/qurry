import { describe, expect, it } from "vitest";
import { compareRanked, type Ranked } from "./rank";

const r = (name: string, source: string, kind = "table", where = "public", db = "shop"): Ranked => ({ name, kind, where, source, db });
const prod = [r("payment", "[PROD] shop"), r("payment_refund", "[PROD] shop"), r("payment", "[PROD] shop", "view", "report")];
const mysql = [r("payment", "[MySQL] shop", "table", "shop"), r("payment_method", "[MySQL] shop", "table", "shop")];
const names = (xs: Ranked[]) => xs.map((x) => `${x.name} ${x.kind} ${x.where} ${x.source}`);

describe("compareRanked", () => {
  it("the same order whichever data source answers first", () => {
    const a = [...prod, ...mysql].sort(compareRanked);
    const b = [...mysql, ...prod].sort(compareRanked);
    expect(names(a)).toEqual(names(b));
    expect(names(a)).toEqual([
      "payment table public [PROD] shop",
      "payment table shop [MySQL] shop",
      "payment view report [PROD] shop",
      "payment_method table shop [MySQL] shop",
      "payment_refund table public [PROD] shop",
    ]);
  });
  it("then by data source and database", () => {
    expect(compareRanked(r("t", "A"), r("t", "B"))).toBeLessThan(0);
    expect(compareRanked(r("t", "A", "table", "public", "x"), r("t", "A", "table", "public", "a"))).toBeGreaterThan(0);
    expect(compareRanked(r("t", "A"), r("t", "A"))).toBe(0);
  });
});
