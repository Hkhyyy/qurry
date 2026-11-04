import { setStatusWidget } from "../lib/statusbar";
import { fracWidth, isTimeType, padFrac } from "../lib/timeFrac";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { Cell, Dialect, dialectOf, QueryResult } from "../lib/api";
import { DateTimePicker, isTemporal } from "./DateTimePicker";
import { ContextMenu, MenuItem } from "./Explorer";
import { DEFAULT_TOKEN, EXTRACTORS, type Extractor, extractRows, HOLE_TOKEN, raw } from "../lib/extract";
import { sharedReady, useSharedReady } from "../lib/shared";
import { DEFAULTS, useFonts } from "../lib/prefs";
import { alignTable, CELL_KIND, cellClasses, useGridAlign } from "../lib/gridAlign";
import { isJsonType, jsonCell } from "../lib/jsonSpans";
import { contains, copyTable, extend, forEachCell, isSingle, type Pos, selCols, selRows as rowsOf, type Selection, single, toggle } from "../lib/gridSelection";

import { cmd, commandsFor, eventSpec, hit, keyTitle, runKey } from "../lib/shortcuts";
import { draftAt, draftToApply, leftDraft, panelText, registerValuePanel, toggleJsonLayout, type PanelDraft } from "../lib/valuePanel";
import { ResultChart } from "./ResultChart";
import { ValuePanel } from "./ValuePanel";
import { Button, CellEditor, CodeBlock, cx, GridCell, GridHead, GridHeaderCell, GridHeaderRow, GridRow, IconButton, JsonText, Modal, Notice, ProgressBar, RowNumberCell, SearchField, Spacer, Text, ToolButton, Toolbar, ToolbarSep } from "../ui";
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

/** Console results have no table to write to (editing them is a later batch) */
export const CONSOLE_READ_ONLY = "Console results are read-only. Open the table to edit its rows.";

let lastExtractor: Extractor = "CSV"; // The last chosen extractor is the default for new tabs (DataGrip)

