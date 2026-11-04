import { dialog, editor, expect, openApp, openConsole, resultTabs, runAll, spyCancels, status, suggestions, test, treeItem, typeSql, work } from "./app";

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

  test("in a JOIN's ON condition `=` lists columns, not values; the WHERE after it lists values", async ({ page }) => {
    await typeSql(page, "select * from orders o left join users u on o.amount = ");
    await page.waitForTimeout(300);
    await expect(suggestions(page)).toHaveCount(0);
    await page.keyboard.press("Control+Space");
    await expect(suggestions(page).first()).toBeVisible();
    const labels = await suggestions(page).locator(".label-name").allInnerTexts();
    expect(labels).toEqual(expect.arrayContaining(["amount", "user_name"]));
    expect(labels).not.toContain("1000");
    await page.keyboard.press("Escape");
    await page.keyboard.type("u.id where o.amount = ");
    await expect(suggestions(page)).toHaveText([/^1000\s*7 rows$/, /^2500\s*3 rows$/]);
  });

  test("MySQL console on the data source's default database inserts its tables bare, as the TUI does", async ({ page }) => {
    await openConsole(page, "[MySQL] shop");
    await typeSql(page, "select * from ord");
    await expect(suggestions(page).first()).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(editor(page).locator(".view-line").first()).toHaveText(/^select\s\*\sfrom\sorders$/);
  });
});

test("the database select lists its databases once the console is shown: its first click opens the whole list", async ({ page }) => {
  // The mock lists late, as a server does: a list asked for only on the first click would open with the current database alone
  await page.evaluate(() => ((window as any).__mockDbListDelay = 200));
  await openConsole(page, "[MySQL] shop");
  const db = work(page).getByTitle("Database", { exact: true });
  await expect(db).toHaveValue("shop");
  await expect(db.locator("option")).toHaveText(["shop", "information_schema", "mysql", "sys"]);
  await db.click();
  await expect(db.locator("option")).toHaveText(["shop", "information_schema", "mysql", "sys"]);
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

test("a run shows in the status bar (bar · Executing… · elapsed) for the focused tab only; ■ cancels it", async ({ page }) => {
  const bar = page.locator(".statusbar .running-bar");
  const stop = page.locator(".statusbar").getByTitle("Cancel Running Statements");
  await typeSql(page, "select pg_sleep(3)");
  await page.keyboard.press("Meta+Enter");
  await expect(bar).toContainText(/^Executing… \d+\.\d s$/);
  await expect(work(page).locator(".running-bar")).toHaveCount(0);
  await expect(stop).toBeEnabled();
  // Another tab that isn't running: its status text and a disabled ■
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await expect(bar).toHaveCount(0);
  await expect(status(page)).toBeVisible();
  await expect(stop).toBeDisabled();
  // Back on the console: its indicator again, the time going on from the run's start
  await page.locator("[role=tab].tab", { hasText: "console" }).first().click();
  await expect(bar).toContainText(/^Executing… (?:[1-9]|\d{2,})\.\d s$/);
  const cancels = await spyCancels(page);
  await stop.click();
  await expect.poll(cancels).toBe(1);
});

test("a console result whose load-more fails keeps the reason in its grid (#80)", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFetchMoreError = "Session closed"));
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
  await work(page).locator(".grid-scroll").first().evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".grid-more-error")).toHaveText("Couldn't load more rows: Session closed");
  await expect(work(page).getByRole("button", { name: "Run again" })).toBeVisible();
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
});

test("Output follows new lines at the bottom, keeps the position when scrolled up, and Clear All follows again (#92)", async ({ page }) => {
  // Five runs of 30 statements and their Output: a busy runner takes most of the default 30 s for that alone
  test.slow();
  // Inserted at once: typing 30 lines key by key takes much of the test's time on a busy runner
  await editor(page).click();
  await page.keyboard.press("Meta+A");
  await page.keyboard.insertText(Array.from({ length: 30 }, (_, i) => `update orders set status = 'ready' where id = ${i};`).join("\n"));
  const output = work(page).locator(".output-log");
  const lines = output.locator(".ol");
  const fromBottom = () => output.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight);
  // Scrolls as the wheel does (a scroll event the pane reads), without waiting on wheel delivery, which a busy runner can drop
  const scrollBy = (dy: number) => output.evaluate((el, d) => el.scrollBy(0, d), dy);
  await runAll(page);
  await expect(status(page)).toHaveText("30 statements executed");
  const perRun = await lines.count();
  const run = async () => {
    const before = await lines.count();
    await editor(page).click();
    await runAll(page);
    await expect.poll(() => lines.count()).toBe(before + perRun);
  };
  expect(await output.evaluate((el) => el.scrollHeight > el.clientHeight * 2)).toBe(true);
  expect(await fromBottom()).toBeLessThanOrEqual(4);

  // Scrolled up: more lines arrive below, the view stays put
  await scrollBy(-200);
  await expect.poll(fromBottom).toBeGreaterThan(100);
  const top = await output.evaluate((el) => el.scrollTop);
  await run();
  expect(await output.evaluate((el) => el.scrollTop)).toBe(top);

  // Back at the bottom, then scrolled up while lines arrive before that scroll's event does (it comes a frame later, later still on a
  // busy machine): the view stays put, the lines don't pull it back down
  await scrollBy(100_000);
  await expect.poll(fromBottom).toBeLessThanOrEqual(4);
  await output.evaluate((el) => {
    const w = window as any;
    w.__holdScroll = (e: Event) => e.target === el && e.stopImmediatePropagation();
    document.addEventListener("scroll", w.__holdScroll, true);
    el.scrollBy(0, -200);
  });
  const held = await output.evaluate((el) => el.scrollTop);
  await run();
  expect(await output.evaluate((el) => el.scrollTop)).toBe(held);
  await page.evaluate(() => document.removeEventListener("scroll", (window as any).__holdScroll, true));

  // Back at the bottom: follows again
  await scrollBy(100_000);
  await expect.poll(fromBottom).toBeLessThanOrEqual(4);
  await run();
  expect(await fromBottom()).toBeLessThanOrEqual(4);

  // Scrolled up, then Clear All: the next run follows
  await scrollBy(-200);
  await expect.poll(fromBottom).toBeGreaterThan(100);
  await work(page).getByTitle(/^Clear All/).click();
  await expect(lines).toHaveCount(0);
  await run();
  expect(await output.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  expect(await fromBottom()).toBeLessThanOrEqual(4);

  // The pane gets shorter after the run (window resized): the newest line stays in view
  const height = await output.evaluate((el) => el.clientHeight);
  const vp = page.viewportSize()!;
  await page.setViewportSize({ width: vp.width, height: vp.height - 150 });
  await expect.poll(() => output.evaluate((el) => el.clientHeight)).toBeLessThan(height);
  await expect.poll(fromBottom).toBeLessThanOrEqual(4);
});
