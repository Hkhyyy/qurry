import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionView } from "../../lib/api";
import { leaf, type Pane } from "../../lib/panes";
import { stash } from "../workspaces/store";
import { buildTab, closeTabs, ddlObject, moveTo, patch, settle, shownStale, togglePin, updTab } from "./ops";
import { liveSql, setTabs, useTabsStore } from "./store";
import { emptyLayout, type Group } from "./types";

// Monaco needs a browser; closing a tab only forgets its model there
vi.mock("../../lib/monaco", () => ({ forgetConsoleModel: () => {} }));
const closed: string[] = [];
vi.mock("../../lib/api", async (orig) => ({ ...(await orig<typeof import("../../lib/api")>()), api: { closeSession: async (id: string) => void closed.push(id) } }));

const conn = { id: "dev", name: "[DEV] shop_app", kind: "postgres", database: "shop_app" } as ConnectionView;
const tab = (title: string, group: Group = 0) => buildTab(conn, "console", { sql: `-- ${title}` }, group, title);
const split = (a: Group, b: Group): Pane => ({ t: "split", dir: "right", a: leaf(a), b: leaf(b) });
const titles = () => useTabsStore.getState().tabs.map((t) => `${t.title}@${t.group}`);
const shown = () => useTabsStore.getState().activeIds.map((id) => useTabsStore.getState().tabs.find((t) => t.id === id)?.title ?? null);

beforeEach(() => {
  useTabsStore.setState({ tabs: [], activeIds: [null, null, null, null], panes: leaf(0), focused: 0, mru: [], recent: [], closedStack: [] });
  stash.clear();
  liveSql.clear();
  closed.length = 0;
});

describe("tab operations", () => {
  it("patch and updTab change the tab in the current workspace, or where it waits in another one", () => {
    const [a, b] = [tab("a"), tab("b")];
    setTabs([a]);
    stash.set("ws2", { ...emptyLayout(), tabs: [b] });
    patch(a.id, { running: true });
    updTab(b.id, (t) => ({ ...t, inTx: true }));
    expect(useTabsStore.getState().tabs[0].running).toBe(true);
    expect(useTabsStore.getState().tabs).toHaveLength(1);
    expect(stash.get("ws2")!.tabs[0].inTx).toBe(true);
  });

  it("a table's DDL tab gets its own view when built (the last choice), so another tab's switch doesn't change it", () => {
    const store = new Map<string, string>([["qurry.ddlView", "structure"]]);
    vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
    const edit = { schema: "public", table: "users", pk: [] };
    const restored = buildTab(conn, "ddl", { sql: "CREATE TABLE users ();", edit }, 0, "users [DDL]");
    store.set("qurry.ddlView", "sql");
    expect(restored.ddlView).toBe("structure");
    expect(buildTab(conn, "ddl", { sql: "CREATE TABLE users ();", edit }, 0, "users [DDL]").ddlView).toBe("sql");
    // A view's DDL or a console has no Structure view
    expect(buildTab(conn, "ddl", { sql: "CREATE VIEW v AS SELECT 1;" }, 0, "v [DDL]").ddlView).toBeUndefined();
    expect(tab("c").ddlView).toBeUndefined();
    vi.unstubAllGlobals();
  });

  it("togglePin keeps pinned tabs first", () => {
    const ts = [tab("a"), tab("b"), tab("c")];
    setTabs(ts);
    togglePin(ts[2]);
    expect(titles()).toEqual(["c@0", "a@0", "b@0"]);
    togglePin(ts[1]);
    expect(titles()).toEqual(["c@0", "b@0", "a@0"]);
    // Unpinned: after the pinned ones
    togglePin(ts[2]);
    expect(titles()).toEqual(["b@0", "c@0", "a@0"]);
  });
});

