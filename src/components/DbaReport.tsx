import { useEffect, useMemo, useRef, useState } from "react";
import { ask } from "../lib/dialogs";
import { Cell, dba, currentFirst, DbaReportInfo, Dialect, QueryResult } from "../lib/api";
import { ResultGrid } from "./ResultGrid";
import { sharedOr } from "../lib/shared";
import { Button, Notice, Select, Text, Toolbar } from "../ui";

const empty: QueryResult = { columns: [], rows: [], rowsAffected: 0, truncated: false, elapsedMs: 0, hasResultSet: true };
/** Reports that reload on their own (views of current state) */
const LIVE = new Set(["sessions", "locks", "vacuum_progress"]);

/** DBA report tab: sessions · locks, slow queries, size · indexes, maintenance, server, privileges. Row right-click = kill session / generate SQL */
export function DbaReport(p: {
  target: string;
  connName: string;
  report: string;
  readOnly: boolean;
  dialect: Dialect;
  visible: boolean;
  /** The database the report runs on ("" = MySQL without a default schema: all schemas) */
  database: string;
  /** The others it can run on: every database of the server, as the console lists them (loaded once the report is shown) */
  loadDbs: () => Promise<string[]>;
  /** The data source has one fixed database: shown, not changeable */
  fixedDb: boolean;
  onDatabase: (db: string) => void;
  onReport: (id: string, title: string) => void;
  onOpenConsole: (sql: string) => void;
  onStatus: (s: string) => void;
}) {
  const [list, setList] = useState<DbaReportInfo[]>([]);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** When the shown result was loaded (HH:MM:SS), for the stale banner after a failed refresh */
  const [loadedAt, setLoadedAt] = useState("");
  const [running, setRunning] = useState(false);
  const [auto, setAuto] = useState(true);
  const [dbs, setDbs] = useState<string[]>([]);
  /** Listing the databases failed: said above the report and in the picker's tooltip, with the way out */
  const [dbsError, setDbsError] = useState<string | null>(null);
  const dbsAsked = useRef(false);
  const loadDbs = () => {
    if (dbsAsked.current) return;
    dbsAsked.current = true;
    p.loadDbs().then(setDbs, (e) => {
      dbsAsked.current = false;
      setDbsError(sharedOr(String(e), (s) => s.dbListFailed(String(e))));
    });
  };
  const seq = useRef(0);
  /** A load is running (the auto-refresh tick waits for it) */
  const inFlight = useRef(false);
  // Restored tabs load when first shown (so they don't connect to every DB at startup)
  const [seen, setSeen] = useState(p.visible);
  useEffect(() => {
    if (p.visible) setSeen(true);
  }, [p.visible]);
  // The picker's list loads once the report is shown (and again for another target), so its first click opens the whole list;
  // focus / mousedown only ask again when that failed
  useEffect(() => {
    if (!seen) return;
    dbsAsked.current = false;
    setDbsError(null);
    loadDbs();
  }, [seen, p.target]);
  const info = list.find((r) => r.id === p.report);

  useEffect(() => {
    if (seen) dba.reports(p.target).then(setList, (e) => setError(String(e)));
  }, [p.target, seen]);

  const load = async () => {
    const n = ++seq.current;
    inFlight.current = true;
    setRunning(true);
    try {
      const r = await dba.run(p.target, p.report);
      if (n !== seq.current) return;
      setResult(r);
      setLoadedAt(new Date().toTimeString().slice(0, 8));
      setError(null);
    } catch (e) {
      // A failed refresh keeps the last result on screen (a busy server is when sessions need killing); the error shows above it
      if (n !== seq.current) return;
      setError(String(e));
    } finally {
      if (n === seq.current) {
        inFlight.current = false;
        setRunning(false);
      }
    }
  };
  useEffect(() => {
    if (!seen) return;
    // Another report: drop everything of the last one, so its error and load time don't show while this one loads
    setResult(null);
    setError(null);
    setLoadedAt("");
    load();
  }, [p.target, p.report, seen]);
  // Sessions · locks: every 3 seconds while visible. A tick is skipped while the last load still runs, so slow loads don't pile up on a busy server
  useEffect(() => {
    if (!auto || !p.visible || !LIVE.has(p.report)) return;
    const t = setInterval(() => inFlight.current || load(), 3000);
    return () => clearInterval(t);
  }, [auto, p.visible, p.report, p.target]);

  const groups = useMemo(() => {
    const m = new Map<string, DbaReportInfo[]>();
    for (const r of list) m.set(r.group, [...(m.get(r.group) ?? []), r]);
    return [...m.entries()];
  }, [list]);

  const str = (v: Cell) => (v === null ? "" : String(v));
  const rowActions = (info?.actions ?? []).map((a) => ({
    label: a.label,
    danger: a.kind === "kill",
    run: async (row: Record<string, Cell>) => {
      if (a.kind === "kill") {
        const pid = Number(row.pid);
        if (!pid) return p.onStatus("No pid in this row");
        if (p.readOnly) return p.onStatus("Read-only data source");
        if (!(await ask(`${a.label} ${pid}?`, { title: p.connName, detail: [{ text: str(row.query ?? row.blocking_query ?? "").slice(0, 300) || "(no query)", value: `pid ${pid}` }], okLabel: a.label, danger: true }))) return;
        try {
          const ok = await dba.kill(p.target, pid, a.terminate);
          p.onStatus(ok ? `${a.label}: ${pid}` : `${a.label}: ${pid} was not found (already gone?)`);
          load();
        } catch (e) {
          p.onStatus(String(e));
        }
      } else if (a.kind === "explain") {
        p.onOpenConsole(await dba.explain(p.target, str(row.query)));
      } else {
        const vals = Object.fromEntries(Object.entries(row).map(([k, v]) => [k, str(v)]));
        p.onOpenConsole(await dba.actionSql(p.target, a.template, vals));
      }
    },
  }));

  return (
    <div className="dba">
      <Toolbar className="dba-bar">
        <Select className="dba-report" value={p.report} onChange={(e) => p.onReport(e.target.value, list.find((r) => r.id === e.target.value)?.title ?? e.target.value)}>
          {groups.map(([g, rs]) => (
            <optgroup key={g} label={g}>
              {rs.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.title}
                </option>
              ))}
            </optgroup>
          ))}
        </Select>
        <Select className="dba-db" value={p.database} onFocus={loadDbs} onMouseDown={loadDbs} onChange={(e) => p.onDatabase(e.target.value)} disabled={p.fixedDb} title={p.fixedDb ? "This data source has one database" : dbsError ?? "Database the report runs on"}>
          {currentFirst(p.database, dbs).map((d) => (
            <option key={d} value={d} title={d}>{d || "(all schemas)"}</option>
          ))}
        </Select>
        <Button onClick={load} disabled={running}>
          {running ? "Loading…" : "Refresh"}
        </Button>
        {LIVE.has(p.report) && (
          <label className="dba-auto">
            <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} /> Auto-refresh (3s)
          </label>
        )}
        <Text tone="muted" className="dba-help">{info?.help}</Text>
      </Toolbar>
      {/* Not an option: a native select is as wide as its longest option */}
      {dbsError && <Notice tone="warn" className="dba-dbs-error">{dbsError}</Notice>}
      {info?.actions.length ? <div className="dba-hint muted">Right-click a row: {info.actions.map((a) => a.label).join(" · ")}</div> : null}
      {error && result && (
        <Notice tone="warn" className="dba-stale">
          Refresh failed, showing data from {loadedAt}: {error}
        </Notice>
      )}
      {error && !result ? (
        <Notice tone="danger">{error}</Notice>
      ) : (
        <div className="dba-grid">
          <ResultGrid
            result={result ?? empty}
            readOnly
            dialect={p.dialect}
            running={running && !result}
            loadingMore={false}
            query={null}
            onFetchMore={() => {}}
            onSubmit={async () => {}}
            onReload={load}
            onCancel={() => {}}
            onExport={() => {}}
            rowActions={rowActions}
          />
        </div>
      )}
    </div>
  );
}
