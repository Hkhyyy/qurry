/** 시간 컬럼의 소수 초 자릿수를 컬럼 안에서 맞춘다 (보기만): 03:19:15 · 09:38:06.521101 → 03:19:15.000000 · 09:38:06.521101 */
const TIME = /^((?:\d{4,}-\d\d-\d\d[ T])?\d\d:\d\d:\d\d)(?:\.(\d+))?/;

export const isTimeType = (typeName: string) => /TIME/i.test(typeName) && !/INTERVAL/i.test(typeName);

/** 불러온 값 중 가장 긴 소수 초 자릿수 (없으면 0) */
export function fracWidth(values: unknown[]): number {
  let n = 0;
  for (const v of values) {
    if (typeof v !== "string") continue;
    const m = TIME.exec(v);
    if (m?.[2]) n = Math.max(n, m[2].length);
  }
  return n;
}

export function padFrac(s: string, width: number): string {
  if (!width) return s;
  const m = TIME.exec(s);
  if (!m) return s;
  const frac = m[2] ?? "";
  if (frac.length >= width) return s;
  return `${m[1]}.${frac.padEnd(width, "0")}${s.slice(m[0].length)}`;
}
