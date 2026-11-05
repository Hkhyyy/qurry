import { useState, type Dispatch, type SetStateAction } from "react";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { api, ConnectionView, defaultDbOf, dialectOf, meta, targetId } from "../../lib/api";
import { askText } from "../../lib/dialogs";
import { leaf, leaves, MAX_PANES, Pane, splitPane } from "../../lib/panes";
import { connTabVars, type Theme } from "../../lib/theme";
import { ConsoleEditor } from "../../components/ConsoleEditor";
import { ConsolePane } from "../../components/ConsolePane";
import { DbaReport } from "../../components/DbaReport";
import { ErdView } from "../../components/ErdView";
import type { MenuItem } from "../../components/Explorer";
import { MigrateView } from "../../components/MigrateView";
import { RagView } from "../../components/RagView";
import { RedisBrowser } from "../../components/RedisBrowser";
import { TableEditor } from "../../components/TableEditor";
import { TlsFix } from "../../components/TlsFix";
import { IconButton, Spacer, TabItem, Tabs, TabStrip, Text, ToolbarSep, ToolButton, Toolbar } from "../../ui";
import { TableStructureView } from "../../components/TableStructure";
import { setDdlView, type DdlView } from "../../lib/structure";
import { IcReload, TabIcon } from "../../ui/icons";
import type { NodeInfo } from "../../components/SchemaTree";
import type { useConsole } from "../console/useConsole";
import type { useExplorer } from "../explorer/useExplorer";
import type { useJobs } from "../jobs/useJobs";
import { cmd, keyTitle } from "../../lib/shortcuts";
import { isMac, sk } from "../shortcuts/labels";
import { closeTabs, ddlObject, moveTo, patch, settle, sqlOf, togglePin } from "../tabs/ops";
import { editorApis, liveSql, setActiveIds, setFocused, setPanes, setTabs, useTabsStore } from "../tabs/store";
import { Group, snapKey, Tab, TabSnapshot } from "../tabs/types";
import type { useTabs } from "../tabs/useTabs";
import { setHistoryFor, setHistoryTab } from "../dialogs/store";
import { setWorkspaces, useWorkspacesStore } from "../workspaces/store";
import type { useWorkspaces } from "../workspaces/useWorkspaces";

/** A tab's database by name: its own, else the connection's default (a tab on the default keeps none) */
const tabDatabase = (t: { database?: string }, c?: ConnectionView) => t.database || c?.database || (c?.kind === "mysql" ? "" : "postgres");

type Deps = {
  conns: ConnectionView[];
  theme: Theme;
  setStatus: (s: string) => void;
  groupActive: (g: Group) => Tab | null;
  labelOf: (t: Tab) => string;
  setTabMenu: (m: { x: number; y: number; items: MenuItem[] } | null) => void;
  setMaximized: Dispatch<SetStateAction<boolean>>;
} & ReturnType<typeof useTabs> &
  ReturnType<typeof useConsole> &
  ReturnType<typeof useExplorer> &
  ReturnType<typeof useJobs> &
  ReturnType<typeof useWorkspaces>;

