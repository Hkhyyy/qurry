import { expect, menuItem, openApp, test, treeNode } from "./app";
import type { Page } from "@playwright/test";

// Mock: the "roles" report is denied, "table_sizes" takes 2 s to load
test("switching reports drops the previous report's error while the new one loads", async ({ page }) => {
  await openApp(page);
  await treeNode(page, "[DEV] shop_app").click({ button: "right" });
  await menuItem(page, "Tools").hover();
  await menuItem(page, "Users & Privileges").click();
  const error = page.locator(".dba .ui-notice", { hasText: "permission denied" });
  await expect(error).toBeVisible();
  // The toolbar names the denied report, not the list's first one
  await expect(page.locator(".dba-report")).toHaveValue("roles");
  await expect(page.locator(".dba-help")).toHaveText(/Login roles/);

  await page.locator(".dba-report").selectOption("table_sizes");
  await expect(page.locator(".dba-bar").getByRole("button", { name: "Loading…" })).toBeVisible();
  // Counted once inside the loading window: an auto-retrying assertion would also pass once the load ends
  expect(await error.count(), "the denied report's error stays on the next report while it loads: reset error and loadedAt with the result in DbaReport's report-switch effect").toBe(0);
  await expect(page.locator(".dba-bar").getByRole("button", { name: "Refresh" })).toBeVisible();
  await expect(page.locator(".dba .ui-notice")).toHaveCount(0);
});

const openReport = async (page: Page, node: string, report: string) => {
  await treeNode(page, node).click({ button: "right" });
  await menuItem(page, "Tools").hover();
  await menuItem(page, report).click();
};
const targets = (page: Page) => page.evaluate(() => ((window as any).__mockDbaTargets ?? []) as string[]);

// Mock: Table Sizes of the default database (shop_app) has 182 MB orders, the other databases' 2 MB
test("the report runs on the picked database: the source's default from the data source, the node's from a database", async ({ page }) => {
  await openApp(page);
  await openReport(page, "[DEV] shop_app", "Table Sizes & Indexes");
  const db = page.locator(".dba-db");
  await expect(db).toHaveValue("shop_app");
  await expect(page.locator(".dba-grid")).toContainText("182 MB");
  // The explorer's databases (the mock's postgres, shop_app, shop_app_test)
  await db.focus();
  await expect(db.locator("option")).toHaveText(["shop_app", "postgres", "shop_app_test"]);
  await db.selectOption("shop_app_test");
  await expect(page.locator(".dba-grid")).toContainText("2 MB");
  await expect(page.locator(".dba-grid")).not.toContainText("182 MB");
  expect((await targets(page)).at(-1)).toBe("dev::shop_app_test");

  await openReport(page, "shop_app_test", "Slow Queries");
  await expect(page.locator(".dba-report")).toHaveValue("slow_queries");
  await expect(db).toHaveValue("shop_app_test");
});

test("auto-refresh keeps the picked database", async ({ page }) => {
  await openApp(page);
  await openReport(page, "[DEV] shop_app", "Sessions & Locks");
  const db = page.locator(".dba-db");
  await expect(db).toHaveValue("shop_app");
  await db.focus();
  await db.selectOption("shop_app_test");
  await expect.poll(async () => (await targets(page)).at(-1)).toBe("dev::shop_app_test");
  const n = (await targets(page)).length;
  // Two ticks of the 3 s auto-refresh
  await expect.poll(async () => (await targets(page)).length, { timeout: 10_000 }).toBeGreaterThanOrEqual(n + 2);
  expect((await targets(page)).slice(n - 1)).toEqual(Array((await targets(page)).length - n + 1).fill("dev::shop_app_test"));
  await expect(db).toHaveValue("shop_app_test");
  await expect(page.locator(".dba-auto input")).toBeChecked();
});
