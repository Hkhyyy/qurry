// Quotes and brackets in the table tab's WHERE / ORDER BY fields: qurry_shared::pairs through WASM, the TUI editor's rule.
// The console keeps Monaco's own auto-closing (the same pairs); its unbalanced marks are in lib/monaco.ts
import type { DbKind } from "./api";
import { sharedOr } from "./shared";

/** The field after `key` (a typed character or "Backspace") at the selection start..end, or undefined when the key edits as usual
 *  (also while the module is loading: the field then works without pairs) */
export function pairKey(value: string, start: number, end: number, key: string, kind: DbKind): { value: string; selStart: number; selEnd: number } | undefined {
  if (key !== "Backspace" && key.length !== 1) return undefined;
  const e = sharedOr(undefined, (s) => s.pairEdit(value, start, end, key, kind));
  if (!e) return undefined;
  return { value: value.slice(0, e.start) + e.insert + value.slice(e.end), selStart: e.selStart, selEnd: e.selEnd };
}
