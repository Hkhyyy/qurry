import { dialog, editor, expand, expect, openApp, runAll, status, test, treeNode, typeSql, work } from "./app";
import type { Page } from "@playwright/test";

// "LAN dev" (mock) is a remote PostgreSQL without TLS: every connect fails with NEEDS_TLS until allow_plaintext turns on
// "Allow unencrypted connection" for it. window.__mockLanProd (set before the app loads) makes it a Production source
const ALLOW = "Allow unencrypted for this data source";
const DETAIL = "The password will be sent unencrypted to 10.0.0.12. Use only on a network you trust.";
const calls = (page: Page) => page.evaluate(() => ({ ...(window as any).__mockTls }));
const lanTree = (page: Page) => page.locator(".ws-explorer.on .conn-tree", { has: page.getByRole("treeitem", { name: /^LAN dev/ }) });

test("tree: the dev source offers the button; one click allows it once and the tree loads", async ({ page }) => {
  await openApp(page);
  await expand(page, "LAN dev");
  const tree = lanTree(page);
  await expect(tree.locator(".tree-status.err")).toContainText("didn't set up an encrypted (TLS) connection");
  await expect(tree.locator(".ui-tls-optin-detail")).toHaveText(DETAIL);
  await tree.getByRole("button", { name: ALLOW }).click();
  await expect(tree.getByRole("treeitem", { name: /^lan_app/ })).toBeVisible();
  await expect(tree.getByRole("button", { name: ALLOW })).toHaveCount(0);
  expect((await calls(page)).allow).toBe(1);
});

test("tree: a Production source has no button and says where the setting is", async ({ page }) => {
  await page.addInitScript(() => ((window as any).__mockLanProd = true));
  await openApp(page);
  await expand(page, "LAN dev");
  const tree = lanTree(page);
  await expect(tree.locator(".tree-status.err")).toContainText("didn't set up an encrypted (TLS) connection");
  await expect(tree.locator(".ui-tls-optin")).toContainText("Production data source");
  await expect(tree.locator(".ui-tls-optin")).toContainText("Options ▸ Allow unencrypted connection");
  await expect(page.getByRole("button", { name: ALLOW })).toHaveCount(0);
  expect((await calls(page)).allow).toBe(0);
});

test("console: the button reconnects and says Connected. Run again. without running the statement again", async ({ page }) => {
  await openApp(page);
  await treeNode(page, "LAN dev").click();
  await page.keyboard.press("Meta+Shift+L");
  await editor(page).click();
  await typeSql(page, "delete from users where id = 1");
  await runAll(page);
  const out = work(page).locator(".output-log");
  await expect(out.locator(".ol.error").last()).toContainText("didn't set up an encrypted (TLS) connection");
  expect((await calls(page)).runs).toBe(1);
  await out.getByRole("button", { name: ALLOW }).click();
  await expect(out.locator(".ol").last()).toContainText("Connected. Run again.");
  await expect(status(page)).toHaveText("Connected. Run again.");
  await expect(out.getByRole("button", { name: ALLOW })).toHaveCount(0);
  expect(await calls(page)).toMatchObject({ allow: 1, runs: 1 });
});

test("dialog: a failed Test Connection ticks the form's Allow unencrypted checkbox and tests again, without saving", async ({ page }) => {
  await openApp(page);
  await treeNode(page, "LAN dev").click();
  await page.keyboard.press("Meta+;");
  await expect(dialog(page)).toBeVisible();
  await dialog(page).getByRole("button", { name: "Test Connection" }).click();
  await expect(dialog(page).locator(".ui-notice")).toContainText("didn't set up an encrypted (TLS) connection");
  await expect(dialog(page).locator(".ui-tls-optin-detail")).toHaveText(DETAIL);
  await dialog(page).getByRole("button", { name: ALLOW }).click();
  await expect(dialog(page).locator(".ui-notice")).toContainText("✓ PostgreSQL");
  await dialog(page).getByRole("tab", { name: "Options" }).click();
  await expect(dialog(page).getByRole("checkbox", { name: /Allow unencrypted connection/ })).toBeChecked();
  expect(await calls(page)).toMatchObject({ allow: 0, saves: 0 });
});

