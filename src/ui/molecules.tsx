/**
 * 분자 (molecules): 원자 몇 개를 묶은 한 줄 · 한 칸 — 목록 한 줄, 상세 줄, 메뉴 항목, 폼 칸, 탭, 안내, 구역 제목.
 * 규격은 README.md
 */
import { HTMLAttributes, InputHTMLAttributes, MouseEvent, ReactNode, useEffect, useState } from "react";
import { Avatar, Kbd, Radio, Swatch, Text, TextInput } from "./atoms";
import { cx } from "./cx";
import { Chevron } from "./icons";

// ───────── 폼 ─────────
/** 라벨 + 칸 한 줄. 힌트 · 오류는 칸 아래 */
export function Field({ label, hint, error, children, className }: { label: ReactNode; hint?: ReactNode; error?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cx("ui-field", className)}>
      <span className="ui-field-label" title={typeof label === "string" ? label : undefined}>
        {label}
      </span>
      {children}
      {hint && <span className="ui-field-hint">{hint}</span>}
      {error && <span className="ui-field-error">{error}</span>}
    </label>
  );
}
/** 여럿 중 하나 (라디오). 선택지마다 짧은 설명 (hint) */
export function RadioGroup<T extends string | number>({ value, onChange, options, row }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; hint?: ReactNode; disabled?: boolean }[]; row?: boolean }) {
  return (
    <div className={cx("ui-radio-group", row && "row")} role="radiogroup">
      {options.map((o) => (
        <Radio key={String(o.value)} checked={value === o.value} disabled={o.disabled} onChange={() => onChange(o.value)}>
          {o.label}
          {o.hint && <Text tone="muted"> {o.hint}</Text>}
        </Radio>
      ))}
    </div>
  );
}
/** 폼 · 창 안의 구역 제목 (줄 전체) */
export const FormSection = ({ children }: { children: ReactNode }) => <div className="ui-form-section">{children}</div>;
/** 칸 아래 · 창 아래의 작은 회색 설명 */
export const Hint = ({ children, className }: { children: ReactNode; className?: string }) => <div className={cx("ui-hint", className)}>{children}</div>;
/** 색 고르기 (Data Source 색). value = null 은 색 없음 */
export function ColorPicker({ value, options, onChange, custom }: { value: string | null; options: { name: string; value: string | null }[]; onChange: (v: string | null) => void; custom?: boolean }) {
  const isPreset = options.some((c) => c.value?.toLowerCase() === value?.toLowerCase());
  return (
    <div className="ui-color-picker">
      {options.map((c) => (
        <Swatch key={c.name} color={c.value} title={c.name} selected={c.value?.toLowerCase() === value?.toLowerCase()} onClick={() => onChange(c.value)} />
      ))}
      {custom && <CustomColor value={value && !isPreset ? value : null} onChange={onChange} />}
    </div>
  );
}

