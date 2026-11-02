/** Result table → chart data (drawn by ResultChart): crates/shared/src/chart.rs through the shared WASM, with the GUI's options */
import type { QueryResult } from "./api";
import { shared, type ChartData } from "./shared";

export type { ChartData };
export type ChartKind = "bar" | "line" | "pie";
/** What can be picked as Y: a column index, or the row count */
export const COUNT = -1;
/** Rows chart.rs reads to tell numeric columns by their values. Only these cross into WASM: copying every cell is the slow part */
const SAMPLE = 200;

/** Columns that can be treated as numeric: numeric type (YEAR too), or most non-empty values are numbers */
export const numericColumns = (r: QueryResult): number[] => shared().numericColumns(r.columns, r.rows.slice(0, SAMPLE));

/** Initial axes: X = first non-numeric column (else the first column), Y = first numeric column other than X (id-like ones last, else the row count) */
export function defaultAxes(r: QueryResult): { x: number; ys: number[] } {
  const a = shared().defaultAxes(r.columns, r.rows.slice(0, SAMPLE));
  return { x: a.x, ys: [a.y ?? COUNT] };
}

/**
 * Aggregate Y per X (sum when the same X spans rows, COUNT is the row count). Date·numeric X ascending, others in first-seen order.
 * Pie: the largest maxSlices-1 + the rest as "Other"
 */
export function buildChart(r: QueryResult, x: number, ys: number[], kind: ChartKind, maxSlices = 10): ChartData {
  // Only the X and Y columns cross into WASM (X first, then each Y). A column index past the result (axes of the previous
  // result for one render) becomes an unnamed column of nulls, as chart.rs treats it
  const keep = [x, ...ys.filter((y) => y !== COUNT)];
  const columns = keep.map((i) => r.columns[i] ?? { name: "?", typeName: "" });
  const rows = r.rows.map((row) => keep.map((i) => row[i] ?? null));
  let next = 1;
  const data = shared().aggregate(columns, rows, 0, ys.map((y) => (y === COUNT ? COUNT : next++)));
  return kind === "pie" ? shared().pie(data, maxSlices) : data;
}

/** Nice ticks: 1·2·5 × 10ⁿ steps covering min..max */
export const niceTicks = (min: number, max: number, count = 5): number[] => shared().niceTicks(min, max, count);

/** Axis labels: 1,200 · 1.2M · 0.25 */
export const shortNumber = (n: number): string => shared().shortNumber(n);
