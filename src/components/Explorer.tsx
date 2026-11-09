import { useEffect, useRef, useState } from "react";
import { ConnectionView, Scope } from "../lib/api";
import { ColorSwatches } from "./ConnectionDialog";
import { collapseTree, NodeInfo, SchemaTree, TreeSelection } from "./SchemaTree";
import { IcCollapseAll, IcMoreV, IcPlus, IcReload } from "../ui/icons";
import { IconButton, Menu as UiMenu, MenuEntry, SearchField, Text } from "../ui";
import { cmd, eventSpec, keyTitle, runById, runKey } from "../lib/shortcuts";
import { clickSelect, NO_SEL, ONE_NODE, withoutNode, type ClickHow, type TreeSel } from "../features/explorer/selection";

export type MenuItem = MenuEntry;

const isMac = navigator.platform.toLowerCase().includes("mac");

interface Props {
  connections: ConnectionView[];
  onAdd: () => void;
  /** Workspace: Data Sources not in this explorer (added from the ＋ menu) */
  others?: ConnectionView[];
  onAddExisting?: (id: string) => void;
  onRefresh: () => void;
  onExport: () => void;
  /** The Import Data Sources submenu, and a call to look for the locations again when the ⋮ menu opens */
  importItems: () => MenuEntry[];
  onImportOpen: () => void;
  /** The workspace shown: only its explorer tells onSelect (each workspace keeps its own explorer, hidden ones mounted) */
  shown: boolean;
  /** info: the node the keys act on (the last clicked); all: every selected node */
  onSelect: (info: NodeInfo | null, all: NodeInfo[]) => void;
  onActivate: (info: NodeInfo, click?: boolean) => void;
  /** A line for the status bar (a key that doesn't work on the selected nodes) */
  onStatus: (text: string) => void;
  /** The node's context menu; with several selected nodes (all), for all of them */
  buildMenu: (info: NodeInfo, all?: NodeInfo[]) => MenuItem[];
  onSetColor: (c: ConnectionView, color: string | null) => void;
  onScopeChange: (c: ConnectionView, scope: Scope) => void;
}

type Menu = { x: number; y: number; items: MenuItem[]; conn?: ConnectionView };

/** Context menu (ui/Menu). Data Source menus get a color picker below */
export function ContextMenu({ menu, onClose, onSetColor }: { menu: Menu; onClose: () => void; onSetColor?: (c: ConnectionView, v: string | null) => void }) {
  return (
    <UiMenu
      items={menu.items}
      at={{ x: menu.x, y: menu.y }}
      onClose={onClose}
      footer={
        menu.conn && onSetColor ? (
          <>
            <Text tone="muted" size="sm">
              Color
            </Text>
            <ColorSwatches
              value={menu.conn.color ?? null}
              onChange={(v) => {
                onSetColor(menu.conn!, v);
                onClose();
              }}
            />
          </>
        ) : undefined
      }
    />
  );
}

