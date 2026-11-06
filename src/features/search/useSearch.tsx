import { useEffect, useMemo, useState, type Dispatch, type SetStateAction } from "react";
import { ConnectionView, meta, targetId } from "../../lib/api";
import type { FtsRequest } from "../../lib/api";
import { setThemePref } from "../../lib/theme";
import { saveSnippet } from "../../components/HistoryDialog";
import { FindPanel, FindRun, FtsScope, FullTextSearchDialog } from "../../components/FullTextSearch";
import { isRelation, NodeInfo, revealInTree } from "../../components/SchemaTree";
import { ActionItem, DbObject, SearchEverywhere, SeTab } from "../../components/SearchEverywhere";
import { RecentFiles, Switcher, SwitchItem } from "../../components/Switcher";
import { TabIcon } from "../../ui/icons";
import type { useConnActions } from "../conns/useConnActions";
import type { useConsole } from "../console/useConsole";
import { setDialog, setHistoryFor, setHistoryTab, setKeymapOpen, setProdLog, setSchemeOpen, setSettingsOpen } from "../dialogs/store";
import type { useEditorGroups } from "../editorGroups/useEditorGroups";
import type { useExplorer } from "../explorer/useExplorer";
import { cmd } from "../../lib/shortcuts";
import { closeTabs } from "../tabs/ops";
import { editorApis, useTabsStore } from "../tabs/store";
import { snapKey, Tab } from "../tabs/types";
import type { useTabs } from "../tabs/useTabs";
import { useWorkspacesStore } from "../workspaces/store";
import type { useWorkspaces } from "../workspaces/useWorkspaces";

type Deps = {
  conns: ConnectionView[];
  setStatus: (s: string) => void;
  treeSel: NodeInfo | null;
  active: Tab | null;
  labelOf: (t: Tab) => string;
  setMaximized: Dispatch<SetStateAction<boolean>>;
  setExplorerHidden: (v: boolean) => void;
  toggleExplorer: () => void;
  bumpFont: (d: number) => void;
} & ReturnType<typeof useTabs> &
  ReturnType<typeof useConsole> &
  ReturnType<typeof useWorkspaces> &
  ReturnType<typeof useConnActions> &
  ReturnType<typeof useExplorer> &
  ReturnType<typeof useEditorGroups>;

