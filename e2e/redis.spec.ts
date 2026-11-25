import type { Page } from "@playwright/test";
import { dialog, expect, openApp, test, treeItem, work } from "./app";

const tree = (page: Page) => page.locator(".ws-explorer.on");
const redisTabs = (page: Page) => page.getByRole("tab", { name: /^\[Redis\] cache/ });
const picker = (page: Page) => work(page).getByRole("combobox", { name: "Database" });
const term = (page: Page) => work(page).getByRole("textbox", { name: "Redis command" });
/** [Redis] cache with window.__mockRedis (readOnly · production · managed) */
async function openRedis(page: Page, mock?: Record<string, boolean>) {
  if (mock) await page.addInitScript((m) => ((window as any).__mockRedis = m), mock);
  await openApp(page);
  await treeItem(page, "db0 7").dblclick();
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db0\]/]);
}
async function openTerminal(page: Page, mock?: Record<string, boolean>) {
  await openRedis(page, mock);
  await work(page).getByRole("tab", { name: "Terminal" }).click();
  await expect(term(page)).toBeFocused();
}
async function run(page: Page, line: string) {
  await term(page).fill(line);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Enter");
}
const ran = (page: Page) => page.evaluate(() => ((window as any).__mockRedisLines ?? []) as string[]);

test("the explorer lists a Redis source's databases with key counts, no keys; one tab per source, switched from the explorer and its picker", async ({ page }) => {
  await openApp(page);
  // Databases only: leaves with their key count, no keys folder under them
  await expect(tree(page).getByRole("treeitem", { name: /^db\d/ })).toHaveText(["db07", "db32"]);
  await expect(treeItem(page, "db0 7")).not.toHaveAttribute("aria-expanded");
  await expect(tree(page).getByRole("treeitem", { name: /^keys/ })).toHaveCount(0);
  // A click with no Redis tab open opens nothing; a double-click opens the source's tab
  await treeItem(page, "db3 2").click();
  await expect(redisTabs(page)).toHaveCount(0);
  await treeItem(page, "db0 7").dblclick();
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db0\]/]);
  await expect(picker(page)).toHaveValue("db0");
  await expect(picker(page).locator("option")).toHaveText(["db0 · 7 keys", "db3 · 2 keys"]);
  await expect(work(page).getByRole("textbox", { name: "Filter keys" })).toHaveAttribute("placeholder", "Filter keys (pattern, e.g. prod:user:*)");
  // A click on another database switches the open tab instead of opening another
  await treeItem(page, "db3 2").click();
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db3\]/]);
  await expect(picker(page)).toHaveValue("db3");
  await expect(work(page).getByRole("treeitem")).toHaveText(["jobs2"]);
  // The workspace file keeps the picked database
  await expect
    .poll(async () => JSON.parse((await page.evaluate(() => localStorage.getItem("mock-ws"))) ?? "{}").workspaces?.[0]?.tabs?.filter((t: { kind: string }) => t.kind === "redis").map((t: { database: string }) => t.database))
    .toEqual(["db3"]);
  // The tab's picker switches it back
  await picker(page).selectOption("db0");
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db0\]/]);
  await expect(work(page).getByRole("treeitem").first()).toHaveText("queue1");
});

test("an old workspace with a Redis tab per database reopens one tab on the database saved last", async ({ page }) => {
  const tab = (database: string) => ({ kind: "redis", title: `[Redis] cache [${database}]`, connId: "rd", database, group: 0, sql: "", where: "", orderBy: "" });
  const file = { version: 2, shared: true, current: "w1", workspaces: [{ id: "w1", name: "Default", connIds: ["rd", "dev"], tabs: [tab("db0"), tab("db3")], panes: { t: "leaf", g: 0 }, activeIds: [0, -1, -1, -1], focused: 0 }] };
  await page.addInitScript((f) => localStorage.setItem("mock-ws", f), JSON.stringify(file));
  await openApp(page);
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db3\]/]);
  await expect(picker(page)).toHaveValue("db3");
});

