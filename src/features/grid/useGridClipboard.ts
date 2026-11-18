import type { Dispatch, SetStateAction } from "react";
import type { EditTarget } from "../../components/ResultGrid";
import type { Cell, Dialect, JoinEdit, QueryResult } from "../../lib/api";
import { copied, type Extractor, extractRows, HOLE_TOKEN, pasteCells, raw } from "../../lib/extract";
import type { NewRow } from "../../lib/gridEdit";
import { copyTable, forEachCell, isSingle, type Pos, type Selection, single } from "../../lib/gridSelection";
import { sharedReady } from "../../lib/shared";
import { setStatusNote } from "../../lib/statusbar";
import { cellKey, cutSet } from "./shared";

/** Copy of cut values that couldn't be read whole: what was copied, and how to get the whole values */
export const cutCopied = (n: number) =>
  `${n} value${n === 1 ? " is" : "s are"} cut (no key to fetch ${n === 1 ? "it" : "them"} whole); raise Settings › Appearance › Data grid › Long values`;

/** The result grid's clipboard: Copy in the chosen Data Extractor, Copy as IN List, Paste */
export function useGridClipboard({
  edit, quoteIdent, sel, hasSel, extractor, valueAt, dataIndex, view, pkIdx, cols, D, rowCount, anyEdit, cur, join, cellOk, joinRows, cellReason, setAdded, setCell,
  cutAt, canFetch, fetchCells,
}: {
  edit: EditTarget | undefined;
  quoteIdent: (name: string) => string;
  sel: Selection | null;
  hasSel: boolean;
  extractor: Extractor;
  valueAt: (r: number, c: number) => Cell;
  dataIndex: (d: number) => number;
  /** Display column → result column (useColumnLayout): a selection is copied and pasted in the shown columns, in their order */
  view: number[];
  pkIdx: number[];
  cols: QueryResult["columns"];
  D: Dialect;
  rowCount: number;
  anyEdit: boolean;
  cur: Pos | null;
  join: JoinEdit | undefined;
  cellOk: (r: number, c: number) => boolean;
  joinRows: string;
  cellReason: (r: number, c: number) => string | undefined;
  setAdded: Dispatch<SetStateAction<NewRow[]>>;
  setCell: (r: number, c: number, next: Cell, whole?: boolean) => void;
  /** Cut data cell (r, c), whether row r's can be read whole, and the read (useGridEdits) */
  cutAt: (r: number, c: number) => boolean;
  canFetch: (r: number) => boolean;
  fetchCells: (cells: [number, number][]) => Promise<QueryResult | undefined>;
}) {
  const tableRef = edit ? `${quoteIdent(edit.schema)}.${quoteIdent(edit.table)}` : "MY_TABLE";

  // ─── Copy (Data Extractor) ───
  /** header = a column-name line at the top of CSV · TSV (Copy is values only — DataGrip; Copy with Headers includes it) */
  /** The cells of `s` in a format: rows and columns in grid order, the union of the ranges' columns (gridSelection.copyTable) */
  const extract = (s: Selection, fmt: Extractor, header = true): string => {
    const t = copyTable<Cell>(s, (d, c) => valueAt(dataIndex(d), view[c]), HOLE_TOKEN);
    const rows = t.rows.map(dataIndex);
    const tc = t.cols.map((c) => view[c]);
    // Key values are read only for SQL Updates (as before: other formats never touch the key columns)
    const keys = fmt === "SQL Updates" ? rows.map((r) => pkIdx.map((k): [string, Cell] => [cols[k].name, valueAt(r, k)])) : [];
    return extractRows(fmt, { names: tc.map((c) => cols[c].name), types: tc.map((c) => cols[c].typeName), vals: t.vals, keys, tableRef, dialect: D, header });
  };
  /** The shared WASM writes the formats: without it (a WebView that refused WebAssembly) say why nothing is copied */
  const formatted = (make: () => string): string | undefined => {
    if (sharedReady()) return make();
    setStatusNote("Copy formats need WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk");
  };
  /** The copy of `s`: its cut cells are read whole first (the result's rows take them, so `make` reads them); those of rows without a
   *  key are copied as cut, and the status bar says so (a read that failed says so itself: fetchFull). Without cut cells the text is written right away, else once read (a
   *  ClipboardItem that waits for it: WebKit refuses a write after the click's moment) */
  const copyOf = (s: Selection, make: () => string | undefined, as: Parameters<typeof copied>[1]) => {
    const cells: [number, number][] = [];
    forEachCell(s, (d, c) => void (cutAt(dataIndex(d), view[c]) && cells.push([dataIndex(d), view[c]])));
    const done = (text: string | undefined) => (text !== undefined && copied(text, as), text);
    if (!cells.length) {
      const text = done(make());
      if (text !== undefined) navigator.clipboard.writeText(text);
      return;
    }
    const read = cells.filter(([r]) => canFetch(r));
    const text = (read.length ? fetchCells(read) : Promise.resolve(undefined)).then((res) => {
      const still = cutSet(res?.cut);
      const left = cells.filter(([r, c]) => !canFetch(r) || still.has(cellKey(r, c))).length;
      const t = done(make());
      if (t !== undefined && left) setStatusNote(cutCopied(left));
      if (t === undefined) throw new Error("nothing to copy");
      return new Blob([t], { type: "text/plain" });
    });
    navigator.clipboard.write([new ClipboardItem({ "text/plain": text })]).catch(() => {});
  };
  const copySelection = (header = false) => {
    if (!sel || !hasSel) return;
    // The one selected cell (not sel.cur: a ⌘-click that took a cell out leaves the cursor outside the selection)
    const one = sel.ranges[0];
    const rawOne = isSingle(sel) && !header && (extractor === "CSV" || extractor === "TSV");
    // Paste reads this text back as its format (one copied row splits into cells); one raw value stays one value
    copyOf(sel, () => (rawOne ? raw(valueAt(dataIndex(one.r0), view[one.c0])) : formatted(() => extract(sel, extractor, header))), rawOne ? "other" : extractor);
  };
  /** Selected cells as ('a', 'b') (paste straight into WHERE x IN). If only one cell is selected, its whole column */
  const copyInList = () => {
    if (!sel || !hasSel) return;
    const c = sel.ranges[0].c0;
    const s = isSingle(sel) ? single({ r: 0, c }, { r: rowCount - 1, c }) : sel;
    copyOf(s, () => formatted(() => extract(s, "SQL IN List")), "other");
  };
  /** Paste: TSV/CSV into cells (pasteCells: an empty field is NULL), adding new rows if there are not enough (DataGrip) */
  const paste = async () => {
    if (!anyEdit || !cur) return;
    // Paste goes into one place (DataGrip); a ⌘-click meant as a plain click is the usual way to get several ranges here
    if ((sel?.ranges.length ?? 0) > 1) return setStatusNote("Paste needs one selected range: click a cell to select just it (⌘-click adds cells to the selection)");
    if (!sharedReady()) return setStatusNote("Paste needs WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk");
    const grid = pasteCells(await navigator.clipboard.readText());
    // A join: only into cells that can be edited, and no rows added
    if (join) {
      const spots = grid.flatMap((cells, i) => cells.map((_, j) => [cur.r + i, cur.c + j]).filter(([, c]) => c < view.length));
      const bad = spots.find(([d, c]) => d >= rowCount || !cellOk(dataIndex(d), view[c]));
      if (bad) return setStatusNote(bad[0] >= rowCount ? joinRows : cellReason(dataIndex(bad[0]), view[bad[1]]) || "Paste reaches a cell that can't be edited");
    }
    grid.forEach((cells, i) => {
      const d = cur.r + i;
      const r = d < rowCount ? dataIndex(d) : -1;
      if (r < 0) {
        const row: NewRow = cols.map(() => undefined);
        cells.forEach((v, j) => {
          if (cur.c + j < view.length) row[view[cur.c + j]] = v;
        });
        setAdded((a) => [...a, row]);
      } else cells.forEach((v, j) => cur.c + j < view.length && setCell(r, view[cur.c + j], v, true));
    });
  };
  return { copySelection, copyInList, paste };
}
