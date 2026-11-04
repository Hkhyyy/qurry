import type { Page } from "@playwright/test";
import { dialog, expect, openApp, openConsole, runAll, status, test, typeSql, work } from "./app";

// Console results that come from one table with its key are edited like a table tab (#86). The mock's backend gives
// `select * from users` an edit target and a join a reason (lib/mock.ts consoleEdit)
const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
const info = (page: Page) => work(page).locator(".tb-info");
const button = (page: Page, name: RegExp) => work(page).getByRole("button", { name });

/** Record the backend calls with their arguments from now on */
async function spy(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__calls = [];
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: unknown) => (w.__calls.push([cmd, args]), orig(cmd, args));
  });
  return () => page.evaluate(() => (window as any).__calls as [string, any][]);
}

async function editUser2(page: Page, conn?: string) {
  await openApp(page);
  await openConsole(page, conn);
  await typeSql(page, "select * from users");
  await runAll(page);
  await cell(page, "user_2").click();
  await page.keyboard.press("Meta+Alt+KeyN");
  await expect(info(page)).toContainText("1 pending");
}

test("select * from users: an edit is submitted as the table tab submits it, then the result runs again", async ({ page }) => {
  await editUser2(page);
  const calls = await spy(page);
  await button(page, /^Submit/).click();
  await expect.poll(async () => (await calls()).find(([c]) => c === "execute_batch")?.[1]).toEqual({ id: "dev", statements: ["UPDATE public.users SET user_name = NULL WHERE id = '2';"], exactOne: true });
  await expect.poll(async () => (await calls()).filter(([c]) => c === "run_query").map(([, a]) => a.sql)).toContain("select * from users");
  await expect(info(page)).not.toContainText("pending");
});

test("a join is read-only and its edit buttons say why", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select u.id, u.user_name from users u join orders o on o.user_id = u.id");
  await runAll(page);
  const add = button(page, /^Add Row/);
  await expect(add).toBeDisabled();
  await expect(add).toHaveAttribute("aria-description", "Read-only: the columns come from more than one table.");
});

test("on a Production data source Submit asks first, listing the statements; Cancel saves nothing", async ({ page }) => {
  await editUser2(page, "[PROD] shop_app");
  const calls = await spy(page);
  await button(page, /^Submit/).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [PROD] shop_app");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["UPDATE public.users SET user_name = NULL WHERE id = '2';"]);
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(info(page)).toContainText("1 pending");
  expect((await calls()).map(([c]) => c)).not.toContain("execute_batch");
  await button(page, /^Submit/).click();
  await dialog(page).getByRole("button", { name: "Save" }).click();
  await expect.poll(async () => (await calls()).map(([c]) => c)).toContain("execute_batch");
  await expect(info(page)).not.toContainText("pending");
});

test("running again with pending edits asks first; Cancel keeps them, Discard runs", async ({ page }) => {
  await editUser2(page);
  await work(page).locator(".monaco-editor").first().click();
  await runAll(page);
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Pending changes");
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(info(page)).toContainText("1 pending");
  await work(page).locator(".monaco-editor").first().click();
  await runAll(page);
  await dialog(page).getByRole("button", { name: "Discard" }).click();
  await expect(info(page)).not.toContainText("pending");
  await expect(status(page)).not.toHaveText("Executing…");
});

test("in Manual mode a console result is read-only, with the reason", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await work(page).getByTitle("Transaction Mode", { exact: true }).click();
  await page.getByRole("menuitem", { name: /Manual/ }).click();
  await work(page).locator(".monaco-editor").first().click();
  await typeSql(page, "select * from users");
  await runAll(page);
  const add = button(page, /^Add Row/);
  await expect(add).toBeDisabled();
  await expect(add).toHaveAttribute("aria-description", /^Read-only in a transaction/);
});
