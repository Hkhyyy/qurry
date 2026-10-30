/** Result table → chart data (drawn by ResultChart). Same X values are merged, COUNT is the row count */
import type { Cell, QueryResult } from "./api";

export type ChartKind = "bar" | "line" | "pie";
/** What can be picked as Y: a column index, or the row count */
export const COUNT = -1;

export interface ChartData {
  labels: string[];
  series: { name: string; values: number[] }[];
}

const NUM_TYPE = /^(int|int2|int4|int8|integer|smallint|bigint|tinyint|mediumint|numeric|decimal|float|float4|float8|double|real|money|year)|unsigned/i;
const TIME_TYPE = /date|time/i;

export const toNumber = (c: Cell): number | null => {
  if (c === null || c === "" || typeof c === "boolean") return null;
  const n = typeof c === "number" ? c : Number(String(c).replace(/,/g, ""));
  return Number.isFinite(n) ? n : null;
};

/** Columns that can be treated as numeric: numeric type, or most non-empty values are numbers */
export function numericColumns(r: QueryResult): number[] {
  return r.columns
    .map((c, i) => {
      if (NUM_TYPE.test(c.typeName)) return i;
      const vals = r.rows.slice(0, 200).map((row) => row[i]).filter((v) => v !== null && v !== "");
      return vals.length > 0 && vals.filter((v) => typeof v !== "boolean" && toNumber(v) !== null).length / vals.length >= 0.9 && !TIME_TYPE.test(c.typeName) ? i : -1;
    })
    .filter((i) => i >= 0);
}

const ID_NAME = /^id$|_id$|^id_/i;

/** Initial axes: X = first non-numeric column (else the first column), Y = first numeric column other than X (id-like ones last, else the row count) */
export function defaultAxes(r: QueryResult): { x: number; ys: number[] } {
  const nums = numericColumns(r);
  const x = r.columns.findIndex((_, i) => !nums.includes(i));
  const xi = x >= 0 ? x : 0;
  const cands = nums.filter((i) => i !== xi);
  const y = cands.find((i) => !ID_NAME.test(r.columns[i].name)) ?? cands[0];
  return { x: xi, ys: [y ?? COUNT] };
}

const label = (c: Cell) => (c === null ? "(null)" : String(c));

/**
 * Aggregate Y per X (sum when the same X spans rows, COUNT is the row count). Date·numeric X ascending, others in first-seen order.
 * Pie: the largest maxSlices-1 + the rest as "Other"
 */
export function buildChart(r: QueryResult, x: number, ys: number[], kind: ChartKind, maxSlices = 10): ChartData {
  const order: string[] = [];
  const sums = new Map<string, number[]>();
  for (const row of r.rows) {
    const k = label(row[x]);
    let acc = sums.get(k);
    if (!acc) {
      acc = ys.map(() => 0);
      sums.set(k, acc);
      order.push(k);
    }
    ys.forEach((y, j) => {
      acc![j] += y === COUNT ? 1 : (toNumber(row[y]) ?? 0);
    });
  }
  const xType = r.columns[x]?.typeName ?? "";
  if (TIME_TYPE.test(xType)) order.sort();
  else if (numericColumns(r).includes(x)) order.sort((a, b) => (toNumber(a) ?? 0) - (toNumber(b) ?? 0));
  const names = ys.map((y) => (y === COUNT ? "count" : r.columns[y]?.name ?? "?"));
  if (kind === "pie") {
    const items = order.map((k) => ({ k, v: sums.get(k)![0] })).filter((i) => i.v > 0).sort((a, b) => b.v - a.v);
    const top = items.length > maxSlices ? items.slice(0, maxSlices - 1) : items;
    const rest = items.slice(top.length).reduce((s, i) => s + i.v, 0);
    return { labels: [...top.map((i) => i.k), ...(rest > 0 ? ["Other"] : [])], series: [{ name: names[0], values: [...top.map((i) => i.v), ...(rest > 0 ? [rest] : [])] }] };
  }
  return { labels: order, series: names.map((name, j) => ({ name, values: order.map((k) => sums.get(k)![j]) })) };
}

/** Nice ticks: 1·2·5 × 10ⁿ steps covering min..max */
export function niceTicks(min: number, max: number, count = 5): number[] {
  if (min === max) {
    max = min + 1;
    min = Math.min(0, min);
  }
  const span = max - min;
  const raw = span / Math.max(1, count);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const lo = Math.floor(min / step) * step;
  const hi = Math.ceil(max / step) * step;
  const out: number[] = [];
  for (let v = lo; v <= hi + step / 2; v += step) out.push(Number(v.toPrecision(12)));
  return out;
}

/** Axis labels: 1,200 · 1.2M · 0.25 */
export function shortNumber(n: number): string {
  const a = Math.abs(n);
  if (a >= 1e9) return `${+(n / 1e9).toFixed(1)}B`;
  if (a >= 1e6) return `${+(n / 1e6).toFixed(1)}M`;
  if (a >= 1e4) return `${+(n / 1e3).toFixed(1)}K`;
  return Number.isInteger(n) ? n.toLocaleString("en-US") : String(+n.toFixed(2));
}
