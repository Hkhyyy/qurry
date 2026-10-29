import { sqlLiteral, type Cell } from "./api";

/** Numeric·boolean column types (PG·MySQL). Not things like interval */
export function isNumericType(t: string): boolean {
  return /^(int[248]?|integer|tinyint|smallint|mediumint|bigint|serial[248]?|smallserial|bigserial|numeric|decimal|real|float[48]?|double|bool|boolean|oid)\b/i.test(t.trim());
}

const NUM = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;

/**
 * Selected values as a list to put right after IN: ('a', 'b'). With multiple columns, one tuple per row (('a', 1), ('b', 2)). Duplicates·NULL are dropped.
 * types = column type per column: numeric columns keep the value as is (even numeric·bigint that arrive as text), others are quoted (even 010 that looks numeric)
 */
export function inList(rows: Cell[][], types: string[] = []): string {
  const lit = (v: Cell, c: number) => {
    if (typeof v === "number" || typeof v === "boolean") return String(v);
    if (v !== null && isNumericType(types[c] ?? "") && NUM.test(v)) return v;
    return sqlLiteral(v);
  };
  const items = rows
    .filter((r) => r.some((v) => v !== null))
    .map((r) => (r.length === 1 ? lit(r[0], 0) : `(${r.map(lit).join(", ")})`));
  const uniq = [...new Set(items)];
  return `(${uniq.length ? uniq.join(", ") : "NULL"})`;
}
