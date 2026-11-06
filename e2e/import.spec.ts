import type { Page } from "@playwright/test";
import { dialog, expect, menuItem, openApp, status, test } from "./app";

// Import Data Sources (#63) on the mock backend: the locations it finds, the native picker (window.__mockImportPick) and the
// preview with a checkbox per added row
test.beforeEach(async ({ page }) => openApp(page));

const DG = "DataGrip 2025.2 · global  (7)";

async function openSubmenu(page: Page) {
  await page.locator(".ws-explorer.on").getByTitle("More").click();
  await menuItem(page, "Import Data Sources").hover();
}

test("the submenu lists the locations found, then File… and Folder…", async ({ page }) => {
  await openSubmenu(page);
  const sub = page.locator(".ui-menu.sub");
  await expect(sub).toMatchAriaSnapshot(`
    - menu:
      - /children: equal
      - text: Found on this computer
      - menuitem "${DG}"
      - menuitem "IntelliJ IDEA · project shop-api  (3)"
      - menuitem "DBeaver · workspace6 General  (6)"
      - separator
      - menuitem "File…"
      - menuitem "Folder…"
  `);
  // The path is the tooltip
  await expect(menuItem(page, DG)).toHaveAttribute("title", "~/Library/Application Support/JetBrains/DataGrip2025.2/options");
});

test("nothing found: the submenu says so and keeps File… and Folder…", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockImportNone = true));
  // The locations are read again when the ⋮ menu opens
  await openSubmenu(page);
  await expect(page.locator(".ui-menu.sub")).toContainText("No DataGrip, IntelliJ or DBeaver settings found");
  await expect(menuItem(page, "File…")).toBeVisible();
  await expect(page.locator(".ui-menu.sub").getByRole("menuitem")).toHaveCount(2);
});

test("the preview: a checkbox per added row, Import N counts them and is disabled at 0; only the checked rows are imported", async ({ page }) => {
  await openSubmenu(page);
  await menuItem(page, DG).click();
  const d = dialog(page);
  await expect(d).toContainText("Import these data sources from DataGrip 2025.2 · global?");
  await expect(d).toContainText("5 to add, 2 skipped. New data sources start with MCP Off.");
  await expect(d.locator(".ui-detail-row")).toHaveCount(7);
  const boxes = d.getByRole("checkbox");
  // Skipped rows have none
  await expect(boxes).toHaveCount(5);
  await expect(d.locator(".ui-detail-row", { hasText: "warehouse" }).getByRole("checkbox")).toHaveCount(0);
  await expect(d.locator(".ui-detail-row", { hasText: "warehouse" })).toContainText("Snowflake isn't supported");
  await expect(d.locator(".ui-detail-row", { hasText: "analytics" }).locator(".tone-warn")).toHaveText("SSL without certificate checks");
  const ok = d.getByRole("button", { name: /^Import \d+$/ });
  await expect(ok).toHaveText("Import 5");
  await d.locator(".ui-detail-row", { hasText: "events" }).getByRole("checkbox").uncheck();
  await expect(ok).toHaveText("Import 4");
  for (const b of await boxes.all()) await b.uncheck();
  await expect(ok).toHaveText("Import 0");
  await expect(ok).toBeDisabled();
  await d.locator(".ui-detail-row", { hasText: "orders-prod" }).getByRole("checkbox").check();
  await d.locator(".ui-detail-row", { hasText: "local-pg" }).getByRole("checkbox").check();
  await expect(ok).toHaveText("Import 2");
  await ok.click();
  await expect(d).toHaveCount(0);
  await expect(status(page)).toHaveText("Imported 2 data sources — passwords are asked on the first connect");
  expect(await page.evaluate(() => (window as any).__mockImported)).toEqual([true, false, false, true, false, false, false]);
});

test("Cancel and Escape import nothing", async ({ page }) => {
  for (const close of ["Cancel", "Escape"]) {
    await openSubmenu(page);
    await menuItem(page, DG).click();
    await expect(dialog(page).getByRole("button", { name: "Import 5" })).toBeVisible();
    if (close === "Cancel") await dialog(page).getByRole("button", { name: "Cancel" }).click();
    else await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
  }
  expect(await page.evaluate(() => (window as any).__mockImported)).toBeUndefined();
});

test("File…: the picked file's preview; a picker closed imports nothing; a file that can't be read says where", async ({ page }) => {
  await openSubmenu(page);
  await menuItem(page, "File…").click();
  await expect(dialog(page)).toContainText("Import these data sources from DBeaver · General?");
  await dialog(page).getByRole("button", { name: "Cancel" }).click();

  await page.evaluate(() => ((window as any).__mockImportPick = null));
  await openSubmenu(page);
  await menuItem(page, "Folder…").click();
  await expect(dialog(page)).toHaveCount(0);

  await page.evaluate(() => ((window as any).__mockImportPick = "broken"));
  await openSubmenu(page);
  await menuItem(page, "File…").click();
  const d = dialog(page);
  await expect(d).toContainText("line 42, column 7: the element <data-source> isn't closed");
  await expect(d).toContainText("Nothing was imported.");
  // A notice: OK only
  await expect(d.getByRole("button")).toHaveText(["OK"]);
  await d.getByRole("button", { name: "OK" }).click();
  await expect(d).toHaveCount(0);
});

test("all skipped: a notice with the rows instead of the preview", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockImportAllSkipped = true));
  await openSubmenu(page);
  await menuItem(page, DG).click();
  const d = dialog(page);
  await expect(d).toContainText("Nothing to import from DataGrip 2025.2 · global: its 2 data sources are skipped.");
  await expect(d.getByRole("checkbox")).toHaveCount(0);
  await expect(d.getByRole("button")).toHaveText(["OK"]);
});

test("Find Action lists the same locations", async ({ page }) => {
  await page.keyboard.press("Meta+Shift+A");
  await page.keyboard.type("Import Data Sources");
  const list = page.locator(".search-everywhere");
  await expect(list).toContainText("Import Data Sources: File…");
  await expect(list).toContainText("Import Data Sources: Folder…");
  await expect(list).toContainText("Import Data Sources: DataGrip 2025.2 · global (7)");
  await expect(list).toContainText("Import Data Sources: DBeaver · workspace6 General (6)");
});
