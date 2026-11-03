import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ask, askText, notice } from "./lib/dialogs";
import { listen } from "@tauri-apps/api/event";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { api, backup, ConnectionView, ddl, dialectOf, meta, VECTOR_KINDS, redisDb, targetId, updateCheckEnabled } from "./lib/api";
import { RedisBrowser } from "./components/RedisBrowser";
import { KeymapDialog } from "./components/KeymapDialog";
import { UpdateDialog } from "./components/UpdateDialog";
import { ContextMenu, Explorer, MenuItem } from "./components/Explorer";
import { ConnectionDialog } from "./components/ConnectionDialog";
import { ConsoleEditor } from "./components/ConsoleEditor";
import { ConsolePane } from "./components/ConsolePane";
import { HistoryDialog, migrateOldHistory, saveSnippet } from "./components/HistoryDialog";
import { ProductionLog } from "./components/ProductionLog";
import { CopyRowsDialog, GenerateDataDialog } from "./components/DataTools";
import { NewKind, NewObjectDialog, TableDesigner } from "./components/ObjectEditors";
import { EditTarget } from "./components/ResultGrid";
import { TableEditor } from "./components/TableEditor";
import { TlsFix } from "./components/TlsFix";
import { RagView } from "./components/RagView";
import { WorkspaceBar } from "./components/WorkspaceBar";
import { TextDialog } from "./components/TextDialog";
import { Badge, CodeBlock, ConfirmHost, Overlay, Spacer, TabItem, TabStrip, Text, ToolButton, Toolbar, ToolbarSep } from "./ui";
import { RecentFiles, Switcher, SwitchItem } from "./components/Switcher";
import { ActionItem, DbObject, invalidateSearchIndex, SearchEverywhere, SeTab } from "./components/SearchEverywhere";
import { FindPanel, FindRun, FtsScope, FullTextSearchDialog } from "./components/FullTextSearch";
import { DEFAULT_FONT, editorFontSize, setEditorFontSize } from "./lib/prefs";
import { bindings, eventSpec, formatSpec, hit, loadKeymap, onKeymapChange } from "./lib/shortcuts";
import { invalidateCompletion } from "./lib/completion";
import { useSharedReady } from "./lib/shared";
import { tableTabLabels } from "./lib/tabLabels";
import type { BackupOptions, FtsRequest } from "./lib/api";
import { isRelation, NodeInfo, revealInTree, schemaChanged } from "./components/SchemaTree";
import { leaf, leaves, MAX_PANES, Pane, splitPane } from "./lib/panes";
import { connBg, connStripe, getThemePref, setThemePref, THEME_LABEL, ThemePref, useTheme } from "./lib/theme";
import { BackupDialog, ExportDialog, ImportDialog } from "./components/Transfer";
import { DbaReport } from "./components/DbaReport";
import { ErdView } from "./components/ErdView";
import { MigrateView } from "./components/MigrateView";
import { Group, snapKey, Tab, TabSnapshot } from "./features/tabs/types";
import { isMac, sk } from "./features/shortcuts/labels";
import { offered, SettingsDialog, upToDate } from "./features/settings/SettingsDialog";
import { StatusBar } from "./features/settings/StatusBar";
import { editorApis, liveSql, setActiveIds, setFocused, setPanes, setTabs, useTabsStore } from "./features/tabs/store";
import { allTabs, closeTabs, moveTo, patch, settle, sqlOf, togglePin, updTab } from "./features/tabs/ops";
import { setWorkspaces, stash, useWorkspacesStore } from "./features/workspaces/store";
import { useWorkspaces } from "./features/workspaces/useWorkspaces";
import { useConsole } from "./features/console/useConsole";
import { useTabs } from "./features/tabs/useTabs";
import { useJobs } from "./features/jobs/useJobs";
import { useConnActions } from "./features/conns/useConnActions";
import { setBackupReq, setCopyReq, setDialog, setExportReq, setGenReq, setHistoryFor, setHistoryTab, setImportReq, setKeymapOpen, setObjEdit, setProdLog, setQuickDoc, setSchemeOpen, setSettingsOpen, setUpdate, setUpdateOpen, useDialogsStore } from "./features/dialogs/store";
import "./App.css";

