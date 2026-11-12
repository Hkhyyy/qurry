// DataGrip Data Extractors (copy formats): qurry_shared::extract through WASM
import type { Cell, Dialect } from "./api";
import { GUI_EXTRACT } from "./inList";
import { shared } from "./shared";

export const EXTRACTORS = ["CSV", "TSV", "SQL Inserts", "SQL Updates", "Where Clause", "SQL IN List", "JSON", "Markdown"] as const;
export type Extractor = (typeof EXTRACTORS)[number];

/** Display value inserted by Set DEFAULT (qurry_shared::extract::DEFAULT_TOKEN) */
export const DEFAULT_TOKEN = "\u0000DEFAULT";
/** A copied cell no selected range holds, not a NULL: each format leaves it out (qurry_shared::extract::HOLE_TOKEN) */
export const HOLE_TOKEN = "\u0000HOLE";

export const raw = (c: Cell) => (c === null ? "" : c === DEFAULT_TOKEN ? "DEFAULT" : String(c));

export interface ExtractInput {
  /** Selected columns */
  names: string[];
  types: string[];
  /** Selected cells, one array per row */
  vals: Cell[][];
  /** SQL Updates: (key column, value) pairs per row — empty when the table has no key */
  keys: [string, Cell][][];
  tableRef: string;
  /** Identifiers and literals are written by its quoting kind */
  dialect: Pick<Dialect, "quoting">;
  /** A column-name line at the top of CSV · TSV (Copy is values only — DataGrip; Copy with Headers includes it) */
  header: boolean;
}

/**
 * Pasted text as grid cells, one array per row, null for NULL (qurry_shared::extract::paste_cells, as the TUI reads it): one line
 * without a tab is one value; otherwise TSV when a tab is outside quotes, else CSV (RFC 4180 quotes, an empty field NULL, "" an
 * empty string). Reads back what CSV and TSV copy write
 */
export function pasteCells(text: string): (string | null)[][] {
  return shared().pasteCells(text);
}

/** Rows as text in a Data Extractor format */
export function extractRows(fmt: Extractor, x: ExtractInput): string {
  const { dialect, ...rows } = x;
  return shared().extractRows(fmt, rows, dialect.quoting, GUI_EXTRACT);
}
