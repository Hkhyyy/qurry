import type { Virtualizer } from "@tanstack/react-virtual";
import type { Dispatch, RefObject, SetStateAction } from "react";
import type { EditTarget } from "../../components/ResultGrid";
import { type Cell, type Dialect, type JoinEdit, type QueryResult } from "../../lib/api";
import { DEFAULT_TOKEN } from "../../lib/extract";
import { editStatements, joinStatements, type NewRow, submitFix } from "../../lib/gridEdit";
import type { Pos } from "../../lib/gridSelection";
import { sharedReady } from "../../lib/shared";
import { setStatusNote, tableColumns } from "../../lib/statusbar";
import { display, editableValue } from "./shared";

/** The result grid's edits: a cell's value, NULL / DEFAULT / UUID, rows added, cloned and deleted, revert, and Submit */
export function useGridEdits({
  editing, setEditing, changes, setChanges, deleted, setDeleted, added, setAdded, preview, setPreview, submitErr, setSubmitErr,
  result, cols, baseCount, rowCount, join, canEdit, anyEdit, pkIdx, edit, D, tcol, valueAt, cellReason, cellOk,
  active, hasSel, selRows, forSelected, selectCell, virt, scrollRef, p,
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
    if (deleted.has(r)) return;
    const why = cellReason(r, c);
    // A join's read-only cell says why (one table's whole grid says it on its buttons)
    if (why !== undefined) return void (join && why && setStatusNote(why));
    const v = valueAt(r, c);
    if (r < baseCount && !editableValue(v, cols[c].typeName)) return;
    setEditing({ r, c, value: v === null || v === DEFAULT_TOKEN ? "" : String(v) });
  };

  /** Every selected cell that can be edited gets `v()` (a join's read-only cells keep theirs) */
  const setSelected = (v: () => Cell) => anyEdit && forSelected((r, c) => void (cellOk(r, c) && setCell(r, c, v())));
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
  const cloneRow = () => {
    if (!canEdit || !active) return;
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
