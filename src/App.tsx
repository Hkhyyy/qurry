import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { api, ConnectionView, meta, targetId, updateCheckEnabled } from "./lib/api";
import { KeymapDialog } from "./components/KeymapDialog";
import { UpdateDialog } from "./components/UpdateDialog";
import { ContextMenu, Explorer, MenuItem } from "./components/Explorer";
import { ConnectionDialog } from "./components/ConnectionDialog";
import { HistoryDialog, migrateOldHistory, saveSnippet } from "./components/HistoryDialog";
import { ProductionLog } from "./components/ProductionLog";
import { CopyRowsDialog, GenerateDataDialog } from "./components/DataTools";
import { NewObjectDialog, TableDesigner } from "./components/ObjectEditors";
import { WorkspaceBar } from "./components/WorkspaceBar";
import { TextDialog } from "./components/TextDialog";
import { Badge, CodeBlock, ConfirmHost, Overlay, Text, Toolbar, ToolbarSep } from "./ui";
import { RecentFiles, Switcher, SwitchItem } from "./components/Switcher";
import { ActionItem, DbObject, invalidateSearchIndex, SearchEverywhere, SeTab } from "./components/SearchEverywhere";
import { FindPanel, FindRun, FtsScope, FullTextSearchDialog } from "./components/FullTextSearch";
import { DEFAULT_FONT, editorFontSize, setEditorFontSize } from "./lib/prefs";
import { eventSpec, hit, loadKeymap, onKeymapChange } from "./lib/shortcuts";
import { invalidateCompletion } from "./lib/completion";
import { useSharedReady } from "./lib/shared";
import { tableTabLabels } from "./lib/tabLabels";
import type { FtsRequest } from "./lib/api";
import { isRelation, NodeInfo, revealInTree, schemaChanged } from "./components/SchemaTree";
import { leaves } from "./lib/panes";
import { getThemePref, setThemePref, THEME_LABEL, ThemePref, useTheme } from "./lib/theme";
import { BackupDialog, ExportDialog, ImportDialog } from "./components/Transfer";
import { Group, snapKey, Tab } from "./features/tabs/types";
import { isMac, sk } from "./features/shortcuts/labels";
import { offered, SettingsDialog, upToDate } from "./features/settings/SettingsDialog";
import { StatusBar } from "./features/settings/StatusBar";
import { editorApis, useTabsStore } from "./features/tabs/store";
import { closeTabs } from "./features/tabs/ops";
import { stash, useWorkspacesStore } from "./features/workspaces/store";
import { useWorkspaces } from "./features/workspaces/useWorkspaces";
import { useConsole } from "./features/console/useConsole";
import { useTabs } from "./features/tabs/useTabs";
import { useJobs } from "./features/jobs/useJobs";
import { useConnActions } from "./features/conns/useConnActions";
import { useExplorer } from "./features/explorer/useExplorer";
import { useEditorGroups } from "./features/editorGroups/useEditorGroups";
import { setBackupReq, setCopyReq, setDialog, setExportReq, setGenReq, setHistoryFor, setHistoryTab, setImportReq, setKeymapOpen, setObjEdit, setProdLog, setQuickDoc, setSchemeOpen, setSettingsOpen, setUpdate, setUpdateOpen, useDialogsStore } from "./features/dialogs/store";
import "./App.css";

