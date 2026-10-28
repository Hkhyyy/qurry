import { useState } from "react";
import { initials } from "../lib/workspace";
import { ContextMenu, MenuItem } from "./Explorer";
import { IconButton, RailItem } from "../ui";

export interface WorkspaceInfo {
  id: string;
  name: string;
  /** 이 워크스페이스의 탐색기에 보일 Data Source */
  connIds: string[];
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
        <RailItem
          key={w.id}
          initials={initials(w.name)}
          label={w.name}
          active={w.id === p.current}
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
        />
      ))}
      <IconButton className="ws-add" title="New Workspace" onClick={p.onNew}>
        {/* 글자 '+' 는 글꼴 기준선 때문에 가운데에 오지 않는다 */}
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
      </IconButton>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
