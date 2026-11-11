import type { Page } from "@playwright/test";
import { dialog, editor, expect, menuItem, openApp, openConsole, resultTabs, runAll, spyRuns, test, typeSql, work } from "./app";

// A console's result tabs follow its statements (#151, lib/resultTabs): one per statement, shown when the caret enters it, replaced
// by a rerun, closed with its statement, at most Settings › Result tabs unpinned

const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
const tab = (page: Page, label: string | RegExp) => resultTabs(page).filter({ hasText: label });
const shown = (page: Page) => work(page).locator(".result-tabs [role=tab][aria-selected=true] .tab-label");
const labels = (page: Page) => work(page).locator(".result-tabs [role=tab] .tab-label");
/** Run the statement at the caret: ⌘↩, then its first choice in the Statements popup (Escape first: a completion list would take the ⌘↩) */
const run = async (page: Page) => {
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  await expect(work(page).locator(".stmt-chooser .sc-item").first()).toBeVisible();
  await page.keyboard.press("Enter");
};
/** The caret to the start of line `n` (1-based) */
const caretTo = async (page: Page, n: number) => {
  await page.keyboard.press("Meta+ArrowUp");
  for (let i = 1; i < n; i++) await page.keyboard.press("ArrowDown");
};

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openConsole(page);
});

test("one tab per statement: the caret shows it, a rerun replaces it, a pinned one stays, a deleted statement closes its tab", async ({ page }) => {
  await typeSql(page, "select * from users limit 2;\nselect * from payments;");
  await caretTo(page, 1);
  await run(page);
  await expect(labels(page)).toHaveText(["Output", "1: users"]);
  await caretTo(page, 2);
  await run(page);
  await expect(labels(page)).toHaveText(["Output", "1: users", "2: payments"]);
  await expect(shown(page)).toHaveText("2: payments");
  const id = await tab(page, "1: users").getAttribute("data-result");

  // The caret back in statement 1 shows its tab, without running anything
  const runs = await spyRuns(page);
  await caretTo(page, 1);
  await expect(shown(page)).toHaveText("1: users");
  await expect(cell(page, "user_2")).toBeVisible();
  expect(await runs()).toBe(0);

  // Edited and run again: the same tab (its id, so its column widths) gets the new rows
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Backspace");
  await page.keyboard.type("3");
  await run(page);
  await expect(labels(page)).toHaveText(["Output", "1: users", "2: payments"]);
  await expect(cell(page, "user_3")).toBeVisible();
  await expect(tab(page, "1: users")).toHaveAttribute("data-result", id!);

  // Pinned: never overwritten; running the statement again opens a new tab that follows it
  await tab(page, "1: users").click({ button: "right" });
  await menuItem(page, "Pin Tab").click();
  await editor(page).click();
  await caretTo(page, 1);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Backspace");
  await page.keyboard.type("1");
  await run(page);
  await expect(labels(page)).toHaveText(["Output", /^1: users/, "2: payments", "1: users"]);
  await expect(cell(page, "user_2")).toHaveCount(0);
  await resultTabs(page).nth(1).click();
  await expect(cell(page, "user_3")).toBeVisible();
  await expect(resultTabs(page).nth(1).locator(".tab-pin")).toBeVisible();

  // A statement added and run, then deleted: its tab closes and the tabs after it get their numbers back
  await editor(page).click();
  await caretTo(page, 2);
  await page.keyboard.press("Home");
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.type("select * from contracts;");
  await caretTo(page, 2);
  await run(page);
  await expect(labels(page)).toHaveText(["Output", /^1: users/, "3: payments", "1: users", "2: contracts"]);
  await caretTo(page, 2);
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Backspace");
  await expect(labels(page)).toHaveText(["Output", /^1: users/, "2: payments", "1: users"]);
});

