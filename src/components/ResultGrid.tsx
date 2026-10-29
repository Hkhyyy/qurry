import { setStatusWidget } from "../lib/statusbar";
import { fracWidth, isTimeType, padFrac } from "../lib/timeFrac";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { Cell, Dialect, dialectOf, QueryResult } from "../lib/api";
import { DateTimePicker, isTemporal } from "./DateTimePicker";
import { ContextMenu, MenuItem } from "./Explorer";
import { inList } from "../lib/inList";
import { ResultChart } from "./ResultChart";
import { Button, CellEditor, CodeBlock, GridCell, GridHeaderCell, GridHeaderRow, GridRow, IconButton, Modal, Notice, RowNumberCell, SearchField, Spacer, Text, TextArea, ToolButton, Toolbar, ToolbarSep } from "../ui";
import {
  IcChart, IcClock, IcExport, IcGear, IcMinus, IcPlus, IcPreview, IcReload, IcRevert, IcSearch, IcStop, IcSubmit, IcTranspose, IcView,
} from "../ui/icons";

const ROW_H = 24;
const HEADER_H = 26;
const ROWNUM_W = 52;
const isMac = navigator.platform.toLowerCase().includes("mac");

export interface EditTarget {
  schema: string;
  table: string;
  pk: string[];
}

/** DataGrip Data Extractors (copy formats) */
export const EXTRACTORS = ["CSV", "TSV", "SQL Inserts", "SQL Updates", "Where Clause", "SQL IN List", "JSON", "Markdown"] as const;
export type Extractor = (typeof EXTRACTORS)[number];
let lastExtractor: Extractor = "CSV"; // The last chosen extractor is the default for new tabs (DataGrip)

/** Display value inserted by Set DEFAULT */
export const DEFAULT_TOKEN = "\u0000DEFAULT";

interface Props {
  result: QueryResult;
  /** For the table data editor: the edit target */
  edit?: EditTarget;
  /** Read-only data source: editing disabled */
  readOnly?: boolean;
  /** SQL dialect (identifier quoting, literals) */
  dialect?: Dialect;
  running: boolean;
  loadingMore: boolean;
  query: string | null;
  onFetchMore: () => void;
  onSubmit: (statements: string[]) => Promise<void>;
  onReload: () => void;
  onCancel: () => void;
  onExport: () => void;
  /** Table data: server-side sort (ORDER BY) */
  onServerSort?: (orderBy: string) => void;
  /** Table data: Filter by ▸ */
  onFilterBy?: (where: string) => void;
  onDdl?: () => void;
  onCopyQueryToConsole?: (sql: string) => void;
  /** Table data: WHERE / ORDER BY bar below the toolbar */
  filterBar?: React.ReactNode;
  /** Row actions (DBA reports: end session, generate SQL) — top of the context menu. row = column name → value */
  rowActions?: { label: string; danger?: boolean; run: (row: Record<string, Cell>) => void }[];
  /** Table data (pgvector): find rows close to this row. row = column name → value */
  onFindSimilar?: (column: string, row: Record<string, Cell>) => void;
}

/** pgvector columns (vector · halfvec · sparsevec) */
const isVectorType = (t: string) => /^(vector|halfvec|sparsevec)$/i.test(t);

type SortKey = { col: number; dir: "asc" | "desc" };
type Pos = { r: number; c: number }; // r = on-screen (display) order

function display(c: Cell): string {
  if (c === null) return "<null>";
  if (c === DEFAULT_TOKEN) return "<default>";
  return String(c);
}
const raw = (c: Cell) => (c === null ? "" : c === DEFAULT_TOKEN ? "DEFAULT" : String(c));

/** Cells whose original value is not shown as is (arrays, bytea, etc.) are not editable. */
const editableValue = (v: Cell, typeName: string) => typeName !== "BYTEA" && !(typeof v === "string" && /^<[A-Z0-9_ \[\]]+>$/.test(v));

function csvField(s: string, sep: string) {
  return s.includes(sep) || s.includes('"') || s.includes("\n") ? `"${s.replace(/"/g, '""')}"` : s;
}

