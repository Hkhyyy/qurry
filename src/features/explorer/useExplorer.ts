import { useState } from "react";
import { api, backup, ConnectionView, ddl, dialectOf, meta, redisDb, targetId, VECTOR_KINDS } from "../../lib/api";
import type { BackupOptions } from "../../lib/api";
import { ask, askText, notice } from "../../lib/dialogs";
import { invalidateCompletion } from "../../lib/completion";
import { hasStructure } from "../../lib/structure";
import { setTableColumns } from "../../lib/statusbar";
import { bindings, cmd, formatSpec } from "../../lib/shortcuts";
import type { MenuItem } from "../../components/Explorer";
import { forSelection } from "./selection";
import type { NewKind } from "../../components/ObjectEditors";
import type { EditTarget } from "../../components/ResultGrid";
import { invalidateSearchIndex } from "../../components/SearchEverywhere";
import { isRelation, NodeInfo, schemaChanged } from "../../components/SchemaTree";
import type { useConnActions } from "../conns/useConnActions";
import type { useConsole } from "../console/useConsole";
import { setBackupReq, setCopyReq, setDialog, setExportReq, setGenReq, setImportReq, setObjEdit, setQuickDoc, useDialogsStore } from "../dialogs/store";
import type { useJobs } from "../jobs/useJobs";
import { sk } from "../shortcuts/labels";
import { allTabs, patch, updTab } from "../tabs/ops";
import { useTabsStore } from "../tabs/store";
import type { Group } from "../tabs/types";
import type { useTabs } from "../tabs/useTabs";
import { useWorkspacesStore } from "../workspaces/store";
import type { useWorkspaces } from "../workspaces/useWorkspaces";

type Deps = {
  setStatus: (s: string) => void;
  refresh: () => Promise<void>;
  setTabMenu: (m: { x: number; y: number; items: MenuItem[] } | null) => void;
} & Pick<ReturnType<typeof useTabs>, "activate" | "addTab" | "openConsole"> &
  Pick<ReturnType<typeof useConsole>, "run"> &
  Pick<ReturnType<typeof useJobs>, "runJob"> &
  Pick<ReturnType<typeof useConnActions>, "removeConn"> &
  Pick<ReturnType<typeof useWorkspaces>, "setInWs">;

