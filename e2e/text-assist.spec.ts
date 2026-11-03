import type { Locator } from "@playwright/test";
import { dialog, expect, openApp, test, treeItem, treeNode, work } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

/** Comment fields may spell check (prose), never autocorrect */
const prose = (scope: Locator) => scope.locator("input[spellcheck=true], textarea[spellcheck=true]").evaluateAll((els) => els.map((el) => el.getAttribute("autocorrect")));

/** Every text field in `scope` has autocorrect and spell check off (spell check may be on with `allowProse`: comments) */
async function check(scope: Locator, allowProse = false) {
  expect(await scope.locator("input, textarea").count()).toBeGreaterThan(0);
  const bad = await scope.locator("input:not([type=checkbox]):not([type=radio]):not([type=color]), textarea").evaluateAll((els, ok) => els.filter((el) => el.getAttribute("autocorrect") !== "off" || (el.getAttribute("spellcheck") !== "false" && !ok)).map((el) => el.outerHTML.slice(0, 120)), allowProse);
  expect(bad).toEqual([]);
  for (const a of await prose(scope)) expect(a).toBe("off");
}

test("no autocorrect or spell check in the data source form", async ({ page }) => {
  await treeNode(page, "[PROD] shop_app").click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Properties/ }).click();
  for (const tab of ["General", "SSH/SSL"]) {
    const t = dialog(page).getByRole("tab", { name: new RegExp(`^${tab.replace("/", "\\/")}`) });
    if (await t.count()) await t.click();
    await check(dialog(page));
  }
});

test("no autocorrect or spell check in Settings", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await check(dialog(page));
  await dialog(page).getByRole("combobox", { name: "Grid font family" }).click();
  await check(dialog(page));
});

test("no autocorrect in the object editor; only comments spell check", async ({ page }) => {
  await treeItem(page, "users").first().click();
  await page.keyboard.press("Meta+F6");
  await expect(dialog(page).locator(".ui-modal-title")).toContainText("Modify Table");
  await check(dialog(page), true);
  // Names and types never spell check
  await expect(dialog(page).getByRole("combobox", { name: "Type of id" })).toHaveAttribute("spellcheck", "false");
  await expect(dialog(page).getByRole("textbox").first()).toHaveAttribute("spellcheck", "false");
});

test("no autocorrect or spell check in the grid filter", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await check(work(page).locator(".dg-filter-field"));
});