test("a source whose Database is 3 opens its tab on db3: picker, keys and the terminal prompt", async ({ page }) => {
  await page.addInitScript(() => ((window as any).__mockRedisDb = "3"));
  await openApp(page);
  await treeItem(page, "db3 2").dblclick();
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db3\]/]);
  await expect(picker(page)).toHaveValue("db3");
  await expect(work(page).getByRole("treeitem")).toHaveText(["jobs2"]);
  await work(page).getByRole("tab", { name: "Terminal" }).click();
  await expect(work(page).locator(".rd-term-in")).toContainText("db3>");
});

// Restored on load, before the shared module is ready, so the tab keeps "db3"; the dropped-default path is in workspaces/layout.test.ts
test("a workspace's Redis tab on the source's own database (3) reopens on db3", async ({ page }) => {
  await page.addInitScript(() => ((window as any).__mockRedisDb = "3"));
  const tab = { kind: "redis", title: "[Redis] cache [db3]", connId: "rd", database: "db3", group: 0, sql: "", where: "", orderBy: "" };
  const file = { version: 2, shared: true, current: "w1", workspaces: [{ id: "w1", name: "Default", connIds: ["rd", "dev"], tabs: [tab], panes: { t: "leaf", g: 0 }, activeIds: [0, -1, -1, -1], focused: 0 }] };
  await page.addInitScript((f) => localStorage.setItem("mock-ws", f), JSON.stringify(file));
  await openApp(page);
  await expect(redisTabs(page)).toHaveText([/^\[Redis\] cache \[db3\]/]);
  await expect(picker(page)).toHaveValue("db3");
  await expect(work(page).getByRole("treeitem")).toHaveText(["jobs2"]);
});

test("key tree: folder counts beside the name, the TTL column has key TTLs only (— without expiry); the filter is a SCAN pattern", async ({ page }) => {
  await openRedis(page);
  const keys = work(page).getByRole("treeitem");
  await expect(keys).toHaveText(["queue1", "session1", "user2", "STREAMevents—", "ZSETrank1d 4h", "SETtags—"]);
  // The count is beside the name (muted), not in the TTL column
  const user = work(page).getByRole("treeitem", { name: "user 2" });
  await expect(user.locator(".node-sub")).toHaveText("2");
  await expect(user.locator(".node-trailing")).toHaveCount(0);
  await expect(work(page).getByRole("treeitem", { name: /events/ }).locator(".rd-ttl.none")).toHaveText("—");
  await user.click();
  await expect(keys).toHaveText(["queue1", "session1", "user2", "11", "HASHuser:1—", "STREAMevents—", "ZSETrank1d 4h", "SETtags—"]);
  await work(page).getByRole("treeitem").filter({ hasText: /^HASHuser:1—$/ }).click();
  await expect(work(page)).toContainText("a@b.c");
  const filter = work(page).getByRole("textbox", { name: "Filter keys" });
  await filter.fill("user:*");
  await filter.press("Enter");
  await expect(keys).toHaveText(["user2", "11", "HASHuser:1—"]);
});

test("Config is greyed out where the server refuses CONFIG (managed Redis), with a tooltip saying why and where the settings are", async ({ page }) => {
  await openRedis(page, { managed: true });
  const config = work(page).getByRole("tab", { name: "Config" });
  await expect(config).toHaveAttribute("aria-disabled", "true");
  await expect(config).toHaveAttribute("title", /doesn't allow CONFIG.*ElastiCache.*provider's console.*Info/);
  await config.click({ force: true });
  await expect(config).toHaveAttribute("aria-selected", "false");
  await expect(work(page).getByRole("tab", { name: "Content" })).toHaveAttribute("aria-selected", "true");
});

test("Config is a tab like the others where CONFIG works", async ({ page }) => {
  await openRedis(page);
  const config = work(page).getByRole("tab", { name: "Config" });
  await expect(config).not.toHaveAttribute("aria-disabled");
  await config.click();
  await expect(config).toHaveAttribute("aria-selected", "true");
});

