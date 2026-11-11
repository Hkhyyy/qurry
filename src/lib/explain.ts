// The Explain statement for a kind (qurry_shared::explain::explain_sql through WASM): the statement the TUI and MCP send too.
import type { DbKind } from "./api";
import { sharedOr } from "./shared";

/** Without the WASM module there is no statement to send */
export const EXPLAIN_NOT_LOADED = "Explain isn't available until the statement builder has loaded: try again in a moment";

/**
 * The statement that shows `sql`'s plan on `kind` (PostgreSQL FORMAT JSON when `json`), or why the kind has none (`err`: send nothing).
 * The text before the statement is explainSql("", …)'s (a server's error position minus its length is one in the statement)
 */
export function explainSql(sql: string, kind: DbKind, analyze: boolean, json: boolean): { sql: string } | { err: string } {
  const r = sharedOr<{ Ok: string } | { Err: string }>({ Err: EXPLAIN_NOT_LOADED }, (s) => s.explainSql(sql, kind, analyze, json));
  return "Ok" in r ? { sql: r.Ok } : { err: r.Err };
}
