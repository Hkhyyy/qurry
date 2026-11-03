import { ConnectionView, dialectOf } from "../../lib/api";
import type { WorkspaceInfo } from "../../components/WorkspaceBar";
import { leaf, leaves, MAX_PANES, Pane, prunePanes } from "../../lib/panes";
import { buildTab, sqlOf } from "../tabs/ops";
import { setActiveIds, setFocused, setPanes, setTabs, useTabsStore } from "../tabs/store";
import { emptyLayout, Tab, WsLayout } from "../tabs/types";
import { setCurWs, setWorkspaces, stash, useWorkspacesStore } from "./store";

// ───────── workspace-gui.json: what is saved, and how it comes back on relaunch ─────────
/** What to save: tabs (console SQL, table WHERE/ORDER BY, DDL, Redis, DBA report, diagram schemas), splits, the tab shown in each pane, and focus */
const wsJson = () => {
  const ser = (l: WsLayout) => ({
    tabs: l.tabs.map((t) => ({
      kind: t.kind,
      title: t.title,
      connId: t.connId,
      database: t.database,
      group: t.group,
      sql: t.kind === "console" || t.kind === "ddl" ? sqlOf(t) : "",
      where: t.where,
      orderBy: t.orderBy,
      edit: t.edit ? { schema: t.edit.schema, table: t.edit.table } : undefined,
      pinned: t.pinned,
      redisKey: t.redisKey,
      dbaReport: t.dbaReport,
      erdSchemas: t.erdSchemas,
      objKind: t.objKind,
      ddlOf: t.ddlOf,
    })),
    panes: l.panes,
    activeIds: l.activeIds.map((id) => l.tabs.findIndex((t) => t.id === id)),
    focused: l.focused,
  });
  const live: WsLayout = { tabs: useTabsStore.getState().tabs, panes: useTabsStore.getState().panes, activeIds: useTabsStore.getState().activeIds, focused: useTabsStore.getState().focused, mru: [] };
  return JSON.stringify({
    version: 2,
    // The workspace list has been synced with the file shared with the TUI (missing ones were deleted on the other side)
    shared: true,
    current: useWorkspacesStore.getState().curWs,
    workspaces: useWorkspacesStore.getState().workspaces.map((w) => ({ id: w.id, name: w.name, connIds: w.connIds, ...ser(w.id === useWorkspacesStore.getState().curWs ? live : stash.get(w.id) ?? emptyLayout()) })),
  });
};
/** One saved workspace → tabs and split view (tabs load when selected) */
const buildLayout = (ws: any, conns: ConnectionView[]): WsLayout => {
  if (!ws || !Array.isArray(ws.tabs)) return emptyLayout();
  const okPane = (p: any): boolean => p && (p.t === "leaf" ? Number.isInteger(p.g) && p.g >= 0 && p.g < MAX_PANES : p.t === "split" && okPane(p.a) && okPane(p.b));
  const pane0: Pane = okPane(ws.panes) ? ws.panes : leaf(0);
  const ls0 = leaves(pane0);
  const built: (Tab | null)[] = ws.tabs.map((x: any) => {
    const c = conns.find((cc) => cc.id === x.connId);
    // Also restore DBA report, diagram, compare and RAG tabs (they load themselves when first shown)
    if (!c || !["console", "table", "ddl", "redis", "dba", "erd", "migrate", "rag"].includes(x.kind)) return null;
    const group = ls0.includes(x.group) ? x.group : ls0[0];
    let tableSql: string | undefined;
    if (x.kind === "table" && x.edit) {
      const { q } = dialectOf(c.kind);
      tableSql = `SELECT * FROM ${q(x.edit.schema)}.${q(x.edit.table)}${x.where ? ` WHERE ${x.where}` : ""}${x.orderBy ? ` ORDER BY ${x.orderBy}` : ""};`;
    }
    const t = buildTab(c, x.kind, { sql: x.kind === "table" ? tableSql : x.sql, database: x.database, tableSql, edit: x.edit ? { ...x.edit, pk: [] } : undefined, where: x.where, orderBy: x.orderBy, objKind: typeof x.objKind === "string" ? x.objKind : undefined, ddlOf: typeof x.ddlOf?.schema === "string" && typeof x.ddlOf?.name === "string" ? { schema: x.ddlOf.schema, name: x.ddlOf.name } : undefined }, group, x.title);
    return { ...t, pinned: x.pinned, redisKey: x.redisKey, dbaReport: x.dbaReport, erdSchemas: x.erdSchemas, stale: x.kind !== "ddl" };
  });
  const ts = built.filter((t): t is Tab => !!t);
  if (!ts.length) return emptyLayout();
  const pane = prunePanes(pane0, new Set(ts.map((t) => t.group)));
  const ls = leaves(pane);
  const act = [0, 1, 2, 3].map((g) => {
    if (!ls.includes(g)) return null;
    const want = built[ws.activeIds?.[g]];
    return want && want.group === g ? want.id : ts.find((t) => t.group === g)?.id ?? null;
  });
  return { tabs: ts, panes: pane, activeIds: act, focused: ls.includes(ws.focused) ? ws.focused : ls[0], mru: act.filter((x): x is string => !!x) };
};
/** Show this tab and split view. Only the tab shown in each pane is loaded (the rest when selected) */
const showLayout = (l: WsLayout, loadStale: (t: Tab) => void) => {
  setTabs(l.tabs);
  setPanes(l.panes);
  setActiveIds(l.activeIds);
  setFocused(l.focused);
  useTabsStore.setState({ mru: l.mru });
  for (const id of l.activeIds) {
    const t = l.tabs.find((x) => x.id === id);
    if (t?.stale) loadStale(t);
  }
};
/** Restore saved tabs and workspaces. Returns whether the list has been synced with the shared workspace list */
const restoreWs = (data: any, conns: ConnectionView[], loadStale: (t: Tab) => void): boolean => {
  // version 1 (before workspaces) = a single workspace
  const list: any[] | null = Array.isArray(data?.workspaces) ? data.workspaces : Array.isArray(data?.tabs) ? [{ id: "ws1", name: "Default", ...data }] : null;
  if (!list?.length) return false;
  // Saved file without explorer lists (before 0.2.0 and 0.2.0): the first workspace gets all Data Sources
  const hasLists = list.some((w) => Array.isArray(w.connIds));
  const infos: WorkspaceInfo[] = list.map((w, i) => ({
    id: typeof w.id === "string" && w.id ? w.id : `ws${i + 1}`,
    name: typeof w.name === "string" && w.name.trim() ? w.name : `Workspace ${i + 1}`,
    connIds: hasLists ? (Array.isArray(w.connIds) ? w.connIds.filter((x: unknown) => typeof x === "string") : []) : i === 0 ? conns.map((c) => c.id) : [],
  }));
  const cur = infos.find((w) => w.id === data.current)?.id ?? infos[0].id;
  stash.clear();
  list.forEach((w, i) => {
    const l = buildLayout(w, conns);
    if (infos[i].id === cur) showLayout(l, loadStale);
    else stash.set(infos[i].id, l);
  });
  setWorkspaces(infos);
  setCurWs(cur);
  return data?.shared === true;
};

export { buildLayout, restoreWs, showLayout, wsJson };
