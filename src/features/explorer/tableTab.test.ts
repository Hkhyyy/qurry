import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionView, TableDetail } from "../../lib/api";
import { buildTab } from "../tabs/ops";
import { setTabs, useTabsStore } from "../tabs/store";

let detail: { resolve: (d: TableDetail) => void; reject: (e: unknown) => void; callId: string };
const cancels: string[] = [];
vi.mock("../../lib/monaco", () => ({ forgetConsoleModel: () => {} }));
vi.mock("../../lib/statusbar", () => ({ setTableColumns: () => {} }));
vi.mock("../../lib/api", async (orig) => ({
  ...(await orig<typeof import("../../lib/api")>()),
  api: { reconnecting: async () => [], cancelCall: async (id: string) => void cancels.push(id) },
  meta: { table: (_t: string, _s: string, _n: string, callId: string) => new Promise<TableDetail>((resolve, reject) => (detail = { resolve, reject, callId })) },
}));
const { cancelTableLoad, loadTableTab } = await import("./tableTab");

const conn = { id: "dev", name: "[DEV] shop_app", kind: "postgres", database: "shop_app" } as ConnectionView;
const tabNow = (id: string) => useTabsStore.getState().tabs.find((t) => t.id === id)!;
const cols = (pk: string): TableDetail => ({ columns: [{ name: pk, dataType: "bigint", nullable: false, defaultValue: null, comment: null, isPk: true }], indexes: [], constraints: [] });

beforeEach(() => setTabs([]));

describe("a table tab opened from the explorer (#135)", () => {
  it("loads inside the tab: Loading… before the metadata is in, then the primary key and the rows", async () => {
    const tab = buildTab(conn, "table", { edit: { schema: "public", table: "users", pk: [] } }, 0, "users");
    setTabs([tab]);
    const run = vi.fn(async () => {});
    const done = loadTableTab(tab, "dev", "SELECT * FROM users;", run, () => {});
    expect(tabNow(tab.id).running).toBe(true);
    expect(run).not.toHaveBeenCalled();
    detail.resolve(cols("id"));
    await done;
    expect(tabNow(tab.id).edit?.pk).toEqual(["id"]);
    expect(run).toHaveBeenCalledWith(tab, "SELECT * FROM users;");
  });

  it("a metadata error is said in the status bar (after the rows, read-only), never swallowed", async () => {
    const tab = buildTab(conn, "table", { edit: { schema: "public", table: "users", pk: [] } }, 0, "users");
    setTabs([tab]);
    const status: string[] = [];
    const run = vi.fn(async () => void status.push("10 rows retrieved"));
    const done = loadTableTab(tab, "dev", "SELECT 1;", run, (s) => status.push(s));
    detail.reject("permission denied for schema pg_catalog");
    await done;
    expect(run).toHaveBeenCalledOnce();
    expect(status).toEqual(["10 rows retrieved", "Read-only: couldn't read the columns of public.users: permission denied for schema pg_catalog"]);
    expect(tabNow(tab.id).edit?.pk).toEqual([]);
  });

  it("Cancel while it waits for the metadata stops that load, and the rows never load", async () => {
    const tab = buildTab(conn, "table", { edit: { schema: "public", table: "users", pk: [] } }, 0, "users");
    setTabs([tab]);
    const status: string[] = [];
    const run = vi.fn(async () => {});
    const done = loadTableTab(tab, "dev", "SELECT 1;", run, (s) => status.push(s));
    expect(cancelTableLoad(tab.id)).toBe(true);
    expect(cancels).toEqual([detail.callId]);
    detail.reject("Cancelled");
    await done;
    expect(run).not.toHaveBeenCalled();
    expect(tabNow(tab.id).running).toBe(false);
    expect(status).toEqual(["Cancelled"]);
    // Its rows' run is cancelled the usual way (cancel_query)
    expect(cancelTableLoad(tab.id)).toBe(false);
  });

  it("a tab closed before the metadata came runs nothing", async () => {
    const tab = buildTab(conn, "table", { edit: { schema: "public", table: "users", pk: [] } }, 0, "users");
    setTabs([tab]);
    const run = vi.fn(async () => {});
    const done = loadTableTab(tab, "dev", "SELECT 1;", run, () => {});
    setTabs([]);
    detail.resolve(cols("id"));
    await done;
    expect(run).not.toHaveBeenCalled();
  });
});
