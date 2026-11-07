import type { Page } from "@playwright/test";
import { dialog, expect, openApp, openConsole, runAll, status, test, treeNode, typeSql, work } from "./app";

// Console results that come from one table with its key are edited like a table tab (#86), a join per table (#102). The mock's
// backend gives `select * from users` an edit target, the voc / users join per-table targets and other joins a reason
// (lib/mock.ts consoleEdit, vocJoin)
const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
const info = (page: Page) => work(page).locator(".tb-info");
const button = (page: Page, name: RegExp) => work(page).getByRole("button", { name });
const txLabel = (page: Page) => work(page).getByTitle("Transaction Mode", { exact: true });
const pendingBadge = (page: Page) => work(page).locator(".tx-pending");

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
const submitted = (calls: () => Promise<[string, any][]>) => async () => (await calls()).find(([c]) => c === "submit_edits")?.[1];

async function editUser2(page: Page, conn?: string) {
  await openApp(page);
  await openConsole(page, conn);
  await typeSql(page, "select * from users");
  await runAll(page);
  await cell(page, "user_2").click();
  await page.keyboard.press("Meta+Alt+KeyN");
  await expect(info(page)).toContainText("1 pending");
}

/** Edit a grid cell: double-click it, type, Enter */
async function editCell(page: Page, at: ReturnType<typeof cell>, text: string) {
  await at.dblclick();
  await page.keyboard.press("Meta+A");
  await page.keyboard.type(text);
  await page.keyboard.press("Enter");
}

test("select * from users: an edit is submitted as the table tab submits it, then the result runs again", async ({ page }) => {
  await editUser2(page);
  const calls = await spy(page);
  await button(page, /^Submit/).click();
  await expect.poll(submitted(calls)).toEqual({ id: "dev", tabId: expect.any(String), statements: ["UPDATE public.users SET user_name = NULL WHERE id = '2';"] });
  await expect.poll(async () => (await calls()).filter(([c]) => c === "run_query").map(([, a]) => a.sql)).toContain("select * from users");
  await expect(info(page)).not.toContainText("pending");
});

test("a join without a table's key is read-only and its edit buttons say why", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select u.user_name, o.total from users u join orders o on o.user_id = u.id");
  await runAll(page);
  const add = button(page, /^Add Row/);
  await expect(add).toBeDisabled();
  await expect(add).toHaveAttribute("aria-description", "Read-only: the result doesn't include the primary key of public.orders (id).");
});

test("a join edits per table: each table's row by its own key, read-only columns and the outer side say why, no rows added", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select vo.*, pu.idx, pu.role, pu.status, upper(pu.role) as shout from crm.voc vo left join users pu on vo.user_idx = pu.idx");
  await runAll(page);
  // Rows can't be added or deleted; the computed column says why on its header
  await expect(button(page, /^Add Row/)).toBeDisabled();
  await expect(button(page, /^Add Row/)).toHaveAttribute("aria-description", "Rows can be added or deleted only in a result from one table. In a join, edit the cells of a table whose primary key is in the result.");
  await expect(work(page).locator(".cell.head.read-only")).toHaveText([/shout/]);
  await expect(work(page).locator(".cell.head.read-only")).toHaveAttribute("aria-description", "Read-only: shout is computed, not a column of the table.");
  // voc 2 has no user: that side's cells say why on hover, and an edit there does nothing
  const noRow = work(page).locator('[role=gridcell][title="Read-only: no public.users row matches this row (outer join)."]');
  await expect(noRow).toHaveCount(3);
  await noRow.first().dblclick();
  await expect(work(page).locator(".cell-input")).toHaveCount(0);
  await expect(info(page)).not.toContainText("pending");
  // users.role of voc 1 and voc.status of voc 2 (two idx and status columns: each goes to its own table)
  await editCell(page, cell(page, "member"), "admin");
  await editCell(page, cell(page, "new"), "seen");
  await expect(info(page)).toContainText("2 pending");
  const calls = await spy(page);
  await button(page, /^Submit/).click();
  await expect.poll(submitted(calls)).toEqual({
    id: "dev",
    tabId: expect.any(String),
    statements: ["UPDATE public.users SET role = 'admin' WHERE idx = '10';", "UPDATE crm.voc SET status = 'seen' WHERE idx = '2';"],
  });
});

