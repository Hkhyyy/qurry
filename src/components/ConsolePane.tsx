import { useState } from "react";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { QueryResult, Dialect } from "../lib/api";
import { splitScript } from "../lib/sqlSplit";
import { DEFAULT_EXEC, ExecSettings, planExecution } from "../lib/execPlan";
import { ConsoleEditor, EditorApi, RunRequest } from "./ConsoleEditor";
import { ContextMenu, MenuItem } from "./Explorer";
import { ResultGrid } from "./ResultGrid";
import { IcClock, IcGear, IcReload, IcStop } from "./icons";

const isMac = navigator.platform.toLowerCase().includes("mac");

const SETTINGS_KEY = "dbtool.execSettings";
export function loadExecSettings(): ExecSettings {
  try {
    return { ...DEFAULT_EXEC, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") };
  } catch {
    return DEFAULT_EXEC;
  }
}

export interface PlanNode {
  op: string;
  detail: string;
  startup?: number;
  total?: number;
  rows?: number;
  width?: number;
  actualTime?: number;
  actualRows?: number;
  loops?: number;
  /** 자식 몫을 뺀 이 노드만의 시간(ms) / 비용 */
  selfTime?: number;
  selfCost?: number;
  /** 전체에서 이 노드 몫 (%): ANALYZE 면 시간, 아니면 비용 */
  pct: number;
  warnings: string[];
  props: [string, string][];
  children: PlanNode[];
}

export interface ResultEntry {
  id: string;
  title: string;
  sql: string;
  kind: "result" | "plan";
  result: QueryResult | null;
  plan?: PlanNode;
  pinned: boolean;
}

export interface OutputLine {
  ts: string;
  text: string;
  level: "sql" | "info" | "error";
}

interface Props {
  tabId: string;
  target: string;
  initialSql: string;
  connName: string;
  dialect?: Dialect;
  database: string;
  schema: string;
  dbList: string[];
  schemaList: string[];
  running: boolean;
  loadingMore: boolean;
  results: ResultEntry[];
  activeResult: string;
  output: OutputLine[];
  txManual: boolean;
  inTx: boolean;
  onChange: (sql: string) => void;
  /** 실제 실행: 문장 목록 */
  onExecute: (statements: string[], explain?: "plan" | "analyze") => void;
  onCancel: () => void;
  onSetActiveResult: (id: string) => void;
  onPin: (id: string, pinned: boolean) => void;
  onCloseResult: (id: string) => void;
  onFetchMore: (entryId: string) => void;
  onExport: (sql: string) => void;
  onHistory: () => void;
  onTxMode: (manual: boolean) => void;
  onCommit: () => void;
  onRollback: () => void;
  onLoadDbs: () => void;
  onLoadSchemas: () => void;
  onSwitchDb: (db: string) => void;
  onSwitchSchema: (schema: string) => void;
  /** Data Source Options → Switch schema: Disable */
  schemaSwitchDisabled?: boolean;
  registerApi: (tabId: string, api: EditorApi) => void;
  onClearOutput: () => void;
}

export function ConsolePane(p: Props) {
  const [settings, setSettingsState] = useState<ExecSettings>(loadExecSettings);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [chooser, setChooser] = useState<{ options: { label: string; stmts: string[]; ranges: { start: number; end: number }[] }[]; idx: number; req: RunRequest } | null>(null);
  const [api, setApi] = useState<EditorApi | null>(null);

  const setSettings = (s: ExecSettings) => {
    setSettingsState(s);
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
    } catch {
      /* 저장 실패는 무시 */
    }
  };

  /** ⌘↩ 실행 규칙 (DataGrip Query Execution 설정) */
  const splitFn = p.dialect?.split ?? ((t: string) => splitScript(t, p.dialect?.monaco === "mysql"));
  const onRun = (req: RunRequest) => {
    const { text, offset, selection } = req;
    if (selection && selection.text.trim()) {
      if (settings.selection === "single") {
        req.highlight([{ start: selection.start, end: selection.end }]);
        return p.onExecute([selection.text.trim()], req.explain);
      }
      const segs = splitFn(selection.text);
      req.highlight(segs.map((s) => ({ start: selection.start + s.start, end: selection.start + s.end })));
      return p.onExecute(segs.map((s) => selection.text.slice(s.start, s.end).trim()), req.explain);
    }
    const plan = planExecution(text, offset, splitFn(text), settings);
    if (!plan) return;
    if ("run" in plan) {
      req.highlight(plan.run.map((x) => ({ start: x.start, end: x.end })));
      return p.onExecute(plan.run.map((x) => x.sql), req.explain);
    }
    // Ask what to execute: Statements 팝업
    setChooser({ options: plan.ask.map((o) => ({ label: o.label, stmts: o.stmts.map((x) => x.sql), ranges: o.stmts })), idx: 0, req });
  };

  const pickChoice = (i: number) => {
    if (!chooser) return;
    const o = chooser.options[i];
    chooser.req.highlight(o.ranges);
    p.onExecute(o.stmts, chooser.req.explain);
    setChooser(null);
    api?.focus();
  };

  const dropdown = (e: React.MouseEvent, items: MenuItem[]) => {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 2, items });
  };
  const radio = <T extends string>(cur: T, val: T, label: string, set: (v: T) => void): MenuItem => ({ label: (cur === val ? "● " : "　") + label, action: () => set(val) });

  const active = p.results.find((r) => r.id === p.activeResult);

  return (
    <div className="console-pane">
      {/* ── 콘솔 툴바 (DataGrip) ── */}
      <div className="dg-toolbar console-toolbar" onMouseDown={(e) => e.stopPropagation()}>
        <button className="tb run" title={`Execute (${isMac ? "⌘↩" : "Ctrl+Enter"})`} onClick={() => api?.run()} disabled={p.running}>▶</button>
        <button
          className="tb text"
          title="Explain Plan"
          onClick={(e) =>
            dropdown(e, [
              { label: "Explain Plan", action: () => api?.run("plan") },
              { label: "Explain Analyse", action: () => api?.run("analyze") },
            ])
          }
        >
          Explain ▾
        </button>
        <button className="tb" title={`Browse Query History (${isMac ? "⌥⌘e" : "Ctrl+Alt+e"})`} onClick={p.onHistory}><IcClock /></button>
        <button
          className="tb"
          title="Query Execution Settings"
          onClick={(e) =>
            dropdown(e, [
              { label: "When caret inside statement execute", submenu: [
                radio(settings.inside, "ask", "Ask what to execute", (v) => setSettings({ ...settings, inside: v })),
                radio(settings.inside, "smallest", "Smallest statement", (v) => setSettings({ ...settings, inside: v })),
                radio(settings.inside, "whole", "Whole script", (v) => setSettings({ ...settings, inside: v })),
                radio(settings.inside, "fromCaret", "Everything from caret", (v) => setSettings({ ...settings, inside: v })),
              ] },
              { label: "When caret outside statement execute", submenu: [
                radio(settings.outside, "nothing", "Nothing", (v) => setSettings({ ...settings, outside: v })),
                radio(settings.outside, "whole", "Whole script", (v) => setSettings({ ...settings, outside: v })),
                radio(settings.outside, "below", "Everything below caret", (v) => setSettings({ ...settings, outside: v })),
              ] },
              { label: "For selection execute", submenu: [
                radio(settings.selection, "single", "Exactly as a single statement", (v) => setSettings({ ...settings, selection: v })),
                radio(settings.selection, "separate", "Exactly as separate statements", (v) => setSettings({ ...settings, selection: v })),
              ] },
            ])
          }
        >
          <IcGear />
        </button>
        <span className="tb-sep" />
        <button
          className={"tb text" + (p.txManual ? " on" : "")}
          title="Transaction Mode"
          onClick={(e) =>
            dropdown(e, [
              { label: (!p.txManual ? "● " : "　") + "Auto", action: () => p.onTxMode(false) },
              { label: (p.txManual ? "● " : "　") + "Manual", action: () => p.onTxMode(true) },
            ])
          }
        >
          Tx: {p.txManual ? "Manual" : "Auto"} ▾
        </button>
        {p.txManual && (
          <>
            <button className={"tb text" + (p.inTx ? " accent" : "")} title="Commit" onClick={p.onCommit} disabled={!p.inTx}>✓ Commit</button>
            <button className="tb text" title="Roll back" onClick={p.onRollback} disabled={!p.inTx}>↶ Roll back</button>
          </>
        )}
        <button className="tb stop" title={`Cancel Running Statements (${isMac ? "⌘F2" : "Ctrl+F2"})`} onClick={p.onCancel} disabled={!p.running}><IcStop /></button>
        <span className="grow" />
        <span className="cb-ds">{p.connName}</span>
        <select className="cb-sel" value={p.database} onFocus={p.onLoadDbs} onMouseDown={p.onLoadDbs} onChange={(e) => p.onSwitchDb(e.target.value)} title="Database">
          {[...new Set([p.database, ...p.dbList])].map((d) => (
            <option key={d} value={d}>{d}</option>
          ))}
        </select>
        <select className="cb-sel" value={p.schema} onFocus={p.onLoadSchemas} onMouseDown={p.onLoadSchemas} onChange={(e) => p.onSwitchSchema(e.target.value)} disabled={p.schemaSwitchDisabled} title={p.schemaSwitchDisabled ? "Schema switching is disabled for this data source" : "Switch current schema"}>
          {[...new Set([p.schema, ...p.schemaList])].map((x) => (
            <option key={x} value={x}>{x}</option>
          ))}
        </select>
      </div>

      <PanelGroup direction="vertical" autoSaveId="dbtool-console">
        <Panel defaultSize={58} minSize={15}>
          <div className="editor-area">
            <ConsoleEditor
              tabId={p.tabId}
              connId={p.target}
              initialSql={p.initialSql}
              language={p.dialect?.monaco ?? "pgsql"}
              onChange={p.onChange}
              onRun={onRun}
              registerApi={(id, a) => {
                setApi(a);
                p.registerApi(id, a);
              }}
            />
            {chooser && (
              <div className="stmt-chooser" tabIndex={-1} ref={(el) => el?.focus()} onKeyDown={(e) => {
                if (e.key === "ArrowDown") setChooser({ ...chooser, idx: Math.min(chooser.idx + 1, chooser.options.length - 1) });
                else if (e.key === "ArrowUp") setChooser({ ...chooser, idx: Math.max(chooser.idx - 1, 0) });
                else if (e.key === "Enter") pickChoice(chooser.idx);
                else if (e.key === "Escape") { setChooser(null); api?.focus(); }
                e.preventDefault();
                e.stopPropagation();
              }}>
                <div className="sc-title">Statements</div>
                {chooser.options.map((o, i) => (
                  <div key={i} className={"sc-item" + (i === chooser.idx ? " on" : "")} onMouseEnter={() => setChooser({ ...chooser, idx: i })} onClick={() => pickChoice(i)}>
                    {o.label}
                  </div>
                ))}
              </div>
            )}
          </div>
        </Panel>
        <PanelResizeHandle className="resize-v" />
        <Panel defaultSize={42} minSize={10}>
          <div className="panel results-panel">
            {/* 결과 탭: Output + 결과 셋들 (Services 결과 영역) */}
            <div className="result-tabs">
              <div className={"rtab" + (p.activeResult === "output" ? " active" : "")} onClick={() => p.onSetActiveResult("output")}>Output</div>
              {p.results.map((r) => (
                <div
                  key={r.id}
                  className={"rtab" + (p.activeResult === r.id ? " active" : "")}
                  onClick={() => p.onSetActiveResult(r.id)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setMenu({
                      x: e.clientX,
                      y: e.clientY,
                      items: [
                        { label: r.pinned ? "Unpin Tab" : "Pin Tab", action: () => p.onPin(r.id, !r.pinned) },
                        { label: "Close", action: () => p.onCloseResult(r.id) },
                        { label: "Close All Unpinned", action: () => p.results.filter((x) => !x.pinned).forEach((x) => p.onCloseResult(x.id)) },
                      ],
                    });
                  }}
                  title={r.sql}
                >
                  {r.pinned && <span className="pin">📌</span>}
                  {r.kind === "plan" ? "Plan: " : ""}
                  {r.title}
                  <span className="rclose" onClick={(e) => { e.stopPropagation(); p.onCloseResult(r.id); }}>×</span>
                </div>
              ))}
              <span className="grow" />
              {p.activeResult === "output" && (
                <button className="tb" title="Clear All" onClick={p.onClearOutput}><IcReload /></button>
              )}
            </div>
            <div className="result-area">
              {p.activeResult === "output" && (
                <div className="output-log">
                  {p.output.length === 0 && <div className="muted pad">Execute a statement ({isMac ? "⌘↩" : "Ctrl+Enter"}) to see the output.</div>}
                  {p.output.map((l, i) => (
                    <div key={i} className={"ol " + l.level}>
                      <span className="ts">[{l.ts}]</span> {l.text}
                    </div>
                  ))}
                </div>
              )}
              {active?.kind === "result" && active.result && (
                <ResultGrid
                  result={active.result}
                  running={p.running}
                  loadingMore={p.loadingMore}
                  query={active.sql}
                  onFetchMore={() => p.onFetchMore(active.id)}
                  onSubmit={async () => {}}
                  onReload={() => p.onExecute([active.sql])}
                  onCancel={p.onCancel}
                  onExport={() => p.onExport(active.sql)}
                  dialect={p.dialect}
                />
              )}
              {active?.kind === "plan" && active.plan && <PlanView plan={active.plan} />}
            </div>
          </div>
        </Panel>
      </PanelGroup>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}

