/**
 * Organisms: chunks assembled from molecules — toolbars, dialogs, confirm boxes, menus, popovers, lists, tables, form grids.
 * Spec in README.md
 */
import { Children, CSSProperties, FocusEvent, HTMLAttributes, InputHTMLAttributes, KeyboardEvent, ReactNode, Ref, RefObject, TdHTMLAttributes, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button, IconButton, Spacer, TextInput } from "./atoms";
import { cx } from "./cx";
import { DetailRow, EmptyState, ListItem, MenuItem, MenuLabel, MenuSeparator, SectionTitle } from "./molecules";
import { listKey, matchOptions, placePopup } from "./popup";

// ───────── Toolbar ─────────
export function Toolbar({ children, className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cx("ui-toolbar", className)} role="toolbar" {...rest}>
      {children}
    </div>
  );
}

// ───────── Dialog ─────────
export type ModalSize = "sm" | "md" | "lg" | "xl";
/**
 * Dialog. Closes on Esc · outside click. The footer is right-aligned (Cancel → primary button).
 * layer="dialog" is a confirm · input box shown above other modals
 */
export function Modal(p: {
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  size?: ModalSize;
  className?: string;
  layer?: "modal" | "dialog";
  tone?: "danger";
  /** Don't close on outside click (forms being filled in) */
  stickyBackdrop?: boolean;
  /** Fixed size: the window keeps its size when switching tabs or loading content (scrolls inside if it overflows) */
  fixed?: boolean;
  headExtra?: ReactNode;
  onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void;
  bodyClassName?: string;
}) {
  // Take focus when opened (unless an autoFocus inside got it first) — so Esc · Enter always go to this window
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (box.current && !box.current.contains(document.activeElement)) box.current.focus();
  }, []);
  return (
    <div className={cx("ui-backdrop", "for-modal", p.layer === "dialog" && "layer-dialog")} onMouseDown={p.stickyBackdrop ? undefined : p.onClose}>
      <div
        ref={box}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        className={cx("ui-modal", p.size && p.size !== "md" && `size-${p.size}`, p.fixed && "fixed", p.tone && `tone-${p.tone}`, p.className)}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // Don't leak to app-wide shortcuts
          e.stopPropagation();
          p.onKeyDown?.(e);
          if (e.key === "Escape" && !e.defaultPrevented) p.onClose();
        }}
      >
        <div className="ui-modal-head">
          <div className="ui-modal-title" title={typeof p.title === "string" ? p.title : undefined}>
            {p.title}
          </div>
          {p.headExtra && <Spacer />}
          {p.headExtra}
        </div>
        <div className={cx("ui-modal-body", p.bodyClassName)}>{p.children}</div>
        {p.footer && <div className="ui-modal-foot">{p.footer}</div>}
      </div>
    </div>
  );
}

/** Backdrop behind popups (search · switcher · quick docs). Clicking outside closes. clear = no dimming */
export function Overlay({ onClose, variant, children, onKeyDown }: { onClose: () => void; variant?: "clear" | "light"; children: ReactNode; onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void }) {
  return (
    <div className={cx("ui-backdrop", variant && `overlay-${variant}`)} onMouseDown={onClose} onKeyDown={onKeyDown}>
      {children}
    </div>
  );
}

/** Floating panel (Search Everywhere · switcher · Go to …): title / header · body (list) · hint at the bottom */
export function Popover({ title, head, foot, children, className, width }: { title?: ReactNode; head?: ReactNode; foot?: ReactNode; children: ReactNode; className?: string; width?: number }) {
  return (
    <div className={cx("ui-popover", className)} style={width ? { width } : undefined} onMouseDown={(e) => e.stopPropagation()}>
      {title && <div className="ui-popover-title">{title}</div>}
      {head}
      {children}
      {foot && <div className="ui-popover-foot">{foot}</div>}
    </div>
  );
}

// ───────── List ─────────
/** Multi-row list (holds ListItems). empty when empty. scroll area · bordered box (boxed) */
export function List({ children, empty, boxed, className, listRef, style }: { children?: ReactNode; empty?: ReactNode; boxed?: boolean; className?: string; listRef?: Ref<HTMLDivElement>; style?: CSSProperties }) {
  const isEmpty = !children || (Array.isArray(children) && children.flat().filter(Boolean).length === 0);
  return (
    <div className={cx("ui-list", boxed && "boxed", className)} role="listbox" ref={listRef} style={style}>
      {isEmpty && empty ? <EmptyState>{empty}</EmptyState> : children}
    </div>
  );
}
/** Group heading inside a list (Actions · Files · Database) */
export const ListGroup = SectionTitle;

