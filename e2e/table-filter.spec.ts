import type { Page } from "@playwright/test";
import { expect, openApp, test, treeItem, work } from "./app";

// The table tab's WHERE / ORDER BY fields complete like the console (qurry_shared::complete::suggest_filter), listed in a PopupList
const field = (page: Page, kw: "WHERE" | "ORDER BY") => work(page).getByRole("combobox", { name: kw, exact: true });
const list = (page: Page) => page.locator(".dg-filter-complete");
const rows = (page: Page) => list(page).locator(".ui-list-title");
/** The SQL of each run_query from now on */
async function spyQueries(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__sqls = [];
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: any) => (cmd === "run_query" && w.__sqls.push(args.req?.sql ?? args.sql), orig(cmd, args));
  });
  return () => page.evaluate(() => (window as any).__sqls as string[]);
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
});

test("WHERE: typing a word lists the table's columns first; ↩ picks one, ↩ with the list closed applies", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  await where.pressSequentially("us");
  await expect(rows(page).first()).toHaveText("user_name");
  await expect(rows(page)).toContainText(["user_name", "users", "using"]);
  // Under the field, left-aligned to it
  const [f, l] = [await where.boundingBox(), await list(page).boundingBox()];
  expect(Math.abs(l!.x - f!.x)).toBeLessThanOrEqual(1);
  expect(l!.y).toBeGreaterThanOrEqual(f!.y + f!.height);
  await page.keyboard.press("Enter");
  await expect(where).toHaveValue("user_name");
  await expect(list(page)).toHaveCount(0);
  // Not in a string literal, nor after an operator
  await where.pressSequentially(" like 'us");
  await expect(list(page)).toHaveCount(0);
  await where.pressSequentially("%'");
  const sqls = await spyQueries(page);
  await page.keyboard.press("Enter");
  await expect.poll(sqls).toEqual([expect.stringContaining("WHERE user_name like 'us%'")]);
});

test("ORDER BY: columns, then ASC / DESC after a column; ⇥ and a click pick, the caret lands after the pick", async ({ page }) => {
  const order = field(page, "ORDER BY");
  await order.click();
  await order.pressSequentially("i");
  await expect(rows(page).first()).toHaveText("id");
  await page.keyboard.press("Tab");
  await expect(order).toHaveValue("id");
  await expect(order).toBeFocused();
  await order.pressSequentially(" de");
  await expect(rows(page)).toHaveText(["delete", "desc"]);
  await page.keyboard.press("ArrowDown");
  await expect(list(page).locator(".ui-list-item.selected .ui-list-title")).toHaveText("desc");
  await list(page).getByText("desc", { exact: true }).click();
  await expect(order).toHaveValue("id desc");
  await expect(order).toBeFocused();
  expect(await order.evaluate((e: HTMLInputElement) => e.selectionStart)).toBe(7);
});

test("⎋ closes the list first, ⌃Space reopens it, then ⎋ resets the field", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  await where.pressSequentially("id > 1 and us");
  await expect(list(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(list(page)).toHaveCount(0);
  await expect(where).toHaveValue("id > 1 and us");
  // ⌃Space asks again at the caret (as in the console); ⎋ closes it again
  await page.keyboard.press("Control+Space");
  await expect(rows(page).first()).toHaveText("user_name");
  await expect(where).toHaveValue("id > 1 and us");
  await page.keyboard.press("Escape");
  await expect(list(page)).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(where).toHaveValue("");
  await expect(where).not.toBeFocused();
});

test("a whole keyword or column closes the list, so ↩ applies (`is null` doesn't become `is nulls`)", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  await where.pressSequentially("user_name is nul");
  await expect(rows(page)).toHaveText(["null", "nulls"]);
  await where.pressSequentially("l");
  await expect(list(page)).toHaveCount(0);
  const sqls = await spyQueries(page);
  await page.keyboard.press("Enter");
  await expect.poll(sqls).toEqual([expect.stringContaining("WHERE user_name is null")]);
});
