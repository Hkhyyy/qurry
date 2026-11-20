import { setStatusNote } from "../lib/statusbar";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";
import { Cell, Dialect, dialectOf, type JoinEdit, QueryResult } from "../lib/api";
import { DateTimePicker, isTemporal } from "./DateTimePicker";
import { type NewRow } from "../lib/gridEdit";
import { ContextMenu, MenuItem } from "./Explorer";
import { EXTRACTORS, type Extractor, raw } from "../lib/extract";
import { sharedOr } from "../lib/shared";
import { DEFAULTS, useFonts } from "../lib/prefs";
import { CELL_KIND } from "../lib/gridAlign";
import { type WidthsOf } from "../lib/columnWidths";
import { jsonCell } from "../lib/jsonSpans";
import { contains, extend, forEachCell, type Pos, remapCols, selCols, selRows as rowsOf, type Selection, single, toggle } from "../lib/gridSelection";

import { cmd, commandsFor, eventSpec, hit, keyTitle, runKey } from "../lib/shortcuts";
import { draftAt, toggleJsonLayout, type PanelDraft } from "../lib/valuePanel";
import { ResultChart } from "./ResultChart";
import { SqlBlock } from "./SqlBlock";
import { ValuePanel } from "./ValuePanel";
import { Button, CellEditor, CheckListPopover, FixSql, GridCell, GridHead, GridHeaderCell, GridHeaderRow, GridRow, IconButton, JsonText, Modal, Notice, RowNumberCell, SearchField, Spacer, Text, ToolButton, Toolbar, ToolbarSep } from "../ui";
import {
  IcChart, IcClock, IcExport, IcGear, IcMinus, IcPlus, IcPreview, IcReload, IcRevert, IcSearch, IcStop, IcSubmit, IcTranspose, IcView,
} from "../ui/icons";
import { cellKey, cutLength, cutSet, cutTitle, keyColumns, keyed, type SortKey } from "../features/grid/shared";
import { TransposedView } from "./TransposedView";
import { useColumnLayout } from "../features/grid/useColumnLayout";
import { useGridClipboard } from "../features/grid/useGridClipboard";
import { useGridEdits } from "../features/grid/useGridEdits";
import { useGridFind } from "../features/grid/useGridFind";
import { useGridStatus } from "../features/grid/useGridStatus";
import { useGridWidths } from "../features/grid/useGridWidths";
import { useSortOrder } from "../features/grid/useSortOrder";
import { useValuePanel } from "../features/grid/useValuePanel";

const ROW_H = 24;
const HEADER_H = 26;
const ROWNUM_W = 52;
const isMac = navigator.platform.toLowerCase().includes("mac");

export interface EditTarget {
  schema: string;
  table: string;
  pk: string[];
  /** The table column of each result column, when the names differ (a console result with aliases: QueryResult.edit) */
  columns?: string[];
}

/** A console result the backend didn't check (qurry_shared::errors::CONSOLE_READ_ONLY, the TUI's text) */
export const CONSOLE_READ_ONLY = "Console results are read-only. Open the table to edit its rows.";

let lastExtractor: Extractor = "CSV"; // The last chosen extractor is the default for new tabs (DataGrip)

interface Props {
  result: QueryResult;
  /** For the table data editor: the edit target */
  edit?: EditTarget;
  /** A console result from a join (#102): per column, the table its edits go to or why it is read-only (no rows added or deleted) */
  editJoin?: JoinEdit;
  /** Read-only data source: editing disabled */
  readOnly?: boolean;
  /** Why rows can't be edited here, on the greyed-out edit buttons (console results: the backend's reason, or CONSOLE_READ_ONLY) */
  readOnlyReason?: string;
  /** The number of pending edits changed (a console asks before a rerun drops them) */
  onPendingChange?: (n: number) => void;
  /** SQL dialect (identifier quoting, literals) */
  dialect?: Dialect;
  running: boolean;
  loadingMore: boolean;
  query: string | null;
  onFetchMore: () => void;
  /** Read cut cells ([data row, col], QueryResult.cut) whole into the result by its key (useConsole fetchFull); → the result then.
   *  Absent: cut cells stay cut */
  onFetchFull?: (cells: [number, number][]) => Promise<QueryResult | undefined>;
  onSubmit: (statements: string[]) => Promise<void>;
  /** A console's run again resolves when it ends (Run again then gives the keys to its new grid) */
  onReload: () => void | Promise<void>;
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
  /** Whose column widths and hidden and pinned columns these are: a table tab's table (saved, shared with the TUI) or a console
   *  result's id; absent: this grid's own */
  widthsOf?: WidthsOf;
  /** The data source (target id) the grid's table is in: the status bar's Size reads its columns' declared types from it */
  target?: string;
}