test("on a Production data source Submit asks first, listing the statements; Cancel saves nothing", async ({ page }) => {
  await editUser2(page, "[PROD] shop_app");
  const calls = await spy(page);
  await button(page, /^Submit/).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [PROD] shop_app");
  // A Production console is in Manual: the edits run in its transaction
  await expect(dialog(page)).toContainText("Run 1 change in this console's transaction? They stay pending until you Commit or Roll back.");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["UPDATE public.users SET user_name = NULL WHERE id = '2';"]);
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(info(page)).toContainText("1 pending");
  expect((await calls()).map(([c]) => c)).not.toContain("submit_edits");
  await button(page, /^Submit/).click();
  await dialog(page).getByRole("button", { name: "Run" }).click();
  await expect.poll(async () => (await calls()).map(([c]) => c)).toContain("submit_edits");
  await expect(info(page)).not.toContainText("pending");
  await expect(pendingBadge(page)).toBeVisible();
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

test("in Manual mode Submit runs in the console's transaction and stays pending until Commit", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await txLabel(page).click();
  await page.getByRole("menuitem", { name: /Manual/ }).click();
  await work(page).locator(".monaco-editor").first().click();
  await typeSql(page, "select * from users");
  await runAll(page);
  await cell(page, "user_2").click();
  await page.keyboard.press("Meta+Alt+KeyN");
  const calls = await spy(page);
  await button(page, /^Submit/).click();
  await expect.poll(submitted(calls)).toMatchObject({ id: "dev", tabId: expect.any(String) });
  await expect(pendingBadge(page)).toHaveText("● Transaction pending");
  const commit = work(page).locator(".console-toolbar").getByRole("button", { name: "Commit" });
  await expect(commit).toBeEnabled();
  await commit.click();
  await expect(pendingBadge(page)).toHaveCount(0);
});

test("a Production console starts in Manual and says so once; the pending badge; closing asks; the option off starts in Auto", async ({ page }) => {
  await openApp(page);
  await openConsole(page, "[PROD] shop_app");
  await expect(txLabel(page)).toHaveText("Tx: Manual ▾");
  const said = work(page).locator(".output-log .ol", { hasText: "Production: this console starts in Tx: Manual" });
  await expect(said).toHaveCount(1);
  await typeSql(page, "update users set user_name = 'x' where id = 1");
  await runAll(page);
  await dialog(page).getByRole("button", { name: "Run" }).click();
  await expect(pendingBadge(page)).toHaveText("● Transaction pending");
  // Closing the tab asks first; Cancel keeps it
  await page.keyboard.press("Meta+W");
  await expect(dialog(page)).toContainText("uncommitted changes. Roll them back and close?");
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(pendingBadge(page)).toBeVisible();
  // A second console of the source doesn't say it again
  await openConsole(page, "[PROD] shop_app");
  await expect(txLabel(page)).toHaveText("Tx: Manual ▾");
  await expect(work(page).locator(".output-log .ol", { hasText: "starts in Tx: Manual" })).toHaveCount(0);
  // The option off (Data Source Properties › Options): new consoles start in Auto
  await treeNode(page, "[PROD] shop_app").click();
  await page.keyboard.press("Meta+;");
  await dialog(page).getByRole("tab", { name: "Options" }).click();
  const option = dialog(page).getByRole("checkbox", { name: /start consoles in Manual/ });
  await expect(option).toBeChecked();
  await option.click();
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await openConsole(page, "[PROD] shop_app");
  await expect(txLabel(page)).toHaveText("Tx: Auto ▾");
});
