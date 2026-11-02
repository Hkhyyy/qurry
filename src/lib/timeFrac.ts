// Align fractional-second digits of a time column within the column (display only): 03:19:15 · 09:38:06.521101 → 03:19:15.000000 · 09:38:06.521101.
// qurry_shared::format through WASM, the TUI grid's rule. Until the module is loaded values show as they are
import { sharedOr } from "./shared";

export const isTimeType = (typeName: string) => sharedOr(false, (s) => s.isTimeType(typeName));

/** The longest fractional-second digit count among the loaded values (0 if none) */
export function fracWidth(values: unknown[]): number {
  return sharedOr(0, (s) => s.fracWidth(values.filter((v): v is string => typeof v === "string")));
}

export function padFrac(s: string, width: number): string {
  if (!width) return s;
  return sharedOr(s, (x) => x.padFrac(s, width));
}
