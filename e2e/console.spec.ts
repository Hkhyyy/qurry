import { dialog, editor, expect, openApp, openConsole, resultTabs, runAll, spyCancels, status, suggestions, test, treeItem, typeSql, work } from "./app";
import type { Page } from "@playwright/test";

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

test("statement words are keywords in both grammars (pgsql knew only reserved words: UPDATE, SET, INSERT … were plain names)", async ({ page }) => {
  // The words drawn in WHERE's color (a keyword's)
  const keywords = () =>
    editor(page).locator(".view-line span span").evaluateAll((els) => {
      const where = els.find((e) => e.textContent === "WHERE");
      return where ? els.filter((e) => getComputedStyle(e).color === getComputedStyle(where).color).map((e) => e.textContent!.trim()) : [];
    });
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  for (const conn of ["[DEV] shop_app", "[MySQL] shop"]) {
    if (conn !== "[DEV] shop_app") await openConsole(page, conn);
    await typeSql(page, "UPDATE users SET a = 1 WHERE id = 9; BEGIN; COMMIT");
    await page.keyboard.press("Escape");
    await expect.poll(keywords).toEqual(["UPDATE", "SET", "WHERE", "BEGIN", "COMMIT"]);
  }
});

test("word operators are keywords, symbol operators keep their color: `a = 1 AND b IN (2)` (#136)", async ({ page }) => {
  const colors = () =>
    editor(page).locator(".view-line span span").evaluateAll((els) => Object.fromEntries(els.map((e) => [e.textContent!.trim(), getComputedStyle(e).color])));
  await typeSql(page, "SELECT 1 WHERE a = 1 AND b IN (2) OR c IS NOT NULL");
  await page.keyboard.press("Escape");
  await expect.poll(async () => (await colors())["AND"]).toBeTruthy();
  const c = await colors();
  for (const w of ["AND", "IN", "OR", "IS", "NOT", "NULL"]) expect(c[w], w).toBe(c["WHERE"]);
  // `=` (with the spaces around it, one piece when they are the same color) is not drawn as a keyword
  const eq = Object.entries(c).find(([t]) => t.includes("="));
  expect(eq?.[1]).toBeTruthy();
  expect(eq![1]).not.toBe(c["WHERE"]);
});

test("a run shows in the status bar (bar · Executing… · elapsed) for the focused tab only; hovering it shows ✕, which cancels it (#107)", async ({ page }) => {
  const bar = page.locator(".statusbar .running-bar");
  const x = bar.getByRole("button", { name: "Cancel (⌘F2)" });
  // Long enough to outlast the tab switches below on a slow runner; the ✕ ends it
  await typeSql(page, "select pg_sleep(30)");
  await page.keyboard.press("Meta+Enter");
  await expect(bar).toContainText(/^Executing… \d+\.\d s/);
  await expect(work(page).locator(".running-bar")).toHaveCount(0);
  // No ▶ ■ buttons in the status bar any more; the ✕ only on hover (its room kept, so nothing moves)
  await expect(page.locator(".statusbar").getByRole("button", { name: /^Execute/ })).toHaveCount(0);
  await expect(x).toBeHidden();
  const before = (await bar.boundingBox())!;
  await bar.hover();
  await expect(x).toBeVisible();
  expect((await bar.boundingBox())!.width).toBe(before.width);
  // Another tab that isn't running: its status text, no indicator
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await expect(bar).toHaveCount(0);
  await expect(status(page)).toBeVisible();
  // Back on the console: its indicator again, the time going on from the run's start
  await page.locator("[role=tab].tab", { hasText: "console" }).first().click();
  await expect(bar).toContainText(/^Executing… (?:[1-9]|\d{2,})\.\d s/);
  const cancels = await spyCancels(page);
  await bar.hover();
  await x.click();
  await expect.poll(cancels).toBe(1);
});

test("the console toolbar's ▶ is ■ in the same spot while a statement runs; ■ cancels, and it is ▶ again after (#107)", async ({ page }) => {
  const toolbar = work(page).locator(".console-toolbar");
  const run = toolbar.getByRole("button", { name: "Execute (⌘↩)" });
  const stop = toolbar.getByRole("button", { name: "Cancel (⌘F2)" });
  await expect(run).toBeEnabled();
  await expect(stop).toHaveCount(0);
  const at = (await run.boundingBox())!;
  await typeSql(page, "select pg_sleep(2)");
  await run.click();
  await expect(stop).toBeVisible();
  await expect(run).toHaveCount(0);
  const box = (await stop.boundingBox())!;
  expect([box.x, box.y, box.width]).toEqual([at.x, at.y, at.width]);
  // One button: no second ■ further along the toolbar
  await expect(toolbar.locator(".ui-icon-btn.tone-danger")).toHaveCount(1);
  const cancels = await spyCancels(page);
  await stop.click();
  await expect.poll(cancels).toBe(1);
  // The mock lets the statement finish: ▶ again, in the same spot
  await expect(run).toBeVisible({ timeout: 5000 });
  await expect(stop).toHaveCount(0);
});

test("a console result's load-more is the grid's ■, not the console's: ▶ stays (#107)", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFetchMoreDelay = 1500));
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
  const grid = work(page).locator(".grid-wrap");
  await grid.locator(".grid-scroll").evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(page.locator(".statusbar .running-bar")).toContainText("Loading more rows");
  await expect(work(page).locator(".console-toolbar").getByRole("button", { name: "Execute (⌘↩)" })).toBeEnabled();
  await expect(grid.getByRole("button", { name: "Cancel (⌘F2)" })).toBeVisible();
  await expect(grid.getByRole("button", { name: /^Reload Page/ })).toHaveCount(0);
  // ■ cancels the fetch in flight: the 500 rows stay, the grid says why no more came, ↻ is back
  await grid.getByRole("button", { name: "Cancel (⌘F2)" }).click();
  await expect(work(page).locator(".grid-more-error")).toHaveText("Couldn't load more rows: ERROR: canceling statement due to user request");
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
  await expect(grid.getByRole("button", { name: "Reload Page (⌘r)" })).toBeVisible();
});

