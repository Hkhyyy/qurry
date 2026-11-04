import { create } from "zustand";
import type { ConnectionView, UpdateInfo } from "../../lib/api";
import type { NewKind } from "../../components/ObjectEditors";
import { setter } from "../setter";

/** Which dialogs are open, and what each was opened for (null / false = closed) */
interface DialogsState {
  settingsOpen: false | "appearance" | "keymap";
  /** Quick Switch Theme */
  schemeOpen: boolean;
  /** Keymap Reference */
  keymapOpen: boolean;
  /** New version (checked at startup + every hour) and its window */
  update: UpdateInfo | null;
  updateOpen: boolean;
  /** Data Source dialog: new, or edit */
  dialog: { edit?: ConnectionView } | null;
  /** Query History / Snippets of this console */
  historyFor: string | null;
  historyTab: "history" | "snippets";
  /** Production operation log window (connId null = all Data Sources) */
  prodLog: { connId: string | null } | null;
  exportReq: { target: string; sql: string; name: string; table: [string, string] | null } | null;
  importReq: { target: string; schema: string; table?: string; refresh?: () => void } | null;
  /** Generate test data window */
  genReq: { target: string; schema: string; table: string; conn: ConnectionView } | null;
  /** Create / modify objects (New ▸ · Modify Table…) */
  objEdit: { target: string; conn: ConnectionView; db?: string; schema: string; table?: string; newKind?: NewKind; refresh?: () => void } | null;
  /** Copy a subset of production data window */
  copyReq: { target: string; schema: string; table: string; conn: ConnectionView } | null;
  backupReq: { c: ConnectionView; db?: string } | null;
  quickDoc: { title: string; text: string } | null;
}

export const useDialogsStore = create<DialogsState>()(() => ({
  settingsOpen: false,
  schemeOpen: false,
  keymapOpen: false,
  update: null,
  updateOpen: false,
  dialog: null,
  historyFor: null,
  historyTab: "history",
  prodLog: null,
  exportReq: null,
  importReq: null,
  genReq: null,
  objEdit: null,
  copyReq: null,
  backupReq: null,
  quickDoc: null,
}));
export const setSettingsOpen = setter(useDialogsStore, "settingsOpen");
export const setSchemeOpen = setter(useDialogsStore, "schemeOpen");
export const setKeymapOpen = setter(useDialogsStore, "keymapOpen");
export const setUpdate = setter(useDialogsStore, "update");
export const setUpdateOpen = setter(useDialogsStore, "updateOpen");
export const setDialog = setter(useDialogsStore, "dialog");
export const setHistoryFor = setter(useDialogsStore, "historyFor");
export const setHistoryTab = setter(useDialogsStore, "historyTab");
export const setProdLog = setter(useDialogsStore, "prodLog");
export const setExportReq = setter(useDialogsStore, "exportReq");
export const setImportReq = setter(useDialogsStore, "importReq");
export const setGenReq = setter(useDialogsStore, "genReq");
export const setObjEdit = setter(useDialogsStore, "objEdit");
export const setCopyReq = setter(useDialogsStore, "copyReq");
export const setBackupReq = setter(useDialogsStore, "backupReq");
export const setQuickDoc = setter(useDialogsStore, "quickDoc");
