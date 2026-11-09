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

/** The width in characters closest to `px` pixels, kept between the narrowest and the widest (the shared rule); never 0, which forgets a width */
export const toChars = (px: number, em: number) => stepWidth(Math.max((px - CELL_PAD) / (CHAR_PX * em), 1), 0);

/** `chars` moved by `steps` narrow (-) / widen (+) steps, kept in range (qurry_shared::format::column_width); unchanged until the module loads */
export const stepWidth = (chars: number, steps: number) => sharedOr(Math.round(chars), (s) => s.columnWidth(chars, steps));

/**
 * Fit: the characters a column needs for its header (`headPx` pixels) and its longest text in `texts` (the rows loaded so far). A wide
 * character (Hangul, CJK, emoji) counts 2, as the TUI draws it (qurry_shared::format::text_width): about 10.4 px at 12px, within 2 × 7.3
 */
export function fitChars(headPx: number, texts: string[], em: number): number {
  const len = sharedOr(texts.reduce((n, t) => Math.max(n, t.length), 0), (s) => s.textWidth(texts));
  // Rounding noise (87.6 / 7.3 = 12.000…02) isn't another character
  return stepWidth(Math.max(Math.ceil((headPx - CELL_PAD) / (CHAR_PX * em) - 1e-6), len), 0);
}

/** The keys a result's columns keep their set widths under: the name, `id#2` for a name the result repeats (a join's two `id`s) */
export const widthKeys = (names: string[]) => sharedOr(names, (s) => s.columnKeys(names));

/** Whose widths: a table tab's table (saved), or a console result's id (in memory) */
export type WidthsOf = TableRef | string;

const keyOf = (w: WidthsOf) => (typeof w === "string" ? `result:${w}` : JSON.stringify([w.source, w.database, w.schema, w.table]));
const cache = new Map<string, ColumnWidths>();
/** Grids showing a table's widths; when the last one closes they are forgotten, so the table's next tab reads what is saved then */
const users = new Map<string, number>();
/** Changes made here, so a read that answers after one doesn't undo it */
const edits = new Map<string, number>();
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const NONE: ColumnWidths = {};

const put = (k: string, w: ColumnWidths) => {
  cache.set(k, w);
  emit();
};

/** Why saving failed, and that the widths still apply and how to go back */
export const saveError = (e: unknown) =>
  `Couldn't save the column widths (${e}). They apply until the tab closes; the column header's menu › Reset Widths goes back to the computed widths.`;

/**
 * The set widths of `of` (re-renders on change), and changing them. A table's are read from the settings each time a grid for it
 * opens (the TUI may have changed them), on top of what is on screen; `set` with `save` merges into what is saved, `reset` forgets
 * them. A failed save keeps them on screen until the tab closes and says so through `onError`
 */
export function useColumnWidths(of: WidthsOf, onError: (msg: string) => void) {
  const k = keyOf(of);
  const widths = useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => cache.get(k) ?? NONE,
  );
  useEffect(() => {
    if (typeof of === "string") return;
    users.set(k, (users.get(k) ?? 0) + 1);
    const n = edits.get(k) ?? 0;
    // A file that can't be read: the computed widths (the core already skips entries it can't read)
    api.columnWidths(of).then((w) => users.has(k) && (edits.get(k) ?? 0) === n && put(k, { ...(cache.get(k) ?? NONE), ...w }), () => {});
    return () => {
      const left = (users.get(k) ?? 1) - 1;
      if (left) return void users.set(k, left);
      users.delete(k);
      cache.delete(k);
    };
  }, [k]);
  const edited = () => edits.set(k, (edits.get(k) ?? 0) + 1);
  /** `changes`: column key → characters; 0 forgets the column's width (back to the computed one) */
  const set = (changes: ColumnWidths, save = true) => {
    edited();
    const next: ColumnWidths = { ...(cache.get(k) ?? NONE), ...changes };
    for (const [c, w] of Object.entries(changes)) if (w === 0) delete next[c];
    put(k, next);
    if (save && typeof of !== "string") api.setColumnWidths(of, changes).then((w) => put(k, w), (e) => onError(saveError(e)));
  };
  const reset = () => {
    edited();
    put(k, NONE);
    if (typeof of !== "string") api.resetColumnWidths(of).catch((e) => onError(saveError(e)));
  };
  return { widths, set, reset };
}
