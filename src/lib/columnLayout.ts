// The columns a user hid and pinned in a result grid. Table tabs keep them per table in settings.json "columnLayout" through the core
// (api.columnLayout / api.setColumnLayout), which the TUI reads and writes too. Which column shows where is the shared rule
// (qurry_shared::format::column_view), by the keys widths are kept under (widthKeys)
import { useEffect, useSyncExternalStore } from "react";
import { api, type ColumnLayout } from "./api";
import { keyOf, type WidthsOf } from "./columnWidths";
import { sharedOr, type ColumnView } from "./shared";

/** The display order of a result with column `keys` under `layout`: pinned first, hidden left out. Until the module loads, every
 *  column in result order (nothing hidden or pinned) */
export const columnView = (keys: string[], layout: ColumnLayout): ColumnView =>
  sharedOr({ view: keys.map((_, i) => i), pinned: 0, hidden: 0 }, (s) => s.columnView(keys, layout));

/** The display columns the header menu's Hide, Pin and Unpin act on from display column `c` (#216, DataGrip): every column selected
 *  on the headers (`selected`, in display order) when `c` is among them, else `c` alone; `pin` the ones not pinned yet, `unpin` the pinned ones */
export function headerColumns(c: number, selected: number[], pinned: number) {
  const cols = selected.includes(c) ? selected : [c];
  return { cols, pin: cols.filter((x) => x >= pinned), unpin: cols.filter((x) => x < pinned) };
}

/** "Pin Column", or "Pin 3 Columns" for several */
export const columnsLabel = (verb: string, n: number) => (n > 1 ? `${verb} ${n} Columns` : `${verb} Column`);

/** `keys` pinned after the pinned ones, in the order given (their display order); ones pinned already stay where they are */
export const pinKeys = (l: ColumnLayout, keys: string[]): ColumnLayout => ({ ...l, pinned: [...l.pinned, ...keys.filter((k) => !l.pinned.includes(k))] });
/** `keys` unpinned: back to their place in the result */
export const unpinKeys = (l: ColumnLayout, keys: string[]): ColumnLayout => ({ ...l, pinned: l.pinned.filter((k) => !keys.includes(k)) });
/** `keys` hidden (the caller keeps one column shown) */
export const hideKeys = (l: ColumnLayout, keys: string[]): ColumnLayout => ({ ...l, hidden: [...l.hidden.filter((k) => !keys.includes(k)), ...keys] });

// The layout on screen per table or console result, as columnWidths.ts keeps widths: a table's is read again each time a grid for it
// opens (the TUI may have changed it) and forgotten when the last one closes; a console result's lives in memory only
const cache = new Map<string, ColumnLayout>();
const users = new Map<string, number>();
/** Changes made here, so a read that answers after one doesn't undo it */
const edits = new Map<string, number>();
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
export const NO_LAYOUT: ColumnLayout = { hidden: [], pinned: [] };

/** Why saving failed, that the layout still applies, and how to get every column back */
export const layoutSaveError = (e: unknown) =>
  `Couldn't save the hidden and pinned columns (${e}). They apply until the tab closes; the column header's menu › Columns… › Show All, or the options menu › Reset View, shows every column again.`;

/** The hidden and pinned columns of `of` (re-renders on change), and `set` to replace them (a table's are saved; a failed save
 *  says so through `onError`) */
export function useLayoutOf(of: WidthsOf, onError: (msg: string) => void) {
  const k = keyOf(of);
  const layout = useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => cache.get(k) ?? NO_LAYOUT,
  );
  useEffect(() => {
    if (typeof of === "string") return;
    users.set(k, (users.get(k) ?? 0) + 1);
    const n = edits.get(k) ?? 0;
    // A file that can't be read: every column, in result order (the core already skips entries it can't read)
    api.columnLayout(of).then((l) => users.has(k) && (edits.get(k) ?? 0) === n && (cache.set(k, l), emit()), () => {});
    return () => {
      const left = (users.get(k) ?? 1) - 1;
      if (left) return void users.set(k, left);
      users.delete(k);
      cache.delete(k);
    };
  }, [k]);
  const set = (next: ColumnLayout) => {
    edits.set(k, (edits.get(k) ?? 0) + 1);
    cache.set(k, next);
    emit();
    if (typeof of !== "string") api.setColumnLayout(of, next).catch((e) => onError(layoutSaveError(e)));
  };
  return { layout, set };
}