describe("pane operations", () => {
  it("moveTo shows the tab in the other pane and focuses it", () => {
    const ts = [tab("a"), tab("b"), tab("c", 1)];
    useTabsStore.setState({ tabs: ts, panes: split(0, 1), activeIds: [ts[0].id, ts[2].id, null, null] });
    moveTo(ts[1].id, 1);
    expect(titles()).toEqual(["a@0", "b@1", "c@1"]);
    expect(shown()).toEqual(["a", "b", null, null]);
    expect(useTabsStore.getState().focused).toBe(1);
  });

  it("settle drops an empty pane and moves focus to one that is left", () => {
    const ts = [tab("a"), tab("b", 1)];
    useTabsStore.setState({ tabs: ts, panes: split(0, 1), activeIds: [ts[0].id, ts[1].id, null, null], focused: 1 });
    const rest = [ts[0]];
    setTabs(rest);
    settle(rest, useTabsStore.getState().panes);
    expect(useTabsStore.getState().panes).toEqual(leaf(0));
    expect(shown()).toEqual(["a", null, null, null]);
    expect(useTabsStore.getState().focused).toBe(0);
  });

  it("settle shows the most recently used tab of a pane whose tab went away", () => {
    const ts = [tab("a"), tab("b"), tab("c")];
    useTabsStore.setState({ tabs: ts, activeIds: [ts[2].id, null, null, null], mru: [ts[2].id, ts[0].id, ts[1].id] });
    const rest = [ts[0], ts[1]];
    setTabs(rest);
    settle(rest, leaf(0));
    expect(shown()).toEqual(["a", null, null, null]);
  });

  it("closeTabs closes the sessions, keeps them for Reopen Closed Tab and unsplits an emptied pane", async () => {
    const ts = [tab("a"), tab("b", 1)];
    useTabsStore.setState({ tabs: ts, panes: split(0, 1), activeIds: [ts[0].id, ts[1].id, null, null], focused: 1, mru: [ts[1].id, ts[0].id] });
    liveSql.set(ts[1].id, "select 2");
    await closeTabs([ts[1].id]);
    expect(titles()).toEqual(["a@0"]);
    expect(useTabsStore.getState().panes).toEqual(leaf(0));
    expect(useTabsStore.getState().focused).toBe(0);
    expect(useTabsStore.getState().mru).toEqual([ts[0].id]);
    expect(useTabsStore.getState().closedStack.map((s) => [s.title, s.sql])).toEqual([["b", "select 2"]]);
    expect(liveSql.has(ts[1].id)).toBe(false);
    expect(closed).toEqual([ts[1].id]);
  });

  it("shownStale: a restored tab revealed by closing or moving is due to load, hidden ones and loaded ones are not (#187)", async () => {
    const [a, b, c, d, e] = [tab("a"), tab("b"), tab("c"), tab("d", 1), tab("e", 1)].map((t) => ({ ...t, stale: true }));
    const loaded = (t: typeof a) => ({ ...t, stale: false });
    useTabsStore.setState({ tabs: [a, b, loaded(c), d, loaded(e)], panes: split(0, 1), activeIds: [a.id, e.id, null, null], mru: [a.id, b.id, c.id, e.id] });
    const due = () => shownStale(useTabsStore.getState()).map((t) => t.title);
    // b and d are stale but hidden
    expect(due()).toEqual(["a"]);
    patch(a.id, { stale: false });
    await closeTabs([a.id]);
    expect(shown()).toEqual(["b", "e", null, null]);
    expect(due()).toEqual(["b"]);
    patch(b.id, { stale: false });
    await closeTabs([b.id]);
    expect(shown()).toEqual(["c", "e", null, null]);
    expect(due()).toEqual([]);
    moveTo(d.id, 0);
    expect(shown()).toEqual(["d", "e", null, null]);
    expect(due()).toEqual(["d"]);
  });

  it("a console starts in the Transaction mode the core says (ConnectionView startsManual / productionStartsManual), and says why once when it is Production", () => {
    const view = (id: string, startsManual: boolean, productionStartsManual: boolean) => ({ ...conn, id, options: { confirmChanges: true }, startsManual, productionStartsManual }) as ConnectionView;
    const tabOf = (c: ConnectionView) => buildTab(c, "console", {}, 0, "c");
    expect(tabOf(view("v-prod", true, true)).txManual).toBe(true);
    expect(tabOf(view("v-prod", true, true)).output).toEqual([]);
    expect(tabOf(view("v-own", true, false))).toMatchObject({ txManual: true, output: [] });
    // Production where the core says Auto (Redis, a driver plugin, "start consoles in Manual" off): no copy of the rule overrides it
    expect(tabOf({ ...view("v-redis", false, false), kind: "redis" } as ConnectionView)).toMatchObject({ txManual: false, output: [] });
    const told = tabOf(view("v-told", true, true)).output;
    expect(told).toHaveLength(1);
    expect(told[0].text).toContain("Production");
    expect(buildTab(view("v-told", true, true), "table", {}, 0, "t").txManual).toBe(false);
  });

  it("a view without the fields (a mock or a test) starts a Production console in Manual, never in Auto", () => {
    const old = (id: string, options = {}) => ({ ...conn, id, options }) as ConnectionView;
    expect(buildTab(old("o-prod", { confirmChanges: true }), "console", {}, 0, "c").txManual).toBe(true);
    expect(buildTab(old("o-prod2", { confirmChanges: true }), "console", {}, 0, "c").output).toHaveLength(1);
    expect(buildTab(old("o-dev"), "console", {}, 0, "c").txManual).toBe(false);
  });
});

describe("ddlObject", () => {
  it("is the saved object, or a table's from a tab saved before DDL tabs kept it; none for an old view tab", () => {
    expect(ddlObject(buildTab(conn, "ddl", { ddlOf: { schema: "public", name: "v" }, objKind: "view" }, 0, "v [DDL]"))).toEqual({ schema: "public", name: "v" });
    expect(ddlObject(buildTab(conn, "ddl", { edit: { schema: "public", table: "users", pk: [] } }, 0, "users [DDL]"))).toEqual({ schema: "public", name: "users" });
    expect(ddlObject(buildTab(conn, "ddl", { objKind: "view" }, 0, "v [DDL]"))).toBeUndefined();
  });
});
