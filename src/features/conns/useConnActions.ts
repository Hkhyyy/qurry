import { useEffect, useState } from "react";
import { invalidateCompletion } from "../../lib/completion";
import { invalidateSearchIndex } from "../../components/SearchEverywhere";
import { api, ConnectionView, ImportLocation, ImportPlan, removedDespite } from "../../lib/api";
import { ask, notice } from "../../lib/dialogs";
import { cmd } from "../../lib/shortcuts";
import { confirm, confirmPick, type MenuEntry } from "../../ui";
import { closeTabs } from "../tabs/ops";
import { useTabsStore } from "../tabs/store";

/** Data Source actions: color, shown schemas, export / import of the list, remove */
export function useConnActions({ conns, refresh, setStatus }: { conns: ConnectionView[]; refresh: () => Promise<void>; setStatus: (s: string) => void }) {
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

  // Import Data Sources ▸ the DataGrip / IntelliJ / DBeaver locations found (read again each time the menu or Find Action opens)
  const [importLocs, setImportLocs] = useState<ImportLocation[]>([]);
  const loadImportLocs = () => {
    api.importLocations().then(setImportLocs, (e) => console.error("import locations:", e));
  };
  useEffect(loadImportLocs, []);

  /** The preview: one row per source, a checkbox on each added / updated row; nothing importable = a notice (OK only) */
  const confirmImport = async (plan: ImportPlan) => {
    const detail = plan.rows.map((r) => ({ text: r.text, note: r.note, value: r.value, tone: r.tone ?? undefined, checkable: r.checkable }));
    if (!plan.importable) {
      setStatus(plan.message.split("\n")[0]);
      return void (await confirm({ title: plan.title, message: plan.message, detail, cancelLabel: null }));
    }
    const picked = await confirmPick({ title: plan.title, message: plan.message, detail, pickLabel: (n) => `Import ${n}` });
    if (!picked) return;
    const r = await api.importPicked(picked);
    setStatus(r.message);
    refresh();
  };

  /** A found location (its path) or the native picker (File… / Folder…) */
  const importFrom = async (from: { path: string } | { folder: boolean }) => {
    try {
      // A file from someone else can turn off Read-only or add SQL to run on connect → show what changes and confirm
      const plan = "path" in from ? await api.importLocation(from.path) : await api.pickImport(from.folder);
      if (plan) await confirmImport(plan);
    } catch (e) {
      // Nothing was imported: say why where it can be read whole (a parse error has the line and column)
      setStatus(String(e).split("\n")[0]);
      await notice(String(e), "Import Data Sources");
    }
  };
  const importConns = () => importFrom({ folder: false });

  /** The Import Data Sources submenu (explorer ⋮); Find Action lists the same */
  const importItems = (): MenuEntry[] => [
    { heading: importLocs.length ? "Found on this computer" : "No DataGrip, IntelliJ or DBeaver settings found" },
    ...importLocs.map((l) => ({ label: `${l.label}  (${l.count})`, title: l.shown, action: () => importFrom({ path: l.path }) })),
    { separator: true as const },
    { label: "File…", ...cmd("ds.import"), title: "A Qurry export, dataSources.xml or DBeaver's data-sources.json", action: () => importFrom({ folder: false }) },
    { label: "Folder…", title: "A project or its .idea, an IDE options folder, a DBeaver workspace or .dbeaver folder", action: () => importFrom({ folder: true }) },
  ];

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

  /** Refresh (⌥⌘y, Find Action): Data Source list + search and autocomplete caches */
  const refreshAll = () => {
    invalidateSearchIndex();
    for (const c of conns) invalidateCompletion(c.id);
    refresh();
  };
  return { setColor, setScope, exportConns, importConns, importFrom, importLocs, importItems, loadImportLocs, removeConn, refreshAll };
}
