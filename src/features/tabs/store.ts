import { create } from "zustand";
import type { ConsoleApi } from "../../components/ConsolePane";
import { leaf, type Pane } from "../../lib/panes";
import { setter } from "../setter";
import type { Group, Tab, TabSnapshot } from "./types";

/** The current workspace's tabs and split view (other workspaces wait in the workspaces stash) */
interface TabsState {
  tabs: Tab[];
  /** Tab shown in each pane */
  activeIds: (string | null)[];
  /** Split view (up to 4 panes) */
  panes: Pane;
  focused: Group;
  /** Tab ids, most recently used first */
  mru: string[];
  /** Recent Files (⌘e) */
  recent: TabSnapshot[];
  /** Reopen Closed Tab (⇧⌘T) */
  closedStack: TabSnapshot[];
}

export const useTabsStore = create<TabsState>()(() => ({ tabs: [], activeIds: [null, null, null, null], panes: leaf(0), focused: 0, mru: [], recent: [], closedStack: [] }));
export const setTabs = setter(useTabsStore, "tabs");
export const setActiveIds = setter(useTabsStore, "activeIds");
export const setPanes = setter(useTabsStore, "panes");
export const setFocused = setter(useTabsStore, "focused");

// SQL typed in the console is kept here, not in state (setTabs) → the whole app does not redraw on every keystroke
export const liveSql = new Map<string, string>();
/** Each console's editor (run, insert, focus …), by tab id */
export const editorApis = new Map<string, ConsoleApi>();
