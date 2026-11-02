// What kind of statement it is, by the core guard's rules (qurry_shared::guard through WASM), so the GUI decides like the TUI.
// UI decisions only (the Production confirmation, Explain Analyse's rollback, the explorer reload): the backend enforces read-only itself.
import type { DbKind } from "./api";
import { shared } from "./shared";

/**
 * The statement doesn't change data (with several statements in the text, none does). False while the WASM module isn't loaded, so a statement is then treated as a write:
 * Production asks first and Explain Analyse rolls it back
 */
export function isRead(kind: DbKind, sql: string): boolean {
  try {
    return shared().isRead(sql, kind);
  } catch {
    return false;
  }
}

/**
 * Why Explain Analyse asks before it runs the statement (qurry_shared::guard::explain_analyze_ask, the TUI's words), or undefined.
 * Without the WASM module nothing can be checked, so it asks
 */
export function explainAnalyzeAsk(kind: DbKind, sql: string): string | undefined {
  try {
    return shared().explainAnalyzeAsk(sql, kind);
  } catch {
    return "it can't be checked without the statement splitter";
  }
}

/** What Output says after Explain Analyse rolled the statement back (PostgreSQL · MySQL; the words after "Explain Analyse "), or undefined */
export function explainAnalyzeNote(kind: DbKind, sql: string): { level: "info" | "warn"; text: string } | undefined {
  try {
    const n = shared().explainAnalyzeNote(sql, kind);
    return n && { level: n[0] ? "warn" : "info", text: n[1] };
  } catch {
    return { level: "info", text: "ran the statement in a transaction and rolled it back; it couldn't be checked for effects a rollback doesn't undo" };
  }
}

/**
 * The statement changes structure (CREATE, ALTER, DROP …; leading comments skipped). Null while the WASM module isn't loaded:
 * it may, so the explorer reloads and Production says so
 */
export function isDdl(kind: DbKind, sql: string): boolean | null {
  try {
    return shared().isDdl(sql, kind);
  } catch {
    return null;
  }
}
