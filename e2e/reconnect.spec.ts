import { expect, openApp, test, treeItem } from "./app";
import type { Page } from "@playwright/test";

// #135: a table tab opens at once and loads inside it; while its source reconnects, the status bar's running indicator and a toast
// (bottom right) say so with the time, Cancel stops the load, and a failure keeps the reason with Retry. The mock's
// window.__mockReconnect makes [DEV] shop_app reconnect (src/lib/mock.ts)
test.beforeEach(async ({ page }) => openApp(page));

const reconnect = (page: Page, r: { fail?: string; own?: boolean; after?: number; listFor?: number } | null) => page.evaluate((r) => ((window as any).__mockReconnect = r && { id: "dev", ...r }), r);
const toast = (page: Page) => page.locator(".ui-toast");
const running = (page: Page) => page.locator(".statusbar .running-bar");
const tableBody = (page: Page) => page.locator(".work-layer:visible .table-body");
const WAITING = /^Reconnecting \[DEV\] shop_app through SSH… \d+\.\d s/;

test("a table tab opens at once with Loading…; a reconnect shows in the status bar and the toast, and Cancel ends the load", async ({ page }) => {
  await reconnect(page, {});
  await treeItem(page, "users").first().dblclick();
  await expect(page.getByRole("tab", { name: /users/ })).toBeVisible();
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toBeVisible();
  await expect(toast(page)).toHaveText(new RegExp(WAITING.source + "Cancel$"));
  await expect(running(page)).toContainText(WAITING);
  await expect(toast(page)).toHaveAttribute("role", "status");

  // Bottom right, above the status bar; the same size while the time ticks
  const [t, bar, vw] = [await toast(page).boundingBox(), await page.locator(".statusbar").boundingBox(), page.viewportSize()!.width];
  expect(Math.round(vw - (t!.x + t!.width))).toBe(12);
  expect(Math.round(bar!.y - (t!.y + t!.height))).toBe(10);
  expect(t!.width).toBe(380);
  await page.waitForTimeout(1200);
  expect(await toast(page).boundingBox()).toEqual(t);

  await toast(page).getByRole("button", { name: "Cancel" }).click();
  await expect(toast(page)).toHaveText("Cancelled while reconnecting [DEV] shop_app through SSH.×");
  await expect(running(page)).toHaveCount(0);
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toHaveCount(0);
  await expect(page.locator(".statusbar")).toContainText("Cancelled while reconnecting [DEV] shop_app through SSH.");
  await toast(page).getByRole("button", { name: "Close" }).click();
  await expect(toast(page)).toHaveCount(0);
});

test("a failed reconnect keeps the reason and Retry; Retry loads the table", async ({ page }) => {
  const reason = "SSH connect timeout: bastion.example.com:22 didn't answer within 10s. Check the SSH server, then run it again.";
  await reconnect(page, { fail: reason, after: 1800 });
  await treeItem(page, "users").first().dblclick();
  await expect(toast(page)).toContainText(WAITING);
  await expect(toast(page)).toHaveAttribute("role", "alert");
  await expect(toast(page)).toContainText("Couldn't reconnect [DEV] shop_app through SSH");
  await expect(toast(page)).toContainText(reason);
  await expect(page.locator(".statusbar")).toContainText(`Couldn't reconnect [DEV] shop_app through SSH: ${reason}`);
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toHaveCount(0);

  await reconnect(page, null);
  await toast(page).getByRole("button", { name: "Retry" }).click();
  await expect(toast(page)).toHaveCount(0);
  await expect(page.locator(".statusbar")).toContainText(/rows retrieved/);
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toHaveCount(0);
});

test("an explorer list waiting on a reconnect shows only Loading… in the tree; Cancel ends it there too", async ({ page }) => {
  await reconnect(page, {});
  await treeItem(page, "billing").click();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".ws-explorer.on .tree-status")).toHaveText("Loading…");
  await expect(toast(page)).toContainText(WAITING);
  await toast(page).getByRole("button", { name: "Cancel" }).click();
  await expect(page.locator(".ws-explorer.on .tree-status")).toHaveText("Cancelled while reconnecting [DEV] shop_app through SSH.");
});

const stop = (page: Page) => page.locator(".work-layer:visible").getByTitle(/^Cancel/);
const reload = (page: Page) => page.locator(".work-layer:visible").getByTitle(/^Reload Page/);

test("the table tab's own Cancel before a reconnect shows ends its load, and its rows never load", async ({ page }) => {
  // Listed as reconnecting for only half a second, so the toast (shown after a second) never comes, however slow the browser is
  await reconnect(page, { listFor: 500 });
  await treeItem(page, "users").first().dblclick();
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toBeVisible();
  // Within the first second nothing says the source reconnects yet
  await stop(page).click();
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toHaveCount(0);
  await expect(page.locator(".statusbar")).toContainText("Cancelled");
  await reconnect(page, null);
  await page.waitForTimeout(1500);
  await expect(page.locator(".statusbar")).not.toContainText(/rows retrieved/);
  await expect(toast(page)).toHaveCount(0);
});

test("the table tab's own Cancel while the toast says its source reconnects cancels that reconnect", async ({ page }) => {
  await reconnect(page, {});
  await treeItem(page, "users").first().dblclick();
  await expect(toast(page)).toContainText(WAITING);
  await stop(page).click();
  await expect(toast(page)).toHaveText("Cancelled while reconnecting [DEV] shop_app through SSH.×");
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toHaveCount(0);
});

test("Reload of a table tab left without its metadata goes through the same load: the reconnect shows, and its Cancel ends it", async ({ page }) => {
  await reconnect(page, {});
  await treeItem(page, "users").first().dblclick();
  await expect(toast(page)).toContainText(WAITING);
  await toast(page).getByRole("button", { name: "Cancel" }).click();
  await toast(page).getByRole("button", { name: "Close" }).click();
  // No primary key yet: Reload loads the metadata again, waiting on the reconnect as the first load did
  await reload(page).click();
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toBeVisible();
  await expect(toast(page)).toContainText(WAITING);
  await stop(page).click();
  await expect(toast(page)).toHaveText("Cancelled while reconnecting [DEV] shop_app through SSH.×");
  await expect(tableBody(page).getByText("Loading…", { exact: true })).toHaveCount(0);
  // Once the source answers, Reload loads the primary key and the rows
  await reconnect(page, null);
  await reload(page).click();
  await expect(page.locator(".statusbar")).toContainText(/rows retrieved/);
  await expect(toast(page)).toHaveCount(0);
});

test("a load that fails for its own reason after its source reconnected says that reason, not that it couldn't reconnect", async ({ page }) => {
  await reconnect(page, { fail: "permission denied for schema public", own: true, after: 1800 });
  await treeItem(page, "users").first().dblclick();
  await expect(toast(page)).toContainText(WAITING);
  // The rows still load, read-only, and the status bar says why
  await expect(page.locator(".statusbar")).toContainText("Read-only: couldn't read the columns of public.users: permission denied for schema public");
  await expect(toast(page)).toHaveCount(0);
});

test("the waiting toast goes once the backend no longer lists the source", async ({ page }) => {
  // Listed for 3 s: the toast (shown after a second) has two seconds to be seen even on a slow browser, then goes
  await reconnect(page, { listFor: 3000 });
  await treeItem(page, "users").first().dblclick();
  await expect(toast(page)).toContainText(WAITING);
  await expect(running(page)).toContainText(WAITING);
  await expect(toast(page)).toHaveCount(0, { timeout: 8000 });
  await expect(running(page)).not.toContainText("Reconnecting");
});
