// Grid cell alignment per value class (Settings › Appearance › Alignment). The class rule and the defaults are qurry_shared::format
// through WASM, the TUI grid's rule; the classes the user changed are kept in settings.json, which the TUI reads too.
// Until the module is loaded (or when the WebView refuses WebAssembly) cells get no alignment class: the stylesheet's fallback
// keeps numbers on the right and the rest on the left, as before this setting, and Settings can't change it
import { useSyncExternalStore } from "react";
import { api, type Cell } from "./api";
import { DEFAULT_TOKEN } from "./extract";
import { sharedOr, sharedReady, type Align, type Alignments, type ValueClass } from "./shared";

/** Settings rows, in the order the dialog shows them */
export const VALUE_CLASSES: { cls: ValueClass; label: string }[] = [
  { cls: "string", label: "String" },
  { cls: "number", label: "Number" },
  { cls: "dateTime", label: "Date / time" },
  { cls: "boolean", label: "Boolean" },
  { cls: "null", label: "NULL" },
  { cls: "jsonBinary", label: "JSON / binary" },
];

/** Cell look per class (GridCell kind): number color, <null> in italics. Alignment is separate */
export const CELL_KIND: Partial<Record<ValueClass, "num" | "null">> = { number: "num", null: "null" };

let overrides: Alignments = {};
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

/** Read the saved overrides (at start). If that fails the defaults stay */
export async function loadGridAlign() {
  try {
    overrides = (await api.gridAlign()) ?? {};
    emit();
  } catch {
    /* Defaults */
  }
}

export const alignOf = (cls: ValueClass, o: Alignments = overrides): Align => sharedOr<Align>("left", (s) => s.alignOf(cls, o));

/** Change one class and save. Picking the default drops the override (so a later default applies). Rolls back if saving fails */
export async function setGridAlign(cls: ValueClass, a: Align) {
  const prev = overrides;
  const next = { ...overrides };
  // Without the module the default isn't known: keep the pick rather than drop it
  if (sharedReady() && a === alignOf(cls, {})) delete next[cls];
  else next[cls] = a;
  overrides = next;
  emit();
  try {
    await api.setGridAlign(next);
  } catch (e) {
    overrides = prev;
    emit();
    throw e;
  }
}

/** The saved overrides (re-renders on change) */
export function useGridAlign(): Alignments {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => overrides,
  );
}

/** Alignment of every class under `o`, for a grid render; empty without the module (cells keep the stylesheet's alignment) */
export const alignTable = (o: Alignments): Alignments => (sharedReady() ? Object.fromEntries(VALUE_CLASSES.map(({ cls }) => [cls, alignOf(cls, o)])) : {});

/**
 * Class of a column's cells. The shared rule reads only the value's kind (null, number, boolean, text), not its content,
 * so it is asked once per column and kind. A not-yet-written DEFAULT counts as NULL (it shows as <default>, styled like <null>)
 */
export function cellClasses(typeName: string): (v: Cell) => ValueClass {
  const memo = new Map<string, ValueClass>();
  return (v) => {
    const kind = v === null || v === DEFAULT_TOKEN ? "null" : typeof v;
    const hit = memo.get(kind);
    if (hit) return hit;
    const c = sharedOr<ValueClass>(kind === "null" || kind === "number" ? kind : "string", (s) => s.valueClass(typeName, kind === "null" ? null : v));
    if (sharedReady()) memo.set(kind, c);
    return c;
  };
}
