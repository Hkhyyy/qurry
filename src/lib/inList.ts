import { sqlLiteral, type Cell } from "./api";

const lit = (v: Cell) => (typeof v === "number" || typeof v === "boolean" ? String(v) : sqlLiteral(v));

/** 고른 값을 IN 뒤에 바로 붙일 목록으로: ('a', 'b'). 여러 열이면 행마다 묶음 (('a', 1), ('b', 2)). 중복·NULL 은 뺀다 */
export function inList(rows: Cell[][]): string {
  const items = rows
    .filter((r) => r.some((v) => v !== null))
    .map((r) => (r.length === 1 ? lit(r[0]) : `(${r.map(lit).join(", ")})`));
  const uniq = [...new Set(items)];
  return `(${uniq.length ? uniq.join(", ") : "NULL"})`;
}
