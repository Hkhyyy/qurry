import { expect, openApp, test, treeItem } from "./app";
import type { Page } from "@playwright/test";

// #135: a table tab opens at once and loads inside it; while its source reconnects, the status bar's running indicator and a toast
// (bottom right) say so with the time, Cancel stops the load, and a failure keeps the reason with Retry. The mock's
// window.__mockReconnect makes [DEV] shop_app reconnect (src/lib/mock.ts)
test.beforeEach(async ({ page }) => openApp(page));

const reconnect = (page: Page, r: { fail?: string; after?: number } | null) => page.evaluate((r) => ((window as any).__mockReconnect = r && { id: "dev", ...r }), r);
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