interface Props {
  result: QueryResult;
  /** For the table data editor: the edit target */
  edit?: EditTarget;
  /** Read-only data source: editing disabled */
  readOnly?: boolean;
  /** Why rows can't be edited here, on the greyed-out edit buttons (console results: CONSOLE_READ_ONLY) */
  readOnlyReason?: string;
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

function display(c: Cell): string {
  if (c === null) return "<null>";
  if (c === DEFAULT_TOKEN) return "<default>";
  return String(c);
}

/** Cells whose original value is not shown as is (arrays, bytea, etc.) are not editable. */
const editableValue = (v: Cell, typeName: string) => typeName !== "BYTEA" && !(typeof v === "string" && /^<[A-Z0-9_ \[\]]+>$/.test(v));

export function ResultGrid(p: Props) {
  const { result, edit, loadingMore } = p;
  const D = p.dialect ?? dialectOf("postgres");
  const quoteIdent = D.q;
  const sqlLiteral = D.lit;
  const lit = (c: Cell) => (c === DEFAULT_TOKEN ? "DEFAULT" : sqlLiteral(c));
  const cols = result.columns;
  const scrollRef = useRef<HTMLDivElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [sorts, setSorts] = useState<SortKey[]>([]);
  /** Selected ranges in display coordinates (lib/gridSelection.ts) */
  const [sel, setSel] = useState<Selection | null>(null);
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
  /** Value Editor side panel: open, a request to focus it (⇧↩), the edit typed there, why its last apply was refused */
  const [panel, setPanel] = useState(false);
  const [panelFocus, setPanelFocus] = useState(0);
  const [draft, setDraft] = useState<PanelDraft | null>(null);
  const [panelError, setPanelError] = useState<string | null>(null);
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
    setSel(null);
    setEditing(null);
    setChanges({});
    setDeleted(new Set());
    setAdded([]);
    setDraft(null);
    setPanelError(null);
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

  // Character widths below are for the default grid font size (12px)
  const em = useFonts().grid.size / DEFAULTS.grid.size;
  const rownumW = Math.max(ROWNUM_W, Math.round(ROWNUM_W * em));
  const widths = useMemo(
    () =>
      cols.map((col, ci) => {
        const head = col.name.length * 7.6 * em + col.typeName.length * 6.2 + 48 + (edit?.pk.includes(col.name) ? 16 : 0);
        let len = 0;
        for (const row of result.rows.slice(0, 200)) len = Math.max(len, display(row[ci]).length);
        return Math.min(Math.max(head, len * 7.3 * em + 20, 64), 420 * em);
      }),
    [cols, firstRow, edit, em],
  );
  const total = rownumW + widths.reduce((a, b) => a + b, 0);
  // Time columns: align fractional-second digits within the column (values unchanged)
  const wasmReady = useSharedReady();
  const fracW = useMemo(() => cols.map((c, ci) => (isTimeType(c.typeName) ? fracWidth(result.rows.map((r) => r[ci])) : 0)), [cols, result.rows, wasmReady]);
  const shown = (v: Cell, ci: number) => (fracW[ci] && typeof v === "string" ? padFrac(v, fracW[ci]) : display(v));
  // Value class of each cell and where it sits (Settings › Appearance › Alignment; the TUI grid's rule)
  const classOf = useMemo(() => cols.map((c) => cellClasses(c.typeName)), [cols, wasmReady]);
  const alignOverrides = useGridAlign();
  const aligns = useMemo(() => cols.map((c, ci) => alignTable(alignOverrides, c.typeName, result.rows, ci)), [cols, result.rows, alignOverrides, wasmReady]);
  // JSON columns color their text cells (lib/jsonSpans.ts, the TUI grid's tokenizer)
  const jsonCol = useMemo(() => cols.map((c) => isJsonType(c.typeName)), [cols, wasmReady]);

  const virt = useVirtualizer({ count: rowCount, getScrollElement: () => scrollRef.current, estimateSize: () => ROW_H, overscan: 20 });
  // Horizontally, render only visible columns too (tables with many columns). Blank cells fill the width on both sides
  const colVirt = useVirtualizer({ horizontal: true, count: cols.length, getScrollElement: () => scrollRef.current, estimateSize: (i) => widths[i] ?? 64, paddingStart: rownumW, overscan: 4 });
  useEffect(() => colVirt.measure(), [widths]);
  const colItems = colVirt.getVirtualItems();
  const padL = colItems.length ? colItems[0].start - rownumW : 0;
  const padR = colItems.length ? colVirt.getTotalSize() - colItems[colItems.length - 1].end : 0;
  const spacer = (w: number) => (w > 0 ? <div style={{ flex: "none", width: w }} /> : null);
  const items = virt.getVirtualItems();
  const lastItem = items[items.length - 1];
  useEffect(() => {
    // After a failed load the reason stays in the grid: no retry on every scroll (running the query again starts over)
    if (lastItem && result.truncated && !result.moreError && !loadingMore && !p.running && lastItem.index >= baseCount - 30) p.onFetchMore();
  }, [lastItem?.index, result.truncated, result.moreError, loadingMore, p.running, baseCount]);

  // ─── Selection ───
  const cur = sel?.cur ?? null; // the last clicked cell: the active one, the Value Editor's
  const hasSel = !!sel?.ranges.length;
  const inSel = (d: number, c: number) => contains(sel, d, c);
  const active = cur ? { r: dataIndex(cur.r), c: cur.c } : null; // data coordinates
  const selRowSet = useMemo(() => new Set(rowsOf(sel)), [sel]);
  const selColSet = useMemo(() => new Set(selCols(sel)), [sel]);
  /** Data rows of every selected range, in grid order (the row-level edits: Delete Row, Revert Selected) */
  const selRows = (): number[] => rowsOf(sel).map(dataIndex);
  /** Every selected cell once, in data coordinates: the one path the cell edits (Set NULL, Revert Selected, …) take */
  const forSelected = (fn: (r: number, c: number) => void | false) => forEachCell(sel, (d, c) => fn(dataIndex(d), c));
  const selectCell = (pos: Pos, ext: boolean) => setSel((s) => (ext ? extend(s, pos) : single(pos)));
  /** A mouse click: ⇧ extends the last range to b, ⌘ adds a..b (a cell, a row, a column) or takes it out, a plain click starts over (DataGrip) */
  const clickSel = (e: React.MouseEvent, a: Pos, b: Pos = a) => setSel((s) => (e.shiftKey ? extend(s, b) : (isMac ? e.metaKey : e.ctrlKey) ? toggle(s, a, b) : single(a, b)));

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
    const at = cur ? matches.findIndex((m) => m.r === cur.r && m.c === cur.c) : -1;
    const m = matches[(at + dir + matches.length) % matches.length];
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
  // ─── Value Editor panel (follows the current cell) ───
  const panelCell = active && active.r < rowCount ? active : null;
  const panelReadOnly = (r: number, c: number) => !canEdit || deleted.has(r) || (r < baseCount && !editableValue(valueAt(r, c), cols[c].typeName));
  const panelShown = panelCell ? panelText(cols[panelCell.c].typeName, raw(valueAt(panelCell.r, panelCell.c))) : { json: false, text: "" };
  /** Write a draft to its cell; the reason when a json / jsonb column refuses it */
  const writeDraft = (d: PanelDraft): string | null => {
    const res = draftToApply(d, cols[d.c].typeName, raw(valueAt(d.r, d.c)));
    if (res && "error" in res) return res.error;
    if (res) setCell(d.r, d.c, res.text);
    return null;
  };
  // The selection left the drafted cell: apply the edit there (DataGrip applies on leaving); one that can't be applied is dropped, saying why
  useEffect(() => {
    const d = leftDraft(draft, panelCell);
    if (!d) return setPanelError(null);
    const err = writeDraft(d);
    setDraft(null);
    setPanelError(err && `${cols[d.c].name} not changed: ${err}`);
  }, [panelCell?.r, panelCell?.c]);
  const focusGrid = () => (scrollRef.current ?? wrapRef.current)?.focus();
  const openPanel = () => {
    setPanel(true);
    setPanelFocus((n) => n + 1);
  };
  const applyPanel = () => {
    const err = draft ? writeDraft(draft) : null;
    setPanelError(err);
    if (err) return;
    setDraft(null);
    focusGrid();
  };
  /** × / ⌘w / ⇧⎋: apply the edit, close, back to the grid on the same cell. A refused edit keeps the panel open with the reason */
  const closePanel = () => {
    const err = draft ? writeDraft(draft) : null;
    setPanelError(err);
    if (err) return;
    setDraft(null);
    setPanel(false);
    focusGrid();
  };
  const closePanelRef = useRef(closePanel);
  closePanelRef.current = closePanel;
  useEffect(() => (panel && wrapRef.current ? registerValuePanel(wrapRef.current, () => closePanelRef.current()) : undefined), [panel]);

  // Status bar Aggregator: Count / Sum / Avg / Min / Max when 2+ cells are selected
  useEffect(() => {
    if (!hasSel || isSingle(sel)) return setStatusWidget("aggregate", undefined);
    let count = 0, nums = 0, sum = 0, min = Infinity, max = -Infinity;
    forSelected((r, c) => {
      if (count >= 200000) return false;
      const v = valueAt(r, c);
      if (v === null) return;
      count++;
      const n = typeof v === "number" ? v : typeof v === "string" && /^-?\d+(\.\d+)?(e[+-]?\d+)?$/i.test(v.trim()) ? Number(v) : NaN;
      if (!Number.isNaN(n)) {
        nums++;
        sum += n;
        min = Math.min(min, n);
        max = Math.max(max, n);
      }
    });
    const f = (n: number) => (Number.isInteger(n) ? n.toLocaleString() : n.toLocaleString(undefined, { maximumFractionDigits: 4 }));
    setStatusWidget("aggregate", nums ? `Count: ${count}  Sum: ${f(sum)}  Avg: ${f(sum / nums)}  Min: ${f(min)}  Max: ${f(max)}` : `Count: ${count}`);
  }, [sel, result, changes]);
  useEffect(() => () => setStatusWidget("aggregate", undefined), []);

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
    if (!canEdit || !hasSel) return;
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
    if (!hasSel) {
      setChanges({});
      setDeleted(new Set());
      setAdded([]);
      return;
    }
    const rows = selRows();
    setChanges((ch) => {
      const n = { ...ch };
      forSelected((r, c) => {
        if (!n[r] || !(c in n[r])) return;
        const row = { ...n[r] };
        delete row[c];
        if (Object.keys(row).length) n[r] = row;
        else delete n[r];
      });
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
  /** The cells of `s` in a format: rows and columns in grid order, the union of the ranges' columns (gridSelection.copyTable) */
  const extract = (s: Selection, fmt: Extractor, header = true): string => {
    const t = copyTable<Cell>(s, (d, c) => valueAt(dataIndex(d), c), HOLE_TOKEN);
    const rows = t.rows.map(dataIndex);
    // Key values are read only for SQL Updates (as before: other formats never touch the key columns)
    const keys = fmt === "SQL Updates" ? rows.map((r) => pkIdx.map((k): [string, Cell] => [cols[k].name, valueAt(r, k)])) : [];
    return extractRows(fmt, { names: t.cols.map((c) => cols[c].name), types: t.cols.map((c) => cols[c].typeName), vals: t.vals, keys, tableRef, dialect: D, header });
  };
  /** The shared WASM writes the formats: without it (a WebView that refused WebAssembly) say why nothing is copied */
  const formatted = (make: () => string): string | undefined => {
    if (sharedReady()) return make();
    setStatusWidget("aggregate", "Copy formats need WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk");
  };
  const copySelection = (header = false) => {
    if (!sel || !hasSel) return;
    // The one selected cell (not sel.cur: a ⌘-click that took a cell out leaves the cursor outside the selection)
    const one = sel.ranges[0];
    const text = isSingle(sel) && !header && (extractor === "CSV" || extractor === "TSV") ? raw(valueAt(dataIndex(one.r0), one.c0)) : formatted(() => extract(sel, extractor, header));
    if (text !== undefined) navigator.clipboard.writeText(text);
  };
  /** Selected cells as ('a', 'b') (paste straight into WHERE x IN). If only one cell is selected, its whole column */
  const copyInList = () => {
    if (!sel || !hasSel) return;
    const c = sel.ranges[0].c0;
    const text = formatted(() => extract(isSingle(sel) ? single({ r: 0, c }, { r: rowCount - 1, c }) : sel, "SQL IN List"));
    if (text !== undefined) navigator.clipboard.writeText(text);
  };
  /** Paste: split multi-line TSV/CSV into cells, adding new rows if there are not enough (DataGrip) */
  const paste = async () => {
    if (!canEdit || !cur) return;
    // Paste goes into one place (DataGrip); a ⌘-click meant as a plain click is the usual way to get several ranges here
    if ((sel?.ranges.length ?? 0) > 1) return setStatusWidget("aggregate", "Paste needs one selected range: click a cell to select just it (⌘-click adds cells to the selection)");
    const text = await navigator.clipboard.readText();
    const lines = text.replace(/\r/g, "").split("\n").filter((l, i, a) => l !== "" || i < a.length - 1);
    const grid = lines.map((l) => l.split(l.includes("\t") ? "\t" : ","));
    grid.forEach((cells, i) => {
      const d = cur.r + i;
      const r = d < rowCount ? dataIndex(d) : -1;
      if (r < 0) {
        const row: Cell[] = cols.map(() => null);
        cells.forEach((v, j) => {
          if (cur.c + j < cols.length) row[cur.c + j] = v;
        });
        setAdded((a) => [...a, row]);
      } else cells.forEach((v, j) => cur.c + j < cols.length && setCell(r, cur.c + j, v));
    });
  };

  // ─── Keyboard (DataGrip grid keymap) ───
  const onKeyDown = (e: React.KeyboardEvent) => {
    // On .grid-wrap, so the keys work in Chart and Transpose too; the find bar's and filter fields' own typing stays theirs
    if (editing || (e.target as HTMLElement).closest("input, textarea, select")) return;
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const k = e.key.toLowerCase();
    const stop = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    // Chart and Transpose have no cell selection: only the grid commands below
    const cells = !chart && !transposed;
    if (cells && mod && k === "a") {
      setSel(single({ r: 0, c: 0 }, { r: rowCount - 1, c: cols.length - 1 }));
      return stop();
    }
    if (cells && mod && k === "c" && !e.shiftKey) return copySelection(false), stop();
    if (cells && mod && k === "v") return paste(), stop();
    // Grid commands (Settings › Keymap › Data Grid): what the toolbar, its menus and the cell menu show. While running, the global
    // shortcut (editor.cancel) already cancelled when it has the same key (so it is not called twice)
    const spec = eventSpec(e.nativeEvent);
    if (e.defaultPrevented && hit(spec, "grid.cancel")) return stop();
    // A grid command's key is the grid's even when the command is disabled now (⌘r while running must not reload the WebView)
    if (commandsFor(spec, "grid").length) return runKey(spec, "grid", () => [...toolbarCommands(), ...(cur && cells ? cellItems(cur.r, cur.c) : [])]), stop();
    if (!cur || !cells) return;
    if ((e.key === "Enter" || e.key === "F2") && canEdit && active) return startEdit(active.r, active.c), stop();
    if (e.key === "Escape") {
      setFindOpen(false);
      return;
    }
    const move: Record<string, [number, number]> = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1], Tab: [0, e.shiftKey ? -1 : 1] };
    const m = move[e.key];
    if (m) {
      const r = Math.min(Math.max(cur.r + m[0], 0), rowCount - 1);
      const c = Math.min(Math.max(cur.c + m[1], 0), cols.length - 1);
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
  /** The cell menu of display row d, column c. Grid command keys run its entries for the selected cell */
  const cellItems = (d: number, c: number): MenuItem[] => {
    const r = dataIndex(d);
    const v = valueAt(r, c);
    const k = isMac ? { copy: "⌘c", paste: "⌘v" } : { copy: "Ctrl+c", paste: "Ctrl+v" };
    const name = quoteIdent(cols[c].name);
    // Row values at the moment the menu opened (the chosen row stays even if auto refresh reorders rows — so the wrong session is not killed)
    const rowObj = Object.fromEntries(cols.map((col, i) => [col.name, valueAt(r, i)]));
    const items: MenuItem[] = [
      ...(p.rowActions?.length ? ([...p.rowActions.map((a) => ({ label: a.label, danger: a.danger, action: () => a.run(rowObj) })), { separator: true }] as MenuItem[]) : []),
      ...(canEdit ? ([{ label: "Edit", shortcut: "↩", action: () => startEdit(r, c) }] as MenuItem[]) : []),
      { label: "Open in Value Editor", ...cmd("grid.valueEditor"), action: openPanel },
      ...(canEdit
        ? ([
            { label: "Revert Selected", ...cmd("grid.revert"), action: revertSelected },
            { separator: true },
            { label: "Set DEFAULT", ...cmd("grid.setDefault"), action: setDefault },
            { label: "NULL", ...cmd("grid.setNull"), action: setNull },
            { label: "Generate UUID", ...cmd("grid.genUuid"), action: genUuid },
          ] as MenuItem[])
        : []),
      { separator: true },
      { label: "Copy", shortcut: k.copy, action: () => copySelection(false) },
      { label: "Copy with Headers", ...cmd("grid.copyHeaders"), action: () => copySelection(true), disabled: extractor !== "CSV" && extractor !== "TSV" },
      { label: "Copy as IN List  ('a', 'b', …)", ...cmd("grid.copyInList"), action: copyInList },
      { label: "Change Data Extractor", submenu: EXTRACTORS.map((x) => ({ label: (x === extractor ? "✓ " : "　") + x, action: () => setExtractor(x) })) },
      ...(canEdit ? ([{ label: "Paste", shortcut: k.paste, action: paste }] as MenuItem[]) : []),
      ...(canEdit
        ? ([
            { separator: true },
            { label: "Add Row", ...cmd("grid.addRow"), action: addRow },
            { label: "Delete Row", ...cmd("grid.deleteRows"), action: deleteRows },
            { label: "Clone Row", ...cmd("grid.cloneRow"), action: cloneRow },
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
      { label: "Export Data…", ...cmd("grid.export"), action: p.onExport },
      ...(p.onDdl ? ([{ label: "Go To DDL", ...cmd("grid.ddl"), action: p.onDdl }] as MenuItem[]) : []),
    ];
    return items;
  };
  const openCellMenu = (e: React.MouseEvent, d: number, c: number) => {
    e.preventDefault();
    if (!inSel(d, c)) selectCell({ r: d, c }, false);
    setMenu({ x: e.clientX, y: e.clientY, items: cellItems(d, c) });
  };

  // ─── Toolbar dropdowns ───
  const dropdown = (e: React.MouseEvent, items: MenuItem[]) => {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 2, items });
  };
  /** The find bar, opened and focused (its key, DataGrip); the toolbar button toggles it */
  const openFind = () => {
    setFindOpen(true);
    setTimeout(() => wrapRef.current?.querySelector<HTMLInputElement>(".grid-find input")?.focus());
  };
  const viewItems = (): MenuItem[] => [
    { label: (!transposed && !chart ? "✓ " : "　") + "Table", ...cmd("grid.viewTable"), action: () => (setTransposed(false), setChart(false)) },
    { label: (transposed ? "✓ " : "　") + "Transpose", ...cmd("grid.viewTranspose"), action: () => (setTransposed(true), setChart(false)) },
    { label: (chart ? "✓ " : "　") + "Chart", ...cmd("grid.viewChart"), action: () => (setChart(true), setTransposed(false)) },
  ];
  const optionItems = (): MenuItem[] => [
    ...(p.filterBar ? ([{ label: (showFilter ? "✓ " : "　") + "Show Filter", ...cmd("grid.showFilter"), action: () => setShowFilter((v) => !v) }] as MenuItem[]) : []),
    ...(p.onServerSort ? ([{ label: (sortViaOrderBy ? "✓ " : "　") + "Sort via ORDER BY", ...cmd("grid.sortOrderBy"), action: () => { setSorts([]); setSortViaOrderBy((v) => !v); } }] as MenuItem[]) : []),
    { label: "Reset View", ...cmd("grid.resetView"), action: () => { setSorts([]); setTransposed(false); setChart(false); if (serverSort) p.onServerSort!(""); } },
    { separator: true },
    { label: "View Query", ...cmd("grid.viewQuery"), action: () => setShowQuery(true), disabled: !p.query },
    ...(p.onCopyQueryToConsole ? ([{ label: "Copy Query to Console", ...cmd("grid.queryToConsole"), action: () => p.query && p.onCopyQueryToConsole!(p.query), disabled: !p.query }] as MenuItem[]) : []),
    { label: "Copy Query to Clipboard", ...cmd("grid.copyQuery"), action: () => p.query && navigator.clipboard.writeText(p.query), disabled: !p.query },
  ];
  /** What the toolbar's buttons and menus run, for their command keys (the buttons are drawn below with the same conditions) */
  const toolbarCommands = (): MenuItem[] => [
    { label: "Reload Page", ...cmd("grid.reload"), action: p.onReload, disabled: p.running },
    { label: "Cancel Running Statements", ...cmd("grid.cancel"), action: p.onCancel, disabled: !p.running },
    { label: "Add Row", ...cmd("grid.addRow"), action: addRow, disabled: !canEdit },
    { label: "Delete Row", ...cmd("grid.deleteRows"), action: deleteRows, disabled: !canEdit || !hasSel },
    { label: "Revert Selected", ...cmd("grid.revert"), action: revertSelected, disabled: !pendingCount },
    { label: "Preview Pending Changes", ...cmd("grid.preview"), action: () => setPreview({ sql: buildStatements(), readOnly: true }), disabled: !pendingCount },
    { label: "Submit", ...cmd("grid.submit"), action: submit, disabled: !pendingCount },
    ...(p.onDdl ? ([{ label: "Go to DDL", ...cmd("grid.ddl"), action: p.onDdl }] as MenuItem[]) : []),
    { label: "Find on Current Page", ...cmd("grid.find"), action: openFind },
    { label: "Export Data…", ...cmd("grid.export"), action: p.onExport },
    ...viewItems(),
    ...optionItems(),
  ];

  /** An edit button's tooltip and description: the command, and why it's greyed out when the grid says so */
  const editButton = (title: string, id: string) => ({
    title: p.readOnlyReason ? `${keyTitle(title, id)} — ${p.readOnlyReason}` : keyTitle(title, id),
    "aria-description": p.readOnlyReason,
  });
  const rowsLabel = `${baseCount}${result.truncated ? "+" : ""} row${baseCount === 1 ? "" : "s"}`;

  return (
    <div className="grid-wrap" ref={wrapRef} tabIndex={-1} onKeyDown={onKeyDown}>
      {/* ── DataGrip grid toolbar ── */}
      <Toolbar onMouseDown={(e) => e.stopPropagation()}>
        <IconButton title={keyTitle("Reload Page", "grid.reload")} onClick={p.onReload} disabled={p.running}><IcReload /></IconButton>
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
        <IconButton tone="danger" title={keyTitle("Cancel Running Statements", "grid.cancel")} onClick={p.onCancel} disabled={!p.running}><IcStop /></IconButton>
        <ToolbarSep />
        <IconButton {...editButton("Add Row", "grid.addRow")} onClick={addRow} disabled={!canEdit}><IcPlus /></IconButton>
        <IconButton {...editButton("Delete Row", "grid.deleteRows")} onClick={deleteRows} disabled={!canEdit || !hasSel}><IcMinus /></IconButton>
        <IconButton {...editButton("Revert Selected", "grid.revert")} onClick={revertSelected} disabled={!pendingCount}><IcRevert /></IconButton>
        <IconButton {...editButton("Preview Pending Changes", "grid.preview")} onClick={() => setPreview({ sql: buildStatements(), readOnly: true })} disabled={!pendingCount}><IcPreview /></IconButton>
        <IconButton active={false} className={pendingCount ? "accent" : undefined} {...editButton("Submit", "grid.submit")} onClick={submit} disabled={!pendingCount}><IcSubmit /></IconButton>
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
        {p.onDdl && <ToolButton title={keyTitle("Go to DDL", "grid.ddl")} onClick={p.onDdl}>DDL</ToolButton>}
        <IconButton active={findOpen}
          title={keyTitle("Find on Current Page", "grid.find")}
          onClick={() => (findOpen ? setFindOpen(false) : openFind())}
        >
          <IcSearch />
        </IconButton>
        <span className="tb-info">{rowsLabel}{pendingCount ? ` · ${pendingCount} pending` : ""}</span>
        <Spacer />
        <ToolButton title="Data Extractor (copy format)" onClick={(e) => dropdown(e, EXTRACTORS.map((x) => ({ label: (x === extractor ? "✓ " : "　") + x, action: () => setExtractor(x) })))}>
          {extractor} ▾
        </ToolButton>
        <IconButton title={keyTitle("Export Data…", "grid.export")} onClick={p.onExport}><IcExport /></IconButton>
        <IconButton active={transposed || chart}
          title="View as"
          onClick={(e) => dropdown(e, viewItems())}
        >
          {chart ? <IcChart /> : transposed ? <IcTranspose /> : <IcView />}
        </IconButton>
        <IconButton
          title="Show Options Menu"
          onClick={(e) => dropdown(e, optionItems())}
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

      {/* The grid and, to its right, the Value Editor panel (DataGrip): the grid narrows, the splitter's place is remembered */}
      <PanelGroup direction="horizontal" autoSaveId="dbtool-value-panel" className="grid-split">
        <Panel id="grid" order={1} minSize={25}>
          <div className="grid-body">
            {/* Column header above the scroller (not sticky inside it), so it can't drift off the toolbar while rows scroll */}
            {!chart && !transposed && (
              <GridHead body={scrollRef}>
                <GridHeaderRow width={total} height={HEADER_H}>
                  <RowNumberCell
                    width={rownumW}
                    title="Select all (⌘a)"
                    onMouseDown={() => setSel(single({ r: 0, c: 0 }, { r: rowCount - 1, c: cols.length - 1 }))}
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
                        colSelected={selColSet.has(i)}
                        sort={key ? { dir: key.dir, order: sorts.length > 1 ? idx + 1 : undefined } : undefined}
                        // Header click = select the whole column (DataGrip)
                        onMouseDown={(e) => clickSel(e, { r: 0, c: i }, { r: rowCount - 1, c: i })}
                        onSortClick={(e) => clickSort(i, e.altKey)}
                      />
                    );
                  })}
                  {spacer(padR)}
                </GridHeaderRow>
              </GridHead>
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
                onMouseUp={() => (dragging.current = false)}
                onMouseLeave={() => (dragging.current = false)}
              >
                <div style={{ width: total, height: virt.getTotalSize(), position: "relative" }}>
                  {items.map((vi) => {
                    const d = vi.index;
                    const r = dataIndex(d);
                    const row = rowAt(r);
                    if (!row) return null;
                    const isNew = r >= baseCount;
                    const isDel = deleted.has(r);
                    return (
                      <GridRow key={vi.key} top={vi.start} height={ROW_H} width={total} odd={d % 2 === 1} added={isNew} deleted={isDel}>
                        <RowNumberCell
                          width={rownumW}
                          selected={selRowSet.has(d)}
                          // Gutter number click = select the row
                          onMouseDown={(e) => clickSel(e, { r: d, c: 0 }, { r: d, c: cols.length - 1 })}
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
                          const cls = classOf[ci](val);
                          return (
                            <GridCell
                              key={ci}
                              width={widths[ci]}
                              kind={CELL_KIND[cls]}
                              align={aligns[ci][cls]}
                              selected={inSel(d, ci)}
                              active={cur?.r === d && cur?.c === ci}
                              changed={!!changed}
                              match={isMatch(d, ci)}
                              onMouseDown={(e) => {
                                if (e.button !== 0) return;
                                dragging.current = true;
                                clickSel(e, { r: d, c: ci });
                              }}
                              onMouseEnter={() => {
                                if (dragging.current) selectCell({ r: d, c: ci }, true);
                              }}
                              onDoubleClick={() => startEdit(r, ci)}
                              onContextMenu={(e) => openCellMenu(e, d, ci)}
                            >
                              {cls === "jsonBinary" && jsonCol[ci] ? <JsonText {...jsonCell(String(val))} /> : shown(val, ci)}
                            </GridCell>
                          );
                        })}
                        {spacer(padR)}
                      </GridRow>
                    );
                  })}
                </div>
                {/* One row's room under the rows while more can come, so the progress bar's coming and going moves nothing */}
                {result.truncated && !result.moreError && (
                  <div className={cx("grid-more-progress", loadingMore && "on")} style={{ height: ROW_H }}>
                    {loadingMore && <ProgressBar pct={null} width={140} label="Loading more rows" />}
                  </div>
                )}
                {!loadingMore && result.moreError && (
                  <div className="grid-loading grid-more-error" role="alert">
                    {result.moreError}
                  </div>
                )}
              </div>
            )}
          </div>
        </Panel>
        {panel && !chart && (
          <>
            <PanelResizeHandle className="resize-h" />
            <Panel id="value" order={2} defaultSize={35} minSize={15}>
              <ValuePanel
                title={panelCell ? `${cols[panelCell.c].name} : ${cols[panelCell.c].typeName.toLowerCase()}` : undefined}
                text={draftAt(draft, panelCell) ? draft.text : panelShown.text}
                json={panelShown.json}
                isNull={!!panelCell && valueAt(panelCell.r, panelCell.c) === null}
                readOnly={!panelCell || panelReadOnly(panelCell.r, panelCell.c)}
                error={panelError}
                focusSeq={panelFocus}
                onChange={(text) => {
                  if (!panelCell) return;
                  setPanelError(null);
                  setDraft({ ...panelCell, text, base: panelShown.text, editable: !panelReadOnly(panelCell.r, panelCell.c) });
                }}
                onToggleLayout={() => {
                  if (!panelCell) return;
                  const text = toggleJsonLayout(draftAt(draft, panelCell) ? draft.text : panelShown.text);
                  setDraft({ ...panelCell, text, base: panelShown.text, editable: !panelReadOnly(panelCell.r, panelCell.c) });
                }}
                onApply={applyPanel}
                onBack={focusGrid}
                onClose={closePanel}
              />
            </Panel>
          </>
        )}
      </PanelGroup>

      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}

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
  useSharedReady(); // time fractions and value classes come from the WASM module: render again once it has loaded
  const fracW = cols.map((c, ci) => (isTimeType(c.typeName) ? fracWidth(rows.map((r) => r[ci])) : 0));
  const overrides = useGridAlign();
  return (
    <div className="grid-scroll transposed">
      <table>
        <tbody>
          {cols.map((c, ci) => {
            const classOf = cellClasses(c.typeName);
            const aligns = alignTable(overrides, c.typeName, rows, ci);
            return (
              <tr key={ci}>
                <th>
                  <span className="col-name">{c.name}</span> <span className="col-type">{c.typeName.toLowerCase()}</span>
                </th>
                {rows.map((r, ri) => {
                  const cls = classOf(r[ci]);
                  return (
                    <td key={ri} className={cx(CELL_KIND[cls], aligns[cls] && `al-${aligns[cls]}`)}>
                      {fracW[ci] && typeof r[ci] === "string" ? padFrac(r[ci] as string, fracW[ci]) : display(r[ci])}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
