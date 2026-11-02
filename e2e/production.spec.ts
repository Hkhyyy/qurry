import { dialog, expect, openApp, openConsole, runAll, status, test, typeSql } from "./app";

// [PROD] shop_app has options.confirmChanges (mock preview_impact counts UPDATE/DELETE: 3 rows with WHERE, 500 without; INSERT and DDL can't be counted)
test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openConsole(page, "[PROD] shop_app");
});

test("changes list each statement with its row count; INSERT can't be counted, DDL shows schema change", async ({ page }) => {
  await typeSql(page, "update users set user_name = 'x' where id = 1;\ninsert into users (user_name) values ('y');\nalter table users add column x int;\nselect 1;");
  await runAll(page);
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [PROD] shop_app");
  await expect(dialog(page).locator(".ui-confirm-message")).toHaveText("These statements change data. Run them?");
  const rows = dialog(page).locator(".ui-detail-row");
  await expect(rows.locator(".ui-detail-text")).toHaveText(["update users set user_name = 'x' where id = 1", "insert into users (user_name) values ('y')", "alter table users add column x int"]);
  await expect(rows.locator(".ui-detail-value")).toHaveText(["3 rows", "can't count", "schema change"]);
  await expect(rows.locator(".ui-detail-value")).toHaveClass([/tone-warn/, /tone-danger/, /tone-danger/]);
  await expect(dialog(page).getByRole("button", { name: "Run" })).toHaveClass(/ui-btn-danger/);
  await dialog(page).getByRole("button", { name: "Run" }).click();
  await expect(status(page)).toHaveText("4 statements executed");
});

test("Cancel runs nothing", async ({ page }) => {
  await typeSql(page, "update users set user_name = 'x' where id = 1");
  await runAll(page);
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(status(page)).toHaveText("Ready");
});

test("dangerous statement asks first, then the Production confirmation follows", async ({ page }) => {
  await typeSql(page, "delete from users");
  await runAll(page);
  await expect(dialog(page)).toContainText("[PROD] shop_app: dangerous statement");
  await expect(dialog(page)).toContainText("DELETE without WHERE changes every row. Run it anyway?");
  await expect(dialog(page).locator("code")).toHaveText("delete from users");
  await dialog(page).getByRole("button", { name: "Run Anyway" }).click();
  await expect(dialog(page).locator(".ui-detail-value")).toHaveText("500 rows");
  await dialog(page).getByRole("button", { name: "Run" }).click();
  await expect(status(page)).toHaveText("1 statement executed");
});

test("reads run without a confirmation", async ({ page }) => {
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  await expect(status(page)).toHaveText("1 statement executed");
  await expect(dialog(page)).toHaveCount(0);
});

// D1 (0.3.0): the GUI reads statements with the core guard, like the TUI. A write keyword only inside a string doesn't make a read a
// write, and a parenthesized SELECT is a read; a quoted name like "update" still counts as a write
test("a read that only mentions a write keyword in a string runs without a confirmation", async ({ page }) => {
  await typeSql(page, "select 'delete' as action");
  await page.keyboard.press("Meta+Enter");
  await expect(status(page)).toHaveText("1 statement executed");
  await expect(dialog(page)).toHaveCount(0);
});

test("only the real writes are listed: string keywords and parenthesized selects are reads, a quoted \"update\" name is not", async ({ page }) => {
  await typeSql(page, "select 'delete' as action;\n(select 1) union (select 2);\nselect \"update\" from users;\nupdate users set user_name = 'x' where id = 1;");
  await runAll(page);
  const rows = dialog(page).locator(".ui-detail-row");
  await expect(rows.locator(".ui-detail-text")).toHaveText(['select "update" from users', "update users set user_name = 'x' where id = 1"]);
  await dialog(page).getByRole("button", { name: "Run" }).click();
  await expect(status(page)).toHaveText("4 statements executed");
});

// [MySQL] shop also has Production on: there # starts a comment, so the statement is a TRUNCATE
test("MySQL: a TRUNCATE after a # comment shows schema change", async ({ page }) => {
  await openConsole(page, "[MySQL] shop");
  await typeSql(page, "# wipe\ntruncate t");
  await runAll(page);
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [MySQL] shop");
  await expect(dialog(page).locator(".ui-detail-value")).toHaveText("schema change");
});

// A backslash in a string: under NO_BACKSLASH_ESCAPES the server ends the string after x\ and runs the DELETE
test("MySQL: a WITH whose DELETE hides behind a backslash in a string asks first", async ({ page }) => {
  await openConsole(page, "[MySQL] shop");
  await typeSql(page, "with a as (select 'x\\') delete from t -- ') select 1");
  await page.keyboard.press("Meta+Enter");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [MySQL] shop");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText("with a as (select 'x\\') delete from t -- ') select 1");
});

// A selection run as one statement goes to the server as one text, which runs all of it: a write after a read asks
for (const [conn, sql] of [
  ["[PROD] shop_app", "select 1; do $$ begin delete from orders; end $$"],
  ["[MySQL] shop", "select 1; prepare s from 'delete from orders'; execute s"],
] as const) {
  test(`a selection run as one statement asks when a write follows a read: ${sql}`, async ({ page }) => {
    await page.evaluate(() => localStorage.setItem("dbtool.execSettings", JSON.stringify({ inside: "ask", outside: "nothing", selection: "single" })));
    await openConsole(page, conn);
    await typeSql(page, sql);
    await runAll(page);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText(`Production: ${conn}`);
    await expect(dialog(page).locator(".ui-detail-text")).toHaveText(sql);
    await dialog(page).getByRole("button", { name: "Cancel" }).click();
    await expect(status(page)).toHaveText("Ready");
  });
}
