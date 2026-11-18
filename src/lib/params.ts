// User parameters (`:name` in code) through WASM: qurry_shared::params, the rule the TUI uses too.
// Without the module nothing is found, so a run sends the console's text as before.
import type { DbKind } from "./api";
import { sharedOr } from "./shared";

/** A `:name` in the text: start..end (UTF-16) from the `:` to the end of the name */
export interface Param {
  name: string;
  start: number;
  end: number;
}

/** Every `:name` in code (not in strings, quoted names, comments or dollar quotes), in text order */
export function findParams(sql: string, kind: DbKind): Param[] {
  return sharedOr<Param[]>([], (s) => s.findParams(sql, kind));
}

/** `sql` with each `:name` replaced by its value as typed (the user writes the quotes); a name without a value stays as is */
export function substituteParams(sql: string, kind: DbKind, values: Record<string, string>): string {
  return sharedOr(sql, (s) => s.substituteParams(sql, kind, values));
}
