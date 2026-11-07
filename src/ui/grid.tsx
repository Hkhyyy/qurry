/**
 * Cell pieces of the result grid (ResultGrid): header row · header cell · row · row number · data cell · edit cell.
 * Virtual scrolling, selection and edit state live in ResultGrid; only the look is defined here. Spec: README.md
 */
import { CSSProperties, FocusEvent, KeyboardEvent, MouseEvent, ReactNode, RefObject, useEffect, useRef } from "react";
import { textAssistOff } from "./atoms";
import { cx } from "./cx";
import "./grid.css";

type Mouse = (e: MouseEvent) => void;

/**
 * Header strip above the grid's scroller (body). It sits outside the scroller, so vertical scrolling can't move it (a sticky
 * header inside the scroller could lag WKWebView's async scrolling and show rows above it); it only follows the body's
 * scrollLeft. Wheel over it scrolls the body; a click keeps the focus on the body, so keyboard navigation keeps working.
 */
export function GridHead({ body, children }: { body: RefObject<HTMLElement | null>; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  // After commit (not a layout effect): the body may come after the head, and its ref is attached later in the same commit
  useEffect(() => {
    const el = body.current;
    if (!el) return;
    const sync = () => {
      if (ref.current) ref.current.scrollLeft = el.scrollLeft;
    };
    sync();
    el.addEventListener("scroll", sync, { passive: true });
    return () => el.removeEventListener("scroll", sync);
  }, [body]);
  return (
    <div
      ref={ref}
      className="grid-head"
      onWheel={(e) => body.current?.scrollBy(e.deltaX, e.deltaY)}
      onMouseDownCapture={(e) => {
        e.preventDefault();
        body.current?.focus({ preventScroll: true });
      }}
    >
      {children}
    </div>
  );
}

/** Header row (inside GridHead) */
export const GridHeaderRow = ({ width, height, children }: { width: number; height: number; children: ReactNode }) => (
  <div className="grid-header" style={{ width, height }} role="row">
    {children}
  </div>
);

/**
 * Header cell: 🔑 name type ……… sort button (↑ ↓ · order number if several).
 * sort = current sort (if none, ⇅ only on hover). With onResize the right edge is a resize handle: drag it to resize,
 * double-click it to fit (onFit)
 */
export function GridHeaderCell({
  name,
  type,
  pk,
  width,
  colSelected,
  sort,
  onMouseDown,
  onSortClick,
  onContextMenu,
  onResize,
  onFit,
}: {
  name: string;
  type: string;
  pk?: boolean;
  width: number;
  colSelected?: boolean;
  sort?: { dir: "asc" | "desc"; order?: number };
  onMouseDown?: Mouse;
  onSortClick?: (e: MouseEvent) => void;
  onContextMenu?: Mouse;
  onResize?: Mouse;
  onFit?: () => void;
}) {
  return (
    <div className={cx("cell head", sort && "sorted", colSelected && "col-sel")} style={{ width }} title={`${name} : ${type}`} role="columnheader" onMouseDown={onMouseDown} onContextMenu={onContextMenu}>
      {pk && <span className="pk-mark">🔑</span>}
      <span className="col-name">{name}</span>
      <span className="col-type">{type.toLowerCase()}</span>
      {onSortClick && (
        <span className={cx("sort-btn", sort && "on")} title="Sort (⌥+click: add to sorting)" onMouseDown={(e) => e.stopPropagation()} onClick={onSortClick}>
          {sort ? (sort.dir === "asc" ? "↑" : "↓") : "⇅"}
          {sort?.order !== undefined && <sub>{sort.order}</sub>}
        </span>
      )}
      {onResize && (
        <span
          className="col-resize"
          title="Drag to resize · double-click to fit the content"
          onMouseDown={(e) => {
            e.stopPropagation();
            onResize(e);
          }}
          onDoubleClick={onFit}
        />
      )}
    </div>
  );
}

/** One row (virtual scroll position top). added · deleted are unsaved changes */
export const GridRow = ({ top, height, width, odd, added, deleted, children }: { top: number; height: number; width: number; odd?: boolean; added?: boolean; deleted?: boolean; children: ReactNode }) => (
  <div className={cx("grid-row", odd && "odd", added && "added", deleted && "deleted")} style={{ top, height, width }} role="row">
    {children}
  </div>
);

/** Row number cell on the left (click selects the row). Also used for the blank header cell (select all) */
export const RowNumberCell = ({ children, width, selected, title, onMouseDown }: { children?: ReactNode; width: number; selected?: boolean; title?: string; onMouseDown?: Mouse }) => (
  <div className={cx("cell rownum", selected && "row-sel")} style={{ width }} title={title} onMouseDown={onMouseDown} role="rowheader">
    {children}
  </div>
);

/** Data cell. kind = null (<null> · italic) · num (number color); align = Settings › Appearance › Alignment for its value class */
export function GridCell({
  children,
  width,
  kind,
  align,
  selected,
  active,
  changed,
  match,
  onMouseDown,
  onMouseEnter,
  onDoubleClick,
  onContextMenu,
}: {
  children: ReactNode;
  width: number;
  kind?: "null" | "num";
  align?: "left" | "right" | "center";
  selected?: boolean;
  /** Cell with the cursor */
  active?: boolean;
  /** Unsaved change */
  changed?: boolean;
  /** Find match */
  match?: boolean;
  onMouseDown?: Mouse;
  onMouseEnter?: () => void;
  onDoubleClick?: Mouse;
  onContextMenu?: Mouse;
}) {
  return (
    <div
      className={cx("cell", kind, align && `al-${align}`, selected && "sel", active && "active", changed && "changed", match && "match")}
      style={{ width }}
      role="gridcell"
      aria-selected={selected}
      onMouseDown={onMouseDown}
      onMouseEnter={onMouseEnter}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
    >
      {children}
    </div>
  );
}

/** A JSON cell's text in colors (inside GridCell, still one line): `spans` are [start, end, kind] UTF-16 ranges in order, kind
 *  key · string · number · keyword · punct. Punctuation is the color of the span around the colored part (half the elements
 *  of a span each); text after the last piece (where it stops being JSON) stays plain */
export function JsonText({ text, spans }: { text: string; spans: [number, number, string][] }) {
  const end = spans.length ? spans[spans.length - 1][1] : 0;
  const out: ReactNode[] = [];
  let at = 0;
  for (const [s, e, kind] of spans) {
    if (kind === "punct") continue;
    if (s > at) out.push(text.slice(at, s));
    out.push(
      <span key={s} className={`json-${kind}`}>
        {text.slice(s, e)}
      </span>,
    );
    at = e;
  }
  if (end > at) out.push(text.slice(at, end));
  return (
    <>
      {end > 0 && <span className="json-punct">{out}</span>}
      {text.slice(end)}
    </>
  );
}

/** Cell editing: input (+ date picker button · picker panel). onBlurOut when focus leaves the cell */
export function CellEditor({
  width,
  value,
  onChange,
  onKeyDown,
  onBlurOut,
  more,
  children,
  style,
}: {
  width: number;
  value: string;
  onChange: (v: string) => void;
  onKeyDown: (e: KeyboardEvent<HTMLInputElement>) => void;
  onBlurOut: () => void;
  /** Right … button (date · time picker) */
  more?: { title: string; onClick: () => void };
  children?: ReactNode;
  style?: CSSProperties;
}) {
  return (
    <span
      className="cell-edit"
      style={{ width, ...style }}
      onBlur={(e: FocusEvent<HTMLSpanElement>) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node)) onBlurOut();
      }}
    >
      <input autoFocus className="cell-input" {...textAssistOff()} value={value} onChange={(e) => onChange(e.target.value)} onKeyDown={onKeyDown} />
      {more && (
        <button className="cell-more" title={more.title} onMouseDown={(e) => e.preventDefault()} onClick={more.onClick}>
          …
        </button>
      )}
      {children}
    </span>
  );
}
