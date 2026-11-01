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

/** The statement at the cursor. If the cursor is in a gap between statements, picks the previous one. */
export function statementAt(text: string, offset: number, kind: DbKind = "postgres") {
  const segs = splitStatements(text, kind);
  if (segs.length === 0) return null;
  let pick = segs.find((s) => offset >= s.start && offset <= s.end + 1);
  if (!pick) pick = [...segs].reverse().find((s) => s.end < offset) ?? segs[0];
  const raw = text.slice(pick.start, pick.end);
  const lead = raw.length - raw.trimStart().length;
  const sql = raw.trim();
  return { sql, start: pick.start + lead, end: pick.start + lead + sql.length };
}

/**
 * Statement splitting for the editor: after splitting on semicolons, also split statements written without `;` at line-start statement keywords (like DataGrip).
 * UNION · INSERT … SELECT · WITH … SELECT · GRANT SELECT etc. are one statement.
 */
export function splitScript(text: string, kind: DbKind = "postgres"): Segment[] {
  return segments(shared().splitScript(text, kind, EDITOR));
}
