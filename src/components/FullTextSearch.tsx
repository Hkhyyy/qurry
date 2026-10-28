import { useState } from "react";
import { FtsHit, FtsRequest } from "../lib/api";
import { Button, Checkbox, EmptyState, FormGrid, IconButton, Modal, Notice, Select, Spacer, Table, Td, Text, TextInput, Th } from "../ui";

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
    <Modal
      title="Full-text Search"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={go} disabled={!text.trim()}>
            Search
          </Button>
        </>
      }
    >
      <FormGrid labelWidth={90}>
        <label>Search for</label>
        <TextInput
          autoFocus
          value={text}
          spellCheck={false}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") go();
          }}
        />
        <span />
        <div className="ui-row">
          <Checkbox checked={matchCase} onChange={(v) => setMatchCase(v)}>Match case</Checkbox>
          <Select value={mode} onChange={(e) => setMode(e.target.value as FtsRequest["mode"])}>
            {MODES.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </Select>
        </div>
        <label>Search in</label>
        <Select value={columns} onChange={(e) => setColumns(e.target.value as FtsRequest["columns"])}>
          <option value="text">Text columns</option>
          <option value="numeric">Numeric columns</option>
          <option value="all">All columns</option>
        </Select>
        <label>Show first</label>
        <div className="ui-row">
          <TextInput className="w-port" type="number" min={1} max={1000} value={limit} onChange={(e) => setLimit(Math.max(1, Number(e.target.value) || 1))} />
          <Text tone="muted">matches per table/view</Text>
        </div>
        <label>Scope</label>
        <span className="fts-scope" title={where}>
          {where}
        </span>
      </FormGrid>
    </Modal>
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
        <Spacer />
        {run && (
          <IconButton title={run.pinned ? "Unpin Tab" : "Pin Tab"} onClick={() => onPin(run.id)}>
            📌
          </IconButton>
        )}
        <IconButton title="Hide (⇧⎋)" onClick={onClose}>
          —
        </IconButton>
      </div>
      <div className="find-body">
        {!run && <EmptyState>No search results. Use Full-text Search (⌥⇧⌘F).</EmptyState>}
        {run?.running && <EmptyState>Searching {run.scope.connName} · {run.scope.db}…</EmptyState>}
        {run?.error && <Notice tone="danger">{run.error}</Notice>}
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
                    <span className="chev">{open ? "▾" : "▸"}</span> {key} <Text tone="muted">{h.result.rows.length}{h.result.truncated ? "+" : ""}</Text>
                  </div>
                  {open && (
                    <div className="find-rows">
                      <Table>
                        <thead>
                          <tr>
                            {h.result.columns.map((c) => (
                              <Th key={c.name}>{c.name}</Th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {h.result.rows.map((r, i) => (
                            <tr key={i} onDoubleClick={() => onOpen(run, h)}>
                              {r.map((v, j) => (
                                <Td key={j} className={v === null ? "null" : (run.matchCase ? String(v).includes(run.text) : String(v).toLowerCase().includes(run.text.toLowerCase())) ? "hit" : ""}>
                                  {v === null ? "<null>" : String(v).length > 80 ? String(v).slice(0, 80) + "…" : String(v)}
                                </Td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </Table>
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
