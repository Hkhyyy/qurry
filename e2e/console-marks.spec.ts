import { editor, expect, openApp, openConsole, resultTabs, runAll, test, typeSql, work } from "./app";

// Gutter marks of executed statements (DataGrip): ✓ done, ✗ failed, a spinner while running. The mock fails statements on no_such_table
const marks = (page: Parameters<typeof editor>[0]) => editor(page).locator(".stmt-mark");
/** The editor line (1-based) a gutter mark sits on */
const lineOf = async (page: Parameters<typeof editor>[0], cls: string) => {
  const y = (await editor(page).locator(`.stmt-mark.${cls}`).boundingBox())!.y;
  const nums = editor(page).locator(".line-numbers");
  for (let i = 0; i < (await nums.count()); i++) {
    const b = (await nums.nth(i).boundingBox())!;
    if (Math.abs(b.y - y) < 2) return Number(await nums.nth(i).innerText());
  }
  return null;
};

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openConsole(page);
});

test("a run stops at the failed statement: ✓ on the 1st, ✗ on the 2nd, none on the 3rd; hover, edit and click", async ({ page }) => {
  await typeSql(page, "select 1;\nselect * from no_such_table;\nselect * from users;");
  await runAll(page);
  await expect(editor(page).locator(".stmt-mark.error")).toHaveCount(1);
  await expect(marks(page)).toHaveCount(2);
  expect(await lineOf(page, "ok")).toBe(1);
  expect(await lineOf(page, "error")).toBe(2);
  // The run stopped there: the 3rd statement never ran (Output has no echo for it)
  await expect(work(page).locator(".output-log")).not.toContainText("select * from users");

  // Hover: when it ran, and its result or error
  await editor(page).locator(".stmt-mark.error").hover();
  await expect(page.locator(".monaco-hover:visible")).toContainText(/Failed \d{4}-\d\d-\d\d \d\d:\d\d:\d\d/);
  await expect(page.locator(".monaco-hover:visible")).toContainText('ERROR: relation "no_such_table" does not exist');
  await editor(page).locator(".stmt-mark.ok").hover();
  await expect(page.locator(".monaco-hover:visible")).toContainText(/Executed \d{4}-\d\d-\d\d \d\d:\d\d:\d\d\s*\d+\+? rows? retrieved starting from 1 in \d+ ms/);

  // Click: switches the result area to Output and selects that statement's line
  await resultTabs(page).nth(1).click();
  await expect(work(page).locator(".output-log")).toHaveCount(0);
  await editor(page).locator(".stmt-mark.ok").click();
  await expect(work(page).locator(".output-log .ol.on")).toHaveText(/> select 1;?$/);

  // A new line above: the marks follow their statements
  await editor(page).locator(".view-lines").click();
  await page.keyboard.press("Meta+ArrowUp");
  await page.keyboard.press("Home");
  await page.keyboard.press("Enter");
  await expect.poll(() => lineOf(page, "ok")).toBe(2);
  expect(await lineOf(page, "error")).toBe(3);

  // Editing the failed statement drops its mark, the other stays (the caret is on line 2 after the Enter)
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowLeft");
  await page.keyboard.type("x");
  await expect(editor(page).locator(".stmt-mark.error")).toHaveCount(0);
  await expect(marks(page)).toHaveCount(1);

  // A new run replaces the marks of the statements it runs (no doubles): the edited one runs now, so all three are done
  await runAll(page);
  await expect(editor(page).locator(".stmt-mark.ok")).toHaveCount(3);
  await expect(marks(page)).toHaveCount(3);
});

test("a running statement shows a spinner until it is done", async ({ page }) => {
  await typeSql(page, "select pg_sleep(1.5)");
  await page.keyboard.press("Meta+Enter");
  await expect(editor(page).locator(".stmt-mark.running")).toHaveCount(1);
  await expect(editor(page).locator(".stmt-mark.ok")).toHaveCount(1, { timeout: 5000 });
  await expect(marks(page)).toHaveCount(1);
});
