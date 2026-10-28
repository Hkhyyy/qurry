/**
 * 원자 (atoms): 더 쪼갤 수 없는 조각 — 글자 · 코드 · 단축키 · 버튼 · 입력칸 · 배지 · 구분선 · 색 견본 · 줄 배치.
 * 화면 코드는 <span className="muted"> 같은 맨 HTML 대신 이것을 쓴다. 규격은 README.md
 */
import { ButtonHTMLAttributes, CSSProperties, forwardRef, HTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from "react";
import { cx } from "./cx";

// ───────── 글자 ─────────
export type Tone = "default" | "muted" | "subtle" | "accent" | "ok" | "warn" | "danger";
/** 글자 한 조각. tone = 색, size = xs 11 · sm 12 · md 13 · lg 14, mono = 고정폭, truncate = 넘치면 … (title 로 전체) */
export function Text({
  children,
  tone = "default",
  size,
  mono,
  strong,
  truncate,
  title,
  className,
  as: As = "span",
  style,
}: {
  children?: ReactNode;
  tone?: Tone;
  size?: "xs" | "sm" | "md" | "lg";
  mono?: boolean;
  strong?: boolean;
  truncate?: boolean;
  title?: string;
  className?: string;
  as?: "span" | "div";
  style?: CSSProperties;
}) {
  return (
    <As
      className={cx("ui-text", tone !== "default" && `tone-${tone}`, size && `size-${size}`, mono && "mono", strong && "strong", truncate && "ui-ellipsis", className)}
      title={title ?? (truncate && typeof children === "string" ? children : undefined)}
      style={style}
    >
      {children}
    </As>
  );
}

/** 문장 · 이름 같은 짧은 코드 (고정폭, 줄바꿈 유지) */
export const Code = ({ children, className }: { children: ReactNode; className?: string }) => <code className={cx("ui-code", className)}>{children}</code>;

/** 여러 줄 코드 (SQL 미리 보기 · DDL · 오류 원문) */
export function CodeBlock({ children, className, wrap = true, style }: { children: ReactNode; className?: string; wrap?: boolean; style?: CSSProperties }) {
  return (
    <pre className={cx("ui-code-block", !wrap && "nowrap", className)} style={style}>
      {children}
    </pre>
  );
}

/** 단축키 표기 (⌘k, ⇧⌘A …). 메뉴 · 목록 · 안내 줄에서 같은 모양 */
export const Kbd = ({ children, className }: { children: ReactNode; className?: string }) => <kbd className={cx("ui-kbd", className)}>{children}</kbd>;

// ───────── 배치 ─────────
/** 가로로 늘어놓기 (가운데 정렬). gap = 1~6 (토큰 간격) */
export function Row({ children, gap = 4, wrap, className, style, ...rest }: HTMLAttributes<HTMLDivElement> & { gap?: 1 | 2 | 3 | 4 | 5 | 6; wrap?: boolean }) {
  return (
    <div className={cx("ui-row", `gap-${gap}`, wrap && "wrap", className)} style={style} {...rest}>
      {children}
    </div>
  );
}
/** 남는 폭을 차지해 뒤를 오른쪽으로 민다 */
export const Spacer = () => <span className="ui-spacer" />;
/** 구분선: 세로 (도구 막대) · 가로 (메뉴 · 구역) */
export const Divider = ({ vertical }: { vertical?: boolean }) => <span className={vertical ? "ui-divider-v" : "ui-divider-h"} role="separator" />;
/** 도구 막대 묶음 사이 세로선 */
export const ToolbarSep = () => <Divider vertical />;

/** 색 견본 (Data Source 색 · 탭 띠). none = 색 없음 (빗금) */
export function Swatch({ color, selected, title, onClick, size = 18 }: { color: string | null; selected?: boolean; title?: string; onClick?: () => void; size?: number }) {
  return (
    <span
      className={cx("ui-swatch", !color && "none", selected && "on", onClick && "clickable")}
      title={title}
      style={{ width: size, height: size, ...(color ? { background: color } : {}) }}
      onClick={onClick}
    />
  );
}

/** 진행 막대. pct = null 이면 얼마나 남았는지 모름 (왔다 갔다) */
export const ProgressBar = ({ pct, width = 120 }: { pct: number | null; width?: number }) => (
  <span className={cx("ui-progress", pct === null && "busy")} style={{ width }} role="progressbar" aria-valuenow={pct ?? undefined}>
    <span style={pct === null ? undefined : { width: `${pct}%` }} />
  </span>
);

/** 이니셜 사각형 (워크스페이스). active = 강조색 바탕 */
export const Avatar = ({ children, active, size = 30 }: { children: ReactNode; active?: boolean; size?: number }) => (
  <span className={cx("ui-avatar", active && "active")} style={{ width: size, height: size }}>
    {children}
  </span>
);

// ───────── 버튼 ─────────
export type ButtonVariant = "default" | "primary" | "danger" | "ghost" | "link";
export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" | "md" }>(function Button({ variant = "default", size = "md", className, type = "button", ...rest }, ref) {
  return <button ref={ref} type={type} className={cx("ui-btn", `ui-btn-${variant}`, size === "sm" && "ui-btn-sm", className)} {...rest} />;
});

