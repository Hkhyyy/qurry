/**
 * Molecules: a row · cell made of a few atoms — list row, detail row, menu item, form field, tab, hint, section title.
 * Spec in README.md
 */
import { HTMLAttributes, InputHTMLAttributes, KeyboardEvent, MouseEvent, ReactNode, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Avatar, Badge, Button, IconButton, Kbd, Radio, Swatch, Text, TextInput } from "./atoms";
import { cx } from "./cx";
import { Chevron, IcInfo } from "./icons";
import { placePopup } from "./popup";

// ───────── About (ⓘ) ─────────
/** The open ⓘ's close: opening another one closes it (one explanation at a time) */
let closeOpenTip: (() => void) | null = null;

/**
 * ⓘ button with what a name means: click, Space or Enter toggles a small popover (the menu surface) under it, kept inside the
 * window. Esc closes the popover only (not the dialog around it) and puts focus back on the ⓘ; a click outside, a scroll or
 * opening another ⓘ closes it. A span with the button role, not a <button>: a <button> inside a <label> (a Field's) would be
 * the label's control, so a click on the name would open the ⓘ instead of focusing the field
 */
export function InfoTip({ about, info }: { about: string; info: ReactNode[] }) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const btn = useRef<HTMLSpanElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const id = useId();
  // Placed once its size is known (hidden until then)
  useLayoutEffect(() => {
    if (!open || !btn.current || !pop.current) return setAt(null);
    const r = btn.current.getBoundingClientRect();
    setAt(placePopup(r, pop.current.getBoundingClientRect(), { width: window.innerWidth, height: window.innerHeight }));
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(false);
    closeOpenTip?.();
    closeOpenTip = close;
    const outside = (e: Event) => !pop.current?.contains(e.target as Node) && !btn.current?.contains(e.target as Node);
    const down = (e: globalThis.MouseEvent) => outside(e) && setOpen(false);
    const scroll = (e: Event) => outside(e) && setOpen(false);
    const key = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      btn.current?.focus();
    };
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", scroll);
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", scroll);
      if (closeOpenTip === close) closeOpenTip = null;
    };
  }, [open]);
  return (
    <>
      <span
        ref={btn}
        role="button"
        tabIndex={0}
        className="ui-icon-btn ui-info-btn"
        aria-label={`About ${about}`}
        title={`About ${about}`}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        // Its text (not the note, whose name is "About …"): read out with the ⓘ while it is open
        aria-describedby={open ? `${id}-text` : undefined}
        // Inside a <label>: don't also activate its control
        onClick={(e) => (e.preventDefault(), setOpen((o) => !o))}
        onKeyDown={(e) => {
          if (e.key !== "Enter" && e.key !== " ") return;
          // Not the dialog's Enter (OK, Open Undo …) and no page scroll on Space
          e.preventDefault();
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        <IcInfo />
      </span>
      {open &&
        createPortal(
          <div ref={pop} id={id} role="note" aria-label={`About ${about}`} className="ui-menu ui-info-pop" style={at ?? { left: 0, top: 0, visibility: "hidden" }}>
            <div id={`${id}-text`} className="ui-info-text">
              {info.map((l, i) => (
                <div key={i}>{l}</div>
              ))}
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}
/**
 * A name and its ⓘ right after it, with the same gap everywhere: a field's label, a checkbox (the ⓘ beside it, not inside its
 * label, so clicking the ⓘ doesn't tick it), a section title, a hint line. about = the name the ⓘ is announced with (default: the
 * text itself). Without info it is just the name
 */
export function Named({ children, about, info }: { children: ReactNode; about?: string; info?: ReactNode[] }) {
  if (!info?.length) return <>{children}</>;
  return (
    <span className="ui-named">
      {/* A text name shrinks with … in a narrow label column; the ⓘ never does */}
      {typeof children === "string" ? <span className="ui-named-text">{children}</span> : children}
      <InfoTip about={about ?? (typeof children === "string" ? children : "")} info={info} />
    </span>
  );
}

// ───────── Form ─────────
/**
 * Label + field on one line. Hint · error below the field; info = what the field means, behind an ⓘ after the label (Named).
 * group = several controls with their own labels: a div (role group), so a click on the field's label doesn't land in the first control
 */
export function Field({ label, hint, error, info, children, className, group }: { label: ReactNode; hint?: ReactNode; error?: ReactNode; info?: ReactNode[]; children: ReactNode; className?: string; group?: boolean }) {
  const Tag = group ? "div" : "label";
  const name = typeof label === "string" ? label : undefined;
  return (
    <Tag className={cx("ui-field", className)} role={group ? "group" : undefined} aria-label={group ? name : undefined}>
      <span className="ui-field-label" title={name}>
        <Named about={name} info={info}>
          {label}
        </Named>
      </span>
      {children}
      {hint && <span className="ui-field-hint">{hint}</span>}
      {error && <span className="ui-field-error">{error}</span>}
    </Tag>
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
      <TextInput {...rest} />
      {busy && (
        <span className="ui-search-busy" title="Loading…">
          ◌
        </span>
      )}
    </div>
  );
}

// ───────── Tabs · titles ─────────
/** A `disabled` tab is greyed out and can't be picked, but still shows its `title` (why) on hover: aria-disabled, not disabled */
export function Tabs<T extends string>({ items, value, onChange, size, label }: { items: { id: T; label: ReactNode; disabled?: boolean; title?: string }[]; value: T; onChange: (id: T) => void; size?: "sm"; label?: string }) {
  // ←/→ (and Home/End) select the previous/next tab and move focus with it, as in the ARIA tab pattern (skipping disabled ones)
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const i = items.findIndex((t) => t.id === value);
    const on = items.map((t, j) => (t.disabled ? -1 : j)).filter((j) => j >= 0);
    const n = { ArrowLeft: on.filter((j) => j < i).pop(), ArrowRight: on.find((j) => j > i), Home: on[0], End: on[on.length - 1] }[e.key];
    if (n === undefined || n === i) return;
    e.preventDefault();
    onChange(items[n].id);
    e.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]')[n]?.focus();
  };
  return (
    <div className={cx("ui-tabs", size === "sm" && "size-sm")} role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {items.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={value === t.id}
          aria-disabled={t.disabled || undefined}
          title={t.title}
          className={cx("ui-tab", value === t.id && "active", t.disabled && "disabled")}
          onClick={() => !t.disabled && onChange(t.id)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
/** Small section title inside a list · window (Actions · Files · Format · To …) */
export const SectionTitle = ({ children, className }: { children: ReactNode; className?: string }) => <div className={cx("ui-section-title", className)}>{children}</div>;

// ───────── Hints ─────────
/**
 * A console's open transaction (#106), beside its Commit / Roll back: its changes aren't saved until one of them, and closing the
 * tab, switching the database or quitting asks first. production = a Production data source (red); idle = the reminder fired
 */
export function TxPending({ production, idle }: { production?: boolean; idle?: string }) {
  const title = `A transaction is open in this console: Commit saves what changed in it, Roll back undoes it. Closing the tab, switching the database or quitting asks first.${idle ? `\n${idle}` : ""}`;
  return (
    <span role="status" className="tx-pending" title={title}>
      <Badge tone={production ? "danger" : "warn"}>● Transaction pending{idle ? " · idle" : ""}</Badge>
    </span>
  );
}

export function Notice({ tone, children, className }: { tone?: "warn" | "danger" | "ok"; children: ReactNode; className?: string }) {
  return <div className={cx("ui-notice", tone && `tone-${tone}`, className)}>{children}</div>;
}
/**
 * Under a connect error that needs TLS (qurry_shared::errors::tls_opt_in): turn on "Allow unencrypted connection" for this data source in one click.
 * Never offered for Production (the core refuses too): it says where the setting is instead. A failed `onAllow` shows its error here
 */
export function TlsOptIn({ host, production, inDialog, onAllow }: { host: string; production?: boolean; inDialog?: boolean; onAllow: () => Promise<unknown> }) {
  // Inside Data Source Properties the Options tab is right there; elsewhere it's the dialog's shortcut
  const where = inDialog ? "Options" : "Data Source Properties (⌘;) ▸ Options";
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  if (production)
    return (
      <div className="ui-tls-optin">
        <span className="ui-tls-optin-detail">
          Production data source: Qurry won't turn off encryption from here. If the server has no TLS and the network is trusted, turn on {where} ▸ Allow unencrypted connection.
        </span>
      </div>
    );
  return (
    <div className="ui-tls-optin">
      <Button
        size="sm"
        disabled={busy}
        title={`To undo, turn off ${where} ▸ Allow unencrypted connection`}
        onClick={async () => {
          setBusy(true);
          setErr(null);
          try {
            await onAllow();
          } catch (e) {
            setErr(String(e));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? "Allowing…" : "Allow unencrypted for this data source"}
      </Button>
      <span className="ui-tls-optin-detail">The password will be sent unencrypted to {host}. Use only on a network you trust.</span>
      {err && <Notice tone="danger">{err}</Notice>}
    </div>
  );
}
/**
 * What a wait in the background says (a data source that reconnects), bottom right above the status bar (inside Toasts): its text,
 * then its actions on the right. tone="danger": it failed (role alert, a danger border); onClose: a × in the corner
 */
export function Toast({ tone, children, actions, onClose }: { tone?: "danger"; children: ReactNode; actions?: ReactNode; onClose?: () => void }) {
  return (
    <div className={cx("ui-toast", tone && `tone-${tone}`, onClose && "closable")} role={tone === "danger" ? "alert" : "status"}>
      {children}
      {actions && <div className="ui-toast-acts">{actions}</div>}
      {onClose && (
        <IconButton size="sm" className="ui-toast-x" title="Close" onClick={onClose}>
          ×
        </IconButton>
      )}
    </div>
  );
}
/** The toasts, stacked bottom right above the status bar; clicks go through around them */
export const Toasts = ({ children }: { children: ReactNode }) => <div className="ui-toasts">{children}</div>;
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

/** Detail row: code (statement · query) + value on the right (rows · pid · version), a note in plain text under the code (why it
 *  asks). Value colored by tone */
/** lead = a control before the text (a checkbox to leave the row out) */
export function DetailRow({ children, value, tone, note, lead }: { children: ReactNode; value?: ReactNode; tone?: "danger" | "warn"; note?: ReactNode; lead?: ReactNode }) {
  return (
    <div className="ui-detail-row">
      {lead !== undefined && <span className="ui-detail-lead">{lead}</span>}
      {note === undefined ? (
        <code className="ui-detail-text">{children}</code>
      ) : (
        <span className="ui-detail-main">
          <code className="ui-detail-text">{children}</code>
          <span className="ui-detail-note">{note}</span>
        </span>
      )}
      {value !== undefined && <span className={cx("ui-detail-value", tone && `tone-${tone}`)}>{value}</span>}
    </div>
  );
}

// ───────── Menu item ─────────
/** Menu row: name ……… shortcut (▸ for submenus) */
export function MenuItem({ label, shortcut, danger, disabled, title, hasSubmenu, children, onClick, onMouseEnter }: { label: ReactNode; shortcut?: string; danger?: boolean; disabled?: boolean; title?: string; hasSubmenu?: boolean; children?: ReactNode; onClick?: () => void; onMouseEnter?: () => void }) {
  return (
    <div
      className={cx("ui-menu-item", danger && "danger", disabled && "disabled")}
      role="menuitem"
      title={title}
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
  id,
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
  /** DOM id: the explorer reads the selectable rows' order from it (⇧-click range) */
  id?: string;
  /** Mouse down on the row (the event: ⌘ / ⇧ / right button), or a click on its expander (no event) */
  onSelect?: (e?: MouseEvent) => void;
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
      id={id}
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
      {/* data-text: App.css reserves the bold width so a tab doesn't widen when it becomes active */}
      <span className="tab-label" data-text={typeof label === "string" ? label : undefined}>
        {label}
      </span>
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
