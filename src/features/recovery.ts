// Coming back from a render error without losing work (#181). Recover mounts the app again: the stores (tabs, console text, results,
// workspaces) live outside React, and the backend's console sessions with their transactions stay as they were. A reload is the last
// resort: it saves the workspace first, and the next page closes the sessions the page before left behind (their transactions are
// rolled back), which a toast says
import { useSyncExternalStore } from "react";
import { api } from "../lib/api";
import { allTabs } from "./tabs/ops";
import { saveWorkspaceNow } from "./workspaces/useWorkspaces";

/** The toasts saying so, kept here: one can be added while the app is being mounted again (the old one isn't gone yet) */
let notices: { id: number; text: string }[] = [];
let seq = 0;
const subs = new Set<() => void>();
const changed = () => subs.forEach((f) => f());

/** A toast for a few seconds (App shows them) */
export function recoveryNotice(text: string) {
  const id = ++seq;
  notices = [...notices, { id, text }];
  changed();
  setTimeout(() => dismissRecovery(id), 8000);
}

export function dismissRecovery(id: number) {
  notices = notices.filter((n) => n.id !== id);
  changed();
}

/** The recovery toasts shown now */
export const useRecoveryNotices = () =>
  useSyncExternalStore(
    (f) => (subs.add(f), () => void subs.delete(f)),
    () => notices,
  );

/** Consoles (in any workspace) with uncommitted changes: a reload rolls them back */
export const uncommitted = () => allTabs().filter((t) => t.inTx).length;

/** Reload the window, with the open tabs and console text saved first */
export async function reloadWindow() {
  await saveWorkspaceNow();
  location.reload();
}

/** "Reloaded: rolled back 1 open transaction in [DEV] shop_app" */
export function rolledBackNotice(closed: { name: string; rolledBack: boolean }[]): string | null {
  const rb = closed.filter((c) => c.rolledBack);
  if (!rb.length) return null;
  const names = [...new Set(rb.map((c) => c.name))].join(", ");
  return `Reloaded: rolled back ${rb.length} open transaction${rb.length === 1 ? "" : "s"} in ${names}`;
}

/** At page start: the console sessions of the page before this one (a reload) are closed, so none keeps a transaction or locks */
export function closeSessionsOfLastPage() {
  return api
    .closeAllSessions()
    .then((closed) => {
      const n = rolledBackNotice(closed);
      if (n) recoveryNotice(n);
    })
    .catch(() => {});
}
