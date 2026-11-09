import { expect, menuItem, openApp, openConsole, test, treeNode } from "./app";
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
  await page.locator(".dba-bar").getByRole("button", { name: "About Users & Roles" }).click();
  await expect(page.getByRole("note", { name: "About Users & Roles" })).toHaveText(/Login roles/);
  await page.keyboard.press("Escape");

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
  const db = page.locator(".work-layer:visible .dba-db");
  await expect(db).toHaveValue("shop_app");
  await expect(page.locator(".dba-grid")).toContainText("182 MB");
  // Every database of the server, the current one first (the mock's postgres, shop_app, shop_app_test)
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
  const db = page.locator(".work-layer:visible .dba-db");
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

// The account can't connect to the source's default database: the picker lists through the report's own database, and when that
// is the default one too it says how to get out (a default database the account can connect to)
test("the picker lists through the report's database, and says what to do when it can't list", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => ((window as any).__mockDbListFail = true));
  await openReport(page, "shop_app_test", "Slow Queries");
  const db = page.locator(".work-layer:visible .dba-db");
  await expect(db).toHaveValue("shop_app_test");
  await db.focus();
  await expect(db.locator("option")).toHaveText(["shop_app_test", "postgres", "shop_app"]);

  await openReport(page, "[DEV] shop_app", "Sessions & Locks");
  await expect(db).toHaveValue("shop_app");
  await db.focus();
  // The error isn't an option (a native select is as wide as its longest option): it shows above the report and as the tooltip
  const why = /^Couldn't list the databases: permission denied for database "shop_app"[\s\S]*Data Source Properties › General › Database/;
  await expect(db.locator("option")).toHaveText(["shop_app"]);
  await expect(page.locator(".work-layer:visible .dba-dbs-error")).toHaveText(why);
  await expect(db).toHaveAttribute("title", why);
});

// The account can't connect to one database: picking it says so in the report, and the list (read through the source's own
// target when the report's refuses) stays there to pick another one; the select keeps its width
test("picking a database the account can't connect to keeps the list and the select's width", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => ((window as any).__mockDbRefuse = "shop_app_test"));
  await openReport(page, "[DEV] shop_app", "Sessions & Locks");
  const db = page.locator(".work-layer:visible .dba-db");
  await expect(db.locator("option")).toHaveText(["shop_app", "postgres", "shop_app_test"]);
  await db.selectOption("shop_app_test");
  await expect(page.locator(".work-layer:visible .dba .ui-notice")).toContainText("Couldn't connect to database shop_app_test");
  await db.focus();
  await expect(db.locator("option")).toHaveText(["shop_app_test", "postgres", "shop_app"]);
  await expect(page.locator(".work-layer:visible .dba-dbs-error")).toHaveCount(0);
  expect((await db.boundingBox())!.width).toBeLessThanOrEqual(240);
  await db.selectOption("postgres");
  await expect(page.locator(".work-layer:visible .dba .ui-notice")).toHaveCount(0);
});

test("the database picker lists once the report is shown: its first click opens the whole list", async ({ page }) => {
  await openApp(page);
  // Listing takes a while (as on a server): asked for only on the first click, the list would open with the current database alone
  await page.evaluate(() => ((window as any).__mockDbListDelay = 200));
  await openReport(page, "[DEV] shop_app", "Sessions & Locks");
  const db = page.locator(".work-layer:visible .dba-db");
  await expect(db).toHaveValue("shop_app");
  await expect(db.locator("option")).toHaveText(["shop_app", "postgres", "shop_app_test"]);
  await db.click();
  await expect(db.locator("option")).toHaveText(["shop_app", "postgres", "shop_app_test"]);
});

// Manage Shown Schemas only filters the explorer: the picker still lists the default database it hides, after picking another one too
test("the picker lists every database, also the ones Manage Shown Schemas hides", async ({ page }) => {
  await openApp(page);
  await page.locator(".ws-explorer.on .scope-badge").first().click();
  const popup = page.locator(".scope-popup");
  const box = (name: string | RegExp) => popup.getByRole("treeitem", { name }).getByRole("checkbox");
  await box("All databases").click();
  await box(/^shop_app_test/).click();
  await page.keyboard.press("Enter");
  await expect(treeNode(page, "[DEV] shop_app")).toContainText("1 of 3");

  await openReport(page, "[DEV] shop_app", "Table Sizes & Indexes");
  const db = page.locator(".work-layer:visible .dba-db");
  await expect(db).toHaveValue("shop_app");
  await expect(db.locator("option")).toHaveText(["shop_app", "postgres", "shop_app_test"]);
  await db.selectOption("shop_app_test");
  await db.focus();
  await expect(db.locator("option")).toHaveText(["shop_app_test", "postgres", "shop_app"]);
  await db.selectOption("shop_app");
  await expect(page.locator(".dba-grid")).toContainText("182 MB");
});

test("the console's and the report's database pickers list the same databases", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  const consoleDb = page.locator(".work-layer:visible .cb-sel").first();
  await consoleDb.focus();
  await expect(consoleDb.locator("option")).toHaveText(["shop_app", "postgres", "shop_app_test"]);
  const listed = await consoleDb.locator("option").allTextContents();

  await openReport(page, "[DEV] shop_app", "Table Sizes & Indexes");
  const db = page.locator(".work-layer:visible .dba-db");
  await db.focus();
  await expect(db.locator("option")).toHaveText(listed);
});
