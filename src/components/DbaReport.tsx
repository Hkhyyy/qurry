import { useEffect, useMemo, useRef, useState } from "react";
import { ask } from "../lib/dialogs";
import { Cell, dba, DbaReportInfo, Dialect, QueryResult } from "../lib/api";
import { ResultGrid } from "./ResultGrid";
import { Button, Notice, Select } from "../ui";

const empty: QueryResult = { columns: [], rows: [], rowsAffected: 0, truncated: false, elapsedMs: 0, hasResultSet: true };
/** 저절로 다시 불러오는 리포트 (지금 상태를 보는 것) */
const LIVE = new Set(["sessions", "locks"]);

/** DBA 리포트 탭: 세션·락, 느린 쿼리, 용량·인덱스, 유지보수, 서버, 권한. 행 우클릭 = 세션 종료 / SQL 만들기 */
export function DbaReport(p: {
  target: string;
  connName: string;
  report: string;
  readOnly: boolean;
  dialect: Dialect;
  visible: boolean;
  onReport: (id: string, title: string) => void;
  onOpenConsole: (sql: string) => void;
  onStatus: (s: string) => void;
}) {
  const [list, setList] = useState<DbaReportInfo[]>([]);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [auto, setAuto] = useState(true);
  const seq = useRef(0);
  // 되살린 탭은 처음 보일 때 불러온다 (켜자마자 모든 DB 에 붙지 않게)
  const [seen, setSeen] = useState(p.visible);
  useEffect(() => {
    if (p.visible) setSeen(true);
  }, [p.visible]);
  const info = list.find((r) => r.id === p.report);

  useEffect(() => {
    if (seen) dba.reports(p.target).then(setList, (e) => setError(String(e)));
  }, [p.target, seen]);

  const load = async () => {
    const n = ++seq.current;
    setRunning(true);
    try {
      const r = await dba.run(p.target, p.report);
      if (n !== seq.current) return;
      setResult(r);
      setError(null);
    } catch (e) {
      if (n !== seq.current) return;
      setError(String(e));
      setResult(null);
    } finally {
      if (n === seq.current) setRunning(false);
    }
  };
  useEffect(() => {
    if (!seen) return;
    setResult(null);
    load();
  }, [p.target, p.report, seen]);
  // 세션·락: 보이는 동안 3초마다
  useEffect(() => {
    if (!auto || !p.visible || !LIVE.has(p.report)) return;
    const t = setInterval(load, 3000);
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
      } else {
        const vals = Object.fromEntries(Object.entries(row).map(([k, v]) => [k, str(v)]));
        p.onOpenConsole(await dba.actionSql(p.target, a.template, vals));
      }
    },
  }));

  return (
    <div className="dba">
      <div className="dba-bar">
        <Select value={p.report} onChange={(e) => p.onReport(e.target.value, list.find((r) => r.id === e.target.value)?.title ?? e.target.value)}>
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
        <Button onClick={load} disabled={running}>
          {running ? "Loading…" : "Refresh"}
        </Button>
        {LIVE.has(p.report) && (
          <label className="dba-auto">
            <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} /> Auto-refresh (3s)
          </label>
        )}
        <span className="muted dba-help">{info?.help}</span>
      </div>
      {info?.actions.length ? <div className="dba-hint muted">Right-click a row: {info.actions.map((a) => a.label).join(" · ")}</div> : null}
      {error ? (
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
