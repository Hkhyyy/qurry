import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";
import type { ConnectionView } from "../../lib/api";
import { leaves } from "../../lib/panes";
import { closeFocusedValuePanel } from "../../lib/valuePanel";
import { commandsFor, eventSpec, hit, runKey } from "../../lib/shortcuts";
import type { NodeInfo } from "../../components/SchemaTree";
import type { useConnActions } from "../conns/useConnActions";
import type { useConsole } from "../console/useConsole";
import { setDialog, setHistoryFor, setHistoryTab, setKeymapOpen, setSchemeOpen, setSettingsOpen } from "../dialogs/store";
import type { useEditorGroups } from "../editorGroups/useEditorGroups";
import type { useExplorer } from "../explorer/useExplorer";
import { ONE_NODE } from "../explorer/selection";
import type { useSearch } from "../search/useSearch";
import { closeTabs } from "../tabs/ops";
import { useTabsStore } from "../tabs/store";
import type { Group, Tab } from "../tabs/types";
import type { useTabs } from "../tabs/useTabs";
import { useWorkspacesStore } from "../workspaces/store";
import type { useWorkspaces } from "../workspaces/useWorkspaces";

type Deps = {
  conns: ConnectionView[];
  keymapVer: number;
  treeSel: NodeInfo | null;
  treeSelAll: NodeInfo[];
  setStatus: (s: string) => void;
  active: Tab | null;
  focused: Group;
  tabs: Tab[];
  split: boolean;
  toggleExplorer: () => void;
  bumpFont: (d: number) => void;
  setMaximized: Dispatch<SetStateAction<boolean>>;
} & ReturnType<typeof useTabs> &
  ReturnType<typeof useConsole> &
  ReturnType<typeof useWorkspaces> &
  ReturnType<typeof useConnActions> &
  ReturnType<typeof useExplorer> &
  ReturnType<typeof useEditorGroups> &
  ReturnType<typeof useSearch>;

/** Keys outside the keymap table, under the ids the handler below knows them by. They stay fixed: ⇧⎋ is Escape (which always closes),
 *  the Switcher opens on ⌃⇥ and picks when ⌃ is released (a hold-and-release, not a key combination) */
const FIXED = new Map<string, (e: KeyboardEvent) => boolean>([
  // ⇧⎋ Hide Active Tool Window
  ["toolwindow.hide", (e) => e.shiftKey && e.key === "Escape"],
  // ⌃⇥ / ⌃⇧⇥ Switcher
  ["switcher", (e) => e.ctrlKey && !e.metaKey && e.key === "Tab"],
]);

/** The app-wide key handler: command shortcuts of the keymap (lib/shortcuts.ts) and the fixed keys (⇧⇧, ⇧⎋, ⌃⇥). Commands without a
 *  handler here run the entry with their id in Find Action's list or the active tab's menu (global), or in the selected node's menu (tree) */
