import { describe, expect, it } from "vitest";
import type { Cell, QueryResult } from "./api";
import { buildChart, COUNT, defaultAxes, niceTicks, numericColumns, shortNumber } from "./chart";
import resultCorpus from "../../crates/shared/tests/corpus/results.json";
import resultGoldens from "../../crates/shared/tests/goldens/ts/results.json";
import numberGoldens from "../../crates/shared/tests/goldens/ts/numbers.json";

const res = (columns: [string, string][], rows: QueryResult["rows"]): QueryResult => ({
  columns: columns.map(([name, typeName]) => ({ name, typeName })),
  rows,
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 0,
  hasResultSet: true,
});

describe("chart", () => {
  const sales = res(
    [["region", "TEXT"], ["day", "DATE"], ["amount", "NUMERIC"], ["note", "TEXT"]],
    [
      ["Seoul", "2026-10-02", "100.50", "a"],
      ["Busan", "2026-10-01", "40", null],
      ["Seoul", "2026-10-01", "9.5", "b"],
      [null, "2026-10-03", null, "c"],
    ],
  );

  it("finds number columns by type or by values", () => {
    expect(numericColumns(sales)).toEqual([2]);
    // Values are numbers even if the type is TEXT (e.g. tables imported from CSV)
    expect(numericColumns(res([["n", "TEXT"], ["s", "TEXT"]], [["1,200", "x"], ["3", "y"], [null, null]]))).toEqual([0]);
    expect(defaultAxes(sales)).toEqual({ x: 0, ys: [2] });
    expect(defaultAxes(res([["name", "TEXT"]], [["a"]]))).toEqual({ x: 0, ys: [COUNT] });
    // id isn't picked first for Y
    expect(defaultAxes(res([["id", "INT8"], ["user_id", "INT4"], ["name", "TEXT"], ["amount", "NUMERIC"]], [[1, 2, "a", "3"]]))).toEqual({ x: 2, ys: [3] });
  });

  it("sums the same X and counts rows", () => {
    expect(buildChart(sales, 0, [2, COUNT], "bar")).toEqual({
      labels: ["Seoul", "Busan", "(null)"],
      series: [
        { name: "amount", values: [110, 40, 0] },
        { name: "count", values: [2, 1, 1] },
      ],
    });
    // Date X is in time order
    expect(buildChart(sales, 1, [2], "line").labels).toEqual(["2026-10-01", "2026-10-02", "2026-10-03"]);
  });

  it("sends WASM only the X and Y columns, and only 200 rows to tell numeric columns", () => {
    // Numbers in the first 200 rows, text after: still numeric (as before), and numeric X still sorts ascending
    const rows: QueryResult["rows"] = [...Array.from({ length: 200 }, (_, i) => [String(300 - i), "pad", 1]), ["x", "pad", 2]];
    const r = res([["n", "TEXT"], ["note", "TEXT"], ["v", "INT4"]], rows);
    expect(numericColumns(r)).toEqual([0, 2]);
    const data = buildChart(r, 0, [2, COUNT, 2], "bar");
    expect(data.labels.slice(0, 3)).toEqual(["x", "101", "102"]);
    expect(data.series.map((s) => [s.name, s.values[0]])).toEqual([["v", 2], ["count", 1], ["v", 2]]);
    // Axes left from a result with more columns (one render before they are picked again) don't throw
    expect(buildChart(r, 7, [9], "bar")).toEqual({ labels: ["(null)"], series: [{ name: "?", values: [0] }] });
  });

  it("pie keeps the biggest slices and folds the rest into Other", () => {
    const r = res([["k", "TEXT"], ["v", "INT4"]], [["a", 1], ["b", 5], ["c", 3], ["d", 2], ["e", 0]]);
    expect(buildChart(r, 0, [1], "pie", 3)).toEqual({ labels: ["b", "c", "Other"], series: [{ name: "v", values: [5, 3, 3] }] });
  });

  it("nice ticks and short numbers", () => {
    expect(niceTicks(0, 97)).toEqual([0, 20, 40, 60, 80, 100]);
    expect(niceTicks(-3, 12, 4)).toEqual([-5, 0, 5, 10, 15]);
    expect(niceTicks(5, 5)).toEqual([0, 2, 4, 6]);
    expect(shortNumber(1234567)).toBe("1.2M");
    expect(shortNumber(15300)).toBe("15.3K");
    expect(shortNumber(1200)).toBe("1,200");
    expect(shortNumber(0.256)).toBe("0.26");
  });

  // The goldens were written by the TS chart this replaced; cargo test checks them natively, this checks the WASM build the GUI runs
  it("matches the GUI goldens through the WASM module", () => {
    const results = resultGoldens as Record<string, Record<string, unknown>>;
    for (const e of resultCorpus) {
      const r = res(e.columns.map((c) => [c.name, c.typeName]), e.rows as Cell[][]);
      const names = r.columns.map((c) => c.name);
      const axes = defaultAxes(r);
      const y = axes.ys[0];
      const g = results[e.id];
      expect(numericColumns(r).map((i) => names[i]), e.id).toEqual(g.numericColumns);
      expect({ x: names[axes.x] ?? null, y: y === COUNT ? null : names[y] }, e.id).toEqual(g.defaultAxes);
      expect(buildChart(r, axes.x, axes.ys, "bar"), e.id).toEqual(g["buildChart(bar)"]);
      expect(buildChart(r, axes.x, axes.ys, "pie"), e.id).toEqual(g["buildChart(pie)"]);
    }
    for (const [key, g] of Object.entries(numberGoldens as Record<string, { niceTicks?: number[]; shortNumber?: string }>)) {
      if (key.startsWith("ticks:")) {
        const t: number[] = JSON.parse(key.slice(6));
        expect(niceTicks(t[0], t[1], t[2]), key).toEqual(g.niceTicks);
      } else expect(shortNumber(Number(key.slice(6))), key).toEqual(g.shortNumber);
    }
  });
});
