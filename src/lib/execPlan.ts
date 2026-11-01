// What ⌘↩ runs: qurry_shared::exec through WASM, the same rules as the TUI's ⌃↩
import type { DbKind } from "./api";
import { shared } from "./shared";
import type { Segment } from "./sqlSplit";

/** DataGrip Settings › Database › Query Execution */
export interface ExecSettings {
  inside: "ask" | "smallest" | "whole" | "fromCaret";
  outside: "nothing" | "whole" | "below";
  selection: "single" | "separate";
}
/** Like DataGrip, ask what to run when the cursor is inside a statement */
export const DEFAULT_EXEC: ExecSettings = { inside: "ask", outside: "nothing", selection: "separate" };

export interface Stmt {
  start: number;
  end: number;
  sql: string;
}
export type ExecPlan = { run: Stmt[] } | { ask: { label: string; stmts: Stmt[] }[] } | null;

/**
 * With no selection, decide which statement to run from the cursor position, over the console's statements (`segs`, in order).
 * Non-SQL lines before a statement (numbered titles etc.) are left out of what runs; inside BEGIN … COMMIT the transaction is offered
 */
export function planExecution(text: string, offset: number, segs: Segment[], s: ExecSettings, kind: DbKind = "postgres"): ExecPlan {
  return shared().planExecution(text, offset, segs, kind, s) ?? null;
}
