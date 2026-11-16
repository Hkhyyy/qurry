// What more than one file of the result grid (components/ResultGrid.tsx and its hooks here) uses
import type { Cell } from "../../lib/api";
import { DEFAULT_TOKEN } from "../../lib/extract";

export type SortKey = { col: number; dir: "asc" | "desc" };

export function display(c: Cell): string {
  if (c === null) return "<null>";
  if (c === DEFAULT_TOKEN) return "<default>";
  return String(c);
}

/** Cells whose original value is not shown as is (arrays, bytea, etc.) are not editable. */
export const editableValue = (v: Cell, typeName: string) => typeName !== "BYTEA" && !(typeof v === "string" && /^<[A-Z0-9_ \[\]]+>$/.test(v));