/** A tab's database by name: its own, else the connection's default (a tab on the default keeps none) */
const tabDatabase = (t: { database?: string }, c?: ConnectionView) => t.database || c?.database || (c?.kind === "mysql" ? "" : "postgres");

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

  const { dbCache, schemaCache, loadDbList, loadSchemaList, switchDatabase, switchSchema, run, log, reconnectAfterTls, focusTableGrid, executeConsole, fetchMoreEntry, setTxMode, txEnd, applyTableFilter, fetchMore, cancel, setPendingRuns, canCancel, cancelRun, submitEdits, loadStale } = useConsole({ conns, setStatus, refresh });
  const { activate, addTab, openConsole } = useTabs({ setStatus, loadStale });
  const { readyToQuit, switchWs, newWs, renameWs, deleteWs, moveToWs, setInWs, shownWs } = useWorkspaces({ conns, setStatus, loadStale });

  const openTable = async (c: ConnectionView, schema: string, table: string, db?: string) => {
    const target = targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "table" && t.target === target && t.edit?.schema === schema && t.edit?.table === table);
    if (existing) return activate(existing);
    return openTableTab(c, schema, table, db);
  };

  /** Open a new table data tab (when duplicated by split, a separate tab with independent filter and sort) */
  const openTableTab = async (c: ConnectionView, schema: string, table: string, db?: string, group?: Group, where = "", orderBy = "") => {
    const target = targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
    const { q } = dialectOf(c.kind);
    let sql = `SELECT * FROM ${q(schema)}.${q(table)}`;
    if (where) sql += ` WHERE ${where}`;
    if (orderBy) sql += ` ORDER BY ${orderBy}`;
    sql += ";";
    let edit: EditTarget = { schema, table, pk: [] };
    try {
      const d = await meta.table(target, schema, table);
      edit = { schema, table, pk: d.columns.filter((x) => x.isPk).map((x) => x.name) };
    } catch {
      /* Read-only if the metadata lookup fails */
    }
    const tab = addTab(c, "table", { title: table, sql, database: db, tableSql: sql, edit, group, where, orderBy });
    run(tab, sql);
  };

  const openDdl = async (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    const target = targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
    const title = `${info.name} [DDL]`;
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "ddl" && t.target === target && t.title === title);
    if (existing) return activate(existing);
    try {
      setStatus(`Loading DDL of ${info.schema}.${info.name}…`);
      const ddl = await meta.ddl(target, info.schema, info.name, info.kind);
      addTab(info.conn, "ddl", { title, sql: ddl, database: info.db });
      setStatus("Ready");
    } catch (e) {
      setStatus(`DDL failed: ${String(e)}`);
    }
  };

  /** SQL Generator: build a statement for the selected table and put it in a new console */
  const generate = async (info: NodeInfo, what: "select" | "insert" | "update" | "delete" | "ddl") => {
    if (!info.schema || !info.name) return;
    const target = targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
    const quoteIdent = dialectOf(info.conn.kind).q;
    const full = `${quoteIdent(info.schema)}.${quoteIdent(info.name)}`;
    try {
      let sql: string;
      if (what === "ddl") sql = await meta.ddl(target, info.schema, info.name, info.kind);
      else {
        const d = await meta.table(target, info.schema, info.name);
        const cols = d.columns;
        const pk = cols.filter((c) => c.isPk);
        const keys = pk.length ? pk : cols.slice(0, 1);
        const ph = (c: (typeof cols)[number]) => `NULL /* ${c.name} ${c.dataType} */`;
        const where = keys.map((c) => `${quoteIdent(c.name)} = ${ph(c)}`).join("\n  AND ");
        if (what === "select") sql = `SELECT ${cols.map((c) => `t.${quoteIdent(c.name)}`).join(",\n       ")}\nFROM ${full} t;`;
        else if (what === "insert")
          sql = `INSERT INTO ${full} (${cols.map((c) => quoteIdent(c.name)).join(", ")})\nVALUES (${cols.map(ph).join(",\n        ")});`;
        else if (what === "update")
          sql = `UPDATE ${full}\nSET ${cols.filter((c) => !c.isPk).map((c) => `${quoteIdent(c.name)} = ${ph(c)}`).join(",\n    ")}\nWHERE ${where};`;
        else sql = `DELETE\nFROM ${full}\nWHERE ${where};`;
      }
      openConsole(info.conn, info.db, sql);
    } catch (e) {
      setStatus(`Generate failed: ${String(e)}`);
    }
  };

  const exportReq = useDialogsStore((s) => s.exportReq);
  const importReq = useDialogsStore((s) => s.importReq);
  const genReq = useDialogsStore((s) => s.genReq);
  const objEdit = useDialogsStore((s) => s.objEdit);
  const copyReq = useDialogsStore((s) => s.copyReq);
  const { jobs, runJob, startExport, startImport, exportCsv } = useJobs({ setStatus });
  const { setColor, setScope, exportConns, importConns, removeConn } = useConnActions({ refresh, setStatus });
  const exportTable = (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    const { q } = dialectOf(info.conn.kind);
    setExportReq({ target: nodeTarget(info), sql: `SELECT * FROM ${q(info.schema)}.${q(info.name)}`, name: info.name, table: [info.schema, info.name] });
  };

  const dangerous = async (info: NodeInfo, verb: "TRUNCATE" | "DROP") => {
    if (!info.schema || !info.name) return;
    const target = targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
    const kw = verb === "DROP" ? (info.kind === "view" ? "VIEW" : info.kind === "matview" ? "MATERIALIZED VIEW" : info.kind === "sequence" ? "SEQUENCE" : "TABLE") : "TABLE";
    const { q } = dialectOf(info.conn.kind);
    const sql = `${verb} ${verb === "DROP" ? kw + " " : ""}${q(info.schema)}.${q(info.name)};`;
    if (!(await ask("Execute this statement?", { title: info.conn.name, detail: [{ text: sql }], okLabel: "Execute" }))) return;
    try {
      const r = await api.executeBatch(target, [sql]);
      setStatus(`Executed: ${sql}${r.warning ? ` · Warning: ${r.warning}` : ""}`);
      (verb === "DROP" ? info.refreshParent ?? info.refresh : info.refresh)?.();
    } catch (e) {
      setStatus(String(e));
    }
  };

  /** Redis browser tab (Medis style) */
  const openRedis = (c: ConnectionView, db?: string, key?: string) => {
    const database = redisDb(db ?? c.database);
    const target = targetId(c.id, database);
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "redis" && t.target === target);
    if (existing) {
      if (key) patch(existing.id, { redisKey: key });
      return activate(existing);
    }
    const tab = addTab(c, "redis", { title: `${c.name} [${database}]`, database, group: undefined });
    patch(tab.id, { target, redisKey: key });
  };

  /** DBA report tab (for the same connection target, only switch the report in that tab) */
  const openDba = (c: ConnectionView, db?: string, report = "sessions", title = "Sessions") => {
    const database = db && db !== (c.database || "postgres") ? db : undefined;
    const target = targetId(c.id, database);
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "dba" && t.target === target);
    if (existing) {
      patch(existing.id, { dbaReport: report, title: `${title} — ${c.name}` });
      return activate(existing);
    }
    const tab = addTab(c, "dba", { title: `${title} — ${c.name}`, database: db });
    patch(tab.id, { dbaReport: report });
  };

  /** ERD tab: diagram of the schemas' tables and foreign keys (if none, all user schemas of this DB) */
  const openErd = async (c: ConnectionView, db: string | undefined, schemas?: string[]) => {
    const database = db && db !== (c.database || "postgres") ? db : undefined;
    const target = targetId(c.id, database);
    let list = schemas;
    if (!list) {
      const all = await meta.schemas(target).catch(() => []);
      list = all.filter((x) => !x.isSystem).map((x) => x.name);
    }
    const title = `Diagram: ${list.length === 1 ? list[0] : `${list.length} schemas`} — ${c.name}`;
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "erd" && t.target === target && (t.erdSchemas ?? []).join(",") === list!.join(","));
    if (existing) return activate(existing);
    const tab = addTab(c, "erd", { title, database: db });
    patch(tab.id, { erdSchemas: list });
  };

  /** Compare & Migrate tab (only one) */
  const openMigrate = (c: ConnectionView, db?: string) => {
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "migrate");
    if (existing) return activate(existing);
    addTab(c, "migrate", { title: "Compare & Migrate", database: db });
  };

  /** AI Search (RAG) tab (only one). Opened from a table, it starts a new index for that table */
  const [ragInit, setRagInit] = useState<{ n: number; connId: string; db?: string; schema?: string; table?: string } | undefined>();
  const openRag = (c: ConnectionView, db?: string, schema?: string, table?: string) => {
    // When n changes, redraw the tab to open the new index form for that table
    setRagInit((x) => ({ n: (x?.n ?? 0) + 1, connId: c.id, db, schema, table }));
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "rag");
    if (existing) return activate(existing);
    addTab(c, "rag", { title: "AI Search (RAG)", database: db });
  };

  /** Tree node double-click / F4 */
  const activateNode = (info: NodeInfo) => {
    if (info.conn.kind === "redis") {
      if (info.kind === "key") return openRedis(info.conn, info.db, info.name);
      if (info.kind === "conn" || info.kind === "db") return openRedis(info.conn, info.db);
    }
    if (isRelation(info.kind) && info.schema && info.name) openTable(info.conn, info.schema, info.name, info.db);
    else if (info.kind === "routine" || info.kind === "sequence") openDdl(info);
  };

  // ───────── Database Explorer actions (DataGrip keymap) ─────────
  const K = { modify: sk("tree.modify"), rename: sk("tree.rename"), edit: sk("tree.editData"), props: sk("tree.properties"), refresh: sk("tree.refresh"), deact: sk("tree.disconnect"), jump: sk("tree.jumpConsole"), ddl: sk("tree.ddl"), gen: sk("tree.sqlGen"), ddlClip: sk("tree.ddlCopy"), ddlCons: sk("tree.ddlConsole"), ref: sk("tree.copyRef"), doc: sk("tree.quickDoc"), drop: sk("tree.remove"), dup: sk("tree.duplicate"), console: sk("console.new"), force: bindings("tree.refresh")[1] ? formatSpec(bindings("tree.refresh")[1]) : undefined, newK: sk("tree.newDs") };
  const quickDoc = useDialogsStore((s) => s.quickDoc);
  // ───────── Backup / restore ─────────
  const backupReq = useDialogsStore((s) => s.backupReq);
  const dbTarget = (c: ConnectionView, db?: string) => targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
  const startBackup = async (o: Omit<BackupOptions, "schemas">) => {
    const r0 = backupReq;
    setBackupReq(null);
    if (!r0) return;
    const name = `${r0.db ?? r0.c.database ?? r0.c.name}-${new Date().toISOString().slice(0, 10)}`;
    const r = await runJob(`Back up ${name}`, (job) => backup.run(dbTarget(r0.c, r0.db), { ...o, schemas: [] }, name, job));
    if (r) setStatus(`Backed up ${r[0] < 1048576 ? `${Math.max(1, Math.round(r[0] / 1024))} KB` : `${(r[0] / 1048576).toFixed(1)} MB`} to ${r[1]}`);
  };
  const startRestore = async (c: ConnectionView, db?: string) => {
    const where = `${c.name}${db ? ` / ${db}` : ""}`;
    const mysql = c.kind === "mysql";
    const how = mysql
      ? "Objects in the backup are dropped and recreated (by the backup's DROP … IF EXISTS statements); the rest of the database is left alone. A MySQL restore isn't atomic: if a statement fails, what ran before it stays."
      : "It runs the backup's statements on this database in one transaction: if a statement fails, nothing changes.";
    if (!(await ask(`Restore a backup into ${where}?\n\n${how}${c.options?.confirmChanges ? "\n\n⚠ This is a Production data source." : ""}`))) return;
    // --clean is a pg_restore option: a MySQL dump drops its objects itself
    const clean = !mysql && (await ask("Drop the objects the backup contains before restoring them (--clean)?", { title: "Restore", okLabel: "Drop and Recreate", cancelLabel: "Keep and Add" }));
    const r = await runJob(`Restore into ${where}`, (job) => backup.restore(dbTarget(c, db), clean, job));
    if (r) setStatus(`Restored ${r} into ${where}`);
  };
  const nodeTarget = (info: NodeInfo) => targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
  const fqn = (info: NodeInfo) => [info.db, info.schema, info.name].filter(Boolean).join(".") || info.conn.name;
  const ddlText = (info: NodeInfo) => meta.ddl(nodeTarget(info), info.schema!, info.name!, info.kind);

  const ddlToClipboard = async (info: NodeInfo) => {
    try {
      await navigator.clipboard.writeText(await ddlText(info));
      setStatus(`DDL of ${info.schema}.${info.name} copied to clipboard`);
    } catch (e) {
      setStatus(String(e));
    }
  };
  const ddlToConsole = async (info: NodeInfo) => {
    try {
      openConsole(info.conn, info.db, await ddlText(info));
    } catch (e) {
      setStatus(String(e));
    }
  };
  /** SQL Generator tool window: DDL + Copy / Save to File / Run in Console */
  const sqlGenerator = async (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    try {
      const ddl = await ddlText(info);
      addTab(info.conn, "ddl", { title: `SQL Generator: ${info.name}`, sql: ddl, database: info.db });
    } catch (e) {
      setStatus(String(e));
    }
  };
  const showQuickDoc = async (info: NodeInfo) => {
    if (info.kind === "conn") return setQuickDoc({ title: info.conn.name, text: `${info.conn.user}@${info.conn.host}:${info.conn.port}${info.conn.database ? "/" + info.conn.database : ""}${info.conn.ssh?.enabled ? `\nvia SSH ${info.conn.ssh.user}@${info.conn.ssh.host}` : ""}${info.conn.ssm?.enabled ? `\nvia AWS SSM ${info.conn.ssm.target}` : ""}` });
    if (!info.schema || !info.name) return setQuickDoc({ title: fqn(info), text: info.kind });
    try {
      setQuickDoc({ title: fqn(info), text: await ddlText(info) });
    } catch (e) {
      setQuickDoc({ title: fqn(info), text: String(e) });
    }
  };
  /** Jump to Query Console…: this data source's consoles + New Query Console */
  const jumpToConsole = (info: NodeInfo, x?: number, y?: number) => {
    const consoles = useTabsStore.getState().tabs.filter((t) => t.kind === "console" && t.connId === info.conn.id);
    const el = document.querySelector(".ws-explorer.on .tree-node.selected")?.getBoundingClientRect();
    setTabMenu({
      x: x ?? (el ? el.left + 40 : 300),
      y: y ?? (el ? el.bottom : 200),
      items: [
        { label: "New Query Console", shortcut: K.console, action: () => openConsole(info.conn, info.db) },
        ...(consoles.length ? ([{ separator: true }] as MenuItem[]) : []),
        ...consoles.map((t) => ({ label: t.title, action: () => activate(t) })),
      ],
    });
  };
  const deactivate = async (c: ConnectionView) => {
    const pending = allTabs().filter((t) => t.connId === c.id && t.inTx);
    if (pending.length && !(await ask(`${pending.length} console(s) on ${c.name} have uncommitted changes. Disconnecting rolls them back. Continue?`, { okLabel: "Disconnect and Roll Back", danger: true }))) return;
    await api.disconnect(c.id);
    for (const t of pending) updTab(t.id, (x) => ({ ...x, inTx: false }));
    invalidateCompletion(c.id);
    setStatus(`Deactivated ${c.name}`);
    refresh();
  };
  const duplicateConn = async (c: ConnectionView) => {
    const { hasPassword: _h, connected: _c, id: _id, ...config } = c;
    // A copy is a new data source: it starts hidden from MCP like any other new one
    const saved = await api.saveConnection({ ...config, id: "", name: `${c.name} (copy)`, options: { ...(config.options ?? {}), mcp: "off" } });
    await refresh();
    setDialog({ edit: { ...c, ...saved, hasPassword: false, connected: false } });
  };

  /** The node's schema (in MySQL the DB is the schema, in PG it is public) */
  const schemaOf = (info: NodeInfo) => info.schema ?? (info.conn.kind === "mysql" ? info.db ?? info.conn.database : "public");
  const canDdl = (c: ConnectionView) => c.kind === "postgres" || c.kind === "mysql";
  const newObject = (info: NodeInfo, newKind: NewKind | "table") =>
    setObjEdit({ target: newKind === "database" ? info.conn.id : nodeTarget(info), conn: info.conn, db: info.db, schema: schemaOf(info), newKind: newKind === "table" ? undefined : newKind, refresh: info.kind === "conn" || info.kind === "db" ? info.refresh : undefined });
  const modifyTable = (info: NodeInfo) => setObjEdit({ target: nodeTarget(info), conn: info.conn, db: info.db, schema: schemaOf(info), table: info.name });
  /** Rename… (⇧F6): ask for the name, show the DDL, and run it */
  const renameNode = async (info: NodeInfo) => {
    const what = info.kind;
    const old = info.name ?? (what === "schema" ? info.schema : what === "db" ? info.db : undefined);
    if (!old || !canDdl(info.conn)) return;
    const name = await askText(`Rename ${what === "db" ? "database" : what} "${old}"`, old);
    if (!name || name === old) return;
    try {
      const sql = await ddl.rename(what === "db" ? info.conn.id : nodeTarget(info), what, schemaOf(info), old, name);
      const c = info.conn;
      if (!(await ask(c.options?.confirmChanges ? "This changes the schema of a Production data source." : `Rename "${old}" to "${name}"?`, { title: c.options?.confirmChanges ? `Production: ${c.name}` : "Rename", detail: [{ text: sql, value: "schema change", tone: "danger" }], okLabel: "Rename", danger: !!c.options?.confirmChanges }))) return;
      // Ran, but the Production log couldn't record it: not a failure (running it again would repeat it)
      const warning = await ddl.run(what === "db" ? c.id : nodeTarget(info), [sql]);
      setStatus(`Renamed ${old} to ${name}`);
      if (warning) void notice(warning, "Ran, with a warning");
      if (what === "schema" || what === "db") info.refresh?.();
      else info.refreshParent?.();
      schemaChanged(c.id, info.db ?? (c.database || (c.kind === "postgres" ? "postgres" : "")));
    } catch (e) {
      setStatus(String(e));
    }
  };

  /** DataGrip tree context menu (structure from docs 1.11) */
  const buildMenu = (info: NodeInfo): MenuItem[] => {
    const c = info.conn;
    const isObj = !!info.name;
    const rel = isRelation(info.kind);
    const pg = c.kind === "postgres";
    const objects: MenuItem[] = canDdl(c)
      ? [
          { separator: true },
          { label: "Table…", action: () => newObject(info, "table") },
          { label: "View…", action: () => newObject(info, "view") },
          ...(pg ? ([{ label: "Materialized View…", action: () => newObject(info, "matview") }, { label: "Sequence…", action: () => newObject(info, "sequence") }] as MenuItem[]) : []),
          ...(pg && info.kind !== "schema" && !info.name ? ([{ label: "Schema…", action: () => newObject(info, "schema") }] as MenuItem[]) : []),
          ...(info.kind === "conn" || info.kind === "db" ? ([{ label: "Database…", action: () => newObject(info, "database") }] as MenuItem[]) : []),
        ]
      : [];
    const items: MenuItem[] = [
      { label: "New", submenu: [{ label: "Query Console", shortcut: K.console, action: () => openConsole(c, info.db) }, ...objects, { separator: true }, { label: "Data Source…", action: () => setDialog({}) }] },
    ];
    if (canDdl(c) && (info.kind === "table" || info.kind === "partitioned")) items.push({ label: "Modify Table…", shortcut: K.modify, action: () => modifyTable(info) });
    if (canDdl(c) && (["table", "partitioned", "view", "matview", "sequence"].includes(info.kind) || (pg && (info.kind === "schema" || info.kind === "db"))))
      items.push({ label: "Rename…", shortcut: K.rename, action: () => renameNode(info) });
    if (rel) items.push({ label: "Edit Data", shortcut: K.edit, action: () => activateNode(info) });
    if (info.kind === "conn") items.push({ label: "Properties", shortcut: K.props, action: () => setDialog({ edit: c }) });
    items.push({ separator: true });
    items.push({
      label: "Copy/Paste",
      submenu: [
        { label: "Copy Reference", shortcut: K.ref, action: () => navigator.clipboard.writeText(fqn(info)) },
        ...(info.kind === "conn" ? ([{ label: "Duplicate", shortcut: K.dup, action: () => duplicateConn(c) }] as MenuItem[]) : []),
      ],
    });
    items.push({ label: "Refresh", shortcut: K.refresh, action: () => info.refresh?.() });
    if (info.kind === "conn" && c.connected) items.push({ label: "Deactivate", shortcut: K.deact, action: () => deactivate(c) });
    items.push({ label: "Quick Documentation", shortcut: K.doc, action: () => showQuickDoc(info) });
    items.push({ separator: true });
    items.push({
      label: "Navigation",
      submenu: [
        ...(isObj ? ([{ label: "Go to DDL", shortcut: K.ddl, action: () => openDdl(info) }] as MenuItem[]) : []),
        { label: "Jump to Query Console…", shortcut: K.jump, action: () => jumpToConsole(info) },
      ],
    });
    if (info.kind === "table" && c.kind !== "redis" && c.kind !== "mongodb" && !VECTOR_KINDS.includes(c.kind)) items.push({ label: "AI Search (RAG) — Index This Table…", action: () => openRag(c, info.db, info.schema, info.name) });
    if (isObj)
      items.push({
        label: "SQL Scripts",
        submenu: [
          { label: "SQL Generator…", shortcut: K.gen, action: () => sqlGenerator(info) },
          { label: "Generate DDL to Clipboard", shortcut: K.ddlClip, action: () => ddlToClipboard(info) },
          { label: "Generate DDL to Query Console", shortcut: K.ddlCons, action: () => ddlToConsole(info) },
          ...(rel
            ? ([
                { separator: true },
                { label: "SELECT Statement", action: () => generate(info, "select") },
                ...(info.kind === "table" || info.kind === "partitioned"
                  ? ([
                      { label: "INSERT Statement", action: () => generate(info, "insert") },
                      { label: "UPDATE Statement", action: () => generate(info, "update") },
                      { label: "DELETE Statement", action: () => generate(info, "delete") },
                    ] as MenuItem[])
                  : []),
              ] as MenuItem[])
            : []),
        ],
      });
    if (isObj && info.kind !== "routine")
      items.push({
        label: "Object Actions",
        submenu: [
          ...(info.kind === "table" || info.kind === "partitioned" ? ([{ label: "Truncate…", danger: true, action: () => dangerous(info, "TRUNCATE") }] as MenuItem[]) : []),
          { label: "Drop…", shortcut: K.drop, danger: true, action: () => dangerous(info, "DROP") },
        ],
      });
    if (rel)
      items.push({
        label: "Import/Export",
        submenu: [
          { label: "Export Data…", action: () => exportTable(info) },
          ...(info.kind === "table" || info.kind === "partitioned"
            ? ([
                { label: "Import Data from File…", action: () => setImportReq({ target: nodeTarget(info), schema: info.schema!, table: info.name }) },
                { label: "Generate Test Data…", action: () => setGenReq({ target: nodeTarget(info), schema: info.schema ?? "", table: info.name!, conn: c }) },
                ...(c.kind === "postgres" || c.kind === "mysql" ? ([{ label: "Copy Related Rows To…", action: () => setCopyReq({ target: nodeTarget(info), schema: info.schema ?? "", table: info.name!, conn: c }) }] as MenuItem[]) : []),
              ] as MenuItem[])
            : []),
        ],
      });
    if (c.kind !== "redis" && (info.kind === "schema" || info.kind === "db" || (info.kind === "conn" && c.kind === "mysql")))
      items.push({ label: "Diagrams", submenu: [{ label: "Show Diagram (ERD)", action: () => openErd(c, info.db, info.kind === "schema" && info.schema ? [info.schema] : c.kind === "mysql" ? [info.db ?? c.database] : undefined) }] });
    if (info.kind === "schema" && info.schema && c.kind !== "redis")
      items.push({ label: "Import/Export", submenu: [{ label: "Import Data into New Table…", action: () => setImportReq({ target: nodeTarget(info), schema: info.schema!, refresh: info.refresh }) }] });
    items.push({
      label: "Tools",
      submenu: [
        { label: "Manage Shown Schemas…", action: () => document.querySelector<HTMLElement>(".ws-explorer.on .tree-node.conn.selected .scope-badge, .ws-explorer.on .tree-node.selected .scope-badge")?.click() },
        ...(c.kind !== "redis" && (info.kind === "conn" || info.kind === "db")
          ? ([
              { separator: true },
              { label: "Sessions & Locks", action: () => openDba(c, info.db, "sessions", "Sessions") },
              { label: "Slow Queries", action: () => openDba(c, info.db, "slow_queries", "Slow Queries (Top 100)") },
              { label: "Table Sizes & Indexes", action: () => openDba(c, info.db, "table_sizes", "Table Sizes") },
              { label: "Server Overview", action: () => openDba(c, info.db, "server", "Server Overview") },
              { label: "Users & Privileges", action: () => openDba(c, info.db, "roles", "Users & Roles") },
              { separator: true },
              { label: "Compare & Migrate (schemas, data, migration files)…", action: () => openMigrate(c, info.db) },
              { label: "AI Search (RAG)…", action: () => openRag(c, info.db) },
              { separator: true },
              { label: "Back Up Database…", action: () => setBackupReq({ c, db: info.db }) },
              { label: "Restore Database…", action: () => startRestore(c, info.db) },
            ] as MenuItem[])
          : []),
      ],
    });
    if (info.kind === "conn") {
      items.push({ label: "Diagnostics", submenu: [{ label: "Force Refresh", shortcut: K.force, action: () => (invalidateSearchIndex(c.id), invalidateCompletion(c.id), info.refresh?.()) }] });
      items.push({ separator: true });
      const others = workspaces.filter((w) => w.id !== curWs && !w.connIds.includes(c.id));
      if (others.length) items.push({ label: "Add to Workspace", submenu: others.map((w) => ({ label: w.name, action: () => setInWs(w.id, c.id, true) })) });
      items.push({ label: "Remove from This Workspace", action: () => setInWs(curWs, c.id, false) });
      items.push({ label: "Remove Data Source", shortcut: K.drop, danger: true, action: () => removeConn(c) });
    }
    return items;
  };


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
      openTableTab(c, snap.edit.schema, snap.edit.table, snap.database, undefined, snap.where, snap.orderBy);
    } else addTab(c, snap.kind, { title: snap.title, sql: snap.sql, database: snap.database });
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
    if (t.kind === "table" && t.edit) openTableTab(c, t.edit.schema, t.edit.table, t.database, r.g, t.where, t.orderBy);
    else addTab(c, t.kind, { title: t.kind === "console" ? `${t.title} (copy)` : t.title, sql: sqlOf(t), database: t.database, group: r.g });
  };

  const openTabMenu = (e: React.MouseEvent, t: Tab) => {
    e.preventDefault();
    e.stopPropagation();
    activate(t);
    const sameGroup = tabs.filter((x) => x.group === t.group);
    const idx = sameGroup.findIndex((x) => x.id === t.id);
    const ls = leaves(panes);
    const other: Group = ls[(ls.indexOf(t.group) + 1) % ls.length];
    const canSplit = ls.length < MAX_PANES;
    const c = conns.find((x) => x.id === t.connId);
    const items: MenuItem[] = [
      { label: "Close", shortcut: sk("tab.close"), action: () => closeTabs([t.id]) },
      { label: "Close Other Tabs", action: () => closeTabs(sameGroup.filter((x) => x.id !== t.id && !x.pinned).map((x) => x.id)) },
      { label: "Close All but Pinned", action: () => closeTabs(sameGroup.filter((x) => !x.pinned).map((x) => x.id)) },
      { label: "Close Tabs to the Left", action: () => closeTabs(sameGroup.slice(0, idx).filter((x) => !x.pinned).map((x) => x.id)), disabled: idx === 0 },
      { label: "Close Tabs to the Right", action: () => closeTabs(sameGroup.slice(idx + 1).filter((x) => !x.pinned).map((x) => x.id)), disabled: idx === sameGroup.length - 1 },
      { label: "Close All Tabs", action: () => closeTabs(sameGroup.map((x) => x.id)) },
      { label: "Reopen Closed Tab", action: reopenClosed, disabled: useTabsStore.getState().closedStack.length === 0 },
      { separator: true },
      { label: t.pinned ? "Unpin Tab" : "Pin Tab", action: () => togglePin(t) },
      {
        label: "Copy Path/Reference…",
        submenu: [
          { label: "Copy Reference", action: () => navigator.clipboard.writeText(t.edit ? `${t.edit.schema}.${t.edit.table}` : t.title) },
          ...(c ? ([{ label: "Copy Data Source Name", action: () => navigator.clipboard.writeText(c.name) }] as MenuItem[]) : []),
        ],
      },
      { separator: true },
      { label: "Split Right", action: () => splitClone(t, "right"), disabled: !canSplit },
      { label: "Split Down", action: () => splitClone(t, "down"), disabled: !canSplit },
      { label: "Split and Move Right", action: () => splitAndMove(t, "right"), disabled: !canSplit || sameGroup.length < 2 },
      { label: "Split and Move Down", action: () => splitAndMove(t, "down"), disabled: !canSplit || sameGroup.length < 2 },
      ...(split
        ? ([
            { label: "Move To Next Group", action: () => moveTo(t.id, other) },
            { label: "Open In Next Group", action: () => { const cc = c; if (!cc) return; if (t.kind === "table" && t.edit) openTableTab(cc, t.edit.schema, t.edit.table, t.database, other, t.where, t.orderBy); else addTab(cc, t.kind, { title: t.title, sql: sqlOf(t), database: t.database, group: other }); } },
            { label: "Unsplit All", action: unsplit },
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
        { label: "Duplicate", action: () => addTab(c, "console", { title: `${t.title} (copy)`, sql: sqlOf(t), database: t.database, group: t.group }) },
        { label: "Rename File…", shortcut: "⇧F6", action: () => renameConsole(t) },
      );
    setTabMenu({ x: e.clientX, y: e.clientY, items });
  };

  /** Refresh (⌥⌘y, Find Action): Data Source list + search and autocomplete caches */
  const refreshAll = () => {
    invalidateSearchIndex();
    for (const c of conns) invalidateCompletion(c.id);
    refresh();
  };

  const renameConsole = async (t: Tab) => {
    const name = (await askText("Rename console", t.title));
    if (name && name.trim()) patch(t.id, { title: name.trim() });
  };

  // ───────── Switcher / Recent Files / Maximize ─────────
  const [switcher, setSwitcher] = useState<{ items: SwitchItem[]; start: number } | null>(null);
  const [recentOpen, setRecentOpen] = useState(false);
  const [maximized, setMaximized] = useState(false);
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
      <div className={"panel editor-group" + (split && focused === g ? " focused" : "")} onMouseDownCapture={() => setFocused(g)}>
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
                icon={t.kind === "table" ? "▦" : t.kind === "ddl" ? "ƒ" : t.kind === "redis" ? "◆" : t.kind === "dba" ? "◉" : t.kind === "erd" ? "⧉" : t.kind === "migrate" ? "⇄" : t.kind === "rag" ? "✦" : "⌘"}
                iconClass={t.kind}
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
                style={color ? { background: connBg(color, theme), boxShadow: `inset 0 2px 0 ${connStripe(color, theme)}` } : undefined}
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
                    onExecute={(stmts, explain) => executeConsole(t.id, stmts, explain)}
                    onCancel={() => cancel(t)}
                    onSetActiveResult={(id) => patch(t.id, { activeResult: id })}
                    onPin={(id, pinned) => setTabs((ts) => ts.map((x) => (x.id === t.id ? { ...x, results: x.results.map((r) => (r.id === id ? { ...r, pinned } : r)) } : x)))}
                    onCloseResult={(id) =>
                      setTabs((ts) => ts.map((x) => (x.id === t.id ? { ...x, results: x.results.filter((r) => r.id !== id), activeResult: x.activeResult === id ? "output" : x.activeResult } : x)))
                    }
                    onFetchMore={(entryId) => fetchMoreEntry(t.id, entryId)}
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
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <DbaReport
                    target={t.target}
                    connName={c?.name ?? ""}
                    report={t.dbaReport ?? "sessions"}
                    readOnly={!!c?.readOnly}
                    dialect={dialectOf(c?.kind ?? "postgres")}
                    visible={t.id === ga?.id}
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
                  title={t.title}
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
                  onApply={(w, o) => applyTableFilter(t, w, o)}
                  onCancel={() => cancel(t)}
                  onFetchMore={() => fetchMore(t)}
                  onSubmit={(st) => submitEdits(t, st)}
                  onExportCsv={() => exportCsv(t)}
                  onDdl={() => {
                    const c = conns.find((x) => x.id === t.connId);
                    if (c && t.edit) openDdl({ kind: "table", conn: c, db: t.database, schema: t.edit.schema, name: t.edit.table });
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
            .map((t) => (
              <div key={t.id} className="work-layer ddl-layer" style={{ display: t.id === ga?.id ? "flex" : "none" }}>
                <Toolbar>
                  <span className="tb-info" style={{ marginLeft: 0 }}>{t.title.startsWith("SQL Generator") ? "SQL Generator" : "DDL"} · read-only</span>
                  <Spacer />
                  <ToolButton title="Copy to Clipboard" onClick={() => { navigator.clipboard.writeText(t.sql); setStatus("Copied to clipboard"); }}>Copy</ToolButton>
                  <ToolButton
                    title="Save to File"
                    onClick={async () => {
                      try {
                        const path = await api.saveTextFile(`${t.title.replace(/^SQL Generator: | \[DDL\]$/g, "")}.sql`, t.sql);
                        if (path) setStatus(`Saved ${path}`);
                      } catch (e) {
                        setStatus(String(e));
                      }
                    }}
                  >
                    Save to File
                  </ToolButton>
                  <ToolButton title="Run in a console" onClick={() => { const c = conns.find((x) => x.id === t.connId); if (c) openConsole(c, t.database, t.sql); }}>Run in Console</ToolButton>
                </Toolbar>
                <div className="ddl-editor">
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
            ))}
        </div>
      </div>
    );
  };

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
