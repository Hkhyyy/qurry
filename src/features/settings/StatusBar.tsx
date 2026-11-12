import { Fragment, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cx, Spacer, StatusItem, Text } from "../../ui";
import { cellText, holdFocusWidgets, loadStatusItems, resetStatusItems, setStatusItem, setStatusNote, useStatusItems, useStatusWidgets, type FocusWidgets } from "../../lib/statusbar";
import { shared, useSharedReady, type StatusItemId, type StatusPart, type StatusSegment } from "../../lib/shared";
import { TransferBar, type TransferJob } from "../../components/Transfer";
import { RunningBar } from "../../components/RunningBar";

/** What the bar holds that can go when it is too narrow: the status bar segments and the encoding label */
type Droppable = StatusPart | "utf8";

/** The focused grid's segments for the items on (qurry_shared::statusbar::segments). Without the WebAssembly module (or if it throws):
 *  its current cell */
function useSegments(w: FocusWidgets, items?: StatusItemId[]): StatusSegment[] {
  const saved = useStatusItems();
  const on = items ?? saved;
  const ready = useSharedReady();
  const g = w.grid;
  if (!g) return [];
  if (ready) {
    try {
      return shared().statusSegments(on, g);
    } catch (e) {
      console.error(`The status bar items could not be worked out (${e})`);
    }
  }
  const [r, c] = g.pos ?? [];
  return r && c && r.count === 1 && c.count === 1 ? [{ id: "position", value: cellText(r.first, c.first) }] : [];
}

/**
 * Information only (DataGrip): what the focused tab is doing (`busy`: a run, a load-more, and since when, shown instead of the status
 * text; its ✕ on hover cancels it), then on the right the focused grid's items (each its own segment: Position, Shown columns and the
 * ones the user turned on), the focused editor's position, Read-only, UTF-8. Right-click the bar, or click an item, for the checklist
 * of items (shared with Settings › Appearance › Data grid and the TUI). When it is too narrow the status text shrinks to 140 px first, then the
 * segments go in qurry_shared::statusbar::DROP order after UTF-8; Position and Read-only stay. `widgets` and `items`: what Storybook shows
 * instead of the focused editor's or grid's, and of the items saved
 */
function StatusBar({ status, busy, readOnly, update, onUpdate, jobs, widgets, items }: { status: string; busy?: { label: string; since: number; onCancel: () => void }; readOnly: boolean; update?: string; onUpdate: () => void; jobs: TransferJob[]; widgets?: FocusWidgets; items?: StatusItemId[] }) {
  const live = useStatusWidgets();
  const w = widgets ? { ...live, ...widgets } : live;
  const [menu, setMenu] = useState<{ x: number; barTop: number } | null>(null);
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
  // A shorter value can bring back what went: the values count too
  const key = [all.map((s) => `${s.id}:${s.value}`).join(), status, busy?.label, update, w.pos, readOnly, jobs.length].join("\u0001");
  useLayoutEffect(() => setDrop(0), [key]);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && el.scrollWidth > el.clientWidth + 1 && drop < order.length) setDrop(drop + 1);
  });
  // The items and their names come from the WebAssembly module: without it there is no checklist
  const openMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    if (!ready) return;
    setMenu({ x: e.clientX, barTop: ref.current?.getBoundingClientRect().top ?? window.innerHeight });
  };
  const right: { key: string; node: React.ReactNode }[] = [
    ...segs.map((s) => ({
      key: s.id,
      node: (
        // Not a button: a click (or right-click) on the bar opens the checklist; Settings › Appearance › Data grid is the keyboard's way there
        <span className="sb-seg" onClick={openMenu}>
          <StatusItem>
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
      {menu && <StatusItemsMenu at={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}

/**
 * The status bar's checklist (DataGrip: right-click the status bar): the items by group, Restore Defaults. Opens just above the bar
 * from the click, inside the window, and stays open while items are toggled; ↑↓ Home End move, Space or ↩ toggles, Esc, ⇥ or a click
 * outside closes and gives focus back
 */
function StatusItemsMenu({ at, onClose }: { at: { x: number; barTop: number }; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const id = useId();
  // Placed once measured (before it is painted): its left edge at the click, kept inside the window, its bottom 2 px above the bar
  const [place, setPlace] = useState<{ left: number; bottom: number } | null>(null);
  useLayoutEffect(() => {
    const w = ref.current?.getBoundingClientRect().width ?? 0;
    setPlace({ left: Math.max(4, Math.min(at.x, window.innerWidth - w - 4)), bottom: window.innerHeight - at.barTop + 2 });
  }, []);
  const on = useStatusItems();
  const items = shared().statusBarInfo().items;
  const [act, setAct] = useState(0);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    // The TUI may have changed them since
    loadStatusItems();
    const back = document.activeElement as HTMLElement | null;
    // It takes the focus: the bar keeps showing the grid's items meanwhile, until the focus is back there
    holdFocusWidgets(true);
    ref.current?.focus();
    const down = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && closeRef.current();
    // On the next tick, so the click that opened it isn't one outside
    const t = setTimeout(() => window.addEventListener("mousedown", down, true));
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", down, true);
      back?.focus?.();
      holdFocusWidgets(false);
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
      aria-activedescendant={`${id}-${act}`}
      // Measured at the left edge first (its natural width), then placed before it is painted
      style={place ?? { left: 0, bottom: window.innerHeight - at.barTop + 2 }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => (e.preventDefault(), e.stopPropagation())}
      onKeyDown={(e) => {
        if (e.key === "Escape" || e.key === "Tab") onClose();
        else if (e.key === "Home") setAct(0);
        else if (e.key === "End") setAct(rows - 1);
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
              <div className="ui-menu-label-row" role="none">
                {it.group}
              </div>
            </>
          )}
          <div id={`${id}-${i}`} className={cx("ui-menu-item", act === i && "active")} role="menuitemcheckbox" aria-checked={on.includes(it.id)} onMouseEnter={() => setAct(i)} onClick={() => run(i)}>
            <span className="ui-menu-label">
              <span className="sb-check">{on.includes(it.id) ? "✓" : ""}</span>
              {it.label}
            </span>
            {it.hint && <span className="ui-menu-key">{it.hint}</span>}
          </div>
        </Fragment>
      ))}
      <div className="ui-menu-sep" role="separator" />
      <div id={`${id}-${items.length}`} className={cx("ui-menu-item", act === items.length && "active")} role="menuitem" onMouseEnter={() => setAct(items.length)} onClick={() => run(items.length)}>
        <span className="ui-menu-label">
          <span className="sb-check" />
          Restore Defaults
        </span>
      </div>
      <div className="ui-menu-label-row" role="none">
        Space toggles · Esc closes
      </div>
    </div>,
    document.body,
  );
}

export { StatusBar };
