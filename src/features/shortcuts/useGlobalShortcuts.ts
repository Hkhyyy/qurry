import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";
import type { ConnectionView } from "../../lib/api";
import { leaves } from "../../lib/panes";
import { eventSpec, hit } from "../../lib/shortcuts";
import type { NodeInfo } from "../../components/SchemaTree";
import type { useConnActions } from "../conns/useConnActions";
import type { useConsole } from "../console/useConsole";
import { setDialog, setHistoryFor, setHistoryTab, setKeymapOpen, setSchemeOpen, setSettingsOpen } from "../dialogs/store";
import type { useEditorGroups } from "../editorGroups/useEditorGroups";
import type { useExplorer } from "../explorer/useExplorer";
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

/** The app-wide key handler: command shortcuts of the keymap (lib/shortcuts.ts) and the fixed keys (⇧⇧, ⇧⎋, ⌃⇥, ⇧F6) */
export function useGlobalShortcuts({
  conns, keymapVer, treeSel, active, focused, tabs, split, toggleExplorer, bumpFont, setMaximized, openFts, setSe, setFindOpen, findOpen, openSwitcher, setRecentOpen, switchWs, activate, openConsole, jumpToConsole, activateNode, deactivate, sqlGenerator, ddlToClipboard, ddlToConsole, showQuickDoc, canDdl, modifyTable, renameNode, duplicateConn, removeConn, dangerous, openDdl, applyTableFilter, refreshAll, canCancel, cancelRun, renameConsole, reopenClosed,
}: Deps) {
  // The key handler is registered only once, so pass the latest functions via a ref
  const latest = useRef({ toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs });
  latest.current = { toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs };
  const lastShift = useRef(0);

  // ───────── Shortcuts (DataGrip keymap) ─────────
  const keyState = useRef({ treeSel, active, focused, tabs, split });
  keyState.current = { treeSel, active, focused, tabs, split };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // If the Switcher / Recent Files popup is open, the popup handles keys
      if (document.querySelector(".switcher, .search-everywhere, .modal.fts, .modal.keymap, .keymap-recording")) return;
      const { treeSel, active, tabs } = keyState.current;
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
      if (is("goto.object")) return e.preventDefault(), L.setSe("database");
      if (is("goto.symbol")) return e.preventDefault(), L.setSe("symbols");
      if (is("goto.file")) return e.preventDefault(), L.setSe("files");
      if (is("find.action")) return e.preventDefault(), L.setSe("actions");
      if (is("fts")) return e.preventDefault(), void L.openFts();
      if (is("explorer")) return e.preventDefault(), L.toggleExplorer();
      // ⌘1~9: workspaces (explorer is ⌘0)
      for (let n = 1; n <= 9; n++) if (is(`ws.${n}`)) return e.preventDefault(), void (useWorkspacesStore.getState().workspaces[n - 1] && L.switchWs(useWorkspacesStore.getState().workspaces[n - 1].id));
      if (is("find.window")) return e.preventDefault(), L.setFindOpen((o) => !o);
      // ⇧⎋ Hide Active Tool Window
      if (e.shiftKey && e.key === "Escape") {
        const ae = document.activeElement;
        if (ae?.closest?.("[data-toolwindow=find]")) return e.preventDefault(), L.setFindOpen(false);
        if (ae?.closest?.("[data-tree]")) return e.preventDefault(), L.toggleExplorer();
      }
      if (is("font.bigger")) return e.preventDefault(), L.bumpFont(1);
      if (is("font.smaller")) return e.preventDefault(), L.bumpFont(-1);
      const inTree = (document.activeElement as HTMLElement | null)?.closest?.("[data-tree]");
      const inInput = ["INPUT", "TEXTAREA", "SELECT"].includes((document.activeElement as HTMLElement | null)?.tagName ?? "");
      if (is("console.new")) {
        e.preventDefault();
        const conn = treeSel?.conn ?? conns.find((c) => c.id === active?.connId);
        if (conn) openConsole(conn, treeSel?.db ?? active?.database);
        return;
      }
      if (inTree && treeSel && !inInput) {
        const act = (fn: () => void) => {
          e.preventDefault();
          e.stopPropagation();
          fn();
        };
        // Edit Data (default console for a data source)
        if (is("tree.editData"))
          return act(() => ((treeSel.kind === "conn" || treeSel.kind === "db") && treeSel.conn.kind !== "redis" ? jumpToConsole(treeSel) : activateNode(treeSel)));
        if (is("tree.properties") && treeSel.kind === "conn") return act(() => setDialog({ edit: treeSel.conn }));
        if (is("tree.refresh")) return act(() => treeSel.refresh?.());
        if (is("tree.disconnect") && treeSel.conn.connected) return act(() => deactivate(treeSel.conn));
        if (is("tree.jumpConsole")) return act(() => jumpToConsole(treeSel));
        if (is("tree.sqlGen") && treeSel.name) return act(() => sqlGenerator(treeSel));
        if (is("tree.ddlCopy") && treeSel.name) return act(() => ddlToClipboard(treeSel));
        if (is("tree.ddlConsole") && treeSel.name) return act(() => ddlToConsole(treeSel));
        if (is("tree.quickDoc")) return act(() => showQuickDoc(treeSel));
        if (is("tree.modify") && (treeSel.kind === "table" || treeSel.kind === "partitioned") && canDdl(treeSel.conn)) return act(() => modifyTable(treeSel));
        if (is("tree.rename")) return act(() => renameNode(treeSel));
        if (is("tree.newDs")) return act(() => setDialog({}));
        if (is("tree.duplicate") && treeSel.kind === "conn") return act(() => duplicateConn(treeSel.conn));
        if (is("tree.remove"))
          return act(() => (treeSel.kind === "conn" ? removeConn(treeSel.conn) : treeSel.name ? dangerous(treeSel, "DROP") : undefined));
      }
      if (is("tree.ddl") && inTree && treeSel && treeSel.name) {
        e.preventDefault();
        openDdl(treeSel);
        return;
      }
      if (is("tree.copyRef") && inTree && treeSel) {
        e.preventDefault();
        navigator.clipboard.writeText(treeSel.name ? `${treeSel.schema}.${treeSel.name}` : treeSel.schema ?? treeSel.db ?? treeSel.conn.name);
        return;
      }
      if (is("refresh")) {
        e.preventDefault();
        if (inTree && treeSel?.refresh) treeSel.refresh();
        else if (active?.kind === "table") applyTableFilter(active, active.where, active.orderBy);
        else refreshAll();
        return;
      }
      if (is("editor.history") && active?.kind === "console") {
        e.preventDefault();
        setHistoryTab("history");
        setHistoryFor(active.id);
        return;
      }
      if (is("editor.cancel") && !inTree && active && canCancel(active)) {
        e.preventDefault();
        cancelRun(active);
        return;
      }
      // ⇧F6 = rename console (same as Rename File… in the tab menu)
      if (e.key === "F6" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && active?.kind === "console") {
        e.preventDefault();
        renameConsole(active);
        return;
      }
      if (is("tab.close") && active) {
        e.preventDefault();
        closeTabs([active.id]);
        return;
      }
      // ⌃⇥ / ⌃⇧⇥: Switcher (most recently used order)
      if (e.ctrlKey && !e.metaKey && e.key === "Tab") {
        e.preventDefault();
        e.stopPropagation();
        openSwitcher(e.shiftKey);
        return;
      }
      if (is("keymap")) {
        e.preventDefault();
        setKeymapOpen(true);
        return;
      }
      if (is("settings")) {
        e.preventDefault();
        setSettingsOpen("appearance");
        return;
      }
      if (is("theme")) {
        e.preventDefault();
        setSchemeOpen(true);
        return;
      }
      if (is("recent")) {
        e.preventDefault();
        e.stopPropagation();
        setRecentOpen(true);
        return;
      }
      if ((is("tab.next") || is("tab.prev")) && active) {
        e.preventDefault();
        const g = tabs.filter((t) => t.group === active.group);
        const i = g.findIndex((t) => t.id === active.id);
        const n = g[(i + (is("tab.prev") ? -1 : 1) + g.length) % g.length];
        if (n) activate(n);
        return;
      }
      if (is("tab.reopen")) {
        e.preventDefault();
        reopenClosed();
        return;
      }
      if (is("maximize")) {
        e.preventDefault();
        setMaximized((m) => !m);
        return;
      }
      // Move between split editor groups (Goto Next Splitter)
      if (is("split.other") && keyState.current.split) {
        e.preventDefault();
        // Next pane (previous with ⇧)
        const ls = leaves(useTabsStore.getState().panes);
        const g: Group = ls[(ls.indexOf(keyState.current.focused) + (e.shiftKey ? -1 : 1) + ls.length) % ls.length];
        const id = useTabsStore.getState().activeIds[g];
        const t = tabs.find((x) => x.id === id);
        if (t) activate(t);
        return;
      }
      // Ignore F4 / Enter pressed in an input field
      if (inInput) return;
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // keymapVer: menus opened from keys (⇧⌘F10 …) show the current shortcut labels
  }, [conns, keymapVer]);
}
