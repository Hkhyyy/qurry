import { expect, it, vi } from "vitest";
import type { ConnectionView } from "../../lib/api";
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
