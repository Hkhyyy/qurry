import type { Virtualizer } from "@tanstack/react-virtual";
import type { Dispatch, RefObject, SetStateAction } from "react";
import type { EditTarget } from "../../components/ResultGrid";
import { type Cell, type Dialect, type JoinEdit, type QueryResult } from "../../lib/api";
import { DEFAULT_TOKEN } from "../../lib/extract";
import { type Changes, editStatements, joinStatements, type NewRow, submitFix } from "../../lib/gridEdit";
import type { Pos } from "../../lib/gridSelection";
import { sharedReady } from "../../lib/shared";
import { setStatusNote, tableColumns } from "../../lib/statusbar";
import { cellKey, cutSet, display, editableValue } from "./shared";

/** A cut cell (its text is only the start of the value) takes only a value that isn't made from that text (`whole`: paste, NULL,
 *  DEFAULT, UUID); an edit of its text waits for the whole value. A cell already changed holds the change, not the cut text */
export const refusesCut = (cut: Set<string>, changes: Changes, r: number, c: number, whole: boolean) => !whole && cut.has(cellKey(r, c)) && !(changes[r] && c in changes[r]);
export const CUT_NOT_CHANGED = "Not changed: the cell holds only the start of its value. Edit it (↩) once the whole value is read";

/** The change set with data cell (r, c) set to `next`; back to the value as loaded drops the change */
export function changeCell(ch: Changes, rows: Cell[][], r: number, c: number, next: Cell): Changes {
  const orig = rows[r][c];
  if (next === orig || (orig !== null && next !== null && next !== DEFAULT_TOKEN && display(orig) === String(next))) {
    if (!ch[r] || !(c in ch[r])) return ch;
    const row = { ...ch[r] };
    delete row[c];
    const n = { ...ch };
    if (Object.keys(row).length) n[r] = row;
    else delete n[r];
    return n;
  }
  return { ...ch, [r]: { ...ch[r], [c]: next } };
}