/** pgvector columns (vector · halfvec · sparsevec) */
const isVectorType = (t: string) => /^(vector|halfvec|sparsevec)$/i.test(t);

export function ResultGrid(p: Props) {
  const { result, edit, loadingMore } = p;
  const D = p.dialect ?? dialectOf("postgres");
  const quoteIdent = D.q;
  const sqlLiteral = D.lit;
  const cols = result.columns;
  const scrollRef = useRef<HTMLDivElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [sorts, setSorts] = useState<SortKey[]>([]);
  /** Selected ranges in display coordinates (lib/gridSelection.ts) */
  const [selState, setSel] = useState<Selection | null>(null);
  const [editing, setEditing] = useState<{ r: number; c: number; value: string; picker?: boolean } | null>(null);
  const [changes, setChanges] = useState<Record<number, Record<number, Cell>>>({});
  const [deleted, setDeleted] = useState<Set<number>>(new Set());
  /** New rows: a cell never set is undefined (left out of the INSERT, so the column default applies), one set to NULL is null */
  const [added, setAdded] = useState<NewRow[]>([]);
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
  const valueLayout = useDefaultLayout({ id: "dbtool-value-panel", panelIds: panel && !chart ? ["grid", "value"] : ["grid"] });
  const [showQuery, setShowQuery] = useState(false);
  /** Columns…: the list of every column, to show or hide them */
  const [columnsOpen, setColumnsOpen] = useState(false);
  const dragging = useRef(false);

  const setExtractor = (x: Extractor) => {
    lastExtractor = x;
    setExtractorState(x);
  };

  /** The table column of result column i: what the generated SQL names */
  const tcol = (i: number) => edit?.columns?.[i] ?? cols[i].name;
  const pkIdx = useMemo(() => keyColumns(edit, cols), [edit, cols]);
  const canEdit = !p.readOnly && !!edit && edit.pk.length > 0 && pkIdx.every((i) => i >= 0);
  /** A join result: its cells are edited per table, by that table's key; rows can't be added, cloned or deleted */
  const join = p.readOnly ? undefined : p.editJoin;
  const anyEdit = canEdit || !!join;
  /** The key columns the header marks: a join's tables' keys */
  const keyCols = join ? join.tables.flatMap((t) => t.pkAt) : pkIdx;
  const joinRows = sharedOr("Rows can be added or deleted only in a result from one table.", (s) => s.editJoinRows());
  const baseCount = result.rows.length;
  const rowCount = baseCount + added.length;
  const serverSort = !!p.onServerSort && sortViaOrderBy;

  const { clickSort, order, dataIndex } = useSortOrder({ cols, result, sorts, setSorts, serverSort, quoteIdent, p });

  const rowAt = (r: number) => (r < baseCount ? result.rows[r] : added[r - baseCount]);
  const valueAt = (r: number, c: number): Cell => (r < baseCount && changes[r] && c in changes[r] ? changes[r][c] : rowAt(r)?.[c] ?? null);

  /** Why data cell (r, c) can't be edited ("": it can't, nothing to say), or undefined when it can. In a join, per table: a column
   *  of a table whose key isn't in the result says why, and the outer side where no row matched (its key NULL) is read-only */
  const cellReason = (r: number, c: number): string | undefined => {
    if (!join) return canEdit ? undefined : (p.readOnlyReason ?? "");
    const col = join.columns[c];
    if (!col) return "";
    if ("readOnly" in col) return col.readOnly;
    const t = join.tables[col.table];
    const name = `${t.schema}.${t.table}`;
    return t.pkAt.some((i) => result.rows[r]?.[i] === null) ? sharedOr(`Read-only: no ${name} row matches this row (outer join).`, (s) => s.editNoRow(name)) : undefined;
  };
  const cellOk = (r: number, c: number) => !deleted.has(r) && cellReason(r, c) === undefined;

  // ─── Cut values (Settings › Long values): read whole by the key when needed, never written back as cut ───
  const cut = useMemo(() => cutSet(result.cut), [result.cut]);
  /** Data cell (r, c) holds only the start of its value (a changed cell holds the change) */
  const cutAt = (r: number, c: number) => r < baseCount && cut.has(cellKey(r, c)) && !(changes[r] && c in changes[r]);
  const canFetch = (r: number) => !!p.onFetchFull && keyed(pkIdx, cut, r);
  /** Why cut cell (r, c) can't be read whole */
  const cutReason = (r: number, c: number) => {
    const at = `Cut at ${cutLength(result.rows[r]?.[c] ?? null)} characters`;
    if (edit && !edit.pk.length) return `${at}; the whole value needs a primary key (${edit.schema}.${edit.table} has none)`;
    if (edit && pkIdx.some((k) => cut.has(cellKey(r, k)))) return `${at}; the whole value is read by the row's key, which is cut too`;
    if (join) return `${at}; whole values are read only in a result from one table. Open the table to edit it`;
    return `${at}; the whole value needs a primary key. ${result.readOnlyReason ?? CONSOLE_READ_ONLY}`;
  };
  const fetchCells = (cells: [number, number][]) => (p.onFetchFull ? p.onFetchFull(cells) : Promise.resolve(undefined));
  /** A join's column that can't be edited in any row: why, for its header */
  const colReason = (c: number) => {
    const col = join?.columns[c];
    return col && "readOnly" in col ? col.readOnly : undefined;
  };

  // Loading more puts the new rows before the added ones: what pointed at an added row (the selection, the cell being edited, the
  // Value Editor's draft) moves down with it, so Delete Row or Revert Selected never hit a row that was just loaded
  const lastBase = useRef(baseCount);
  useEffect(() => {
    const was = lastBase.current;
    lastBase.current = baseCount;
    const by = baseCount - was;
    if (by <= 0 || !added.length) return;
    const mv = (r: number) => (r >= was ? r + by : r);
    setSel((s) => s && { ranges: s.ranges.map((x) => ({ ...x, r0: mv(x.r0), r1: mv(x.r1) })), anchor: { ...s.anchor, r: mv(s.anchor.r) }, cur: { ...s.cur, r: mv(s.cur.r) } });
    setEditing((ed) => ed && { ...ed, r: mv(ed.r) });
    setDraft((d) => d && { ...d, r: mv(d.r) });
  }, [baseCount]);

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
  useEffect(() => void p.onPendingChange?.(pendingCount), [pendingCount]);
  useEffect(() => () => void p.onPendingChange?.(0), []);
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
  const { wasmReady, setW, widths, shown, classOf, aligns, jsonCol, fitColumn, resetWidths, toggleFit, fitAll, stepColumn, startResize } = useGridWidths({
    cols, em, keyCols, pkIdx, join, result, firstRow, rowCount, valueAt, p,
  });
  // Display column c shows result column view[c] (hidden ones left out, pinned ones first): the selection and the cursor are in
  // display columns, edits, values and widths in result columns
  const layout = useColumnLayout({ cols, widths, rownumW, scrollRef, wasmReady, layoutOf: p.widthsOf });
  const { view, colVirt, shownCols, padR, total } = layout;
  // Pin, Unpin or Hide: the selection and the cursor follow their columns (as rows follow dataIndex), set before this render paints
  const [selView, setSelView] = useState(view);
  const sel = selView === view ? selState : remapCols(selState, selView, view);
  if (selView !== view) {
    setSelView(view);
    setSel(sel);
  }
  /** The column header's menu on display column c: hide or pin it, Columns…, widths of it or all of them */
  const headerItems = (c: number, i = view[c]): MenuItem[] => [
    { label: "Hide Column", ...cmd("grid.colHide"), action: () => layout.hide(i), disabled: view.length < 2 },
    c < layout.pinned ? { label: "Unpin Column", ...cmd("grid.colPin"), action: () => layout.unpin(i) } : { label: "Pin Column", ...cmd("grid.colPin"), action: () => layout.pin(i) },
    { label: "Columns…", ...cmd("grid.columns"), action: () => setColumnsOpen(true) },
    { separator: true },
    { label: "Fit Column", ...cmd("grid.colFit"), shortcut: cmd("grid.colFit").shortcut ?? "double-click the edge", action: () => fitColumn(i) },
    { label: "Fit All Columns", ...cmd("grid.colFitAll"), action: fitAll },
    { label: "Reset Widths", ...cmd("grid.colReset"), shortcut: cmd("grid.colReset").shortcut ?? "computed widths", action: resetWidths, disabled: !Object.keys(setW).length },
    { separator: true },
    { label: "Narrow Column", ...cmd("grid.colNarrow"), action: () => stepColumn(i, -1) },
    { label: "Widen Column", ...cmd("grid.colWiden"), action: () => stepColumn(i, 1) },
  ];

  const virt = useVirtualizer({ count: rowCount, getScrollElement: () => scrollRef.current, estimateSize: () => ROW_H, overscan: 20 });
  // Horizontally only the visible columns are drawn (useColumnLayout); blank cells fill the width between and after them
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
  const active = cur ? { r: dataIndex(cur.r), c: view[cur.c] } : null; // data coordinates
  const selRowSet = useMemo(() => new Set(rowsOf(sel)), [sel]);
  const selColSet = useMemo(() => new Set(selCols(sel)), [sel]);
  /** Data rows of every selected range, in grid order (the row-level edits: Delete Row, Revert Selected) */
  const selRows = (): number[] => rowsOf(sel).map(dataIndex);
  /** Every selected cell once, in data coordinates: the one path the cell edits (Set NULL, Revert Selected, …) take */
  const forSelected = (fn: (r: number, c: number) => void | false) => forEachCell(sel, (d, c) => fn(dataIndex(d), view[c]));
  const selectCell = (pos: Pos, ext: boolean) => setSel((s) => (ext ? extend(s, pos) : single(pos)));
  /** A mouse click: ⇧ extends the last range to b, ⌘ adds a..b (a cell, a row, a column) or takes it out, a plain click starts over (DataGrip) */
  const clickSel = (e: React.MouseEvent, a: Pos, b: Pos = a) => setSel((s) => (e.shiftKey ? extend(s, b) : (isMac ? e.metaKey : e.ctrlKey) ? toggle(s, a, b) : single(a, b)));

  useGridStatus({
    colVirt, scrollRef, wrapRef, chart, transposed, pinned: layout.pinned, pinEdge: layout.pinEdge, hasSel, sel, cur, wasmReady, forSelected, valueAt,
    result, changes, added, dataIndex, view, hidden: layout.hidden, edit, tcol, cols, p,
  });

  const { matches, isMatch, gotoMatch, openFind } = useGridFind({
    findOpen, setFindOpen, findText, rowCount, dataIndex, view, valueAt, result, changes, added, order, cur, selectCell, virt, scrollToCol: layout.scrollToCol, wrapRef,
  });

  const { commitEdit, setCell, startEdit, setNull, setDefault, genUuid, addRow, cloneRow, deleteRows, revertSelected, submit, fix, previewPending, doSubmitPreview } = useGridEdits({
    editing, setEditing, changes, setChanges, deleted, setDeleted, added, setAdded, preview, setPreview, submitErr, setSubmitErr,
    result, cols, baseCount, rowCount, join, canEdit, anyEdit, pkIdx, edit, D, tcol, valueAt, cellReason, cellOk,
    active, hasSel, selRows, forSelected, selectCell, virt, scrollRef, cut, cutAt, canFetch, cutReason, fetchCells, p,
  });
  const focusGrid = () => (scrollRef.current ?? wrapRef.current)?.focus();
  // The button goes with the failure: the keys go to the grid again, the new one when the run replaced it (a console's result is
  // drawn anew in the same place after its run)
  const runAgain = async () => {
    const place = wrapRef.current?.parentElement;
    focusGrid();
    await p.onReload();
    requestAnimationFrame(() => place?.querySelector<HTMLElement>(":scope > .grid-wrap .grid-scroll")?.focus());
  };
  const { panelCell, panelReadOnly, panelShown, panelNote, openPanel, applyPanel, closePanel } = useValuePanel({
    active, rowCount, cols, valueAt, cellOk, baseCount, setCell, draft, setDraft, setPanelError, panel, setPanel, setPanelFocus, focusGrid, wrapRef,
    cutAt, canFetch, cutReason, fetchCells,
  });

  // A note (why a paste or a copy format couldn't run) is about what was selected then (whole values read into the rows keep it)
  useEffect(() => setStatusNote(undefined), [sel, cols, baseCount, changes]);
  useEffect(() => () => setStatusNote(undefined), []);

  const { copySelection, copyInList, paste } = useGridClipboard({
    edit, quoteIdent, sel, hasSel, extractor, valueAt, dataIndex, view, pkIdx, cols, D, rowCount, anyEdit, cur, join, cellOk, joinRows, cellReason, setAdded, setCell,
    cutAt, canFetch, fetchCells,
  });

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
      setSel(single({ r: 0, c: 0 }, { r: rowCount - 1, c: view.length - 1 }));
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
    if ((e.key === "Enter" || e.key === "F2") && anyEdit && active) return startEdit(active.r, active.c), stop();
    if (e.key === "Escape") {
      setFindOpen(false);
      return;
    }
    const move: Record<string, [number, number]> = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1], Tab: [0, e.shiftKey ? -1 : 1] };
    const m = move[e.key];
    if (m) {
      const r = Math.min(Math.max(cur.r + m[0], 0), rowCount - 1);
      const c = Math.min(Math.max(cur.c + m[1], 0), view.length - 1);
      selectCell({ r, c }, e.shiftKey && e.key !== "Tab");
      virt.scrollToIndex(r);
      layout.scrollToCol(c);
      return stop();
    }
    // Typing directly in a cell starts editing (DataGrip). A key with ⌃, ⌘ or ⌥ is a shortcut on every platform (⌃` Quick Switch Theme): left to the global keys
    if (anyEdit && active && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      startEdit(active.r, active.c, e.key);
      return stop();
    }
  };

  // ─── Cell context menu ───
  /** The cell menu of display row d, display column dc. Grid command keys run its entries for the selected cell */
  const cellItems = (d: number, dc: number): MenuItem[] => {
    const [r, c] = [dataIndex(d), view[dc]];
    const v = valueAt(r, c);
    const k = isMac ? { copy: "⌘c", paste: "⌘v" } : { copy: "Ctrl+c", paste: "Ctrl+v" };
    const name = quoteIdent(cols[c].name);
    // Row values at the moment the menu opened (the chosen row stays even if auto refresh reorders rows — so the wrong session is not killed)
    const rowObj = Object.fromEntries(cols.map((col, i) => [col.name, valueAt(r, i)]));
    const items: MenuItem[] = [
      ...(p.rowActions?.length ? ([...p.rowActions.map((a) => ({ label: a.label, danger: a.danger, action: () => a.run(rowObj) })), { separator: true }] as MenuItem[]) : []),
      ...(anyEdit ? ([{ label: "Edit", shortcut: "↩", action: () => startEdit(r, c), disabled: !cellOk(r, c) }] as MenuItem[]) : []),
      { label: "Open in Value Editor", ...cmd("grid.valueEditor"), action: openPanel },
      ...(anyEdit
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
      ...(anyEdit ? ([{ label: "Paste", shortcut: k.paste, action: paste }] as MenuItem[]) : []),
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
  const viewItems = (): MenuItem[] => [
    { label: (!transposed && !chart ? "✓ " : "　") + "Table", ...cmd("grid.viewTable"), action: () => (setTransposed(false), setChart(false)) },
    { label: (transposed ? "✓ " : "　") + "Transpose", ...cmd("grid.viewTranspose"), action: () => (setTransposed(true), setChart(false)) },
    { label: (chart ? "✓ " : "　") + "Chart", ...cmd("grid.viewChart"), action: () => (setChart(true), setTransposed(false)) },
  ];
  const optionItems = (): MenuItem[] => [
    ...(p.filterBar ? ([{ label: (showFilter ? "✓ " : "　") + "Show Filter", ...cmd("grid.showFilter"), action: () => setShowFilter((v) => !v) }] as MenuItem[]) : []),
    ...(p.onServerSort ? ([{ label: (sortViaOrderBy ? "✓ " : "　") + "Sort via ORDER BY", ...cmd("grid.sortOrderBy"), action: () => { setSorts([]); setSortViaOrderBy((v) => !v); } }] as MenuItem[]) : []),
    { label: "Reset View", ...cmd("grid.resetView"), action: () => { setSorts([]); setTransposed(false); setChart(false); layout.reset(); if (serverSort) p.onServerSort!(""); } },
    { separator: true },
    { label: "View Query", ...cmd("grid.viewQuery"), action: () => setShowQuery(true), disabled: !p.query },
    ...(p.onCopyQueryToConsole ? ([{ label: "Copy Query to Console", ...cmd("grid.queryToConsole"), action: () => p.query && p.onCopyQueryToConsole!(p.query), disabled: !p.query }] as MenuItem[]) : []),
    { label: "Copy Query to Clipboard", ...cmd("grid.copyQuery"), action: () => p.query && navigator.clipboard.writeText(p.query), disabled: !p.query },
  ];
  /** What the toolbar's buttons and menus run, for their command keys (the buttons are drawn below with the same conditions) */
  const toolbarCommands = (): MenuItem[] => [
    { label: "Reload Page", ...cmd("grid.reload"), action: p.onReload, disabled: p.running },
    { label: "Cancel Running Statements", ...cmd("grid.cancel"), action: p.onCancel, disabled: !p.running && !loadingMore },
    { label: "Add Row", ...cmd("grid.addRow"), action: addRow, disabled: !canEdit },
    { label: "Delete Row", ...cmd("grid.deleteRows"), action: deleteRows, disabled: !canEdit || !hasSel },
    { label: "Revert Selected", ...cmd("grid.revert"), action: revertSelected, disabled: !pendingCount },
    { label: "Preview Pending Changes", ...cmd("grid.preview"), action: previewPending, disabled: !pendingCount },
    { label: "Submit", ...cmd("grid.submit"), action: submit, disabled: !pendingCount },
    ...(p.onDdl ? ([{ label: "Go to DDL", ...cmd("grid.ddl"), action: p.onDdl }] as MenuItem[]) : []),
    { label: "Find on Current Page", ...cmd("grid.find"), action: openFind },
    { label: "Export Data…", ...cmd("grid.export"), action: p.onExport },
    ...viewItems(),
    ...optionItems(),
    // The header menu's commands on the current column (the focused cell's, or the clicked header's); not in Transpose or Chart.
    // Columns… needs no column
    ...(cur && !chart && !transposed ? headerItems(cur.c) : headerItems(0).map((x) => ({ ...x, disabled: chart || transposed || !("label" in x && x.label === "Columns…") }))),
  ];

  /** An edit button's tooltip and description: the command, and why it's greyed out when the grid says so (a join's row buttons: why
   *  rows can't be added or deleted there) */
  const reason = anyEdit ? undefined : p.readOnlyReason;
  const rowReason = canEdit ? undefined : join ? joinRows : p.readOnlyReason;
  const editButton = (title: string, id: string, why = reason) => ({
    title: why ? `${keyTitle(title, id)} — ${why}` : keyTitle(title, id),
    "aria-description": why,
  });
  const rowsLabel = `${baseCount}${result.truncated ? "+" : ""} row${baseCount === 1 ? "" : "s"}`;

  return (
    <div className="grid-wrap" ref={wrapRef} tabIndex={-1} onKeyDown={onKeyDown}>
      {/* ── DataGrip grid toolbar ── */}
      <Toolbar onMouseDown={(e) => e.stopPropagation()}>
        {/* One button: ↻ reloads, and is ■ in the same spot during every load of this grid (a run, a load-more on scroll); ■ cancels only
            the fetch in flight, the rows already here stay */}
        {p.running || loadingMore ? (
          <IconButton tone="danger" title={keyTitle("Cancel", "grid.cancel")} onClick={p.onCancel}><IcStop /></IconButton>
        ) : (
          <IconButton title={keyTitle("Reload Page", "grid.reload")} onClick={p.onReload}><IcReload /></IconButton>
        )}
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
        <ToolbarSep />
        <IconButton {...editButton("Add Row", "grid.addRow", rowReason)} onClick={addRow} disabled={!canEdit}><IcPlus /></IconButton>
        <IconButton {...editButton("Delete Row", "grid.deleteRows", rowReason)} onClick={deleteRows} disabled={!canEdit || !hasSel}><IcMinus /></IconButton>
        <IconButton {...editButton("Revert Selected", "grid.revert")} onClick={revertSelected} disabled={!pendingCount}><IcRevert /></IconButton>
        <IconButton {...editButton("Preview Pending Changes", "grid.preview")} onClick={previewPending} disabled={!pendingCount}><IcPreview /></IconButton>
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
      <Group orientation="horizontal" className="grid-split" defaultLayout={valueLayout.defaultLayout} onLayoutChanged={valueLayout.onLayoutChanged}>
        <Panel id="grid" minSize="25%">
          <div className="grid-body">
            {/* Column header above the scroller (not sticky inside it), so it can't drift off the toolbar while rows scroll */}
            {!chart && !transposed && (
              <GridHead
                body={scrollRef}
                corner={
                  <GridHeaderRow width={rownumW} height={HEADER_H}>
                    <RowNumberCell
                      width={rownumW}
                      title="Select all (⌘a)"
                      onMouseDown={() => setSel(single({ r: 0, c: 0 }, { r: rowCount - 1, c: view.length - 1 }))}
                    />
                  </GridHeaderRow>
                }
              >
                <GridHeaderRow width={total - rownumW} height={HEADER_H}>
                  {shownCols.map(({ c, i, pad, pin }) => {
                    const idx = sorts.findIndex((x) => x.col === i);
                    const key = sorts[idx];
                    return (
                      <GridHeaderCell
                        key={i}
                        name={cols[i].name}
                        type={cols[i].typeName}
                        pk={keyCols.includes(i)}
                        readOnly={colReason(i)}
                        width={widths[i]}
                        pin={pin}
                        before={pad}
                        colSelected={selColSet.has(c)}
                        sort={key ? { dir: key.dir, order: sorts.length > 1 ? idx + 1 : undefined } : undefined}
                        // Header click = select the whole column (DataGrip)
                        onMouseDown={(e) => clickSel(e, { r: 0, c }, { r: rowCount - 1, c })}
                        onSortClick={(e) => clickSort(i, e.altKey)}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          setMenu({ x: e.clientX, y: e.clientY, items: headerItems(c) });
                        }}
                        onResize={(e) => startResize(e, i)}
                        onFit={() => toggleFit(i)}
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
              <TransposedView
                cols={cols}
                rows={Array.from({ length: Math.min(rowCount, 300) }, (_, d) => cols.map((_, c) => valueAt(dataIndex(d), c)))}
                cutAt={(d, c) => cutAt(dataIndex(d), c)}
              />
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
                          onMouseDown={(e) => clickSel(e, { r: d, c: 0 }, { r: d, c: view.length - 1 })}
                        >
                          {isNew ? "+" : d + 1}
                        </RowNumberCell>
                        {shownCols.map(({ c, i: ci, pad, pin }) => {
                          const val = valueAt(r, ci);
                          const changed = !isNew && changes[r] && ci in changes[r];
                          if (editing && editing.r === r && editing.c === ci) {
                            const temporal = isTemporal(cols[ci].typeName);
                            return (
                              <CellEditor
                                key={ci}
                                width={widths[ci]}
                                pin={pin}
                                before={pad}
                                value={editing.value}
                                onChange={(v) => setEditing({ ...editing, value: v })}
                                onBlurOut={commitEdit}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter" || e.key === "Tab") {
                                    e.preventDefault();
                                    commitEdit();
                                    if (e.key === "Tab") selectCell({ r: d, c: Math.min(c + 1, view.length - 1) }, false);
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
                          const isCut = cutAt(r, ci);
                          return (
                            <GridCell
                              key={ci}
                              width={widths[ci]}
                              pin={pin}
                              before={pad}
                              kind={CELL_KIND[cls]}
                              align={aligns[ci][cls]}
                              selected={inSel(d, c)}
                              active={cur?.r === d && cur?.c === c}
                              changed={!!changed}
                              match={isMatch(d, c)}
                              // A join's cell that can't be edited: why, on hover (whole columns say it on their header too)
                              title={join && !isNew ? cellReason(r, ci) || undefined : undefined}
                              cut={isCut ? cutTitle(cutLength(val)) : undefined}
                              onMouseDown={(e) => {
                                if (e.button !== 0) return;
                                dragging.current = true;
                                clickSel(e, { r: d, c });
                              }}
                              onMouseEnter={() => {
                                if (dragging.current) selectCell({ r: d, c }, true);
                              }}
                              onDoubleClick={() => startEdit(r, ci)}
                              onContextMenu={(e) => openCellMenu(e, d, c)}
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
              </div>
            )}
          </div>
        </Panel>
        {panel && !chart && (
          <>
            <Separator className="resize-h" />
            <Panel id="value" defaultSize="35%" minSize="15%">
              <ValuePanel
                title={panelCell ? `${cols[panelCell.c].name} : ${cols[panelCell.c].typeName.toLowerCase()}` : undefined}
                text={draftAt(draft, panelCell) ? draft.text : panelShown.text}
                json={panelShown.json}
                isNull={!!panelCell && valueAt(panelCell.r, panelCell.c) === null}
                readOnly={!panelCell || panelReadOnly(panelCell.r, panelCell.c)}
                note={panelNote}
                error={panelError}
                focusSeq={panelFocus}
                onChange={(text) => {
                  if (!panelCell) return;
                  setPanelError(null);
                  setDraft({ ...panelCell, text, base: panelShown.text, editable: !panelReadOnly(panelCell.r, panelCell.c) });
                }}
                onToggleLayout={() => {
                  // A cut value's start isn't JSON to format, and a draft of it would outlive the whole value's arrival
                  if (!panelCell || cutAt(panelCell.r, panelCell.c)) return;
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
      </Group>
      {/* A failed load-more, below the rows (covering none), in full: why no more rows came (the status bar says it too), and
          running the statement again starts over */}
      {!loadingMore && result.moreError && (
        <Notice tone="danger" className="grid-more-error-row">
          <span className="grid-more-error">{result.moreError}</span>
          <Button size="sm" onClick={runAgain} disabled={p.running}>Run again</Button>
        </Notice>
      )}

      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
      {columnsOpen && (
        <CheckListPopover title="Columns…" items={layout.listItems()} onToggle={layout.setShown} onClose={() => (setColumnsOpen(false), focusGrid())}
          action={{ label: "Show All", onClick: layout.showAll, disabled: !layout.hidden }} />
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
          <SqlBlock className="sql-preview" sql={p.query} kind={D.quoting} />
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
          <SqlBlock className="sql-preview" sql={preview.sql.join("\n")} kind={D.quoting} />
          {submitErr && <Notice tone="danger">{submitErr}</Notice>}
          {fix && <FixSql {...fix} />}
        </Modal>
      )}
    </div>
  );
}
