/**
 * Molecules: a row · cell made of a few atoms — list row, detail row, menu item, form field, tab, hint, section title.
 * Spec in README.md
 */
import { HTMLAttributes, InputHTMLAttributes, MouseEvent, ReactNode, useEffect, useState } from "react";
import { Avatar, Kbd, Radio, Swatch, Text, TextInput } from "./atoms";
import { cx } from "./cx";
import { Chevron } from "./icons";

// ───────── Form ─────────
/** Label + field on one line. Hint · error below the field */
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
/** One of several (radio). Short description (hint) per option */
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
/** Section title inside a form · window (full width) */
export const FormSection = ({ children }: { children: ReactNode }) => <div className="ui-form-section">{children}</div>;
/** Small gray description below a field · window */
export const Hint = ({ children, className }: { children: ReactNode; className?: string }) => <div className={cx("ui-hint", className)}>{children}</div>;
/** Color picker (Data Source color). value = null means no color */
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
/** #abc · abc → #aabbcc (otherwise null) */
export function normalizeHex(s: string): string | null {
  const m = HEX.exec(s.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((x) => x + x).join("") : m[1];
  return `#${h.toLowerCase()}`;
}

/** Custom pick: system color picker (on macOS includes an eyedropper for screen pixels) + hex input */
function CustomColor({ value, onChange }: { value: string | null; onChange: (v: string) => void }) {
  const [text, setText] = useState(value ?? "");
  useEffect(() => setText((t) => (normalizeHex(t) === value ? t : value ?? "")), [value]);
  // Apply valid hex immediately as typed (so pressing OK right away works), revert if left invalid
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
 * Search field: ⌕ + input (+ loading ◌). bare = large without border (popover header — Search Everywhere)
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

// ───────── Tabs · titles ─────────
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
/** Small section title inside a list · window (Actions · Files · Format · To …) */
export const SectionTitle = ({ children, className }: { children: ReactNode; className?: string }) => <div className={cx("ui-section-title", className)}>{children}</div>;

// ───────── Hints ─────────
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
/** Key hint line: "↩ Open · ⇥ Next tab" — keys are Kbd, separated by · */
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

// ───────── List row ─────────
/**
 * One list row (search results · history · switcher · migrations · RAG index …).
 * [color stripe] [icon] title description ……… right
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
  /** Leading slot (checkbox etc.) */
  leading?: ReactNode;
  /** Second line (small gray) */
  subtitle?: ReactNode;
  /** Data Source color stripe */
  color?: string | null;
  hint?: ReactNode;
  trailing?: ReactNode;
  selected?: boolean;
  /** Not open · disabled */
  dim?: boolean;
  mono?: boolean;
  /** Full title shown on hover */
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

/** Detail row: code (statement · query) + value on the right (rows · pid · version). Value colored by tone */
export function DetailRow({ children, value, tone }: { children: ReactNode; value?: ReactNode; tone?: "danger" | "warn" }) {
  return (
    <div className="ui-detail-row">
      <code className="ui-detail-text">{children}</code>
      {value !== undefined && <span className={cx("ui-detail-value", tone && `tone-${tone}`)}>{value}</span>}
    </div>
  );
}

// ───────── Menu item ─────────
/** Menu row: name ……… shortcut (▸ for submenus) */
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
/** Dimmed description row in a menu (not clickable) */
export const MenuLabel = ({ children }: { children: ReactNode }) => <div className="ui-menu-label-row">{children}</div>;

// ───────── Status bar ─────────
/** Small cell on the right of the status bar (1:20 · UTF-8 · 🔒 Read-only · update) */
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

// ───────── Tree row ─────────
/**
 * One tree row (Database Explorer · schema picker): indent ▸ icon name [badge] description.
 * Clicking ▸ calls onToggle, clicking the row calls onSelect, double-click calls onDoubleClick (onToggle if absent)
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
  /** Right end (TTL · count) */
  trailing?: ReactNode;
  /** Before the icon (checkbox) */
  leading?: ReactNode;
  selected?: boolean;
  title?: string;
  className?: string;
  style?: React.CSSProperties;
  /** Key that Select In Explorer looks for */
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

// ───────── A single tab (editor tab · result tab) ─────────
/**
 * Closable tab: [icon] name (📌 | ×). When pinned, 📌 instead of × (click to unpin).
 * variant = editor (editor tab on top) · result (result tab below the console). Drag · right-click are passed through
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

// ───────── Left vertical bar cell (workspace) ─────────
/** One cell of the vertical bar: initials square + small name. Right-click · double-click are passed through */
export function RailItem({ initials, label, active, ...rest }: HTMLAttributes<HTMLButtonElement> & { initials: string; label: string; active?: boolean }) {
  return (
    <button type="button" className={cx("ui-rail-item", active && "active")} aria-pressed={active} {...rest}>
      <Avatar active={active}>{initials}</Avatar>
      <span className="ui-rail-label">{label}</span>
    </button>
  );
}
