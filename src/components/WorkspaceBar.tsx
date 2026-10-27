import { useState } from "react";
import { initials } from "../lib/workspace";
import { ContextMenu, MenuItem } from "./Explorer";

export interface WorkspaceInfo {
  id: string;
  name: string;
}

/** 맨 왼쪽 얇은 워크스페이스 목록 (Orca 처럼): 고르면 그 워크스페이스의 탭·분할 화면으로. 우클릭 = 이름 바꾸기·지우기 */
export function WorkspaceBar(p: {
  list: WorkspaceInfo[];
  current: string;
  /** 워크스페이스마다 열린 탭 수 */
  counts: Record<string, number>;
  shortcut: (n: number) => string;
  onSwitch: (id: string) => void;
  onNew: () => void;
  onRename: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  return (
    <div className="ws-bar">
      {p.list.map((w, i) => (
        <button
          key={w.id}
          className={"ws-item" + (w.id === p.current ? " on" : "")}
          title={`${w.name} — ${p.counts[w.id] ?? 0} tab(s)${i < 9 ? ` (${p.shortcut(i + 1)})` : ""}`}
          onClick={() => p.onSwitch(w.id)}
          onDoubleClick={() => p.onRename(w.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({
              x: e.clientX,
              y: e.clientY,
              items: [
                { label: "Rename…", action: () => p.onRename(w.id) },
                { label: "Delete Workspace…", danger: true, action: () => p.onDelete(w.id), disabled: p.list.length < 2 },
              ],
            });
          }}
        >
          <span className="ws-badge">{initials(w.name)}</span>
          <span className="ws-name">{w.name}</span>
        </button>
      ))}
      <button className="ws-add" title="New Workspace" onClick={p.onNew}>
        +
      </button>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
