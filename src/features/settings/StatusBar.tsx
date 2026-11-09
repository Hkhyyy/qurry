import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cx, Spacer, StatusItem, Text } from "../../ui";
import { cellText, holdFocusWidgets, loadStatusItems, resetStatusItems, setStatusItem, setStatusNote, useStatusItems, useStatusWidgets, type FocusWidgets } from "../../lib/statusbar";
import { shared, useSharedReady, type StatusItemId, type StatusPart, type StatusSegment } from "../../lib/shared";
import { TransferBar, type TransferJob } from "../../components/Transfer";
import { RunningBar } from "../../components/RunningBar";

/** What the bar holds that can go when it is too narrow: the status bar segments and the encoding label */
type Droppable = StatusPart | "utf8";

/** The focused grid's segments for the items on (qurry_shared::statusbar::segments). Without the WebAssembly module: its current cell */
function useSegments(w: FocusWidgets, items?: StatusItemId[]): StatusSegment[] {
  const saved = useStatusItems();
  const on = items ?? saved;
  const ready = useSharedReady();
  const g = w.grid;
  if (!g) return [];
  if (ready) return shared().statusSegments(on, g);
  const [r, c] = g.pos ?? [];
  return r && c && r.count === 1 && c.count === 1 ? [{ id: "position", value: cellText(r.first, c.first) }] : [];
}

/**
 * Information only (DataGrip): what the focused tab is doing (`busy`: a run, a load-more, and since when, shown instead of the status
 * text; its ✕ on hover cancels it), then on the right the focused grid's items (each its own segment: Position, Shown columns and the
 * ones the user turned on), the focused editor's position, Read-only, UTF-8. Right-click the bar, or click an item, for the checklist
 * of items (shared with Settings › Appearance and the TUI). When it is too narrow the status text shrinks to 140 px first, then the
 * segments go in qurry_shared::statusbar::DROP order after UTF-8; Position and Read-only stay. `widgets` and `items`: what Storybook shows
 * instead of the focused editor's or grid's, and of the items saved
 */
function StatusBar({ status, busy, readOnly, update, onUpdate, jobs, widgets, items }: { status: string; busy?: { label: string; since: number; onCancel: () => void }; readOnly: boolean; update?: string; onUpdate: () => void; jobs: TransferJob[]; widgets?: FocusWidgets; items?: StatusItemId[] }) {
  const live = useStatusWidgets();
  const w = widgets ? { ...live, ...widgets } : live;
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const all = useSegments(w, items);
  const ready = useSharedReady();
  const order: Droppable[] = ["utf8", ...(ready ? shared().statusBarInfo().drop : [])];
  const ref = useRef<HTMLDivElement>(null);
  // How many of `order` are gone: back to none whenever the bar or what it shows changes, then one more while it overflows
  const [drop, setDrop] = useState(0);
  const gone = new Set(order.slice(0, drop));
  const segs = all.filter((s) => !gone.has(s.id));
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setDrop(0));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const key = [all.map((s) => s.id).join(), status, busy?.label, update, w.pos, readOnly, jobs.length].join("\u0001");
  useLayoutEffect(() => setDrop(0), [key]);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && el.scrollWidth > el.clientWidth + 1 && drop < order.length) setDrop(drop + 1);
  });
  // The items and their names come from the WebAssembly module: without it there is no checklist
  const openMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    if (!ready) return;
    // The checklist takes the focus: the bar keeps showing the grid's items meanwhile
    holdFocusWidgets(true);
    setMenu({ x: e.clientX, y: e.clientY });
  };
  const closeMenu = () => {
    setMenu(null);
    holdFocusWidgets(false);
  };
  const right: { key: string; node: React.ReactNode }[] = [
    ...segs.map((s) => ({
      key: s.id,
      node: (
        <span className="sb-seg" onClick={openMenu}>
          <StatusItem onClick={() => undefined}>
            {s.label && <span className="sb-label">{s.label}:</span>}
            <span className="sb-value">{s.value}</span>
          </StatusItem>
        </span>
      ),
    })),
    ...(w.pos ? [{ key: "caret", node: <StatusItem><span className="sb-value">{w.pos}</span></StatusItem> }] : []),
    ...(readOnly ? [{ key: "ro", node: <StatusItem>🔒 Read-only</StatusItem> }] : []),
    ...(gone.has("utf8") ? [] : [{ key: "utf8", node: <StatusItem>UTF-8</StatusItem> }]),
  ];
  return (
    // A press on the bar leaves the focus where it is (the grid's items stay; its buttons still take clicks)
    <div className="statusbar" ref={ref} onContextMenu={openMenu} onMouseDown={(e) => e.preventDefault()}>
      {busy ? (
        <RunningBar label={busy.label} since={busy.since} onCancel={busy.onCancel} />
      ) : (
        <Text truncate className={cx("sb-text", drop >= order.length && "sb-text-squeezed")}>
          {status}
        </Text>
      )}
      <Spacer />
      <TransferBar jobs={jobs} />
      {update && (
        <StatusItem accent title="A new version is available" onClick={onUpdate}>
          ⬆ v{update} available
        </StatusItem>
      )}
      {w.note && <StatusItem>{w.note}</StatusItem>}
      {right.map((r, i) => (
        <Fragment key={r.key}>
          {i > 0 && <span className="sb-sep" />}
          {r.node}
        </Fragment>
      ))}
      {menu && <StatusItemsMenu at={menu} onClose={closeMenu} />}
    </div>
  );
}