test("the status bar shows Ln, Col only while the console editor has focus, Row, Col for a focused result grid, nothing for a table tab (#107)", async ({ page }) => {
  const pos = page.locator(".statusbar .ui-status-item").filter({ hasText: /^(Ln|Row) / });
  // Inserted at once: an Enter typed after `select *` could pick a completion
  await typeSql(page, "");
  await page.keyboard.insertText("select *\nfrom users");
  await expect(pos).toHaveText("Ln 2, Col 11");
  await page.keyboard.press("Shift+Home");
  await expect(pos).toHaveText("Ln 2, Col 1 (10 chars)");
  await page.keyboard.press("End");
  await page.keyboard.press("Meta+Enter");
  // The result grid focused: its current cell, the editor's position gone
  const grid = work(page).locator(".grid-wrap");
  await grid.getByRole("gridcell", { name: "user_3", exact: true }).click();
  await expect(pos).toHaveText("Row 3, Col 2");
  await page.keyboard.press("ArrowRight");
  await expect(pos).toHaveText("Row 3, Col 3");
  // Back in the editor
  await editor(page).click();
  await expect(pos).toHaveText(/^Ln \d+, Col \d+$/);
  // A table tab (its grid not clicked yet): nothing, not the console's last position
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await expect(pos).toHaveCount(0);
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await expect(pos).toHaveText("Row 1, Col 2");
  // The explorer focused: nothing
  await treeItem(page, "users").first().click();
  await expect(pos).toHaveCount(0);
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

test.describe("Statements popup (⌘↩ with several choices)", () => {
  const chooser = (page: Page) => work(page).locator(".stmt-chooser");
  /** The editor's lines in order (Monaco reuses line elements, and WebKit's innerText drops their line breaks) */
  const text = (page: Page) =>
    editor(page).evaluate((ed) =>
      [...ed.querySelectorAll<HTMLElement>(".view-line")]
        .sort((a, b) => parseFloat(a.style.top) - parseFloat(b.style.top))
        .map((l) => l.textContent!.replace(/\u00a0/g, " "))
        .join("\n"),
    );
  /** The editor lines the preview highlight covers (1-based) */
  const previewLines = (page: Page) =>
    editor(page).evaluate((ed) => {
      const h = parseFloat(getComputedStyle(ed.querySelector(".view-line")!).height);
      return [...ed.querySelectorAll(".view-overlays .stmt-preview")].map((x) => Math.round(parseFloat((x.parentElement as HTMLElement).style.top) / h) + 1).sort();
    });
  const open = async (page: Page) => {
    await page.keyboard.press("Meta+Enter");
    await expect(chooser(page)).toBeVisible();
  };

  test.beforeEach(async ({ page }) => {
    await typeSql(page, "select 1;\nselect 2;\nselect 3;");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Home");
  });

  test("highlights the text each option would run, and nothing once closed", async ({ page }) => {
    await open(page);
    await expect(chooser(page).locator(".sc-item")).toHaveText(["select 2", "All statements (3)", "From caret to end"]);
    await expect.poll(() => previewLines(page)).toEqual([2]);
    await page.keyboard.press("ArrowDown");
    await expect.poll(() => previewLines(page)).toEqual([1, 2, 3]);
    await page.keyboard.press("ArrowDown");
    await expect.poll(() => previewLines(page)).toEqual([2, 3]);
    await chooser(page).locator(".sc-item").first().hover();
    await expect.poll(() => previewLines(page)).toEqual([2]);
    await page.keyboard.press("Escape");
    await expect(chooser(page)).toHaveCount(0);
    await expect.poll(() => previewLines(page)).toEqual([]);
  });

  test("takes the keys: Esc closes without running, typing doesn't reach the editor, focus comes back at the caret", async ({ page }) => {
    const before = await text(page);
    await open(page);
    // Any other key closes it and does nothing else
    await page.keyboard.press("x");
    await expect(chooser(page)).toHaveCount(0);
    expect(await text(page)).toBe(before);
    await open(page);
    await page.keyboard.press("Escape");
    await expect(chooser(page)).toHaveCount(0);
    expect(await text(page)).toBe(before);
    await expect(resultTabs(page)).toHaveText(["Output"]);
    // Back in the editor at the same caret (line 2, column 1)
    await page.keyboard.type("-- ");
    await expect.poll(() => text(page)).toMatch(/^select 1;\n-- select 2;\nselect 3;$/);
  });

  test("a click outside closes it without running; Enter runs the highlighted option", async ({ page }) => {
    await open(page);
    await work(page).locator(".results-panel").click();
    await expect(chooser(page)).toHaveCount(0);
    await expect.poll(() => previewLines(page)).toEqual([]);
    await expect(resultTabs(page)).toHaveText(["Output"]);
    await editor(page).click();
    await page.keyboard.press("Meta+ArrowDown");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Home");
    await open(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(chooser(page)).toHaveCount(0);
    await resultTabs(page).first().click();
    const log = work(page).locator(".output-log");
    await expect(log).toContainText("select 2");
    await expect(log).toContainText("select 3");
    await expect(log).not.toContainText("select 1");
  });
});
