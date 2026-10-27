import { sqlLiteral, type Cell } from "./api";

/** 숫자·불리언 컬럼 타입 (PG·MySQL). interval 같은 것은 아니다 */
export function isNumericType(t: string): boolean {
  return /^(int[248]?|integer|tinyint|smallint|mediumint|bigint|serial[248]?|smallserial|bigserial|numeric|decimal|real|float[48]?|double|bool|boolean|oid)\b/i.test(t.trim());
}

const NUM = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;

/**
 * 고른 값을 IN 뒤에 바로 붙일 목록으로: ('a', 'b'). 여러 열이면 행마다 묶음 (('a', 1), ('b', 2)). 중복·NULL 은 뺀다.
 * types = 열마다 컬럼 타입: 숫자 컬럼은 값 그대로 (numeric·bigint 는 글로 와도), 나머지는 따옴표 (숫자처럼 보이는 010 도)
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
