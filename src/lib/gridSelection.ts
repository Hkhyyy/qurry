/**
 * Result grid selection as a list of ranges (DataGrip): a click selects one cell, ⇧-click extends the last range, ⌘-click adds a cell
 * (a row number: a row; a column header: a column) or, when all of it is already selected, takes it out. Coordinates are display
 * positions (r = on-screen row order, c = column).
 */
export type Pos = { r: number; c: number };
/** A rectangle, both ends inclusive */
export type Rect = { r0: number; r1: number; c0: number; c1: number };
/** anchor = where the last range started (⇧-click extends from it); cur = the last clicked cell (the active cell, the Value Editor's) */
export type Selection = { ranges: Rect[]; anchor: Pos; cur: Pos };

export const rectOf = (a: Pos, b: Pos): Rect => ({ r0: Math.min(a.r, b.r), r1: Math.max(a.r, b.r), c0: Math.min(a.c, b.c), c1: Math.max(a.c, b.c) });
const inRect = (x: Rect, r: number, c: number) => r >= x.r0 && r <= x.r1 && c >= x.c0 && c <= x.c1;

/** A plain click (or keyboard move): one range from a to b, everything else dropped */
export const single = (a: Pos, b: Pos = a): Selection => ({ ranges: [rectOf(a, b)], anchor: a, cur: b });

/** ⇧-click / drag: the last range now runs from the anchor to `to`. After ⌘-click took cells out, no range holds the anchor and a new one starts there */
export function extend(sel: Selection | null, to: Pos): Selection {
  if (!sel) return single(to);
  const last = sel.ranges[sel.ranges.length - 1];
  const keep = last && inRect(last, sel.anchor.r, sel.anchor.c) ? sel.ranges.slice(0, -1) : sel.ranges;
  return { ranges: [...keep, rectOf(sel.anchor, to)], anchor: sel.anchor, cur: to };
}

/** `x` minus `hole`: up to four rectangles around it */
function subtract(x: Rect, hole: Rect): Rect[] {
  if (hole.r1 < x.r0 || hole.r0 > x.r1 || hole.c1 < x.c0 || hole.c0 > x.c1) return [x];
  const out: Rect[] = [];
  if (hole.r0 > x.r0) out.push({ ...x, r1: hole.r0 - 1 });
  if (hole.r1 < x.r1) out.push({ ...x, r0: hole.r1 + 1 });
  const r0 = Math.max(x.r0, hole.r0), r1 = Math.min(x.r1, hole.r1);
  if (hole.c0 > x.c0) out.push({ r0, r1, c0: x.c0, c1: hole.c0 - 1 });
  if (hole.c1 < x.c1) out.push({ r0, r1, c0: hole.c1 + 1, c1: x.c1 });
  return out;
}

/** ⌘-click: add the range a..b, or take it out when every cell of it is already selected */
export function toggle(sel: Selection | null, a: Pos, b: Pos = a): Selection {
  if (!sel) return single(a, b);
  const add = rectOf(a, b);
  const covered = sel.ranges.reduce((left, x) => left.flatMap((l) => subtract(l, x)), [add]).length === 0;
  const ranges = covered ? sel.ranges.flatMap((x) => subtract(x, add)) : [...sel.ranges, add];
  return { ranges, anchor: a, cur: b };
}

export const contains = (sel: Selection | null, r: number, c: number) => !!sel && sel.ranges.some((x) => inRect(x, r, c));

/** One cell selected (copy writes its raw value; Copy as IN List takes its whole column) */
export const isSingle = (sel: Selection | null) => !!sel && sel.ranges.length === 1 && sel.ranges[0].r0 === sel.ranges[0].r1 && sel.ranges[0].c0 === sel.ranges[0].c1;

const spanUnion = (sel: Selection | null, lo: "r0" | "c0", hi: "r1" | "c1"): number[] => {
  const s = new Set<number>();
  for (const x of sel?.ranges ?? []) for (let i = x[lo]; i <= x[hi]; i++) s.add(i);
  return [...s].sort((a, b) => a - b);
};
/** Display rows any range touches, in grid order */
export const selRows = (sel: Selection | null) => spanUnion(sel, "r0", "r1");
/** Columns any range touches, in grid order */
export const selCols = (sel: Selection | null) => spanUnion(sel, "c0", "c1");

/** Every selected cell once, row by row in grid order; stops when `fn` returns false */
export function forEachCell(sel: Selection | null, fn: (r: number, c: number) => void | false) {
  const cols = selCols(sel);
  for (const r of selRows(sel)) for (const c of cols) if (contains(sel, r, c) && fn(r, c) === false) return;
}

/**
 * What copy and the Data Extractor write: rows in grid order, columns in grid order. Ranges with different columns share one table
 * of the union of their columns; a cell no range holds is `hole`, never a NULL (extract.HOLE_TOKEN): CSV, TSV and Markdown write an
 * empty field, SQL Inserts, SQL Updates, Where Clause and JSON leave that column out of that row, SQL IN List lists each selected cell.
 */
export function copyTable<T>(sel: Selection | null, value: (r: number, c: number) => T, hole: T): { rows: number[]; cols: number[]; vals: T[][] } {
  const rows = selRows(sel), cols = selCols(sel);
  return { rows, cols, vals: rows.map((r) => cols.map((c) => (contains(sel, r, c) ? value(r, c) : hole))) };
}
