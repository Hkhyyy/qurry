/**
 * Atoms: pieces that can't be split further — text · code · shortcuts · buttons · inputs · badges · dividers · color swatches · row layout.
 * Screen code uses these instead of bare HTML like <span className="muted">. Spec in README.md
 */
import { ButtonHTMLAttributes, CSSProperties, forwardRef, HTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from "react";
import { cx } from "./cx";

// ───────── Text ─────────
export type Tone = "default" | "muted" | "subtle" | "accent" | "ok" | "warn" | "danger";
/** A piece of text. tone = color, size = xs 11 · sm 12 · md 13 · lg 14, mono = monospace, truncate = … on overflow (full text in title) */
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

/** Short code like a statement · name (monospace, keeps line breaks) */
export const Code = ({ children, className }: { children: ReactNode; className?: string }) => <code className={cx("ui-code", className)}>{children}</code>;

/** Multi-line code (SQL preview · DDL · raw error) */
export function CodeBlock({ children, className, wrap = true, style }: { children: ReactNode; className?: string; wrap?: boolean; style?: CSSProperties }) {
  return (
    <pre className={cx("ui-code-block", !wrap && "nowrap", className)} style={style}>
      {children}
    </pre>
  );
}

/** Shortcut label (⌘k, ⇧⌘A …). Same look in menus · lists · hint lines */
export const Kbd = ({ children, className }: { children: ReactNode; className?: string }) => <kbd className={cx("ui-kbd", className)}>{children}</kbd>;

// ───────── Layout ─────────
/** Lay out horizontally (vertically centered). gap = 1~6 (token spacing) */
export function Row({ children, gap = 4, wrap, className, style, ...rest }: HTMLAttributes<HTMLDivElement> & { gap?: 1 | 2 | 3 | 4 | 5 | 6; wrap?: boolean }) {
  return (
    <div className={cx("ui-row", `gap-${gap}`, wrap && "wrap", className)} style={style} {...rest}>
      {children}
    </div>
  );
}
/** Takes the remaining width, pushing what follows to the right */
export const Spacer = () => <span className="ui-spacer" />;
/** Divider: vertical (toolbar) · horizontal (menu · section) */
export const Divider = ({ vertical }: { vertical?: boolean }) => <span className={vertical ? "ui-divider-v" : "ui-divider-h"} role="separator" />;
/** Vertical line between toolbar groups */
export const ToolbarSep = () => <Divider vertical />;

/** Color swatch (Data Source color · tab stripe). none = no color (hatched) */
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

/** Progress bar. pct = null means remaining amount unknown (bounces back and forth) */
export const ProgressBar = ({ pct, width = 120 }: { pct: number | null; width?: number }) => (
  <span className={cx("ui-progress", pct === null && "busy")} style={{ width }} role="progressbar" aria-valuenow={pct ?? undefined}>
    <span style={pct === null ? undefined : { width: `${pct}%` }} />
  </span>
);

/** Initials square (workspace). active = accent background */
export const Avatar = ({ children, active, size = 30 }: { children: ReactNode; active?: boolean; size?: number }) => (
  <span className={cx("ui-avatar", active && "active")} style={{ width: size, height: size }}>
    {children}
  </span>
);

// ───────── Buttons ─────────
export type ButtonVariant = "default" | "primary" | "danger" | "ghost" | "link";
export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" | "md" }>(function Button({ variant = "default", size = "md", className, type = "button", ...rest }, ref) {
  return <button ref={ref} type={type} className={cx("ui-btn", `ui-btn-${variant}`, size === "sm" && "ui-btn-sm", className)} {...rest} />;
});

/** Icon-only button. title is required (shows the name · shortcut) */
export function IconButton({ title, active, tone, size = "md", className, type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { title: string; active?: boolean; tone?: "ok" | "danger"; size?: "sm" | "md" }) {
  return <button type={type} title={title} aria-label={title} className={cx("ui-icon-btn", size === "sm" && "ui-icon-btn-sm", active && "active", tone && `tone-${tone}`, className)} {...rest} />;
}

/** Text button in a toolbar (Explain ▾, Tx: Auto ▾, Commit …). active = on, accent = worth pressing now */
export function ToolButton({ active, accent, className, type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { active?: boolean; accent?: boolean }) {
  return <button type={type} className={cx("ui-tool-btn", active && "active", accent && "accent", className)} {...rest} />;
}

// ───────── Badges ─────────
/** Count · status badge. color = fixed background color (a different color per kind, like Redis types) */
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

// ───────── Inputs ─────────
/** Single-line input. full = 100% width, grow = take the remaining width inside a Row */
export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { full?: boolean; grow?: boolean }>(function TextInput({ className, full, grow, ...rest }, ref) {
  return <input ref={ref} className={cx("ui-input", full && "full", grow && "grow", className)} {...rest} />;
});
export function Select({ className, full, ...rest }: SelectHTMLAttributes<HTMLSelectElement> & { full?: boolean }) {
  return <select className={cx("ui-select", full && "full", className)} {...rest} />;
}
export const TextArea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement> & { mono?: boolean }>(function TextArea({ className, mono = true, ...rest }, ref) {
  return <textarea ref={ref} className={cx("ui-textarea", !mono && "sans", className)} {...rest} />;
});
/** Checkbox. indeterminate = partial (some of the children) */
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
