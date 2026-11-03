import type { Page } from "@playwright/test";
import { dialog, expect, openApp, test, treeItem } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

/** Modify Table (⌘F6) on public.users */
async function openDesigner(page: Page) {
  await treeItem(page, "users").first().click();
  await page.keyboard.press("Meta+F6");
  await expect(dialog(page).locator(".ui-modal-title")).toContainText("Modify Table");
}
const typeField = (page: Page, column: string) => dialog(page).getByRole("combobox", { name: `Type of ${column}` });
/** The type suggestions (the ui kit's list popup) */
const popup = (page: Page) => dialog(page).locator(".ui-popup-list");

test("Modify Table › column type: the suggestions open right under the field, ↓ ↩ picks one", async ({ page }) => {
  await openDesigner(page);
  // The app's own popup, not the native one WKWebView misplaces
  await expect(page.locator("datalist")).toHaveCount(0);

  const field = typeField(page, "status");
  await expect(field).toHaveValue("text");
  await field.click();
  await expect(popup(page)).toBeVisible();
  // Opened with all of them, the current type highlighted
  await expect(popup(page).getByRole("option", { selected: true })).toHaveText("text");
  const f = (await field.boundingBox())!;
  const p = (await popup(page).boundingBox())!;
  expect(Math.abs(p.x - f.x)).toBeLessThanOrEqual(3);
  expect(Math.abs(p.y - (f.y + f.height))).toBeLessThanOrEqual(4);
  expect(p.width).toBeGreaterThanOrEqual(f.width - 1);

  // Emptied: all of them, the first highlighted; ↓ ↩ takes the second
  await field.fill("");
  await expect(popup(page).getByRole("option", { selected: true })).toHaveText("integer");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(field).toHaveValue("bigint");
  await expect(popup(page)).toHaveCount(0);
  await expect(field).toBeFocused();
  await expect(dialog(page)).toBeVisible();
});

test("Modify Table › column type: typing filters (prefix first), a click picks, free text is kept, ⎋ closes only the list", async ({ page }) => {
  await openDesigner(page);
  const field = typeField(page, "org_id");

  await field.fill("int");
  await expect(popup(page).getByRole("option")).toHaveText(["integer", "interval", "bigint", "smallint"]);
  await field.fill("JSO");
  await popup(page).getByRole("option", { name: "jsonb", exact: true }).click();
  await expect(field).toHaveValue("jsonb");
  await expect(popup(page)).toHaveCount(0);
  await expect(field).toBeFocused();

  // A type that isn't suggested: no popup, kept as typed (Tab moves on without changing it)
  await field.fill("varchar(100)");
  await expect(popup(page)).toHaveCount(0);
  await page.keyboard.press("Tab");
  await expect(field).toHaveValue("varchar(100)");

  // ⎋ closes the list, the dialog stays; the next ⎋ closes the dialog
  await field.click();
  await expect(popup(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(popup(page)).toHaveCount(0);
  await expect(dialog(page)).toBeVisible();
  await expect(field).toHaveValue("varchar(100)");
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("Modify Table › column type: the list closes when the columns scroll, and follows the light theme", async ({ page }) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await openDesigner(page);
  const field = typeField(page, "id");
  await field.click();
  await expect(popup(page)).toBeVisible();
  const bg = await popup(page).evaluate((el) => getComputedStyle(el).backgroundColor);
  expect(bg).toBe(await page.evaluate(() => getComputedStyle(document.querySelector(".ui-modal")!).backgroundColor));
  await dialog(page).locator(".designer-body").evaluate((el) => el.dispatchEvent(new Event("scroll")));
  await expect(popup(page)).toHaveCount(0);
});