/**
 * List popup under a field — the one drop-down list of the combo boxes (FontPicker, ComboBox). Fixed right under `anchor` and
 * left-aligned to it, above it when there's no room below, always inside the window; at least as wide as the field. The rows
 * (ListItems) are the owner's, and so are the keys (`listKey`); the highlighted row (`active`) stays in view. Clicks keep the focus
 * where it is (the field, or a search box in `head`); scrolling anything else closes it, since it would be left behind.
 * `height` fixes the height (so it doesn't jump while filtering); without it, it fits the rows up to a maximum
 */
export function PopupList({ anchor, active, onClose, head, foot, empty, height, minWidth = 0, className, children, onKeyDown, onBlur }: {
  anchor: RefObject<HTMLElement | null>;
  active: number;
  onClose: () => void;
  head?: ReactNode;
  foot?: ReactNode;
  empty?: ReactNode;
  height?: number;
  minWidth?: number;
  className?: string;
  children?: ReactNode;
  onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void;
  onBlur?: (e: FocusEvent<HTMLDivElement>) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{ left: number; top: number; width: number }>();
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const rows = Children.count(children);
  // Placed before it's painted, again when the rows change its height
  useLayoutEffect(() => {
    const r = anchor.current!.getBoundingClientRect();
    const width = Math.max(r.width, minWidth);
    setAt({ ...placePopup(r, { width, height: box.current!.offsetHeight }, { width: window.innerWidth, height: window.innerHeight }), width });
  }, [rows]);
  useLayoutEffect(() => {
    const row = list.current?.children[active];
    if (!row) return;
    // By hand: scrollIntoView would also scroll the form under the popup
    const l = list.current!.getBoundingClientRect();
    const r = row.getBoundingClientRect();
    if (r.top < l.top) list.current!.scrollTop -= l.top - r.top;
    else if (r.bottom > l.bottom) list.current!.scrollTop += r.bottom - l.bottom;
  }, [active, rows]);
  useEffect(() => {
    const onScroll = (e: Event) => !box.current?.contains(e.target as Node) && closeRef.current();
    window.addEventListener("scroll", onScroll, true);
    return () => window.removeEventListener("scroll", onScroll, true);
  }, []);
  return (
    <div
      ref={box}
      className={cx("ui-popup-list", className)}
      style={{ ...at, height, maxHeight: height }}
      onKeyDown={onKeyDown}
      onBlur={onBlur}
      onMouseDownCapture={(e) => (e.target as HTMLElement).tagName !== "INPUT" && e.preventDefault()}
    >
      {head && <div className="ui-popup-head">{head}</div>}
      <List listRef={list} empty={empty}>
        {children}
      </List>
      {foot && <div className="ui-popup-foot">{foot}</div>}
    </div>
  );
}

/**
 * Free-text field with suggestions (DataGrip's column type field): typing filters `options` (exact, then prefix, then contains; no
 * popup when nothing matches), ↓ or a click lists them all. ↑↓ PgUp PgDn move, ↩ or ⇥ takes the highlighted one, ⎋ closes the list
 * only (not the dialog), a click picks. Anything typed is kept as it is
 */
export function ComboBox({ value, options, onChange, onKeyDown, onBlur, onClick, ...rest }: Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> & { value: string; options: string[]; onChange: (v: string) => void; full?: boolean; grow?: boolean }) {
  const [query, setQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const rows = query === null ? [] : matchOptions(options, query);
  const open = rows.length > 0;
  // What's typed with the best match highlighted, or all of them with the current value highlighted
  const show = (q: string, current = value) => {
    setQuery(q);
    setActive(q ? 0 : Math.max(0, options.indexOf(current)));
  };
  const pick = (v: string) => {
    onChange(v);
    setQuery(null);
  };
  return (
    <>
      <TextInput
        ref={input}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={open}
        value={value}
        onChange={(e) => (onChange(e.target.value), show(e.target.value, e.target.value))}
        onClick={(e) => (!open && show(""), onClick?.(e))}
        onBlur={(e) => (setQuery(null), onBlur?.(e))}
        onKeyDown={(e) => {
          const k = open ? listKey(e.key, active, rows.length) : e.key === "ArrowDown" ? 0 : null;
          if (open && e.key === "Tab") pick(rows[active]);
          else if (k === null) return onKeyDown?.(e);
          else if (!open) show("");
          else if (k === "pick") pick(rows[active]);
          else if (k === "close") setQuery(null);
          else setActive(k);
          // Tab moves on to the next field; the rest are the list's, not the dialog's
          if (e.key !== "Tab") e.preventDefault();
        }}
        {...rest}
      />
      {open && (
        <PopupList anchor={input} active={active} minWidth={160} onClose={() => setQuery(null)}>
          {rows.map((o, i) => (
            <ListItem key={o} title={o} selected={i === active} onMouseEnter={() => setActive(i)} onClick={() => pick(o)} />
          ))}
        </PopupList>
      )}
    </>
  );
}

/** Detail list (statement · row count in confirm boxes, query · pid in sessions …) */
export function DetailList({ items }: { items: { text: ReactNode; value?: ReactNode; tone?: "danger" | "warn"; note?: ReactNode }[] }) {
  return (
    <div className="ui-detail-list">
      {items.map((d, i) => (
        <DetailRow key={i} value={d.value} tone={d.tone} note={d.note}>
          {d.text}
        </DetailRow>
      ))}
    </div>
  );
}

// ───────── Table (small tables: previews · mappings · value lists) ─────────
/** Table for small data. Large results use ResultGrid (cell pieces in ui/grid) */
export function Table({ children, className }: { children: ReactNode; className?: string }) {
  return <table className={cx("ui-table", className)}>{children}</table>;
}
export const Th = ({ children, num, className, ...rest }: TdHTMLAttributes<HTMLTableCellElement> & { num?: boolean }) => (
  <th className={cx(num && "num", className)} {...rest}>
    {children}
  </th>
);
export const Td = ({ children, num, mono, muted, className, ...rest }: TdHTMLAttributes<HTMLTableCellElement> & { num?: boolean; mono?: boolean; muted?: boolean }) => (
  <td className={cx(num && "num", mono && "mono", muted && "muted", className)} {...rest}>
    {children}
  </td>
);

// ───────── Menu ─────────
export type MenuEntry =
  /** id: the keymap command it runs (lib/shortcuts.ts), so its key runs it from the keyboard too */
  | { label: string; action: () => void; shortcut?: string; danger?: boolean; disabled?: boolean; id?: string }
  | { separator: true }
  /** Non-clickable description line */
  | { heading: string }
  | { label: string; submenu: MenuEntry[] };

/**
 * Menu (right-click · ▾ dropdown). Closes on outside click · ⎋ · window blur. Submenus open on hover.
 * at = screen coordinates (right-click) — otherwise a dropdown relative to the parent. footer = appended at the bottom (color picker etc.)
 */
export function Menu({ items, onClose, at, footer }: { items: MenuEntry[]; onClose: () => void; at?: { x: number; y: number }; footer?: ReactNode }) {
  const [sub, setSub] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const onDown = (e: globalThis.MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) closeRef.current();
    };
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        closeRef.current();
      }
    };
    const onBlur = () => closeRef.current();
    // Register on the next tick so the click that opened the menu isn't treated as an "outside click" right away
    const t = setTimeout(() => {
      window.addEventListener("mousedown", onDown, true);
      window.addEventListener("keydown", onKey, true);
      window.addEventListener("blur", onBlur);
    });
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  // Keep it on screen
  const style = at ? { left: Math.min(at.x, window.innerWidth - 260), top: Math.max(4, Math.min(at.y, window.innerHeight - 24 * items.length - 40)) } : undefined;
  const run = (fn: () => void) => {
    fn();
    onClose();
  };
  return (
    <div className={cx("ui-menu", !at && "dropdown")} ref={ref} style={style} role="menu" onMouseDown={(e) => e.stopPropagation()}>
      {items.map((it, i) =>
        "separator" in it ? (
          <MenuSeparator key={i} />
        ) : "heading" in it ? (
          <MenuLabel key={i}>{it.heading}</MenuLabel>
        ) : "submenu" in it ? (
          <MenuItem key={i} label={it.label} hasSubmenu onMouseEnter={() => setSub(i)}>
            {sub === i && (
              <div className="ui-menu sub" role="menu">
                {it.submenu.map((s, j) => ("separator" in s ? <MenuSeparator key={j} /> : "heading" in s ? <MenuLabel key={j}>{s.heading}</MenuLabel> : "action" in s ? <MenuItem key={j} label={s.label} shortcut={s.shortcut} danger={s.danger} disabled={s.disabled} onClick={() => run(s.action)} /> : null))}
              </div>
            )}
          </MenuItem>
        ) : (
          <MenuItem key={i} label={it.label} shortcut={it.shortcut} danger={it.danger} disabled={it.disabled} onMouseEnter={() => setSub(null)} onClick={() => run(it.action)} />
        ),
      )}
      {footer && (
        <>
          <MenuSeparator />
          <div className="ui-menu-footer">{footer}</div>
        </>
      )}
    </div>
  );
}

