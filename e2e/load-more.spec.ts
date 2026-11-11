import type { Page } from "@playwright/test";
import { expect, openApp, openConsole, test, treeItem, typeSql, work } from "./app";

// Loading more rows fails (#157: the core asks to run the query again after a reconnect): the whole reason sits below the grid
// with Run again, which runs it again and gives the keys back to the grid
const ERR =
  "The data source reconnected since this result was read (settings saved, disconnected or idle), and the session was changed since (SET, RESET, USE or a statement that isn't a read), so a new connection wouldn't match. Run the query again to load more rows.";

const failLoads = (page: Page, e: string | null) => page.evaluate((x) => ((globalThis as any).__mockFetchMoreError = x ?? undefined), e);
const grid = (page: Page) => work(page).locator(".grid-scroll").first();
/** Scroll the visible grid to its end, which loads more rows */
const scrollToEnd = (page: Page) => grid(page).evaluate((el) => (el.scrollTop = el.scrollHeight));
const notice = (page: Page) => work(page).locator(".grid-more-error-row");
const pos = (page: Page) => page.locator(".statusbar .ui-status-item").filter({ hasText: /^Row / });

/** The reason in full (not cut), below the scrolling rows and covering none */
async function reasonBelowTheGrid(page: Page) {
  await expect(notice(page).locator(".grid-more-error")).toHaveText(`Couldn't load more rows: ${ERR}`);
  const text = notice(page).locator(".grid-more-error");
  expect(await text.evaluate((el) => el.scrollWidth <= el.clientWidth && el.scrollHeight <= el.clientHeight + 1)).toBe(true);
  const rows = (await grid(page).boundingBox())!;
  const box = (await notice(page).boundingBox())!;
  expect(box.y).toBeGreaterThanOrEqual(rows.y + rows.height - 1);
}

/** Run again: the reason goes, and the grid has the keys right away */
async function runAgain(page: Page) {
  await failLoads(page, null);
  await notice(page).getByRole("button", { name: "Run again" }).click();
  await expect(notice(page)).toHaveCount(0);
  await expect(grid(page)).toBeFocused();
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await failLoads(page, ERR);
});

test("table data: the reason shows in full below the grid, and Run again reloads it with the keys in the grid", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await scrollToEnd(page);
  await reasonBelowTheGrid(page);
  await runAgain(page);
  // The reloaded rows keep the cell cursor: ↓ moves it
  await expect(pos(page)).toHaveText(/^Row 1, /);
  await page.keyboard.press("ArrowDown");
  await expect(pos(page)).toHaveText(/^Row 2, /);
});

test("console result: the reason shows in full below the grid, and Run again runs the statement with the keys in the new grid", async ({ page }) => {
  await openConsole(page);
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await scrollToEnd(page);
  await reasonBelowTheGrid(page);
  await runAgain(page);
  // The new result's grid takes the grid's keys (its Find), not the editor
  await page.keyboard.press("Meta+F");
  await expect(work(page).locator(".grid-find input")).toBeFocused();
  await page.keyboard.press("Escape");
  // The new result loads more again
  await scrollToEnd(page);
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows");
});
