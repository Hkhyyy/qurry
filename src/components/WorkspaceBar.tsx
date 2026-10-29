import { useState } from "react";
import { initials } from "../lib/workspace";
import { ContextMenu, MenuItem } from "./Explorer";
import { IconButton, RailItem } from "../ui";

export interface WorkspaceInfo {
  id: string;
  name: string;
  /** Data Sources shown in this workspace's explorer */
  connIds: string[];
}

/** Thin workspace list at the far left (like Orca): picking one switches to that workspace's tabs·splits. Right-click = rename·delete */
export function WorkspaceBar(p: {
  list: WorkspaceInfo[];
  current: string;
  /** Open tab count per workspace */
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
        {/* The '+' glyph isn't centered because of the font baseline */}
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
      </IconButton>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