export function ResultGrid(p: Props) {
  const { result, edit, loadingMore } = p;
  const D = p.dialect ?? dialectOf("postgres");
  const quoteIdent = D.q;
  const sqlLiteral = D.lit;
  const lit = (c: Cell) => (c === DEFAULT_TOKEN ? "DEFAULT" : sqlLiteral(c));
  const cols = result.columns;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [sorts, setSorts] = useState<SortKey[]>([]);
  const [selA, setSelA] = useState<Pos | null>(null);
  const [selB, setSelB] = useState<Pos | null>(null);
  const [editing, setEditing] = useState<{ r: number; c: number; value: string; picker?: boolean } | null>(null);
  const [changes, setChanges] = useState<Record<number, Record<number, Cell>>>({});
  const [deleted, setDeleted] = useState<Set<number>>(new Set());
  const [added, setAdded] = useState<Cell[][]>([]);
  const [preview, setPreview] = useState<{ sql: string[]; readOnly?: boolean } | null>(null);
  const [submitErr, setSubmitErr] = useState<string | null>(null);
  const [extractor, setExtractorState] = useState<Extractor>(lastExtractor);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [findOpen, setFindOpen] = useState(false);
  const [findText, setFindText] = useState("");
  const [showFilter, setShowFilter] = useState(true);
  const [sortViaOrderBy, setSortViaOrderBy] = useState(true);
  const [transposed, setTransposed] = useState(false);
  /** View as › Chart */
  const [chart, setChart] = useState(false);
  const [interval, setIntervalSec] = useState(0);
  const [valueEditor, setValueEditor] = useState<{ r: number; c: number; text: string } | null>(null);
  const [showQuery, setShowQuery] = useState(false);
  const dragging = useRef(false);

  const setExtractor = (x: Extractor) => {
    lastExtractor = x;
    setExtractorState(x);
  };

  const pkIdx = useMemo(() => (edit ? edit.pk.map((k) => cols.findIndex((c) => c.name === k)) : []), [edit, cols]);
  const canEdit = !p.readOnly && !!edit && edit.pk.length > 0 && pkIdx.every((i) => i >= 0);
  const baseCount = result.rows.length;
  const rowCount = baseCount + added.length;
  const serverSort = !!p.onServerSort && sortViaOrderBy;

  // Reset sorting when the column set changes
  const colKey = cols.map((c) => c.name).join("\u0001");
  useEffect(() => setSorts([]), [colKey]);

  /** Header sort: click = ↑ → ↓ → off, ⌥+click = add a sort key (DataGrip) */
  const clickSort = (col: number, additive: boolean) => {
    setSorts((cur) => {
      const ex = cur.find((k) => k.col === col);
      const nextDir = !ex ? "asc" : ex.dir === "asc" ? "desc" : null;
      let next: SortKey[];
      if (additive) next = nextDir ? (ex ? cur.map((k) => (k.col === col ? { col, dir: nextDir } : k)) : [...cur, { col, dir: nextDir }]) : cur.filter((k) => k.col !== col);
      else next = nextDir ? [{ col, dir: nextDir }] : [];
      if (serverSort) p.onServerSort!(next.map((k) => `${quoteIdent(cols[k.col].name)} ${k.dir.toUpperCase()}`).join(", "));
      return next;
    });
  };

  // Client-side sort (when not sorting on the server)
  const order = useMemo(() => {
    if (serverSort || sorts.length === 0) return null;
    const idx = result.rows.map((_, i) => i);
    const cmp = (a: Cell, b: Cell) => {
      if (a === b) return 0;
      if (a === null) return 1;
      if (b === null) return -1;
      if (typeof a === "number" && typeof b === "number") return a - b;
      const na = Number(a), nb = Number(b);
      if (a !== "" && b !== "" && !isNaN(na) && !isNaN(nb)) return na - nb;
      return String(a).localeCompare(String(b));
    };
    idx.sort((x, y) => {
      for (const k of sorts) {
        const d = cmp(result.rows[x][k.col], result.rows[y][k.col]);
        if (d) return k.dir === "asc" ? d : -d;
      }
      return x - y;
    });
    return idx;
  }, [sorts, result.rows, serverSort]);
  const dataIndex = (d: number) => (order && d < order.length ? order[d] : d);

  const rowAt = (r: number) => (r < baseCount ? result.rows[r] : added[r - baseCount]);
  const valueAt = (r: number, c: number): Cell => (r < baseCount && changes[r] && c in changes[r] ? changes[r][c] : rowAt(r)?.[c] ?? null);

  // Reset edit state when the result changes (kept when load more only added rows)
  const firstRow = result.rows[0];
  useEffect(() => {
    setSelA(null);
    setSelB(null);
    setEditing(null);
    setChanges({});
    setDeleted(new Set());
    setAdded([]);
  }, [cols, firstRow]);

  // Auto refresh (Update Interval)
  const pendingCount = Object.keys(changes).length + deleted.size + added.length;
  useEffect(() => {
    if (!interval) return;
    const t = setInterval(() => {
      // Pause while hidden behind another tab (display:none) or the window is hidden, and resume refreshing once visible again
      const visible = !document.hidden && scrollRef.current?.offsetParent != null;
      if (visible && !p.running && pendingCount === 0) p.onReload();
    }, interval * 1000);
    return () => clearInterval(t);
  }, [interval, p.running, pendingCount]);

  const widths = useMemo(
    () =>
      cols.map((col, ci) => {
        const head = col.name.length * 7.6 + col.typeName.length * 6.2 + 48 + (edit?.pk.includes(col.name) ? 16 : 0);
        let len = 0;
        for (const row of result.rows.slice(0, 200)) len = Math.max(len, display(row[ci]).length);
        return Math.min(Math.max(head, len * 7.3 + 20, 64), 420);
      }),
    [cols, firstRow, edit],
  );
  const total = ROWNUM_W + widths.reduce((a, b) => a + b, 0);
  // Time columns: align fractional-second digits within the column (values unchanged)
  const fracW = useMemo(() => cols.map((c, ci) => (isTimeType(c.typeName) ? fracWidth(result.rows.map((r) => r[ci])) : 0)), [cols, result.rows]);
  const shown = (v: Cell, ci: number) => (fracW[ci] && typeof v === "string" ? padFrac(v, fracW[ci]) : display(v));

  const virt = useVirtualizer({ count: rowCount, getScrollElement: () => scrollRef.current, estimateSize: () => ROW_H, overscan: 20 });
  // Horizontally, render only visible columns too (tables with many columns). Blank cells fill the width on both sides
  const colVirt = useVirtualizer({ horizontal: true, count: cols.length, getScrollElement: () => scrollRef.current, estimateSize: (i) => widths[i] ?? 64, paddingStart: ROWNUM_W, overscan: 4 });
  useEffect(() => colVirt.measure(), [widths]);
  const colItems = colVirt.getVirtualItems();
  const padL = colItems.length ? colItems[0].start - ROWNUM_W : 0;
  const padR = colItems.length ? colVirt.getTotalSize() - colItems[colItems.length - 1].end : 0;
  const spacer = (w: number) => (w > 0 ? <div style={{ flex: "none", width: w }} /> : null);
  const items = virt.getVirtualItems();
  const lastItem = items[items.length - 1];
  useEffect(() => {
    if (lastItem && result.truncated && !loadingMore && lastItem.index >= baseCount - 30) p.onFetchMore();
  }, [lastItem?.index, result.truncated, loadingMore, baseCount]);

  // ─── Selection ───
  const rect = selA && selB ? { r0: Math.min(selA.r, selB.r), r1: Math.max(selA.r, selB.r), c0: Math.min(selA.c, selB.c), c1: Math.max(selA.c, selB.c) } : null;
  const inSel = (d: number, c: number) => !!rect && d >= rect.r0 && d <= rect.r1 && c >= rect.c0 && c <= rect.c1;
  const active = selB ? { r: dataIndex(selB.r), c: selB.c } : null; // data coordinates
  const selRows = (): number[] => (rect ? Array.from({ length: rect.r1 - rect.r0 + 1 }, (_, i) => dataIndex(rect.r0 + i)) : []);
  const selectCell = (pos: Pos, extend: boolean) => {
    if (!extend || !selA) setSelA(pos);
    setSelB(pos);
  };

  // ─── Find (Find on Current Page) ───
  const findLower = findText.toLowerCase();
  const matches = useMemo(() => {
    if (!findOpen || !findLower) return [] as Pos[];
    const out: Pos[] = [];
    for (let d = 0; d < rowCount && out.length < 5000; d++) {
      const r = dataIndex(d);
      for (let c = 0; c < cols.length; c++) if (display(valueAt(r, c)).toLowerCase().includes(findLower)) out.push({ r: d, c });
    }
    return out;
  }, [findOpen, findLower, result.rows, changes, added, order]);
  const isMatch = (d: number, c: number) => findOpen && !!findLower && display(valueAt(dataIndex(d), c)).toLowerCase().includes(findLower);
  const gotoMatch = (dir: 1 | -1) => {
    if (!matches.length) return;
    const cur = selB ? matches.findIndex((m) => m.r === selB.r && m.c === selB.c) : -1;
    const m = matches[(cur + dir + matches.length) % matches.length];
    selectCell(m, false);
    virt.scrollToIndex(m.r);
    colVirt.scrollToIndex(m.c);
  };

  // ─── Editing ───
  const commitEdit = () => {
    if (!editing) return;
    const { r, c, value } = editing;
    setCell(r, c, value);
    setEditing(null);
    scrollRef.current?.focus();
  };
  const setCell = (r: number, c: number, next: Cell) => {
    if (r >= baseCount) {
      setAdded((a) => a.map((row, i) => (i === r - baseCount ? row.map((v, j) => (j === c ? next : v)) : row)));
      return;
    }
    const orig = result.rows[r][c];
    if (next === orig || (orig !== null && next !== null && next !== DEFAULT_TOKEN && display(orig) === String(next))) {
      setChanges((ch) => {
        if (!ch[r] || !(c in ch[r])) return ch;
        const row = { ...ch[r] };
        delete row[c];
        const n = { ...ch };
        if (Object.keys(row).length) n[r] = row;
        else delete n[r];
        return n;
      });
      return;
    }
    setChanges((ch) => ({ ...ch, [r]: { ...ch[r], [c]: next } }));
  };
  const startEdit = (r: number, c: number) => {
    if (!canEdit || deleted.has(r)) return;
    const v = valueAt(r, c);
    if (r < baseCount && !editableValue(v, cols[c].typeName)) return;
    setEditing({ r, c, value: v === null || v === DEFAULT_TOKEN ? "" : String(v) });
  };
  // Status bar Aggregator: Count / Sum / Avg / Min / Max when 2+ cells are selected
  useEffect(() => {
    if (!rect || (rect.r0 === rect.r1 && rect.c0 === rect.c1)) return setStatusWidget("aggregate", undefined);
    let count = 0, nums = 0, sum = 0, min = Infinity, max = -Infinity;
    for (let d = rect.r0; d <= rect.r1 && count < 200000; d++)
      for (let c = rect.c0; c <= rect.c1; c++) {
        const v = valueAt(dataIndex(d), c);
        if (v === null) continue;
        count++;
        const n = typeof v === "number" ? v : typeof v === "string" && /^-?\d+(\.\d+)?(e[+-]?\d+)?$/i.test(v.trim()) ? Number(v) : NaN;
        if (!Number.isNaN(n)) {
          nums++;
          sum += n;
          min = Math.min(min, n);
          max = Math.max(max, n);
        }
      }
    const f = (n: number) => (Number.isInteger(n) ? n.toLocaleString() : n.toLocaleString(undefined, { maximumFractionDigits: 4 }));
    setStatusWidget("aggregate", nums ? `Count: ${count}  Sum: ${f(sum)}  Avg: ${f(sum / nums)}  Min: ${f(min)}  Max: ${f(max)}` : `Count: ${count}`);
  }, [rect?.r0, rect?.r1, rect?.c0, rect?.c1, result, changes]);
  useEffect(() => () => setStatusWidget("aggregate", undefined), []);

  const forSelected = (fn: (r: number, c: number) => void) => {
    if (!rect) return;
    for (let d = rect.r0; d <= rect.r1; d++) for (let c = rect.c0; c <= rect.c1; c++) fn(dataIndex(d), c);
  };
  const setNull = () => canEdit && forSelected((r, c) => setCell(r, c, null));
  const setDefault = () => canEdit && forSelected((r, c) => setCell(r, c, DEFAULT_TOKEN));
  const genUuid = () => canEdit && forSelected((r, c) => setCell(r, c, crypto.randomUUID()));

  const addRow = () => {
    if (!canEdit) return;
    setAdded((a) => [...a, cols.map(() => null)]);
    setTimeout(() => {
      virt.scrollToIndex(rowCount);
      selectCell({ r: rowCount, c: 0 }, false);
    });
  };
  const cloneRow = () => {
    if (!canEdit || !active) return;
    const src = cols.map((_, c) => (pkIdx.includes(c) ? null : valueAt(active.r, c)));
    setAdded((a) => [...a, src]);
    setTimeout(() => virt.scrollToIndex(rowCount));
  };
  const deleteRows = () => {
    if (!canEdit || !rect) return;
    const rows = selRows();
    const newOnes = rows.filter((r) => r >= baseCount).map((r) => r - baseCount);
    if (newOnes.length) setAdded((a) => a.filter((_, i) => !newOnes.includes(i)));
    setDeleted((d) => {
      const n = new Set(d);
      rows.filter((r) => r < baseCount).forEach((r) => n.add(r));
      return n;
    });
  };
  /** Revert Selected: revert only the selection, or everything if nothing is selected */
  const revertSelected = () => {
    if (!rect) {
      setChanges({});
      setDeleted(new Set());
      setAdded([]);
      return;
    }
    const rows = selRows();
    setChanges((ch) => {
      const n = { ...ch };
      for (const r of rows) {
        if (!n[r]) continue;
        const row = { ...n[r] };
        for (let c = rect.c0; c <= rect.c1; c++) delete row[c];
        if (Object.keys(row).length) n[r] = row;
        else delete n[r];
      }
      return n;
    });
    setDeleted((d) => {
      const n = new Set(d);
      rows.forEach((r) => n.delete(r));
      return n;
    });
    const newOnes = rows.filter((r) => r >= baseCount).map((r) => r - baseCount);
    if (newOnes.length) setAdded((a) => a.filter((_, i) => !newOnes.includes(i)));
  };

  const tableRef = edit ? `${quoteIdent(edit.schema)}.${quoteIdent(edit.table)}` : "MY_TABLE";
  const buildStatements = (): string[] => {
    if (!edit) return [];
    const where = (r: number) => pkIdx.map((i) => `${quoteIdent(cols[i].name)} = ${sqlLiteral(result.rows[r][i])}`).join(" AND ");
    const out: string[] = [];
    for (const r of [...deleted].sort((a, b) => a - b)) out.push(`DELETE FROM ${tableRef} WHERE ${where(r)};`);
    for (const [rs, ch] of Object.entries(changes)) {
      const r = Number(rs);
      if (deleted.has(r)) continue;
      const sets = Object.entries(ch).map(([c, v]) => `${quoteIdent(cols[Number(c)].name)} = ${lit(v)}`).join(", ");
      out.push(`UPDATE ${tableRef} SET ${sets} WHERE ${where(r)};`);
    }
    for (const row of added) {
      const idx = row.map((v, i) => (v === null ? -1 : i)).filter((i) => i >= 0);
      out.push(
        idx.length === 0
          ? `INSERT INTO ${tableRef} DEFAULT VALUES;`
          : `INSERT INTO ${tableRef} (${idx.map((i) => quoteIdent(cols[i].name)).join(", ")}) VALUES (${idx.map((i) => lit(row[i])).join(", ")});`,
      );
    }
    return out;
  };
  const submit = async () => {
    const sql = buildStatements();
    if (!sql.length) return;
    setSubmitErr(null);
    try {
      await p.onSubmit(sql);
    } catch (e) {
      setPreview({ sql });
      setSubmitErr(String(e));
    }
  };
  const doSubmitPreview = async () => {
    if (!preview) return;
    setSubmitErr(null);
    try {
      await p.onSubmit(preview.sql);
      setPreview(null);
    } catch (e) {
      setSubmitErr(String(e));
    }
  };

  // ─── Copy (Data Extractor) ───
  /** header = a column-name line at the top of CSV · TSV (Copy is values only — DataGrip; Copy with Headers includes it) */
  const extract = (rows: number[], c0: number, c1: number, fmt: Extractor, header = true): string => {
    const cs = Array.from({ length: c1 - c0 + 1 }, (_, i) => c0 + i);
    const names = cs.map((c) => cols[c].name);
    const vals = rows.map((r) => cs.map((c) => valueAt(r, c)));
    switch (fmt) {
      case "TSV":
        return [...(header ? [names.join("\t")] : []), ...vals.map((v) => v.map(raw).join("\t"))].join("\n");
      case "CSV":
        return [...(header ? [names.map((n) => csvField(n, ",")).join(",")] : []), ...vals.map((v) => v.map((x) => csvField(raw(x), ",")).join(","))].join("\n");
      case "SQL Inserts":
        return vals.map((v) => `INSERT INTO ${tableRef} (${names.map(quoteIdent).join(", ")}) VALUES (${v.map(lit).join(", ")});`).join("\n");
      case "SQL Updates":
        return rows
          .map((r, i) => {
            const w = pkIdx.length ? pkIdx.map((k) => `${quoteIdent(cols[k].name)} = ${sqlLiteral(valueAt(r, k))}`).join(" AND ") : "/* no key */";
            return `UPDATE ${tableRef} SET ${names.map((n, j) => `${quoteIdent(n)} = ${lit(vals[i][j])}`).join(", ")} WHERE ${w};`;
          })
          .join("\n");
      case "Where Clause":
        return vals.map((v) => "(" + names.map((n, j) => (v[j] === null ? `${quoteIdent(n)} IS NULL` : `${quoteIdent(n)} = ${lit(v[j])}`)).join(" AND ") + ")").join(" OR\n");
      case "SQL IN List":
        return inList(vals.map((v) => v.map((x) => (x === DEFAULT_TOKEN ? null : x))), cs.map((c) => cols[c].typeName));
      case "JSON":
        return JSON.stringify(vals.map((v) => Object.fromEntries(names.map((n, j) => [n, v[j] === DEFAULT_TOKEN ? null : v[j]]))), null, 2);
      case "Markdown":
        return [
          `| ${names.join(" | ")} |`,
          `| ${names.map(() => "---").join(" | ")} |`,
          ...vals.map((v) => `| ${v.map((x) => (x === null ? "" : raw(x)).replace(/\|/g, "\\|")).join(" | ")} |`),
        ].join("\n");
    }
  };
  const copySelection = (header = false) => {
    if (!rect) return;
    const single = rect.r0 === rect.r1 && rect.c0 === rect.c1;
    const text = single && !header && (extractor === "CSV" || extractor === "TSV") ? raw(valueAt(dataIndex(rect.r0), rect.c0)) : extract(selRows(), rect.c0, rect.c1, extractor, header);
    navigator.clipboard.writeText(text);
  };
  /** Selected cells as ('a', 'b') (paste straight into WHERE x IN). If only one cell is selected, its whole column */
  const copyInList = () => {
    if (!rect) return;
    const single = rect.r0 === rect.r1 && rect.c0 === rect.c1;
    const rows = single ? Array.from({ length: rowCount }, (_, i) => dataIndex(i)) : selRows();
    navigator.clipboard.writeText(extract(rows, rect.c0, rect.c1, "SQL IN List"));
  };
  /** Paste: split multi-line TSV/CSV into cells, adding new rows if there are not enough (DataGrip) */
  const paste = async () => {
    if (!canEdit || !selB) return;
    const text = await navigator.clipboard.readText();
    const lines = text.replace(/\r/g, "").split("\n").filter((l, i, a) => l !== "" || i < a.length - 1);
    const grid = lines.map((l) => l.split(l.includes("\t") ? "\t" : ","));
    grid.forEach((cells, i) => {
      const d = selB.r + i;
      const r = d < rowCount ? dataIndex(d) : -1;
      if (r < 0) {
        const row: Cell[] = cols.map(() => null);
        cells.forEach((v, j) => {
          if (selB.c + j < cols.length) row[selB.c + j] = v;
        });
        setAdded((a) => [...a, row]);
      } else cells.forEach((v, j) => selB.c + j < cols.length && setCell(r, selB.c + j, v));
    });
  };

  // ─── Keyboard (DataGrip grid keymap) ───
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (editing || valueEditor) return;
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const k = e.key.toLowerCase();
    const stop = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (mod && k === "a") {
      setSelA({ r: 0, c: 0 });
      setSelB({ r: rowCount - 1, c: cols.length - 1 });
      return stop();
    }
    if (mod && k === "c" && !e.shiftKey) return copySelection(false), stop();
    if (mod && k === "v") return paste(), stop();
    if (mod && k === "f") {
      setFindOpen(true);
      setTimeout(() => document.querySelector<HTMLInputElement>(".grid-find input")?.focus());
      return stop();
    }
    if (mod && k === "r") return p.onReload(), stop();
    // While running, the global shortcut (editor.cancel) already cancelled (so it is not called twice)
    if (mod && e.key === "F2") return e.defaultPrevented || p.onCancel(), stop();
    if (mod && e.altKey && e.code === "KeyN") return setNull(), stop();
    if (mod && e.altKey && e.code === "KeyD") return setDefault(), stop();
    if (mod && e.altKey && e.code === "KeyZ") return revertSelected(), stop();
    if (mod && k === "n") return addRow(), stop();
    if (mod && k === "d") return cloneRow(), stop();
    if (mod && e.key === "Backspace") return deleteRows(), stop();
    if (mod && e.key === "Enter") return submit(), stop();
    if (e.shiftKey && e.key === "Enter" && active) {
      setValueEditor({ r: active.r, c: active.c, text: raw(valueAt(active.r, active.c)) });
      return stop();
    }
    if (!selB) return;
    if ((e.key === "Enter" || e.key === "F2") && canEdit && active) return startEdit(active.r, active.c), stop();
    if (e.key === "Escape") {
      setFindOpen(false);
      return;
    }
    const move: Record<string, [number, number]> = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1], Tab: [0, e.shiftKey ? -1 : 1] };
    const m = move[e.key];
    if (m) {
      const r = Math.min(Math.max(selB.r + m[0], 0), rowCount - 1);
      const c = Math.min(Math.max(selB.c + m[1], 0), cols.length - 1);
      selectCell({ r, c }, e.shiftKey && e.key !== "Tab");
      virt.scrollToIndex(r);
      colVirt.scrollToIndex(c);
      return stop();
    }
    // Typing directly in a cell starts editing (DataGrip)
    if (canEdit && active && e.key.length === 1 && !mod && !e.altKey) {
      startEdit(active.r, active.c);
      setEditing((ed) => (ed ? { ...ed, value: e.key } : ed));
      return stop();
    }
  };

  // ─── Cell context menu ───
  const openCellMenu = (e: React.MouseEvent, d: number, c: number) => {
    e.preventDefault();
    if (!inSel(d, c)) selectCell({ r: d, c }, false);
    const r = dataIndex(d);
    const v = valueAt(r, c);
    const k = isMac
      ? { vedit: "⇧↩", nul: "⌥⌘n", def: "⌥⌘d", copy: "⌘c", paste: "⌘v", del: "⌘⌫", clone: "⌘d", revert: "⌥⌘z", rec: "⇧⌘↩" }
      : { vedit: "Shift+Enter", nul: "Ctrl+Alt+n", def: "Ctrl+Alt+d", copy: "Ctrl+c", paste: "Ctrl+v", del: "Ctrl+Backspace", clone: "Ctrl+d", revert: "Ctrl+Alt+z", rec: "Ctrl+Shift+Enter" };
    const name = quoteIdent(cols[c].name);
    // Row values at the moment the menu opened (the chosen row stays even if auto refresh reorders rows — so the wrong session is not killed)
    const rowObj = Object.fromEntries(cols.map((col, i) => [col.name, valueAt(r, i)]));
    const items: MenuItem[] = [
      ...(p.rowActions?.length ? ([...p.rowActions.map((a) => ({ label: a.label, danger: a.danger, action: () => a.run(rowObj) })), { separator: true }] as MenuItem[]) : []),
      ...(canEdit ? ([{ label: "Edit", shortcut: "↩", action: () => startEdit(r, c) }] as MenuItem[]) : []),
      { label: "Open in Value Editor", shortcut: k.vedit, action: () => setValueEditor({ r, c, text: raw(v) }) },
      ...(canEdit
        ? ([
            { label: "Revert Selected", shortcut: k.revert, action: revertSelected },
            { separator: true },
            { label: "Set DEFAULT", shortcut: k.def, action: setDefault },
            { label: "NULL", shortcut: k.nul, action: setNull },
            { label: "Generate UUID", action: genUuid },
          ] as MenuItem[])
        : []),
      { separator: true },
      { label: "Copy", shortcut: k.copy, action: () => copySelection(false) },
      { label: "Copy with Headers", action: () => copySelection(true), disabled: extractor !== "CSV" && extractor !== "TSV" },
      { label: "Copy as IN List  ('a', 'b', …)", action: copyInList },
      { label: "Change Data Extractor", submenu: EXTRACTORS.map((x) => ({ label: (x === extractor ? "✓ " : "　") + x, action: () => setExtractor(x) })) },
      ...(canEdit ? ([{ label: "Paste", shortcut: k.paste, action: paste }] as MenuItem[]) : []),
      ...(canEdit
        ? ([
            { separator: true },
            { label: "Add Row", action: addRow },
            { label: "Delete Row", shortcut: k.del, action: deleteRows },
            { label: "Clone Row", shortcut: k.clone, action: cloneRow },
          ] as MenuItem[])
        : []),
      ...(p.onFindSimilar && p.edit?.pk.length
        ? cols
            .filter((col) => isVectorType(col.typeName))
            .map((col): MenuItem => ({ label: `Find Similar Rows by ${col.name}`, action: () => p.onFindSimilar!(col.name, rowObj) }))
        : []),
      ...(p.onFilterBy
        ? ([
            { separator: true },
            {
              label: "Filter by",
              submenu:
                v === null
                  ? [
                      { label: `${cols[c].name} IS NULL`, action: () => p.onFilterBy!(`${name} IS NULL`) },
                      { label: `${cols[c].name} IS NOT NULL`, action: () => p.onFilterBy!(`${name} IS NOT NULL`) },
                    ]
                  : [
                      { label: `${cols[c].name} = ${sqlLiteral(v)}`, action: () => p.onFilterBy!(`${name} = ${sqlLiteral(v)}`) },
                      { label: `${cols[c].name} <> ${sqlLiteral(v)}`, action: () => p.onFilterBy!(`${name} <> ${sqlLiteral(v)}`) },
                      { label: `${cols[c].name} LIKE '%${raw(v)}%'`, action: () => p.onFilterBy!(`${D.text(name)} LIKE ${sqlLiteral(`%${raw(v)}%`)}`) },
                      { label: `${cols[c].name} IS NULL`, action: () => p.onFilterBy!(`${name} IS NULL`) },
                      { label: `${cols[c].name} IS NOT NULL`, action: () => p.onFilterBy!(`${name} IS NOT NULL`) },
                    ],
            },
          ] as MenuItem[])
        : []),
      { separator: true },
      { label: "Export Data…", action: p.onExport },
      ...(p.onDdl ? ([{ label: "Go To DDL", shortcut: isMac ? "⌘b" : "Ctrl+b", action: p.onDdl }] as MenuItem[]) : []),
    ];
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  // ─── Toolbar dropdowns ───
  const dropdown = (e: React.MouseEvent, items: MenuItem[]) => {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 2, items });
  };
  const k = isMac ? { reload: "⌘r", cancel: "⌘F2", add: "⌘n", del: "⌘⌫", revert: "⌥⌘z", submit: "⌘↩", find: "⌘f" } : { reload: "Ctrl+r", cancel: "Ctrl+F2", add: "Ctrl+n", del: "Ctrl+Backspace", revert: "Ctrl+Alt+z", submit: "Ctrl+Enter", find: "Ctrl+f" };

  const rowsLabel = `${baseCount}${result.truncated ? "+" : ""} row${baseCount === 1 ? "" : "s"}`;

  return (
    <div className="grid-wrap">
      {/* ── DataGrip grid toolbar ── */}
      <Toolbar onMouseDown={(e) => e.stopPropagation()}>
        <IconButton title={`Reload Page (${k.reload})`} onClick={p.onReload} disabled={p.running}><IcReload /></IconButton>
        <IconButton active={!!interval}
          title="Update Interval"
          onClick={(e) =>
            dropdown(e, [
              ...[0, 5, 10, 30, 60].map((s) => ({ label: (interval === s ? "✓ " : "　") + (s ? `${s} sec` : "Off"), action: () => setIntervalSec(s) })),
            ])
          }
        >
          <IcClock />
        </IconButton>
        <IconButton tone="danger" title={`Cancel Running Statements (${k.cancel})`} onClick={p.onCancel} disabled={!p.running}><IcStop /></IconButton>
        <ToolbarSep />
        <IconButton title={`Add Row (${k.add})`} onClick={addRow} disabled={!canEdit}><IcPlus /></IconButton>
        <IconButton title={`Delete Row (${k.del})`} onClick={deleteRows} disabled={!canEdit || !rect}><IcMinus /></IconButton>
        <IconButton title={`Revert Selected (${k.revert})`} onClick={revertSelected} disabled={!pendingCount}><IcRevert /></IconButton>
        <IconButton title="Preview Pending Changes" onClick={() => setPreview({ sql: buildStatements(), readOnly: true })} disabled={!pendingCount}><IcPreview /></IconButton>
        <IconButton active={false} className={pendingCount ? "accent" : undefined} title={`Submit (${k.submit})`} onClick={submit} disabled={!pendingCount}><IcSubmit /></IconButton>
        <ToolbarSep />
        <ToolButton
          title="Transaction mode"
          onClick={(e) =>
            dropdown(e, [
              { label: "✓ Auto", action: () => {} },
              { label: "　Manual (not supported yet)", action: () => {}, disabled: true },
            ])
          }
        >
          Tx: Auto ▾
        </ToolButton>
        {p.onDdl && <ToolButton title="Go to DDL" onClick={p.onDdl}>DDL</ToolButton>}
        <IconButton active={findOpen}
          title={`Find on Current Page (${k.find})`}
          onClick={() => {
            setFindOpen((f) => !f);
            setTimeout(() => document.querySelector<HTMLInputElement>(".grid-find input")?.focus());
          }}
        >
          <IcSearch />
        </IconButton>
        <span className="tb-info">{rowsLabel}{pendingCount ? ` · ${pendingCount} pending` : ""}</span>
        <Spacer />
        <ToolButton title="Data Extractor (copy format)" onClick={(e) => dropdown(e, EXTRACTORS.map((x) => ({ label: (x === extractor ? "✓ " : "　") + x, action: () => setExtractor(x) })))}>
          {extractor} ▾
        </ToolButton>
        <IconButton title="Export Data…" onClick={p.onExport}><IcExport /></IconButton>
        <IconButton active={transposed || chart}
          title="View as"
          onClick={(e) =>
            dropdown(e, [
              { label: (!transposed && !chart ? "✓ " : "　") + "Table", action: () => (setTransposed(false), setChart(false)) },
              { label: (transposed ? "✓ " : "　") + "Transpose", action: () => (setTransposed(true), setChart(false)) },
              { label: (chart ? "✓ " : "　") + "Chart", action: () => (setChart(true), setTransposed(false)) },
            ])
          }
        >
          {chart ? <IcChart /> : transposed ? <IcTranspose /> : <IcView />}
        </IconButton>
        <IconButton
          title="Show Options Menu"
          onClick={(e) =>
            dropdown(e, [
              ...(p.filterBar ? ([{ label: (showFilter ? "✓ " : "　") + "Show Filter", action: () => setShowFilter((v) => !v) }] as MenuItem[]) : []),
              ...(p.onServerSort ? ([{ label: (sortViaOrderBy ? "✓ " : "　") + "Sort via ORDER BY", action: () => { setSorts([]); setSortViaOrderBy((v) => !v); } }] as MenuItem[]) : []),
              { label: "Reset View", action: () => { setSorts([]); setTransposed(false); setChart(false); if (serverSort) p.onServerSort!(""); } },
              { separator: true },
              { label: "View Query", action: () => setShowQuery(true), disabled: !p.query },
              ...(p.onCopyQueryToConsole ? ([{ label: "Copy Query to Console", action: () => p.query && p.onCopyQueryToConsole!(p.query), disabled: !p.query }] as MenuItem[]) : []),
              { label: "Copy Query to Clipboard", action: () => p.query && navigator.clipboard.writeText(p.query), disabled: !p.query },
            ])
          }
        >
          <IcGear />
        </IconButton>
      </Toolbar>
      {p.filterBar && showFilter && p.filterBar}
      {findOpen && (
        <div className="grid-find">
          <SearchField
            value={findText}
            placeholder="Find on current page"
            onChange={(e) => setFindText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") gotoMatch(e.shiftKey ? -1 : 1);
              if (e.key === "Escape") {
                setFindOpen(false);
                scrollRef.current?.focus();
              }
              e.stopPropagation();
            }}
          />
          <Text tone="muted">{findText ? `${matches.length}${matches.length >= 5000 ? "+" : ""} results` : ""}</Text>
          <IconButton title="Previous Occurrence (⇧↩)" onClick={() => gotoMatch(-1)}>↑</IconButton>
          <IconButton title="Next Occurrence (↩)" onClick={() => gotoMatch(1)}>↓</IconButton>
          <IconButton title="Close (⎋)" onClick={() => setFindOpen(false)}>×</IconButton>
        </div>
      )}

      {chart ? (
        <ResultChart result={result} />
      ) : transposed ? (
        <TransposedView cols={cols} rows={Array.from({ length: Math.min(rowCount, 300) }, (_, d) => cols.map((_, c) => valueAt(dataIndex(d), c)))} />
      ) : (
        <div
          className="grid-scroll"
          ref={scrollRef}
          tabIndex={0}
          onKeyDown={onKeyDown}
          onMouseUp={() => (dragging.current = false)}
          onMouseLeave={() => (dragging.current = false)}
        >
          <div style={{ width: total, height: virt.getTotalSize() + HEADER_H, position: "relative" }}>
            <GridHeaderRow width={total} height={HEADER_H}>
              <RowNumberCell
                width={ROWNUM_W}
                title="Select all (⌘a)"
                onMouseDown={() => {
                  setSelA({ r: 0, c: 0 });
                  setSelB({ r: rowCount - 1, c: cols.length - 1 });
                }}
              />
              {spacer(padL)}
              {colItems.map(({ index: i }) => {
                const c = cols[i];
                const idx = sorts.findIndex((x) => x.col === i);
                const key = sorts[idx];
                return (
                  <GridHeaderCell
                    key={i}
                    name={c.name}
                    type={c.typeName}
                    pk={edit?.pk.includes(c.name)}
                    width={widths[i]}
                    colSelected={!!rect && rect.c0 <= i && i <= rect.c1}
                    sort={key ? { dir: key.dir, order: sorts.length > 1 ? idx + 1 : undefined } : undefined}
                    onMouseDown={(e) => {
                      // Header click = select the whole column (DataGrip)
                      if (e.shiftKey && selA) setSelB({ r: rowCount - 1, c: i });
                      else {
                        setSelA({ r: 0, c: i });
                        setSelB({ r: rowCount - 1, c: i });
                      }
                    }}
                    onSortClick={(e) => clickSort(i, e.altKey)}
                  />
                );
              })}
              {spacer(padR)}
            </GridHeaderRow>
            {items.map((vi) => {
              const d = vi.index;
              const r = dataIndex(d);
              const row = rowAt(r);
              if (!row) return null;
              const isNew = r >= baseCount;
              const isDel = deleted.has(r);
              return (
                <GridRow key={vi.key} top={HEADER_H + vi.start} height={ROW_H} width={total} odd={d % 2 === 1} added={isNew} deleted={isDel}>
                  <RowNumberCell
                    width={ROWNUM_W}
                    selected={!!rect && rect.r0 <= d && d <= rect.r1}
                    onMouseDown={(e) => {
                      // Gutter number click = select the row
                      if (e.shiftKey && selA) setSelB({ r: d, c: cols.length - 1 });
                      else {
                        setSelA({ r: d, c: 0 });
                        setSelB({ r: d, c: cols.length - 1 });
                      }
                    }}
                  >
                    {isNew ? "+" : d + 1}
                  </RowNumberCell>
                  {spacer(padL)}
                  {colItems.map(({ index: ci }) => {
                    const val = valueAt(r, ci);
                    const changed = !isNew && changes[r] && ci in changes[r];
                    if (editing && editing.r === r && editing.c === ci) {
                      const temporal = isTemporal(cols[ci].typeName);
                      return (
                        <CellEditor
                          key={ci}
                          width={widths[ci]}
                          value={editing.value}
                          onChange={(v) => setEditing({ ...editing, value: v })}
                          onBlurOut={commitEdit}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === "Tab") {
                              e.preventDefault();
                              commitEdit();
                              if (e.key === "Tab") selectCell({ r: d, c: Math.min(ci + 1, cols.length - 1) }, false);
                            } else if (e.key === "Escape") {
                              setEditing(null);
                              scrollRef.current?.focus();
                            }
                            e.stopPropagation();
                          }}
                          more={temporal ? { title: "Choose date/time", onClick: () => setEditing({ ...editing, picker: !editing.picker }) } : undefined}
                        >
                          {temporal && editing.picker && (
                            <DateTimePicker value={editing.value} typeName={cols[ci].typeName} onChange={(v) => setEditing((ed) => (ed ? { ...ed, value: v } : ed))} />
                          )}
                        </CellEditor>
                      );
                    }
                    return (
                      <GridCell
                        key={ci}
                        width={widths[ci]}
                        kind={val === null || val === DEFAULT_TOKEN ? "null" : typeof val === "number" ? "num" : undefined}
                        selected={inSel(d, ci)}
                        active={selB?.r === d && selB?.c === ci}
                        changed={!!changed}
                        match={isMatch(d, ci)}
                        onMouseDown={(e) => {
                          if (e.button !== 0) return;
                          dragging.current = true;
                          selectCell({ r: d, c: ci }, e.shiftKey);
                        }}
                        onMouseEnter={() => {
                          if (dragging.current) setSelB({ r: d, c: ci });
                        }}
                        onDoubleClick={() => startEdit(r, ci)}
                        onContextMenu={(e) => openCellMenu(e, d, ci)}
                      >
                        {shown(val, ci)}
                      </GridCell>
                    );
                  })}
                  {spacer(padR)}
                </GridRow>
              );
            })}
          </div>
          {loadingMore && <div className="grid-loading">Loading more…</div>}
        </div>
      )}

      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}

      {valueEditor && (
        <ValueEditor
          title={`${cols[valueEditor.c].name} : ${cols[valueEditor.c].typeName.toLowerCase()}`}
          text={valueEditor.text}
          readOnly={!canEdit || deleted.has(valueEditor.r)}
          onApply={(t) => {
            setCell(valueEditor.r, valueEditor.c, t);
            setValueEditor(null);
            scrollRef.current?.focus();
          }}
          onClose={() => {
            setValueEditor(null);
            scrollRef.current?.focus();
          }}
        />
      )}

      {showQuery && p.query && (
        <Modal
          title="Query"
          size="lg"
          onClose={() => setShowQuery(false)}
          footer={
            <>
              <Button onClick={() => navigator.clipboard.writeText(p.query!)}>Copy Query to Clipboard</Button>
              <Spacer />
              <Button variant="primary" onClick={() => setShowQuery(false)}>
                Close
              </Button>
            </>
          }
        >
          <CodeBlock className="sql-preview">{p.query}</CodeBlock>
        </Modal>
      )}

      {preview && (
        <Modal
          title={`${preview.readOnly ? "Pending Changes" : "Submit Changes"} — ${preview.sql.length} statement${preview.sql.length > 1 ? "s" : ""}`}
          size="lg"
          onClose={() => setPreview(null)}
          footer={
            <>
              <Button onClick={() => navigator.clipboard.writeText(preview.sql.join("\n"))}>Copy SQL</Button>
              <Spacer />
              <Button onClick={() => setPreview(null)}>Close</Button>
              <Button variant="primary" onClick={doSubmitPreview}>
                Submit
              </Button>
            </>
          }
        >
          <CodeBlock className="sql-preview">{preview.sql.join("\n")}</CodeBlock>
          {submitErr && <Notice tone="danger">{submitErr}</Notice>}
        </Modal>
      )}
    </div>
  );
}

