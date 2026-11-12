import { api, ConnectionView, defaultDbOf } from "../../lib/api";
import { leaves } from "../../lib/panes";
import type { EditTarget } from "../../components/ResultGrid";
import { buildTab, closeTabs, remember } from "./ops";
import { setActiveIds, setFocused, setTabs, useTabsStore } from "./store";
import { DdlOf, Group, Tab, TAB_LIMIT } from "./types";

/** Showing a tab, and opening new ones (in the focused pane unless told otherwise) */
export function useTabs({ setStatus, loadStale }: { setStatus: (s: string) => void; loadStale: (t: Tab) => void }) {
  const focused = useTabsStore((s) => s.focused);
  const activate = (t: Tab) => {
    const cur = useTabsStore.getState().tabs.find((x) => x.id === t.id);
    if (cur?.stale) loadStale(cur);
    setFocused(t.group);
    setActiveIds((a) => a.map((x, g) => (g === t.group ? t.id : x)));
    useTabsStore.setState({ mru: [t.id, ...useTabsStore.getState().mru.filter((x) => x !== t.id)] });
    remember(t);
  };

  // ───────── Opening tabs ─────────
  const addTab = (
    c: ConnectionView,
    kind: Tab["kind"],
    opts: { title?: string; sql?: string; database?: string; tableSql?: string; edit?: EditTarget; group?: Group; where?: string; orderBy?: string; objKind?: string; ddlOf?: DdlOf },
  ) => {
    const database = opts.database && opts.database !== defaultDbOf(c) ? opts.database : undefined;
    const base = database ? `${c.name}/${database}` : c.name;
    const n = useTabsStore.getState().tabs.filter((t) => t.kind === "console" && t.connId === c.id && t.database === database).length;
    const ls = leaves(useTabsStore.getState().panes);
    const group: Group = opts.group ?? (ls.includes(focused) ? focused : ls[0]);
    // If the name already starts with brackets like [DEV], don't wrap it again ("console [[DEV] shop]" → "console [DEV] shop")
    const where = base.startsWith("[") ? base : `[${base}]`;
    const tab = buildTab(c, kind, opts, group, opts.title ?? (n === 0 ? `console ${where}` : `console_${n + 1} ${where}`));
    // Transaction control: if Manual is the default, open the session as Manual too
    if (tab.txManual) api.setTxMode(tab.target, tab.id, true).catch((e) => setStatus(String(e)));
    setTabs((ts) => [...ts, tab]);
    activate(tab);
    // Tab limit: when unpinned tabs exceed 10, close the least recently used first (DataGrip)
    setTimeout(() => {
      const inGroup = useTabsStore.getState().tabs.filter((t) => t.group === group && !t.pinned && t.id !== tab.id);
      const over = inGroup.length + 1 - TAB_LIMIT;
      if (over > 0) {
        const byAge = [...inGroup].sort((a, b) => {
          const ia = useTabsStore.getState().mru.indexOf(a.id), ib = useTabsStore.getState().mru.indexOf(b.id);
          return (ib < 0 ? -1 : ib) - (ia < 0 ? -1 : ia);
        });
        closeTabs(byAge.slice(0, over).map((t) => t.id));
      }
    });
    return tab;
  };

  const openConsole = (c: ConnectionView, db?: string, sql?: string) => addTab(c, "console", { database: db, sql });
  return { activate, addTab, openConsole };
}
