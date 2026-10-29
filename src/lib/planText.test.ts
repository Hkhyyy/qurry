import { describe, expect, it } from "vitest";
import { planText } from "./planText";

const node = (op: string, extra: object, children: any[] = []) => ({ op, detail: "", pct: 0, warnings: [], props: [], children, ...extra });

describe("planText", () => {
  it("indents children like EXPLAIN and keeps shares and warnings", () => {
    const plan = node("Sort", { startup: 100, total: 110, rows: 10, pct: 12 }, [
      node("Seq Scan", { detail: "on public.orders  filter: (status = 'paid')", startup: 0, total: 99.5, rows: 10, pct: 88, warnings: ["Seq Scan over 1,000,000 rows"] }),
    ]);
    expect(planText(plan as any)).toBe(
      ["Sort (cost=100..110 rows=10)  [12%]", "->  Seq Scan on public.orders  filter: (status = 'paid') (cost=0..99.50 rows=10)  [88%]", "      ⚠ Seq Scan over 1,000,000 rows"].join("\n"),
    );
  });
  it("shows actual times after ANALYZE", () => {
    expect(planText(node("Result", { total: 0.01, startup: 0, rows: 1, actualTime: 0.004, actualRows: 1, loops: 1, pct: 100 }) as any)).toBe("Result (cost=0..0.01 rows=1) (actual time=0.004 ms rows=1 loops=1)  [100%]");
  });
});