/** The editor groups (split panes): their tab strips, what each tab shows, the tab menu, splitting and tab drag and drop */
export function useEditorGroups({
  conns, theme, setStatus, groupActive, labelOf, setTabMenu, setMaximized, activate, addTab, openConsole, openTable, openTableTab, openDdl, ragInit, runJob, exportCsv, moveToWs, dbCache, schemaCache, executeConsole, cancel, fetchMoreEntry, setTxMode, txEnd, loadDbList, loadSchemaList, switchDatabase, switchSchema, log, reconnectAfterTls, setPendingRuns, applyTableFilter, fetchMore, submitEdits, focusTableGrid,
}: Deps) {
  const tabs = useTabsStore((s) => s.tabs);
  const panes = useTabsStore((s) => s.panes);
  const split = panes.t === "split";
  const focused = useTabsStore((s) => s.focused);
  const workspaces = useWorkspacesStore((s) => s.workspaces);
  const curWs = useWorkspacesStore((s) => s.curWs);
  /** Split the pane holding the tab and move the tab to the new pane (Split and Move Right / Down) */
  const splitAndMove = (t: Tab, dir: "right" | "down") => {
    const r = splitPane(useTabsStore.getState().panes, t.group, dir);
    if (!r) return setStatus(`Up to ${MAX_PANES} panes`);
    setTabs((ts) => {
      const next = ts.map((x) => (x.id === t.id ? { ...x, group: r.g } : x));
      settle(next, r.pane, { g: r.g, id: t.id });
      return next;
    });
  };

  /** DDL tab → Structure: Reload remounts the tab's view (a new key), which loads it again */
  const [structureRev, setStructureRev] = useState<Record<string, number>>({});

  // ───────── Tab drag and drop (DataGrip: reorder / move to another group / drag to an edge to split) ─────────
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<{ g: Group; zone: "tabs" | "center" | "right" | "down"; beforeId: string | null } | null>(null);

  const dropTab = (id: string, g: Group, beforeId: string | null, dir?: "right" | "down") => {
    // Dropped on an edge: split that pane into a new one
    let pane = useTabsStore.getState().panes;
    if (dir) {
      const r = splitPane(pane, g, dir);
      if (!r) return setStatus(`Up to ${MAX_PANES} panes`);
      pane = r.pane;
      g = r.g;
    }
    setTabs((ts) => {
      const t = ts.find((x) => x.id === id);
      if (!t || beforeId === id) return ts;
      const rest = ts.filter((x) => x.id !== id);
      let idx = beforeId ? rest.findIndex((x) => x.id === beforeId) : -1;
      if (idx < 0) {
        const lastIdx = rest.map((x) => x.group).lastIndexOf(g);
        idx = lastIdx < 0 ? rest.length : lastIdx + 1;
      }
      const next = [...rest.slice(0, idx), { ...t, group: g }, ...rest.slice(idx)];
      settle(next, pane, { g, id });
      return next;
    });
  };

  const endDrag = () => {
    setDragId(null);
    setDropHint(null);
  };

  const unsplit = () => {
    setTabs((ts) => ts.map((t) => ({ ...t, group: 0 })));
    setPanes(leaf(0));
    setFocused(0);
    setActiveIds((a) => [a[focused] ?? a.find(Boolean) ?? null, null, null, null]);
  };

  /** Reopen from Reopen Closed Tab / Recent Files */
  const reopen = (snap: TabSnapshot) => {
    const c = conns.find((x) => x.id === snap.connId);
    if (!c) return;
    const open = useTabsStore.getState().tabs.find((t) => snapKey(t) === snap.key);
    if (open) return activate(open);
    if (snap.kind === "table" && snap.edit) {
      openTableTab(c, snap.edit.schema, snap.edit.table, snap.database, undefined, snap.where, snap.orderBy, snap.objKind);
    } else addTab(c, snap.kind, { title: snap.title, sql: snap.sql, database: snap.database, edit: snap.edit, objKind: snap.objKind, ddlOf: snap.ddlOf });
  };
  const reopenClosed = () => {
    const [snap, ...rest] = useTabsStore.getState().closedStack;
    useTabsStore.setState({ closedStack: rest });
    if (snap) reopen(snap);
  };

  /** Split Right / Down: duplicate the same target as an independent tab and open it in the right (lower) pane split from that tab's pane (up to 4 panes) */
  const splitClone = (t: Tab, dir: "right" | "down") => {
    const c = conns.find((x) => x.id === t.connId);
    if (!c) return;
    const r = splitPane(useTabsStore.getState().panes, t.group, dir);
    if (!r) return setStatus(`Up to ${MAX_PANES} panes`);
    setPanes(r.pane);
    if (t.kind === "table" && t.edit) openTableTab(c, t.edit.schema, t.edit.table, t.database, r.g, t.where, t.orderBy, t.objKind);
    else addTab(c, t.kind, { title: t.kind === "console" ? `${t.title} (copy)` : t.title, sql: sqlOf(t), database: t.database, edit: t.edit, group: r.g, objKind: t.objKind });
  };

  /** The tab's context menu. The global key handler runs a command's entry for the active tab by its id */
  // ── DDL tab toolbar (its commands' keys act on the active DDL tab) ──
  const pickDdlView = (t: Tab, v: DdlView) => {
    patch(t.id, { ddlView: v });
    setDdlView(v);
  };
  /** Reload what the tab shows: the structure (a remount loads it again) or the DDL text */
  const reloadDdl = async (t: Tab) => {
    if ((t.ddlView ?? "sql") === "structure") return setStructureRev((r) => ({ ...r, [t.id]: (r[t.id] ?? 0) + 1 }));
    const of = ddlObject(t);
    if (!of) return;
    try {
      setStatus(`Loading DDL of ${of.schema}.${of.name}…`);
      patch(t.id, { sql: await meta.ddl(t.target, of.schema, of.name, t.objKind ?? "table") });
      setStatus("Ready");
    } catch (e) {
      setStatus(`DDL failed: ${String(e)}`);
    }
  };
  const copyDdl = (t: Tab) => {
    navigator.clipboard.writeText(t.sql);
    setStatus("Copied to clipboard");
  };
  const saveDdl = async (t: Tab) => {
    try {
      const path = await api.saveTextFile(`${t.title.replace(/^SQL Generator: | \[DDL\]$/g, "")}.sql`, t.sql);
      if (path) setStatus(`Saved ${path}`);
    } catch (e) {
      setStatus(String(e));
    }
  };
  const runDdl = (t: Tab) => {
    const c = conns.find((x) => x.id === t.connId);
    if (c) openConsole(c, t.database, t.sql);
  };
  const ddlCommands = (t: Tab): MenuItem[] =>
    t.kind !== "ddl"
      ? []
      : [
          ...(t.edit
            ? ([
                { label: "SQL", ...cmd("ddl.sql"), action: () => pickDdlView(t, "sql") },
                { label: "Structure", ...cmd("ddl.structure"), action: () => pickDdlView(t, "structure") },
              ] as MenuItem[])
            : []),
          { label: "Reload", ...cmd("ddl.reload"), action: () => void reloadDdl(t), disabled: !ddlObject(t) },
          { label: "Copy", ...cmd("ddl.copy"), action: () => copyDdl(t) },
          { label: "Save to File", ...cmd("ddl.save"), action: () => void saveDdl(t) },
          { label: "Run in Console", ...cmd("ddl.run"), action: () => runDdl(t) },
        ];

  const tabMenu = (t: Tab): MenuItem[] => {
    const sameGroup = tabs.filter((x) => x.group === t.group);
    const idx = sameGroup.findIndex((x) => x.id === t.id);
    const ls = leaves(panes);
    const other: Group = ls[(ls.indexOf(t.group) + 1) % ls.length];
    const canSplit = ls.length < MAX_PANES;
    const c = conns.find((x) => x.id === t.connId);
    const items: MenuItem[] = [
      { label: "Close", ...cmd("tab.close"), action: () => closeTabs([t.id]) },
      { label: "Close Other Tabs", ...cmd("tab.closeOthers"), action: () => closeTabs(sameGroup.filter((x) => x.id !== t.id && !x.pinned).map((x) => x.id)) },
      { label: "Close All but Pinned", ...cmd("tab.closeAllButPinned"), action: () => closeTabs(sameGroup.filter((x) => !x.pinned).map((x) => x.id)) },
      { label: "Close Tabs to the Left", ...cmd("tab.closeLeft"), action: () => closeTabs(sameGroup.slice(0, idx).filter((x) => !x.pinned).map((x) => x.id)), disabled: idx === 0 },
      { label: "Close Tabs to the Right", ...cmd("tab.closeRight"), action: () => closeTabs(sameGroup.slice(idx + 1).filter((x) => !x.pinned).map((x) => x.id)), disabled: idx === sameGroup.length - 1 },
      { label: "Close All Tabs", ...cmd("tab.closeAll"), action: () => closeTabs(sameGroup.map((x) => x.id)) },
      { label: "Reopen Closed Tab", ...cmd("tab.reopen"), action: reopenClosed, disabled: useTabsStore.getState().closedStack.length === 0 },
      { separator: true },
      { label: t.pinned ? "Unpin Tab" : "Pin Tab", ...cmd("tab.pin"), action: () => togglePin(t) },
      {
        label: "Copy Path/Reference…",
        submenu: [
          { label: "Copy Reference", ...cmd("tab.copyRef"), action: () => navigator.clipboard.writeText(t.edit ? `${t.edit.schema}.${t.edit.table}` : t.title) },
          ...(c ? ([{ label: "Copy Data Source Name", ...cmd("tab.copyConnName"), action: () => navigator.clipboard.writeText(c.name) }] as MenuItem[]) : []),
        ],
      },
      { separator: true },
      { label: "Split Right", ...cmd("split.right"), action: () => splitClone(t, "right"), disabled: !canSplit },
      { label: "Split Down", ...cmd("split.down"), action: () => splitClone(t, "down"), disabled: !canSplit },
      { label: "Split and Move Right", ...cmd("split.moveRight"), action: () => splitAndMove(t, "right"), disabled: !canSplit || sameGroup.length < 2 },
      { label: "Split and Move Down", ...cmd("split.moveDown"), action: () => splitAndMove(t, "down"), disabled: !canSplit || sameGroup.length < 2 },
      ...(split
        ? ([
            { label: "Move To Next Group", ...cmd("tab.moveNext"), action: () => moveTo(t.id, other) },
            { label: "Open In Next Group", ...cmd("tab.openNext"), action: () => { const cc = c; if (!cc) return; if (t.kind === "table" && t.edit) openTableTab(cc, t.edit.schema, t.edit.table, t.database, other, t.where, t.orderBy, t.objKind); else addTab(cc, t.kind, { title: t.title, sql: sqlOf(t), database: t.database, edit: t.edit, group: other, objKind: t.objKind }); } },
            { label: "Unsplit All", ...cmd("unsplit"), action: unsplit },
          ] as MenuItem[])
        : []),
      { separator: true },
      {
        label: "Move to Workspace",
        submenu: [
          ...workspaces.filter((w) => w.id !== curWs).map((w) => ({ label: w.name, action: () => moveToWs(t, w.id) })),
          ...(workspaces.length > 1 ? ([{ separator: true }] as MenuItem[]) : []),
          {
            label: "New Workspace…",
            ...cmd("tab.moveToNewWs"),
            action: async () => {
              const name = (await askText("Workspace name", `Workspace ${useWorkspacesStore.getState().workspaces.length + 1}`));
              if (!name?.trim()) return;
              const w = { id: crypto.randomUUID(), name: name.trim(), connIds: [] };
              setWorkspaces((l) => [...l, w]);
              moveToWs(t, w.id);
            },
          },
        ],
      },
    ];
    if (t.kind === "console" && c)
      items.push(
        { separator: true },
        { label: "Duplicate", ...cmd("tab.duplicate"), action: () => addTab(c, "console", { title: `${t.title} (copy)`, sql: sqlOf(t), database: t.database, group: t.group }) },
        { label: "Rename File…", ...cmd("console.rename"), action: () => renameConsole(t) },
      );
    return items;
  };

  const openTabMenu = (e: React.MouseEvent, t: Tab) => {
    e.preventDefault();
    e.stopPropagation();
    activate(t);
    setTabMenu({ x: e.clientX, y: e.clientY, items: tabMenu(t) });
  };

  const renameConsole = async (t: Tab) => {
    const name = (await askText("Rename console", t.title));
    if (name && name.trim()) patch(t.id, { title: name.trim() });
  };

  // ───────── Editor group rendering ─────────
  /** Split tree: branches lay out side by side (horizontal/vertical), leaves are panes. Only the outermost ratio is saved */
  const renderPanes = (p: Pane, path: string): React.ReactNode =>
    p.t === "leaf" ? (
      renderGroup(p.g)
    ) : (
      <PanelGroup key={path} direction={p.dir === "right" ? "horizontal" : "vertical"} autoSaveId={path === "" ? `dbtool-split-${p.dir}` : undefined}>
        <Panel minSize={15}>{renderPanes(p.a, path + "a")}</Panel>
        <PanelResizeHandle className={p.dir === "right" ? "resize-h" : "resize-v"} />
        <Panel minSize={15}>{renderPanes(p.b, path + "b")}</Panel>
      </PanelGroup>
    );
  const renderGroup = (g: Group) => {
    const gTabs = tabs.filter((t) => t.group === g);
    const ga = groupActive(g);
    const gConn = conns.find((c) => c.id === ga?.connId);
    return (
      <div className={"panel editor-group" + (split ? (focused === g ? " focused" : " unfocused") : "")} onMouseDownCapture={() => setFocused(g)}>
        <TabStrip
          activeKey={ga?.id}
          overflow={gTabs.map((t) => ({ key: t.id, label: labelOf(t), active: t.id === ga?.id, onSelect: () => activate(t) }))}
          className={"tabs" + (dropHint?.zone === "tabs" && dropHint.g === g && dropHint.beforeId === null ? " drop-end" : "")}
          onDragOver={(e) => {
            if (!dragId) return;
            e.preventDefault();
            setDropHint({ g, zone: "tabs", beforeId: null });
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (dragId) dropTab(dragId, g, null);
            endDrag();
          }}
        >
          {gTabs.map((t) => {
            const color = conns.find((c) => c.id === t.connId)?.color;
            return (
              <TabItem
                key={t.id}
                label={labelOf(t)}
                icon={<TabIcon kind={t.kind} obj={t.objKind} />}
                active={t.id === ga?.id}
                pinned={t.pinned}
                onPin={() => togglePin(t)}
                closeTitle="Close. Alt-Click to Close Others"
                onClose={(e) => {
                  // ⌥+click: close all other tabs
                  if (e.altKey) closeTabs(gTabs.filter((x) => x.id !== t.id && !x.pinned).map((x) => x.id));
                  else closeTabs([t.id]);
                }}
                className={(t.id === dragId ? "dragging" : "") + (dropHint?.zone === "tabs" && dropHint.g === g && dropHint.beforeId === t.id ? " drop-before" : "")}
                style={connTabVars(color, theme)}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData("text/plain", t.id);
                  e.dataTransfer.effectAllowed = "move";
                  setDragId(t.id);
                }}
                onDragEnd={endDrag}
                onDragOver={(e) => {
                  if (!dragId) return;
                  e.preventDefault();
                  e.stopPropagation();
                  const r = e.currentTarget.getBoundingClientRect();
                  const i = gTabs.findIndex((x) => x.id === t.id);
                  const beforeId = e.clientX < r.left + r.width / 2 ? t.id : gTabs[i + 1]?.id ?? null;
                  setDropHint({ g, zone: "tabs", beforeId });
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  if (dragId && dropHint) dropTab(dragId, g, dropHint.beforeId);
                  endDrag();
                }}
                onMouseDown={(e) => {
                  if (e.button === 1) closeTabs([t.id]);
                  else activate(t);
                }}
                onContextMenu={(e) => openTabMenu(e, t)}
                onDoubleClick={() => setMaximized((m) => !m)}
                title={t.kind === "table" ? `${t.edit?.schema}.${t.edit?.table}` : t.title}
              />
            );
          })}
        </TabStrip>
        <div className="work-area" style={gConn?.color ? { boxShadow: `inset 0 2px 0 ${gConn.color}` } : undefined}>
          {/* While dragging a tab: center = into this group, right/bottom edge = split */}
          {dragId && (
            <div
              className="drop-overlay"
              onDragOver={(e) => {
                e.preventDefault();
                const r = e.currentTarget.getBoundingClientRect();
                const fx = (e.clientX - r.left) / r.width;
                const fy = (e.clientY - r.top) / r.height;
                // A pane's right / bottom edge = split that pane (up to 4 panes)
                const canSplit = leaves(panes).length < MAX_PANES;
                const zone = canSplit && fx > 0.7 ? "right" : canSplit && fy > 0.7 ? "down" : "center";
                setDropHint({ g, zone, beforeId: null });
              }}
              onDragLeave={() => setDropHint((h) => (h && h.g === g && h.zone !== "tabs" ? null : h))}
              onDrop={(e) => {
                e.preventDefault();
                const h = dropHint;
                if (h && h.g === g) {
                  if (h.zone === "right" || h.zone === "down") {
                    // Ignore a pane's only tab, since splitting would leave it unchanged
                    if (!(gTabs.length === 1 && gTabs[0].id === dragId)) dropTab(dragId, g, null, h.zone);
                  } else dropTab(dragId, g, null);
                }
                endDrag();
              }}
            >
              {dropHint?.g === g && dropHint.zone !== "tabs" && <div className={"drop-zone " + dropHint.zone} />}
            </div>
          )}
          {/* SQL console (DataGrip: console toolbar + editor + result tabs/Output) */}
          {!gTabs.length && (
            <div className="empty center">
              Right-click a data source → <b>New Query Console</b> ({isMac ? "⇧⌘L" : "Ctrl+Shift+L"}).
              <br />
              <Text tone="muted">Double-click a table ({isMac ? "⌘↓" : "F4"}) to edit data, {isMac ? "⌘b" : "Ctrl+b"} for DDL, {isMac ? "⌘↩" : "Ctrl+Enter"} to execute.</Text>
              <br />
              <Text tone="muted">All shortcuts: <b>Keymap Reference</b> ({sk("keymap")}) · Search Everywhere ({isMac ? "⇧⇧" : "Shift Shift"})</Text>
            </div>
          )}
          {gTabs
            .filter((t) => t.kind === "console")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <ConsolePane
                    tabId={t.id}
                    target={t.target}
                    initialSql={sqlOf(t)}
                    connName={c?.name ?? ""}
                    database={tabDatabase(t, c)}
                    schema={t.schema ?? (c?.kind === "mysql" ? t.database || c?.database || "" : "public")}
                    kind={c?.kind}
                    dialect={dialectOf(c?.kind)}
                    dbList={dbCache[t.connId] ?? []}
                    schemaList={schemaCache[t.target] ?? []}
                    running={t.running}
                    loadingMore={t.loadingMore}
                    results={t.results}
                    activeResult={t.activeResult}
                    output={t.output}
                    txManual={t.txManual}
                    inTx={t.inTx}
                    onChange={(sql) => liveSql.set(t.id, sql)}
                    onExecute={(stmts, explain, mark) => executeConsole(t.id, stmts, explain, mark)}
                    onCancel={() => cancel(t)}
                    onSetActiveResult={(id) => patch(t.id, { activeResult: id })}
                    onPin={(id, pinned) => setTabs((ts) => ts.map((x) => (x.id === t.id ? { ...x, results: x.results.map((r) => (r.id === id ? { ...r, pinned } : r)) } : x)))}
                    onCloseResult={(id) =>
                      setTabs((ts) => ts.map((x) => (x.id === t.id ? { ...x, results: x.results.filter((r) => r.id !== id), activeResult: x.activeResult === id ? "output" : x.activeResult } : x)))
                    }
                    onFetchMore={(entryId) => fetchMoreEntry(t.id, entryId)}
                    readOnly={!!c?.readOnly}
                    onSubmit={(entryId, st) => submitEdits(t, st, () => {
                      const e = t.results.find((r) => r.id === entryId);
                      if (e) void executeConsole(t.id, [e.sql]);
                    })}
                    onExport={(sql) => exportCsv({ ...t, lastSql: sql })}
                    onHistory={() => (setHistoryTab("history"), setHistoryFor(t.id))}
                    onTxMode={(manual) => setTxMode(t, manual)}
                    onCommit={() => txEnd(t, "commit")}
                    onRollback={() => txEnd(t, "rollback")}
                    onLoadDbs={() => loadDbList(t.connId)}
                    onLoadSchemas={() => loadSchemaList(t.target)}
                    onSwitchDb={(db) => switchDatabase(t, db)}
                    onSwitchSchema={(sc) => switchSchema(t, sc)}
                    schemaSwitchDisabled={conns.find((x) => x.id === t.connId)?.options?.switchSchema === "disable"}
                    registerApi={(id, a) => editorApis.set(id, a)}
                    onClearOutput={() => patch(t.id, { output: [] })}
                    errorAction={<TlsFix conn={c} msg={t.output[t.output.length - 1]?.level === "error" ? t.output[t.output.length - 1].text : null} then={() => (editorApis.get(t.id)?.focus(), reconnectAfterTls(t.connId, t.id))} />}
                    onError={(m) => (log(t.id, "error", m), patch(t.id, { activeResult: "output" }), setStatus("Not run: the statement splitter couldn't load (see Output)"))}
                    onLoadingChange={() => setPendingRuns((n) => n + 1)}
                  />
                </div>
              );
            })}
          {/* AI Search (RAG) */}
          {gTabs
            .filter((t) => t.kind === "rag")
            .map((t) => (
              <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                <RagView key={ragInit?.n} conns={conns} initial={ragInit} runJob={runJob} onStatus={setStatus} />
              </div>
            ))}
          {/* Compare & migrate */}
          {gTabs
            .filter((t) => t.kind === "migrate")
            .map((t) => (
              <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                <MigrateView
                  conns={conns}
                  initialConnId={t.connId}
                  initialDb={t.database}
                  onOpenConsole={(connId, db, sql) => {
                    const c = conns.find((x) => x.id === connId);
                    if (c) openConsole(c, db, sql);
                  }}
                  onStatus={setStatus}
                />
              </div>
            ))}
          {/* ERD */}
          {gTabs
            .filter((t) => t.kind === "erd")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <ErdView target={t.target} schemas={t.erdSchemas ?? []} visible={t.id === ga?.id} onOpenTable={(sc, tb) => c && openTable(c, sc, tb, t.database)} onStatus={setStatus} />
                </div>
              );
            })}
          {/* DBA reports */}
          {gTabs
            .filter((t) => t.kind === "dba")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              const def = c ? defaultDbOf(c) : "";
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <DbaReport
                    target={t.target}
                    connName={c?.name ?? ""}
                    report={t.dbaReport ?? "sessions"}
                    readOnly={!!c?.readOnly}
                    dialect={dialectOf(c?.kind ?? "postgres")}
                    visible={t.id === ga?.id}
                    database={t.database ?? def}
                    // Every database of the server (as the console lists them), through the report's own database (the account may not
                    // connect to the default one), else through the source's (the report's may be one it can't connect to);
                    // MySQL without a default schema can go back to all schemas
                    loadDbs={() =>
                      meta
                        .databases(t.target)
                        .catch((e) => (c && t.target !== c.id ? meta.databases(c.id) : Promise.reject(e)))
                        .then((ds) => [...(def ? [] : [""]), ...ds])
                    }
                    fixedDb={!!c?.options?.singleDb}
                    onDatabase={(db) => c && patch(t.id, { database: db !== def ? db : undefined, target: targetId(c.id, db !== def ? db : undefined) })}
                    onReport={(id, title) => c && patch(t.id, { dbaReport: id, title: `${title} — ${c.name}` })}
                    onOpenConsole={(sql) => c && openConsole(c, t.database, sql)}
                    onStatus={setStatus}
                  />
                </div>
              );
            })}
          {/* Redis browser */}
          {gTabs
            .filter((t) => t.kind === "redis")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <RedisBrowser
                    target={t.target}
                    connName={c?.name ?? ""}
                    database={t.database ?? "db0"}
                    dbList={dbCache[t.connId] ?? []}
                    readOnly={!!c?.readOnly}
                    initialKey={t.redisKey}
                    onLoadDbs={() => loadDbList(t.connId)}
                    onSwitchDb={(db) => c && patch(t.id, { database: db, target: targetId(c.id, db), title: `${c.name} [${db}]`, redisKey: undefined })}
                    onStatus={setStatus}
                  />
                </div>
              );
            })}
          {/* Table data editor */}
          {gTabs
            .filter((t) => t.kind === "table")
            .map((t) => (
              <div key={t.id} className="work-layer" data-tab={t.id} style={{ display: t.id === ga?.id ? "block" : "none" }}>
                <TableEditor
                  where={t.where}
                  orderBy={t.orderBy}
                  running={t.running}
                  error={t.error}
                  errorAction={<TlsFix conn={conns.find((x) => x.id === t.connId)} msg={t.error} then={() => (focusTableGrid(t.id), reconnectAfterTls(t.connId))} />}
                  result={t.result}
                  query={t.lastSql}
                  edit={t.edit && t.lastSql === t.tableSql ? t.edit : undefined}
                  readOnly={!!conns.find((x) => x.id === t.connId)?.readOnly}
                  dialect={dialectOf(conns.find((x) => x.id === t.connId)?.kind)}
                  loadingMore={t.loadingMore}
                  completion={t.edit && { target: t.target, schema: t.edit.schema, table: t.edit.table, db: tabDatabase(t, conns.find((x) => x.id === t.connId)), mysql: conns.find((x) => x.id === t.connId)?.kind === "mysql" }}
                  onApply={(w, o) => applyTableFilter(t, w, o)}
                  onCancel={() => cancel(t)}
                  onFetchMore={() => fetchMore(t)}
                  onSubmit={(st) => submitEdits(t, st)}
                  onExportCsv={() => exportCsv(t)}
                  onDdl={() => {
                    const c = conns.find((x) => x.id === t.connId);
                    if (c && t.edit) openDdl({ kind: (t.objKind ?? "table") as NodeInfo["kind"], conn: c, db: t.database, schema: t.edit.schema, name: t.edit.table });
                  }}
                  onCopyQueryToConsole={(sql) => { const c = conns.find((x) => x.id === t.connId); if (c) openConsole(c, t.database, sql); }}
                  onFindSimilar={
                    conns.find((x) => x.id === t.connId)?.kind === "postgres"
                      ? async (column, row) => {
                          const c = conns.find((x) => x.id === t.connId);
                          if (!c || !t.edit) return;
                          try {
                            const sql = await meta.similarRowsSql(t.target, t.edit.schema, t.edit.table, column, t.edit.pk.map((k) => [k, row[k] === null ? null : String(row[k])]));
                            openConsole(c, t.database, sql);
                            setStatus("Similar-rows query opened in a new console");
                          } catch (e) {
                            setStatus(String(e));
                          }
                        }
                      : undefined
                  }
                />
              </div>
            ))}
          {/* DDL (read-only) */}
          {gTabs
            .filter((t) => t.kind === "ddl")
            .map((t) => {
              // A table's DDL also has the Structure view (openDdl sets edit for tables, buildTab its view)
              const view = t.ddlView ?? "sql";
              const pick = (v: DdlView) => pickDdlView(t, v);
              return (
              <div key={t.id} className="work-layer ddl-layer" style={{ display: t.id === ga?.id ? "flex" : "none" }}>
                <Toolbar>
                  <span className="tb-info" style={{ marginLeft: 0 }}>{t.title.startsWith("SQL Generator") ? "SQL Generator" : "DDL"} · read-only</span>
                  {t.edit && <ToolbarSep />}
                  {t.edit && <Tabs size="sm" label="DDL view" items={[{ id: "sql", label: "SQL" }, { id: "structure", label: "Structure" }]} value={view} onChange={pick} />}
                  <IconButton title={ddlObject(t) ? keyTitle("Reload", "ddl.reload") : "Reopen the DDL to reload"} onClick={() => void reloadDdl(t)} disabled={!ddlObject(t)}><IcReload /></IconButton>
                  <Spacer />
                  <ToolButton title={keyTitle("Copy to Clipboard", "ddl.copy")} onClick={() => copyDdl(t)}>Copy</ToolButton>
                  <ToolButton title={keyTitle("Save to File", "ddl.save")} onClick={() => void saveDdl(t)}>Save to File</ToolButton>
                  <ToolButton title={keyTitle("Run in a console", "ddl.run")} onClick={() => runDdl(t)}>Run in Console</ToolButton>
                </Toolbar>
                {/* Stays mounted (loads once per tab, on first show; a restored or background tab doesn't connect) */}
                {t.edit && <TableStructureView key={structureRev[t.id] ?? 0} target={t.target} schema={t.edit.schema} table={t.edit.table} visible={view === "structure" && t.id === ga?.id} onShowSql={() => pick("sql")} />}
                <div className="ddl-editor" style={{ display: view === "sql" ? undefined : "none" }}>
                <ConsoleEditor
                  tabId={t.id}
                  connId={t.target}
                  database={tabDatabase(t, conns.find((x) => x.id === t.connId))}
                  initialSql={t.sql}
                  language={dialectOf(conns.find((x) => x.id === t.connId)?.kind).monaco}
                  readOnly
                  onChange={() => {}}
                  onRun={() => {}}
                  registerApi={() => {}}
                />
                </div>
              </div>
              );
            })}
        </div>
      </div>
    );
  };
  return { renderPanes, splitClone, unsplit, reopen, reopenClosed, renameConsole, tabMenu, ddlCommands };
}
