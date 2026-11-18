import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import { type RefObject, useEffect, useId, useMemo } from "react";
import type { ColumnLayout, QueryResult } from "../../lib/api";
import { columnView, NO_LAYOUT, useLayoutOf } from "../../lib/columnLayout";
import { widthKeys, type WidthsOf } from "../../lib/columnWidths";
import { setStatusNote } from "../../lib/statusbar";
import type { CheckItem, Pin } from "../../ui";

/**
 * The result grid's hidden and pinned columns (the header menu, Columns…; DataGrip's Hide Column and Pin Column) and where each
 * column is drawn. The grid draws display columns: `view[c]` is display column c's index in the result, the first `pinned` stay at
 * the left. A table tab's layout is saved per table (`layoutOf`, shared with the TUI), a console result's kept in memory. Sideways
 * virtualization is here too, so the pinned columns are always drawn
 */
export function useColumnLayout({ cols, widths, rownumW, scrollRef, wasmReady, layoutOf }: {
  cols: QueryResult["columns"];
  /** Pixels of each result column (useGridWidths) */
  widths: number[];
  rownumW: number;
  scrollRef: RefObject<HTMLDivElement | null>;
  wasmReady: boolean;
  layoutOf: WidthsOf | undefined;
}) {
  const own = useId();
  const { layout, set } = useLayoutOf(layoutOf ?? own, (msg) => setStatusNote(msg));
  const keys = useMemo(() => widthKeys(cols.map((c) => c.name)), [cols, wasmReady]);
  const { view, pinned, hidden } = useMemo(() => columnView(keys, layout), [keys, layout, wasmReady]);
  /** Each pinned column's left edge in the row, after the row numbers and the pinned columns before it */
  const pinLeft = useMemo(() => view.slice(0, pinned).reduce<number[]>((at, i, c) => [...at, at[c] + widths[i]], [rownumW]), [view, pinned, widths, rownumW]);
  const pinOf = (c: number): Pin | undefined => (c < pinned ? { left: pinLeft[c], edge: c === pinned - 1 } : undefined);

  // Horizontally only the visible columns are drawn (tables with many columns), and always the pinned ones. A column scrolled into
  // view lands right of the pinned ones, not under them
  const colVirt = useVirtualizer({
    horizontal: true,
    count: view.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (c) => widths[view[c]] ?? 64,
    paddingStart: rownumW,
    scrollPaddingStart: pinLeft[pinned],
    overscan: 4,
    rangeExtractor: (range) => [...new Set([...Array.from({ length: pinned }, (_, c) => c), ...defaultRangeExtractor(range)])].sort((a, b) => a - b),
  });
  useEffect(() => colVirt.measure(), [widths, view]);
  const items = colVirt.getVirtualItems();
  /** The display columns drawn now: display c, result column i, the blank before it, its pin */
  const shownCols = items.map((it, k) => ({ c: it.index, i: view[it.index], pad: it.start - (k ? items[k - 1].end : rownumW), pin: pinOf(it.index) }));
  const padR = items.length ? colVirt.getTotalSize() - items[items.length - 1].end : 0;
  /** The width of a row: the row numbers and the shown columns */
  const total = colVirt.getTotalSize();
  /** Bring display column c into view (a pinned one always is) */
  const scrollToCol = (c: number) => c >= pinned && colVirt.scrollToIndex(c);

  const change = (f: (l: ColumnLayout) => ColumnLayout) => set(f(layout));
  const without = (list: string[], k: string) => list.filter((x) => x !== k);
  /** Hide result column i; the last shown column stays */
  const hide = (i: number) => view.length > 1 && change((l) => ({ ...l, hidden: [...without(l.hidden, keys[i]), keys[i]] }));
  const show = (key: string) => change((l) => ({ ...l, hidden: without(l.hidden, key) }));
  const showAll = () => change((l) => ({ ...l, hidden: [] }));
  /** Pin result column i after the pinned ones / unpin it (back to its place in the result) */
  const pin = (i: number) => change((l) => ({ ...l, pinned: [...without(l.pinned, keys[i]), keys[i]] }));
  const unpin = (i: number) => change((l) => ({ ...l, pinned: without(l.pinned, keys[i]) }));
  /** Every column shown, nothing pinned (Reset View) */
  const reset = () => (layout.hidden.length || layout.pinned.length) && set(NO_LAYOUT);
  /** Columns…: every result column in result order, checked when shown */
  const listItems = (): CheckItem[] =>
    cols.map((col, i) => ({ key: keys[i], label: col.name, hint: col.typeName.toLowerCase(), mark: layout.pinned.includes(keys[i]) ? "pinned" : undefined, checked: !layout.hidden.includes(keys[i]) }));
  const setShown = (key: string, on: boolean) => (on ? show(key) : hide(keys.indexOf(key)));

  return { view, pinned, pinEdge: pinLeft[pinned], hidden, colVirt, shownCols, padR, total, pinOf, scrollToCol, hide, show, showAll, pin, unpin, reset, listItems, setShown };
}
