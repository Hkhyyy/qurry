import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { api, ConnectionView, updateCheckEnabled } from "./lib/api";
import { ContextMenu, Explorer, MenuItem } from "./components/Explorer";
import { migrateOldHistory } from "./components/HistoryDialog";
import { WorkspaceBar } from "./components/WorkspaceBar";
import { Badge, Text, Toolbar, ToolbarSep } from "./ui";
import { invalidateSearchIndex } from "./components/SearchEverywhere";
import { DEFAULT_FONT, font, setFont } from "./lib/prefs";
import { loadKeymap, onKeymapChange } from "./lib/shortcuts";
import { invalidateCompletion } from "./lib/completion";
import { useSharedReady } from "./lib/shared";
import { tableTabLabels } from "./lib/tabLabels";
import { NodeInfo } from "./components/SchemaTree";
import { leaves } from "./lib/panes";
import { useTheme } from "./lib/theme";
import { Group, Tab } from "./features/tabs/types";
import { isMac } from "./features/shortcuts/labels";
import { offered, upToDate } from "./features/settings/SettingsDialog";
import { StatusBar } from "./features/settings/StatusBar";
import { editorApis, useTabsStore } from "./features/tabs/store";
import { stash, useWorkspacesStore } from "./features/workspaces/store";
import { useWorkspaces } from "./features/workspaces/useWorkspaces";
import { useConsole } from "./features/console/useConsole";
import { useTabs } from "./features/tabs/useTabs";
import { useJobs } from "./features/jobs/useJobs";
import { useConnActions } from "./features/conns/useConnActions";
import { useExplorer } from "./features/explorer/useExplorer";
import { useEditorGroups } from "./features/editorGroups/useEditorGroups";
import { useSearch } from "./features/search/useSearch";
import { Dialogs, TopDialogs } from "./features/dialogs/Dialogs";
import { useGlobalShortcuts } from "./features/shortcuts/useGlobalShortcuts";
import { setDialog, setKeymapOpen, setSettingsOpen, setUpdate, setUpdateOpen, useDialogsStore } from "./features/dialogs/store";
import "./App.css";

export default function App() {
  const theme = useTheme();
  // Redraw the menu shortcut labels when the keymap changes (or the WASM module that draws them has loaded)
  const [keymapVer, setKeymapVer] = useState(0);
  useEffect(() => {
    const off = onKeymapChange(() => setKeymapVer((n) => n + 1));
    return () => void off();
  }, []);
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
  const [status, setStatus] = useState("Ready");
  // A keymap.json that doesn't load: the default keys work, the status bar says how to get the saved ones back
  useEffect(() => void loadKeymap().then((err) => err && setStatus(err)), []);
  const [treeSel, setTreeSel] = useState<NodeInfo | null>(null);
  // Every selected node (⌘-click / ⇧-click): the explorer's commands for several run on these
  const [treeSelAll, setTreeSelAll] = useState<NodeInfo[]>([]);
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);

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
  const { run, canCancel, cancelRun, loadStale } = cons;
  const tabOps = useTabs({ setStatus, loadStale });
  const { activate, addTab, openConsole } = tabOps;
  const ws = useWorkspaces({ conns, setStatus, loadStale });
  const { readyToQuit, switchWs, newWs, renameWs, deleteWs, setInWs, shownWs } = ws;

  const transfer = useJobs({ setStatus });
  const { jobs, runJob, startExport, startImport } = transfer;
  const connActions = useConnActions({ conns, refresh, setStatus });
  const { setColor, setScope, exportConns, importConns, removeConn } = connActions;
  const ex = useExplorer({ setStatus, refresh, activate, addTab, openConsole, run, runJob, removeConn, setInWs, setTabMenu });
  const { activateNode, buildMenu, startBackup } = ex;

  const [maximized, setMaximized] = useState(false);
  const app = { conns, theme, setStatus, refresh, treeSel, treeSelAll, active, groupActive, labelOf, setTabMenu, setMaximized, ...cons, ...tabOps, ...ws, ...transfer, ...connActions, ...ex };
  const groups = useEditorGroups(app);
  const { renderPanes } = groups;

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

  // ── Editor font size ──
  const bumpFont = (d: number) => void setFont("editor", { size: d === 0 ? DEFAULT_FONT : font("editor").size + d });
  const search = useSearch({ ...app, ...groups, setExplorerHidden, toggleExplorer, bumpFont });
  const { findOpen } = search;

  useGlobalShortcuts({ ...app, ...groups, ...search, keymapVer, focused, tabs, split, toggleExplorer, bumpFont });

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
          onKeymap={() => setKeymapOpen(true)}
          onSettings={() => setSettingsOpen("appearance")}
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
                    shown={w.id === curWs}
                    onSelect={(info, all) => (setTreeSel(info), setTreeSelAll(all))}
                    onActivate={activateNode}
                    onStatus={setStatus}
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
            {search.findPanel}
          </Panel>
        </>
      )}
      </PanelGroup>
      </div>

      {search.switcherPopup}
      <Dialogs conns={conns} setStatus={setStatus} openConsole={openConsole} startBackup={startBackup} startExport={startExport} startImport={startImport} readyToQuit={readyToQuit} />
      {search.popups}
      <StatusBar
        run={{ canRun: !!active && active.kind === "console" && !canCancel(active), running: canCancel(active), onRun: () => active && editorApis.get(active.id)?.run(), onCancel: () => active && cancelRun(active) }}
        status={status} readOnly={!!activeConn?.readOnly} update={update?.latest?.version} onUpdate={() => setUpdateOpen(true)} jobs={jobs} />

      {tabMenu && <ContextMenu menu={tabMenu} onClose={() => setTabMenu(null)} />}

      <TopDialogs conns={conns} setStatus={setStatus} refresh={refresh} openConsole={openConsole} />
    </div>
  );
}
