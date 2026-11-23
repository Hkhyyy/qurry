import { create } from "zustand";
import type { ConnectionView, DbKind, UpdateInfo } from "../../lib/api";
import type { NewKind } from "../../components/ObjectEditors";
import type { SeTab } from "../../components/SearchEverywhere";
import type { SwitchItem } from "../../components/Switcher";
import type { SettingsPage } from "../settings/SettingsDialog";
import { setter } from "../setter";

/** The transient popups: Switcher (⌃⇥), Recent Files (⌘e), Quick Switch Theme (⌃`) and Search Everywhere with its Go to … and Find Action tabs */
export type Popup = { kind: "switcher"; items: SwitchItem[]; start: number } | { kind: "recent" } | { kind: "theme" } | { kind: "search"; tab: SeTab };

/** Which dialogs are open, and what each was opened for (null / false = closed) */
interface DialogsState {
  /** true: the last page used */
  settingsOpen: boolean | SettingsPage | "theme";
  /** The one transient popup open (DataGrip: opening one closes the other) */
  popup: Popup | null;
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
  quickDoc: { title: string; text: string; /** The text is DDL in this kind's dialect (colored) */ kind?: DbKind } | null;
}

export const useDialogsStore = create<DialogsState>()(() => ({
  settingsOpen: false,
  popup: null,
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

/** Opens a transient popup in place of the one open (and Quick Documentation). Ignored while a modal dialog is up, as DataGrip's dialogs
 *  block the main window's actions: the popups stack under the dialogs, so it would open hidden */
export const openPopup = (popup: Popup) => {
  if (!globalThis.document?.querySelector(".ui-modal[aria-modal='true']")) useDialogsStore.setState({ popup, quickDoc: null });
};
/** Closes the popup of this kind, if it is still the one open (an action picked in one may have opened another) */
export const closePopup = (kind: Popup["kind"]) => useDialogsStore.setState((s) => (s.popup?.kind === kind ? { popup: null } : {}));
