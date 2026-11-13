// Search Everywhere's scope: which data sources it searches. The last one picked is kept in settings.json ("searchScope"), which the
// TUI's ⌃p reads too (crates/tui/src/app/palette.rs, same names)
import { useSyncExternalStore } from "react";
import { api, type ConnectionView, type SearchScope } from "../../lib/api";
import { wsConns } from "../workspaces/ops";

export const SEARCH_SCOPES: { id: SearchScope; label: string }[] = [
  { id: "workspace", label: "This Workspace" },
  { id: "all", label: "All Places" },
  { id: "context", label: "Context Data Sources" },
];

/** The data sources a scope searches. This Workspace: the current workspace's explorer list. Context Data Sources without a
 *  context (nothing selected, no tab) is This Workspace, so a saved Context still finds something */
export function scopeConns(scope: SearchScope, conns: ConnectionView[], wsConnIds: string[], contextConnId?: string): ConnectionView[] {
  if (scope === "all") return conns;
  if (scope === "context" && contextConnId) return conns.filter((c) => c.id === contextConnId);
  return wsConns(wsConnIds, conns);
}

/** The scope a Search Everywhere opened now shows: a saved Context Data Sources with no context is This Workspace */
export const shownScope = (saved: SearchScope, contextConnId?: string): SearchScope => (saved === "context" && !contextConnId ? "workspace" : saved);

/** The next scope, for the Go to Database Object key pressed again (skips Context Data Sources when there is none) */
export function nextScope(s: SearchScope, contextConnId?: string): SearchScope {
  const ids = SEARCH_SCOPES.map((x) => x.id).filter((id) => id !== "context" || contextConnId);
  return ids[(ids.indexOf(s) + 1) % ids.length];
}

let current: SearchScope = "workspace";
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

/** Read the saved scope (at start). If that fails it stays This Workspace */
export async function loadSearchScope() {
  try {
    current = (await api.searchScope()) ?? "workspace";
    emit();
  } catch {
    /* This Workspace */
  }
}

/** Pick and save. The pick stays for this run if saving fails */
export function setSearchScope(s: SearchScope) {
  current = s;
  emit();
  return api.setSearchScope(s);
}

/** The saved scope (re-renders on change) */
export function useSearchScope(): SearchScope {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => current,
  );
}
