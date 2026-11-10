import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { analyzePlan, api, auditLog, ConnectionView, dialectOf, DbKind, meta, previewImpact, QueryResult, targetId } from "../../lib/api";
import { ask, notice } from "../../lib/dialogs";
import { pushHistory } from "../../components/HistoryDialog";
import type { OutputLine, ResultEntry } from "../../components/ConsolePane";
import type { MarkStmt } from "../../lib/stmtMark";
import { schemaChanged } from "../../components/SchemaTree";
import { commitNoticeTitle } from "../../lib/commitNotice";
import { sharedOr } from "../../lib/shared";
import { setTableColumns } from "../../lib/statusbar";
import { cancelTableLoad } from "../explorer/tableTab";
import { explainAnalyzeAsk, explainAnalyzeNote, isDdl, isRead } from "../../lib/stmtKind";
import { patch, updTab } from "../tabs/ops";
import { editorApis, setTabs, useTabsStore } from "../tabs/store";
import type { Tab } from "../tabs/types";

/** Max rows kept in the result grid (same as the TUI). Beyond that, export to CSV */
const MAX_ROWS = 100_000;
const capMsg = (n: number) => `Showing the first ${n.toLocaleString()} rows — export to CSV for all of them`;
/** Explain Analyse: ask before the statements qurry_shared::guard::explain_analyze_ask names (the TUI's words). True = go on */
async function askExplainAnalyze(name: string | undefined, kind: DbKind, stmts: string[]): Promise<boolean> {
  const asks = stmts.flatMap((s) => {
    const why = explainAnalyzeAsk(kind, s);
    return why ? [{ s, why }] : [];
  });
  if (!asks.length) return true;
  // Several: each statement with its own reason (as the TUI lists them)
  const one = asks.length === 1;
  const detail = asks.map(({ s, why }) => ({ text: s.replace(/\s+/g, " ").slice(0, 300), note: one ? undefined : why }));
  const text = one ? `Explain Analyse really runs the statement, and ${asks[0].why}. Run it anyway?` : `Explain Analyse really runs the statements, and ${asks.length} of them need a look. Run them anyway?`;
  return ask(text, { title: name ? `${name}: Explain Analyse` : "Explain Analyse", detail, okLabel: "Run Anyway", danger: true });
}

