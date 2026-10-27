import { useEffect, useState } from "react";
import { Cell, Dialect, QueryResult } from "../lib/api";
import { EditTarget, ResultGrid } from "./ResultGrid";
import { IcFilter, IcOrder } from "./icons";

/** DataGrip 테이블 데이터 에디터: 그리드 툴바 + WHERE / ORDER BY 바 + 그리드 */
interface Props {
  title: string;
  where: string;
  orderBy: string;
  running: boolean;
  error: string | null;
  result: QueryResult | null;
  query: string | null;
  edit?: EditTarget;
  readOnly?: boolean;
  dialect?: Dialect;
  loadingMore: boolean;
  onApply: (where: string, orderBy: string) => void;
  onCancel: () => void;
  onFetchMore: () => void;
  onSubmit: (statements: string[]) => Promise<void>;
  onExportCsv: () => void;
  onDdl: () => void;
  onCopyQueryToConsole: (sql: string) => void;
  onFindSimilar?: (column: string, row: Record<string, Cell>) => void;
}

export function TableEditor(p: Props) {
  const [where, setWhere] = useState(p.where);
  const [orderBy, setOrderBy] = useState(p.orderBy);
  useEffect(() => setWhere(p.where), [p.where]);
  useEffect(() => setOrderBy(p.orderBy), [p.orderBy]);
  const apply = (w = where, o = orderBy) => p.onApply(w.trim(), o.trim());

  const field = (icon: React.ReactNode, kw: string, value: string, set: (v: string) => void, reset: string, onClear: () => void) => (
    <label className="dg-filter-field">
      {icon}
      <span className="kw">{kw}</span>
      <input
        value={value}
        spellCheck={false}
        onChange={(e) => set(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") apply();
          if (e.key === "Escape") {
            set(reset);
            (e.target as HTMLInputElement).blur();
          }
          e.stopPropagation();
        }}
      />
      {value && (
        <button className="clear" title="Clear" onClick={onClear}>
          ×
        </button>
      )}
    </label>
  );

  const filterBar = (
    <div className="dg-filter">
      {field(<IcFilter />, "WHERE", where, setWhere, p.where, () => {
        setWhere("");
        apply("", orderBy);
      })}
      {field(<IcOrder />, "ORDER BY", orderBy, setOrderBy, p.orderBy, () => {
        setOrderBy("");
        apply(where, "");
      })}
    </div>
  );

  const empty: QueryResult = { columns: [], rows: [], rowsAffected: 0, truncated: false, elapsedMs: 0, hasResultSet: true };

  return (
    <div className="table-editor">
      <div className="table-body">
        <ResultGrid
          result={p.result ?? empty}
          edit={p.edit}
          readOnly={p.readOnly}
          dialect={p.dialect}
          running={p.running}
          loadingMore={p.loadingMore}
          query={p.query}
          onFetchMore={p.onFetchMore}
          onSubmit={p.onSubmit}
          onReload={() => apply()}
          onCancel={p.onCancel}
          onExport={p.onExportCsv}
          onServerSort={(ob) => {
            setOrderBy(ob);
            apply(where, ob);
          }}
          onFilterBy={(w) => {
            setWhere(w);
            apply(w, orderBy);
          }}
          onDdl={p.onDdl}
          onCopyQueryToConsole={p.onCopyQueryToConsole}
          onFindSimilar={p.onFindSimilar}
          filterBar={filterBar}
        />
        {p.running && !p.result && <div className="grid-overlay muted">Loading {p.title}…</div>}
        {p.error && <pre className="error-box grid-overlay">{p.error}</pre>}
      </div>
    </div>
  );
}
