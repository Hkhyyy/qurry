// What kind of statement it is, by the core guard's rules (qurry_shared::guard through WASM), so the GUI decides like the TUI.
// UI decisions only (the Production confirmation, Explain Analyse's rollback, the explorer reload): the backend enforces read-only itself.
import type { DbKind } from "./api";
import { shared } from "./shared";

/**
 * The statement doesn't change data. False while the WASM module isn't loaded, so a statement is then treated as a write:
 * Production asks first and Explain Analyse rolls it back
 */
export function isRead(kind: DbKind, sql: string): boolean {
  try {
    return shared().isRead(sql, kind);
  } catch {
    return false;
  }
}

/** The statement changes structure (CREATE, ALTER, DROP …; leading comments skipped). False while the WASM module isn't loaded */
export function isDdl(kind: DbKind, sql: string): boolean {
  try {
    return shared().isDdl(sql, kind);
  } catch {
    return false;
  }
}