/**
 * The status bar's checklist (DataGrip: right-click the status bar): the items by group, Restore Defaults. Opens upward from the
 * click and stays open while items are toggled; ↑↓ move, Space or ↩ toggles, Esc or a click outside closes and gives focus back
 */
function StatusItemsMenu({ at, onClose }: { at: { x: number; y: number }; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const on = useStatusItems();
  const items = shared().statusBarInfo().items;
  const [act, setAct] = useState(0);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    // The TUI may have changed them since
    loadStatusItems();
    const back = document.activeElement as HTMLElement | null;
    ref.current?.focus();
    const down = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && closeRef.current();
    // On the next tick, so the click that opened it isn't one outside
    const t = setTimeout(() => window.addEventListener("mousedown", down, true));
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", down, true);
      back?.focus?.();
    };
  }, []);
  const rows = items.length + 1;
  const save = (p: Promise<void>) => p.catch((e) => setStatusNote(String(e)));
  const run = (i: number) => save(i === items.length ? resetStatusItems() : setStatusItem(items[i].id, !on.includes(items[i].id)));
  let group = "";
  return createPortal(
    <div
      ref={ref}
      className="ui-menu sb-menu"
      role="menu"
      aria-label="Status bar items"
      tabIndex={-1}
      style={{ left: Math.min(at.x, window.innerWidth - 280), bottom: Math.max(4, window.innerHeight - at.y + 4) }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => (e.preventDefault(), e.stopPropagation())}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
        else if (e.key === "ArrowDown") setAct((a) => (a + 1) % rows);
        else if (e.key === "ArrowUp") setAct((a) => (a + rows - 1) % rows);
        else if (e.key === " " || e.key === "Enter") run(act);
        else return;
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {items.map((it, i) => (
        <Fragment key={it.id}>
          {it.group !== group && (group = it.group) && (
            <>
              {i > 0 && <div className="ui-menu-sep" role="separator" />}
              <div className="ui-menu-label-row">{it.group}</div>
            </>
          )}
          <div className={cx("ui-menu-item", act === i && "active")} role="menuitemcheckbox" aria-checked={on.includes(it.id as StatusItemId)} onMouseEnter={() => setAct(i)} onClick={() => run(i)}>
            <span className="ui-menu-label">
              <span className="sb-check">{on.includes(it.id) ? "✓" : ""}</span>
              {it.label}
            </span>
            {it.hint && <span className="ui-menu-key">{it.hint}</span>}
          </div>
        </Fragment>
      ))}
      <div className="ui-menu-sep" role="separator" />
      <div className={cx("ui-menu-item", act === items.length && "active")} role="menuitem" onMouseEnter={() => setAct(items.length)} onClick={() => run(items.length)}>
        <span className="ui-menu-label">
          <span className="sb-check" />
          Restore Defaults
        </span>
      </div>
      <div className="ui-menu-label-row">Space toggles · Esc closes</div>
    </div>,
    document.body,
  );
}

export { StatusBar };
