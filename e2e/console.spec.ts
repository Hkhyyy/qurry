import { dialog, editor, expect, openApp, openConsole, resultTabs, runAll, status, suggestions, test, typeSql, work } from "./app";

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

  test("MySQL console on the data source's default database inserts its tables bare, as the TUI does", async ({ page }) => {
    await openConsole(page, "[MySQL] shop");
    await typeSql(page, "select * from ord");
    await expect(suggestions(page).first()).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(editor(page).locator(".view-line").first()).toHaveText(/^select\s\*\sfrom\sorders$/);
  });
});

test("run one statement → one result tab", async ({ page }) => {
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  await expect(work(page).locator(".exec-highlight")).not.toHaveCount(0);
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

// The backend's message for a Commit cut off by a lost connection (app.rs: `{e} — {COMMIT_UNKNOWN_ERR}`), and one that just failed
const commitLost =
  "Connection reset by peer (os error 54) — connection lost while committing, so it is unknown whether the changes were committed. Check the data before you run them again (the next run reconnects).";
for (const [msg, title] of [
  [commitLost, "Commit outcome unknown"],
  ["Session closed", "Commit failed"],
]) {
  test(`a failed Commit says so in a notice, the status bar and Output: ${title}`, async ({ page }) => {
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
    await expect(dialog(page)).toContainText(title);
    await expect(dialog(page)).toContainText(msg);
    await dialog(page).getByRole("button", { name: "OK" }).click();
    await expect(status(page)).toContainText(msg);
    await expect(resultTabs(page).first()).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(".work-layer:visible .output-log")).toContainText(msg);
  });
}

// A rollback undoes a DELETE but not a sequence step: Explain Analyse asks first and doesn't claim nothing changed
test("Explain Analyse asks before a write a rollback can't undo, and says what stays", async ({ page }) => {
  await typeSql(page, "select nextval('orders_id_seq')");
  await page.locator(".work-layer:visible .console-toolbar").getByText("Explain").click();
  await page.getByRole("menuitem", { name: "Explain Analyse" }).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("[DEV] shop_app: Explain Analyse");
  await expect(dialog(page)).toContainText("a rollback can't undo what it does outside the transaction: sequence values");
  await dialog(page).getByRole("button", { name: "Run Anyway" }).click();
  await resultTabs(page).first().click();
  const output = page.locator(".work-layer:visible .output-log");
  await expect(output).toContainText("rolled it back, but what it did outside the transaction stays (sequence values");
  await expect(output).not.toContainText("nothing was changed");
});

test("MySQL: `--` greys out as a comment only where the server reads one (`select 1--1` runs `1 - -1`)", async ({ page }) => {
  await openConsole(page, "[MySQL] shop");
  const italic = () =>
    editor(page).locator(".view-line span span").evaluateAll((els) => els.filter((e) => getComputedStyle(e).fontStyle === "italic").map((e) => e.textContent).join("").replace(/\u00a0/g, " "));
  await typeSql(page, "select 1--1; delete from orders");
  await expect(editor(page).locator(".view-line")).toContainText("delete from orders");
  expect(await italic()).toBe("");
  await typeSql(page, "select 1 -- note");
  await expect.poll(italic).toBe("-- note");
});