test("Terminal: command completion with argument hints, key names from SCAN, HELP, ↑↓ history, replies by type", async ({ page }) => {
  await openTerminal(page);
  const choices = work(page).getByRole("listbox", { name: "Completions" }).getByRole("option");
  await term(page).pressSequentially("HG");
  await expect(choices.locator("b")).toHaveText(["HGET", "HGETALL"]);
  await expect(choices.first()).toContainText("key field");
  await expect(work(page).locator(".rd-term-ghost")).toHaveText("HG key field");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Tab");
  await expect(term(page)).toHaveValue("HGETALL ");
  // The key argument completes from a bounded SCAN: ⇥ picks; ↩ picks only after ↑↓ moved to a key
  await term(page).pressSequentially("user:");
  await expect(choices).toHaveText(["user:1", "user:1:name"]);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(term(page)).toHaveValue("HGETALL user:1:name ");
  await term(page).fill("HGETALL ");
  await term(page).pressSequentially("user:");
  await expect(choices).toHaveText(["user:1", "user:1:name"]);
  await page.keyboard.press("Tab");
  await expect(term(page)).toHaveValue("HGETALL user:1 ");
  // A key typed out in full is listed first, and ↩ runs the line (not the longer user:1:name)
  await term(page).fill("HGETALL ");
  await term(page).pressSequentially("user:1");
  await expect(choices).toHaveText(["user:1", "user:1:name"]);
  await page.keyboard.press("Enter");
  await expect.poll(() => ran(page)).toEqual(["HGETALL user:1"]);
  const out = work(page).locator(".rd-term-out");
  await expect(out.locator("table").last().locator("tr")).toHaveText(["fieldvalue", "age30", "emaila@b.c"]);
  await run(page, "LRANGE queue:mail 0 -1");
  await expect(out.locator("table").last().locator("tr")).toHaveText(["indexvalue", "0mail-1", "1mail-2", "2mail-3"]);
  await run(page, "ZRANGE rank 0 -1 WITHSCORES");
  await expect(out.locator("table").last().locator("tr")).toHaveText(["memberscore", "alice10", "bob20"]);
  await run(page, "SMEMBERS tags");
  await expect(out.locator("table").last().locator("tr")).toHaveText(["member", "beta", "vip"]);
  await run(page, "GET missing");
  await expect(out.locator("pre.nil").last()).toHaveText("(nil)");
  await run(page, "NOSUCH x");
  await expect(out.locator("pre.err").last()).toHaveText(/^\(error\) ERR unknown command 'NOSUCH'/);
  await run(page, "HELP hget");
  await expect(out.locator("pre.note").last()).toHaveText("HGET key field — Returns the value of a field in a hash. (read)");
  // ↑↓: this data source's history, newest first
  await page.keyboard.press("ArrowUp");
  await expect(term(page)).toHaveValue("HELP hget");
  await page.keyboard.press("ArrowUp");
  await expect(term(page)).toHaveValue("NOSUCH x");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(term(page)).toHaveValue("");
});

test("Terminal on a read-only source refuses a write without sending it", async ({ page }) => {
  await openTerminal(page, { readOnly: true });
  await run(page, "SET greeting hi");
  await expect(work(page).locator(".rd-term-out pre.err").last()).toHaveText(/is read-only: SET changes data/);
  await expect(dialog(page)).toHaveCount(0);
  await run(page, "FLUSHDB");
  await expect(work(page).locator(".rd-term-out pre.err").last()).toHaveText(/is read-only: FLUSHDB changes data/);
  await expect(dialog(page)).toHaveCount(0);
  await run(page, "GET greeting");
  await expect(work(page).locator(".rd-term-out pre.nil")).toHaveCount(1);
  expect(await ran(page)).toEqual(["GET greeting"]);
});

