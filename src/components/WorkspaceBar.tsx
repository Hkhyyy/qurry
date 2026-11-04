import { useEffect, useRef, useState } from "react";
import { cmd, keyTitle } from "../lib/shortcuts";
import { initials } from "../lib/workspace";
import { ContextMenu, MenuItem } from "./Explorer";
import { IconButton, RailItem } from "../ui";
import { IcGear, IcKeyboard } from "../ui/icons";

export interface WorkspaceInfo {
  id: string;
  name: string;
  /** Data Sources shown in this workspace's explorer */
  connIds: string[];
}

/** Thin workspace list at the far left (like Orca): picking one switches to that workspace's tabs·splits. Right-click = rename·delete.
 *  The list scrolls; Keymap Reference and Settings stay pinned at the bottom (the gear of JetBrains' and VS Code's activity bar) */
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
  onKeymap: () => void;
  onSettings: () => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  // Keep the current workspace in view when it changes (⌘1…⌘9, a new workspace at the end of a long list)
  useEffect(() => {
    listRef.current?.querySelector(".ui-rail-item.active")?.scrollIntoView({ block: "nearest" });
  }, [p.current]);
  return (
    <div className="ws-bar">
      <div className="ws-list" ref={listRef}>
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
                  { label: "Rename…", ...cmd("ws.rename"), action: () => p.onRename(w.id) },
                  { label: "Delete Workspace…", ...cmd("ws.delete"), danger: true, action: () => p.onDelete(w.id), disabled: p.list.length < 2 },
                ],
              });
            }}
          />
        ))}
        <IconButton className="ws-add" title={keyTitle("New Workspace", "ws.new")} onClick={p.onNew}>
          {/* The '+' glyph isn't centered because of the font baseline */}
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
            <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </IconButton>
      </div>
      <div className="ws-foot">
        <IconButton title={keyTitle("Keymap Reference", "keymap")} onClick={p.onKeymap}>
          <IcKeyboard />
        </IconButton>
        <IconButton title={keyTitle("Settings", "settings")} onClick={p.onSettings}>
          <IcGear />
        </IconButton>
      </div>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
