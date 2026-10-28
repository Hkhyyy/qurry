import { useEffect, useRef, useState } from "react";
import { ConnectionView, Scope } from "../lib/api";
import { ColorSwatches } from "./ConnectionDialog";
import { NodeInfo, SchemaTree, TreeSelection } from "./SchemaTree";
import { IconButton, Menu as UiMenu, MenuEntry, SearchField, Text } from "../ui";

export type MenuItem = MenuEntry;

interface Props {
  connections: ConnectionView[];
  onAdd: () => void;
  /** 워크스페이스: 이 탐색기에 없는 Data Source (＋ 메뉴에서 넣는다) */
  others?: ConnectionView[];
  onAddExisting?: (id: string) => void;
  onRefresh: () => void;
  onExport: () => void;
  onImport: () => void;
  onSelect: (info: NodeInfo | null) => void;
  onActivate: (info: NodeInfo) => void;
  buildMenu: (info: NodeInfo) => MenuItem[];
  onSetColor: (c: ConnectionView, color: string | null) => void;
  onScopeChange: (c: ConnectionView, scope: Scope) => void;
}

type Menu = { x: number; y: number; items: MenuItem[]; conn?: ConnectionView };

/** 우클릭 메뉴 (ui/Menu). Data Source 메뉴면 아래에 색 고르기 */
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
  const [sel, setSelId] = useState<string | null>(null);

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

  // ── 키보드 탐색 + Speed search (DataGrip: 트리에서 바로 타이핑) ──
  const treeRef = useRef<HTMLDivElement>(null);
  const [speed, setSpeed] = useState("");
  const rows = () => Array.from(treeRef.current?.querySelectorAll<HTMLElement>(".tree-node") ?? []);
  const selectEl = (el: HTMLElement | undefined) => {
    if (!el) return;
    el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    el.scrollIntoView({ block: "nearest" });
  };
  const onTreeKey = (e: React.KeyboardEvent) => {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const list = rows();
    const cur = list.findIndex((el) => el.classList.contains("selected"));
    const el = list[cur];
    const matchesSpeed = (s: string) => list.filter((x) => (x.querySelector(".node-name")?.textContent ?? "").toLowerCase().includes(s.toLowerCase()));
    if (speed && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      // speed search 중에는 다음/이전 일치 항목
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
        // 부모 노드로 이동 (들여쓰기가 더 작은 위쪽 노드)
        const indent = parseFloat(el.style.paddingLeft || "0");
        for (let i = cur - 1; i >= 0; i--) if (parseFloat(list[i].style.paddingLeft || "0") < indent) return selectEl(list[i]), e.preventDefault();
      }
      e.preventDefault();
    } else if (e.key === "Enter" && el) {
      el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
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

  const setSel = (id: string, info?: NodeInfo) => {
    setSelId(id);
    p.onSelect(info ?? null);
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="explorer-title" title="Database Explorer">
          Database Explorer
        </span>
        <span className="menu-anchor">
          <IconButton
            title="Add Data Source"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => (p.others?.length ? setAddMenu((m) => !m) : p.onAdd())}
          >
            ＋
          </IconButton>
          {addMenu && (
            <UiMenu
              onClose={() => setAddMenu(false)}
              items={[
                { label: "New Data Source…", action: p.onAdd },
                { separator: true },
                { heading: "Add to this workspace:" },
                ...(p.others ?? []).map((c) => ({ label: c.name, action: () => p.onAddExisting?.(c.id) })),
              ]}
            />
          )}
        </span>
        <IconButton title="Refresh (⌥⌘y)" onClick={p.onRefresh}>⟳</IconButton>
        <span className="menu-anchor">
          <IconButton
            title="More"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => setMore((m) => !m)}
          >
            ⋯
          </IconButton>
          {more && (
            <UiMenu
              onClose={() => setMore(false)}
              items={[
                { label: `${showSystem ? "✓ " : "　"}Show System Schemas`, action: () => setShowSystem((v) => !v) },
                { separator: true },
                { label: "Export Data Sources…", action: p.onExport },
                { label: "Import Data Sources…", action: p.onImport },
              ]}
            />
          )}
        </span>
      </div>
      <div className="tree-filter">
        <SearchField placeholder="Filter tables, views, routines (expanded schemas)" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      <TreeSelection.Provider value={{ sel, setSel }}>
        <div className="tree" tabIndex={0} data-tree="1" ref={treeRef} onKeyDown={onTreeKey} onBlur={() => setSpeed("")}>
          {speed && <div className="speed-search">Search for: <b>{speed}</b></div>}
          {p.connections.length === 0 && (
            <div className="empty">
              No data sources.
              <br />
              <a onClick={p.onAdd}>＋ New data source</a>
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
          {p.connections.map((c) => (
            <SchemaTree
              key={c.id}
              conn={c}
              filter={filter}
              showSystem={showSystem}
              onActivate={p.onActivate}
              onScopeChange={p.onScopeChange}
              onMenu={(e, info) => setMenu({ x: e.clientX, y: e.clientY, items: p.buildMenu(info), conn: info.kind === "conn" ? info.conn : undefined })}
            />
          ))}
        </div>
      </TreeSelection.Provider>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} onSetColor={p.onSetColor} />}
    </div>
  );
}