test("Terminal on Production asks before a write, not before a read", async ({ page }) => {
  await openTerminal(page, { production: true });
  await run(page, "GET greeting");
  await expect(work(page).locator(".rd-term-out pre.nil")).toHaveCount(1);
  await expect(dialog(page)).toHaveCount(0);
  await run(page, "SET greeting hi");
  await expect(dialog(page)).toContainText("Production: [Redis] cache");
  await expect(dialog(page)).toContainText("This command changes data. Run it?");
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  expect(await ran(page)).toEqual(["GET greeting"]);
  await run(page, "SET greeting hi");
  await dialog(page).getByRole("button", { name: "Run", exact: true }).click();
  await expect.poll(() => ran(page)).toEqual(["GET greeting", "SET greeting hi"]);
});

test("Terminal: FLUSHDB and DEL always ask, also off Production", async ({ page }) => {
  await openTerminal(page);
  await run(page, "FLUSHDB");
  await expect(dialog(page)).toContainText("FLUSHDB deletes or replaces data. Run it anyway?");
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await run(page, "DEL tags");
  await expect(dialog(page)).toContainText("DEL deletes or replaces data");
  await dialog(page).getByRole("button", { name: "Run Anyway" }).click();
  await expect.poll(() => ran(page)).toEqual(["DEL tags"]);
  await expect(term(page)).toBeFocused();
});

// #220: the Redis browser's loads give a database tab's feedback. The mock holds a call with window.__mockRedisSlow, fails it with
// __mockRedisFail and makes the source reconnect with __mockReconnect (src/lib/mock.ts); set after the explorer read its key counts
const mock = (page: Page, g: Record<string, unknown>) => page.evaluate((g) => Object.assign(window, g), g);
const keyList = (page: Page) => work(page).locator(".rd-keys");
const value = (page: Page) => work(page).locator(".rd-tab-body");
const status = (page: Page) => page.locator(".statusbar");
const toast = (page: Page) => page.locator(".ui-toast");
const REFUSED = "Redis: Connection refused (os error 61)";
async function openWith(page: Page, g: Record<string, unknown>) {
  await openApp(page);
  await mock(page, g);
  await treeItem(page, "db0 7").dblclick();
}

test("the key list and a key's value say Loading… with ■; ■ and ⌘F2 stop them, Retry loads them", async ({ page }) => {
  await openWith(page, { __mockRedisSlow: { scan: true } });
  await expect(keyList(page).getByRole("status")).toHaveText(/^Loading…/);
  // Past a second, how long; the toolbar's ⟳ is ■ meanwhile, as the grid's
  await expect(keyList(page).getByRole("status")).toHaveText(/^Loading… \d+\.\d s$/);
  await expect(work(page).locator(".rd-toolbar").getByTitle(/^Cancel/)).toBeVisible();
  await keyList(page).getByRole("status").getByTitle(/^Cancel/).click();
  await expect(keyList(page)).toContainText("Cancelled");
  await expect(status(page)).toContainText("Cancelled");
  await expect(work(page).locator(".rd-toolbar").getByTitle("Scan again (⌘r)")).toBeVisible();
  await mock(page, { __mockRedisSlow: { key: true } });
  await keyList(page).getByRole("button", { name: "Retry" }).click();
  await work(page).getByRole("treeitem", { name: /rank/ }).click();
  await expect(value(page).getByRole("status")).toHaveText(/^Loading…/);
  await page.keyboard.press("Meta+F2");
  await expect(value(page)).toContainText("Cancelled");
  await mock(page, { __mockRedisSlow: {} });
  await value(page).getByRole("button", { name: "Retry" }).click();
  await expect(value(page)).toContainText("alice");
});

