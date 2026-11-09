// The shared completion engine through Monaco on the mock (src/lib/mock.ts completion_meta: orders.user_id → users.id, the user
// function order_total): FK join conditions, templates, signature help, unknown names underlined with quick fixes (⌥↩)
import { editor, expect, openApp, openConsole, suggestions, test, typeSql } from "./app";
import type { Page } from "@playwright/test";

const firstLine = (page: Page) => editor(page).locator(".view-line").first();

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openConsole(page);
});

test("after `join <table>` ⌃Space offers the ON condition from the foreign key; after `join` the related table comes first", async ({ page }) => {
  await typeSql(page, "select * from orders o join users u ");
  await page.keyboard.press("Control+Space");
  await expect(suggestions(page).first()).toContainText("on o.user_id = u.id");
  await expect(suggestions(page).first()).toContainText("fk orders → users");
  await page.keyboard.press("Enter");
  await expect(firstLine(page)).toHaveText(/^select\s\*\sfrom\sorders\so\sjoin\susers\su\son\so\.user_id\s=\su\.id$/);
  await typeSql(page, "select * from users u join ");
  await page.keyboard.press("Control+Space");
  await expect(suggestions(page).first()).toContainText(/^orders\s*public · fk users/);
});

test("a statement start lists statement keywords, not the previous statement's columns; `upd` writes its template, ⇥ goes from placeholder to placeholder", async ({ page }) => {
  await typeSql(page, "select * from orders");
  // The list for `orders` may open after a first ⎋ on a slow machine (CI): Enter would pick from it instead of starting a line.
  // ⎋ until the list stays closed for a moment
  await expect(async () => {
    await page.keyboard.press("Escape");
    await page.waitForTimeout(400);
    expect(await suggestions(page).count()).toBe(0);
  }).toPass();
  await page.keyboard.press("Enter");
  await page.keyboard.type("se");
  await expect(suggestions(page).first()).toBeVisible();
  const labels = await suggestions(page).locator(".label-name").allInnerTexts();
  expect(labels).toEqual(expect.arrayContaining(["select", "set", "sel"]));
  expect(labels).not.toContain("status");
  await page.keyboard.press("Escape");
  await typeSql(page, "upd");
  await suggestions(page).filter({ has: page.locator(".label-name", { hasText: /^upd$/ }) }).click();
  await page.keyboard.type("users");
  // The table's own suggestion closes first (⎋ once more would leave the template)
  await expect(suggestions(page).first()).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Tab");
  await page.keyboard.type("status");
  await expect(firstLine(page)).toHaveText(/^update\susers\sset\sstatus\s=\svalue\swhere\scondition$/);
});

test("`*` asked for (⌃Space) expands into its columns", async ({ page }) => {
  await typeSql(page, "select * from users");
  for (let i = 0; i < " from users".length; i++) await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Control+Space");
  await expect(suggestions(page)).toHaveCount(1);
  await expect(suggestions(page).first()).toContainText("expand *");
  await page.keyboard.press("Enter");
  await expect(firstLine(page)).toHaveText(/^select\sid,\suser_name\sfrom\susers$/);
});

test("inside a call's parentheses signature help shows the function and the parameter at the caret", async ({ page }) => {
  await typeSql(page, "select order_total(1, ");
  const hints = page.locator(".parameter-hints-widget");
  await expect(hints).toBeVisible();
  await expect(hints).toContainText("order_total(order_id bigint, with_tax boolean) → numeric");
  await expect(hints.locator(".parameter.active")).toHaveText("with_tax boolean");
  await page.keyboard.type("true)");
  await expect(hints).toBeHidden();
});

test("an unknown column is underlined; ⌥↩ on it offers the closest names and picking one fixes it", async ({ page }) => {
  await typeSql(page, "select nme from users");
  await expect(editor(page).locator(".squiggly-warning")).toHaveCount(1);
  await page.keyboard.press("Home");
  for (let i = 0; i < 8; i++) await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Alt+Enter");
  const fix = page.getByText("Change to user_name");
  await expect(fix).toBeVisible();
  await fix.click();
  await expect(firstLine(page)).toHaveText(/^select\suser_name\sfrom\susers$/);
  await expect(editor(page).locator(".squiggly-warning")).toHaveCount(0);
  // No false alarm for a CTE, an alias or a function
  await typeSql(page, "with r as (select id k from users) select r.k, lower(k) n from r order by n");
  await page.waitForTimeout(600);
  await expect(editor(page).locator(".squiggly-warning")).toHaveCount(0);
});