/** Query consoles and table tabs: running, Explain, transactions, loading more rows, the database / schema switch */
export function useConsole({ conns, setStatus, refresh }: { conns: ConnectionView[]; setStatus: (s: string) => void; refresh: () => Promise<void> }) {
  /** When a restored tab is first shown: tables load their data, consoles set their transaction mode (Redis views load themselves) */
  const loadStale = async (t: Tab) => {
    patch(t.id, { stale: false });
    if (t.kind === "console" && t.txManual) api.setTxMode(t.target, t.id, true).catch((e) => setStatus(String(e)));
    if (t.kind === "table") loadTable(t);
  };
  /** Load a table tab's PK and data together (if the connection fails, waiting for both would take twice as long) */
  const loadTable = (t: Tab) => {
    const { edit, tableSql } = t;
    if (!edit || !tableSql) return;
    meta
      .table(t.target, edit.schema, edit.table)
      .then((d) => {
        // The status bar's Size reads the columns' declared types from here, fresh on each load
        setTableColumns(t.target, edit.schema, edit.table, d.columns);
        updTab(t.id, (x) => (x.edit ? { ...x, edit: { ...x.edit, pk: d.columns.filter((c) => c.isPk).map((c) => c.name) } } : x));
      })
      .catch(() => {
        /* Read-only if the metadata lookup fails */
      });
    run(t, tableSql);
  };
  // Data Source just connected: reload table tabs left empty by a failed connection
  const wasConnected = useRef(new Set<string>());
  useEffect(() => {
    const now = new Set(conns.filter((c) => c.connected).map((c) => c.id));
    const back = [...now].filter((id) => !wasConnected.current.has(id));
    wasConnected.current = now;
    if (!back.length) return;
    for (const t of useTabsStore.getState().tabs) {
      if (t.kind === "table" && back.includes(t.connId) && t.error && !t.running && !t.stale) loadTable(t);
    }
  }, [conns]);

  // ───────── Console DB / schema selection (top right of the DataGrip console) ─────────
  const [dbCache, setDbCache] = useState<Record<string, string[]>>({});
  const [schemaCache, setSchemaCache] = useState<Record<string, string[]>>({});
  const loadDbList = (connId: string) => {
    if (dbCache[connId]) return;
    meta.databases(connId).then((d) => setDbCache((c) => ({ ...c, [connId]: d })), () => {});
  };
  const loadSchemaList = (target: string) => {
    if (schemaCache[target]) return;
    meta.schemas(target).then((d) => setSchemaCache((c) => ({ ...c, [target]: d.filter((x) => !x.isSystem).map((x) => x.name) })), () => {});
  };
  const switchDatabase = async (tab: Tab, db: string) => {
    const c = conns.find((x) => x.id === tab.connId);
    if (!c) return;
    if (tab.inTx && !(await ask("This console has uncommitted changes. Switching the database rolls them back. Continue?", { okLabel: "Switch and Roll Back", danger: true }))) return;
    const database = db !== (c.database || "postgres") ? db : undefined;
    api.closeSession(tab.id).catch(() => {}); // The next run opens a session on the new DB
    patch(tab.id, { database, target: targetId(c.id, database), schema: undefined, inTx: false });
    setStatus(`Console switched to database ${db}`);
  };
  const switchSchema = async (tab: Tab, schema: string) => {
    try {
      const c = conns.find((x) => x.id === tab.connId);
      if (c?.kind === "mysql") {
        // In MySQL, schema = database
        await api.runQuery(tab.target, `USE ${dialectOf("mysql").q(schema)}`, tab.id);
        patch(tab.id, { schema });
        setStatus(`USE ${schema}`);
        return;
      }
      await api.runQuery(tab.target, `SET search_path TO ${dialectOf("postgres").q(schema)}, public`, tab.id);
      patch(tab.id, { schema });
      setStatus(`search_path = ${schema}, public`);
    } catch (e) {
      setStatus(String(e));
    }
  };

  // ───────── Running ─────────
  const run = async (tab: Tab, sql: string) => {
    if (!sql.trim()) return;
    patch(tab.id, { running: true, error: null, lastSql: sql });
    setStatus("Executing…");
    try {
      const r = await api.runQuery(tab.target, sql, tab.id);
      patch(tab.id, { running: false, result: r });
      setStatus(r.hasResultSet ? `${r.rows.length}${r.truncated ? "+" : ""} rows retrieved in ${r.elapsedMs} ms` : `${r.rowsAffected} rows affected in ${r.elapsedMs} ms`);
      refresh();
    } catch (e) {
      patch(tab.id, { running: false, error: String(e), result: null });
      setStatus("Query failed");
    }
  };

  // ───────── Console run (multiple statements → multiple result tabs, Output log) ─────────
  const nowTs = () => {
    const d = new Date();
    const p2 = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())} ${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}`;
  };
  const log = (id: string, level: OutputLine["level"], text: string, stmt?: string) => updTab(id, (t) => ({ ...t, output: [...t.output.slice(-999), { ts: nowTs(), text, level, stmt }] }));
  /**
   * After "Allow unencrypted" under a console's or table tab's connect error: only reconnect. A console never runs the statement again (it may write);
   * a table tab reloads once connected, as after any failed connection
   */
  const reconnectAfterTls = async (connId: string, consoleTab?: string) => {
    setStatus(`Connecting to ${conns.find((c) => c.id === connId)?.name ?? connId}…`);
    try {
      await api.connect(connId);
      if (consoleTab) log(consoleTab, "info", "Connected. Run again.");
      setStatus(consoleTab ? "Connected. Run again." : "Connected");
    } catch (e) {
      if (consoleTab) log(consoleTab, "error", String(e));
      setStatus(`Connection failed: ${String(e)}`);
    }
    refresh();
  };
  /** The table tab's grid takes the keys (the error and its button are about to go away) */
  const focusTableGrid = (tabId: string) => document.querySelector<HTMLElement>(`.work-layer[data-tab="${tabId}"] .grid-scroll`)?.focus();
  /** Result tab title: leading comment of the query → table name → Result N (DataGrip) */
  const resultTitle = (sql: string, n: number) => {
    const c = /^\s*(?:--\s*(.+)|\/\*\s*([\s\S]*?)\*\/)/.exec(sql);
    if (c) return (c[1] ?? c[2]).trim().split("\n")[0].slice(0, 40);
    const f = /\bfrom\s+((?:"[^"]+"|\w+)(?:\.(?:"[^"]+"|\w+))?)/i.exec(sql);
    return f ? f[1].replace(/"/g, "") : `Result ${n}`;
  };
  /** `mark`: the gutter marks of the statements (running, then done or failed; none for those after a failure) */
  const executeConsole = async (tabId: string, stmts: string[], explain?: "plan" | "analyze", mark?: MarkStmt) => {
    const tab = useTabsStore.getState().tabs.find((t) => t.id === tabId);
    if (!tab || !stmts.length) return;
    const c = conns.find((x) => x.id === tab.connId);
    const kind = c?.kind ?? "postgres";
    // Confirm dangerous statements (UPDATE/DELETE without WHERE, DROP, TRUNCATE — same rules as the core guard). Not for Explain:
    // Explain Analyse asks its own question below, with this reason when there is one (one dialog, as in the TUI)
    if (!explain) {
      for (const s of stmts) {
        const why = await api.unsafeReason(kind, s).catch(() => null);
        if (why && !(await ask(`${why}. Run it anyway?`, { title: c ? `${c.name}: dangerous statement` : "Dangerous statement", detail: [{ text: s.slice(0, 300), tone: "danger" }], okLabel: "Run Anyway", danger: true }))) return;
      }
    }
    // Explain Analyse really runs the statement: PostgreSQL · MySQL roll it back (below), other kinds don't. Ask first when the rollback
    // may not undo it all or nothing does (qurry_shared::guard::explain_analyze_ask, the TUI's rule and words)
    const wraps = kind === "postgres" || kind === "mysql";
    if (explain === "analyze" && !(await askExplainAnalyze(c?.name, kind, stmts))) return;
    // Production: for data-changing statements, count the rows that will change, show it and ask (counted with SELECT count(*))
    if (c?.options?.confirmChanges && !explain) {
      // Same check as the TUI (core guard): a write keyword only inside a string or a comment (MySQL # too) doesn't make a read a write
      const writes = stmts.filter((s) => !isRead(kind, s));
      if (writes.length) {
        setStatus("Counting the rows this will change…");
        const detail: { text: string; value: string; tone?: "danger" | "warn" }[] = [];
        for (const w of writes) {
          const n = await previewImpact(tab.target, w).catch((e) => `can't count: ${String(e).split("\n")[0]}`);
          // isDdl is null while the WASM module isn't loaded
          const ddl = n === null ? isDdl(kind, w) : false;
          const value = typeof n === "number" ? `${n.toLocaleString("en-US")} row${n === 1 ? "" : "s"}` : n === null ? (ddl ? "schema change" : ddl === null ? "can't count · may change the schema" : "can't count") : n;
          // More rows changed = more prominent: 0 rows normal, 1~99 warning, 100+ · uncountable · DDL danger
          const tone = typeof n === "number" ? (n >= 100 ? "danger" : n > 0 ? "warn" : undefined) : "danger";
          detail.push({ text: w.replace(/\s+/g, " ").slice(0, 300), value, tone });
        }
        setStatus("Ready");
        if (!(await ask("These statements change data. Run them?", { title: `Production: ${c.name}`, detail, okLabel: "Run", danger: true }))) return;
      }
    }
    // Replace unpinned result tabs with the new results
    // If the result tab being viewed goes away, switch to Output (so the running SQL shows instead of an empty pane)
    updTab(tabId, (t) => {
      const results = t.results.filter((r) => r.pinned);
      return { ...t, running: true, results, cursorEntry: undefined, activeResult: results.some((r) => r.id === t.activeResult) ? t.activeResult : "output" };
    });
    setStatus("Executing…");
    let n = tab.results.filter((r) => r.pinned).length;
    let lastActive = "output";
    // Number of changing statements already committed in Auto mode (reported if a later one fails)
    let committed = 0;
    // Statements that ran but whose result the Production log couldn't record (a warning, not a failure: running them again would repeat them)
    let warned = 0;
    for (const [i, sql0] of stmts.entries()) {
      const isMy = c?.kind === "mysql";
      const bare = sql0.replace(/;\s*$/, "");
      const sql = !explain ? sql0 : isMy ? `EXPLAIN ${explain === "analyze" ? "ANALYZE " : ""}${bare}` : `EXPLAIN (FORMAT JSON${explain === "analyze" ? ", ANALYZE, BUFFERS" : ""}) ${bare}`;
      const out = crypto.randomUUID();
      const ts = nowTs();
      log(tabId, "sql", `${c?.name ?? ""}> ${sql0}`, out);
      mark?.(i, { state: "running", out, ts });
      const ok = (text: string) => (log(tabId, "info", text), mark?.(i, { state: "ok", out, ts, text }));
      // Explain Analyse actually runs the statement → run it in a transaction and always roll back (a savepoint when one may be
      // open, so nothing pending is committed: core's explain_wrap, the TUI's rule). Reads too: a function a SELECT calls may write
      const dry = explain === "analyze" && wraps;
      const inTx0 = !!useTabsStore.getState().tabs.find((t) => t.id === tabId)?.inTx;
      try {
        let r: QueryResult;
        if (dry) {
          const [begin, back] = await api.explainWrap(tab.target, tabId);
          await api.runQuery(tab.target, begin, tabId);
          let after: QueryResult | undefined;
          try {
            r = await api.runQuery(tab.target, sql, tabId);
          } finally {
            after = await api.runQuery(tab.target, back, tabId).catch((e) => (log(tabId, "error", `Roll back after Explain Analyse failed: ${String(e)}`), undefined));
          }
          r = { ...r, inTransaction: after?.inTransaction ?? inTx0 };
          const note = explainAnalyzeNote(kind, sql0);
          if (note) log(tabId, note.level, `Explain Analyse ${note.text}`);
        } else {
          r = await api.runQuery(tab.target, sql, tabId);
        }
        pushHistory({ connId: tab.connId, database: tab.database, sql: sql0 });
        updTab(tabId, (t) => ({ ...t, inTx: !!r.inTransaction, txIdle: undefined }));
        if (!r.inTransaction && !isRead(kind, sql0) && !dry) committed++;
        if (r.warning) {
          warned++;
          log(tabId, "warn", `Warning: ${r.warning}`);
        }
        if (explain && !isMy) {
          const raw = String(r.rows[0]?.[0] ?? "");
          const plan = await analyzePlan(raw).catch(() => null);
          const pretty = (() => {
            try {
              return JSON.stringify(JSON.parse(raw), null, 2);
            } catch {
              return raw;
            }
          })();
          const entry: ResultEntry = { id: crypto.randomUUID(), title: resultTitle(sql0, ++n), sql: sql0, kind: "plan", result: null, plan: plan ?? undefined, planJson: pretty, pinned: false };
          updTab(tabId, (t) => ({ ...t, results: [...t.results, entry] }));
          lastActive = entry.id;
          ok(`Plan retrieved in ${r.elapsedMs} ms`);
        } else if (r.hasResultSet) {
          const entry: ResultEntry = { id: crypto.randomUUID(), title: resultTitle(sql0, ++n), sql: sql0, kind: "result", result: r, pinned: false };
          updTab(tabId, (t) => ({ ...t, results: [...t.results, entry], cursorEntry: r.truncated ? entry.id : t.cursorEntry }));
          lastActive = entry.id;
          ok(`${r.rows.length}${r.truncated ? "+" : ""} row${r.rows.length === 1 ? "" : "s"} retrieved starting from 1 in ${r.elapsedMs} ms`);
        } else {
          ok(`${r.rowsAffected} row${r.rowsAffected === 1 ? "" : "s"} affected in ${r.elapsedMs} ms`);
          // Production: notify if an undo SQL was recorded (production operation log)
          if (c?.options?.confirmChanges && r.rowsAffected > 0 && !dry) {
            const last = (await auditLog(tab.connId, "", 1).catch(() => []))[0];
            if (last?.undo) log(tabId, "info", "Undo saved — ⇧⌘A › Production Log…");
          }
        }
      } catch (e) {
        log(tabId, "error", String(e));
        // The server's spot, in the statement's own text (Explain sent it after its EXPLAIN …)
        const pre = explain ? sql.length - bare.length : 0;
        const pos = sharedOr(undefined, (s) => s.errorPosition(String(e), sql));
        mark?.(i, { state: "error", out, ts, text: sharedOr(String(e), (s) => s.withoutPosition(String(e))), at: pos !== undefined && pos >= pre ? pos - pre : undefined });
        if (committed > 0) log(tabId, "error", `Note: ${committed} earlier statement${committed > 1 ? "s" : ""} of this run ${committed > 1 ? "were" : "was"} already committed (Auto mode) and ${committed > 1 ? "were" : "was"} not rolled back`);
        // Even on error a transaction may be open (e.g. first statement fails in Manual) → sync the Commit/Roll back buttons
        syncTx(tabId);
        // DDL among the earlier statements may already be applied
        if (!explain && stmts.some((s) => isDdl(kind, s) !== false)) schemaChanged(tab.connId, tab.database ?? (c?.database || (c?.kind === "postgres" ? "postgres" : "")));
        lastActive = "output";
        updTab(tabId, (t) => ({ ...t, running: false, activeResult: "output" }));
        setStatus(mark ? "Query failed: the error is marked in the editor and kept in Output — fix the statement and run it again" : "Query failed (see Output)");
        refresh();
        return;
      }
    }
    updTab(tabId, (t) => ({ ...t, running: false, activeResult: lastActive, lastSql: stmts[stmts.length - 1] }));
    setStatus(`${stmts.length} statement${stmts.length > 1 ? "s" : ""} executed${warned ? ` · ${warned} with a warning (see Output)` : ""}`);
    // If tables or columns were created, altered, or dropped, show it in the explorer right away
    if (!explain && stmts.some((s) => isDdl(kind, s) !== false)) schemaChanged(tab.connId, tab.database ?? (c?.database || (c?.kind === "postgres" ? "postgres" : "")));
    refresh();
  };

  /** The grid's line for a failed load-more (the TUI's words: qurry_shared::errors::more_rows_failed) */
  const moreRowsFailed = (e: unknown) => sharedOr(String(e), (s) => s.moreRowsFailed(String(e)));
  const fetchMoreEntry = async (tabId: string, entryId: string) => {
    const tab = useTabsStore.getState().tabs.find((t) => t.id === tabId);
    // A run in the tab is replacing the results: a load-more now would be answered for the old one (the new grid asks for its own)
    if (tab?.running) return;
    if (!tab || tab.cursorEntry !== entryId) {
      // The server cursor is open only on the last result
      updTab(tabId, (t) => ({ ...t, results: t.results.map((r) => (r.id === entryId && r.result ? { ...r, result: { ...r.result, truncated: false } } : r)) }));
      return;
    }
    const shown = tab.results.find((x) => x.id === entryId)?.result?.rows.length ?? 0;
    if (shown >= MAX_ROWS) return setStatus(capMsg(shown));
    updTab(tabId, (t) => ({ ...t, loadingMore: true }));
    // A run in the tab meanwhile has a result of its own (its cursorEntry): this answer is for the old one, so it is dropped
    const answer = (f: (t: Tab) => Tab) => updTab(tabId, (t) => (t.cursorEntry === entryId ? f({ ...t, loadingMore: false }) : { ...t, loadingMore: false }));
    try {
      const r = await api.fetchMore(tabId);
      answer((t) => ({
        ...t,
        cursorEntry: r.truncated ? entryId : undefined,
        results: t.results.map((x) => (x.id === entryId && x.result ? { ...x, result: { ...x.result, rows: [...x.result.rows, ...r.rows], truncated: r.truncated } } : x)),
      }));
    } catch (e) {
      // The result isn't complete: keep its "+" and say in the grid why no more rows came
      const moreError = moreRowsFailed(e);
      const current = useTabsStore.getState().tabs.find((t) => t.id === tabId)?.cursorEntry === entryId;
      answer((t) => ({ ...t, cursorEntry: undefined, results: t.results.map((x) => (x.id === entryId && x.result ? { ...x, result: { ...x.result, moreError } } : x)) }));
      if (current) setStatus(moreError);
    }
  };

  /** Read the console's transaction state into the tab (after an error the Commit / Roll back buttons may be stale) */
  const syncTx = (tabId: string) => api.txState(tabId).then((inTx) => updTab(tabId, (t) => ({ ...t, inTx, txIdle: inTx ? t.txIdle : undefined })), () => {});
  // A Production console's transaction left open (core tx_reminders, on the housekeeping beat): in its Output, on its pending badge
  // and in the status bar
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<{ tabId: string; message: string }>("tx-idle", (e) => {
      const { tabId, message } = e.payload;
      log(tabId, "warn", message);
      updTab(tabId, (t) => ({ ...t, txIdle: message }));
      setStatus(message);
    })
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  /** Commit / Roll back failed: in Output (the status bar cuts it off) and the status bar; a failed COMMIT also in a notice, since it
   * may say the changes were not saved, or that it is unknown whether they were and the data needs a check */
  const txFailed = (tab: Tab, e: unknown, commit: boolean) => {
    log(tab.id, "error", String(e));
    setStatus(String(e));
    patch(tab.id, { activeResult: "output" });
    syncTx(tab.id);
    if (commit) void notice(String(e), commitNoticeTitle(String(e)));
  };
  const setTxMode = async (tab: Tab, manual: boolean) => {
    let commit = false;
    try {
      // Ask first if there are uncommitted changes (it used to commit silently)
      if (!manual && tab.inTx) {
        if (!(await ask("This console has uncommitted changes.", { title: "Switch to Auto", okLabel: "Commit and Switch", cancelLabel: "Stay in Manual" }))) return;
        commit = true;
        const warning = await api.commitTx(tab.id);
        commit = false;
        log(tab.id, "info", "Transaction committed");
        if (warning) log(tab.id, "warn", `Warning: ${warning}`);
      }
      await api.setTxMode(tab.target, tab.id, manual);
      updTab(tab.id, (t) => ({ ...t, txManual: manual, inTx: manual ? t.inTx : false, txIdle: manual ? t.txIdle : undefined }));
      log(tab.id, "info", `Transaction mode: ${manual ? "Manual" : "Auto"}`);
    } catch (e) {
      txFailed(tab, e, commit);
    }
  };
  const txEnd = async (tab: Tab, what: "commit" | "rollback") => {
    try {
      const warning = what === "commit" ? await api.commitTx(tab.id) : await api.rollbackTx(tab.id);
      updTab(tab.id, (t) => ({ ...t, inTx: false, txIdle: undefined }));
      log(tab.id, "info", what === "commit" ? "Transaction committed" : "Transaction rolled back");
      if (warning) {
        log(tab.id, "warn", `Warning: ${warning}`);
        setStatus(`Warning: ${warning}`);
      }
    } catch (e) {
      txFailed(tab, e, what === "commit");
    }
  };

  const applyTableFilter = (tab: Tab, where: string, orderBy: string) => {
    if (!tab.edit) return;
    const { q } = dialectOf(conns.find((x) => x.id === tab.connId)?.kind);
    let sql = `SELECT * FROM ${q(tab.edit.schema)}.${q(tab.edit.table)}`;
    if (where) sql += ` WHERE ${where}`;
    if (orderBy) sql += ` ORDER BY ${orderBy}`;
    sql += ";";
    patch(tab.id, { where, orderBy, tableSql: sql });
    // Also reload the PK for tabs that didn't get one (tabs opened during a failed connection)
    if (tab.edit.pk.length) run({ ...tab, tableSql: sql }, sql);
    else loadTable({ ...tab, tableSql: sql });
  };

  const fetchMore = async (tab: Tab) => {
    if (useTabsStore.getState().tabs.find((t) => t.id === tab.id)?.running) return;
    const shown = tab.result?.rows.length ?? 0;
    if (shown >= MAX_ROWS) return setStatus(capMsg(shown));
    // The result asked for: a run meanwhile (reload, filter) replaced it, and the answer for the old one is dropped
    const sent = useTabsStore.getState().tabs.find((t) => t.id === tab.id)?.result;
    const answer = (f: (r: QueryResult) => QueryResult, status: string) => {
      const current = useTabsStore.getState().tabs.find((t) => t.id === tab.id)?.result === sent;
      setTabs((ts) => ts.map((t) => (t.id !== tab.id ? t : t.result && t.result === sent ? { ...t, loadingMore: false, result: f(t.result) } : { ...t, loadingMore: false })));
      if (current) setStatus(status);
    };
    patch(tab.id, { loadingMore: true });
    try {
      const r = await api.fetchMore(tab.id);
      answer((x) => ({ ...x, rows: [...x.rows, ...r.rows], truncated: r.truncated }), `${(sent?.rows.length ?? 0) + r.rows.length}${r.truncated ? "+" : ""} rows retrieved`);
    } catch (e) {
      const moreError = moreRowsFailed(e);
      answer((x) => ({ ...x, moreError }), moreError);
    }
  };

  const cancel = async (tab: Tab) => {
    // A table tab still waiting for its metadata: no statement runs on its session yet
    if (cancelTableLoad(tab.id)) return;
    try {
      await api.cancelQuery(tab.id);
      setStatus("Cancel requested");
    } catch (e) {
      setStatus(String(e));
    }
  };

  // Renders the status bar again when a ⌘↩ starts or stops waiting for the module (the wait lives in ConsolePane)
  const [, setPendingRuns] = useState(0);
  /** ⌘F2, the toolbars' ■ and the status bar's ✕: cancel the running statement or load-more (it reads on the tab's session, so the same cancel
   * stops it), or the ⌘↩ still waiting for the WebAssembly module */
  const canCancel = (t?: Tab | null) => !!t && (t.running || t.loadingMore || !!editorApis.get(t.id)?.loading());
  /** What the status bar says while a console or table tab is busy (its RunningBar), or undefined */
  const busyLabel = (t: Tab) =>
    t.kind !== "console" && t.kind !== "table"
      ? undefined
      : editorApis.get(t.id)?.loading()
        ? "Loading the statement splitter"
        : t.running
          ? t.kind === "console" ? "Executing" : `Loading ${t.title}`
          : t.loadingMore
            ? "Loading more rows"
            : undefined;
  const cancelRun = (t: Tab) => {
    const ed = editorApis.get(t.id);
    if (ed?.loading()) ed.cancelPending();
    else cancel(t);
  };

  /** Save a grid's edits, for a table tab and a console result alike. then = show the saved rows (run the grid's statement again).
   *  A console in Manual mode or with a transaction open runs them in it: they stay pending until Commit / Roll back (#106) */
  const submitEdits = async (tab: Tab, statements: string[], then = () => void (tab.lastSql && run(tab, tab.lastSql))) => {
    const console_ = tab.kind === "console";
    const inTx = console_ && (tab.txManual || tab.inTx);
    // Production asks first, listing the statements (each changes exactly one row)
    const c = conns.find((x) => x.id === tab.connId);
    if (c?.options?.confirmChanges) {
      const detail = statements.map((s) => ({ text: s.replace(/\s+/g, " ").slice(0, 300), value: /^\s*insert/i.test(s) ? "new row" : "1 row", tone: "warn" as const }));
      const n = statements.length;
      const what = `${n} change${n === 1 ? "" : "s"}`;
      const text = inTx ? `Run ${what} in this console's transaction? They stay pending until you Commit or Roll back.` : `Save ${what} to this data source?`;
      if (!(await ask(text, { title: `Production: ${c.name}`, detail, okLabel: inTx ? "Run" : "Save", danger: true }))) return;
    }
    // None is saved unless each UPDATE/DELETE affects exactly 1 row
    const r = await api.submitEdits(tab.target, console_ ? tab.id : undefined, statements).catch((e) => {
      // A failure may have ended the transaction (a deadlock, a lost connection): the Commit / Roll back buttons follow
      if (console_) syncTx(tab.id);
      throw e;
    });
    if (inTx) updTab(tab.id, (t) => ({ ...t, inTx: true, txIdle: undefined }));
    // A warning means it was saved (only the Production log couldn't record it): reload as on success, so it isn't submitted twice
    const pending = inTx ? " · pending until Commit / Roll back" : "";
    setStatus(`Submitted ${statements.length} statements, ${r.rows} rows affected${pending}${r.warning ? ` · Warning: ${r.warning}` : ""}`);
    if (inTx) log(tab.id, "info", `Submitted ${statements.length} grid edit${statements.length === 1 ? "" : "s"} in the transaction (${r.rows} row${r.rows === 1 ? "" : "s"}): pending until Commit / Roll back`);
    then();
    // The reload replaces the status line right away, and a table tab has no Output: tell them in a dialog (as the TUI does)
    if (r.warning) void notice(r.warning, "Submitted, with a warning");
  };
  return { dbCache, schemaCache, loadDbList, loadSchemaList, switchDatabase, switchSchema, run, log, reconnectAfterTls, focusTableGrid, executeConsole, fetchMoreEntry, setTxMode, txEnd, applyTableFilter, fetchMore, cancel, setPendingRuns, canCancel, busyLabel, cancelRun, submitEdits, loadStale };
}
