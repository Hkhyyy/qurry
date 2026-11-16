import { type Dispatch, type RefObject, type SetStateAction, useEffect, useRef, useState } from "react";
import type { Cell, QueryResult } from "../../lib/api";
import { raw } from "../../lib/extract";
import type { Pos } from "../../lib/gridSelection";
import { draftToApply, leftDraft, panelText, registerValuePanel, type PanelDraft } from "../../lib/valuePanel";
import { cellKey, editableValue } from "./shared";

/** The result grid's Value Editor side panel: the cell it shows, its draft applied on leaving, open / apply / close */
export function useValuePanel({
  active, rowCount, cols, valueAt, cellOk, baseCount, setCell, draft, setDraft, setPanelError, panel, setPanel, setPanelFocus, focusGrid, wrapRef,
  cutAt, canFetch, cutReason, fetchCells,
}: {
  active: Pos | null;
  rowCount: number;
  cols: QueryResult["columns"];
  valueAt: (r: number, c: number) => Cell;
  cellOk: (r: number, c: number) => boolean;
  baseCount: number;
  setCell: (r: number, c: number, next: Cell) => void;
  draft: PanelDraft | null;
  setDraft: Dispatch<SetStateAction<PanelDraft | null>>;
  setPanelError: Dispatch<SetStateAction<string | null>>;
  panel: boolean;
  setPanel: Dispatch<SetStateAction<boolean>>;
  setPanelFocus: Dispatch<SetStateAction<number>>;
  focusGrid: () => void;
  wrapRef: RefObject<HTMLDivElement | null>;
  /** Cut data cell (r, c), whether row r's can be read whole and why not, and the read (useGridEdits) */
  cutAt: (r: number, c: number) => boolean;
  canFetch: (r: number) => boolean;
  cutReason: (r: number, c: number) => string;
  fetchCells: (cells: [number, number][]) => Promise<QueryResult | undefined>;
}) {
  // ─── Value Editor panel (follows the current cell) ───
  const panelCell = active && active.r < rowCount ? active : null;
  // A cut cell shows its cut text read-only while its whole value is read (then the whole value, editable as any), or for good with
  // why when its row has no key. A read that failed (said in the status bar) isn't tried again for that cell in this grid
  const [failed, setFailed] = useState<Set<string>>(new Set());
  useEffect(() => setFailed(new Set()), [cols]);
  const panelCut = !!panelCell && cutAt(panelCell.r, panelCell.c);
  const panelNote = !panelCell || !panelCut
    ? undefined
    : !canFetch(panelCell.r)
      ? cutReason(panelCell.r, panelCell.c)
      : failed.has(cellKey(panelCell.r, panelCell.c))
        ? "The whole value couldn't be read (see the status bar): this is its start"
        : "Loading the whole value…";
  const fetchable = panel && panelCut && canFetch(panelCell!.r) && !failed.has(cellKey(panelCell!.r, panelCell!.c));
  useEffect(() => {
    if (!fetchable || !panelCell) return;
    const { r, c } = panelCell;
    fetchCells([[r, c]]).then((res) => {
      if (!res || res.cut?.some(([x, y]) => x === r && y === c)) setFailed((f) => new Set(f).add(cellKey(r, c)));
    });
  }, [fetchable, panelCell?.r, panelCell?.c]);
  const panelReadOnly = (r: number, c: number) => !cellOk(r, c) || cutAt(r, c) || (r < baseCount && !editableValue(valueAt(r, c), cols[c].typeName));
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
  return { panelCell, panelReadOnly, panelShown, panelNote, openPanel, applyPanel, closePanel };
}