export default function App() {
  const theme = useTheme();
  // Redraw the menu shortcut labels when the keymap changes (or the WASM module that draws them has loaded)
  const [keymapVer, setKeymapVer] = useState(0);
  useEffect(() => {
    const off = onKeymapChange(() => setKeymapVer((n) => n + 1));
    void loadKeymap();
    return () => void off();
  }, []);
  const settingsOpen = useDialogsStore((s) => s.settingsOpen);
  const schemeOpen = useDialogsStore((s) => s.schemeOpen);
  const keymapOpen = useDialogsStore((s) => s.keymapOpen);
  // On window close, ask if there are uncommitted changes (closing rolls them back)
  useEffect(() => {
    let off: (() => void) | undefined;
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().onCloseRequested(async (e) => {
          if (!(await readyToQuit())) e.preventDefault();
        }),
      )
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  // New version (checked at startup + every hour; failures, and a check that was skipped, are ignored silently and keep the badge)
  const update = useDialogsStore((s) => s.update);
  const updateOpen = useDialogsStore((s) => s.updateOpen);
  // An offer that went away closes its window, so a later offer doesn't open it unasked
  useEffect(() => {
    if (!update) setUpdateOpen(false);
  }, [update]);
  useEffect(() => {
    const check = () => {
      if (!updateCheckEnabled()) return;
      api
        .checkUpdate(false)
        .then((u) => setUpdate(offered(u)))
        .catch(() => {});
    };
    check();
    const t = setInterval(check, 60 * 60 * 1000);
    return () => clearInterval(t);
  }, []);
  const [conns, setConns] = useState<ConnectionView[]>([]);
  const tabs = useTabsStore((s) => s.tabs);
  const activeIds = useTabsStore((s) => s.activeIds);
  const panes = useTabsStore((s) => s.panes);
  const split = panes.t === "split";
  const focused = useTabsStore((s) => s.focused);
  const workspaces = useWorkspacesStore((s) => s.workspaces);
  const curWs = useWorkspacesStore((s) => s.curWs);
  const dialog = useDialogsStore((s) => s.dialog);
  const [status, setStatus] = useState("Ready");
  const [treeSel, setTreeSel] = useState<NodeInfo | null>(null);
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const historyFor = useDialogsStore((s) => s.historyFor);
  const historyTab = useDialogsStore((s) => s.historyTab);
  const prodLog = useDialogsStore((s) => s.prodLog);

  // Called on every query run, so only update state when something actually changed (avoids redrawing the whole tree and re-registering shortcut handlers)
  const connsJson = useRef("");
  const refresh = useCallback(async () => {
    try {
      const cs = await api.listConnections();
      const j = JSON.stringify(cs);
      if (j !== connsJson.current) {
        connsJson.current = j;
        setConns(cs);
      }
    } catch (e) {
      setStatus(String(e));
    }
  }, []);
  useEffect(() => {
    refresh();
  }, [refresh]);
  // Qurry › Check for Updates…
  useEffect(() => {
    let off: (() => void) | undefined;
    listen("check-update", async () => {
      setStatus("Checking for updates…");
      try {
        const u = await api.checkUpdate(true);
        setUpdate(offered(u));
        if (u.latest) {
          setUpdateOpen(true);
          setStatus("");
        } else setStatus(upToDate(u));
      } catch (e) {
        setStatus(String(e));
      }
    })
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  // Help menu › Keymap Reference
  useEffect(() => {
    let off: (() => void) | undefined;
    listen("open-keymap", () => setKeymapOpen(true))
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  // Qurry › Settings…
  useEffect(() => {
    let off: (() => void) | undefined;
    listen("open-settings", () => setSettingsOpen("appearance"))
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  useEffect(migrateOldHistory, []);
  // Update the tree state when the backend closes a connection (e.g. Auto-disconnect)
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<string>("conn-disconnected", (e) => {
      invalidateSearchIndex(e.payload);
      invalidateCompletion(e.payload);
      refresh();
      setStatus(`Disconnected (idle): ${e.payload}`);
    })
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, [refresh]);
  useEffect(() => {
    const onErr = (e: ErrorEvent) => setStatus(`Error: ${e.message}`);
    const onRej = (e: PromiseRejectionEvent) => setStatus(`Error: ${String(e.reason)}`);
    const closeMenu = () => setTabMenu(null);
    window.addEventListener("error", onErr);
    window.addEventListener("unhandledrejection", onRej);
    window.addEventListener("mousedown", closeMenu);
    return () => {
      window.removeEventListener("error", onErr);
      window.removeEventListener("unhandledrejection", onRej);
      window.removeEventListener("mousedown", closeMenu);
    };
  }, []);

  const groupActive = (g: Group) => tabs.find((t) => t.id === activeIds[g] && t.group === g) ?? null;
  const active = groupActive(focused) ?? leaves(panes).map(groupActive).find(Boolean) ?? null;
  const activeConn = conns.find((c) => c.id === active?.connId);
  // Several table tabs with the same name → schema.table (db.schema.table if the schema is also the same)
  const wasmReady = useSharedReady();
  const tabLabels = useMemo(
    () =>
      tableTabLabels(
        tabs.flatMap((t) => (t.kind === "table" && t.edit ? [{ id: t.id, db: t.database || conns.find((c) => c.id === t.connId)?.database || "", schema: t.edit.schema, table: t.edit.table }] : [])),
      ),
    [tabs, conns, wasmReady],
  );
  const labelOf = (t: Tab) => tabLabels.get(t.id) ?? t.title;

  const cons = useConsole({ conns, setStatus, refresh });
  const { run, setTxMode, txEnd, applyTableFilter, canCancel, cancelRun, loadStale } = cons;
  const tabOps = useTabs({ setStatus, loadStale });
  const { activate, addTab, openConsole } = tabOps;
  const ws = useWorkspaces({ conns, setStatus, loadStale });
  const { readyToQuit, switchWs, newWs, renameWs, deleteWs, setInWs, shownWs } = ws;

  const exportReq = useDialogsStore((s) => s.exportReq);
  const importReq = useDialogsStore((s) => s.importReq);
  const genReq = useDialogsStore((s) => s.genReq);
  const objEdit = useDialogsStore((s) => s.objEdit);
  const copyReq = useDialogsStore((s) => s.copyReq);
  const transfer = useJobs({ setStatus });
  const { jobs, runJob, startExport, startImport } = transfer;
  const connActions = useConnActions({ conns, refresh, setStatus });
  const { setColor, setScope, exportConns, importConns, removeConn, refreshAll } = connActions;
  const ex = useExplorer({ setStatus, refresh, activate, addTab, openConsole, run, runJob, removeConn, setInWs, setTabMenu });
  const { openTableTab, openDdl, activateNode, showQuickDoc, jumpToConsole, deactivate, duplicateConn, canDdl, modifyTable, renameNode, buildMenu, sqlGenerator, ddlToClipboard, ddlToConsole, dangerous, startBackup } = ex;
  const quickDoc = useDialogsStore((s) => s.quickDoc);
  const backupReq = useDialogsStore((s) => s.backupReq);

  const [maximized, setMaximized] = useState(false);
  const app = { conns, theme, setStatus, refresh, treeSel, active, groupActive, labelOf, setTabMenu, setMaximized, ...cons, ...tabOps, ...ws, ...transfer, ...connActions, ...ex };
  const groups = useEditorGroups(app);
  const { renderPanes, splitClone, unsplit, reopen, reopenClosed, renameConsole } = groups;

  // ───────── Switcher / Recent Files / Maximize ─────────
  const [switcher, setSwitcher] = useState<{ items: SwitchItem[]; start: number } | null>(null);
  const [recentOpen, setRecentOpen] = useState(false);
  const kindIcon = (k: Tab["kind"]) => (k === "table" ? "▦" : k === "ddl" ? "ƒ" : k === "redis" ? "◆" : "⌘");
  const hintOf = (connId: string, db?: string) => {
    const c = conns.find((x) => x.id === connId);
    return c ? c.name + (db ? ` · ${db}` : "") : "";
  };
  const openSwitcher = (back: boolean) => {
    const ts = useTabsStore.getState().tabs;
    const ordered = [...useTabsStore.getState().mru.map((id) => ts.find((t) => t.id === id)).filter((t): t is Tab => !!t), ...ts.filter((t) => !useTabsStore.getState().mru.includes(t.id))];
    if (!ordered.length) return;
    const items = ordered.map((t) => ({ key: t.id, icon: kindIcon(t.kind), title: labelOf(t), hint: hintOf(t.connId, t.database), color: conns.find((c) => c.id === t.connId)?.color }));
    setSwitcher({ items, start: back ? items.length - 1 : 1 });
  };
  const recentItems = (): SwitchItem[] => {
    const openKeys = new Map(useTabsStore.getState().tabs.map((t) => [snapKey(t), t] as const));
    return useTabsStore.getState().recent
      .filter((r) => conns.some((c) => c.id === r.connId))
      .map((r) => ({ key: r.key, icon: kindIcon(r.kind), title: r.title, hint: hintOf(r.connId, r.database), open: openKeys.has(r.key), color: conns.find((c) => c.id === r.connId)?.color }));
  };

  // ───────── Search Everywhere (⇧⇧) / Go to Database Object (⌘o) / Find Action (⇧⌘A) ─────────
  const [se, setSe] = useState<SeTab | null>(null);

  // ── Tool windows: Database Explorer (⌘0), Find (⇧⌘F) ──
  const [explorerHidden, setExplorerHidden] = useState(false);
  const toggleExplorer = () => {
    const tree = document.querySelector<HTMLElement>(".ws-explorer.on [data-tree]");
    const focusedInTree = !!document.activeElement?.closest?.("[data-tree]");
    if (explorerHidden || maximized) {
      setExplorerHidden(false);
      setMaximized(false);
      setTimeout(() => document.querySelector<HTMLElement>(".ws-explorer.on [data-tree]")?.focus(), 50);
    } else if (focusedInTree) setExplorerHidden(true);
    else tree?.focus();
  };
  const [findOpen, setFindOpen] = useState(false);
  const [findRuns, setFindRuns] = useState<FindRun[]>([]);
  const [findActive, setFindActive] = useState<string | null>(null);
  const [ftsScope, setFtsScope] = useState<FtsScope | null>(null);
  const openFts = async () => {
    const sel = treeSel;
    const c = sel?.conn ?? conns.find((x) => x.id === active?.connId);
    if (!c) return setStatus("Select a data source, database, schema or table to search in");
    const db = sel?.db ?? active?.database ?? (c.database || "postgres");
    if (sel && isRelation(sel.kind) && sel.schema && sel.name)
      return setFtsScope({ connId: c.id, connName: c.name, db, schemas: [], tables: [[sel.schema, sel.name]] });
    if (sel?.kind === "schema" && sel.schema) return setFtsScope({ connId: c.id, connName: c.name, db, schemas: [sel.schema], tables: [] });
    try {
      const schemas = (await meta.schemas(targetId(c.id, db))).filter((s) => !s.isSystem).map((s) => s.name);
      setFtsScope({ connId: c.id, connName: c.name, db, schemas, tables: [] });
    } catch (e) {
      setStatus(String(e));
    }
  };
  const runFts = async (scope: FtsScope, req: FtsRequest) => {
    setFtsScope(null);
    const id = crypto.randomUUID();
    const title = `"${req.text}"`;
    setFindRuns((rs) => [...rs.filter((r) => r.pinned), { id, title, scope, running: true, hits: [], text: req.text, matchCase: req.matchCase }]);
    setFindActive(id);
    setFindOpen(true);
    try {
      const hits = await meta.fullTextSearch(targetId(scope.connId, scope.db), req);
      setFindRuns((rs) => rs.map((r) => (r.id === id ? { ...r, running: false, hits } : r)));
      setStatus(`Full-text search: ${hits.reduce((n, h) => n + h.result.rows.length, 0)} matches in ${hits.length} tables`);
    } catch (e) {
      setFindRuns((rs) => rs.map((r) => (r.id === id ? { ...r, running: false, error: String(e) } : r)));
    }
  };

  // ── Editor font size ──
  const [, setFontTick] = useState(editorFontSize());
  const bumpFont = (d: number) => setFontTick(setEditorFontSize(d === 0 ? DEFAULT_FONT : editorFontSize() + d));
  const objNode = (o: DbObject): NodeInfo | null => {
    const conn = conns.find((c) => c.id === o.connId);
    if (!conn) return null;
    const kind = (o.kind === "function" || o.kind === "procedure" ? "routine" : o.kind) as NodeInfo["kind"];
    return { kind, conn, db: o.db, schema: o.schema, name: o.kind === "schema" ? undefined : o.name };
  };
  const seFiles = () => {
    const openKeys = new Map(useTabsStore.getState().tabs.map((t) => [snapKey(t), t] as const));
    const seen = new Set<string>();
    const items = [];
    for (const id of useTabsStore.getState().mru) {
      const t = useTabsStore.getState().tabs.find((x) => x.id === id);
      if (t) {
        seen.add(snapKey(t));
        items.push({ key: snapKey(t), icon: kindIcon(t.kind), title: t.title, hint: hintOf(t.connId, t.database), open: true });
      }
    }
    for (const r of useTabsStore.getState().recent)
      if (!seen.has(r.key) && conns.some((c) => c.id === r.connId))
        items.push({ key: r.key, icon: kindIcon(r.kind), title: r.title, hint: hintOf(r.connId, r.database), open: openKeys.has(r.key) });
    return items;
  };
  const seActions = (): ActionItem[] => {
    const t = active;
    const c = treeSel?.conn ?? conns.find((x) => x.id === t?.connId);
    return [
      { name: "New Query Console", shortcut: sk("console.new"), group: "File", run: () => c && openConsole(c, treeSel?.db ?? t?.database), disabled: !c },
      { name: "New Data Source…", group: "File", run: () => setDialog({}) },
      { name: "New Workspace…", group: "Window", run: () => newWs() },
      ...workspaces.map((w, i) => ({ name: `Workspace: ${w.name}`, shortcut: i < 9 ? sk(`ws.${i + 1}`) : undefined, group: "Window", run: () => switchWs(w.id), disabled: w.id === curWs })),
      { name: "Data Source Properties", shortcut: sk("tree.properties"), group: "File", run: () => c && setDialog({ edit: c }), disabled: !c },
      { name: "Export Data Sources…", group: "File", run: () => exportConns() },
      { name: "Import Data Sources…", group: "File", run: () => importConns() },
      { name: "Settings…", shortcut: sk("settings"), group: "File", run: () => setSettingsOpen("appearance") },
      { name: "Keymap Reference", shortcut: sk("keymap"), group: "Help", run: () => setKeymapOpen(true) },
      { name: "Edit Keymap…", group: "Help", run: () => setSettingsOpen("keymap") },
      { name: "Theme: Dark", group: "Appearance", run: () => setThemePref("dark") },
      { name: "Theme: Light", group: "Appearance", run: () => setThemePref("light") },
      { name: "Theme: High contrast", group: "Appearance", run: () => setThemePref("hc") },
      { name: "Theme: Sync with OS", group: "Appearance", run: () => setThemePref("system") },
      { name: "Quick Switch Theme…", shortcut: sk("theme"), group: "Appearance", run: () => setSchemeOpen(true) },
      { name: "Execute", shortcut: sk("editor.run"), group: "Run", run: () => t && editorApis.get(t.id)?.run(), disabled: t?.kind !== "console" },
      { name: "Explain Plan", group: "Run", run: () => t && editorApis.get(t.id)?.run("plan"), disabled: t?.kind !== "console" },
      { name: "Explain Analyse", group: "Run", run: () => t && editorApis.get(t.id)?.run("analyze"), disabled: t?.kind !== "console" },
      { name: "Cancel Running Statements", shortcut: sk("editor.cancel"), group: "Run", run: () => t && cancelRun(t), disabled: !canCancel(t) },
      { name: "Commit", group: "Transaction", run: () => t && txEnd(t, "commit"), disabled: !t?.inTx },
      { name: "Roll Back", group: "Transaction", run: () => t && txEnd(t, "rollback"), disabled: !t?.inTx },
      { name: "Transaction Mode: Manual", group: "Transaction", run: () => t && setTxMode(t, true), disabled: t?.kind !== "console" },
      { name: "Transaction Mode: Auto", group: "Transaction", run: () => t && setTxMode(t, false), disabled: t?.kind !== "console" },
      { name: "Browse Query History…", shortcut: sk("editor.history"), group: "Run", run: () => t && (setHistoryTab("history"), setHistoryFor(t.id)), disabled: t?.kind !== "console" },
      { name: "Snippets…", group: "Run", run: () => t && (setHistoryTab("snippets"), setHistoryFor(t.id)), disabled: t?.kind !== "console" },
      { name: "Production Log…", group: "Run", run: () => setProdLog({ connId: null }) },
      {
        name: "Save Query as Snippet…",
        group: "Run",
        run: async () => {
          const c = t && conns.find((x) => x.id === t.connId);
          const sql = t && editorApis.get(t.id)?.text();
          if (c && sql && (await saveSnippet(sql, c.id, c.name))) setStatus("Snippet saved (⌘k › Snippets…)");
        },
        disabled: t?.kind !== "console",
      },
      { name: "Go to Database Object…", shortcut: sk("goto.object"), group: "Navigate", run: () => setTimeout(() => setSe("database")) },
      { name: "Go to Symbol…", shortcut: sk("goto.symbol"), group: "Navigate", run: () => setTimeout(() => setSe("symbols")) },
      { name: "Go to File…", shortcut: sk("goto.file"), group: "Navigate", run: () => setTimeout(() => setSe("files")) },
      { name: "Full-text Search…", shortcut: sk("fts"), group: "Navigate", run: () => openFts() },
      { name: "Recent Files", shortcut: sk("recent"), group: "Navigate", run: () => setRecentOpen(true) },
      { name: "Switcher", shortcut: "⌃⇥", group: "Navigate", run: () => openSwitcher(false) },
      { name: "Reopen Closed Tab", shortcut: sk("tab.reopen"), group: "Window", run: reopenClosed },
      { name: "Close Tab", shortcut: sk("tab.close"), group: "Window", run: () => t && closeTabs([t.id]), disabled: !t },
      { name: "Close All Tabs", group: "Window", run: () => closeTabs(useTabsStore.getState().tabs.map((x) => x.id)) },
      { name: "Split Right", group: "Window", run: () => t && splitClone(t, "right"), disabled: !t },
      { name: "Split Down", group: "Window", run: () => t && splitClone(t, "down"), disabled: !t },
      { name: "Unsplit All", group: "Window", run: unsplit, disabled: !split },
      { name: "Maximize Editor", shortcut: sk("maximize"), group: "Window", run: () => setMaximized((m) => !m) },
      { name: "Database Explorer", shortcut: sk("explorer"), group: "Tool Windows", run: () => toggleExplorer() },
      { name: "Find (Full-text Search results)", shortcut: sk("find.window"), group: "Tool Windows", run: () => setFindOpen((o) => !o) },
      { name: "Refresh", shortcut: sk("refresh"), group: "Database", run: () => refreshAll() },
      { name: "Increase Editor Font Size", shortcut: sk("font.bigger"), group: "Editor", run: () => bumpFont(1) },
      { name: "Decrease Editor Font Size", shortcut: sk("font.smaller"), group: "Editor", run: () => bumpFont(-1) },
      { name: "Reset Editor Font Size", group: "Editor", run: () => bumpFont(0) },
    ];
  };

  // The key handler is registered only once, so pass the latest functions via a ref
  const latest = useRef({ toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs });
  latest.current = { toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs };
  const lastShift = useRef(0);

  // ───────── Shortcuts (DataGrip keymap) ─────────
  const keyState = useRef({ treeSel, active, focused, tabs, split });
  keyState.current = { treeSel, active, focused, tabs, split };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // If the Switcher / Recent Files popup is open, the popup handles keys
      if (document.querySelector(".switcher, .search-everywhere, .modal.fts, .modal.keymap, .keymap-recording")) return;
      const { treeSel, active, tabs } = keyState.current;
      const L = latest.current;
      // ⇧⇧: Search Everywhere
      if (e.key === "Shift") {
        if (!e.repeat) {
          const now = Date.now();
          if (now - lastShift.current < 400) {
            lastShift.current = 0;
            L.setSe("all");
          } else lastShift.current = now;
        }
        return;
      }
      lastShift.current = 0;
      // Command shortcuts are in lib/shortcuts.ts (changed in Settings › Keymap)
      const spec = eventSpec(e);
      const is = (id: string) => hit(spec, id);
      if (is("goto.object")) return e.preventDefault(), L.setSe("database");
      if (is("goto.symbol")) return e.preventDefault(), L.setSe("symbols");
      if (is("goto.file")) return e.preventDefault(), L.setSe("files");
      if (is("find.action")) return e.preventDefault(), L.setSe("actions");
      if (is("fts")) return e.preventDefault(), void L.openFts();
      if (is("explorer")) return e.preventDefault(), L.toggleExplorer();
      // ⌘1~9: workspaces (explorer is ⌘0)
      for (let n = 1; n <= 9; n++) if (is(`ws.${n}`)) return e.preventDefault(), void (useWorkspacesStore.getState().workspaces[n - 1] && L.switchWs(useWorkspacesStore.getState().workspaces[n - 1].id));
      if (is("find.window")) return e.preventDefault(), L.setFindOpen((o) => !o);
      // ⇧⎋ Hide Active Tool Window
      if (e.shiftKey && e.key === "Escape") {
        const ae = document.activeElement;
        if (ae?.closest?.("[data-toolwindow=find]")) return e.preventDefault(), L.setFindOpen(false);
        if (ae?.closest?.("[data-tree]")) return e.preventDefault(), L.toggleExplorer();
      }
      if (is("font.bigger")) return e.preventDefault(), L.bumpFont(1);
      if (is("font.smaller")) return e.preventDefault(), L.bumpFont(-1);
      const inTree = (document.activeElement as HTMLElement | null)?.closest?.("[data-tree]");
      const inInput = ["INPUT", "TEXTAREA", "SELECT"].includes((document.activeElement as HTMLElement | null)?.tagName ?? "");
      if (is("console.new")) {
        e.preventDefault();
        const conn = treeSel?.conn ?? conns.find((c) => c.id === active?.connId);
        if (conn) openConsole(conn, treeSel?.db ?? active?.database);
        return;
      }
      if (inTree && treeSel && !inInput) {
        const act = (fn: () => void) => {
          e.preventDefault();
          e.stopPropagation();
          fn();
        };
        // Edit Data (default console for a data source)
        if (is("tree.editData"))
          return act(() => ((treeSel.kind === "conn" || treeSel.kind === "db") && treeSel.conn.kind !== "redis" ? jumpToConsole(treeSel) : activateNode(treeSel)));
        if (is("tree.properties") && treeSel.kind === "conn") return act(() => setDialog({ edit: treeSel.conn }));
        if (is("tree.refresh")) return act(() => treeSel.refresh?.());
        if (is("tree.disconnect") && treeSel.conn.connected) return act(() => deactivate(treeSel.conn));
        if (is("tree.jumpConsole")) return act(() => jumpToConsole(treeSel));
        if (is("tree.sqlGen") && treeSel.name) return act(() => sqlGenerator(treeSel));
        if (is("tree.ddlCopy") && treeSel.name) return act(() => ddlToClipboard(treeSel));
        if (is("tree.ddlConsole") && treeSel.name) return act(() => ddlToConsole(treeSel));
        if (is("tree.quickDoc")) return act(() => showQuickDoc(treeSel));
        if (is("tree.modify") && (treeSel.kind === "table" || treeSel.kind === "partitioned") && canDdl(treeSel.conn)) return act(() => modifyTable(treeSel));
        if (is("tree.rename")) return act(() => renameNode(treeSel));
        if (is("tree.newDs")) return act(() => setDialog({}));
        if (is("tree.duplicate") && treeSel.kind === "conn") return act(() => duplicateConn(treeSel.conn));
        if (is("tree.remove"))
          return act(() => (treeSel.kind === "conn" ? removeConn(treeSel.conn) : treeSel.name ? dangerous(treeSel, "DROP") : undefined));
      }
      if (is("tree.ddl") && inTree && treeSel && treeSel.name) {
        e.preventDefault();
        openDdl(treeSel);
        return;
      }
      if (is("tree.copyRef") && inTree && treeSel) {
        e.preventDefault();
        navigator.clipboard.writeText(treeSel.name ? `${treeSel.schema}.${treeSel.name}` : treeSel.schema ?? treeSel.db ?? treeSel.conn.name);
        return;
      }
      if (is("refresh")) {
        e.preventDefault();
        if (inTree && treeSel?.refresh) treeSel.refresh();
        else if (active?.kind === "table") applyTableFilter(active, active.where, active.orderBy);
        else refreshAll();
        return;
      }
      if (is("editor.history") && active?.kind === "console") {
        e.preventDefault();
        setHistoryTab("history");
        setHistoryFor(active.id);
        return;
      }
      if (is("editor.cancel") && !inTree && active && canCancel(active)) {
        e.preventDefault();
        cancelRun(active);
        return;
      }
      // ⇧F6 = rename console (same as Rename File… in the tab menu)
      if (e.key === "F6" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && active?.kind === "console") {
        e.preventDefault();
        renameConsole(active);
        return;
      }
      if (is("tab.close") && active) {
        e.preventDefault();
        closeTabs([active.id]);
        return;
      }
      // ⌃⇥ / ⌃⇧⇥: Switcher (most recently used order)
      if (e.ctrlKey && !e.metaKey && e.key === "Tab") {
        e.preventDefault();
        e.stopPropagation();
        openSwitcher(e.shiftKey);
        return;
      }
      if (is("keymap")) {
        e.preventDefault();
        setKeymapOpen(true);
        return;
      }
      if (is("settings")) {
        e.preventDefault();
        setSettingsOpen("appearance");
        return;
      }
      if (is("theme")) {
        e.preventDefault();
        setSchemeOpen(true);
        return;
      }
      if (is("recent")) {
        e.preventDefault();
        e.stopPropagation();
        setRecentOpen(true);
        return;
      }
      if ((is("tab.next") || is("tab.prev")) && active) {
        e.preventDefault();
        const g = tabs.filter((t) => t.group === active.group);
        const i = g.findIndex((t) => t.id === active.id);
        const n = g[(i + (is("tab.prev") ? -1 : 1) + g.length) % g.length];
        if (n) activate(n);
        return;
      }
      if (is("tab.reopen")) {
        e.preventDefault();
        reopenClosed();
        return;
      }
      if (is("maximize")) {
        e.preventDefault();
        setMaximized((m) => !m);
        return;
      }
      // Move between split editor groups (Goto Next Splitter)
      if (is("split.other") && keyState.current.split) {
        e.preventDefault();
        // Next pane (previous with ⇧)
        const ls = leaves(useTabsStore.getState().panes);
        const g: Group = ls[(ls.indexOf(keyState.current.focused) + (e.shiftKey ? -1 : 1) + ls.length) % ls.length];
        const id = useTabsStore.getState().activeIds[g];
        const t = tabs.find((x) => x.id === id);
        if (t) activate(t);
        return;
      }
      // Ignore F4 / Enter pressed in an input field
      if (inInput) return;
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // keymapVer: menus opened from keys (⇧⌘F10 …) show the current shortcut labels
  }, [conns, keymapVer]);

  // Search Everywhere's file and action lists are built once when opened (rebuilding them every time App redraws while open would break the inner memo each time)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const seLists = useMemo(() => (se ? { files: seFiles(), actions: seActions() } : null), [se]);

  return (
    <div className="app">
      <Toolbar className="app-toolbar" style={activeConn?.color ? { boxShadow: `inset 4px 0 0 ${activeConn.color}` } : undefined}>
        <Text strong>Qurry</Text>
        <ToolbarSep />
        {/* Connection info (user@host) is hidden so it doesn't show during screen sharing */}
        {activeConn ? (
          <>
            <Badge color={activeConn.color ?? undefined} className="tb-conn">{activeConn.name}</Badge>
            <Badge className="tb-db">{active?.database || activeConn.database || "postgres"}</Badge>
          </>
        ) : (
          <Text tone="muted">No console</Text>
        )}
      </Toolbar>

      <div className="ws-row">
      {!maximized && (
        <WorkspaceBar
          list={workspaces}
          current={curWs}
          counts={Object.fromEntries(workspaces.map((w) => [w.id, w.id === curWs ? tabs.length : stash.get(w.id)?.tabs.length ?? 0]))}
          shortcut={(n) => (isMac ? `⌘${n}` : `Ctrl+${n}`)}
          onSwitch={switchWs}
          onNew={newWs}
          onRename={renameWs}
          onDelete={deleteWs}
        />
      )}
      <PanelGroup direction="vertical" className="main" autoSaveId="dbtool-v">
      <Panel id="top" order={1} minSize={20}>
      <PanelGroup direction="horizontal" autoSaveId="dbtool-h">
        {!maximized && !explorerHidden && (
        <Panel id="explorer" order={1} defaultSize={20} minSize={12}>
          {workspaces
            .filter((w) => shownWs.includes(w.id))
            .map((w) => {
              const mine = w.connIds.map((id) => conns.find((c) => c.id === id)).filter((c): c is ConnectionView => !!c);
              return (
                <div key={w.id} className={"ws-explorer" + (w.id === curWs ? " on" : "")} style={{ display: w.id === curWs ? "block" : "none" }}>
                  <Explorer
                    connections={mine}
                    others={conns.filter((c) => !w.connIds.includes(c.id))}
                    onAddExisting={(id) => setInWs(w.id, id, true)}
                    onAdd={() => setDialog({})}
                    onRefresh={refresh}
                    onExport={exportConns}
                    onImport={importConns}
                    onSelect={setTreeSel}
                    onActivate={activateNode}
                    buildMenu={buildMenu}
                    onSetColor={setColor}
                    onScopeChange={setScope}
                  />
                </div>
              );
            })}
        </Panel>
        )}
        {!maximized && !explorerHidden && <PanelResizeHandle className="resize-h" />}
        <Panel id="editors" order={2}>
          {renderPanes(panes, "")}
        </Panel>
      </PanelGroup>
      </Panel>
      {findOpen && !maximized && (
        <>
          <PanelResizeHandle className="resize-v" />
          <Panel id="find" order={2} defaultSize={30} minSize={10}>
            <FindPanel
              runs={findRuns}
              active={findActive}
              onActive={setFindActive}
              onClose={() => setFindOpen(false)}
              onCloseRun={(id) => setFindRuns((rs) => rs.filter((r) => r.id !== id))}
              onPin={(id) => setFindRuns((rs) => rs.map((r) => (r.id === id ? { ...r, pinned: !r.pinned } : r)))}
              onOpen={(run, hit) => {
                const c = conns.find((x) => x.id === run.scope.connId);
                if (c) openTableTab(c, hit.schema, hit.table, run.scope.db, undefined, hit.whereClause);
              }}
            />
          </Panel>
        </>
      )}
      </PanelGroup>
      </div>

      {switcher && (
        <Switcher
          items={switcher.items}
          start={switcher.start}
          onClose={() => setSwitcher(null)}
          onPick={(id) => {
            setSwitcher(null);
            const t = useTabsStore.getState().tabs.find((x) => x.id === id);
            if (t) activate(t);
          }}
        />
      )}
      {schemeOpen && (
        <RecentFiles
          title="Theme"
          start={["dark", "light", "hc", "system"].indexOf(getThemePref())}
          items={[
            { key: "dark", icon: getThemePref() === "dark" ? "●" : "", title: "Dark", hint: "" },
            { key: "light", icon: getThemePref() === "light" ? "●" : "", title: "Light", hint: "" },
            { key: "hc", icon: getThemePref() === "hc" ? "●" : "", title: "High contrast", hint: "" },
            { key: "system", icon: getThemePref() === "system" ? "●" : "", title: "Sync with OS", hint: `currently ${THEME_LABEL[theme]}` },
          ]}
          onClose={() => setSchemeOpen(false)}
          onPick={(k) => {
            setSchemeOpen(false);
            setThemePref(k as ThemePref);
          }}
        />
      )}
      {settingsOpen && (
        <SettingsDialog
          initialPage={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          onUpdate={(u) => {
            setUpdate(u);
            setSettingsOpen(false);
            setUpdateOpen(true);
          }}
          onChannel={setUpdate}
        />
      )}
      {backupReq && <BackupDialog what={backupReq.db ?? backupReq.c.name} pg={backupReq.c.kind === "postgres"} onRun={startBackup} onClose={() => setBackupReq(null)} />}
      {exportReq && <ExportDialog what={exportReq.name} onExport={startExport} onClose={() => setExportReq(null)} />}
      {objEdit &&
        (() => {
          const done = (msg: string) => {
            setStatus(msg);
            setObjEdit(null);
            objEdit.refresh?.();
            schemaChanged(objEdit.conn.id, objEdit.db ?? (objEdit.conn.database || (objEdit.conn.kind === "postgres" ? "postgres" : "")));
          };
          const toConsole = (sql: string) => {
            openConsole(objEdit.conn, objEdit.db, sql);
            setObjEdit(null);
          };
          return objEdit.newKind ? (
            <NewObjectDialog kind={objEdit.newKind} target={objEdit.target} conn={objEdit.conn} schema={objEdit.schema} onDone={done} onOpenConsole={toConsole} onClose={() => setObjEdit(null)} />
          ) : (
            <TableDesigner target={objEdit.target} conn={objEdit.conn} schema={objEdit.schema} table={objEdit.table} onDone={done} onOpenConsole={toConsole} onClose={() => setObjEdit(null)} />
          );
        })()}
      {copyReq && (
        <CopyRowsDialog
          source={copyReq.target}
          schema={copyReq.schema}
          table={copyReq.table}
          conn={copyReq.conn}
          conns={conns}
          onDone={(msg) => {
            setStatus(msg);
            setCopyReq(null);
          }}
          onClose={() => setCopyReq(null)}
        />
      )}
      {genReq && (
        <GenerateDataDialog
          target={genReq.target}
          schema={genReq.schema}
          table={genReq.table}
          production={genReq.conn.options?.confirmChanges ? genReq.conn.name : undefined}
          onDone={(n) => {
            setStatus(`${n.toLocaleString("en-US")} test row${n === 1 ? "" : "s"} inserted into ${genReq.schema ? `${genReq.schema}.` : ""}${genReq.table}`);
            setGenReq(null);
          }}
          onClose={() => setGenReq(null)}
        />
      )}
      {importReq && <ImportDialog target={importReq.target} schema={importReq.schema} table={importReq.table} onImport={startImport} onClose={() => setImportReq(null)} />}
      <TextDialog />
      <ConfirmHost />
      {updateOpen && update?.latest && <UpdateDialog info={update} onClose={() => setUpdateOpen(false)} beforeInstall={readyToQuit} />}
      {keymapOpen && <KeymapDialog onClose={() => setKeymapOpen(false)} onEdit={() => (setKeymapOpen(false), setSettingsOpen("keymap"))} />}
      {se && (
        <SearchEverywhere
          initialTab={se}
          conns={conns}
          contextConnId={treeSel?.conn.id ?? active?.connId}
          files={seLists?.files ?? []}
          actions={seLists?.actions ?? []}
          onClose={() => setSe(null)}
          onSelectInTree={(o) => {
            setExplorerHidden(false);
            setMaximized(false);
            revealInTree({ connId: o.connId, db: o.db, schema: o.schema, name: o.kind === "schema" ? undefined : o.name, kind: o.kind });
          }}
          onOpenData={(o) => {
            const n = objNode(o);
            if (n) activateNode(n);
          }}
          onDdl={(o) => {
            const n = objNode(o);
            if (n) openDdl(n);
          }}
          onQuickDoc={(o) => {
            const n = objNode(o);
            if (n) showQuickDoc(n);
          }}
          onOpenFile={(key) => {
            const snap = useTabsStore.getState().recent.find((r) => r.key === key);
            const open = useTabsStore.getState().tabs.find((t) => snapKey(t) === key);
            if (open) activate(open);
            else if (snap) reopen(snap);
          }}
        />
      )}
      {ftsScope && <FullTextSearchDialog scope={ftsScope} onClose={() => setFtsScope(null)} onSearch={(req) => runFts(ftsScope, req)} />}
      {recentOpen && (
        <RecentFiles
          items={recentItems()}
          onClose={() => setRecentOpen(false)}
          onPick={(key) => {
            setRecentOpen(false);
            const snap = useTabsStore.getState().recent.find((r) => r.key === key);
            if (snap) reopen(snap);
          }}
        />
      )}
      <StatusBar
        run={{ canRun: !!active && active.kind === "console" && !canCancel(active), running: canCancel(active), onRun: () => active && editorApis.get(active.id)?.run(), onCancel: () => active && cancelRun(active) }}
        onSettings={() => setSettingsOpen("appearance")}
        settingsKey={sk("settings")}
        onKeymap={() => setKeymapOpen(true)}
        status={status} readOnly={!!activeConn?.readOnly} update={update?.latest?.version} onUpdate={() => setUpdateOpen(true)} jobs={jobs} />

      {tabMenu && <ContextMenu menu={tabMenu} onClose={() => setTabMenu(null)} />}

      {historyFor && (() => {
        const t = tabs.find((x) => x.id === historyFor);
        const c = conns.find((x) => x.id === t?.connId);
        return t && c ? (
          <HistoryDialog
            connId={c.id}
            title={`${c.name} Query History`}
            initialTab={historyTab}
            onPick={(sql) => {
              editorApis.get(t.id)?.insert(sql);
              setHistoryFor(null);
            }}
            onClose={() => setHistoryFor(null)}
          />
        ) : null;
      })()}

      {prodLog && (
        <ProductionLog
          connId={prodLog.connId}
          onOpenUndo={(e) => {
            const [base, db] = e.connId.split("::");
            const c = conns.find((x) => x.id === base);
            if (!c) return setStatus(`Data source "${e.connName}" no longer exists`);
            openConsole(c, db || undefined, e.undo);
            setProdLog(null);
          }}
          onClose={() => setProdLog(null)}
        />
      )}

      {quickDoc && (
        <Overlay variant="light" onClose={() => setQuickDoc(null)} onKeyDown={(e) => e.key === "Escape" && setQuickDoc(null)}>
          <div className="quick-doc" onMouseDown={(e) => e.stopPropagation()} tabIndex={-1} ref={(el) => el?.focus()} onKeyDown={(e) => e.key === "Escape" && setQuickDoc(null)}>
            <div className="qd-title">{quickDoc.title}</div>
            <CodeBlock className="qd-body">{quickDoc.text}</CodeBlock>
          </div>
        </Overlay>
      )}

      {dialog && (
        <ConnectionDialog
          edit={dialog.edit}
          onClose={() => setDialog(null)}
          onSaved={async (saved) => {
            setDialog(null);
            await refresh();
            try {
              setStatus(`Connecting to ${saved.name}…`);
              const v = await api.connect(saved.id);
              setStatus(`Connected: ${v.split(" on ")[0]}`);
            } catch (e) {
              setStatus(`Connection failed: ${String(e)}`);
            }
            refresh();
          }}
        />
      )}
    </div>
  );
}
