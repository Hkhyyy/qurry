import { useEffect, useId, useMemo, useRef } from "react";
import type { Cell, JoinEdit, QueryResult } from "../../lib/api";
import { CELL_PAD, CHAR_PX, fitChars, stepWidth, toChars, toPx, useColumnWidths, widthKeys, type WidthsOf } from "../../lib/columnWidths";
import { alignTable, cellClasses, useGridAlign } from "../../lib/gridAlign";
import { isJsonType } from "../../lib/jsonSpans";
import { useSharedReady } from "../../lib/shared";
import { setStatusNote } from "../../lib/statusbar";
import { fracWidth, isTimeType, padFrac } from "../../lib/timeFrac";
import { display } from "./shared";

/** The result grid's column widths (computed, set by the user, fit, step, drag) and how each column shows its values */
export function useGridWidths({ cols, em, keyCols, pkIdx, join, result, firstRow, rowCount, valueAt, p }: {
  cols: QueryResult["columns"];
  em: number;
  keyCols: number[];
  pkIdx: number[];
  join: JoinEdit | undefined;
  result: QueryResult;
  firstRow: Cell[] | undefined;
  rowCount: number;
  valueAt: (r: number, c: number) => Cell;
  p: { widthsOf?: WidthsOf };
}) {
  const headPx = useMemo(() => cols.map((col, ci) => col.name.length * 7.6 * em + col.typeName.length * 6.2 + 48 + (keyCols.includes(ci) ? 16 : 0)), [cols, pkIdx, join, em]);
  const computed = useMemo(
    () =>
      cols.map((_, ci) => {
        let len = 0;
        for (const row of result.rows.slice(0, 200)) len = Math.max(len, display(row[ci]).length);
        return Math.min(Math.max(headPx[ci], len * CHAR_PX * em + CELL_PAD, 64), 420 * em);
      }),
    [cols, firstRow, headPx, em],
  );
  const wasmReady = useSharedReady();
  // Widths a user set (drag, fit, the header menu, ⌃⌥← / ⌃⌥→) by column name (`id#2` for a repeated one), in characters (lib/columnWidths.ts)
  const ownWidths = useId();
  const userWidths = useColumnWidths(p.widthsOf ?? ownWidths, (msg) => setStatusNote(msg));
  const setW = userWidths.widths;
  const wkey = useMemo(() => widthKeys(cols.map((c) => c.name)), [cols, wasmReady]);
  /** Column i's set width (characters), or undefined while it has none */
  const setOf = (i: number): number | undefined => (Object.prototype.hasOwnProperty.call(setW, wkey[i]) ? setW[wkey[i]] : undefined);
  /** The one place a column's width comes from (layout, virtualization, scrolling): the user's, else the computed one */
  const widths = useMemo(() => computed.map((w, i) => { const s = setOf(i); return s === undefined ? w : toPx(s, em); }), [computed, setW, wkey, em]);
  // Time columns: align fractional-second digits within the column (values unchanged)
  const fracW = useMemo(() => cols.map((c, ci) => (isTimeType(c.typeName) ? fracWidth(result.rows.map((r) => r[ci])) : 0)), [cols, result.rows, wasmReady]);
  const shown = (v: Cell, ci: number) => (fracW[ci] && typeof v === "string" ? padFrac(v, fracW[ci]) : display(v));
  // Value class of each cell and where it sits (Settings › Appearance › Data grid › Alignment; the TUI grid's rule)
  const classOf = useMemo(() => cols.map((c) => cellClasses(c.typeName)), [cols, wasmReady]);
  const alignOverrides = useGridAlign();
  const aligns = useMemo(() => cols.map((c, ci) => alignTable(alignOverrides, c.typeName, result.rows, ci)), [cols, result.rows, alignOverrides, wasmReady]);
  // JSON columns color their text cells (lib/jsonSpans.ts, the TUI grid's tokenizer)
  const jsonCol = useMemo(() => cols.map((c) => isJsonType(c.typeName)), [cols, wasmReady]);

  // ─── Column widths ───
  /** Characters column i needs for its header and its longest cell as shown, over the rows loaded so far (DataGrip: Fit Column) */
  const fitOf = (i: number) => fitChars(headPx[i], Array.from({ length: rowCount }, (_, r) => shown(valueAt(r, i), i)), em);
  const fitColumn = (i: number) => userWidths.set({ [wkey[i]]: fitOf(i) });
  /** A column's set width before a double-click fitted it (0: none, the computed one), so the next double-click puts it back (TUI: =) */
  const beforeFit = useRef(new Map<string, number>());
  // A new result (or Reset Widths) starts without it, as the TUI's grid does
  useEffect(() => beforeFit.current.clear(), [result]);
  const resetWidths = () => {
    beforeFit.current.clear();
    userWidths.reset();
  };
  const toggleFit = (i: number) => {
    const [k, fit] = [wkey[i], fitOf(i)];
    // Back exactly: a width the column had, or none (its computed pixels; rounding them to characters could land on the fit again)
    if (setOf(i) === fit) {
      userWidths.set({ [k]: beforeFit.current.get(k) ?? 0 });
      beforeFit.current.delete(k);
    } else {
      beforeFit.current.set(k, setOf(i) ?? 0);
      userWidths.set({ [k]: fit });
    }
  };
  const fitAll = () => userWidths.set(Object.fromEntries(wkey.map((k, i) => [k, fitOf(i)])));
  /** Narrow (-1) / widen (+1) column i by one step of the shared rule (the TUI's < >), from the width it has now */
  const stepColumn = (i: number, steps: number) => userWidths.set({ [wkey[i]]: stepWidth(setOf(i) ?? toChars(widths[i], em), steps) });
  /** Dragging a header's right edge: the width follows the pointer (in whole characters), saved once on release */
  const startResize = (e: React.MouseEvent, i: number) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const [x0, w0, name] = [e.clientX, widths[i], wkey[i]];
    let last: number | null = null;
    const move = (ev: MouseEvent) => {
      const w = toChars(w0 + ev.clientX - x0, em);
      if (w !== last) userWidths.set({ [name]: (last = w) }, false);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("col-resizing");
      if (last !== null) userWidths.set({ [name]: last });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    document.body.classList.add("col-resizing");
  };
  return { wasmReady, setW, widths, shown, classOf, aligns, jsonCol, fitColumn, resetWidths, toggleFit, fitAll, stepColumn, startResize };
}
