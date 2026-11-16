import { type Dispatch, type SetStateAction, useEffect, useMemo } from "react";
import type { Cell, QueryResult } from "../../lib/api";
import type { SortKey } from "./shared";

/** The result grid's sort: header clicks, on the server (ORDER BY) or over the loaded rows */
export function useSortOrder({ cols, result, sorts, setSorts, serverSort, quoteIdent, p }: {
  cols: QueryResult["columns"];
  result: QueryResult;
  sorts: SortKey[];
  setSorts: Dispatch<SetStateAction<SortKey[]>>;
  serverSort: boolean;
  quoteIdent: (name: string) => string;
  p: { onServerSort?: (orderBy: string) => void };
}) {
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
  return { clickSort, order, dataIndex };
}