test("a failed scan, key counts or value says why with Retry, and the status bar says it too", async ({ page }) => {
  await openWith(page, { __mockRedisFail: { scan: REFUSED, overview: REFUSED } });
  await expect(keyList(page).locator(".rd-load-error")).toContainText(REFUSED);
  await expect(status(page)).toContainText(REFUSED);
  // One failure said once: the key counts line waits for the keys
  await expect(work(page).getByText(/^Couldn't read the key counts/)).toHaveCount(0);
  await mock(page, { __mockRedisFail: { overview: REFUSED } });
  await keyList(page).getByRole("button", { name: "Retry" }).click();
  await expect(work(page).getByRole("treeitem", { name: /rank/ })).toBeVisible();
  const counts = work(page).locator(".rd-load-error", { hasText: `Couldn't read the key counts: ${REFUSED}` });
  await expect(counts).toBeVisible();
  await expect(picker(page)).toHaveValue("db0");
  await mock(page, { __mockRedisFail: { key: "Redis: WRONGTYPE Operation against a key holding the wrong kind of value" } });
  await counts.getByRole("button", { name: "Retry" }).click();
  await expect(counts).toHaveCount(0);
  await expect(picker(page).locator("option")).toHaveText(["db0 · 7 keys", "db3 · 2 keys"]);
  await work(page).getByRole("treeitem", { name: /rank/ }).click();
  await expect(value(page).locator(".rd-load-error")).toContainText("WRONGTYPE");
  await expect(status(page)).toContainText("WRONGTYPE");
  await mock(page, { __mockRedisFail: {} });
  await value(page).getByRole("button", { name: "Retry" }).click();
  await expect(value(page)).toContainText("alice");
});

test("a reconnect shows the toast and the status bar for the scan, an edit and the Terminal; Cancel ends them, a failed one keeps Retry", async ({ page }) => {
  const waiting = /^Reconnecting \[Redis\] cache through SSH… \d+\.\d s/;
  await openWith(page, { __mockReconnect: { id: "rd" } });
  await expect(keyList(page).getByRole("status")).toHaveText(/^Loading…/);
  await expect(toast(page)).toContainText(waiting);
  await expect(status(page).locator(".running-bar")).toContainText(waiting);
  await toast(page).getByRole("button", { name: "Cancel" }).click();
  await expect(toast(page)).toHaveText("Cancelled while reconnecting [Redis] cache through SSH.×");
  await expect(keyList(page).getByRole("status")).toHaveCount(0);
  await expect(keyList(page).getByRole("button", { name: "Retry" })).toBeVisible();

  // An edit waits the same way
  await mock(page, { __mockReconnect: null });
  await keyList(page).getByRole("button", { name: "Retry" }).click();
  await work(page).getByRole("treeitem", { name: /rank/ }).click();
  await expect(value(page)).toContainText("alice");
  await mock(page, { __mockReconnect: { id: "rd" } });
  await work(page).locator(".rd-ttl-edit input").fill("60");
  await work(page).locator(".rd-ttl-edit input").press("Enter");
  await expect(toast(page)).toContainText(waiting);
  await toast(page).getByRole("button", { name: "Cancel" }).click();
  await expect(status(page)).toContainText("Cancelled while reconnecting [Redis] cache through SSH.");

  // The Terminal: Running… while it waits; the reconnect fails with its reason and Retry, which runs the line again
  const reason = "SSH connect timeout: bastion.example.com:22 didn't answer within 10s. Check the SSH server, then run it again.";
  await mock(page, { __mockReconnect: { id: "rd", fail: reason, after: 1800 } });
  await work(page).getByRole("tab", { name: "Terminal" }).click();
  await run(page, "HGETALL user:1");
  await expect(work(page).locator(".rd-term-out").getByRole("status")).toHaveText(/^Running…/);
  await expect(toast(page)).toContainText(waiting);
  await expect(toast(page)).toContainText("Couldn't reconnect [Redis] cache through SSH");
  await expect(toast(page)).toContainText(reason);
  await expect(work(page).locator(".rd-term-out pre.err")).toHaveText(`(error) Couldn't reconnect [Redis] cache through SSH: ${reason}`);
  await expect(work(page).locator(".rd-term-out").getByRole("status")).toHaveCount(0);
  await mock(page, { __mockReconnect: null });
  await toast(page).getByRole("button", { name: "Retry" }).click();
  await expect(toast(page)).toHaveCount(0);
  await expect(work(page).locator(".rd-term-table")).toContainText("a@b.c");
  expect(await ran(page)).toEqual(["HGETALL user:1"]);
});
