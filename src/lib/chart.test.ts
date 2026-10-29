import { describe, expect, it } from "vitest";
import type { QueryResult } from "./api";
import { buildChart, COUNT, defaultAxes, niceTicks, numericColumns, shortNumber } from "./chart";

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
    // 타입이 TEXT 여도 값이 수면 (CSV 로 넣은 표 등)
    expect(numericColumns(res([["n", "TEXT"], ["s", "TEXT"]], [["1,200", "x"], ["3", "y"], [null, null]]))).toEqual([0]);
    expect(defaultAxes(sales)).toEqual({ x: 0, ys: [2] });
    expect(defaultAxes(res([["name", "TEXT"]], [["a"]]))).toEqual({ x: 0, ys: [COUNT] });
    // id 는 Y 로 먼저 고르지 않는다
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
    // 날짜 X 는 시간순
    expect(buildChart(sales, 1, [2], "line").labels).toEqual(["2026-10-01", "2026-10-02", "2026-10-03"]);
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
});
