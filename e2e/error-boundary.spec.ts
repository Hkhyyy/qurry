import { test, expect, openApp, openConsole, runAll, typeSql, treeItem, treeNode, work } from "./app";
import type { Page } from "@playwright/test";

// #181: a render error mounts its tab (or the whole app) again by itself, once, keeping the tabs, the console text and open
// transactions; failing again soon after shows a fallback (no loop). Every error reaches qurry.log

/** The backend answers `cmd` with `value` (a reply the front end can't render): `times` times, else until the page reloads */
const answer = (page: Page, cmd: string, value: unknown, times = Infinity) =>
  page.evaluate(
    ([cmd, value, times]) => {
      const w = window as any;
      const orig = w.__TAURI_INTERNALS__.invoke;
      let n = 0;
      w.__TAURI_INTERNALS__.invoke = (c: string, args: unknown) => (c === cmd && n++ < (times as number) ? Promise.resolve(value) : orig(c, args));
      w.__unanswer = () => (w.__TAURI_INTERNALS__.invoke = orig);
    },
    [cmd, value, times === Infinity ? 1e9 : times] as const,
  );
const unanswer = (page: Page) => page.evaluate(() => (window as any).__unanswer());
const boundaryLogs = (page: Page) =>
  page.evaluate(() => (((window as any).__mockLogged ?? []) as { source: string; message: string; stack?: string; componentStack?: string }[]).filter((e) => e.source === "boundary"));
/** A result the grid can't draw (a column name and a cell that are objects) */
const BAD_RESULT = { columns: [{ name: { not: "a name" }, typeName: "text" }], rows: [[{ not: "a cell" }]], hasResultSet: true, truncated: false };
const toast = (page: Page) => page.locator(".ui-toast");
const editorText = (page: Page) => work(page).locator(".monaco-editor .view-lines");
const txPending = (page: Page) => work(page).locator(".tx-pending");

/** A console in Manual mode with an open transaction, and more text typed after it ran */
async function consoleWithOpenTx(page: Page) {
  await openConsole(page);
  await work(page).locator(".console-toolbar").getByText("Tx: Auto").click();
  await page.getByRole("menuitem", { name: /Manual/ }).click();
  await work(page).locator(".monaco-editor").first().click();
  await typeSql(page, "update users set user_name = 'a' where id = 1;");
  await runAll(page);
  await expect(txPending(page)).toBeVisible();
  await page.keyboard.press("Meta+End");
  await page.keyboard.type("\nselect 2;");
}

test("a tab's one-time render error recovers by itself: a toast, the console text and the other tabs kept, the error logged", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(page.getByRole("tab", { name: /users/ })).toBeVisible();
  await openConsole(page);
  await typeSql(page, "SELECT 1");
  await answer(page, "run_query", BAD_RESULT);
  await runAll(page);
  await expect(toast(page)).toContainText(/Recovered from an error in console .* — details in the log/);
  await expect(page.locator(".error-fallback")).toHaveCount(0);
  await expect(editorText(page)).toContainText("SELECT 1");
  // Logged with its stacks, never the statement
  await expect.poll(async () => (await boundaryLogs(page)).length).toBe(1);
  const [entry] = await boundaryLogs(page);
  expect(entry.message).toMatch(/^tab: \w*Error: /);
  expect(entry.componentStack).toContain("ConsolePane");
  expect(JSON.stringify(entry)).not.toContain("SELECT 1");
  await expect(treeNode(page, "[DEV] shop_app")).toBeVisible();
  await page.getByRole("tab", { name: /users/ }).click();
  await expect(work(page).locator(".grid-scroll")).toBeVisible();
});

