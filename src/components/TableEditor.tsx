import { ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Cell, Dialect, QueryResult } from "../lib/api";
import { filterSuggestions } from "../lib/completion";
import type { CompletionItem } from "../lib/shared";
import { EditTarget, ResultGrid } from "./ResultGrid";
import { ColumnIcon, IcFilter, IcOrder, SchemaIcon, TableIcon } from "../ui/icons";
import { IconButton, ListItem, listKey, Notice, PopupList, TextInput } from "../ui";

/** What the WHERE / ORDER BY fields complete against: the console's index of `target`, the tab's table and database */
export interface FilterCompletion {
  target: string;
  schema: string;
  table: string;
  db: string;
  mysql: boolean;
}

/** DataGrip table data editor: grid toolbar + WHERE / ORDER BY bar + grid */
interface Props {
  where: string;
  orderBy: string;
  running: boolean;
  error: string | null;
  /** Under the error: a way out of it (the TLS opt-in), or nothing */
  errorAction?: ReactNode;
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
  /** Absent: the fields are plain text */
  completion?: FilterCompletion;
}

export function TableEditor(p: Props) {
  const [where, setWhere] = useState(p.where);
  const [orderBy, setOrderBy] = useState(p.orderBy);
  useEffect(() => setWhere(p.where), [p.where]);
  useEffect(() => setOrderBy(p.orderBy), [p.orderBy]);
  const apply = (w = where, o = orderBy) => p.onApply(w.trim(), o.trim());

  const field = (icon: ReactNode, kw: "WHERE" | "ORDER BY", value: string, set: (v: string) => void, reset: string, onClear: () => void) => (
    <FilterField icon={icon} kw={kw} value={value} onChange={set} reset={reset} onApply={() => apply()} onClear={onClear} completion={p.completion} />
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
        {p.error && (
          <Notice tone="danger" className="grid-overlay">
            {p.error}
            {p.errorAction}
          </Notice>
        )}
      </div>
    </div>
  );
}

/** Keywords (and values, which the filter doesn't list) keep an empty icon slot, so every label starts in one column */
const ITEM_ICON: Record<CompletionItem["kind"], ReactNode> = { column: <ColumnIcon />, table: <TableIcon />, schema: <SchemaIcon />, keyword: <></>, value: <></> };

/**
 * One field of the filter bar. While an identifier is typed it lists the console's candidates for the field's text in its query
 * (`SELECT * FROM schema.table WHERE <text>`), with the keys of the column type ComboBox: ↑↓ PgUp PgDn move, ↩ / ⇥ pick, ⎋ closes the
 * list. With the list closed, ↩ applies the filter and ⎋ resets the field
 */
function FilterField(p: { icon: ReactNode; kw: "WHERE" | "ORDER BY"; value: string; onChange: (v: string) => void; reset: string; onApply: () => void; onClear: () => void; completion?: FilterCompletion }) {
  const input = useRef<HTMLInputElement>(null);
  const [list, setList] = useState<{ from: number; caret: number; items: CompletionItem[] } | null>(null);
  const [active, setActive] = useState(0);
  /** The last request: an answer to an older one is dropped */
  const asked = useRef(0);
  /** Where the caret goes after a pick (set once React has rendered the new value) */
  const caretAfter = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (caretAfter.current === null) return;
    input.current?.setSelectionRange(caretAfter.current, caretAfter.current);
    caretAfter.current = null;
  });
  const close = () => {
    asked.current++;
    setList(null);
  };
  const ask = async (text: string, caret: number, explicit = false) => {
    const c = p.completion;
    if (!c) return;
    const n = ++asked.current;
    const s = await filterSuggestions(c.target, { schema: c.schema, table: c.table, orderBy: p.kw === "ORDER BY", text, caret, db: c.db, mysql: c.mysql, explicit });
    if (n !== asked.current) return;
    setList(s.items.length ? { ...s, caret } : null);
    setActive(0);
  };
  const pick = (it: CompletionItem) => {
    if (!list) return;
    const v = p.value.slice(0, list.from) + it.insert + p.value.slice(list.caret);
    caretAfter.current = list.from + it.insert.length;
    p.onChange(v);
    close();
  };
  const open = !!list;
  return (
    <>
      <label className="dg-filter-field">
        {p.icon}
        <span className="kw">{p.kw}</span>
        <TextInput
          ref={input}
          value={p.value}
          spellCheck={false}
          aria-label={p.kw}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open}
          onChange={(e) => {
            p.onChange(e.target.value);
            // Typing opens or re-filters the list, deleting only re-filters an open one
            const typed = (e.nativeEvent as InputEvent).inputType?.startsWith("insert");
            if (typed || open) void ask(e.target.value, e.target.selectionStart ?? e.target.value.length);
          }}
          onBlur={close}
          onMouseDown={close}
          onKeyDown={(e) => {
            e.stopPropagation();
            // ⌃Space asks at the caret, as in the console (reopens the list after ⎋)
            if (e.key === " " && e.ctrlKey) {
              e.preventDefault();
              const el = e.target as HTMLInputElement;
              void ask(el.value, el.selectionStart ?? el.value.length, true);
              return;
            }
            if (open) {
              const k = e.key === "Tab" ? "pick" : listKey(e.key, active, list.items.length);
              if (k === "pick") pick(list.items[active]);
              else if (k === "close") close();
              else if (typeof k === "number") setActive(k);
              else if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) close();
              if (k !== null) e.preventDefault();
              return;
            }
            if (e.key === "Enter") p.onApply();
            if (e.key === "Escape") {
              p.onChange(p.reset);
              (e.target as HTMLInputElement).blur();
            }
          }}
        />
        {p.value && (
          <IconButton size="sm" className="clear" title="Clear" onClick={p.onClear}>
            ×
          </IconButton>
        )}
      </label>
      {open && (
        <PopupList anchor={input} active={active} minWidth={240} className="dg-filter-complete" onClose={close}>
          {list.items.map((it, i) => (
            <ListItem key={`${it.kind}:${it.insert}`} icon={ITEM_ICON[it.kind]} title={it.label} hint={it.detail} mono selected={i === active} onMouseEnter={() => setActive(i)} onClick={() => pick(it)} />
          ))}
        </PopupList>
      )}
    </>
  );
}
