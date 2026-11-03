import { api, ConnectionView, targetId } from "../../lib/api";
import { ask } from "../../lib/dialogs";
import { forgetConsoleModel } from "../../lib/monaco";
import { leaves, Pane, prunePanes } from "../../lib/panes";
import type { EditTarget } from "../../components/ResultGrid";
import { stash } from "../workspaces/store";
import { editorApis, liveSql, setActiveIds, setFocused, setPanes, setTabs, useTabsStore } from "./store";
import { Group, snapKey, Tab, TabSnapshot } from "./types";

// ───────── Tab and pane operations on the tabs store (and on the workspaces stash, for tabs in another workspace) ─────────
const last = <T,>(a: T[]): T | undefined => a[a.length - 1];
/** If the tab is in another workspace, change it there (so query results that finished meanwhile are not lost) */
const updStashed = (id: string, fn: (t: Tab) => Tab) => {
  for (const w of stash.values()) {
    if (w.tabs.some((t) => t.id === id)) {
      w.tabs = w.tabs.map((t) => (t.id === id ? fn(t) : t));
      return true;
    }
  }
  return false;
};
/** Tabs of all workspaces */
const allTabs = () => [...useTabsStore.getState().tabs, ...[...stash.values()].flatMap((w) => w.tabs)];
const patch = (id: string, p: Partial<Tab>) => {
  if (!updStashed(id, (t) => ({ ...t, ...p }))) setTabs((ts) => ts.map((t) => (t.id === id ? { ...t, ...p } : t)));
};
const updTab = (id: string, fn: (t: Tab) => Tab) => {
  if (!updStashed(id, fn)) setTabs((ts) => ts.map((t) => (t.id === id ? fn(t) : t)));
};
const sqlOf = (t: Tab) => liveSql.get(t.id) ?? t.sql;
const snapshot = (t: Tab): TabSnapshot => ({
  key: snapKey(t),
  kind: t.kind,
  title: t.title,
  connId: t.connId,
  database: t.database,
  sql: sqlOf(t),
  edit: t.edit,
  where: t.where,
  orderBy: t.orderBy,
});
const remember = (t: Tab) => {
  const snap = snapshot(t);
  useTabsStore.setState({ recent: [snap, ...useTabsStore.getState().recent.filter((r) => r.key !== snap.key)].slice(0, 50) });
};

/** One tab (state only; does not open or run anything) */
const buildTab = (
  c: ConnectionView,
  kind: Tab["kind"],
  opts: { sql?: string; database?: string; tableSql?: string; edit?: EditTarget; where?: string; orderBy?: string },
  group: Group,
  title: string,
): Tab => {
  const database = opts.database && opts.database !== (c.database || "postgres") ? opts.database : undefined;
  return {
    id: crypto.randomUUID(),
    kind,
    group,
    connId: c.id,
    database,
    target: targetId(c.id, database),
    title,
    sql: opts.sql ?? "",
    running: false,
    result: null,
    error: null,
    lastSql: null,
    loadingMore: false,
    where: opts.where ?? "",
    orderBy: opts.orderBy ?? "",
    tableSql: opts.tableSql,
    edit: opts.edit,
    results: [],
    activeResult: "output",
    output: [],
    txManual: kind === "console" && !!c.options?.txManual,
    inTx: false,
  };
};

// ───────── Closing tabs / splits ─────────
const closeTabs = async (ids: string[]) => {
  if (!ids.length) return;
  // Closing rolls back → ask if there are uncommitted changes
  const pending = useTabsStore.getState().tabs.filter((t) => ids.includes(t.id) && t.inTx);
  if (pending.length && !(await ask(`${pending.length === 1 ? `"${pending[0].title}" has` : `${pending.length} consoles have`} uncommitted changes. Roll them back and close?`, { okLabel: "Roll Back and Close", danger: true }))) return;
  for (const t of useTabsStore.getState().tabs.filter((x) => ids.includes(x.id))) useTabsStore.setState({ closedStack: [snapshot(t), ...useTabsStore.getState().closedStack].slice(0, 30) });
  useTabsStore.setState({ mru: useTabsStore.getState().mru.filter((x) => !ids.includes(x)) });
  for (const id of ids) {
    forgetConsoleModel(id);
    editorApis.delete(id);
    liveSql.delete(id);
    api.closeSession(id).catch(() => {});
  }
  setTabs((ts) => {
    const rest = ts.filter((t) => !ids.includes(t.id));
    settle(rest, useTabsStore.getState().panes);
    return rest;
  });
};

/** After tabs change: remove empty panes (a neighbor fills in) and sync each pane's shown tab and focus. show = show that tab in that pane */
const settle = (next: Tab[], pane: Pane, show?: { g: Group; id: string }) => {
  const p = prunePanes(pane, new Set(next.map((t) => t.group)));
  const ls = leaves(p);
  setPanes(p);
  setActiveIds((a) =>
    a.map((cur, g) => {
      if (!ls.includes(g)) return null;
      if (show?.g === g) return show.id;
      if (cur && next.some((t) => t.id === cur && t.group === g)) return cur;
      return useTabsStore.getState().mru.find((id) => next.some((t) => t.id === id && t.group === g)) ?? last(next.filter((t) => t.group === g))?.id ?? null;
    }),
  );
  setFocused((f) => (show && ls.includes(show.g) ? show.g : ls.includes(f) ? f : ls[0]));
};

/** Move a tab to another pane */
const moveTo = (id: string, g: Group) =>
  setTabs((ts) => {
    const next = ts.map((t) => (t.id === id ? { ...t, group: g } : t));
    settle(next, useTabsStore.getState().panes, { g, id });
    return next;
  });

const togglePin = (t: Tab) => {
  setTabs((ts) => {
    const flipped = ts.map((x) => (x.id === t.id ? { ...x, pinned: !x.pinned } : x));
    // Pinned tabs go to the left within the group (Keep pinned tabs on the left)
    return [...flipped.filter((x) => x.pinned), ...flipped.filter((x) => !x.pinned)];
  });
};

export { allTabs, buildTab, closeTabs, moveTo, patch, remember, settle, snapshot, sqlOf, togglePin, updStashed, updTab };
