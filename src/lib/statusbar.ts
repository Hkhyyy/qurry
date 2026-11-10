import { useSyncExternalStore } from "react";
import { api, meta, type ColumnMeta } from "./api";
import { sharedOr, type GridStatus, type StatusItemId, type StatusSpan } from "./shared";

/** Status bar right-side widgets. `note` (a grid's notes: why a paste or a copy format can't run) is from whichever side set it last;
 * `pos` (an editor's Ln, Col) and `grid` (what a focused grid tells: its position, selection, current cell and shown columns) are the
 * focused editor's or grid's: each sets them for its element, and they show only while focus is inside that element (a table tab, the
 * tree or a dialog shows none, not the console's last position) */
export type FocusWidgets = { pos?: string; grid?: GridStatus };
let note: string | undefined;
const owners = new Map<Element, FocusWidgets>();
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

export function setStatusNote(v: string | undefined) {
  if (note === v) return;
  note = v;
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
/** "Row 3, Col 5": a result grid's current cell (1-based), while the WebAssembly module (which words the Position item) isn't there */
export const cellText = (row: number, col: number) => `Row ${row}, Col ${col}`;

/** Rows or columns (0-based, ascending, each once) as the status bar's span: 1-based first, last and how many */
export const spanOf = (sorted: number[]): StatusSpan => ({ first: sorted[0] + 1, last: sorted[sorted.length - 1] + 1, count: sorted.length });

/** The columns (0-based) a grid shows between `left` and `right` of its scroller: those wholly inside, or the ones cut at its edges when
 * none is (a column wider than the view); null when there are none */
export function shownColumns(cols: { start: number; end: number }[], left: number, right: number): [number, number] | null {
  const whole = cols.flatMap((c, i) => (c.start >= left - 0.5 && c.end <= right + 0.5 ? [i] : []));
  const shown = whole.length ? whole : cols.flatMap((c, i) => (c.end > left && c.start < right ? [i] : []));
  return shown.length ? [shown[0], shown[shown.length - 1]] : null;
}

const ownerOf = (a: Element | null) => [...owners.keys()].find((el) => a && el.contains(a)) ?? null;
let held: Element | null = null;
/** While the status bar's checklist has the focus: keep showing the widgets of the editor or grid that had it (they still follow it) */
export function holdFocusWidgets(hold: boolean) {
  held = hold ? ownerOf(document.activeElement) : null;
  emit();
}

let snap: { note?: string } & FocusWidgets = {};
function snapshot() {
  const a = typeof document === "undefined" ? null : document.activeElement;
  const el = ownerOf(a) ?? held;
  const w = (el && owners.get(el)) ?? {};
  // The same object while nothing changed (useSyncExternalStore compares it); read on every render, so a tab hidden with focus in it counts
  if (snap.note !== note || snap.pos !== w.pos || snap.grid !== w.grid) snap = { note, pos: w.pos, grid: w.grid };
  return snap;
}

if (typeof document !== "undefined") {
  document.addEventListener("focusin", emit);
  document.addEventListener("focusout", emit);
}

const subscribe = (f: () => void) => (subs.add(f), () => void subs.delete(f));
export const useStatusWidgets = () => useSyncExternalStore(subscribe, snapshot);

// ───────── Which items are on (settings.json "statusBarItems", shared with the TUI) ─────────
// Read at start and again whenever a checklist opens (the TUI may have changed it); each change is applied by the core to what the
// file holds now, and what it saved replaces ours. Until it is read: the defaults (qurry_shared::statusbar::DEFAULTS)
let items: StatusItemId[] | null = null;
const itemSubs = new Set<() => void>();
const itemsChanged = (next: StatusItemId[]) => {
  items = next;
  itemSubs.forEach((f) => f());
};

/** Read the saved items (at start, and when a checklist opens). If that fails what we have stays */
export async function loadStatusItems() {
  try {
    itemsChanged(await api.statusBarItems());
  } catch {
    /* Kept */
  }
}

const saved = async (p: Promise<StatusItemId[]>, prev: StatusItemId[]) => {
  try {
    itemsChanged(await p);
  } catch (e) {
    itemsChanged(prev);
    throw `Couldn't save the status bar items (${e}). They are back to what they were.`;
  }
};

/** Turn an item on or off (at once, then saved; rolled back if saving fails) */
export function setStatusItem(id: StatusItemId, on: boolean): Promise<void> {
  const prev = currentItems();
  itemsChanged(on ? [...prev.filter((i) => i !== id), id] : prev.filter((i) => i !== id));
  return saved(api.setStatusBarItem(id, on), prev);
}

/** Restore Defaults */
export function resetStatusItems(): Promise<void> {
  return saved(api.resetStatusBarItems(), currentItems());
}

let defaults: StatusItemId[] | null = null;
// The same array on every call (useSyncExternalStore compares it)
const currentItems = (): StatusItemId[] => items ?? (defaults ??= sharedOr<StatusItemId[] | null>(null, (s) => s.statusBarInfo().defaults)) ?? NONE;
const NONE: StatusItemId[] = [];

/** The items on (re-renders on change) */
export const useStatusItems = (): StatusItemId[] => useSyncExternalStore((f) => (itemSubs.add(f), () => void itemSubs.delete(f)), currentItems);

// ───────── Table columns' declared types, for Size (its column limit) ─────────
// A grid asks only while Size is on and its current cell is in a table column; each table is read once per data source, again each time
// a table tab loads it (setTableColumns), and asked again a while after it couldn't be read
const RETRY_MS = 30_000;
const tables = new Map<string, ColumnMeta[] | "loading" | { failedAt: number }>();
const tableSubs = new Set<() => void>();
let tablesVersion = 0;
const tableKey = (target: string, schema: string, table: string) => JSON.stringify([target, schema, table]);
const tablesChanged = () => {
  tablesVersion++;
  tableSubs.forEach((f) => f());
};

/** A table's columns as just read (a table tab's load): Size uses them from now on */
export function setTableColumns(target: string, schema: string, table: string, cols: ColumnMeta[]) {
  tables.set(tableKey(target, schema, table), cols);
  tablesChanged();
}

/** The table's columns, or undefined while they load (or when they can't be read: Size then shows the bytes only) */
export function tableColumns(target: string, schema: string, table: string): ColumnMeta[] | undefined {
  const k = tableKey(target, schema, table);
  const v = tables.get(k);
  if (v === undefined || (typeof v === "object" && "failedAt" in v && Date.now() - v.failedAt >= RETRY_MS)) {
    tables.set(k, "loading");
    meta.table(target, schema, table).then(
      (d) => tables.set(k, d.columns),
      () => tables.set(k, { failedAt: Date.now() }),
    ).finally(tablesChanged);
  }
  return Array.isArray(v) ? v : undefined;
}

/** Re-renders when a table's columns arrive */
export const useTableColumnsLoaded = () => useSyncExternalStore((f) => (tableSubs.add(f), () => void tableSubs.delete(f)), () => tablesVersion);
