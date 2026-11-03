import { api, ConnectionView, removedDespite } from "../../lib/api";
import { ask } from "../../lib/dialogs";
import { closeTabs } from "../tabs/ops";
import { useTabsStore } from "../tabs/store";

/** Data Source actions: color, shown schemas, export / import of the list, remove */
export function useConnActions({ refresh, setStatus }: { refresh: () => Promise<void>; setStatus: (s: string) => void }) {
  // Only that part: the list may be stale (another app changed the source since), so the backend changes it in the saved one
  const setColor = async (c: ConnectionView, color: string | null) => {
    await api.setConnectionColor(c.id, color);
    refresh();
  };

  const setScope = async (c: ConnectionView, scope: NonNullable<ConnectionView["scope"]>) => {
    await api.setConnectionScope(c.id, scope);
    refresh();
  };

  const exportConns = async () => {
    try {
      const r = await api.exportConnections();
      if (r) setStatus(`Exported ${r[0]} data sources (passwords excluded) to ${r[1]}`);
    } catch (e) {
      setStatus(String(e));
    }
  };

  const importConns = async () => {
    try {
      // A file from someone else can turn off Read-only or add SQL to run on connect → show what changes and confirm
      const preview = await api.pickImportFile();
      if (!preview) return;
      if (!preview.length) return setStatus("Nothing to import");
      if (!(await ask(`Import these data sources?\n\n${preview.join("\n")}`))) return;
      const r = await api.importPicked();
      setStatus(`Imported: ${r.added} added, ${r.updated} updated (re-enter passwords)`);
      refresh();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const removeConn = async (c: ConnectionView) => {
    if (!(await ask(`Delete data source "${c.name}"?`, { okLabel: "Delete", danger: true }))) return;
    try {
      await api.deleteConnection(c.id);
    } catch (e) {
      setStatus(String(e));
      // Removed even if its saved password couldn't be (damaged secrets file): the message says so. Any other error kept it, and its tabs stay
      if (!removedDespite(e)) return;
    }
    closeTabs(useTabsStore.getState().tabs.filter((t) => t.connId === c.id).map((t) => t.id));
    refresh();
  };
  return { setColor, setScope, exportConns, importConns, removeConn };
}
