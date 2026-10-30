import { expect, menuItem, openApp, test, treeNode } from "./app";

// Mock: the "roles" report is denied, "table_sizes" takes 2 s to load
test("switching reports drops the previous report's error while the new one loads", async ({ page }) => {
  await openApp(page);
  await treeNode(page, "[DEV] shop_app").click({ button: "right" });
  await menuItem(page, "Tools").hover();
  await menuItem(page, "Users & Privileges").click();
  const error = page.locator(".dba .ui-notice", { hasText: "permission denied" });
  await expect(error).toBeVisible();

  await page.locator(".dba-bar select").selectOption("table_sizes");
  await expect(page.locator(".dba-bar").getByRole("button", { name: "Loading…" })).toBeVisible();
  // Counted once inside the loading window: an auto-retrying assertion would also pass once the load ends
  expect(await error.count(), "the denied report's error stays on the next report while it loads: reset error and loadedAt with the result in DbaReport's report-switch effect").toBe(0);
  await expect(page.locator(".dba-bar").getByRole("button", { name: "Refresh" })).toBeVisible();
  await expect(page.locator(".dba .ui-notice")).toHaveCount(0);
});
