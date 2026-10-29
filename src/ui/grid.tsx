/**
 * Cell pieces of the result grid (ResultGrid): header row · header cell · row · row number · data cell · edit cell.
 * Virtual scrolling, selection and edit state live in ResultGrid; only the look is defined here. Spec: README.md
 */
import { CSSProperties, FocusEvent, KeyboardEvent, MouseEvent, ReactNode } from "react";
import { cx } from "./cx";
import "./grid.css";

type Mouse = (e: MouseEvent) => void;

/** Top header row (sticks while scrolling) */
export const GridHeaderRow = ({ width, height, children }: { width: number; height: number; children: ReactNode }) => (
  <div className="grid-header" style={{ width, height }} role="row">
    {children}
  </div>
);

/**
 * Header cell: 🔑 name type ……… sort button (↑ ↓ · order number if several).
 * sort = current sort (if none, ⇅ only on hover)
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
}: {
  name: string;
  type: string;
  pk?: boolean;
  width: number;
  colSelected?: boolean;
  sort?: { dir: "asc" | "desc"; order?: number };
  onMouseDown?: Mouse;
  onSortClick?: (e: MouseEvent) => void;
}) {
  return (
    <div className={cx("cell head", sort && "sorted", colSelected && "col-sel")} style={{ width }} title={`${name} : ${type}`} role="columnheader" onMouseDown={onMouseDown}>
      {pk && <span className="pk-mark">🔑</span>}
      <span className="col-name">{name}</span>
      <span className="col-type">{type.toLowerCase()}</span>
      {onSortClick && (
        <span className={cx("sort-btn", sort && "on")} title="Sort (⌥+click: add to sorting)" onMouseDown={(e) => e.stopPropagation()} onClick={onSortClick}>
          {sort ? (sort.dir === "asc" ? "↑" : "↓") : "⇅"}
          {sort?.order !== undefined && <sub>{sort.order}</sub>}
        </span>
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

/** Data cell. kind = null (<null> · italic) · num (right-aligned · number color) */
export function GridCell({
  children,
  width,
  kind,
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
      className={cx("cell", kind, selected && "sel", active && "active", changed && "changed", match && "match")}
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
      <input autoFocus className="cell-input" value={value} onChange={(e) => onChange(e.target.value)} onKeyDown={onKeyDown} />
      {more && (
        <button className="cell-more" title={more.title} onMouseDown={(e) => e.preventDefault()} onClick={more.onClick}>
          …
        </button>
      )}
      {children}
    </span>
  );
}
