// Statement splitting for the console: qurry_shared::sql through WASM, by the data source's kind (strings, comments, dollar quotes,
// MySQL # and \ escapes, [names] …), so the GUI splits like the TUI and the server.
import type { DbKind } from "./api";
import { shared, type SplitOptions } from "./shared";

export interface Segment {
  start: number;
  end: number;
}

/**
 * Where the editor keeps the GUI's own splitting (D2 of the 0.3.0 architecture batch): the splitStatements and splitScript rows
 * of crates/shared/tests/DIVERGENCES.md. Only picks what the editor runs; the backend splits again by the server's rules before any check
 */
const EDITOR: SplitOptions = { flatComments: true, mysqlDashComment: true, dollarAfterName: true };

const segments = (r: [number, number][]): Segment[] => r.map(([start, end]) => ({ start, end }));

/** Split statements on semicolons, skipping strings, quoted names, comments and dollar quotes as `kind` reads them */
export function splitStatements(text: string, kind: DbKind = "postgres"): Segment[] {
  return segments(shared().splitStatements(text, kind, EDITOR));
}

/**
 * Statement splitting for the editor: after splitting on semicolons, also split statements written without `;` at line-start statement keywords (like DataGrip).
 * UNION · INSERT … SELECT · WITH … SELECT · GRANT SELECT etc. are one statement.
 */
export function splitScript(text: string, kind: DbKind = "postgres"): Segment[] {
  return segments(shared().splitScript(text, kind, EDITOR));
}
