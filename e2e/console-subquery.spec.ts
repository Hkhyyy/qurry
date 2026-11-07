import { dialog, editor, expect, openApp, openConsole, resultTabs, test, typeSql, work } from "./app";
import type { Page } from "@playwright/test";

// Run a subquery (⌘↩ inside one offers it after the whole statement, DataGrip) and the server's error marked in the editor.
// The mock fails statements on no_such_table, with PostgreSQL's position of it

const chooser = (page: Page) => work(page).locator(".stmt-chooser");
/**
 * Where an overlay class (the chooser's .stmt-preview, the error's .squiggly-error) is drawn, in characters of its line:
 * [{ line, from, to }] (1-based line, 0-based columns; the console's text is ASCII, so every character is as wide)
 */
const overlays = (page: Page, cls: string) =>
  editor(page).evaluate((ed, cls) => {
    const lines = [...ed.querySelectorAll<HTMLElement>(".view-line")];
    const h = parseFloat(getComputedStyle(lines[0]).height);
    const top = (el: HTMLElement) => parseFloat(el.style.top);
    const width = (l: HTMLElement) => (l.firstElementChild as HTMLElement).getBoundingClientRect().width / l.textContent!.length;
    return [...ed.querySelectorAll<HTMLElement>(`.view-overlays .${cls}`)]
      .map((x) => {
        const row = top(x.parentElement as HTMLElement);
        const line = lines.find((l) => Math.abs(top(l) - row) < 1)!;
        const cw = width(line);
        const left = parseFloat(x.style.left) - (line.getBoundingClientRect().left - (x.parentElement as HTMLElement).getBoundingClientRect().left);
        return { line: Math.round(row / h) + 1, from: Math.round(left / cw), to: Math.round((left + x.getBoundingClientRect().width) / cw) };
      })
      .sort((a, b) => a.line - b.line);
  }, cls);
/** The caret to (1-based) line and column, through Go to Line (⌘L) */
const caretTo = async (page: Page, line: number, col: number) => {
  await page.keyboard.press("Meta+l");
  await page.keyboard.type(`${line}:${col}`);
  await page.keyboard.press("Enter");
};

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => {
    throw e;
  });
  await openApp(page);
});

test("⌘↩ inside a subquery offers it after the statement, innermost first, highlights exactly it and runs only it", async ({ page }) => {
  await openConsole(page);
  const sql = "select * from orders where user_id in (select id from users where n > (select avg(n) from users));";
  await typeSql(page, `select 1;\n${sql}`);
  const inner = sql.indexOf("select avg");
  await caretTo(page, 2, inner + 3);
  await page.keyboard.press("Meta+Enter");
  await expect(chooser(page)).toBeVisible();
  await expect(chooser(page).locator(".sc-item")).toHaveText([sql.slice(0, -1).slice(0, 80), "select avg(n) from users", "select id from users where n > (select avg(n) from users)", "All statements (2)"]);
  await page.keyboard.press("ArrowDown");
  await expect.poll(() => overlays(page, "stmt-preview")).toEqual([{ line: 2, from: inner, to: inner + "select avg(n) from users".length }]);
  await page.keyboard.press("ArrowDown");
  const outer = sql.indexOf("select id");
  await expect.poll(() => overlays(page, "stmt-preview")).toEqual([{ line: 2, from: outer, to: sql.length - 2 }]);
  await page.keyboard.press("Enter");
  await expect(chooser(page)).toHaveCount(0);
  await resultTabs(page).first().click();
  const log = work(page).locator(".output-log");
  await expect(log).toContainText("> select id from users where n > (select avg(n) from users)");
  await expect(log).not.toContainText("from orders");
});

test("a subquery picked on a Production source asks like any change", async ({ page }) => {
  await openConsole(page, "[PROD] shop_app");
  const sub = "with d as (delete from users where id = 4 returning id) select id from d";
  await typeSql(page, `select * from (${sub}) x;`);
  await caretTo(page, 1, `select * from (${sub}`.length - 3);
  await page.keyboard.press("Meta+Enter");
  await expect(chooser(page).locator(".sc-item").nth(1)).toHaveText(sub);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [PROD] shop_app");
  await expect(dialog(page).locator(".ui-detail-row")).toHaveCount(1);
  await expect(dialog(page).locator(".ui-detail-row")).toContainText(sub);
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(work(page).locator(".output-log")).not.toContainText("with d as");
});

test("a failed statement after the first: the error is marked at the server's position, with its message; an edit or the next run clears it, Output keeps it", async ({ page }) => {
  await openConsole(page);
  await typeSql(page, "select 1;\nselect id, n from no_such_table where n > 1;\nselect 3;");
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Meta+Enter");
  const at = "select id, n from ".length;
  await expect.poll(() => overlays(page, "squiggly-error")).toEqual([{ line: 2, from: at, to: at + "no_such_table".length }]);
  await expect(work(page).locator(".output-log")).toContainText('ERROR: relation "no_such_table" does not exist');
  // Its message on hover, with the way out
  await editor(page).locator(".squiggly-error").hover({ force: true });
  await expect(page.locator(".monaco-hover:visible")).toContainText('relation "no_such_table" does not exist');
  await expect(page.locator(".monaco-hover:visible")).toContainText("Fix the statement and run it again");
  // An edit clears it; Output keeps the error
  await caretTo(page, 3, 1);
  await page.keyboard.type(" ");
  await expect.poll(() => overlays(page, "squiggly-error")).toEqual([]);
  await expect(work(page).locator(".output-log")).toContainText('ERROR: relation "no_such_table" does not exist');
  // Again, then a run of another statement clears it before it starts
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Meta+Enter");
  await expect.poll(() => overlays(page, "squiggly-error")).toHaveLength(1);
  await caretTo(page, 1, 3);
  await page.keyboard.press("Meta+Enter");
  await expect(chooser(page)).toBeVisible();
  await page.keyboard.press("Enter");
  await expect.poll(() => overlays(page, "squiggly-error")).toEqual([]);
});
