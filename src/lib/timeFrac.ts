/** Align fractional-second digits of a time column within the column (display only): 03:19:15 · 09:38:06.521101 → 03:19:15.000000 · 09:38:06.521101 */
const TIME = /^((?:\d{4,}-\d\d-\d\d[ T])?\d\d:\d\d:\d\d)(?:\.(\d+))?/;

export const isTimeType = (typeName: string) => /TIME/i.test(typeName) && !/INTERVAL/i.test(typeName);

/** The longest fractional-second digit count among the loaded values (0 if none) */
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