/** 아이콘만 있는 버튼. title 은 필수 (이름 · 단축키를 보여 준다) */
export function IconButton({ title, active, tone, size = "md", className, type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { title: string; active?: boolean; tone?: "ok" | "danger"; size?: "sm" | "md" }) {
  return <button type={type} title={title} aria-label={title} className={cx("ui-icon-btn", size === "sm" && "ui-icon-btn-sm", active && "active", tone && `tone-${tone}`, className)} {...rest} />;
}

/** 도구 막대의 글자 버튼 (Explain ▾, Tx: Auto ▾, Commit …). active = 켜져 있음, accent = 지금 누를 만함 */
export function ToolButton({ active, accent, className, type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { active?: boolean; accent?: boolean }) {
  return <button type={type} className={cx("ui-tool-btn", active && "active", accent && "accent", className)} {...rest} />;
}

// ───────── 배지 ─────────
/** 개수 · 상태 배지. color = 정해진 색 바탕 (Redis 타입처럼 종류마다 다른 색) */
export function Badge({ children, tone = "neutral", color, title, onClick, className }: { children: ReactNode; tone?: "neutral" | "accent" | "warn" | "danger"; color?: string; title?: string; onClick?: (e: React.MouseEvent) => void; className?: string }) {
  return (
    <span
      className={cx("ui-badge", tone !== "neutral" && `tone-${tone}`, color && "solid", onClick && "clickable", className)}
      style={color ? { background: color } : undefined}
      title={title}
      onClick={onClick}
      onMouseDown={onClick ? (e) => e.stopPropagation() : undefined}
    >
      {children}
    </span>
  );
}

// ───────── 입력 ─────────
/** 한 줄 입력. full = 폭 100%, grow = 줄(Row) 안에서 남는 폭을 차지 */
export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { full?: boolean; grow?: boolean }>(function TextInput({ className, full, grow, ...rest }, ref) {
  return <input ref={ref} className={cx("ui-input", full && "full", grow && "grow", className)} {...rest} />;
});
export function Select({ className, full, ...rest }: SelectHTMLAttributes<HTMLSelectElement> & { full?: boolean }) {
  return <select className={cx("ui-select", full && "full", className)} {...rest} />;
}
export const TextArea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement> & { mono?: boolean }>(function TextArea({ className, mono = true, ...rest }, ref) {
  return <textarea ref={ref} className={cx("ui-textarea", !mono && "sans", className)} {...rest} />;
});
/** 체크박스. indeterminate = 일부만 (하위 항목 중 몇 개) */
export function Checkbox({ checked, onChange, children, disabled, title, indeterminate }: { checked: boolean; onChange: (v: boolean) => void; children?: ReactNode; disabled?: boolean; title?: string; indeterminate?: boolean }) {
  return (
    <label className="ui-check" title={title} onMouseDown={(e) => e.stopPropagation()}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        ref={(el) => {
          if (el) el.indeterminate = !!indeterminate;
        }}
        onChange={(e) => onChange(e.target.checked)}
      />
      {children}
    </label>
  );
}
export function Radio({ checked, onChange, children, disabled }: { checked: boolean; onChange: () => void; children?: ReactNode; disabled?: boolean }) {
  return (
    <label className="ui-check">
      <input type="radio" checked={checked} disabled={disabled} onChange={onChange} />
      {children}
    </label>
  );
}
