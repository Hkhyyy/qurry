import { test, expect, openApp, openConsole, runAll, typeSql, treeItem, treeNode, work } from "./app";
import type { Page } from "@playwright/test";

// #181: a render error takes down only its tab (or, outside the tabs, shows the window's fallback) and reaches qurry.log

/** From now on the backend answers `cmd` with `value` (a reply the front end can't render) */
const answer = (page: Page, cmd: string, value: unknown) =>
  page.evaluate(([cmd, value]) => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = (c: string, args: unknown) => (c === cmd ? Promise.resolve(value) : orig(c, args));
  }, [cmd, value] as const);
const logged = (page: Page) => page.evaluate(() => ((window as any).__mockLogged ?? []) as { source: string; message: string; stack?: string; componentStack?: string }[]);

test("a tab that fails to render shows its error; the explorer and the other tabs keep working, and Reload Tab brings it back", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(page.getByRole("tab", { name: /users/ })).toBeVisible();
  await openConsole(page);
  await typeSql(page, "SELECT 1");
  await answer(page, "run_query", { columns: [{ name: { not: "a name" }, typeName: "text" }], rows: [[{ not: "a cell" }]], hasResultSet: true, truncated: false });
  await runAll(page);
  const fallback = work(page).getByRole("alert");
  await expect(fallback).toContainText("This tab hit an error");
  await expect(fallback.getByRole("button", { name: "Reload Tab" })).toBeVisible();
  await expect(fallback.getByRole("button", { name: "Copy Details" })).toBeVisible();
  // Logged with its stacks, never the statement
  await expect.poll(async () => (await logged(page)).map((e) => e.source)).toContain("boundary");
  const entry = (await logged(page)).find((e) => e.source === "boundary")!;
  expect(entry.message).toMatch(/^tab: \w*Error: /);
  expect(entry.componentStack).toContain("ConsolePane");
  expect(JSON.stringify(entry)).not.toContain("SELECT 1");
  // The rest of the window works: the explorer, the other tab
  await expect(treeNode(page, "[DEV] shop_app")).toBeVisible();
  await page.getByRole("tab", { name: /users/ }).click();
  await expect(work(page).locator(".grid-scroll")).toBeVisible();
  await expect(work(page).getByRole("alert")).toHaveCount(0);
  // Reload Tab: the console again, its text kept
  await page.getByRole("tab", { name: /console/i }).first().click();
  await work(page).getByRole("button", { name: "Reload Tab" }).click();
  await expect(work(page).getByRole("alert")).toHaveCount(0);
  await expect(work(page).locator(".monaco-editor .view-lines")).toContainText("SELECT 1");
});

test("an error outside the tabs shows the window's fallback with Reload Window, which reloads the app", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "SELECT 1");
  // Every run reads the data sources again: a list the app can't render
  await answer(page, "list_connections", null);
  await runAll(page);
  const fallback = page.locator(".error-fallback");
  await expect(fallback).toContainText("Qurry hit an error");
  await expect(treeNode(page, "[DEV] shop_app")).toHaveCount(0);
  await expect.poll(async () => (await logged(page)).some((e) => e.source === "boundary" && e.message.startsWith("window: "))).toBe(true);
  await fallback.getByRole("button", { name: "Reload Window" }).click();
  await expect(treeNode(page, "[DEV] shop_app")).toBeVisible();
});

test("window errors and unhandled rejections reach the log, and the status bar still shows them", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => {
    setTimeout(() => {
      throw new Error("boom from a timer");
    });
    Promise.reject(new Error("rejected boom"));
  });
  await expect.poll(async () => (await logged(page)).map((e) => `${e.source}: ${e.message}`)).toEqual(expect.arrayContaining(["error: Error: boom from a timer", "unhandledrejection: Error: rejected boom"]));
  await expect(page.locator(".statusbar .sb-text")).toContainText("Error:");
});