/** Switcher (⌃⇥), Recent Files (⌘e), Search Everywhere (⇧⇧) with Go to … and Find Action, Full-text Search and its Find tool window */
export function useSearch({
  conns, setStatus, treeSel, active, labelOf, setMaximized, setExplorerHidden, toggleExplorer, bumpFont, openConsole, activate, newWs, renameWs, deleteWs, switchWs, exportConns, importConns, importFrom, importLocs, loadImportLocs, cancelRun, canCancel, txEnd, setTxMode, reopen, reopenClosed, splitClone, unsplit, refreshAll, activateNode, openDdl, showQuickDoc, openTableTab,
}: Deps) {
  const workspaces = useWorkspacesStore((s) => s.workspaces);
  const curWs = useWorkspacesStore((s) => s.curWs);
  const split = useTabsStore((s) => s.panes.t === "split");
  // ───────── Switcher / Recent Files / Maximize ─────────
  const [switcher, setSwitcher] = useState<{ items: SwitchItem[]; start: number } | null>(null);
  const [recentOpen, setRecentOpen] = useState(false);
  const kindIcon = (t: { kind: string; objKind?: string }) => <TabIcon kind={t.kind} obj={t.objKind} />;
  const hintOf = (connId: string, db?: string) => {
    const c = conns.find((x) => x.id === connId);
    return c ? c.name + (db ? ` · ${db}` : "") : "";
  };
  const openSwitcher = (back: boolean) => {
    const ts = useTabsStore.getState().tabs;
    const ordered = [...useTabsStore.getState().mru.map((id) => ts.find((t) => t.id === id)).filter((t): t is Tab => !!t), ...ts.filter((t) => !useTabsStore.getState().mru.includes(t.id))];
    if (!ordered.length) return;
    const items = ordered.map((t) => ({ key: t.id, icon: kindIcon(t), title: labelOf(t), hint: hintOf(t.connId, t.database), color: conns.find((c) => c.id === t.connId)?.color }));
    setSwitcher({ items, start: back ? items.length - 1 : 1 });
  };
  const recentItems = (): SwitchItem[] => {
    const openKeys = new Map(useTabsStore.getState().tabs.map((t) => [snapKey(t), t] as const));
    return useTabsStore.getState().recent
      .filter((r) => conns.some((c) => c.id === r.connId))
      .map((r) => ({ key: r.key, icon: kindIcon(r), title: r.title, hint: hintOf(r.connId, r.database), open: openKeys.has(r.key), color: conns.find((c) => c.id === r.connId)?.color }));
  };

  // ───────── Search Everywhere (⇧⇧) / Go to Database Object (⌘o) / Find Action (⇧⌘A) ─────────
  const [se, setSe] = useState<SeTab | null>(null);
  // Find Action lists the Import Data Sources locations: look for them again when it opens
  useEffect(() => {
    if (se) loadImportLocs();
  }, [se]);

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
        items.push({ key: snapKey(t), icon: kindIcon(t), title: t.title, hint: hintOf(t.connId, t.database), open: true });
      }
    }
    for (const r of useTabsStore.getState().recent)
      if (!seen.has(r.key) && conns.some((c) => c.id === r.connId))
        items.push({ key: r.key, icon: kindIcon(r), title: r.title, hint: hintOf(r.connId, r.database), open: openKeys.has(r.key) });
    return items;
  };
  /** Find Action's list. An item with a command id (cmd) is what that command's key runs too, for global commands (useGlobalShortcuts) */
  const seActions = (): ActionItem[] => {
    const t = active;
    const c = treeSel?.conn ?? conns.find((x) => x.id === t?.connId);
    return [
      { name: "New Query Console", ...cmd("console.new"), group: "File", run: () => c && openConsole(c, treeSel?.db ?? t?.database), disabled: !c },
      { name: "New Data Source…", ...cmd("tree.newDs"), group: "File", run: () => setDialog({}) },
      { name: "New Workspace…", ...cmd("ws.new"), group: "Window", run: () => newWs() },
      { name: "Rename Workspace…", ...cmd("ws.rename"), group: "Window", run: () => renameWs(curWs) },
      { name: "Delete Workspace…", ...cmd("ws.delete"), group: "Window", run: () => deleteWs(curWs), disabled: workspaces.length < 2 },
      ...workspaces.map((w, i) => ({ name: `Workspace: ${w.name}`, ...(i < 9 ? cmd(`ws.${i + 1}`) : {}), group: "Window", run: () => switchWs(w.id), disabled: w.id === curWs })),
      { name: "Data Source Properties", ...cmd("tree.properties"), group: "File", run: () => c && setDialog({ edit: c }), disabled: !c },
      { name: "Export Data Sources…", ...cmd("ds.export"), group: "File", run: () => exportConns() },
      { name: "Import Data Sources: File…", ...cmd("ds.import"), group: "File", run: () => importConns() },
      { name: "Import Data Sources: Folder…", group: "File", run: () => importFrom({ folder: true }) },
      ...importLocs.map((l) => ({ name: `Import Data Sources: ${l.label} (${l.count})`, group: "File", run: () => importFrom({ path: l.path }) })),
      { name: "Settings…", ...cmd("settings"), group: "File", run: () => setSettingsOpen("appearance") },
      { name: "Keymap Reference", ...cmd("keymap"), group: "Help", run: () => setKeymapOpen(true) },
      { name: "Edit Keymap…", ...cmd("keymap.edit"), group: "Help", run: () => setSettingsOpen("keymap") },
      { name: "Theme: Dark", ...cmd("theme.dark"), group: "Appearance", run: () => setThemePref("dark") },
      { name: "Theme: Light", ...cmd("theme.light"), group: "Appearance", run: () => setThemePref("light") },
      { name: "Theme: High contrast", ...cmd("theme.hc"), group: "Appearance", run: () => setThemePref("hc") },
      { name: "Theme: Sync with OS", ...cmd("theme.system"), group: "Appearance", run: () => setThemePref("system") },
      { name: "Quick Switch Theme…", ...cmd("theme"), group: "Appearance", run: () => setSchemeOpen(true) },
      { name: "Execute", ...cmd("editor.run"), group: "Run", run: () => t && editorApis.get(t.id)?.run(), disabled: t?.kind !== "console" },
      { name: "Explain Plan", ...cmd("editor.explain"), group: "Run", run: () => t && editorApis.get(t.id)?.run("plan"), disabled: t?.kind !== "console" },
      { name: "Explain Analyse", ...cmd("editor.explainAnalyze"), group: "Run", run: () => t && editorApis.get(t.id)?.run("analyze"), disabled: t?.kind !== "console" },
      { name: "Cancel Running Statements", ...cmd("editor.cancel"), group: "Run", run: () => t && cancelRun(t), disabled: !canCancel(t) },
      { name: "Commit", ...cmd("tx.commit"), group: "Transaction", run: () => t && txEnd(t, "commit"), disabled: !t?.inTx },
      { name: "Roll Back", ...cmd("tx.rollback"), group: "Transaction", run: () => t && txEnd(t, "rollback"), disabled: !t?.inTx },
      { name: "Transaction Mode: Manual", ...cmd("tx.manual"), group: "Transaction", run: () => t && setTxMode(t, true), disabled: t?.kind !== "console" },
      { name: "Transaction Mode: Auto", ...cmd("tx.auto"), group: "Transaction", run: () => t && setTxMode(t, false), disabled: t?.kind !== "console" },
      { name: "Browse Query History…", ...cmd("editor.history"), group: "Run", run: () => t && (setHistoryTab("history"), setHistoryFor(t.id)), disabled: t?.kind !== "console" },
      { name: "Snippets…", ...cmd("snippets"), group: "Run", run: () => t && (setHistoryTab("snippets"), setHistoryFor(t.id)), disabled: t?.kind !== "console" },
      { name: "Production Log…", ...cmd("prodLog"), group: "Run", run: () => setProdLog({ connId: null }) },
      {
        name: "Save Query as Snippet…",
        ...cmd("snippet.save"),
        group: "Run",
        run: async () => {
          const c = t && conns.find((x) => x.id === t.connId);
          const sql = t && editorApis.get(t.id)?.text();
          if (c && sql && (await saveSnippet(sql, c.id, c.name))) setStatus("Snippet saved (⌘k › Snippets…)");
        },
        disabled: t?.kind !== "console",
      },
      { name: "Go to Database Object…", ...cmd("goto.object"), group: "Navigate", run: () => setTimeout(() => setSe("database")) },
      { name: "Go to Symbol…", ...cmd("goto.symbol"), group: "Navigate", run: () => setTimeout(() => setSe("symbols")) },
      { name: "Go to File…", ...cmd("goto.file"), group: "Navigate", run: () => setTimeout(() => setSe("files")) },
      { name: "Full-text Search…", ...cmd("fts"), group: "Navigate", run: () => openFts() },
      { name: "Recent Files", ...cmd("recent"), group: "Navigate", run: () => setRecentOpen(true) },
      { name: "Switcher", shortcut: "⌃⇥", group: "Navigate", run: () => openSwitcher(false) },
      {
        name: "Select in Database Explorer",
        ...cmd("selectIn"),
        group: "Navigate",
        run: () => {
          if (!t?.edit) return;
          setExplorerHidden(false);
          setMaximized(false);
          revealInTree({ connId: t.connId, db: t.database || conns.find((x) => x.id === t.connId)?.database, schema: t.edit.schema, name: t.edit.table, kind: "table" });
        },
        disabled: !t?.edit,
      },
      { name: "Reopen Closed Tab", ...cmd("tab.reopen"), group: "Window", run: reopenClosed },
      { name: "Close Tab", ...cmd("tab.close"), group: "Window", run: () => t && closeTabs([t.id]), disabled: !t },
      { name: "Close All Tabs", ...cmd("tab.closeAll"), group: "Window", run: () => closeTabs(useTabsStore.getState().tabs.map((x) => x.id)) },
      { name: "Split Right", ...cmd("split.right"), group: "Window", run: () => t && splitClone(t, "right"), disabled: !t },
      { name: "Split Down", ...cmd("split.down"), group: "Window", run: () => t && splitClone(t, "down"), disabled: !t },
      { name: "Unsplit All", ...cmd("unsplit"), group: "Window", run: unsplit, disabled: !split },
      { name: "Maximize Editor", ...cmd("maximize"), group: "Window", run: () => setMaximized((m) => !m) },
      { name: "Database Explorer", ...cmd("explorer"), group: "Tool Windows", run: () => toggleExplorer() },
      { name: "Find (Full-text Search results)", ...cmd("find.window"), group: "Tool Windows", run: () => setFindOpen((o) => !o) },
      { name: "Refresh", ...cmd("refresh"), group: "Database", run: () => refreshAll() },
      { name: "Increase Editor Font Size", ...cmd("font.bigger"), group: "Editor", run: () => bumpFont(1) },
      { name: "Decrease Editor Font Size", ...cmd("font.smaller"), group: "Editor", run: () => bumpFont(-1) },
      { name: "Reset Editor Font Size", ...cmd("font.reset"), group: "Editor", run: () => bumpFont(0) },
    ];
  };

  // Search Everywhere's file and action lists are built once when opened (rebuilding them every time App redraws while open would break the inner memo each time)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const seLists = useMemo(() => (se ? { files: seFiles(), actions: seActions() } : null), [se]);

  return {
    openSwitcher, setRecentOpen, setSe, findOpen, setFindOpen, openFts,
    /** Find Action's list, built now: the global key handler runs a command's item by its id */
    paletteActions: seActions,
    /** Mounted where App had it: before the dialogs (overlays stack in mount order) */
    switcherPopup: (
      <>
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
      </>
    ),
    popups: (
      <>
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
      </>
    ),
    findPanel: (
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
    ),
  };
}
