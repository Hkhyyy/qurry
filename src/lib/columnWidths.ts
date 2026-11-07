// Column widths a user set in a result grid (drag a header's edge, double-click it to fit, the header menu, ⌃⌥← / ⌃⌥→).
// Widths are characters (qurry_shared::format::ColumnWidths, what the TUI draws a column as); the grid turns them into pixels here.
// Table tabs keep them per table in settings.json "columnWidths" through the core, which the TUI reads and writes too; console results
// keep them in memory for that result. A column without a set width keeps the width the grid computes for it
import { useEffect, useSyncExternalStore } from "react";
import { api, type ColumnWidths, type TableRef } from "./api";
import { sharedOr } from "./shared";

/** A grid character and a cell's padding at the default grid font size (12px), in pixels; `em` scales them for another size */
export const CHAR_PX = 7.3;
export const CELL_PAD = 20;

/** Pixels of a column `chars` characters wide */
export const toPx = (chars: number, em: number) => Math.round(chars * CHAR_PX * em + CELL_PAD);

/** The width in characters closest to `px` pixels, kept between the narrowest and the widest (the shared rule) */
export const toChars = (px: number, em: number) => stepWidth((px - CELL_PAD) / (CHAR_PX * em), 0);

/** `chars` moved by `steps` narrow (-) / widen (+) steps, kept in range (qurry_shared::format::column_width); unchanged until the module loads */
export const stepWidth = (chars: number, steps: number) => sharedOr(Math.round(chars), (s) => s.columnWidth(chars, steps));

/** Fit: the characters a column needs for its header (`headPx` pixels) and its longest text in `texts` (the rows loaded so far) */
export function fitChars(headPx: number, texts: string[], em: number): number {
  let len = 0;
  for (const t of texts) len = Math.max(len, t.length);
  // Rounding noise (87.6 / 7.3 = 12.000…02) isn't another character
  return stepWidth(Math.max(Math.ceil((headPx - CELL_PAD) / (CHAR_PX * em) - 1e-6), len), 0);
}

/** Whose widths: a table tab's table (saved), or a console result's id (in memory) */
export type WidthsOf = TableRef | string;

const keyOf = (w: WidthsOf) => (typeof w === "string" ? `result:${w}` : JSON.stringify([w.source, w.database, w.schema, w.table]));
const cache = new Map<string, ColumnWidths>();
const asked = new Set<string>();
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const NONE: ColumnWidths = {};

const put = (k: string, w: ColumnWidths) => {
  cache.set(k, w);
  emit();
};

/** Why saving failed, and that the widths still apply and how to go back */
export const saveError = (e: unknown) =>
  `Couldn't save the column widths (${e}). They apply until the app closes; the column header's menu › Reset Widths goes back to the computed widths.`;

/**
 * The set widths of `of` (re-renders on change), and changing them. A table's are read from the settings once; `set` with `save`
 * merges into what is saved (the TUI may have changed others), `reset` forgets them. A failed save keeps them on screen and says
 * so through `onError`
 */
export function useColumnWidths(of: WidthsOf, onError: (msg: string) => void) {
  const k = keyOf(of);
  const widths = useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => cache.get(k) ?? NONE,
  );
  useEffect(() => {
    if (typeof of === "string" || asked.has(k)) return;
    asked.add(k);
    // A file that can't be read: the computed widths (the core already skips entries it can't read)
    api.columnWidths(of).then((w) => !cache.has(k) && put(k, w), () => {});
  }, [k]);
  const set = (changes: ColumnWidths, save = true) => {
    put(k, { ...(cache.get(k) ?? NONE), ...changes });
    if (save && typeof of !== "string") api.setColumnWidths(of, changes).then((w) => put(k, w), (e) => onError(saveError(e)));
  };
  const reset = () => {
    put(k, NONE);
    if (typeof of !== "string") api.resetColumnWidths(of).catch((e) => onError(saveError(e)));
  };
  return { widths, set, reset };
}