export function Explorer(p: Props) {
  const [menu, setMenu] = useState<Menu | null>(null);
  const [more, setMore] = useState(false);
  const [addMenu, setAddMenu] = useState(false);
  const [filter, setFilter] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [sel, setSel] = useState<TreeSel>(NO_SEL);
  // Read in the same event (a right-click selects, then opens the menu for the selection)
  const selRef = useRef(sel);
  const infos = useRef(new Map<string, NodeInfo>()).current;

  useEffect(() => {
    const close = () => {
      setMenu(null);
      setMore(false);
      setAddMenu(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
    };
  }, []);

  // ── Keyboard navigation + Speed search (DataGrip: type directly in the tree) ──
  const treeRef = useRef<HTMLDivElement>(null);
  const [speed, setSpeed] = useState("");
  const rows = () => Array.from(treeRef.current?.querySelectorAll<HTMLElement>(".tree-node") ?? []);
  const selectEl = (el: HTMLElement | undefined) => {
    if (!el) return;
    el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    el.scrollIntoView({ block: "nearest" });
  };
  /** The header's ⋮ menu */
  const moreItems = (): MenuEntry[] => [
    { label: `${showSystem ? "✓ " : "　"}Show System Schemas`, ...cmd("tree.showSystem"), action: () => setShowSystem((v) => !v) },
    { separator: true },
    { label: "Export Data Sources…", ...cmd("ds.export"), action: p.onExport },
    { label: "Import Data Sources", submenu: p.importItems() },
  ];
  const onTreeKey = (e: React.KeyboardEvent) => {
    // A control inside the tree (Allow unencrypted, Install driver, an input) keeps its own keys: Enter presses the button
    if ((e.target as HTMLElement).closest("button, input, select, textarea")) return;
    // The header's commands (the node menu's commands run from the global key handler)
    if (runKey(eventSpec(e.nativeEvent), "tree", () => [{ label: "Collapse All", ...cmd("tree.collapseAll"), action: collapseTree }, ...moreItems()])) return e.preventDefault();
    const list = rows();
    // From the last clicked node when several are selected
    const lead = list.findIndex((el) => el.id && el.id === selRef.current.lead);
    const cur = lead >= 0 ? lead : list.findIndex((el) => el.classList.contains("selected"));
    const el = list[cur];
    const matchesSpeed = (s: string) => list.filter((x) => (x.querySelector(".node-name")?.textContent ?? "").toLowerCase().includes(s.toLowerCase()));
    if (speed && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      // During speed search, next/previous match
      const m = matchesSpeed(speed);
      const i = m.findIndex((x) => x === el);
      selectEl(m[(i + (e.key === "ArrowDown" ? 1 : -1) + m.length) % m.length]);
      e.preventDefault();
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "ArrowDown") {
      selectEl(list[Math.min(cur + 1, list.length - 1)] ?? list[0]);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      selectEl(list[Math.max(cur - 1, 0)]);
      e.preventDefault();
    } else if (e.key === "ArrowRight" && el) {
      const tw = el.querySelector<HTMLElement>(".twisty");
      if (tw && tw.dataset.open === "0") tw.click();
      else selectEl(list[cur + 1]);
      e.preventDefault();
    } else if (e.key === "ArrowLeft" && el) {
      const tw = el.querySelector<HTMLElement>(".twisty");
      if (tw && tw.dataset.open === "1") tw.click();
      else {
        // Move to the parent node (the node above with smaller indentation)
        const indent = parseFloat(el.style.paddingLeft || "0");
        for (let i = cur - 1; i >= 0; i--) if (parseFloat(list[i].style.paddingLeft || "0") < indent) return selectEl(list[i]), e.preventDefault();
      }
      e.preventDefault();
    } else if (e.key === "Enter" && el) {
      const all = selected(selRef.current);
      // Several nodes (DataGrip opens each): Edit Data for all of them, when each is a table or view
      if (all.length > 1) {
        if (!runById(p.buildMenu(infos.get(selRef.current.lead ?? "") ?? all[0], all), "tree.editData")) p.onStatus(ONE_NODE);
      } else el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
      setSpeed("");
      e.preventDefault();
    } else if (e.key === "Escape") {
      setSpeed("");
    } else if (e.key === "Backspace" && speed) {
      setSpeed((s) => s.slice(0, -1));
      e.preventDefault();
    } else if (e.key.length === 1 && /\S/.test(e.key)) {
      const next = speed + e.key;
      setSpeed(next);
      const m = matchesSpeed(next);
      if (m.length) selectEl(m.find((x) => list.indexOf(x) >= cur) ?? m[0]);
      e.preventDefault();
    }
  };

  const selected = (s: TreeSel) => s.ids.flatMap((id) => infos.get(id) ?? []);
  // Read by rows as they unmount (their effect keeps the first render's gone)
  const onSelect = useRef(p.onSelect);
  onSelect.current = p.onSelect;
  const shown = useRef(p.shown);
  shown.current = p.shown;
  const tell = (s: TreeSel) => {
    const lead = s.ids.includes(s.lead ?? "") ? s.lead : s.ids[s.ids.length - 1];
    onSelect.current((lead && infos.get(lead)) || null, selected(s));
  };
  // A hidden workspace's rows still unmount (a data source removed, a refresh): its selection mustn't replace the shown one's.
  // Shown again, its own selection is what the keys act on
  useEffect(() => {
    if (p.shown) tell(selRef.current);
  }, [p.shown]);
  const publish = (next: TreeSel) => {
    selRef.current = next;
    setSel(next);
    if (shown.current) tell(next);
  };
  /** A selected row left the screen: the keys act only on the rows still shown (as the context menu does) */
  const gone = (id: string) => {
    if (selRef.current.ids.includes(id)) publish(withoutNode(selRef.current, id));
  };
  const select = (id: string, info?: NodeInfo, e?: React.MouseEvent) => {
    const cur = selRef.current;
    // The right button (⌃-click on macOS) on a selected node keeps the selection: its menu acts on all of it
    if (e && (e.button === 2 || (isMac && e.ctrlKey)) && cur.ids.includes(id)) return;
    const how: ClickHow = !info || !e ? "one" : (isMac ? e.metaKey : e.ctrlKey) ? "toggle" : e.shiftKey ? "range" : "one";
    // The selectable rows on screen, top to bottom (folders and columns have no id)
    const visible = rows().flatMap((el) => (el.id ? [el.id] : []));
    publish(clickSelect(cur, id, how, visible));
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="explorer-title" title="Database Explorer">
          Database Explorer
        </span>
        <span className="menu-anchor">
          <IconButton size="sm"
            title="Add Data Source"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => (addMenu || p.onImportOpen(), setAddMenu((m) => !m))}
          >
            <IcPlus />
          </IconButton>
          {addMenu && (
            <UiMenu
              onClose={() => setAddMenu(false)}
              items={[
                { label: "New Data Source…", ...cmd("tree.newDs"), action: p.onAdd },
                // DataGrip / IntelliJ / DBeaver data sources, as the ⋮ menu has it
                { label: "Import Data Sources", submenu: p.importItems() },
                ...(p.others?.length
                  ? [{ separator: true } as const, { heading: "Add to this workspace:" }, ...p.others.map((c) => ({ label: c.name, action: () => p.onAddExisting?.(c.id) }))]
                  : []),
              ]}
            />
          )}
        </span>
        <IconButton size="sm" title={keyTitle("Refresh", "refresh")} onClick={p.onRefresh}>
          <IcReload />
        </IconButton>
        <IconButton size="sm" title={keyTitle("Collapse All", "tree.collapseAll")} onClick={collapseTree}>
          <IcCollapseAll />
        </IconButton>
        <span className="menu-anchor">
          <IconButton size="sm"
            title="More"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => (more || p.onImportOpen(), setMore((m) => !m))}
          >
            <IcMoreV />
          </IconButton>
          {more && (
            <UiMenu
              onClose={() => setMore(false)}
              items={moreItems()}
            />
          )}
        </span>
      </div>
      <div className="tree-filter">
        <SearchField placeholder="Filter tables, views, routines (expanded schemas)" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      <TreeSelection.Provider value={{ ids: sel.ids, select, infos, gone }}>
        <div className="tree" tabIndex={0} data-tree="1" ref={treeRef} onKeyDown={onTreeKey} onBlur={() => setSpeed("")}>
          {speed && <div className="speed-search">Search for: <b>{speed}</b></div>}
          {p.connections.length === 0 && (
            <div className="empty">
              No data sources.
              <br />
              <a onClick={p.onAdd}>＋ New data source</a>
              <br />
              <a
                onMouseDown={(e) => e.stopPropagation()}
                onClick={() => (p.onImportOpen(), setAddMenu(true))}
                title="From DataGrip / IntelliJ or DBeaver on this computer, or a file"
              >
                Import data sources…
              </a>
              {!!p.others?.length && (
                <>
                  <br />
                  <br />
                  Or add one to this workspace:
                  {p.others.map((c) => (
                    <div key={c.id}>
                      <a onClick={() => p.onAddExisting?.(c.id)}>＋ {c.name}</a>
                    </div>
                  ))}
                </>
              )}
            </div>
          )}
          {/* One block sized to the longest row: names aren't truncated and scroll horizontally (row background · color stripe run to the end) */}
          <div className="tree-content">
            {p.connections.map((c) => (
              <SchemaTree
                key={c.id}
                conn={c}
                filter={filter}
                showSystem={showSystem}
                onActivate={p.onActivate}
                onScopeChange={p.onScopeChange}
                onConnsChanged={p.onRefresh}
                onMenu={(e, info) => {
                  const all = selected(selRef.current);
                  // The color picker is for one data source
                  setMenu({ x: e.clientX, y: e.clientY, items: p.buildMenu(info, all), conn: info.kind === "conn" && all.length < 2 ? info.conn : undefined });
                }}
              />
            ))}
          </div>
        </div>
      </TreeSelection.Provider>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} onSetColor={p.onSetColor} />}
    </div>
  );
}
