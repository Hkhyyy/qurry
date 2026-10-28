import { useEffect, useRef, useState } from "react";
import { ConnectionView, Scope } from "../lib/api";
import { ColorSwatches } from "./ConnectionDialog";
import { NodeInfo, SchemaTree, TreeSelection } from "./SchemaTree";
import { IconButton } from "../ui";

export type MenuItem =
  | { label: string; action: () => void; shortcut?: string; danger?: boolean; disabled?: boolean }
  | { separator: true }
  | { label: string; submenu: MenuItem[] };

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

export function ContextMenu({ menu, onClose, onSetColor }: { menu: Menu; onClose: () => void; onSetColor?: (c: ConnectionView, v: string | null) => void }) {
  const [sub, setSub] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  // 어디서 열었든: 메뉴(하위 메뉴 포함) 바깥 클릭, ⎋, 창 포커스 이탈 시 닫는다
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) closeRef.current();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        closeRef.current();
      }
    };
    const onBlur = () => closeRef.current();
    // 메뉴를 연 클릭이 곧바로 "바깥 클릭"으로 처리되지 않도록 다음 틱에 등록
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
  // 화면 밖으로 나가지 않게 위치 보정
  const x = Math.min(menu.x, window.innerWidth - 260);
  const y = Math.min(menu.y, window.innerHeight - 24 * menu.items.length - 40);
  return (
    <div className="ctx-menu" ref={ref} style={{ left: x, top: Math.max(4, y) }} onMouseDown={(e) => e.stopPropagation()}>
      {menu.items.map((it, i) => {
        if ("separator" in it) return <div key={i} className="ctx-sep" />;
        if ("submenu" in it)
          return (
            <div key={i} className="ctx-item has-sub" onMouseEnter={() => setSub(i)}>
              <span>{it.label}</span>
              <span className="ctx-arrow">▸</span>
              {sub === i && (
                <div className="ctx-menu sub">
                  {it.submenu.map((s, j) =>
                    "separator" in s ? (
                      <div key={j} className="ctx-sep" />
                    ) : "action" in s ? (
                      <div
                        key={j}
                        className={"ctx-item" + (s.danger ? " danger" : "")}
                        onClick={() => {
                          s.action();
                          onClose();
                        }}
                      >
                        <span>{s.label}</span>
                        {s.shortcut && <span className="ctx-key">{s.shortcut}</span>}
                      </div>
                    ) : null,
                  )}
                </div>
              )}
            </div>
          );
        return (
          <div
            key={i}
            className={"ctx-item" + (it.danger ? " danger" : "") + (it.disabled ? " disabled" : "")}
            onMouseEnter={() => setSub(null)}
            onClick={() => {
              if (it.disabled) return;
              it.action();
              onClose();
            }}
          >
            <span>{it.label}</span>
            {it.shortcut && <span className="ctx-key">{it.shortcut}</span>}
          </div>
        );
      })}
      {menu.conn && onSetColor && (
        <>
          <div className="ctx-sep" />
          <div className="menu-colors">
            <span className="muted">Color</span>
            <ColorSwatches
              value={menu.conn.color ?? null}
              onChange={(v) => {
                onSetColor(menu.conn!, v);
                onClose();
              }}
            />
          </div>
        </>
      )}
    </div>
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
            <div className="ctx-menu dropdown" onMouseDown={(e) => e.stopPropagation()}>
              <div className="ctx-item" onClick={() => { p.onAdd(); setAddMenu(false); }}><span>New Data Source…</span></div>
              <div className="ctx-sep" />
              <div className="ctx-item muted-label"><span>Add to this workspace:</span></div>
              {p.others?.map((c) => (
                <div key={c.id} className="ctx-item" onClick={() => { p.onAddExisting?.(c.id); setAddMenu(false); }}><span>{c.name}</span></div>
              ))}
            </div>
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
            <div className="ctx-menu dropdown" onMouseDown={(e) => e.stopPropagation()}>
              <div className="ctx-item" onClick={() => { setShowSystem((v) => !v); setMore(false); }}>
                <span>{showSystem ? "✓ " : "　"}Show System Schemas</span>
              </div>
              <div className="ctx-sep" />
              <div className="ctx-item" onClick={() => { p.onExport(); setMore(false); }}><span>Export Data Sources…</span></div>
              <div className="ctx-item" onClick={() => { p.onImport(); setMore(false); }}><span>Import Data Sources…</span></div>
            </div>
          )}
        </span>
      </div>
      <div className="tree-filter">
        <input placeholder="Filter tables, views, routines (expanded schemas)" value={filter} onChange={(e) => setFilter(e.target.value)} />
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