/** The result grid's edits: a cell's value, NULL / DEFAULT / UUID, rows added, cloned and deleted, revert, and Submit */
export function useGridEdits({
  editing, setEditing, changes, setChanges, deleted, setDeleted, added, setAdded, preview, setPreview, submitErr, setSubmitErr,
  result, cols, baseCount, rowCount, join, canEdit, anyEdit, pkIdx, edit, D, tcol, valueAt, cellReason, cellOk,
  active, hasSel, selRows, forSelected, selectCell, virt, scrollRef, cut, cutAt, canFetch, cutReason, fetchCells, p,
}: {
  editing: { r: number; c: number; value: string; picker?: boolean } | null;
  setEditing: Dispatch<SetStateAction<{ r: number; c: number; value: string; picker?: boolean } | null>>;
  changes: Record<number, Record<number, Cell>>;
  setChanges: Dispatch<SetStateAction<Record<number, Record<number, Cell>>>>;
  deleted: Set<number>;
  setDeleted: Dispatch<SetStateAction<Set<number>>>;
  added: NewRow[];
  setAdded: Dispatch<SetStateAction<NewRow[]>>;
  preview: { sql: string[]; readOnly?: boolean } | null;
  setPreview: Dispatch<SetStateAction<{ sql: string[]; readOnly?: boolean } | null>>;
  submitErr: string | null;
  setSubmitErr: Dispatch<SetStateAction<string | null>>;
  result: QueryResult;
  cols: QueryResult["columns"];
  baseCount: number;
  rowCount: number;
  join: JoinEdit | undefined;
  canEdit: boolean;
  anyEdit: boolean;
  pkIdx: number[];
  edit: EditTarget | undefined;
  D: Dialect;
  tcol: (i: number) => string;
  valueAt: (r: number, c: number) => Cell;
  cellReason: (r: number, c: number) => string | undefined;
  cellOk: (r: number, c: number) => boolean;
  active: Pos | null;
  hasSel: boolean;
  selRows: () => number[];
  forSelected: (fn: (r: number, c: number) => void | false) => void;
  selectCell: (pos: Pos, ext: boolean) => void;
  virt: Virtualizer<HTMLDivElement, Element>;
  scrollRef: RefObject<HTMLDivElement | null>;
  /** The result's cut cells (cutSet), cut data cell (r, c) not changed, whether row r's can be read whole, and why not */
  cut: Set<string>;
  cutAt: (r: number, c: number) => boolean;
  canFetch: (r: number) => boolean;
  cutReason: (r: number, c: number) => string;
  /** Read cut cells whole into the result (its rows are written in place, so valueAt reads them); → the result then, if any */
  fetchCells: (cells: [number, number][]) => Promise<QueryResult | undefined>;
  p: { onSubmit: (statements: string[]) => Promise<void>; target?: string };
}) {
  // ─── Editing ───
  const commitEdit = () => {
    if (!editing) return;
    const { r, c, value } = editing;
    setCell(r, c, value);
    setEditing(null);
    scrollRef.current?.focus();
  };
  /** whole = a value not made from the cell's text (paste, NULL, DEFAULT, UUID), which a cut cell takes too (refusesCut) */
  const setCell = (r: number, c: number, next: Cell, whole = false) => {
    if (r >= baseCount) {
      setAdded((a) => a.map((row, i) => (i === r - baseCount ? row.map((v, j) => (j === c ? next : v)) : row)));
      return;
    }
    if (refusesCut(cut, changes, r, c, whole)) return void setStatusNote(CUT_NOT_CHANGED);
    setChanges((ch) => changeCell(ch, result.rows, r, c, next));
  };
  /** Read row r's cut cells `cs` whole first (a row without a key says why instead): → whether they are whole now */
  const wholeFirst = async (r: number, cs: number[]): Promise<boolean> => {
    if (!cs.length) return true;
    if (!canFetch(r)) return void setStatusNote(cutReason(r, cs[0])), false;
    const res = await fetchCells(cs.map((c) => [r, c]));
    const still = cutSet(res?.cut);
    return !!res && cs.every((c) => !still.has(cellKey(r, c)));
  };
  /** ↩, a double-click, typing (`typed`: the editor starts with it): a cut cell opens with its whole value, read first */
  const startEdit = async (r: number, c: number, typed?: string) => {
    if (deleted.has(r)) return;
    const why = cellReason(r, c);
    // A join's read-only cell says why (one table's whole grid says it on its buttons)
    if (why !== undefined) return void (join && why && setStatusNote(why));
    if (r < baseCount && !editableValue(valueAt(r, c), cols[c].typeName)) return;
    if (cutAt(r, c) && !(await wholeFirst(r, [c]))) return;
    const v = valueAt(r, c);
    setEditing({ r, c, value: typed ?? (v === null || v === DEFAULT_TOKEN ? "" : String(v)) });
  };

  /** Every selected cell that can be edited gets `v()` (a join's read-only cells keep theirs) */
  const setSelected = (v: () => Cell) => anyEdit && forSelected((r, c) => void (cellOk(r, c) && setCell(r, c, v(), true)));
  const setNull = () => setSelected(() => null);
  const setDefault = () => setSelected(() => DEFAULT_TOKEN);
  const genUuid = () => setSelected(() => crypto.randomUUID());

  const addRow = () => {
    if (!canEdit) return;
    setAdded((a) => [...a, cols.map(() => undefined)]);
    setTimeout(() => {
      virt.scrollToIndex(rowCount);
      selectCell({ r: rowCount, c: 0 }, false);
    });
  };
  const cloneRow = async () => {
    if (!canEdit || !active) return;
    // A cut source cell is read whole first: the new row never gets the cut text
    if (active.r < baseCount && !(await wholeFirst(active.r, cols.map((_, c) => c).filter((c) => !pkIdx.includes(c) && cutAt(active.r, c))))) return;
    // Key cells are left untouched; the others keep the source's value, NULL included (a new row's untouched cell stays untouched)
    const src = cols.map((_, c) => (pkIdx.includes(c) ? undefined : active.r < baseCount ? valueAt(active.r, c) : added[active.r - baseCount][c]));
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

  /** Submit's statements (lib/gridEdit: the TUI's rules), each ending with `;` */
  const buildStatements = (): string[] | undefined => {
    if (!sharedReady()) return void setStatusNote("Submit needs WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk");
    if (join) return joinStatements(D.quoting, join, result.rows, changes);
    if (!edit) return [];
    return editStatements(D.quoting, { ...edit, columns: cols.map((_, i) => tcol(i)) }, result.rows, changes, deleted, added);
  };
  const submit = async () => {
    const sql = buildStatements();
    if (!sql?.length) return;
    setSubmitErr(null);
    try {
      await p.onSubmit(sql);
    } catch (e) {
      setPreview({ sql });
      setSubmitErr(String(e));
    }
  };
  // Under the error: the SQL that fixes a key sequence behind the table (#167), from the table's columns (re-renders when they load)
  const fix = submitErr && edit && !join ? submitFix(D.quoting, submitErr, edit, () => (p.target ? tableColumns(p.target, edit.schema, edit.table) : undefined)) : undefined;
  const previewPending = () => {
    const sql = buildStatements();
    if (sql) setPreview({ sql, readOnly: true });
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

  return { commitEdit, setCell, startEdit, setNull, setDefault, genUuid, addRow, cloneRow, deleteRows, revertSelected, submit, fix, previewPending, doSubmitPreview };
}
