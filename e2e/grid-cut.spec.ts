import type { Page } from "@playwright/test";
import { expect, menuItem, openApp, openConsole, openSettings, runAll, test, typeSql, work } from "./app";

// Long values (Settings › Appearance › Data grid): the mock cuts values past gridCutChars (4096) as the core does and lists them in
// QueryResult.cut; fetch_values reads public.cut_demo's whole by its key (lib/mock.ts cutDemo). cut_log has no key
const WHOLE = "long note ".repeat(600) + "the end";
const CUT = WHOLE.slice(0, 4096);
const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());
const cutCells = (page: Page) => work(page).locator(".cell.cut");
/** The cut note of row 2 (the only cut cell of the note column) */
const cutNote = (page: Page) => work(page).locator(".grid-row .cell.cut").filter({ hasText: /^long note/ });
const panel = (page: Page) => work(page).locator(".value-panel");

/** Record the backend calls with their arguments from now on */
async function spy(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__calls = [];
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: unknown) => (w.__calls.push([cmd, args]), orig(cmd, args));
  });
  return (cmd: string) => page.evaluate((cmd) => ((window as any).__calls as [string, any][]).filter(([c]) => c === cmd).map(([, a]) => a), cmd);
}

async function run(page: Page, sql: string) {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, sql);
  await runAll(page);
  await expect(cutCells(page).first()).toBeVisible();
}

test("a cut cell ends with a dim …, and says where it was cut", async ({ page }) => {
  await run(page, "select * from cut_demo");
  // doc of rows 1–3 and note of row 2
  await expect(cutCells(page)).toHaveCount(4);
  await expect(cutNote(page)).toHaveAttribute("title", "Cut at 4096 characters — the Value Editor (⇧↩) shows the whole value");
  const mark = cutNote(page).locator(".cell-cut");
  await expect(mark).toHaveText("…");
  // The marker stays inside the cell however long the text
  const [cell, m] = [await cutNote(page).boundingBox(), await mark.boundingBox()];
  expect(m!.x + m!.width).toBeLessThanOrEqual(cell!.x + cell!.width);
});

test("⇧↩ shows the whole value in the Value Editor, read by the key", async ({ page }) => {
  await run(page, "select * from cut_demo");
  const calls = await spy(page);
  await cutNote(page).click();
  await page.keyboard.press("Shift+Enter");
  await expect(panel(page).locator("textarea.ve-text")).toHaveValue(WHOLE);
  await expect(panel(page).locator("textarea.ve-text")).not.toHaveAttribute("readonly");
  expect(await calls("fetch_values")).toEqual([{ id: "dev", tabId: expect.any(String), req: { schema: "public", table: "cut_demo", key: ["id"], columns: ["note"], rows: [["2"]] } }]);
  // Read once: the marker is gone
  await expect(cutCells(page)).toHaveCount(3);
});

test("editing a cut cell edits its whole value, and Submit's UPDATE has all of it", async ({ page }) => {
  await run(page, "select * from cut_demo");
  const calls = await spy(page);
  await cutNote(page).dblclick();
  const input = work(page).locator(".cell-input");
  await expect(input).toHaveValue(WHOLE);
  await page.keyboard.press("End");
  await page.keyboard.type("!");
  await page.keyboard.press("Enter");
  await work(page).getByRole("button", { name: /^Submit/ }).click();
  await expect.poll(async () => (await calls("submit_edits"))[0]?.statements).toEqual([`UPDATE public.cut_demo SET note = '${WHOLE}!' WHERE id = '2';`]);
});

test("Copy of a selection with cut cells copies the whole values", async ({ page }) => {
  await run(page, "select * from cut_demo");
  await work(page).getByRole("gridcell", { name: "note 1", exact: true }).click();
  await work(page).getByRole("gridcell", { name: "note 3", exact: true }).click({ modifiers: ["Shift"] });
  await page.keyboard.press("Meta+C");
  await expect.poll(() => clipboard(page)).toBe(`note 1\n${WHOLE}\nnote 3`);
});

test("Copy as SQL Updates has the whole value in SET", async ({ page }) => {
  await run(page, "select * from cut_demo");
  await work(page).locator('[title="Data Extractor (copy format)"]').click();
  await menuItem(page, /^(✓ |\s*)SQL Updates$/).click();
  await cutNote(page).click();
  await page.keyboard.press("Meta+C");
  await expect.poll(() => clipboard(page)).toBe(`UPDATE public.cut_demo SET note = '${WHOLE}' WHERE id = '2';`);
});

test("a result without a key keeps the cut text, read-only, and says why", async ({ page }) => {
  await run(page, "select * from cut_log");
  const calls = await spy(page);
  await cutNote(page).click();
  await page.keyboard.press("Shift+Enter");
  await expect(panel(page).locator("textarea.ve-text")).toHaveValue(CUT);
  await expect(panel(page).locator("textarea.ve-text")).toHaveAttribute("readonly");
  await expect(panel(page).locator(".ve-note")).toHaveText("Cut at 4096 characters; the whole value needs a primary key. Read-only: public.cut_log has no primary key.");
  await cutNote(page).click();
  await page.keyboard.press("Meta+C");
  await expect.poll(() => clipboard(page)).toBe(CUT);
  await expect(page.locator(".statusbar")).toContainText("1 value is cut (no key to fetch it whole); raise Settings › Appearance › Data grid › Long values");
  expect(await calls("fetch_values")).toEqual([]);
});

test("Settings › Data grid › Long values saves through set_grid_cut_chars; the next run cuts there", async ({ page }) => {
  await openApp(page);
  const calls = await spy(page);
  await openSettings(page, "Data grid");
  const input = page.getByLabel("Cut values longer than");
  await expect(input).toHaveValue("4096");
  await input.fill("100");
  await expect.poll(() => calls("set_grid_cut_chars")).toEqual([{ n: 100 }]);
  await page.keyboard.press("Escape");
  await openConsole(page);
  await typeSql(page, "select * from cut_demo");
  await runAll(page);
  await expect(cutNote(page)).toHaveAttribute("title", /^Cut at 100 characters/);
});