/** View as | Transpose: swap rows and columns */
function TransposedView({ cols, rows }: { cols: QueryResult["columns"]; rows: Cell[][] }) {
  const fracW = cols.map((c, ci) => (isTimeType(c.typeName) ? fracWidth(rows.map((r) => r[ci])) : 0));
  return (
    <div className="grid-scroll transposed">
      <table>
        <tbody>
          {cols.map((c, ci) => (
            <tr key={ci}>
              <th>
                <span className="col-name">{c.name}</span> <span className="col-type">{c.typeName.toLowerCase()}</span>
              </th>
              {rows.map((r, ri) => (
                <td key={ri} className={r[ci] === null ? "null" : typeof r[ci] === "number" ? "num" : ""}>
                  {fracW[ci] && typeof r[ci] === "string" ? padFrac(r[ci] as string, fracW[ci]) : display(r[ci])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Value editor (⇧↩): ⌘↩ confirm, ⎋ cancel, Toggle Soft-Wrap, Toggle Formatting(JSON) */
function ValueEditor({ title, text, readOnly, onApply, onClose }: { title: string; text: string; readOnly: boolean; onApply: (t: string) => void; onClose: () => void }) {
  const [value, setValue] = useState(text);
  const [wrap, setWrap] = useState(true);
  const format = () => {
    try {
      const j = JSON.parse(value);
      setValue(value.includes("\n") ? JSON.stringify(j) : JSON.stringify(j, null, 2));
    } catch {
      /* Not JSON: leave as is */
    }
  };
  return (
    <div className="value-editor">
      <div className="ve-head">
        <span className="ve-title">{title}</span>
        <Spacer />
        <ToolButton active={wrap} title="Toggle Soft-Wrap" onClick={() => setWrap((w) => !w)}>Wrap</ToolButton>
        <ToolButton title="Toggle Formatting (JSON)" onClick={format}>{"{ }"}</ToolButton>
        <IconButton title="Close (⎋)" onClick={onClose}>×</IconButton>
      </div>
      <TextArea
        autoFocus
        readOnly={readOnly}
        className={"ve-text" + (wrap ? "" : " nowrap")}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          const mod = isMac ? e.metaKey : e.ctrlKey;
          if (mod && e.key === "Enter") {
            e.preventDefault();
            if (!readOnly) onApply(value);
          } else if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          }
          e.stopPropagation();
        }}
      />
      <div className="ve-foot muted">{readOnly ? "Read-only" : `${isMac ? "⌘↩" : "Ctrl+Enter"} to apply · ⎋ to cancel`}</div>
    </div>
  );
}
