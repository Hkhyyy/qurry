import { useEffect, useRef, useState } from "react";
import { api, ConnectionView } from "../../lib/api";
import { ask, askText } from "../../lib/dialogs";
import { forgetConsoleModel } from "../../lib/monaco";
import { leaves } from "../../lib/panes";
import type { WorkspaceInfo } from "../../components/WorkspaceBar";
import { allTabs, settle } from "../tabs/ops";
import { editorApis, liveSql, setTabs, useTabsStore } from "../tabs/store";
import { emptyLayout, Tab } from "../tabs/types";
import { restoreWs, showLayout, wsJson } from "./layout";
import { setCurWs, setWorkspaces, stash, useWorkspacesStore } from "./store";

/** Workspaces: restore and save the open tabs, sync the workspace list shared with the TUI, switch · create · rename · delete, move a tab to another one */
export function useWorkspaces({ conns, setStatus, loadStale }: { conns: ConnectionView[]; setStatus: (s: string) => void; loadStale: (t: Tab) => void }) {
  const tabs = useTabsStore((s) => s.tabs);
  const activeIds = useTabsStore((s) => s.activeIds);
  const panes = useTabsStore((s) => s.panes);
  const focused = useTabsStore((s) => s.focused);
  const workspaces = useWorkspacesStore((s) => s.workspaces);
  const curWs = useWorkspacesStore((s) => s.curWs);
  // ───────── Restoring on relaunch (workspace-gui.json) ─────────
  const restored = useRef(false);
  const savedWs = useRef("");
  /** Before closing the app or relaunching for an update: ask about uncommitted changes and save the open tabs */
  const readyToQuit = async () => {
    const n = allTabs().filter((t) => t.inTx).length;
    if (n && !(await ask(`${n} console(s) have uncommitted changes. Quit and roll them back?`, { okLabel: "Quit and Roll Back", danger: true }))) return false;
    saveWs();
    return true;
  };
  const saveWs = () => {
    if (!restored.current) return;
    const j = wsJson();
    if (j === savedWs.current) return;
    savedWs.current = j;
    api.saveWorkspace(JSON.parse(j)).catch(() => {});
  };

  // ── The workspace list (name · Data Sources) is shared with the TUI: send only what changed, and pull in what the other side changed ──
  /** List last synced with the shared file (null = not yet) */
  const sharedPrev = useRef<WorkspaceInfo[] | null>(null);
  const sameWs = (a: WorkspaceInfo[], b: WorkspaceInfo[]) => JSON.stringify(a.map((w) => [w.id, w.name, w.connIds])) === JSON.stringify(b.map((w) => [w.id, w.name, w.connIds]));
  const wsTabs = (id: string) => (id === useWorkspacesStore.getState().curWs ? useTabsStore.getState().tabs : stash.get(id)?.tabs ?? []);
  /** Apply the shared list to the UI. Close the tabs of workspaces deleted on the other side (keep them if a console has uncommitted changes — they come back on the next sync) */
  const applyShared = (list: WorkspaceInfo[]) => {
    const cur = useWorkspacesStore.getState().workspaces;
    const gone = cur.filter((w) => !list.some((x) => x.id === w.id));
    const keep = gone.filter((w) => wsTabs(w.id).some((t) => t.inTx));
    const next = [...list.map((w) => ({ id: w.id, name: w.name, connIds: w.connIds })), ...keep];
    if (!next.length) return;
    const closing = new Map(gone.filter((x) => !keep.includes(x)).map((w) => [w.id, wsTabs(w.id)]));
    setWorkspaces(next);
    if (!next.some((w) => w.id === useWorkspacesStore.getState().curWs)) switchWs(next[0].id, true);
    for (const [id, ts] of closing) {
      stash.delete(id);
      for (const t of ts) {
        forgetConsoleModel(t.id);
        editorApis.delete(t.id);
        liveSql.delete(t.id);
        api.closeSession(t.id).catch(() => {});
      }
    }
    if (keep.length) setStatus(`Kept workspace ${keep.map((w) => `"${w.name}"`).join(", ")} — it was deleted elsewhere but has uncommitted changes`);
  };
  /** At first launch: match this app's list to the shared list (the first time, pair up ones with the same name) */
  const adoptShared = async (synced: boolean) => {
    const a = await api.adoptWorkspaces(useWorkspacesStore.getState().workspaces, synced);
    for (const [from, to] of a.renamed) {
      const l = stash.get(from);
      if (l) {
        stash.delete(from);
        stash.set(to, l);
      }
      if (useWorkspacesStore.getState().curWs === from) {
        setCurWs(to);
      }
      setWorkspaces((l) => l.map((w) => (w.id === from ? { ...w, id: to } : w)));
    }
    applyShared(a.list);
    sharedPrev.current = a.list;
  };
  // When changed in this app, write only the changes to the shared file
  useEffect(() => {
    const prev = sharedPrev.current;
    if (!prev || sameWs(prev, workspaces)) return;
    const sent = workspaces;
    sharedPrev.current = sent;
    api
      .syncWorkspaces(prev, sent)
      .then((list) => {
        // If it changed again meanwhile, leave it to the next sync
        if (useWorkspacesStore.getState().workspaces !== sent) return;
        sharedPrev.current = list;
        if (!sameWs(list, sent)) applyShared(list);
      })
      .catch((e) => setStatus(`Workspaces: ${String(e)}`));
  }, [workspaces]);
  // Changes from the other side (TUI): when the window regains focus · every few seconds
  useEffect(() => {
    const pull = () => {
      const prev = sharedPrev.current;
      if (!prev || !sameWs(useWorkspacesStore.getState().workspaces, prev)) return;
      api
        .sharedWorkspaces()
        .then((list) => {
          if (!list?.length || sharedPrev.current !== prev || sameWs(list, prev)) return;
          sharedPrev.current = list;
          applyShared(list);
        })
        .catch(() => {});
    };
    const h = setInterval(pull, 3000);
    window.addEventListener("focus", pull);
    return () => {
      clearInterval(h);
      window.removeEventListener("focus", pull);
    };
  }, []);
  /** Switch to another workspace: keep the current one as is (console sessions and transactions too). discard = don't keep the current one (when deleting) */
  const switchWs = (id: string, discard = false) => {
    const from = useWorkspacesStore.getState().curWs;
    if (id === from || !useWorkspacesStore.getState().workspaces.some((w) => w.id === id)) return;
    const target = stash.get(id) ?? emptyLayout();
    stash.delete(id);
    if (!discard) stash.set(from, { tabs: useTabsStore.getState().tabs, panes: useTabsStore.getState().panes, activeIds: useTabsStore.getState().activeIds, focused: useTabsStore.getState().focused, mru: useTabsStore.getState().mru });
    showLayout(target, loadStale);
    setCurWs(id);
  };
  const newWs = async () => {
    const name = (await askText("Workspace name", `Workspace ${useWorkspacesStore.getState().workspaces.length + 1}`));
    if (!name?.trim()) return;
    const w = { id: crypto.randomUUID(), name: name.trim(), connIds: [] };
    setWorkspaces((l) => [...l, w]);
    switchWs(w.id);
  };
  const renameWs = async (id: string) => {
    const w = useWorkspacesStore.getState().workspaces.find((x) => x.id === id);
    const name = w && (await askText("Rename workspace", w.name));
    if (name?.trim()) setWorkspaces((l) => l.map((x) => (x.id === id ? { ...x, name: name.trim() } : x)));
  };
  /** Close a workspace and its tabs (uncommitted changes are rolled back) */
  const deleteWs = async (id: string) => {
    const list = useWorkspacesStore.getState().workspaces;
    const w = list.find((x) => x.id === id);
    if (!w || list.length < 2) return;
    const ts = id === useWorkspacesStore.getState().curWs ? useTabsStore.getState().tabs : stash.get(id)?.tabs ?? [];
    const dirty = ts.filter((t) => t.inTx).length;
    if (!(await ask(`Delete the workspace "${w.name}"${ts.length ? ` and close its ${ts.length} tab(s)` : ""}?${dirty ? `\n\n${dirty} console(s) have uncommitted changes — they are rolled back.` : ""}`, { okLabel: "Delete", danger: true }))) return;
    if (id === useWorkspacesStore.getState().curWs) switchWs(list.find((x) => x.id !== id)!.id, true);
    stash.delete(id);
    for (const t of ts) {
      forgetConsoleModel(t.id);
      editorApis.delete(t.id);
      liveSql.delete(t.id);
      api.closeSession(t.id).catch(() => {});
    }
    setWorkspaces((l) => l.filter((x) => x.id !== id));
  };
  /** Move a tab to another workspace (session kept). Goes at the end of that workspace's first pane */
  const moveToWs = (t: Tab, id: string) => {
    const target = stash.get(id) ?? emptyLayout();
    const g = leaves(target.panes)[0];
    stash.set(id, { ...target, tabs: [...target.tabs, { ...t, group: g }], activeIds: target.activeIds.map((x, i) => (i === g ? t.id : x)), mru: [t.id, ...target.mru] });
    useTabsStore.setState({ mru: useTabsStore.getState().mru.filter((x) => x !== t.id) });
    setTabs((ts) => {
      const rest = ts.filter((x) => x.id !== t.id);
      settle(rest, useTabsStore.getState().panes);
      return rest;
    });
    setStatus(`Moved "${t.title}" to ${useWorkspacesStore.getState().workspaces.find((w) => w.id === id)?.name}`);
  };
  useEffect(() => {
    if (restored.current || !conns.length) return;
    restored.current = true;
    api
      .loadWorkspace()
      .then((data) => restoreWs(data, conns, loadStale))
      .catch(() => false)
      .then((synced) => adoptShared(synced))
      .catch((e) => setStatus(`Workspaces: ${String(e)}`))
      .finally(() => setWsReady(true));
  }, [conns]);
  // Data Sources in no workspace (just created, or created in the TUI) → into the current workspace
  const [wsReady, setWsReady] = useState(false);
  useEffect(() => {
    if (!wsReady) return;
    const known = new Set(useWorkspacesStore.getState().workspaces.flatMap((w) => w.connIds));
    const orphans = conns.map((c) => c.id).filter((id) => !known.has(id));
    if (orphans.length) setWorkspaces((l) => l.map((w) => (w.id === useWorkspacesStore.getState().curWs ? { ...w, connIds: [...w.connIds, ...orphans] } : w)));
  }, [conns, wsReady]);
  /** Add or remove a Data Source in the workspace explorer (the Data Source itself is untouched) */
  const setInWs = (wsId: string, connId: string, on: boolean) =>
    setWorkspaces((l) => l.map((w) => (w.id !== wsId ? w : { ...w, connIds: on ? [...w.connIds.filter((x) => x !== connId), connId] : w.connIds.filter((x) => x !== connId) })));
  /** Workspaces whose explorer has been shown (once shown, hide instead of unmounting to keep the expanded state) */
  const [shownWs, setShownWs] = useState<string[]>([]);
  useEffect(() => setShownWs((l) => (l.includes(curWs) ? l : [...l, curWs])), [curWs]);
  // Save on change (console SQL is not state, so it is also checked periodically)
  useEffect(() => {
    const h = setTimeout(saveWs, 800);
    return () => clearTimeout(h);
  }, [tabs, panes, activeIds, focused, workspaces, curWs]);
  useEffect(() => {
    const h = setInterval(saveWs, 5000);
    return () => clearInterval(h);
  }, []);
  return { readyToQuit, switchWs, newWs, renameWs, deleteWs, moveToWs, setInWs, shownWs };
}
