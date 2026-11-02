/** Result table → chart data (drawn by ResultChart): crates/shared/src/chart.rs through the shared WASM, with the GUI's options */
import type { QueryResult } from "./api";
import { shared, type ChartData } from "./shared";

export type { ChartData };
export type ChartKind = "bar" | "line" | "pie";
/** What can be picked as Y: a column index, or the row count */
export const COUNT = -1;

/** Columns that can be treated as numeric: numeric type (YEAR too), or most non-empty values are numbers */
export const numericColumns = (r: QueryResult): number[] => shared().numericColumns(r.columns, r.rows);

/** Initial axes: X = first non-numeric column (else the first column), Y = first numeric column other than X (id-like ones last, else the row count) */
export function defaultAxes(r: QueryResult): { x: number; ys: number[] } {
  const a = shared().defaultAxes(r.columns, r.rows);
  return { x: a.x, ys: [a.y ?? COUNT] };
}

/**
 * Aggregate Y per X (sum when the same X spans rows, COUNT is the row count). Date·numeric X ascending, others in first-seen order.
 * Pie: the largest maxSlices-1 + the rest as "Other"
 */
export function buildChart(r: QueryResult, x: number, ys: number[], kind: ChartKind, maxSlices = 10): ChartData {
  const data = shared().aggregate(r.columns, r.rows, x, ys);
  return kind === "pie" ? shared().pie(data, maxSlices) : data;
}

/** Nice ticks: 1·2·5 × 10ⁿ steps covering min..max */
export const niceTicks = (min: number, max: number, count = 5): number[] => shared().niceTicks(min, max, count);

/** Axis labels: 1,200 · 1.2M · 0.25 */
export const shortNumber = (n: number): string => shared().shortNumber(n);
