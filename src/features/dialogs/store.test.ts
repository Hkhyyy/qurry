import { beforeEach, expect, it } from "vitest";
import type { ConnectionView } from "../../lib/api";
import { closePopup, openPopup, setDialog, setExportReq, setHistoryFor, setHistoryTab, setQuickDoc, setSettingsOpen, setUpdateOpen, useDialogsStore } from "./store";

const initial = useDialogsStore.getState();
beforeEach(() => useDialogsStore.setState(initial, true));

it("starts with every dialog closed", () => {
  const s = useDialogsStore.getState();
  expect(Object.entries(s).filter(([, v]) => v !== null && v !== false && v !== "history")).toEqual([]);
});

it("opens and closes one dialog without touching the others", () => {
  const edit = { id: "dev", name: "[DEV] shop_app" } as ConnectionView;
  setDialog({ edit });
  setHistoryTab("snippets");
  setHistoryFor("tab-1");
  expect(useDialogsStore.getState()).toMatchObject({ dialog: { edit }, historyFor: "tab-1", historyTab: "snippets", settingsOpen: false, exportReq: null });
  setDialog(null);
  expect(useDialogsStore.getState()).toMatchObject({ dialog: null, historyFor: "tab-1" });
});

it("setters take a value or a function of the current one, and apply right away", () => {
  setSettingsOpen("keymap");
  expect(useDialogsStore.getState().settingsOpen).toBe("keymap");
  setUpdateOpen((o) => !o);
  setUpdateOpen((o) => !o);
  setUpdateOpen((o) => !o);
  expect(useDialogsStore.getState().updateOpen).toBe(true);
  const req = { target: "dev", sql: "SELECT 1", name: "result", table: null };
  setExportReq(req);
  expect(useDialogsStore.getState().exportReq).toBe(req);
});

it("a transient popup replaces the one open, and closing one that was replaced leaves the new one", () => {
  setQuickDoc({ title: "orders", text: "CREATE TABLE orders ()" });
  openPopup({ kind: "theme" });
  expect(useDialogsStore.getState()).toMatchObject({ popup: { kind: "theme" }, quickDoc: null });
  openPopup({ kind: "search", tab: "all" });
  expect(useDialogsStore.getState().popup).toEqual({ kind: "search", tab: "all" });
  // Find Action closes itself after running its action, which may have opened another popup
  openPopup({ kind: "recent" });
  closePopup("search");
  expect(useDialogsStore.getState().popup).toEqual({ kind: "recent" });
  closePopup("recent");
  expect(useDialogsStore.getState().popup).toBeNull();
});
