// Search Everywhere's order for results with the same score: fixed, whatever order their data sources answer in (the TUI's
// sort_objects in crates/tui/src/app/palette.rs, same order)

/** What the tie-break reads of a database result */
export interface Ranked {
  name: string;
  kind: string;
  /** Schema, or schema.table for a column */
  where: string;
  source: string;
  db: string;
}

const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** Same score: by name, then kind, schema, data source name, database */
export const compareRanked = (a: Ranked, b: Ranked) =>
  cmp(a.name, b.name) || cmp(a.kind, b.kind) || cmp(a.where, b.where) || cmp(a.source, b.source) || cmp(a.db, b.db);
