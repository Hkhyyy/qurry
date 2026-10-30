import { dialog, editor, expect, openApp, openConsole, resultTabs, runAll, status, suggestions, test, typeSql } from "./app";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openConsole(page);
});

test.describe("completion", () => {
  test("after `select * from ` stays closed on the space; ⌃Space lists tables, schemas, keywords", async ({ page }) => {
    await typeSql(page, "select * from ");
    await page.waitForTimeout(300);
    await expect(suggestions(page)).toHaveCount(0);
    await page.keyboard.press("Control+Space");
    await expect(suggestions(page).first()).toBeVisible();
    const labels = await suggestions(page).locator(".label-name").allInnerTexts();
    expect(labels.slice(0, 4)).toEqual(["orders", "users", "public", "all"]);
  });

  test("after `alias.` lists that table's columns", async ({ page }) => {
    await typeSql(page, "select * from users u where u.");
    await expect(suggestions(page).first()).toBeVisible();
    await expect(suggestions(page)).toHaveText([/^id\s*bigint$/, /^user_name\s*varchar$/]);
  });

  test("after `status = '` lists the column's frequent values", async ({ page }) => {
    await typeSql(page, "select * from orders where status = '");
    await expect(suggestions(page).first()).toBeVisible();
    await expect(suggestions(page)).toHaveText([/^ready\s*20 rows$/, /^paid\s*10 rows$/, /^it's late\s*1 row$/]);
  });
});

test("run one statement → one result tab", async ({ page }) => {
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  await expect(resultTabs(page)).toHaveText(["Output", /^users/]);
  await expect(resultTabs(page).nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(status(page)).toHaveText("1 statement executed");
});

test("run several → one result tab per statement", async ({ page }) => {
  await typeSql(page, "select 1;\nselect * from users;\n-- by status\nselect * from orders;");
  await runAll(page);
  await expect(resultTabs(page)).toHaveText(["Output", /^Result 1/, /^users/, /^by status/]);
  await expect(status(page)).toHaveText("3 statements executed");
  await resultTabs(page).first().click();
  await expect(page.locator(".work-layer:visible .output-log")).toContainText("[DEV] shop_app> select * from users[");
});

test("Explain Plan → plan tab with the node tree", async ({ page }) => {
  await typeSql(page, "select * from users u join contracts c on u.contract_id = c.id");
  await page.locator(".work-layer:visible .console-toolbar").getByText("Explain").click();
  await page.getByRole("menuitem", { name: "Explain Plan" }).click();
  await expect(resultTabs(page)).toHaveText(["Output", /^Plan: users/]);
  const plan = page.locator(".work-layer:visible .plan-view");
  await expect(plan).toBeVisible();
  // The plan tree comes from analyze_plan (the mock's fixed Gather Merge → Sort → Seq Scan plan)
  await expect(plan).toContainText("2 nodes worth a look");
  for (const op of ["Gather Merge", "Sort", "Seq Scan"]) await expect(plan).toContainText(op);
  await expect(editor(page)).toBeVisible();
});

test("a failed Commit says so in a notice, the status bar and Output", async ({ page }) => {
  const msg = "connection lost while committing, so it is unknown whether the changes were committed. Check the data before you run them again";
  await page.locator(".work-layer:visible .console-toolbar").getByText("Tx: Auto").click();
  await page.getByRole("menuitem", { name: /Manual/ }).click();
  await editor(page).click();
  await typeSql(page, "update users set user_name = 'a' where id = 1");
  await runAll(page);
  const commit = page.locator(".work-layer:visible .console-toolbar").getByRole("button", { name: "Commit" });
  await expect(commit).toBeEnabled();
  // Look at something else first: the failure brings Output back
  await typeSql(page, "select 1");
  await runAll(page);
  await expect(resultTabs(page)).toHaveText(["Output", /^Result/]);
  await expect(resultTabs(page).nth(1)).toHaveAttribute("aria-selected", "true");
  await page.evaluate((m) => ((globalThis as any).__mockCommitError = m), msg);
  await commit.click();
  await expect(dialog(page)).toContainText("Commit failed");
  await expect(dialog(page)).toContainText(msg);
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(status(page)).toContainText("unknown whether the changes were committed");
  await expect(resultTabs(page).first()).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".work-layer:visible .output-log")).toContainText(msg);
});
