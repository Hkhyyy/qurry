import type { Dispatch, SetStateAction } from "react";
import type { EditTarget } from "../../components/ResultGrid";
import type { Cell, Dialect, JoinEdit, QueryResult } from "../../lib/api";
import { copied, type Extractor, extractRows, HOLE_TOKEN, pasteCells, raw } from "../../lib/extract";
import type { NewRow } from "../../lib/gridEdit";
import { copyTable, isSingle, type Pos, type Selection, single } from "../../lib/gridSelection";
import { sharedReady } from "../../lib/shared";
import { setStatusNote } from "../../lib/statusbar";

/** The result grid's clipboard: Copy in the chosen Data Extractor, Copy as IN List, Paste */
export function useGridClipboard({
  edit, quoteIdent, sel, hasSel, extractor, valueAt, dataIndex, pkIdx, cols, D, rowCount, anyEdit, cur, join, cellOk, joinRows, cellReason, setAdded, setCell,
}: {
  edit: EditTarget | undefined;
  quoteIdent: (name: string) => string;
  sel: Selection | null;
  hasSel: boolean;
  extractor: Extractor;
  valueAt: (r: number, c: number) => Cell;
  dataIndex: (d: number) => number;
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
  setCell: (r: number, c: number, next: Cell) => void;
}) {
  const tableRef = edit ? `${quoteIdent(edit.schema)}.${quoteIdent(edit.table)}` : "MY_TABLE";

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
    setStatusNote("Copy formats need WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk");
  };
  const copySelection = (header = false) => {
    if (!sel || !hasSel) return;
    // The one selected cell (not sel.cur: a ⌘-click that took a cell out leaves the cursor outside the selection)
    const one = sel.ranges[0];
    const rawOne = isSingle(sel) && !header && (extractor === "CSV" || extractor === "TSV");
    const text = rawOne ? raw(valueAt(dataIndex(one.r0), one.c0)) : formatted(() => extract(sel, extractor, header));
    if (text === undefined) return;
    // Paste reads this text back as its format (one copied row splits into cells); one raw value stays one value
    copied(text, rawOne ? "other" : extractor);
    navigator.clipboard.writeText(text);
  };
  /** Selected cells as ('a', 'b') (paste straight into WHERE x IN). If only one cell is selected, its whole column */
  const copyInList = () => {
    if (!sel || !hasSel) return;
    const c = sel.ranges[0].c0;
    const text = formatted(() => extract(isSingle(sel) ? single({ r: 0, c }, { r: rowCount - 1, c }) : sel, "SQL IN List"));
    if (text === undefined) return;
    copied(text, "other");
    navigator.clipboard.writeText(text);
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
      const spots = grid.flatMap((cells, i) => cells.map((_, j) => [cur.r + i, cur.c + j]).filter(([, c]) => c < cols.length));
      const bad = spots.find(([d, c]) => d >= rowCount || !cellOk(dataIndex(d), c));
      if (bad) return setStatusNote(bad[0] >= rowCount ? joinRows : cellReason(dataIndex(bad[0]), bad[1]) || "Paste reaches a cell that can't be edited");
    }
    grid.forEach((cells, i) => {
      const d = cur.r + i;
      const r = d < rowCount ? dataIndex(d) : -1;
      if (r < 0) {
        const row: NewRow = cols.map(() => undefined);
        cells.forEach((v, j) => {
          if (cur.c + j < cols.length) row[cur.c + j] = v;
        });
        setAdded((a) => [...a, row]);
      } else cells.forEach((v, j) => cur.c + j < cols.length && setCell(r, cur.c + j, v));
    });
  };
  return { copySelection, copyInList, paste };
}
