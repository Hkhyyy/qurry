import { useSyncExternalStore } from "react";

/** Status bar right-side widgets. `aggregate` (the grid Aggregator, a grid's notes) is from whichever side set it last; `pos` (Ln, Col or
 * Row, Col) and `cols` (a grid's shown columns) are the focused editor's or grid's: each sets them for its element, and they show only
 * while focus is inside that element (a table tab, the tree or a dialog shows none, not the console's last position) */
export type FocusWidgets = { pos?: string; cols?: string };
let aggregate: string | undefined;
const owners = new Map<Element, FocusWidgets>();
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

export function setStatusWidget(_k: "aggregate", v: string | undefined) {
  if (aggregate === v) return;
  aggregate = v;
  emit();
}

/** The widgets of an editor or grid (its element), shown while focus is inside it; undefined when it goes away */
export function setFocusWidgets(el: Element, w: FocusWidgets | undefined) {
  if (w) owners.set(el, w);
  else owners.delete(el);
  emit();
}

/** "Ln 2, Col 11 (8 chars)": a console editor's caret and the selection's length */
export const caretText = (line: number, col: number, chars: number) => `Ln ${line}, Col ${col}${chars ? ` (${chars} chars)` : ""}`;
/** "Row 3, Col 5": a result grid's current cell (1-based) */
export const cellText = (row: number, col: number) => `Row ${row}, Col ${col}`;

/** The columns (0-based) a grid shows between `left` and `right` of its scroller: those wholly inside, or the ones cut at its edges when
 * none is (a column wider than the view); null when there are none */
export function shownColumns(cols: { start: number; end: number }[], left: number, right: number): [number, number] | null {
  const whole = cols.flatMap((c, i) => (c.start >= left - 0.5 && c.end <= right + 0.5 ? [i] : []));
  const shown = whole.length ? whole : cols.flatMap((c, i) => (c.end > left && c.start < right ? [i] : []));
  return shown.length ? [shown[0], shown[shown.length - 1]] : null;
}

let snap: { aggregate?: string } & FocusWidgets = {};
function snapshot() {
  const a = typeof document === "undefined" ? null : document.activeElement;
  const w = [...owners].find(([el]) => a && el.contains(a))?.[1] ?? {};
  // The same object while nothing changed (useSyncExternalStore compares it); read on every render, so a tab hidden with focus in it counts
  if (snap.aggregate !== aggregate || snap.pos !== w.pos || snap.cols !== w.cols) snap = { aggregate, pos: w.pos, cols: w.cols };
  return snap;
}

if (typeof document !== "undefined") {
  document.addEventListener("focusin", emit);
  document.addEventListener("focusout", emit);
}

export const useStatusWidgets = () => useSyncExternalStore((f) => (subs.add(f), () => subs.delete(f)), snapshot);
