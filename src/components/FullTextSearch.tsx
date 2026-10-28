import { useState } from "react";
import { FtsHit, FtsRequest } from "../lib/api";

export interface FtsScope {
  connId: string;
  connName: string;
  db: string;
  /** 스키마 단위 검색 */
  schemas: string[];
  /** 특정 테이블만 (트리에서 테이블을 골랐을 때) */
  tables: [string, string][];
}

const MODES: { id: FtsRequest["mode"]; label: string }[] = [
  { id: "contains", label: "Contains" },
  { id: "starts", label: "Starts with" },
  { id: "ends", label: "Ends with" },
  { id: "matches", label: "Matches" },
  { id: "like", label: "LIKE pattern" },
];

/** ⌥⇧⌘F Full-text Search (Find in database) */
export function FullTextSearchDialog({ scope, onClose, onSearch }: { scope: FtsScope; onClose: () => void; onSearch: (req: FtsRequest) => void }) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<FtsRequest["mode"]>("contains");
  const [matchCase, setMatchCase] = useState(false);
  const [columns, setColumns] = useState<FtsRequest["columns"]>("text");
  const [limit, setLimit] = useState(10);
  const where = scope.tables.length
    ? scope.tables.map(([s, t]) => `${s}.${t}`).join(", ")
    : `${scope.connName} · ${scope.db} · ${scope.schemas.length > 3 ? `${scope.schemas.length} schemas` : scope.schemas.join(", ")}`;
  const go = () => text.trim() && onSearch({ text, mode, matchCase, columns, schemas: scope.schemas, tables: scope.tables, limit });
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className="modal fts" onMouseDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
        <div className="modal-title">Full-text Search</div>
        <div className="form">
          <label>Search for</label>
          <input
            autoFocus
            value={text}
            spellCheck={false}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") go();
              if (e.key === "Escape") onClose();
            }}
          />
          <span />
          <div className="row">
            <label className="check">
              <input type="checkbox" checked={matchCase} onChange={(e) => setMatchCase(e.target.checked)} />
              Match case
            </label>
            <select value={mode} onChange={(e) => setMode(e.target.value as FtsRequest["mode"])}>
              {MODES.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
          </div>
          <label>Search in</label>
          <select value={columns} onChange={(e) => setColumns(e.target.value as FtsRequest["columns"])}>
            <option value="text">Text columns</option>
            <option value="numeric">Numeric columns</option>
            <option value="all">All columns</option>
          </select>
          <label>Show first</label>
          <div className="row">
            <input className="port" type="number" min={1} max={1000} value={limit} onChange={(e) => setLimit(Math.max(1, Number(e.target.value) || 1))} />
            <span className="lbl">matches per table/view</span>
          </div>
          <label>Scope</label>
          <span className="fts-scope" title={where}>
            {where}
          </span>
        </div>
        <div className="modal-actions">
          <span className="grow" />
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={go} disabled={!text.trim()}>
            Search
          </button>
        </div>
      </div>
    </div>
  );
}

export interface FindRun {
  id: string;
  title: string;
  scope: FtsScope;
  running: boolean;
  error?: string;
  hits: FtsHit[];
  pinned?: boolean;
  /** 검색어 (일치하는 셀 강조) */
  text: string;
  matchCase: boolean;
}

/** Find 도구 창 (⇧⌘F): 검색 결과를 테이블별로 묶어 보여 준다 */
export function FindPanel({ runs, active, onActive, onClose, onCloseRun, onPin, onOpen }: {
  runs: FindRun[];
  active: string | null;
  onActive: (id: string) => void;
  onClose: () => void;
  onCloseRun: (id: string) => void;
  onPin: (id: string) => void;
  onOpen: (run: FindRun, hit: FtsHit) => void;
}) {
  const run = runs.find((r) => r.id === active) ?? runs[runs.length - 1];
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  return (
    <div className="find-panel" data-toolwindow="find" tabIndex={-1}>
      <div className="find-head">
        <span className="find-title">Find:</span>
        {runs.map((r) => (
          <span key={r.id} className={"find-tab" + (r.id === run?.id ? " on" : "")} onClick={() => onActive(r.id)}>
            {r.pinned && "📌 "}
            {r.title}
            <span className="x" onClick={(e) => (e.stopPropagation(), onCloseRun(r.id))}>
              ×
            </span>
          </span>
        ))}
        <span className="grow" />
        {run && (
          <button className="icon" title={run.pinned ? "Unpin Tab" : "Pin Tab"} onClick={() => onPin(run.id)}>
            📌
          </button>
        )}
        <button className="icon" title="Hide (⇧⎋)" onClick={onClose}>
          —
        </button>
      </div>
      <div className="find-body">
        {!run && <div className="popup-empty">No search results. Use Full-text Search (⌥⇧⌘F).</div>}
        {run?.running && <div className="popup-empty">Searching {run.scope.connName} · {run.scope.db}…</div>}
        {run?.error && <pre className="error-box">{run.error}</pre>}
        {run && !run.running && !run.error && (
          <>
            <div className="find-summary">
              {run.hits.reduce((n, h) => n + h.result.rows.length, 0)} matches in {run.hits.length} tables
            </div>
            {run.hits.map((h) => {
              const key = `${h.schema}.${h.table}`;
              const open = !collapsed.has(key);
              return (
                <div key={key} className="find-group">
                  <div
                    className="find-table"
                    onClick={() => setCollapsed((s) => (s.has(key) ? (s.delete(key), new Set(s)) : new Set(s).add(key)))}
                    onDoubleClick={() => onOpen(run, h)}
                    title="Double-click to open the table filtered by the matches"
                  >
                    <span className="chev">{open ? "▾" : "▸"}</span> {key} <span className="muted">{h.result.rows.length}{h.result.truncated ? "+" : ""}</span>
                  </div>
                  {open && (
                    <div className="find-rows">
                      <table>
                        <thead>
                          <tr>
                            {h.result.columns.map((c) => (
                              <th key={c.name}>{c.name}</th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {h.result.rows.map((r, i) => (
                            <tr key={i} onDoubleClick={() => onOpen(run, h)}>
                              {r.map((v, j) => (
                                <td key={j} className={v === null ? "null" : (run.matchCase ? String(v).includes(run.text) : String(v).toLowerCase().includes(run.text.toLowerCase())) ? "hit" : ""}>
                                  {v === null ? "<null>" : String(v).length > 80 ? String(v).slice(0, 80) + "…" : String(v)}
                                </td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              );
            })}
          </>
        )}
      </div>
    </div>
  );
}