test("a tab that fails again after recovering shows its fallback (no loop); the rest of the window works, Reload Tab brings it back", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "SELECT 1");
  // The table tab's rows can't be drawn, also when recovering loads them again
  await answer(page, "run_query", BAD_RESULT);
  await treeItem(page, "users").first().dblclick();
  const fallback = work(page).locator(".error-fallback");
  await expect(fallback).toContainText("This tab hit an error");
  await expect(fallback.getByRole("button", { name: "Reload Tab" })).toBeVisible();
  await expect(fallback.getByRole("button", { name: "Copy Details" })).toBeVisible();
  await page.waitForTimeout(1000);
  expect((await boundaryLogs(page)).length).toBe(2);
  await expect(treeNode(page, "[DEV] shop_app")).toBeVisible();
  await page.getByRole("tab", { name: /console/i }).first().click();
  await expect(editorText(page)).toContainText("SELECT 1");
  await page.getByRole("tab", { name: /users/ }).click();
  await unanswer(page);
  await work(page).getByRole("button", { name: "Reload Tab" }).click();
  await expect(work(page).locator(".error-fallback")).toHaveCount(0);
  await expect(work(page).locator(".grid-scroll")).toBeVisible();
});

test("a one-time error outside the tabs recovers the window by itself: text typed just before, the tabs and the open transaction kept", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await consoleWithOpenTx(page);
  await page.waitForTimeout(200);
  // Every run reads the data sources again: once, a list the app can't render
  await answer(page, "list_connections", null, 1);
  await page.keyboard.press("Meta+Enter");
  await expect(toast(page)).toContainText("Recovered from an error — details in the log");
  await expect(page.locator(".error-fallback")).toHaveCount(0);
  await expect.poll(async () => (await boundaryLogs(page)).some((e) => e.message.startsWith("window: "))).toBe(true);
  await expect(page.getByRole("tab", { name: /users/ })).toBeVisible();
  await expect(editorText(page)).toContainText("select 2;");
  await expect(txPending(page)).toBeVisible();
  await expect(work(page).locator(".console-toolbar").getByRole("button", { name: "Commit" })).toBeEnabled();
});

test("an error outside the tabs that comes back shows the window's fallback (no loop); Recover keeps everything", async ({ page }) => {
  await openApp(page);
  await consoleWithOpenTx(page);
  await answer(page, "list_connections", null);
  await page.keyboard.press("Meta+Enter");
  const fallback = page.locator(".error-fallback");
  await expect(fallback).toContainText("Qurry hit an error");
  await expect(fallback.getByRole("button", { name: "Recover" })).toBeVisible();
  await expect(fallback.getByRole("button", { name: "Reload Window" })).toBeVisible();
  await page.waitForTimeout(1000);
  expect((await boundaryLogs(page)).length).toBe(2);
  await unanswer(page);
  await fallback.getByRole("button", { name: "Recover" }).click();
  await expect(treeNode(page, "[DEV] shop_app")).toBeVisible();
  await expect(editorText(page)).toContainText("select 2;");
  await expect(txPending(page)).toBeVisible();
});

test("Reload Window asks about the open transaction, keeps the console text, and the new page rolls the transaction back and says so", async ({ page }) => {
  await openApp(page);
  await consoleWithOpenTx(page);
  await answer(page, "list_connections", null);
  await page.keyboard.press("Meta+Enter");
  const fallback = page.locator(".error-fallback");
  await fallback.getByRole("button", { name: "Reload Window" }).click();
  await expect(fallback).toContainText("1 console has uncommitted changes. Reload rolls them back.");
  await fallback.getByRole("button", { name: "Cancel" }).click();
  await expect(fallback.getByRole("button", { name: "Reload and Roll Back" })).toHaveCount(0);
  await fallback.getByRole("button", { name: "Reload Window" }).click();
  await fallback.getByRole("button", { name: "Reload and Roll Back" }).click();
  await expect(treeNode(page, "[DEV] shop_app")).toBeVisible();
  await expect(toast(page)).toContainText("Reloaded: rolled back 1 open transaction in [DEV] shop_app");
  await expect(editorText(page)).toContainText("select 2;");
  await expect(txPending(page)).toHaveCount(0);
});

test("window errors and unhandled rejections reach the log, and the status bar still shows them", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => {
    setTimeout(() => {
      throw new Error("boom from a timer");
    });
    Promise.reject(new Error("rejected boom"));
  });
  const logged = () => page.evaluate(() => ((window as any).__mockLogged ?? []).map((e: any) => `${e.source}: ${e.message}`));
  await expect.poll(logged).toEqual(expect.arrayContaining(["error: Error: boom from a timer", "unhandledrejection: Error: rejected boom"]));
  await expect(page.locator(".statusbar .sb-text")).toContainText("Error:");
});
