// What more than one file of the result grid (components/ResultGrid.tsx and its hooks here) uses
import type { EditTarget } from "../../components/ResultGrid";
import type { Cell, FullValues, QueryResult } from "../../lib/api";
import { DEFAULT_TOKEN, raw } from "../../lib/extract";

export type SortKey = { col: number; dir: "asc" | "desc" };

export function display(c: Cell): string {
  if (c === null) return "<null>";
  if (c === DEFAULT_TOKEN) return "<default>";
  return String(c);
}

/** Cells whose original value is not shown as is (arrays, bytea, etc.) are not editable. */
export const editableValue = (v: Cell, typeName: string) => typeName !== "BYTEA" && !(typeof v === "string" && /^<[A-Z0-9_ \[\]]+>$/.test(v));

// ─── Cut values (Settings › Appearance › Data grid › Long values): QueryResult.cut, read whole through api.fetchValues ───

/** A cell's key in a result's set of cut cells (QueryResult.cut, by data row) */
export const cellKey = (r: number, c: number) => `${r},${c}`;
export const cutSet = (cut?: [number, number][]) => new Set((cut ?? []).map(([r, c]) => cellKey(r, c)));
/** The characters a cut cell holds (its value is the cut text) */
export const cutLength = (v: Cell) => [...String(v ?? "")].length;
/** A cut cell's hover text; n = cutLength */
export const cutTitle = (n: number) => `Cut at ${n} characters — the Value Editor (⇧↩) shows the whole value`;

/** The run each result object came from: a load-more and fetched values make new objects of the same run, so an answer is dropped
 *  only when a run replaced the result it was asked for */
const runs = new WeakMap<QueryResult, QueryResult>();
export const ofRun = (next: QueryResult, prev: QueryResult) => (runs.set(next, runs.get(prev) ?? prev), next);
export const sameRun = (a?: QueryResult | null, b?: QueryResult | null) => !!a && !!b && (runs.get(a) ?? a) === (runs.get(b) ?? b);

/** The result with a load-more's rows after its own: their cut cells move down by the rows already loaded */
export const appendRows = (x: QueryResult, more: QueryResult): QueryResult =>
  ofRun({ ...x, rows: [...x.rows, ...more.rows], truncated: more.truncated, cut: [...(x.cut ?? []), ...(more.cut ?? []).map(([r, c]): [number, number] => [r + x.rows.length, c])] }, x);

/** The result column of each key column of `edit` (-1: not in the result) */
export const keyColumns = (edit: EditTarget | undefined, cols: QueryResult["columns"]) =>
  edit ? edit.pk.map((k) => (edit.columns ? edit.columns.indexOf(k) : cols.findIndex((c) => c.name === k))) : [];

/** Row r's cut cells can be read whole: one table's result with its key, none of the row's key cells cut */
export const keyed = (pkIdx: number[], cut: Set<string>, r: number) => pkIdx.length > 0 && pkIdx.every((k) => k >= 0 && !cut.has(cellKey(r, k)));

/** fetchValues' request for cut `cells` of keyed rows: one entry per row (its key values as loaded), the columns of all */
export function fullRequest(result: QueryResult, edit: EditTarget, cells: [number, number][]) {
  const pkIdx = keyColumns(edit, result.columns);
  const rows = [...new Set(cells.map(([r]) => r))];
  const cols = [...new Set(cells.map(([, c]) => c))];
  const req: FullValues = {
    schema: edit.schema,
    table: edit.table,
    key: edit.pk,
    columns: cols.map((c) => edit.columns?.[c] ?? result.columns[c].name),
    rows: rows.map((r) => pkIdx.map((k) => (result.rows[r][k] === null ? null : raw(result.rows[r][k])))),
  };
  return { rows, cols, req };
}

/** fetchValues' answer written into the result: each asked cell of a row found gets its whole value and leaves `cut`. The row arrays
 *  are written in place (a new result object of the same run and the same first row), so the grid keeps its edits and selection.
 *  missing = rows not found (their cells stay cut) */
export function applyFull(result: QueryResult, cells: [number, number][], rows: number[], cols: number[], got: (unknown[] | null)[]) {
  const asked = new Set(cells.map(([r, c]) => cellKey(r, c)));
  const done = new Set<string>();
  rows.forEach((r, i) =>
    cols.forEach((c, j) => {
      const k = cellKey(r, c);
      if (!got[i] || !asked.has(k) || !result.rows[r]) return;
      result.rows[r][c] = got[i]![j] as Cell;
      done.add(k);
    }),
  );
  const cut = (result.cut ?? []).filter(([r, c]) => !done.has(cellKey(r, c)));
  return { result: ofRun({ ...result, rows: [...result.rows], cut }, result), missing: got.filter((x) => !x).length };
}
