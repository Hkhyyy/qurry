import type { Virtualizer } from "@tanstack/react-virtual";
import { type Dispatch, type RefObject, type SetStateAction, useMemo } from "react";
import type { Cell, QueryResult } from "../../lib/api";
import type { NewRow } from "../../lib/gridEdit";
import type { Pos } from "../../lib/gridSelection";
import { display } from "./shared";

/** The result grid's find bar (Find on Current Page): the matching cells and going from one to the next */
export function useGridFind({
  findOpen, setFindOpen, findText, rowCount, dataIndex, view, valueAt, result, changes, added, order, cur, selectCell, virt, scrollToCol, wrapRef,
}: {
  findOpen: boolean;
  setFindOpen: Dispatch<SetStateAction<boolean>>;
  findText: string;
  rowCount: number;
  dataIndex: (d: number) => number;
  /** Display column → result column: the shown columns are searched (useColumnLayout) */
  view: number[];
  valueAt: (r: number, c: number) => Cell;
  result: QueryResult;
  changes: Record<number, Record<number, Cell>>;
  added: NewRow[];
  order: number[] | null;
  cur: Pos | null;
  selectCell: (pos: Pos, ext: boolean) => void;
  virt: Virtualizer<HTMLDivElement, Element>;
  scrollToCol: (c: number) => void;
  wrapRef: RefObject<HTMLDivElement | null>;
}) {
  // ─── Find (Find on Current Page) ───
  const findLower = findText.toLowerCase();
  const matches = useMemo(() => {
    if (!findOpen || !findLower) return [] as Pos[];
    const out: Pos[] = [];
    for (let d = 0; d < rowCount && out.length < 5000; d++) {
      const r = dataIndex(d);
      for (let c = 0; c < view.length; c++) if (display(valueAt(r, view[c])).toLowerCase().includes(findLower)) out.push({ r: d, c });
    }
    return out;
  }, [findOpen, findLower, result.rows, changes, added, order, view]);
  /** Whether display row d, display column c matches */
  const isMatch = (d: number, c: number) => findOpen && !!findLower && display(valueAt(dataIndex(d), view[c])).toLowerCase().includes(findLower);
  const gotoMatch = (dir: 1 | -1) => {
    if (!matches.length) return;
    const at = cur ? matches.findIndex((m) => m.r === cur.r && m.c === cur.c) : -1;
    const m = matches[(at + dir + matches.length) % matches.length];
    selectCell(m, false);
    virt.scrollToIndex(m.r);
    scrollToCol(m.c);
  };
  /** The find bar, opened and focused (its key, DataGrip); the toolbar button toggles it */
  const openFind = () => {
    setFindOpen(true);
    setTimeout(() => wrapRef.current?.querySelector<HTMLInputElement>(".grid-find input")?.focus());
  };
  return { matches, isMatch, gotoMatch, openFind };
}