test("tree: keyboard only — Tab to the button, Enter allows it, and the keys stay in the tree", async ({ page, browserName }) => {
  await openApp(page);
  await expand(page, "LAN dev");
  const tree = lanTree(page);
  await expect(tree.getByRole("button", { name: ALLOW })).toBeVisible();
  // WebKit (Safari, the macOS WebView) moves to buttons with ⌥⇥ unless “Press Tab to highlight each item” is on
  await page.keyboard.press(browserName === "webkit" ? "Alt+Tab" : "Tab");
  await expect(tree.getByRole("button", { name: ALLOW })).toBeFocused();
  // Letters on the button aren't the tree's speed search
  await page.keyboard.press("x");
  await expect(page.locator(".speed-search")).toHaveCount(0);
  await page.keyboard.press("Enter");
  await expect(tree.getByRole("treeitem", { name: /^lan_app/ })).toBeVisible();
  expect((await calls(page)).allow).toBe(1);
  await expect(page.locator(".ws-explorer.on .tree")).toBeFocused();
  // ↓ from the data source row selects its database
  await page.keyboard.press("ArrowDown");
  await expect(tree.getByRole("treeitem", { name: /^lan_app/ })).toHaveAttribute("aria-selected", "true");
});

test("console then tree: allowing it in the console reloads the tree's stale error and keeps the keys in the editor", async ({ page }) => {
  await openApp(page);
  await expand(page, "LAN dev");
  const tree = lanTree(page);
  await expect(tree.getByRole("button", { name: ALLOW })).toBeVisible();
  await page.keyboard.press("Meta+Shift+L");
  await editor(page).click();
  await typeSql(page, "select 1");
  await runAll(page);
  const out = work(page).locator(".output-log");
  await out.getByRole("button", { name: ALLOW }).click();
  await expect(out.locator(".ol").last()).toContainText("Connected. Run again.");
  // ⌘↩ runs again right away
  await expect(editor(page).locator("textarea")).toBeFocused();
  await expect(tree.getByRole("treeitem", { name: /^lan_app/ })).toBeVisible();
  await expect(page.getByRole("button", { name: ALLOW })).toHaveCount(0);
  expect((await calls(page)).allow).toBe(1);
});

test("tree then console: after the tree allowed it, the console's old error has no button", async ({ page }) => {
  await openApp(page);
  await treeNode(page, "LAN dev").click();
  await page.keyboard.press("Meta+Shift+L");
  await editor(page).click();
  await typeSql(page, "select 1");
  await runAll(page);
  const out = work(page).locator(".output-log");
  await expect(out.getByRole("button", { name: ALLOW })).toBeVisible();
  await expand(page, "LAN dev");
  await lanTree(page).getByRole("button", { name: ALLOW }).click();
  await expect(lanTree(page).getByRole("treeitem", { name: /^lan_app/ })).toBeVisible();
  await expect(out.getByRole("button", { name: ALLOW })).toHaveCount(0);
  expect((await calls(page)).allow).toBe(1);
});

test("dialog: for Production it points at this dialog's Options tab, not at ⌘;", async ({ page }) => {
  await page.addInitScript(() => ((window as any).__mockLanProd = true));
  await openApp(page);
  await treeNode(page, "LAN dev").click();
  await page.keyboard.press("Meta+;");
  await dialog(page).getByRole("button", { name: "Test Connection" }).click();
  const box = dialog(page).locator(".ui-tls-optin");
  await expect(box).toContainText("turn on Options ▸ Allow unencrypted connection.");
  await expect(box).not.toContainText("⌘;");
});

test("dialog: after Allow the focus stays in the dialog and Escape still closes it", async ({ page }) => {
  // A test that takes as long as a real connect: focus moves once Test Connection is enabled again, not when the promise ends
  await page.addInitScript(() => ((window as any).__mockTestDelay = 300));
  await openApp(page);
  await treeNode(page, "LAN dev").click();
  await page.keyboard.press("Meta+;");
  await dialog(page).getByRole("button", { name: "Test Connection" }).click();
  await dialog(page).getByRole("button", { name: ALLOW }).click();
  await expect(dialog(page).locator(".ui-notice")).toContainText("✓ PostgreSQL");
  await expect(dialog(page).getByRole("button", { name: "Test Connection" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("table tab: Enter on the button reloads the table and the keys stay in its grid", async ({ page }) => {
  const tab = { kind: "table", title: "users", connId: "lan", group: 0, sql: "", where: "", orderBy: "", edit: { schema: "public", table: "users" } };
  const ws = { version: 2, shared: true, current: "ws1", workspaces: [{ id: "ws1", name: "Default", connIds: ["lan"], tabs: [tab] }] };
  await page.addInitScript((w) => localStorage.setItem("mock-ws", w), JSON.stringify(ws));
  await openApp(page);
  const button = work(page).getByRole("button", { name: ALLOW });
  await expect(button).toBeVisible();
  await button.focus();
  await page.keyboard.press("Enter");
  await expect(work(page).locator(".grid-overlay")).toHaveCount(0);
  expect((await calls(page)).allow).toBe(1);
  await expect(work(page).locator(".grid-scroll")).toBeFocused();
});