const HEX = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i;
/** #abc · abc → #aabbcc (아니면 null) */
export function normalizeHex(s: string): string | null {
  const m = HEX.exec(s.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((x) => x + x).join("") : m[1];
  return `#${h.toLowerCase()}`;
}

/** 직접 고르기: 시스템 색 고르개 (macOS 는 화면 픽셀을 찍는 스포이드 포함) + hex 입력 */
function CustomColor({ value, onChange }: { value: string | null; onChange: (v: string) => void }) {
  const [text, setText] = useState(value ?? "");
  useEffect(() => setText((t) => (normalizeHex(t) === value ? t : value ?? "")), [value]);
  // 맞는 hex 면 치는 대로 바로 적용 (OK 를 바로 눌러도 들어가게), 틀린 채로 나가면 되돌린다
  const type = (t: string) => {
    setText(t);
    const h = normalizeHex(t);
    if (h && h !== value) onChange(h);
  };
  return (
    <>
      <label className={cx("ui-swatch clickable ui-swatch-custom", value && "on")} title="Custom color… (system color picker — on macOS its eyedropper picks any pixel on screen)" style={value ? { background: value } : undefined}>
        <input type="color" value={value ?? "#3a3d44"} onChange={(e) => onChange(e.target.value)} />
      </label>
      <TextInput
        className="ui-color-hex"
        value={text}
        placeholder="#hex"
        spellCheck={false}
        onChange={(e) => type(e.target.value)}
        onBlur={() => !normalizeHex(text) && setText(value ?? "")}
      />
    </>
  );
}

/**
 * 찾기 칸: ⌕ + 입력 (+ 불러오는 중 ◌). bare = 테두리 없이 크게 (팝오버 머리 — Search Everywhere)
 */
export function SearchField({ busy, bare, className, ...rest }: InputHTMLAttributes<HTMLInputElement> & { busy?: boolean; bare?: boolean }) {
  return (
    <div className={cx("ui-search", bare && "bare", className)}>
      <span className="ui-search-glass">⌕</span>
      <TextInput spellCheck={false} {...rest} />
      {busy && (
        <span className="ui-search-busy" title="Loading…">
          ◌
        </span>
      )}
    </div>
  );
}

// ───────── 탭 · 제목 ─────────
export function Tabs<T extends string>({ items, value, onChange, size }: { items: { id: T; label: ReactNode }[]; value: T; onChange: (id: T) => void; size?: "sm" }) {
  return (
    <div className={cx("ui-tabs", size === "sm" && "size-sm")} role="tablist">
      {items.map((t) => (
        <button key={t.id} type="button" role="tab" aria-selected={value === t.id} className={cx("ui-tab", value === t.id && "active")} onClick={() => onChange(t.id)}>
          {t.label}
        </button>
      ))}
    </div>
  );
}
/** 목록 · 창 안의 작은 구역 제목 (Actions · Files · Format · To …) */
export const SectionTitle = ({ children, className }: { children: ReactNode; className?: string }) => <div className={cx("ui-section-title", className)}>{children}</div>;

// ───────── 안내 ─────────
export function Notice({ tone, children, className }: { tone?: "warn" | "danger" | "ok"; children: ReactNode; className?: string }) {
  return <div className={cx("ui-notice", tone && `tone-${tone}`, className)}>{children}</div>;
}
export function EmptyState({ title, children, action }: { title?: ReactNode; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="ui-empty">
      {title && <div className="ui-empty-title">{title}</div>}
      {children}
      {action && <div>{action}</div>}
    </div>
  );
}
/** 키 안내 줄: "↩ Open · ⇥ Next tab" — 키는 Kbd, 사이는 · */
export function KeyHints({ items, className }: { items: [string, string][]; className?: string }) {
  return (
    <div className={cx("ui-key-hints", className)}>
      {items.map(([k, label], i) => (
        <span key={i}>
          <Kbd>{k}</Kbd> {label}
        </span>
      ))}
    </div>
  );
}

// ───────── 목록 한 줄 ─────────
/**
 * 목록의 한 줄 (검색 결과 · 히스토리 · 전환기 · 마이그레이션 · RAG 인덱스 …).
 * [색 띠] [아이콘] 제목 설명 ……… 오른쪽
 */
export function ListItem({
  title,
  icon,
  leading,
  subtitle,
  color,
  hint,
  trailing,
  selected,
  dim,
  mono,
  title2,
  onClick,
  onDoubleClick,
  onMouseEnter,
  onContextMenu,
  className,
}: {
  title: ReactNode;
  icon?: ReactNode;
  /** 맨 앞 (체크박스 등) */
  leading?: ReactNode;
  /** 두 번째 줄 (작은 회색) */
  subtitle?: ReactNode;
  /** Data Source 색 띠 */
  color?: string | null;
  hint?: ReactNode;
  trailing?: ReactNode;
  selected?: boolean;
  /** 열려 있지 않은 것 · 비활성 */
  dim?: boolean;
  mono?: boolean;
  /** 마우스를 올리면 보일 전체 제목 */
  title2?: string;
  onClick?: (e: MouseEvent) => void;
  onDoubleClick?: (e: MouseEvent) => void;
  onMouseEnter?: () => void;
  onContextMenu?: (e: MouseEvent) => void;
  className?: string;
}) {
  return (
    <div
      className={cx("ui-list-item", selected && "selected", dim && "dim", className)}
      role="option"
      aria-selected={selected}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onMouseEnter={onMouseEnter}
      onContextMenu={onContextMenu}
    >
      {leading}
      {color !== undefined && <span className="ui-list-stripe" style={color ? { background: color } : undefined} />}
      {icon && <span className="ui-list-icon">{icon}</span>}
      {subtitle ? (
        <span className="ui-list-two">
          <span className={cx("ui-list-title", mono && "mono")} title={title2 ?? (typeof title === "string" ? title : undefined)}>
            {title}
          </span>
          <span className="ui-list-subtitle">{subtitle}</span>
        </span>
      ) : (
        <span className={cx("ui-list-title", mono && "mono")} title={title2 ?? (typeof title === "string" ? title : undefined)}>
          {title}
        </span>
      )}
      {hint && <span className="ui-list-hint">{hint}</span>}
      {trailing && <span className="ui-list-trailing">{trailing}</span>}
    </div>
  );
}

/** 상세 한 줄: 코드 (문장 · 쿼리) + 오른쪽 값 (행 수 · pid · 버전). tone 이면 값 색 */
export function DetailRow({ children, value, tone }: { children: ReactNode; value?: ReactNode; tone?: "danger" | "warn" }) {
  return (
    <div className="ui-detail-row">
      <code className="ui-detail-text">{children}</code>
      {value !== undefined && <span className={cx("ui-detail-value", tone && `tone-${tone}`)}>{value}</span>}
    </div>
  );
}

// ───────── 메뉴 항목 ─────────
/** 메뉴 한 줄: 이름 ……… 단축키 (하위 메뉴면 ▸) */
export function MenuItem({ label, shortcut, danger, disabled, hasSubmenu, children, onClick, onMouseEnter }: { label: ReactNode; shortcut?: string; danger?: boolean; disabled?: boolean; hasSubmenu?: boolean; children?: ReactNode; onClick?: () => void; onMouseEnter?: () => void }) {
  return (
    <div
      className={cx("ui-menu-item", danger && "danger", disabled && "disabled")}
      role="menuitem"
      aria-disabled={disabled}
      onMouseEnter={onMouseEnter}
      onClick={() => !disabled && onClick?.()}
    >
      <span className="ui-menu-label">{label}</span>
      {shortcut && <Kbd className="ui-menu-key">{shortcut}</Kbd>}
      {hasSubmenu && <span className="ui-menu-arrow">▸</span>}
      {children}
    </div>
  );
}
export const MenuSeparator = () => <div className="ui-menu-sep" role="separator" />;
/** 메뉴 안의 흐린 설명 줄 (누를 수 없음) */
export const MenuLabel = ({ children }: { children: ReactNode }) => <div className="ui-menu-label-row">{children}</div>;

// ───────── 상태 줄 ─────────
/** 상태 줄 오른쪽의 작은 칸 (1:20 · UTF-8 · 🔒 Read-only · 업데이트) */
export function StatusItem({ children, title, onClick, accent }: { children: ReactNode; title?: string; onClick?: () => void; accent?: boolean }) {
  return onClick ? (
    <button type="button" className={cx("ui-status-item", "clickable", accent && "accent")} title={title} onClick={onClick}>
      {children}
    </button>
  ) : (
    <span className={cx("ui-status-item", accent && "accent")} title={title}>
      {children}
    </span>
  );
}

// ───────── 트리 한 줄 ─────────
/**
 * 트리의 한 줄 (Database Explorer · 스키마 고르기): 들여쓰기 ▸ 아이콘 이름 [배지] 설명.
 * ▸ 를 누르면 onToggle, 줄을 누르면 onSelect, 두 번 누르면 onDoubleClick (없으면 onToggle)
 */
export function TreeRow({
  depth,
  open,
  leaf,
  icon,
  iconClass,
  label,
  sub,
  badge,
  trailing,
  leading,
  selected,
  title,
  className,
  style,
  dataNode,
  onSelect,
  onToggle,
  onDoubleClick,
  onContextMenu,
  onClick,
}: {
  depth: number;
  open?: boolean;
  leaf?: boolean;
  icon?: ReactNode;
  iconClass?: string;
  label: ReactNode;
  sub?: ReactNode;
  badge?: ReactNode;
  /** 오른쪽 끝 (TTL · 개수) */
  trailing?: ReactNode;
  /** 아이콘 앞 (체크박스) */
  leading?: ReactNode;
  selected?: boolean;
  title?: string;
  className?: string;
  style?: React.CSSProperties;
  /** Select In Explorer 가 찾는 열쇠 */
  dataNode?: string;
  onSelect?: () => void;
  onToggle?: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (e: MouseEvent) => void;
  onClick?: () => void;
}) {
  return (
    <div
      onClick={onClick}
      className={cx("tree-node", selected && "selected", className)}
      style={{ paddingLeft: 4 + depth * 19, ...style }}
      data-node={dataNode}
      title={title}
      role="treeitem"
      aria-expanded={leaf ? undefined : !!open}
      aria-selected={selected}
      onMouseDown={onSelect}
      onDoubleClick={onDoubleClick ?? onToggle}
      onContextMenu={onContextMenu}
    >
      <span
        className="twisty"
        data-open={leaf ? "" : open ? "1" : "0"}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          onSelect?.();
          onToggle?.();
        }}
      >
        {leaf ? null : <Chevron open={!!open} />}
      </span>
      {leading}
      {icon && <span className={cx("node-icon", iconClass)}>{icon}</span>}
      <span className="node-name">{label}</span>
      {badge}
      {sub && <span className="node-sub">{sub}</span>}
      {trailing && <span className="node-trailing">{trailing}</span>}
    </div>
  );
}