// ───────── Form grid ─────────
/**
 * Form alternating label · field (long settings windows). Children in <label>Label</label><field/> order; full-width rows use <FormSection>.
 * For a row without a label, put an empty <span /> first
 */
export function FormGrid({ children, labelWidth = 120, className }: { children: ReactNode; labelWidth?: number; className?: string }) {
  return (
    <div className={cx("ui-form-grid", className)} style={{ ["--ui-form-label" as string]: `${labelWidth}px` }}>
      {children}
    </div>
  );
}

// ───────── confirm() — in-app confirm box ─────────
export interface ConfirmOptions {
  title?: string;
  message: ReactNode;
  /** Statement · target and the value next to it (e.g. rows to change) */
  detail?: { text: string; value?: string; tone?: "danger" | "warn"; note?: string }[];
  okLabel?: string;
  /** null = no Cancel button (a notice to acknowledge) */
  cancelLabel?: string | null;
  danger?: boolean;
}
type ConfirmReq = ConfirmOptions & { resolve: (ok: boolean) => void };
let showConfirm: ((r: ConfirmReq) => void) | null = null;

/** Show a confirm box and wait for the answer. Without a ConfirmHost (tests), the browser confirm */
export function confirm(o: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    if (showConfirm) showConfirm({ ...o, resolve });
    else resolve(window.confirm(typeof o.message === "string" ? o.message : o.title ?? "") as unknown as boolean);
  });
}

