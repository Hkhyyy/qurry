// Selected values as an IN list: qurry_shared::extract through WASM, the same list the TUI copies
import type { Cell, DbKind } from "./api";
import { shared, type ExtractOptions } from "./shared";

/**
 * Where the GUI keeps its own rules (D2 of the 0.3.0 architecture batch): number text stays bare only as -?digits[.digits][e±digits]
 * (the inList rows of crates/shared/tests/DIVERGENCES.md: +4 and .5 are quoted), and number cells are written as the grid shows
 * them, by String() (1e16 as 10000000000000000)
 */
export const GUI_EXTRACT: ExtractOptions = { strictNumbers: true, jsNumbers: true };

/** Numeric·boolean column types (PG·MySQL). Not things like interval */
export function isNumericType(t: string): boolean {
  return shared().isNumericType(t);
}

/**
 * Selected values as a list to put right after IN: ('a', 'b'). With multiple columns, one tuple per row (('a', 1), ('b', 2)). Duplicates·NULL are dropped.
 * types = column type per column: numeric columns keep the value as is (even numeric·bigint that arrive as text), others are quoted (even 010 that looks numeric).
 * Strings as `kind` reads them (MySQL and ClickHouse also double \)
 */
export function inList(rows: Cell[][], types: string[] = [], kind: DbKind = "postgres"): string {
  return shared().inList(rows, types, kind, GUI_EXTRACT);
}