// ───────── 탭 하나 (편집기 탭 · 결과 탭) ─────────
/**
 * 닫을 수 있는 탭: [아이콘] 이름 (📌 | ×). 고정(pinned)하면 × 대신 📌 (누르면 풀기).
 * variant = editor (위쪽 편집기 탭) · result (콘솔 아래 결과 탭). 끌기 · 우클릭은 그대로 넘긴다
 */
export function TabItem({
  label,
  icon,
  iconClass,
  active,
  pinned,
  onClose,
  onPin,
  closeTitle = "Close",
  variant = "editor",
  className,
  ...rest
}: Omit<HTMLAttributes<HTMLDivElement>, "onClose"> & {
  label: ReactNode;
  icon?: ReactNode;
  iconClass?: string;
  active?: boolean;
  pinned?: boolean;
  onClose?: (e: MouseEvent) => void;
  onPin?: () => void;
  closeTitle?: string;
  variant?: "editor" | "result";
}) {
  const base = variant === "editor" ? "tab" : "rtab";
  const stop = (e: MouseEvent) => e.stopPropagation();
  return (
    <div className={cx(base, active && "active", className)} role="tab" aria-selected={active} {...rest}>
      {icon && <span className={cx("tab-icon", iconClass)}>{icon}</span>}
      <span className="tab-label">{label}</span>
      {pinned ? (
        <span className="tab-pin" title="Unpin Tab" onMouseDown={stop} onClick={(e) => (stop(e), onPin?.())}>
          📌
        </span>
      ) : (
        onClose && (
          <span className={variant === "editor" ? "tab-close" : "rclose"} title={closeTitle} onMouseDown={stop} onClick={(e) => (stop(e), onClose(e))}>
            ×
          </span>
        )
      )}
    </div>
  );
}

// ───────── 왼쪽 세로 막대 칸 (워크스페이스) ─────────
/** 세로 막대의 한 칸: 이니셜 사각형 + 작은 이름. 우클릭 · 두 번 클릭은 그대로 넘긴다 */
export function RailItem({ initials, label, active, ...rest }: HTMLAttributes<HTMLButtonElement> & { initials: string; label: string; active?: boolean }) {
  return (
    <button type="button" className={cx("ui-rail-item", active && "active")} aria-pressed={active} {...rest}>
      <Avatar active={active}>{initials}</Avatar>
      <span className="ui-rail-label">{label}</span>
    </button>
  );
}
