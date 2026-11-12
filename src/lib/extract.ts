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

/** What a grid copy last put on the clipboard, and in which format ("other": one raw value, an IN list, SQL …) */
let lastCopy: { text: string; format: Extractor | "other" } | undefined;

/** A grid copy wrote `text` to the clipboard in `format` (a single cell's raw value is "other") */
export function copied(text: string, format: Extractor | "other") {
  lastCopy = { text, format };
}

/**
 * Pasted text as grid cells, one array per row, null for NULL (qurry_shared::extract::paste_cells, as the TUI reads it): one line
 * without a tab is one value; otherwise TSV when a tab is outside quotes, else CSV (RFC 4180 quotes, an empty field NULL, "" an
 * empty string). Text that is the grid's last CSV or TSV copy is read as that format, so one copied row splits into its cells
 */
export function pasteCells(text: string): (string | null)[][] {
  const own = lastCopy?.text === text && (lastCopy.format === "CSV" || lastCopy.format === "TSV") ? lastCopy.format : "auto";
  return shared().pasteCells(text, own);
}

/** Rows as text in a Data Extractor format */
export function extractRows(fmt: Extractor, x: ExtractInput): string {
  const { dialect, ...rows } = x;
  return shared().extractRows(fmt, rows, dialect.quoting, GUI_EXTRACT);
}
