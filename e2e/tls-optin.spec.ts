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