export function useGlobalShortcuts({
  conns, keymapVer, treeSel, treeSelAll, setStatus, active, focused, tabs, split, toggleExplorer, bumpFont, setMaximized, openFts, setSe, setFindOpen, findOpen, openSwitcher, setRecentOpen, switchWs, activate, openConsole, jumpToConsole, activateNode, deactivate, sqlGenerator, ddlToClipboard, ddlToConsole, showQuickDoc, canDdl, modifyTable, renameNode, duplicateConn, removeConn, dangerous, openDdl, applyTableFilter, refreshAll, canCancel, cancelRun, renameConsole, reopenClosed, buildMenu, paletteActions, tabMenu, ddlCommands,
}: Deps) {
  // The key handler is registered only once, so pass the latest functions via a ref
  const latest = useRef({ toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs, buildMenu, paletteActions, tabMenu, ddlCommands });
  latest.current = { toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs, buildMenu, paletteActions, tabMenu, ddlCommands };
  const lastShift = useRef(0);

  // ───────── Shortcuts (DataGrip keymap) ─────────
  const keyState = useRef({ treeSel, treeSelAll, active, focused, tabs, split });
  keyState.current = { treeSel, treeSelAll, active, focused, tabs, split };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // If the Switcher / Recent Files popup is open, the popup handles keys
      if (document.querySelector(".switcher, .search-everywhere, .modal.fts, .modal.keymap, .keymap-recording")) return;
      const { treeSel, treeSelAll, active, tabs } = keyState.current;
      const L = latest.current;
      // ⇧⇧: Search Everywhere
      if (e.key === "Shift") {
        if (!e.repeat) {
          const now = Date.now();
          if (now - lastShift.current < 400) {
            lastShift.current = 0;
            L.setSe("all");
          } else lastShift.current = now;
        }
        return;
      }
      lastShift.current = 0;
      // Command shortcuts are in lib/shortcuts.ts (changed in Settings › Keymap)
      const spec = eventSpec(e);
      const is = (id: string) => hit(spec, id);
      const inTree = (document.activeElement as HTMLElement | null)?.closest?.("[data-tree]");
      /** Console (editor-scope) commands handled here: the console's editor or its tab has the keys (a clicked tab leaves the focus on
       *  the page), not a result grid or the tree */
      const ae = document.activeElement as HTMLElement | null;
      const inConsole = !ae || ae === document.body || (!!ae.closest?.(".monaco-editor, .editor-group > .tabs") && !ae.closest?.(".grid-wrap"));
      const inInput = ["INPUT", "TEXTAREA", "SELECT"].includes((document.activeElement as HTMLElement | null)?.tagName ?? "");
      /** Several nodes selected: the explorer's commands run from the selection's menu (for all of them, or greyed out) */
      const many = inTree && treeSel && !inInput && treeSelAll.length > 1 ? treeSelAll : null;
      /** Tree commands: one node is selected and the explorer (not one of its fields) has the keys */
      const node = inTree && treeSel && !inInput && !many ? treeSel : null;
      const act = (fn: () => void) => {
        e.preventDefault();
        e.stopPropagation();
        fn();
      };
      const tabStep = () => {
        if (!active) return false;
        e.preventDefault();
        const g = tabs.filter((t) => t.group === active.group);
        const i = g.findIndex((t) => t.id === active.id);
        const n = g[(i + (is("tab.prev") ? -1 : 1) + g.length) % g.length];
        if (n) activate(n);
      };
      // ⌘1~9: workspaces (explorer is ⌘0)
      const workspaceKeys = Object.fromEntries(
        [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [`ws.${n}`, () => void (e.preventDefault(), useWorkspacesStore.getState().workspaces[n - 1] && L.switchWs(useWorkspacesStore.getState().workspaces[n - 1].id))]),
      );
      // Action id → what it does, tried in this order: the first one whose key this is, and that doesn't answer false, has the key
      const actions: Record<string, () => void | false> = {
        "goto.object": () => void (e.preventDefault(), L.setSe("database")),
        "goto.symbol": () => void (e.preventDefault(), L.setSe("symbols")),
        "goto.file": () => void (e.preventDefault(), L.setSe("files")),
        "find.action": () => void (e.preventDefault(), L.setSe("actions")),
        fts: () => void (e.preventDefault(), L.openFts()),
        explorer: () => void (e.preventDefault(), L.toggleExplorer()),
        ...workspaceKeys,
        "find.window": () => void (e.preventDefault(), L.setFindOpen((o) => !o)),
        // ⇧⎋ Hide Active Tool Window
        "toolwindow.hide": () => {
          if (closeFocusedValuePanel()) return void e.preventDefault();
          const ae = document.activeElement;
          if (ae?.closest?.("[data-toolwindow=find]")) return void (e.preventDefault(), L.setFindOpen(false));
          if (ae?.closest?.("[data-tree]")) return void (e.preventDefault(), L.toggleExplorer());
          return false;
        },
        "font.bigger": () => void (e.preventDefault(), L.bumpFont(1)),
        "font.smaller": () => void (e.preventDefault(), L.bumpFont(-1)),
        "console.new": () => {
          e.preventDefault();
          const conn = treeSel?.conn ?? conns.find((c) => c.id === active?.connId);
          if (conn) openConsole(conn, treeSel?.db ?? active?.database);
        },
        // Edit Data (default console for a data source)
        "tree.editData": () => (node ? act(() => ((node.kind === "conn" || node.kind === "db") && node.conn.kind !== "redis" ? jumpToConsole(node) : activateNode(node))) : false),
        "tree.properties": () => (node && node.kind === "conn" ? act(() => setDialog({ edit: node.conn })) : false),
        "tree.refresh": () => (node ? act(() => node.refresh?.()) : false),
        "tree.disconnect": () => (node && node.conn.connected ? act(() => deactivate(node.conn)) : false),
        "tree.jumpConsole": () => (node ? act(() => jumpToConsole(node)) : false),
        "tree.sqlGen": () => (node && node.name ? act(() => sqlGenerator(node)) : false),
        "tree.ddlCopy": () => (node && node.name ? act(() => ddlToClipboard(node)) : false),
        "tree.ddlConsole": () => (node && node.name ? act(() => ddlToConsole(node)) : false),
        "tree.quickDoc": () => (node ? act(() => showQuickDoc(node)) : false),
        "tree.modify": () => (node && (node.kind === "table" || node.kind === "partitioned") && canDdl(node.conn) ? act(() => modifyTable(node)) : false),
        "tree.rename": () => (node ? act(() => renameNode(node)) : false),
        "tree.newDs": () => (node ? act(() => setDialog({})) : false),
        "tree.duplicate": () => (node && node.kind === "conn" ? act(() => duplicateConn(node.conn)) : false),
        "tree.remove": () => (node ? act(() => (node.kind === "conn" ? removeConn(node.conn) : node.name ? dangerous([node], "DROP") : undefined)) : false),
        "tree.ddl": () => {
          if (!node?.name) return false;
          e.preventDefault();
          openDdl(node);
        },
        refresh: () => {
          e.preventDefault();
          if (many) many.forEach((n) => n.refresh?.());
          else if (inTree && treeSel?.refresh) treeSel.refresh();
          else if (active?.kind === "table") applyTableFilter(active, active.where, active.orderBy);
          else refreshAll();
        },
        "editor.history": () => {
          if (active?.kind !== "console" || !inConsole) return false;
          e.preventDefault();
          setHistoryTab("history");
          setHistoryFor(active.id);
        },
        "editor.cancel": () => {
          if (!(!inTree && active && canCancel(active))) return false;
          e.preventDefault();
          cancelRun(active);
        },
        // ⇧F6 = rename console (same as Rename File… in the tab menu)
        "console.rename": () => {
          if (active?.kind !== "console" || !inConsole) return false;
          e.preventDefault();
          renameConsole(active);
        },
        // The focused grid's Value Editor panel closes first (focus in the grid or the panel), then the tab
        "tab.close": () => {
          if (closeFocusedValuePanel()) return void e.preventDefault();
          if (!active) return false;
          e.preventDefault();
          closeTabs([active.id]);
        },
        // ⌃⇥ / ⌃⇧⇥: Switcher (most recently used order)
        switcher: () => {
          e.preventDefault();
          e.stopPropagation();
          openSwitcher(e.shiftKey);
        },
        keymap: () => {
          e.preventDefault();
          setKeymapOpen(true);
        },
        settings: () => {
          e.preventDefault();
          setSettingsOpen("appearance");
        },
        theme: () => {
          e.preventDefault();
          setSchemeOpen(true);
        },
        recent: () => {
          e.preventDefault();
          e.stopPropagation();
          setRecentOpen(true);
        },
        "tab.next": tabStep,
        "tab.prev": tabStep,
        "tab.reopen": () => {
          e.preventDefault();
          reopenClosed();
        },
        maximize: () => {
          e.preventDefault();
          setMaximized((m) => !m);
        },
        // Move between split editor groups (Goto Next Splitter)
        "split.other": () => {
          if (!keyState.current.split) return false;
          e.preventDefault();
          // Next pane (previous with ⇧)
          const ls = leaves(useTabsStore.getState().panes);
          const g: Group = ls[(ls.indexOf(keyState.current.focused) + (e.shiftKey ? -1 : 1) + ls.length) % ls.length];
          const id = useTabsStore.getState().activeIds[g];
          const t = tabs.find((x) => x.id === id);
          if (t) activate(t);
        },
      };
      for (const [id, run] of Object.entries(actions)) if ((FIXED.get(id)?.(e) ?? is(id)) && run() !== false) return;
      if (many && commandsFor(spec, "tree").length) {
        e.preventDefault();
        e.stopPropagation();
        if (!runKey(spec, "tree", () => L.buildMenu(treeSel!, many))) setStatus(ONE_NODE);
        return;
      }
      // Commands without a handler above: the menu or Find Action entry with their id
      if ((node && runKey(spec, "tree", () => L.buildMenu(node))) || runKey(spec, "global", () => [...L.paletteActions(), ...(active ? [...L.tabMenu(active), ...L.ddlCommands(active)] : [])])) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // keymapVer: menus opened from keys (⇧⌘F10 …) show the current shortcut labels
  }, [conns, keymapVer]);
}
