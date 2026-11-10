import type { Page } from "@playwright/test";
import TABLE from "../crates/shared/src/themes.json" with { type: "json" };
import { expect, openApp, openInTheme, test, treeItem, work } from "./app";

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
  // The field belongs to the table: its columns and keywords, no table or schema names (users, public)
  await expect(rows(page)).toHaveText(["user_name", "using"]);
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
  // Right after an operand the statement keywords come last
  await expect(rows(page)).toHaveText(["desc", "delete"]);
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

test("⌃Space lists where typing doesn't: the table's columns first at an empty spot, then keywords", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  await page.keyboard.press("Control+Space");
  await expect(rows(page).first()).toHaveText("id");
  await expect(rows(page).filter({ hasText: /^and$/ })).toHaveCount(1);
  await page.keyboard.press("Enter");
  await where.pressSequentially(" > 1 and ");
  await expect(list(page)).toHaveCount(0);
  await page.keyboard.press("Control+Space");
  await expect(rows(page).first()).toHaveText("id");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(where).toHaveValue(/^id > 1 and \w+$/);
});

test("a whole keyword or column closes the list, so ↩ applies (`is null` doesn't become `is nulls`)", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  await where.pressSequentially("user_name is nul");
  // After `is` only what can follow it (`nulls` can't)
  await expect(rows(page)).toHaveText(["null"]);
  await where.pressSequentially("l");
  await expect(list(page)).toHaveCount(0);
  const sqls = await spyQueries(page);
  await page.keyboard.press("Enter");
  await expect.poll(sqls).toEqual([expect.stringContaining("WHERE user_name is null")]);
});

test("other tables and schemas are never listed, typed or asked with ⌃Space", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  // The mock's user function order_total is a function, not a table: it stays
  await where.pressSequentially("orde");
  await expect(rows(page)).toHaveText(["order_total", "order"]);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+Space");
  await expect(rows(page)).toHaveText(["order_total", "order"]);
  await page.keyboard.press("Escape");
  await where.fill("");
  await page.keyboard.press("Control+Space");
  await expect(rows(page).first()).toHaveText("id");
  await expect(rows(page).filter({ hasText: /^(orders|users|public)$/ })).toHaveCount(0);
});

// #136: the fields are drawn in the console's colors (qurry_shared::highlight) on a layer over the input, whose own text is transparent
const rgb = (h: string) => `rgb(${[1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ")})`;
/** Each colored piece of a field's layer: [text, color] */
const colored = (page: Page, kw: "WHERE" | "ORDER BY") =>
  work(page)
    .locator(`.dg-filter-field:has([aria-label="${kw}"]) .ui-hl-layer [class^="ui-hl-"]`)
    .evaluateAll((els) => els.map((e) => [e.textContent, e.className.slice(6), getComputedStyle(e).color]));
for (const id of ["dark", "light"]) {
  test(`WHERE and ORDER BY are drawn in the console's colors; caret, completion and ↩ stay the input's (${id})`, async ({ page }) => {
    const c = TABLE.themes.find((t) => t.id === id)!.colors;
    await openInTheme(page, id);
    await treeItem(page, "users").first().dblclick();
    const where = field(page, "WHERE");
    await where.click();
    // `(` closes itself and `)` types over it (pairs), as before
    await where.pressSequentially("id IN (1, 2)");
    await expect(where).toHaveValue("id IN (1, 2)");
    // --c-number is the theme's number color, set on <html> for every theme
    expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--c-number"))).toBe(c.number);
    await expect.poll(() => colored(page, "WHERE")).toEqual([
      ["IN", "keyword", rgb(c.keyword)],
      ["1", "number", rgb(c.number)],
      ["2", "number", rgb(c.number)],
    ]);
    // The input's own text is transparent, its caret the text color; the layer starts where the input's text does
    expect(await where.evaluate((e) => [getComputedStyle(e).color, getComputedStyle(e).caretColor])).toEqual(["rgba(0, 0, 0, 0)", rgb(c.text)]);
    const [box, text] = [await where.boundingBox(), await work(page).locator('.dg-filter-field:has([aria-label="WHERE"]) .ui-hl-layer > span').boundingBox()];
    expect(Math.abs(text!.x - box!.x)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(text!.y + text!.height / 2 - (box!.y + box!.height / 2))).toBeLessThanOrEqual(1);
    // Completion lists and picks as before; the caret lands after the pick
    await where.pressSequentially(" and us");
    await expect(rows(page).first()).toHaveText("user_name");
    await page.keyboard.press("Enter");
    await expect(where).toHaveValue("id IN (1, 2) and user_name");
    expect(await where.evaluate((e: HTMLInputElement) => e.selectionStart)).toBe(26);
    await where.pressSequentially(" is not null");
    await expect.poll(async () => (await colored(page, "WHERE")).map((x) => x[0])).toEqual(["IN", "1", "2", "and", "is", "not", "null"]);
    const order = field(page, "ORDER BY");
    await order.click();
    await order.pressSequentially("user_name DESC");
    await expect.poll(() => colored(page, "ORDER BY")).toEqual([["DESC", "keyword", rgb(c.keyword)]]);
    const sqls = await spyQueries(page);
    await page.keyboard.press("Enter");
    await expect.poll(sqls).toEqual([expect.stringMatching(/WHERE id IN \(1, 2\) and user_name is not null\s+ORDER BY user_name DESC/)]);
  });
}

test("a long filter scrolled to the caret: the colored layer moves with the input's scroll", async ({ page }) => {
  const where = field(page, "WHERE");
  await where.click();
  await where.pressSequentially(Array.from({ length: 30 }, (_, i) => `id = ${i}`).join(" or "));
  const scroll = () =>
    where.evaluate((e: HTMLInputElement) => [e.scrollLeft, new DOMMatrix(getComputedStyle(e.parentElement!.querySelector(".ui-hl-layer > span")!).transform).m41]);
  await expect.poll(async () => (await scroll())[0]).toBeGreaterThan(0);
  const [left, x] = await scroll();
  expect(x + left).toBe(0);
  // Scrolled back (as the input does when the caret goes home), and after leaving the field
  await where.evaluate((e: HTMLInputElement) => (e.scrollLeft = 0));
  await expect.poll(async () => (await scroll())[1]).toBe(0);
  await where.evaluate((e: HTMLInputElement) => (e.scrollLeft = 300));
  await where.blur();
  await expect.poll(async () => (await scroll()).reduce((a, b) => a + b)).toBe(0);
});
