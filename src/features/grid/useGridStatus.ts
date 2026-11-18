import type { Virtualizer } from "@tanstack/react-virtual";
import { type RefObject, useEffect, useMemo } from "react";
import type { EditTarget } from "../../components/ResultGrid";
import type { Cell, Dialect, JoinEdit, QueryResult } from "../../lib/api";
import { DEFAULT_TOKEN } from "../../lib/extract";
import type { NewRow } from "../../lib/gridEdit";
import { isSingle, type Pos, selCols, selRows as rowsOf, type Selection } from "../../lib/gridSelection";
import { shared, type GridStatus, type StatusItemId } from "../../lib/shared";
import { setFocusWidgets, shownColumns, spanOf, tableColumns, useStatusItems, useTableColumnsLoaded } from "../../lib/statusbar";

/** The status bar items that are over the selected cells */
const SELECTION_ITEMS: StatusItemId[] = ["notNull", "sum", "avg", "min", "max"];

/** What the result grid shows in the status bar while focus is in it */
export function useGridStatus({
  colVirt, scrollRef, wrapRef, chart, transposed, pinned, pinEdge, hasSel, sel, cur, wasmReady, forSelected, valueAt,
  result, changes, added, dataIndex, view, hidden, edit, tcol, cols, p,
}: {
  colVirt: Virtualizer<HTMLDivElement, Element>;
  scrollRef: RefObject<HTMLDivElement | null>;
  wrapRef: RefObject<HTMLDivElement | null>;
  chart: boolean;
  transposed: boolean;
  /** How many display columns are pinned, and the right edge of the pinned strip (useColumnLayout) */
  pinned: number;
  pinEdge: number;
  hasSel: boolean;
  sel: Selection | null;
  cur: Pos | null;
  wasmReady: boolean;
  forSelected: (fn: (r: number, c: number) => void | false) => void;
  valueAt: (r: number, c: number) => Cell;
  result: QueryResult;
  changes: Record<number, Record<number, Cell>>;
  added: NewRow[];
  dataIndex: (d: number) => number;
  /** Display column → result column, and how many the layout hides (useColumnLayout) */
  view: number[];
  hidden: number;
  edit: EditTarget | undefined;
  tcol: (i: number) => string;
  cols: QueryResult["columns"];
  p: { editJoin?: JoinEdit; target?: string; dialect?: Dialect };
}) {
  // ─── Status bar (lib/statusbar, qurry_shared::statusbar words it): the selection or the current cell (Position), the selected cells'
  // Not null / Sum / Avg / Min / Max, the current cell's Size, the shown columns, while focus is in this grid. The shown range follows
  // sideways scrolling (the column virtualizer renders again on scroll); Chart and Transpose have neither ───
  const statusItems = useStatusItems();
  const offset = colVirt.scrollOffset ?? 0;
  // The pinned columns are always on screen; the others count from the pinned strip's right edge (those under it don't show). The
  // range takes the pinned ones in while the first unpinned column still shows next to them
  const el = !chart && !transposed ? scrollRef.current : null;
  const rest = el ? shownColumns(colVirt.measurementsCache.slice(pinned), offset + pinEdge, offset + el.clientWidth) : null;
  const range = rest ? [rest[0] ? rest[0] + pinned : 0, rest[1] + pinned] : el && pinned ? [0, pinned - 1] : null;
  const multi = hasSel && !isSingle(sel);
  const posSpans = useMemo<GridStatus["pos"]>(() => (chart || transposed || !cur ? undefined : multi ? [spanOf(rowsOf(sel)), spanOf(selCols(sel))] : [spanOf([cur.r]), spanOf([cur.c])]), [sel, chart, transposed]);
  // Only while one of them is on: every selected cell's value goes to the module (at most qurry_shared::statusbar::MAX_CELLS)
  const wantStats = multi && wasmReady && statusItems.some((i) => SELECTION_ITEMS.includes(i));
  const stats = useMemo(() => {
    if (!wantStats) return undefined;
    const max = shared().statusBarInfo().maxCells;
    const vals: Cell[] = [];
    // A not-yet-written DEFAULT has no value to count
    forSelected((r, c) => {
      if (vals.length >= max) return false;
      const v = valueAt(r, c);
      if (v !== DEFAULT_TOKEN) vals.push(v);
    });
    return shared().selectionStats(vals);
  }, [wantStats, sel, result, changes, added]);
  // Size: the current cell alone, and its table column's declared type when the grid's columns map to a table (a table tab, a console
  // result of one table or a join's columns of a table)
  // (not a not-yet-written DEFAULT: it has no value yet)
  const sizeCell = !multi && cur && !chart && !transposed && statusItems.includes("size") ? { r: dataIndex(cur.r), c: view[cur.c] } : null;
  const sizeAt = sizeCell && valueAt(sizeCell.r, sizeCell.c) !== DEFAULT_TOKEN ? sizeCell : null;
  const sizeValue = sizeAt ? valueAt(sizeAt.r, sizeAt.c) : null;
  const joinCol = sizeAt ? p.editJoin?.columns[sizeAt.c] : undefined;
  const colRef = !sizeAt ? undefined : edit ? { schema: edit.schema, table: edit.table, column: tcol(sizeAt.c) } : joinCol && "table" in joinCol ? { ...p.editJoin!.tables[joinCol.table], column: joinCol.column } : undefined;
  useTableColumnsLoaded();
  const colMeta = colRef && p.target ? tableColumns(p.target, colRef.schema, colRef.table)?.find((m) => m.name === colRef.column) : undefined;
  const kind = p.dialect?.quoting;
  const gridStatus = useMemo<GridStatus>(
    () => ({
      kind,
      pos: posSpans,
      stats,
      shown: range ? [range[0] + 1, range[1] + 1, view.length] : undefined,
      hidden: chart || transposed ? 0 : hidden,
      cell: sizeAt ? { typeName: cols[sizeAt.c].typeName, value: sizeValue, column: colMeta && { dataType: colMeta.dataType, charset: colMeta.charset } } : undefined,
    }),
    [kind, posSpans, stats, range?.[0], range?.[1], view.length, hidden, chart, transposed, cols, sizeAt?.r, sizeAt?.c, sizeValue, colMeta],
  );
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    setFocusWidgets(el, { grid: gridStatus });
  }, [gridStatus]);
  useEffect(() => {
    const el = wrapRef.current;
    return () => {
      if (el) setFocusWidgets(el, undefined);
    };
  }, []);
}
