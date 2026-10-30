import type { Cell, Dialect } from "./api";
import { inList } from "./inList";

/** DataGrip Data Extractors (copy formats) */
export const EXTRACTORS = ["CSV", "TSV", "SQL Inserts", "SQL Updates", "Where Clause", "SQL IN List", "JSON", "Markdown"] as const;
export type Extractor = (typeof EXTRACTORS)[number];

/** Display value inserted by Set DEFAULT */
export const DEFAULT_TOKEN = "\u0000DEFAULT";

export const raw = (c: Cell) => (c === null ? "" : c === DEFAULT_TOKEN ? "DEFAULT" : String(c));

function csvField(s: string, sep: string) {
  return s.includes(sep) || s.includes('"') || s.includes("\n") ? `"${s.replace(/"/g, '""')}"` : s;
}

export interface ExtractInput {
  /** Selected columns */
  names: string[];
  types: string[];
  /** Selected cells, one array per row */
  vals: Cell[][];
  /** SQL Updates: (key column, value) pairs per row — empty when the table has no key */
  keys: [string, Cell][][];
  tableRef: string;
  dialect: Pick<Dialect, "q" | "lit">;
  /** A column-name line at the top of CSV · TSV (Copy is values only — DataGrip; Copy with Headers includes it) */
  header: boolean;
}

/** Rows as text in a Data Extractor format */
export function extractRows(fmt: Extractor, x: ExtractInput): string {
  const { names, vals, tableRef, header } = x;
  const quoteIdent = x.dialect.q;
  const sqlLiteral = x.dialect.lit;
  const lit = (c: Cell) => (c === DEFAULT_TOKEN ? "DEFAULT" : sqlLiteral(c));
  switch (fmt) {
    case "TSV":
      return [...(header ? [names.join("\t")] : []), ...vals.map((v) => v.map(raw).join("\t"))].join("\n");
    case "CSV":
      return [...(header ? [names.map((n) => csvField(n, ",")).join(",")] : []), ...vals.map((v) => v.map((c) => csvField(raw(c), ",")).join(","))].join("\n");
    case "SQL Inserts":
      return vals.map((v) => `INSERT INTO ${tableRef} (${names.map(quoteIdent).join(", ")}) VALUES (${v.map(lit).join(", ")});`).join("\n");
    case "SQL Updates":
      return vals
        .map((v, i) => {
          const w = x.keys[i]?.length ? x.keys[i].map(([k, c]) => `${quoteIdent(k)} = ${sqlLiteral(c)}`).join(" AND ") : "/* no key */";
          return `UPDATE ${tableRef} SET ${names.map((n, j) => `${quoteIdent(n)} = ${lit(v[j])}`).join(", ")} WHERE ${w};`;
        })
        .join("\n");
    case "Where Clause":
      return vals.map((v) => "(" + names.map((n, j) => (v[j] === null ? `${quoteIdent(n)} IS NULL` : `${quoteIdent(n)} = ${lit(v[j])}`)).join(" AND ") + ")").join(" OR\n");
    case "SQL IN List":
      return inList(vals.map((v) => v.map((c) => (c === DEFAULT_TOKEN ? null : c))), x.types);
    case "JSON":
      return JSON.stringify(vals.map((v) => Object.fromEntries(names.map((n, j) => [n, v[j] === DEFAULT_TOKEN ? null : v[j]]))), null, 2);
    case "Markdown":
      return [
        `| ${names.join(" | ")} |`,
        `| ${names.map(() => "---").join(" | ")} |`,
        ...vals.map((v) => `| ${v.map((c) => (c === null ? "" : raw(c)).replace(/\|/g, "\\|")).join(" | ")} |`),
      ].join("\n");
  }
}
