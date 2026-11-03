import { create } from "zustand";
import type { WorkspaceInfo } from "../../components/WorkspaceBar";
import { setter } from "../setter";
import type { WsLayout } from "../tabs/types";

// ───────── Workspaces (like Orca): each has its own tabs and split view. The current one is in the tabs store, the rest are in stash (tab objects, console sessions, transactions as is) ─────────
interface WorkspacesState {
  workspaces: WorkspaceInfo[];
  curWs: string;
}

export const useWorkspacesStore = create<WorkspacesState>()(() => ({ workspaces: [{ id: "ws1", name: "Default", connIds: [] }], curWs: "ws1" }));
export const setWorkspaces = setter(useWorkspacesStore, "workspaces");
export const setCurWs = setter(useWorkspacesStore, "curWs");

/** Layouts of the workspaces that aren't shown, by id (not React state: changing one doesn't redraw) */
export const stash = new Map<string, WsLayout>();
