/**
 * 결과 표 (ResultGrid) 의 칸 조각: 머리 줄 · 머리 칸 · 행 · 행 번호 · 데이터 칸 · 편집 칸.
 * 가상 스크롤 · 선택 · 편집 상태는 ResultGrid 가 갖고, 모양은 여기서만 정한다. 규격은 README.md
 */
import { CSSProperties, FocusEvent, KeyboardEvent, MouseEvent, ReactNode } from "react";
import { cx } from "./cx";
import "./grid.css";

type Mouse = (e: MouseEvent) => void;

/** 맨 위 머리 줄 (스크롤해도 붙어 있다) */
export const GridHeaderRow = ({ width, height, children }: { width: number; height: number; children: ReactNode }) => (
  <div className="grid-header" style={{ width, height }} role="row">
    {children}
  </div>
);

/**
 * 머리 칸: 🔑 이름 타입 ……… 정렬 버튼 (↑ ↓ · 여럿이면 순번).
 * sort = 지금 정렬 (없으면 마우스를 올릴 때만 ⇅)
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

/** 한 행 (가상 스크롤 위치 top). added · deleted 는 아직 저장 안 한 변경 */
export const GridRow = ({ top, height, width, odd, added, deleted, children }: { top: number; height: number; width: number; odd?: boolean; added?: boolean; deleted?: boolean; children: ReactNode }) => (
  <div className={cx("grid-row", odd && "odd", added && "added", deleted && "deleted")} style={{ top, height, width }} role="row">
    {children}
  </div>
);

/** 왼쪽 행 번호 칸 (누르면 그 행 선택). 머리 줄의 빈 칸 (전체 선택) 에도 쓴다 */
export const RowNumberCell = ({ children, width, selected, title, onMouseDown }: { children?: ReactNode; width: number; selected?: boolean; title?: string; onMouseDown?: Mouse }) => (
  <div className={cx("cell rownum", selected && "row-sel")} style={{ width }} title={title} onMouseDown={onMouseDown} role="rowheader">
    {children}
  </div>
);

/** 데이터 칸. kind = null (<null> · 기울임) · num (오른쪽 정렬 · 숫자색) */
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
  /** 커서가 있는 칸 */
  active?: boolean;
  /** 저장 안 한 변경 */
  changed?: boolean;
  /** 찾기 결과 */
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

/** 칸 편집: 입력칸 (+ 날짜 고르기 버튼 · 고르는 판). 포커스가 칸 밖으로 나가면 onBlurOut */
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
  /** 오른쪽 … 버튼 (날짜 · 시간 고르기) */
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