/** Opening tables, DDL and the tool tabs (Redis, DBA, ERD, Compare, RAG), and the Database Explorer's actions and context menu */
export function useExplorer({ setStatus, refresh, setTabMenu, activate, addTab, openConsole, run, runJob, removeConn, setInWs }: Deps) {
  const workspaces = useWorkspacesStore((s) => s.workspaces);
  const curWs = useWorkspacesStore((s) => s.curWs);
  const backupReq = useDialogsStore((s) => s.backupReq);
  const openTable = async (c: ConnectionView, schema: string, table: string, db?: string, objKind?: string) => {
    const target = targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "table" && t.target === target && t.edit?.schema === schema && t.edit?.table === table);
    if (existing) return activate(existing);
    return openTableTab(c, schema, table, db, undefined, "", "", objKind);
  };

  /** Open a new table data tab (when duplicated by split, a separate tab with independent filter and sort) */
  const openTableTab = async (c: ConnectionView, schema: string, table: string, db?: string, group?: Group, where = "", orderBy = "", objKind?: string) => {
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
      setTableColumns(target, schema, table, d.columns);
    } catch {
      /* Read-only if the metadata lookup fails */
    }
    const tab = addTab(c, "table", { title: table, sql, database: db, tableSql: sql, edit, group, where, orderBy, objKind });
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
      // A table also gets the Structure view, opened in the last choice
      addTab(info.conn, "ddl", { title, sql: ddl, database: info.db, edit: hasStructure(info.kind) ? { schema: info.schema, table: info.name, pk: [] } : undefined, objKind: info.kind, ddlOf: { schema: info.schema, name: info.name } });
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

  const exportTable = (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    const { q } = dialectOf(info.conn.kind);
    setExportReq({ target: nodeTarget(info), sql: `SELECT * FROM ${q(info.schema)}.${q(info.name)}`, name: info.name, table: [info.schema, info.name] });
  };

  /** Truncate… / Drop… of one or several objects of one database: one confirmation lists every statement (the Production one on a
   *  Production source), then they run like the object editors' DDL (ddl.run: one transaction on PostgreSQL, one at a time on MySQL,
   *  whose DDL can't roll back), through the read-only check and the Production log. Drop's statements come from the backend in an
   *  order that runs (an object before what it depends on; one statement per kind on PostgreSQL, one per object on MySQL) */
  const dangerous = async (infos: NodeInfo[], verb: "TRUNCATE" | "DROP") => {
    const objs = infos.filter((i) => i.schema && i.name);
    if (!objs.length) return;
    const c = objs[0].conn;
    const { q } = dialectOf(c.kind);
    let sqls: string[];
    try {
      sqls = verb === "DROP" ? await ddl.drop(nodeTarget(objs[0]), objs.map((i) => ({ kind: i.kind, schema: i.schema!, name: i.name! }))) : objs.map((i) => `TRUNCATE ${q(i.schema!)}.${q(i.name!)};`);
    } catch (e) {
      return setStatus(String(e));
    }
    const prod = !!c.options?.confirmChanges;
    const question = prod
      ? verb === "DROP" ? "This changes the schema of a Production data source." : "This deletes every row of a table in a Production data source."
      : sqls.length === 1 ? "Execute this statement?" : `Execute these ${sqls.length} statements?`;
    const detail = sqls.map((text) => ({ text, value: verb === "DROP" ? "schema change" : "all rows", tone: "danger" as const }));
    if (!(await ask(question, { title: prod ? `Production: ${c.name}` : c.name, detail, okLabel: "Execute", danger: true }))) return;
    try {
      // Ran, but the Production log couldn't record it: not a failure (running it again would repeat it)
      const warning = await ddl.run(nodeTarget(objs[0]), sqls);
      setStatus(`Executed: ${sqls.length === 1 ? sqls[0] : `${sqls.length} statements`}`);
      if (warning) void notice(warning, "Ran, with a warning");
    } catch (e) {
      setStatus(String(e));
    }
    // Each schema's list reloads once, also after a failure (on MySQL the first statements may have run)
    const reload = new Map(objs.map((i) => [`${i.db}.${i.schema}${verb === "DROP" ? "" : `.${i.name}`}`, verb === "DROP" ? i.refreshParent ?? i.refresh : i.refresh]));
    for (const fn of reload.values()) fn?.();
  };

  /** The data source's Redis tab (one per source, Medis style): opened, or switched to `db` (and `key`). `onlyIfOpen`: a
   *  click on a database in the explorer switches an open tab and opens none */
  const openRedis = (c: ConnectionView, db?: string, key?: string, onlyIfOpen = false) => {
    const existing = useTabsStore.getState().tabs.find((t) => t.kind === "redis" && t.connId === c.id);
    if (!existing && onlyIfOpen) return;
    const database = redisDb(db ?? existing?.database ?? c.database);
    const at = { database, target: targetId(c.id, database), title: `${c.name} [${database}]` };
    if (existing) {
      if (existing.target !== at.target) patch(existing.id, { ...at, redisKey: key });
      else if (key) patch(existing.id, { redisKey: key });
      return activate(existing);
    }
    const tab = addTab(c, "redis", { title: at.title, database, group: undefined });
    patch(tab.id, { target: at.target, redisKey: key });
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
  const activateNode = (info: NodeInfo, click = false) => {
    if (info.conn.kind === "redis") {
      if (info.kind === "key") return openRedis(info.conn, info.db, info.name);
      if (info.kind === "conn") return openRedis(info.conn);
      if (info.kind === "db") return openRedis(info.conn, info.db, undefined, click);
    }
    if (click) return;
    if (isRelation(info.kind) && info.schema && info.name) openTable(info.conn, info.schema, info.name, info.db, info.kind);
    else if (info.kind === "routine" || info.kind === "sequence") openDdl(info);
  };

  // ───────── Database Explorer actions (DataGrip keymap) ─────────
  // Force Refresh's line shows the second Refresh key (⌥⇧⌘R) until Force Refresh gets a key of its own
  const forceKey = sk("tree.forceRefresh") ?? (bindings("tree.refresh")[1] ? formatSpec(bindings("tree.refresh")[1]) : undefined);
  // ───────── Backup / restore ─────────
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
  /** Copy Reference (menu and key, one node or several): schema.name, which a console on that database reads (the TUI's y too) */
  const reference = (info: NodeInfo) => [info.schema, info.name].filter(Boolean).join(".") || info.db || info.conn.name;
  const nodeName = (info: NodeInfo) => (info.kind === "conn" ? info.conn.name : info.name ?? info.schema ?? info.db ?? info.conn.name);
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
      addTab(info.conn, "ddl", { title: `SQL Generator: ${info.name}`, sql: ddl, database: info.db, objKind: info.kind, ddlOf: { schema: info.schema, name: info.name } });
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
        { label: "New Query Console", ...cmd("console.new"), action: () => openConsole(info.conn, info.db) },
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

  /** Commands that run for every selected node when several are: id → what runs, undefined when not every node can.
   *  Drop… needs objects of one database (one confirmation, one batch) */
  const multiActions = (all: NodeInfo[]): Record<string, (() => void) | undefined> => {
    const every = (ok: (i: NodeInfo) => boolean, run: () => void) => (all.every(ok) ? run : undefined);
    const oneDb = all.every((i) => i.conn.id === all[0].conn.id && nodeTarget(i) === nodeTarget(all[0]));
    return {
      "tree.editData": every((i) => isRelation(i.kind), () => all.forEach((i) => activateNode(i))),
      "tree.ddl": every((i) => !!i.name, () => all.forEach(openDdl)),
      "tree.copyName": () => void navigator.clipboard.writeText(all.map(nodeName).join("\n")),
      "tree.copyRef": () => void navigator.clipboard.writeText(all.map(reference).join("\n")),
      "tree.refresh": () => all.forEach((i) => i.refresh?.()),
      // Not a routine, nor a Redis key (DROP isn't a Redis command)
      "tree.remove": every((i) => oneDb && !!i.name && i.kind !== "routine" && i.kind !== "key", () => dangerous(all, "DROP")),
    };
  };

  /** DataGrip tree context menu (structure from docs 1.11). all = the selected nodes: when several, only the commands for several run */
  const buildMenu = (info: NodeInfo, all: NodeInfo[] = [info]): MenuItem[] => {
    const items = nodeMenu(info);
    return all.length > 1 ? forSelection(items, multiActions(all)) : items;
  };
  const nodeMenu = (info: NodeInfo): MenuItem[] => {
    const c = info.conn;
    const isObj = !!info.name;
    const rel = isRelation(info.kind);
    const pg = c.kind === "postgres";
    const objects: MenuItem[] = canDdl(c)
      ? [
          { separator: true },
          { label: "Table…", ...cmd("tree.newTable"), action: () => newObject(info, "table") },
          { label: "View…", ...cmd("tree.newView"), action: () => newObject(info, "view") },
          ...(pg ? ([{ label: "Materialized View…", ...cmd("tree.newMatview"), action: () => newObject(info, "matview") }, { label: "Sequence…", ...cmd("tree.newSequence"), action: () => newObject(info, "sequence") }] as MenuItem[]) : []),
          ...(pg && info.kind !== "schema" && !info.name ? ([{ label: "Schema…", ...cmd("tree.newSchema"), action: () => newObject(info, "schema") }] as MenuItem[]) : []),
          ...(info.kind === "conn" || info.kind === "db" ? ([{ label: "Database…", ...cmd("tree.newDatabase"), action: () => newObject(info, "database") }] as MenuItem[]) : []),
        ]
      : [];
    const items: MenuItem[] = [
      { label: "New", submenu: [{ label: "Query Console", ...cmd("console.new"), action: () => openConsole(c, info.db) }, ...objects, { separator: true }, { label: "Data Source…", ...cmd("tree.newDs"), action: () => setDialog({}) }] },
    ];
    if (canDdl(c) && (info.kind === "table" || info.kind === "partitioned")) items.push({ label: "Modify Table…", ...cmd("tree.modify"), action: () => modifyTable(info) });
    if (canDdl(c) && (["table", "partitioned", "view", "matview", "sequence"].includes(info.kind) || (pg && (info.kind === "schema" || info.kind === "db"))))
      items.push({ label: "Rename…", ...cmd("tree.rename"), action: () => renameNode(info) });
    if (rel) items.push({ label: "Edit Data", ...cmd("tree.editData"), action: () => activateNode(info) });
    if (info.kind === "conn") items.push({ label: "Properties", ...cmd("tree.properties"), action: () => setDialog({ edit: c }) });
    items.push({ separator: true });
    items.push({
      label: "Copy/Paste",
      submenu: [
        { label: "Copy Name", ...cmd("tree.copyName"), action: () => navigator.clipboard.writeText(nodeName(info)) },
        { label: "Copy Reference", ...cmd("tree.copyRef"), action: () => navigator.clipboard.writeText(reference(info)) },
        ...(info.kind === "conn" ? ([{ label: "Duplicate", ...cmd("tree.duplicate"), action: () => duplicateConn(c) }] as MenuItem[]) : []),
      ],
    });
    items.push({ label: "Refresh", ...cmd("tree.refresh"), action: () => info.refresh?.() });
    if (info.kind === "conn" && c.connected) items.push({ label: "Deactivate", ...cmd("tree.disconnect"), action: () => deactivate(c) });
    items.push({ label: "Quick Documentation", ...cmd("tree.quickDoc"), action: () => showQuickDoc(info) });
    items.push({ separator: true });
    items.push({
      label: "Navigation",
      submenu: [
        ...(isObj ? ([{ label: "Go to DDL", ...cmd("tree.ddl"), action: () => openDdl(info) }] as MenuItem[]) : []),
        { label: "Jump to Query Console…", ...cmd("tree.jumpConsole"), action: () => jumpToConsole(info) },
      ],
    });
    if (info.kind === "table" && c.kind !== "redis" && c.kind !== "mongodb" && !VECTOR_KINDS.includes(c.kind)) items.push({ label: "AI Search (RAG) — Index This Table…", ...cmd("tree.rag"), action: () => openRag(c, info.db, info.schema, info.name) });
    if (isObj)
      items.push({
        label: "SQL Scripts",
        submenu: [
          { label: "SQL Generator…", ...cmd("tree.sqlGen"), action: () => sqlGenerator(info) },
          { label: "Generate DDL to Clipboard", ...cmd("tree.ddlCopy"), action: () => ddlToClipboard(info) },
          { label: "Generate DDL to Query Console", ...cmd("tree.ddlConsole"), action: () => ddlToConsole(info) },
          ...(rel
            ? ([
                { separator: true },
                { label: "SELECT Statement", ...cmd("tree.genSelect"), action: () => generate(info, "select") },
                ...(info.kind === "table" || info.kind === "partitioned"
                  ? ([
                      { label: "INSERT Statement", ...cmd("tree.genInsert"), action: () => generate(info, "insert") },
                      { label: "UPDATE Statement", ...cmd("tree.genUpdate"), action: () => generate(info, "update") },
                      { label: "DELETE Statement", ...cmd("tree.genDelete"), action: () => generate(info, "delete") },
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
          ...(info.kind === "table" || info.kind === "partitioned" ? ([{ label: "Truncate…", ...cmd("tree.truncate"), danger: true, action: () => dangerous([info], "TRUNCATE") }] as MenuItem[]) : []),
          { label: "Drop…", ...cmd("tree.remove"), danger: true, action: () => dangerous([info], "DROP") },
        ],
      });
    if (rel)
      items.push({
        label: "Import/Export",
        submenu: [
          { label: "Export Data…", ...cmd("tree.export"), action: () => exportTable(info) },
          ...(info.kind === "table" || info.kind === "partitioned"
            ? ([
                { label: "Import Data from File…", ...cmd("tree.import"), action: () => setImportReq({ target: nodeTarget(info), schema: info.schema!, table: info.name }) },
                { label: "Generate Test Data…", ...cmd("tree.genTestData"), action: () => setGenReq({ target: nodeTarget(info), schema: info.schema ?? "", table: info.name!, conn: c }) },
                ...(c.kind === "postgres" || c.kind === "mysql" ? ([{ label: "Copy Related Rows To…", ...cmd("tree.copyRelated"), action: () => setCopyReq({ target: nodeTarget(info), schema: info.schema ?? "", table: info.name!, conn: c }) }] as MenuItem[]) : []),
              ] as MenuItem[])
            : []),
        ],
      });
    if (c.kind !== "redis" && (info.kind === "schema" || info.kind === "db" || (info.kind === "conn" && c.kind === "mysql")))
      items.push({ label: "Diagrams", submenu: [{ label: "Show Diagram (ERD)", ...cmd("tree.erd"), action: () => openErd(c, info.db, info.kind === "schema" && info.schema ? [info.schema] : c.kind === "mysql" ? [info.db ?? c.database] : undefined) }] });
    if (info.kind === "schema" && info.schema && c.kind !== "redis")
      items.push({ label: "Import/Export", submenu: [{ label: "Import Data into New Table…", ...cmd("tree.importNewTable"), action: () => setImportReq({ target: nodeTarget(info), schema: info.schema!, refresh: info.refresh }) }] });
    items.push({
      label: "Tools",
      submenu: [
        { label: "Manage Shown Schemas…", ...cmd("tree.manageSchemas"), action: () => document.querySelector<HTMLElement>(".ws-explorer.on .tree-node.conn.selected .scope-badge, .ws-explorer.on .tree-node.selected .scope-badge")?.click() },
        ...(c.kind !== "redis" && (info.kind === "conn" || info.kind === "db")
          ? ([
              { separator: true },
              { label: "Sessions & Locks", ...cmd("tree.dbaSessions"), action: () => openDba(c, info.db, "sessions", "Sessions") },
              { label: "Slow Queries", ...cmd("tree.dbaSlow"), action: () => openDba(c, info.db, "slow_queries", "Slow Queries (Top 100)") },
              { label: "Table Sizes & Indexes", ...cmd("tree.dbaSizes"), action: () => openDba(c, info.db, "table_sizes", "Table Sizes") },
              { label: "Server Overview", ...cmd("tree.dbaServer"), action: () => openDba(c, info.db, "server", "Server Overview") },
              { label: "Users & Privileges", ...cmd("tree.dbaRoles"), action: () => openDba(c, info.db, "roles", "Users & Roles") },
              { separator: true },
              { label: "Compare & Migrate (schemas, data, migration files)…", ...cmd("tree.migrate"), action: () => openMigrate(c, info.db) },
              { label: "AI Search (RAG)…", ...cmd("tree.ragSearch"), action: () => openRag(c, info.db) },
              { separator: true },
              { label: "Back Up Database…", ...cmd("tree.backup"), action: () => setBackupReq({ c, db: info.db }) },
              { label: "Restore Database…", ...cmd("tree.restore"), action: () => startRestore(c, info.db) },
            ] as MenuItem[])
          : []),
      ],
    });
    if (info.kind === "conn") {
      items.push({ label: "Diagnostics", submenu: [{ label: "Force Refresh", ...cmd("tree.forceRefresh"), shortcut: forceKey, action: () => (invalidateSearchIndex(c.id), invalidateCompletion(c.id), info.refresh?.()) }] });
      items.push({ separator: true });
      const others = workspaces.filter((w) => w.id !== curWs && !w.connIds.includes(c.id));
      if (others.length) items.push({ label: "Add to Workspace", submenu: others.map((w) => ({ label: w.name, action: () => setInWs(w.id, c.id, true) })) });
      items.push({ label: "Remove from This Workspace", ...cmd("tree.removeFromWs"), action: () => setInWs(curWs, c.id, false) });
      items.push({ label: "Remove Data Source", ...cmd("tree.remove"), danger: true, action: () => removeConn(c) });
    }
    return items;
  };
  return { openTable, openTableTab, openDdl, ragInit, activateNode, showQuickDoc, jumpToConsole, deactivate, duplicateConn, canDdl, modifyTable, renameNode, buildMenu, sqlGenerator, ddlToClipboard, ddlToConsole, dangerous, startBackup };
}
