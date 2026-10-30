/** Data export / import (format, column mapping, progress display) */
export type DataFormat = "csv" | "tsv" | "json" | "sql" | "markdown";

export const FORMATS: { id: DataFormat; label: string; hint: string }[] = [
  { id: "csv", label: "CSV", hint: "Excel, spreadsheets" },
  { id: "tsv", label: "TSV", hint: "tab separated — pastes into spreadsheets as cells" },
  { id: "json", label: "JSON", hint: "array of objects" },
  { id: "sql", label: "SQL INSERT", hint: "one INSERT statement per row" },
  { id: "markdown", label: "Markdown", hint: "table for docs, issues, PRs" },
];

const norm = (s: string) => s.toLowerCase().replace(/[\s_-]/g, "");

/** Column to insert for each file column ("" = skip). table = null means a new table (column names as is) */
export function autoMap(file: string[], table: string[] | null): string[] {
  if (!table) return [...file];
  return file.map((f) => table.find((t) => norm(t) === norm(f)) ?? "");
}

/** Progress bar: % if the total is known (an estimate, so capped at 99% if exceeded), otherwise just the row count */
export function progressText(p: { rows: number; done: number; total: number }): { pct: number | null; text: string } {
  // Backup / restore progress in bytes instead of rows
  const rows = p.rows === 0 && p.done > 0 ? `${(p.done / 1048576).toFixed(1)} MB` : `${p.rows.toLocaleString("en-US")} rows`;
  if (!p.total) return { pct: null, text: rows };
  const pct = Math.min(99, Math.floor((p.done / p.total) * 100));
  return { pct, text: `${rows} · ${pct}%` };
}
