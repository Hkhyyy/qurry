/** 결과 표 → 차트 데이터 (ResultChart 가 그린다). 같은 X 는 합치고, COUNT 는 행 수 */
import type { Cell, QueryResult } from "./api";

export type ChartKind = "bar" | "line" | "pie";
/** Y 로 고를 수 있는 것: 컬럼 번호, 또는 행 수 */
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

/** 수로 볼 수 있는 컬럼: 타입이 수이거나, 비어 있지 않은 값의 대부분이 수 */
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

/** 처음 고를 축: X = 수가 아닌 첫 컬럼 (없으면 첫 컬럼), Y = X 가 아닌 첫 수 컬럼 (id 같은 것은 뒤로, 없으면 행 수) */
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
 * X 마다 Y 를 합친다 (같은 X 가 여러 행이면 합, COUNT 는 행 수). 날짜·수 X 는 오름차순, 나머지는 처음 나온 순서.
 * 파이는 큰 것부터 maxSlices-1 개 + 나머지는 "Other"
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

/** 보기 좋은 눈금: min..max 를 덮는 1·2·5 × 10ⁿ 간격 */
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

/** 축 글: 1,200 · 1.2M · 0.25 */
export function shortNumber(n: number): string {
  const a = Math.abs(n);
  if (a >= 1e9) return `${+(n / 1e9).toFixed(1)}B`;
  if (a >= 1e6) return `${+(n / 1e6).toFixed(1)}M`;
  if (a >= 1e4) return `${+(n / 1e3).toFixed(1)}K`;
  return Number.isInteger(n) ? n.toLocaleString("en-US") : String(+n.toFixed(2));
}
