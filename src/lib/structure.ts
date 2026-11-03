import type { ConstraintMeta, ForeignKeyMeta } from "./api";

/** DDL tab: SQL text or the Structure tables. The last choice is kept for the next DDL tab (SQL until Structure is picked) */
export type DdlView = "sql" | "structure";
const KEY = "qurry.ddlView";
export function ddlView(): DdlView {
  try {
    return localStorage.getItem(KEY) === "structure" ? "structure" : "sql";
  } catch {
    return "sql";
  }
}
export function setDdlView(v: DdlView) {
  try {
    localStorage.setItem(KEY, v);
  } catch {
    /* Ignore: the view still switches, it just isn't remembered */
  }
}

/** Object kinds whose DDL tab has a Structure view */
export const hasStructure = (kind: string) => kind === "table" || kind === "partitioned";

export const constraintLabel = (kind: ConstraintMeta["kind"]) => ({ p: "Primary key", f: "Foreign key", u: "Unique", c: "Check" })[kind] ?? kind;

/** What an empty Referenced by / Triggers section says: MySQL lists only what the account has privileges on */
export const noneNote = (onlyVisible: boolean) => (onlyVisible ? "None visible to this account (MySQL lists only what it has privileges on)" : "None");

/** One end of a foreign key: from = the referencing table, to = the referenced one */
export const fkSide = (fk: ForeignKeyMeta, side: "from" | "to") =>
  side === "from" ? `${fk.schema}.${fk.table} (${fk.columns.join(", ")})` : `${fk.refSchema}.${fk.refTable} (${fk.refColumns.join(", ")})`;