/** Place one in App */
export function ConfirmHost() {
  const [queue, setQueue] = useState<ConfirmReq[]>([]);
  const okRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    showConfirm = (r) => setQueue((q) => [...q, r]);
    return () => {
      showConfirm = null;
    };
  }, []);
  const req = queue[0];
  useEffect(() => {
    // Focus Cancel for dangerous confirms, OK otherwise (so it's clear what Enter presses)
    if (req && !req.danger) okRef.current?.focus();
  }, [req]);
  if (!req) return null;
  const done = (ok: boolean) => {
    setQueue((q) => q.slice(1));
    req.resolve(ok);
  };
  return (
    <Modal
      title={req.title ?? "Qurry"}
      size={req.detail?.length ? "lg" : "sm"}
      layer="dialog"
      tone={req.danger ? "danger" : undefined}
      onClose={() => done(false)}
      footer={
        <>
          {req.cancelLabel !== null && (
            <Button autoFocus={req.danger} onClick={() => done(false)}>
              {req.cancelLabel ?? "Cancel"}
            </Button>
          )}
          <Button ref={okRef} variant={req.danger ? "danger" : "primary"} onClick={() => done(true)}>
            {req.okLabel ?? "OK"}
          </Button>
        </>
      }
    >
      <div className="ui-confirm-message">{req.message}</div>
      {req.detail && req.detail.length > 0 && <DetailList items={req.detail} />}
    </Modal>
  );
}

// ───────── Tab strip ─────────
/**
 * Tab strip (editor · results): overflows without a scrollbar. The (vertical) wheel also scrolls sideways, and it scrolls to show the active tab when it changes.
 * On overflow, ▾ at the right end — a list of all tabs (DataGrip's Show Hidden Tabs). Drag-and-drop events are passed through as is
 */
export function TabStrip({ children, activeKey, overflow, className, ...rest }: HTMLAttributes<HTMLDivElement> & { activeKey?: string | null; overflow?: { key: string; label: string; active?: boolean; onSelect: () => void }[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const [over, setOver] = useState(false);
  const [menu, setMenu] = useState(false);
  // Whether it overflows (whether to show ▾)
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const check = () => setOver(el.scrollWidth > el.clientWidth + 1);
    check();
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  });
  // Keep the active tab visible
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>(".active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeKey]);
  return (
    <div className={cx("ui-tab-strip", className)} {...rest}>
      <div
        className="ui-tab-strip-scroll"
        ref={ref}
        role="tablist"
        onWheel={(e) => {
          // Trackpad horizontal scrolling as is; the (vertical) mouse wheel scrolls sideways
          if (ref.current && Math.abs(e.deltaY) > Math.abs(e.deltaX)) ref.current.scrollLeft += e.deltaY;
        }}
      >
        {children}
      </div>
      {over && overflow && (
        <span className="ui-tab-strip-more">
          <IconButton size="sm" title="Show All Tabs" onMouseDown={(e) => e.stopPropagation()} onClick={() => setMenu((m) => !m)}>
            ▾
          </IconButton>
          {menu && <Menu onClose={() => setMenu(false)} items={overflow.map((t) => ({ label: (t.active ? "● " : "　") + t.label, action: t.onSelect }))} />}
        </span>
      )}
    </div>
  );
}
