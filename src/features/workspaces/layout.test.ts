import { expect, it, vi } from "vitest";
import { redisDb, type ConnectionView } from "../../lib/api";
import type { Tab } from "../tabs/types";
import FIXTURE from "../../../e2e/fixtures/workspace-gui.json?raw";
import { restoreWs, wsJson } from "./layout";

vi.mock("../../lib/monaco", () => ({ forgetConsoleModel: () => {} }));

const conn = (id: string, kind: string, database: string) => ({ id, name: id, kind, database }) as ConnectionView;
const CONNS = [conn("dev", "postgres", "shop_app"), conn("prod", "postgres", "shop_app"), conn("my", "mysql", "shop"), conn("rd", "redis", "0"), conn("local", "postgres", "postgres"), conn("lan", "postgres", "lan_app")];

// FIXTURE: the file the app saves today (also restored by e2e/workspace.spec.ts): two workspaces, same-name tables, a console with SQL, a right split
it("workspace-gui.json: restore, then save, gives the same file", () => {
  const stale: string[] = [];
  expect(restoreWs(JSON.parse(FIXTURE), CONNS, (t) => stale.push(t.title))).toBe(true);
  // Only the tab shown in each pane of the current workspace loads
  expect(stale).toEqual(["users", "users"]);
  expect(JSON.stringify(JSON.parse(wsJson()), null, 2)).toBe(FIXTURE.trimEnd());
});

// Before 0.3.2 a Redis tab was per database: a file with two of one source reopens one tab, on the database saved last, and the
// active index that pointed at the dropped entry shows the kept tab
it("old per-database Redis tabs come back as one tab per data source, on the last saved database", () => {
  const tab = (database: string, title: string) => ({ kind: "redis", title, connId: "rd", database, group: 0, sql: "", where: "", orderBy: "" });
  const file = {
    version: 2,
    shared: true,
    current: "w1",
    workspaces: [{ id: "w1", name: "Default", connIds: ["rd"], tabs: [tab("db0", "rd [db0]"), tab("db3", "rd [db3]")], panes: { t: "leaf", g: 0 }, activeIds: [1, -1, -1, -1], focused: 0 }],
  };
  const stale: string[] = [];
  restoreWs(file, CONNS, (t) => stale.push(`${t.title} ${t.target}`));
  expect(stale).toEqual(["rd [db3] rd::db3"]);
  const saved = JSON.parse(wsJson()).workspaces[0];
  expect(saved.tabs.map((t: { database: string }) => t.database)).toEqual(["db3"]);
  expect(saved.activeIds[0]).toBe(0);
});

// A saved Redis tab on the source's own database (Database "3", tab "db3") drops it as the default once the shared module is loaded
// (e.g. a workspace switch after startup); the Redis browser then shows redisDb(c.database), so it stays on db3, not db0
it("a Redis tab on the source's own database restores on the default target and still shows that database", () => {
  const rd = conn("rd", "redis", "3");
  const tab = { kind: "redis", title: "rd [db3]", connId: "rd", database: "db3", group: 0, sql: "", where: "", orderBy: "" };
  const file = { version: 2, shared: true, current: "w1", workspaces: [{ id: "w1", name: "Default", connIds: ["rd"], tabs: [tab], panes: { t: "leaf", g: 0 }, activeIds: [0, -1, -1, -1], focused: 0 }] };
  const restored: Tab[] = [];
  restoreWs(file, [rd], (t) => restored.push(t));
  expect(restored.map((t) => [t.database, t.target])).toEqual([[undefined, "rd"]]);
  // useEditorGroups renders <RedisBrowser database={t.database ?? redisDb(c?.database)}>
  expect(restored[0].database ?? redisDb(rd.database)).toBe("db3");
});
