import type { Page } from "@playwright/test";
import { expect, openApp, test } from "./app";

// Search Everywhere's scope (#169): This Workspace (the default) searches the current workspace's explorer list, All Places every
// data source; MySQL objects are found as well as PostgreSQL ones; the pick is kept (settings.json, which the mock keeps in localStorage)
const ws = {
  version: 2,
  shared: true,
  current: "ws1",
  workspaces: [
    // [DEV] shop_app (PostgreSQL) and [MySQL] shop, both connected
    { id: "ws1", name: "Default", connIds: ["dev", "my", "local", "lan"], tabs: [], panes: { t: "leaf", g: 0 }, activeIds: [-1, -1, -1, -1], focused: 0 },
    // [PROD] shop_app (not connected: refunds from the search cache) and [Redis] cache
    { id: "ws2", name: "Reports", connIds: ["prod", "rd"], tabs: [], panes: { t: "leaf", g: 0 }, activeIds: [-1, -1, -1, -1], focused: 0 },
  ],
};
test.beforeEach(async ({ page }) => {
  await page.addInitScript((w) => {
    if (!sessionStorage.getItem("seeded")) {
      localStorage.setItem("mock-ws", w);
      sessionStorage.setItem("seeded", "1");
    }
  }, JSON.stringify(ws));
  await openApp(page);
});

const se = (page: Page) => page.locator(".search-everywhere");
const scope = (page: Page) => se(page).getByRole("combobox", { name: "Search scope" });
const result = (page: Page, name: string) => se(page).getByRole("option").filter({ has: page.locator(".ui-list-title", { hasText: new RegExp(`^${name}$`) }) });
async function goTo(page: Page, text: string) {
  await page.keyboard.press("Meta+KeyO");
  await expect(se(page)).toBeVisible();
  await se(page).locator("input").fill(text);
}

test("This Workspace is the default: the current workspace's sources only, MySQL tables too", async ({ page }) => {
  await goTo(page, "product_rev");
  await expect(scope(page)).toHaveValue("workspace");
  await expect(result(page, "product_reviews")).toContainText("[MySQL] shop · shop");
  // A PostgreSQL table of the same workspace
  await se(page).locator("input").fill("contracts");
  await expect(result(page, "contracts").first()).toContainText("[DEV] shop_app");
  // refunds is in [PROD] shop_app, which is in Reports
  await se(page).locator("input").fill("refunds");
  await expect(se(page)).toContainText("Nothing found in This Workspace");
  await scope(page).selectOption("all");
  await expect(result(page, "refunds")).toContainText("[PROD] shop_app · shop_app (cached)");
});

test("the pick is kept: reopening and restarting search All Places", async ({ page }) => {
  await goTo(page, "refunds");
  await scope(page).selectOption("all");
  await expect(result(page, "refunds")).toBeVisible();
  await page.keyboard.press("Escape");
  await goTo(page, "refunds");
  await expect(scope(page)).toHaveValue("all");
  await expect(result(page, "refunds")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.reload();
  await expect(page.locator(".tree-node", { hasText: "[DEV] shop_app" })).toBeVisible();
  await goTo(page, "refunds");
  await expect(scope(page)).toHaveValue("all");
  await expect(result(page, "refunds")).toBeVisible();
});

test("⌘o again moves to the next scope; in Reports, This Workspace finds its Redis keys, not the other workspace's tables", async ({ page }) => {
  await page.getByRole("button", { name: /Reports$/ }).click();
  await goTo(page, "session");
  await expect(result(page, "session:abc")).toContainText("[Redis] cache · db0");
  await se(page).locator("input").fill("product_rev");
  await expect(se(page)).toContainText("Nothing found in This Workspace");
  // Nothing is selected and no tab is open: no Context Data Sources, so the key goes between the two
  await page.keyboard.press("Meta+KeyO");
  await expect(scope(page)).toHaveValue("all");
  await expect(result(page, "product_reviews")).toBeVisible();
  await page.keyboard.press("Meta+KeyO");
  await expect(scope(page)).toHaveValue("workspace");
  // A key opens in the Redis tab
  await se(page).locator("input").fill("session:abc");
  await page.keyboard.press("Enter");
  await expect(se(page)).toHaveCount(0);
  await expect(page.getByRole("tab", { name: /\[Redis\] cache/ })).toBeVisible();
});