test("Settings › Result tabs: 0 isn't saved; past the number the least recently shown unpinned tab closes", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const field = dialog(page).getByRole("spinbutton", { name: "Result tabs" });
  await expect(field).toHaveValue("10");
  await field.fill("0");
  await expect(dialog(page)).toContainText("Enter 1 or more.");
  await expect(field).toHaveValue("0");
  expect(await page.evaluate(() => localStorage.getItem("mock-resultTabs"))).toBeNull();
  await field.fill("2");
  await expect(dialog(page)).not.toContainText("Enter 1 or more.");
  expect(await page.evaluate(() => localStorage.getItem("mock-resultTabs"))).toBe("2");
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);

  await editor(page).click();
  await typeSql(page, "select * from users limit 1;\nselect * from orders;\nselect * from payments;");
  await runAll(page);
  await expect(labels(page)).toHaveText(["Output", "2: orders", "3: payments"]);
});

test("an error shows Output and the statement keeps its tab; a caret move never asks to discard a grid's edits", async ({ page }) => {
  await typeSql(page, "select * from users limit 3;\nselect * from payments;");
  await runAll(page);
  await expect(labels(page)).toHaveText(["Output", "1: users", "2: payments"]);

  // Pending edit in statement 1's grid, then the caret into statement 2: no Pending changes, the tab stays
  await tab(page, "1: users").click();
  await cell(page, "user_2").dblclick();
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.type("x");
  await page.keyboard.press("Enter");
  await expect(work(page).locator(".tb-info")).toContainText("1 pending");
  await editor(page).click();
  await caretTo(page, 2);
  await expect(dialog(page)).toHaveCount(0);
  await expect(shown(page)).toHaveText("1: users");

  // Statement 2 now fails: Output is shown, its tab is still there with its last result
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  for (let i = 0; i < "payments".length; i++) await page.keyboard.press("Backspace");
  await page.keyboard.type("no_such_table");
  await run(page);
  // The run asks first: the grid's edit would go
  await dialog(page).getByRole("button", { name: "Discard" }).click();
  await expect(shown(page)).toHaveText("Output");
  await expect(labels(page)).toHaveText(["Output", "1: users", "2: payments"]);
});

test("the caret moving within its statement keeps a tab picked by hand, and Output after an error; entering another statement shows its tab", async ({ page }) => {
  await typeSql(page, "select * from users limit 3;\nselect * from payments;");
  await caretTo(page, 1);
  await run(page);
  await caretTo(page, 2);
  await run(page);
  await expect(shown(page)).toHaveText("2: payments");
  // Picked to compare, then back to typing in statement 2 (the caret where it was)
  await tab(page, "1: users").click();
  await editor(page).locator("textarea").first().focus();
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.waitForTimeout(300);
  await expect(shown(page)).toHaveText("1: users");
  await page.keyboard.press("ArrowUp");
  await expect(shown(page)).toHaveText("1: users");
  await page.keyboard.press("ArrowDown");
  await expect(shown(page)).toHaveText("2: payments");

  // Statement 2 fails: Output stays while it is fixed
  for (let i = 0; i < "payments".length; i++) await page.keyboard.press("Backspace");
  await page.keyboard.type("no_such_table");
  await run(page);
  await expect(shown(page)).toHaveText("Output");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(300);
  await expect(shown(page)).toHaveText("Output");
});

test("↻ on a tab that follows no statement, or on a pinned one, refreshes it in place", async ({ page }) => {
  await typeSql(page, "select * from users limit 3;\nselect * from payments;");
  await caretTo(page, 1);
  for (let i = 0; i < "select * from users".length; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Meta+Enter");
  await expect(labels(page)).toHaveText(["Output", "1: users"]);
  const id = await tab(page, "1: users").getAttribute("data-result");
  const reload = work(page).getByRole("button", { name: /^Reload Page/ });
  await reload.click();
  await expect(cell(page, "user_3")).toBeVisible();
  await reload.click();
  await expect(cell(page, "user_3")).toBeVisible();
  await expect(labels(page)).toHaveText(["Output", "1: users"]);
  await expect(tab(page, "1: users")).toHaveAttribute("data-result", id!);

  await tab(page, "1: users").click({ button: "right" });
  await menuItem(page, "Pin Tab").click();
  await reload.click();
  await expect(cell(page, "user_3")).toBeVisible();
  await expect(resultTabs(page)).toHaveCount(2);
  await expect(resultTabs(page).nth(1).locator(".tab-pin")).toBeVisible();
  await expect(resultTabs(page).nth(1)).toHaveAttribute("data-result", id!);
});
