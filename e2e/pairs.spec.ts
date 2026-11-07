import type { Page } from "@playwright/test";
import { editor, expect, openApp, openConsole, test, treeItem, typeSql, work } from "./app";

// Quotes and brackets (qurry_shared::pairs): the table tab's WHERE / ORDER BY fields pair them as the console's Monaco does,
// and the console marks one left open
const field = (page: Page, kw: "WHERE" | "ORDER BY") => work(page).getByRole("combobox", { name: kw, exact: true });
/** The field's value with its caret as | (or its selection as «…») */
const shown = (page: Page, kw: "WHERE" | "ORDER BY") =>
  field(page, kw).evaluate((el: HTMLInputElement) => {
    const [a, b, v] = [el.selectionStart!, el.selectionEnd!, el.value];
    return a === b ? v.slice(0, a) + "|" + v.slice(a) : v.slice(0, a) + "«" + v.slice(a, b) + "»" + v.slice(b);
  });

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => {
    throw e;
  });
  await openApp(page);
});

test("WHERE: ' ( close themselves, the closing one types over, ⌫ removes an empty pair, a selection is wrapped; ↩ applies it", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  const where = field(page, "WHERE");
  await where.click();
  await where.pressSequentially("user_name in (");
  await expect.poll(() => shown(page, "WHERE")).toBe("user_name in (|)");
  await where.pressSequentially("'a");
  await expect.poll(() => shown(page, "WHERE")).toBe("user_name in ('a|')");
  // Typed over: the field doesn't grow
  await where.pressSequentially("', '");
  await expect.poll(() => shown(page, "WHERE")).toBe("user_name in ('a', '|')");
  await page.keyboard.press("Backspace");
  await expect.poll(() => shown(page, "WHERE")).toBe("user_name in ('a', |)");
  await where.pressSequentially("'user_1')");
  await expect.poll(() => shown(page, "WHERE")).toBe("user_name in ('a', 'user_1')|");
  // Not inside a string
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Backspace");
  await where.pressSequentially("user_name like '(");
  await expect.poll(() => shown(page, "WHERE")).toBe("user_name like '(|'");
  // Around a selection: wrapped, the selection kept
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Backspace");
  await where.pressSequentially("user_1");
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("'");
  await expect.poll(() => shown(page, "WHERE")).toBe("'«user_1»'");
  // ← collapses the selection to its start, ← again goes before the quote
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect.poll(() => shown(page, "WHERE")).toBe("|'user_1'");
  await where.pressSequentially("user_name = ");
  await expect(where).toHaveValue("user_name = 'user_1'");
  // ⎋ closes a completion list first (⎋ with none resets the field)
  if (await page.locator(".dg-filter-complete").isVisible()) await page.keyboard.press("Escape");
  await where.press("Enter");
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
});

test("console: an unclosed ( is marked from it to the end of its line, with what is open; closing it clears the mark", async ({ page }) => {
  await openConsole(page);
  // Typed with Monaco's own auto-closing: delete the ) it adds to leave the ( open
  await typeSql(page, "select count(*");
  await page.keyboard.press("Delete");
  await page.keyboard.press("End");
  await page.keyboard.type("\nfrom users");
  // The completion list and count('s signature help (the caret is still inside the open call) would cover the squiggle's hover
  await expect(page.locator(".parameter-hints-widget")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.locator(".parameter-hints-widget")).toBeHidden();
  const squiggle = editor(page).locator(".squiggly-error");
  await expect(squiggle).toHaveCount(1);
  // On line 1, from the ( (the 13th character) to the line end: measured against the line's monospace text
  const line = editor(page).locator(".view-line", { hasText: "select count" });
  const text = (await line.innerText()).replace(/ /g, " ");
  expect(text).toBe("select count(*");
  const lb = (await line.locator("span").first().boundingBox())!;
  const cw = lb.width / text.length;
  const sb = (await squiggle.boundingBox())!;
  expect(Math.abs(sb.y - lb.y)).toBeLessThan(lb.height);
  expect(Math.abs(sb.x - (lb.x + 12 * cw))).toBeLessThan(2);
  expect(Math.abs(sb.x + sb.width - (lb.x + lb.width))).toBeLessThan(2);
  await page.mouse.move(sb.x + sb.width / 2, sb.y);
  await expect(page.locator(".monaco-hover:visible")).toContainText("Unclosed '('");
  // Closing it clears the mark
  await page.keyboard.press("Meta+ArrowUp");
  await page.keyboard.press("End");
  await page.keyboard.type(")");
  await expect(squiggle).toHaveCount(0);
  // In a string or comment a ( counts for nothing
  await typeSql(page, "select 1 -- (");
  await page.waitForTimeout(400);
  await expect(squiggle).toHaveCount(0);
});