/** Query Plan: Operations Tree (DataGrip Explain Plan) + 노드마다 몫(%) 막대, 경고(⚠), 고른 노드의 상세 */
function PlanView({ plan }: { plan: PlanNode }) {
  const [sel, setSel] = useState<PlanNode | null>(plan);
  const rows: { n: PlanNode; depth: number }[] = [];
  const walk = (n: PlanNode, depth: number) => {
    rows.push({ n, depth });
    n.children.forEach((c) => walk(c, depth + 1));
  };
  walk(plan, 0);
  const fmt = (v?: number) => (v === undefined || v === null ? "" : Number.isInteger(v) ? v.toLocaleString("en-US") : v.toFixed(2));
  const analyzed = plan.actualTime !== undefined && plan.actualTime !== null;
  const warned = rows.filter((r) => r.n.warnings.length).length;
  return (
    <div className="plan-view">
      <div className="plan-table">
        {warned > 0 && <div className="plan-summary">⚠ {warned} node{warned > 1 ? "s" : ""} worth a look — click one for details</div>}
        <table>
          <thead>
            <tr>
              <th>Operation</th>
              <th>{analyzed ? "Time share" : "Cost share"}</th>
              <th>{analyzed ? "Self time" : "Self cost"}</th>
              <th>Params</th>
              <th>Cost (startup..total)</th>
              <th>Rows</th>
              <th>Actual rows</th>
              <th>Loops</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ n, depth }, i) => (
              <tr key={i} className={(sel === n ? "on " : "") + (n.warnings.length ? "warn" : "")} onClick={() => setSel(n)} title={n.warnings.join("\n")}>
                <td style={{ paddingLeft: 8 + depth * 16 }}>
                  {depth > 0 ? "└ " : ""}
                  {n.warnings.length ? <span className="plan-warn">⚠ </span> : null}
                  {n.op}
                </td>
                <td>
                  <span className="plan-bar">
                    <span style={{ width: `${Math.min(100, n.pct)}%`, background: n.pct >= 50 ? "var(--danger)" : n.pct >= 20 ? "var(--c-e5c07b, #e5c07b)" : "var(--accent)" }} />
                  </span>
                  <span className="plan-pct">{n.pct.toFixed(0)}%</span>
                </td>
                <td className="num">{analyzed ? (n.selfTime !== undefined ? `${fmt(n.selfTime)} ms` : "") : fmt(n.selfCost)}</td>
                <td className="muted">{n.detail}</td>
                <td className="num">{n.total !== undefined && n.total !== null ? `${fmt(n.startup)}..${fmt(n.total)}` : ""}</td>
                <td className="num">{fmt(n.rows)}</td>
                <td className="num">{fmt(n.actualRows)}</td>
                <td className="num">{fmt(n.loops)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {sel && (
        <div className="plan-detail">
          <div className="plan-detail-title">{sel.op}</div>
          {sel.warnings.map((w, i) => (
            <div key={i} className="plan-detail-warn">
              ⚠ {w}
            </div>
          ))}
          <table>
            <tbody>
              {sel.props.map(([k, v]) => (
                <tr key={k}>
                  <td className="muted">{k}</td>
                  <td>{v}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
