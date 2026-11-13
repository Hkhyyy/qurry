// Statement splitting for the console: qurry_shared::sql through WASM, by the data source's kind (strings, comments, dollar quotes,
// MySQL # and \ escapes, [names] …), so the GUI splits like the TUI and the server.
import type { DbKind } from "./api";
import { shared, sharedOr, type SplitOptions } from "./shared";

export interface Segment {
  start: number;
  end: number;
}

/**
 * Where the editor keeps the GUI's own splitting (D2 of the 0.3.0 architecture batch): the splitStatements and splitScript rows
 * of crates/shared/tests/DIVERGENCES.md. Only picks what the editor runs; the backend splits again by the server's rules before any check.
 * MySQL `--` with no space after it is not a comment here either (`select 1--1; delete …` would run the DELETE inside what looked like one statement)
 */
const EDITOR: SplitOptions = { dollarAfterName: true };

const segments = (r: [number, number][]): Segment[] => r.map(([start, end]) => ({ start, end }));

/** Split statements on semicolons, skipping strings, quoted names, comments and dollar quotes as `kind` reads them */
export function splitStatements(text: string, kind: DbKind = "postgres"): Segment[] {
  return segments(shared().splitStatements(text, kind, EDITOR));
}

/** The statement at the cursor, trimmed (between statements, the one just before); null if the text has none */
export function statementAt(text: string, offset: number, kind: DbKind = "postgres"): string | null {
  return shared().statementAt(text, offset, kind) ?? null;
}

/**
 * Statement splitting for the editor: after splitting on semicolons, also split statements written without `;` at line-start statement keywords (like DataGrip).
 * UNION · INSERT … SELECT · WITH … SELECT · GRANT SELECT etc. are one statement.
 */
export function splitScript(text: string, kind: DbKind = "postgres"): Segment[] {
  return segments(shared().splitScript(text, kind, EDITOR));
}

/**
 * A statement as the confirm dialogs show it (qurry_shared::sql::without_comments, the TUI's): its comments left out as the server
 * reads them, whitespace collapsed; it still runs as written. Without the WASM module, whitespace collapsed only
 */
export function withoutComments(sql: string, kind: DbKind = "postgres"): string {
  return sharedOr(sql.replace(/\s+/g, " ").trim(), (s) => s.withoutComments(sql, kind));
}
