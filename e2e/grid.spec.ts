import type { Page } from "@playwright/test";
import { dialog, editor, expect, menuItem, openApp, openConsole, openInTheme, openSettings, runAll, spyCancels, test, treeItem, typeSql, work } from "./app";

const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());
/** The mock's created_at alternates no fraction / .5 / .125 */
const T1 = "2026-10-15 12:00:00 +09:00";
const T2 = "2026-10-15 12:00:00.5 +09:00";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
});

async function selectRows12(page: Page) {
  await work(page).getByRole("rowheader", { name: "1", exact: true }).click();
  await work(page).getByRole("rowheader", { name: "2", exact: true }).click({ modifiers: ["Shift"] });
}

// Rows 1–2 copied (⌘C) with each data extractor: values are copied raw (no display padding)
const EXPECTED: Record<string, string> = {
  CSV: `1,user_1,0.00,${T1},note\n2,user_2,1234.50,${T2},`,
  TSV: `1\tuser_1\t0.00\t${T1}\tnote\n2\tuser_2\t1234.50\t${T2}\t`,
  "SQL Inserts":
    `INSERT INTO public.users (id, user_name, amount, created_at, memo) VALUES ('1', 'user_1', '0.00', '${T1}', 'note');\n` +
    `INSERT INTO public.users (id, user_name, amount, created_at, memo) VALUES ('2', 'user_2', '1234.50', '${T2}', NULL);`,
  "SQL Updates":
    `UPDATE public.users SET id = '1', user_name = 'user_1', amount = '0.00', created_at = '${T1}', memo = 'note' WHERE id = '1';\n` +
    `UPDATE public.users SET id = '2', user_name = 'user_2', amount = '1234.50', created_at = '${T2}', memo = NULL WHERE id = '2';`,
  "Where Clause":
    `(id = '1' AND user_name = 'user_1' AND amount = '0.00' AND created_at = '${T1}' AND memo = 'note') OR\n` +
    `(id = '2' AND user_name = 'user_2' AND amount = '1234.50' AND created_at = '${T2}' AND memo IS NULL)`,
  "SQL IN List": `((1, 'user_1', 0.00, '${T1}', 'note'), (2, 'user_2', 1234.50, '${T2}', NULL))`,
  JSON: JSON.stringify([{ id: 1, user_name: "user_1", amount: "0.00", created_at: T1, memo: "note" }, { id: 2, user_name: "user_2", amount: "1234.50", created_at: T2, memo: null }], null, 2),
  Markdown: `| id | user_name | amount | created_at | memo |\n| --- | --- | --- | --- | --- |\n| 1 | user_1 | 0.00 | ${T1} | note |\n| 2 | user_2 | 1234.50 | ${T2} |  |`,
};

for (const [extractor, text] of Object.entries(EXPECTED)) {
  test(`⌘C with the ${extractor} extractor`, async ({ page }) => {
    const button = work(page).locator('[title="Data Extractor (copy format)"]');
    await button.click();
    await menuItem(page, new RegExp(`^(✓ |\\s*)${extractor}$`)).click();
    await expect(button).toHaveText(`${extractor} ▾`);
    await selectRows12(page);
    await page.keyboard.press("Meta+C");
    await expect.poll(() => clipboard(page)).toBe(text);
  });
}

test("extractor menu: every format, the current one checked", async ({ page }) => {
  await work(page).locator('[title="Data Extractor (copy format)"]').click();
  await expect(page.getByRole("menu").getByRole("menuitem")).toHaveText(Object.keys(EXPECTED).map((x) => new RegExp(`^(✓ |\\s*)${x}$`)));
  await expect(menuItem(page, "✓ CSV")).toBeVisible();
});

test("Copy as IN List: a range of cells, or a single cell's whole column", async ({ page }) => {
  const cell = (name: string) => work(page).getByRole("gridcell", { name, exact: true });
  await cell("user_1").click();
  await cell("user_3").click({ modifiers: ["Shift"] });
  await page.keyboard.press("Meta+C");
  await expect.poll(() => clipboard(page)).toBe("user_1\nuser_2\nuser_3");
  await cell("user_1").click({ button: "right" });
  await menuItem(page, /^Copy as IN List/).click();
  await expect.poll(() => clipboard(page)).toBe("('user_1', 'user_2', 'user_3')");

  await cell("user_2").click();
  await cell("user_2").click({ button: "right" });
  await menuItem(page, /^Copy as IN List/).click();
  await expect.poll(() => clipboard(page)).toBe(`(${Array.from({ length: 500 }, (_, i) => `'user_${i + 1}'`).join(", ")})`);
});

test("time fractions are padded to milliseconds in the grid", async ({ page }) => {
  // The body's rows (the header has two: the row-number corner and the column headers)
  const rows = work(page).locator(".grid-scroll").getByRole("row");
  await expect(rows.nth(0).getByRole("gridcell").nth(3)).toHaveText("2026-10-15 12:00:00.000 +09:00");
  await expect(rows.nth(1).getByRole("gridcell").nth(3)).toHaveText("2026-10-15 12:00:00.500 +09:00");
  await expect(rows.nth(2).getByRole("gridcell").nth(3)).toHaveText("2026-10-15 12:00:00.125 +09:00");
});

test("a WebView that refuses WebAssembly: multi-cell copy says why nothing was copied", async ({ page }) => {
  // The module fetch only (not Vite's `.wasm?url` import, which is JavaScript)
  await page.route((u) => u.pathname.endsWith(".wasm") && !u.search, (r) => r.abort());
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await selectRows12(page);
  await page.keyboard.press("Meta+C");
  await expect(page.locator(".statusbar")).toContainText("Copy formats need WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk");
});

test("a load-more that fails keeps its reason in the grid and the result unfinished (#80)", async ({ page }) => {
  const why =
    "The data source reconnected since this result was read (settings saved, disconnected or idle), and loading more would run the statement again, but it isn't a plain read. Run it again yourself to load more rows.";
  await page.evaluate((w) => ((window as any).__mockFetchMoreError = w), why);
  const scroll = work(page).locator(".grid-scroll").first();
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  // The TUI's words (qurry_shared::errors::more_rows_failed), below the rows (not over them), not only in the status bar
  await expect(work(page).locator(".grid-more-error")).toHaveText(`Couldn't load more rows: ${why}`);
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
  // Scrolling on doesn't ask again
  await scroll.evaluate((el) => (el.scrollTop = 0));
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => (window as any).__mockFetchMoreCalls)).toBe(1);
  // Run again starts over: the line goes away and loading more works again
  await page.evaluate(() => ((window as any).__mockFetchMoreError = undefined));
  await work(page).getByRole("button", { name: "Run again" }).click();
  await expect(work(page).locator(".grid-more-error")).toHaveCount(0);
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows");
});

test("rows that arrive after the table tab ran again are dropped, not appended to the new result", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFetchMoreDelay = 1000));
  const scroll = work(page).locator(".grid-scroll").first();
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(page.locator(".statusbar").getByRole("progressbar", { name: "Loading more rows" })).toBeVisible();
  await scroll.focus();
  await page.keyboard.press("Meta+R");
  // A load-more of the new result (if its grid asks right away) fails at once, so only the old answer could add rows
  await page.evaluate(() => Object.assign(window as any, { __mockFetchMoreDelay: undefined, __mockFetchMoreError: "stop" }));
  await page.waitForTimeout(1300);
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
  await expect(page.getByRole("progressbar", { name: "Loading more rows" })).toHaveCount(0);
});

test("loading more shows in the status bar as a run does; the grid gets nothing under its rows and stays put (#94)", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFetchMoreDelay = 1000));
  const scroll = work(page).locator(".grid-scroll").first();
  const status = page.locator(".statusbar");
  const at = () => scroll.evaluate((el) => ({ top: el.scrollTop, height: el.scrollHeight }));
  // No room kept under the 500 rows of 24 px
  expect((await at()).height).toBe(500 * 24);
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  const loading = await at();
  // The status bar's running indicator, in the status text's place: bar, words and elapsed time
  await expect(status.locator(".running-bar")).toContainText(/Loading more rows… \d+\.\d s/);
  await expect(status.getByRole("progressbar", { name: "Loading more rows" })).toBeVisible();
  await expect(work(page).getByRole("progressbar")).toHaveCount(0);
  expect(loading.height).toBe(500 * 24);
  // The rows come in: the indicator goes, the status text is back, the view stays where it was
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows");
  await expect(status.locator(".running-bar")).toHaveCount(0);
  await expect(status.locator(".sb-text")).toHaveText("620 rows retrieved");
  const after = await at();
  expect(after.top).toBe(loading.top);
  expect(after.height).toBe(620 * 24);
});

test("Add Row, then rows loaded below go before the new row: it stays selected, and Delete Row or Revert takes back only it", async ({ page }) => {
  const info = work(page).locator(".tb-info").first();
  const scroll = work(page).locator(".grid-scroll").first();
  for (const undo of ["Delete Row", "Revert Selected"]) {
    await work(page).getByRole("button", { name: /^Add Row/ }).click();
    await expect(info).toContainText("1 pending");
    // Scrolled to the end: the next rows load (once; the result then has all of them)
    await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(info).toHaveText("620 rows · 1 pending");
    // The view stays where it was when rows load (#94), so the new row, now last, may be out of view: scroll to it again
    await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(work(page).locator(".grid-row.added .cell.sel")).toHaveCount(1);
    await work(page).getByRole("button", { name: new RegExp(`^${undo}`) }).click();
    // A loaded row would be marked deleted (still 1 pending) if the selection had stayed where the new row was
    await expect(info).toHaveText("620 rows");
    await expect(work(page).locator(".grid-row.deleted")).toHaveCount(0);
  }
});

test("the grid toolbar's ↻ is ■ in the same spot while the table loads; ■ cancels the load (#107)", async ({ page }) => {
  const toolbar = work(page).locator(".grid-wrap .ui-toolbar").first();
  const reload = toolbar.getByRole("button", { name: "Reload Page (⌘r)" });
  const stop = toolbar.getByRole("button", { name: "Cancel (⌘F2)" });
  await expect(stop).toHaveCount(0);
  const spot = (await reload.boundingBox())!;
  await work(page).locator(".dg-filter-field input").first().fill("pg_sleep(2) is not null");
  await page.keyboard.press("Enter");
  await expect(stop).toBeVisible();
  await expect(reload).toHaveCount(0);
  const box = (await stop.boundingBox())!;
  expect([box.x, box.y, box.width]).toEqual([spot.x, spot.y, spot.width]);
  await expect(toolbar.locator(".ui-icon-btn.tone-danger")).toHaveCount(1);
  const cancels = await spyCancels(page);
  await stop.click();
  await expect.poll(cancels).toBe(1);
  // The mock lets the statement finish: ↻ again
  await expect(reload).toBeVisible({ timeout: 5000 });
});

test("the grid's ↻ is ■ while it loads more on scroll; ■ or the status bar's ✕ cancels only that fetch, the rows stay (#107)", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFetchMoreDelay = 2000));
  const grid = work(page).locator(".grid-wrap");
  const scroll = grid.locator(".grid-scroll");
  const stop = grid.getByRole("button", { name: "Cancel (⌘F2)" });
  const bar = page.locator(".statusbar .running-bar");
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(bar).toContainText("Loading more rows");
  await expect(stop).toBeVisible();
  // The status bar has no ■; its indicator's ✕ shows on hover and cancels the same fetch
  await expect(page.locator(".statusbar").getByRole("button", { name: /^Cancel Running/ })).toHaveCount(0);
  await bar.hover();
  await bar.getByRole("button", { name: "Cancel (⌘F2)" }).click();
  await expect(grid.locator(".grid-more-error")).toHaveText("Couldn't load more rows: ERROR: canceling statement due to user request");
  await expect(grid.locator(".tb-info")).toHaveText("500+ rows");
  await expect(grid.getByRole("gridcell", { name: "user_500", exact: true })).toBeVisible();
  await expect(grid.getByRole("button", { name: "Reload Page (⌘r)" })).toBeVisible();
  await expect(bar).toHaveCount(0);
  // Run again, scroll again, and the toolbar's ■ this time: the same
  await grid.getByRole("button", { name: "Run again" }).click();
  await expect(grid.locator(".grid-more-error")).toHaveCount(0);
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await stop.click();
  await expect(grid.locator(".grid-more-error")).toHaveText("Couldn't load more rows: ERROR: canceling statement due to user request");
  await expect(grid.locator(".tb-info")).toHaveText("500+ rows");
});

test("the status bar shows the focused grid's shown columns: follows sideways scrolling, gone when every column fits (#107)", async ({ page }) => {
  const cols = page.locator(".statusbar .ui-status-item").filter({ hasText: /^cols / });
  // users: every column fits
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await expect(page.locator(".statusbar")).toContainText("Row 1, Col 2");
  await expect(cols).toHaveCount(0);
  await openConsole(page);
  await typeSql(page, "select * from wide_demo");
  await page.keyboard.press("Meta+Enter");
  const scroll = work(page).locator(".grid-scroll").first();
  await work(page).locator(".grid-wrap .cell:not(.rownum)").first().click();
  await expect(cols).toHaveText(/^cols 1–\d+ of \d+$/);
  const [, last, total] = (await cols.textContent())!.match(/(\d+) of (\d+)/)!.map(Number);
  expect(last).toBeLessThan(total);
  // Scrolled sideways (as the trackpad does): the range moves along, the count stays
  await scroll.evaluate((el) => (el.scrollLeft = el.scrollWidth));
  await expect(cols).toHaveText(new RegExp(`^cols \\d+–${total} of ${total}$`));
  await scroll.evaluate((el) => (el.scrollLeft = 0));
  await expect(cols).toHaveText(`cols 1–${last} of ${total}`);
  // A wide window shows them all: gone
  await page.setViewportSize({ width: 6000, height: 900 });
  await expect(cols).toHaveCount(0);
  await page.setViewportSize({ width: 1400, height: 900 });
  await expect(cols).toHaveText(`cols 1–${last} of ${total}`);
  // The editor focused: no columns, its own position
  await editor(page).click();
  await expect(cols).toHaveCount(0);
  await expect(page.locator(".statusbar")).toContainText(/Ln 1, Col \d+/);
});

test("loading more never covers the current cell when ↓ moves to the end of the view (#94)", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFetchMoreDelay = 3000));
  const scroll = work(page).locator(".grid-scroll").first();
  const bar = page.locator(".statusbar").getByRole("progressbar", { name: "Loading more rows" });
  // The view's last row near 440, the cursor near it: ↓ scrolls on until the load starts (30 rows before the end)
  await scroll.evaluate((el) => (el.scrollTop = 440 * 24 - el.clientHeight));
  await work(page).getByRole("gridcell", { name: "user_436", exact: true }).click();
  for (let i = 0; i < 40 && !(await bar.count()); i++) await page.keyboard.press("ArrowDown");
  await expect(bar).toBeVisible();
  // Nothing is drawn over the grid, and the cell under the cursor is inside the view
  await expect(work(page).getByRole("progressbar")).toHaveCount(0);
  const active = (await work(page).locator(".cell.active").boundingBox())!;
  const view = (await scroll.boundingBox())!;
  const client = await scroll.evaluate((el) => el.clientHeight);
  expect(active.y + active.height).toBeLessThanOrEqual(view.y + client);
});

/** text-align of the first three data cells of a column */
const aligns = (page: Page, col: number) =>
  work(page)
    .locator(".grid-row")
    .evaluateAll((rows, c) => rows.slice(0, 3).map((r) => getComputedStyle(r.querySelectorAll(".cell:not(.rownum)")[c]).textAlign), col);

test("NULL sits where its column's type does by default; Settings › Appearance › Data grid › Alignment puts it on the right, kept after a reload (#75, #95)", async ({ page }) => {
  // memo (TEXT): "note", <null>, <null>; amount (NUMERIC, arrives as text): a number
  await expect.poll(() => aligns(page, 4)).toEqual(["left", "left", "left"]);
  await expect.poll(() => aligns(page, 2)).toEqual(["right", "right", "right"]);
  await openSettings(page, "Data grid");
  const nul = dialog(page).getByRole("combobox", { name: "NULL alignment" });
  await expect(nul).toHaveValue("column");
  await expect(nul.locator("option")).toHaveText(["Column type", "Left", "Center", "Right"]);
  await expect(dialog(page).getByRole("combobox", { name: "String alignment" }).locator("option")).toHaveText(["Left", "Center", "Right"]);
  await nul.selectOption("right");
  await page.keyboard.press("Escape");
  await expect.poll(() => aligns(page, 4)).toEqual(["left", "right", "right"]);
  await expect.poll(() => aligns(page, 2)).toEqual(["right", "right", "right"]);
  // Saved (the mock's settings.json): after a reload the grid and Settings still have it, marked as changed
  await page.reload();
  await treeItem(page, "users").first().dblclick();
  await expect.poll(() => aligns(page, 4)).toEqual(["left", "right", "right"]);
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page).getByRole("combobox", { name: "NULL alignment" })).toHaveValue("right");
  await expect(dialog(page).locator(".align-label.custom")).toHaveText(["NULL"]);
});

test("Settings › Appearance › Data grid › Alignment that can't be saved goes back and says so", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = true));
  await openSettings(page, "Data grid");
  await dialog(page).getByRole("combobox", { name: "NULL alignment" }).selectOption("left");
  await expect(dialog(page).getByText(/Couldn't save the alignment \(.*permission denied\)\. It is back to what it was\./)).toBeVisible();
  await expect(dialog(page).getByRole("combobox", { name: "NULL alignment" })).toHaveValue("column");
});

test("a WebView that refuses WebAssembly: numbers stay on the right and Settings › Appearance › Data grid › Alignment says why it can't be changed", async ({ page }) => {
  await page.route((u) => u.pathname.endsWith(".wasm") && !u.search, (r) => r.abort());
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  // id (JSON numbers) on the right as before the setting; user_name and memo (with its NULLs) where text starts (no al-* class)
  await expect.poll(() => aligns(page, 0)).toEqual(["right", "right", "right"]);
  await expect.poll(() => aligns(page, 1)).toEqual(["start", "start", "start"]);
  await expect.poll(() => aligns(page, 4)).toEqual(["start", "start", "start"]);
  await openSettings(page, "Data grid");
  const nul = dialog(page).getByRole("combobox", { name: "NULL alignment" });
  await expect(nul).toBeDisabled();
  await expect(nul).toHaveValue("");
  await expect(dialog(page).getByText("Alignment needs WebAssembly, which this WebView refused")).toBeVisible();
});

test("Settings › Appearance › Data grid › Alignment: clicking a kind's name focuses its own select, not the first one", async ({ page }) => {
  await openSettings(page, "Data grid");
  await dialog(page).locator(".align-label", { hasText: "NULL" }).click();
  await expect(dialog(page).getByRole("combobox", { name: "NULL alignment" })).toBeFocused();
  await dialog(page).getByText("Alignment", { exact: true }).click();
  await expect(dialog(page).getByRole("combobox", { name: "String alignment" })).not.toBeFocused();
});

test("⌘f opens and focuses the find bar, pressed again it stays open (the toolbar button toggles)", async ({ page }) => {
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  const find = work(page).locator(".grid-find input");
  await page.keyboard.press("Meta+KeyF");
  await expect(find).toBeFocused();
  await work(page).getByRole("gridcell", { name: "user_2", exact: true }).click();
  await page.keyboard.press("Meta+KeyF");
  await expect(find).toBeFocused();
  await work(page).getByRole("button", { name: /^Find on Current Page/ }).click();
  await expect(find).toHaveCount(0);
});

test("a grid key whose command is disabled now still belongs to the grid (⌘r while reloading doesn't reach the WebView)", async ({ page }) => {
  await work(page).locator(".dg-filter-field input").first().fill("pg_sleep(3) is not null");
  await page.keyboard.press("Enter");
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  // While it reloads the button is ■ (cancel), not ↻
  await expect(work(page).getByRole("button", { name: /^Reload Page/ })).toHaveCount(0);
  // Read after the event has gone through the app's handlers (the grid stops it, so a bubbling listener never sees it)
  await page.evaluate(() => window.addEventListener("keydown", (e) => e.code === "KeyR" && setTimeout(() => ((window as any).__reloadKey = e.defaultPrevented)), true));
  await page.keyboard.press("Meta+KeyR");
  await expect.poll(() => page.evaluate(() => (window as any).__reloadKey)).toBe(true);
});

test("grid keys work in Transpose and Chart too (find, back to Table)", async ({ page }) => {
  await work(page).getByRole("button", { name: "View as" }).click();
  await menuItem(page, "Transpose").click();
  await work(page).locator(".grid-scroll.transposed").click();
  await page.keyboard.press("Meta+KeyF");
  await expect(work(page).locator(".grid-find input")).toBeFocused();
  await page.keyboard.press("Escape");
  await work(page).getByRole("button", { name: "View as" }).click();
  await menuItem(page, "Chart").click();
  await work(page).locator(".grid-wrap").click({ position: { x: 300, y: 300 } });
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__calls = [];
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: unknown) => (w.__calls.push(cmd), orig(cmd, args));
  });
  await page.keyboard.press("Meta+KeyR");
  await expect.poll(() => page.evaluate(() => (window as any).__calls)).toContain("run_query");
});

for (const theme of ["dark", "light", "hc"]) {
  test(`jsonb cells color keys, strings, numbers, keywords and punctuation apart (${theme}); a large value renders only what fits`, async ({ page }) => {
    await openInTheme(page, theme);
    await openConsole(page);
    await typeSql(page, "select * from json_demo");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Meta+Enter");
    const cell = work(page).getByRole("gridcell").filter({ hasText: '"type":"payment.paid"' }).first();
    await expect(cell).toBeVisible();
    const colors = await cell.evaluate((el) => {
      const of = (k: string) => getComputedStyle(el.querySelector(`.json-${k}`)!).color;
      return { key: of("key"), string: of("string"), number: of("number"), keyword: of("keyword"), punct: of("punct"), text: getComputedStyle(el).color, height: el.getBoundingClientRect().height };
    });
    const kinds = [colors.key, colors.string, colors.number, colors.keyword, colors.punct];
    expect(new Set(kinds).size, JSON.stringify(colors)).toBe(5);
    expect(kinds.slice(0, 4)).not.toContain(colors.text);
    // Still one line (no pretty-printing): a cell is as tall as its row
    expect(colors.height).toBe(24);
    // Row 3 holds ~100 KB: its cell renders the first 200 characters (lib/jsonSpans.ts JSON_CAP), colored
    const big = work(page).getByRole("gridcell").filter({ hasText: '{"items":[' }).first();
    expect(await big.evaluate((el) => [el.textContent!.length, el.querySelectorAll("span").length > 10])).toEqual([200, true]);
  });
}

// ─── Value Editor panel (⇧↩): beside the grid, follows the current cell, the grid keeps the keys ───
test.describe("Value Editor panel", () => {
  const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
  const panel = (page: Page) => work(page).locator(".value-panel");
  const text = (page: Page) => panel(page).locator("textarea.ve-text");
  const gridScroll = (page: Page) => work(page).locator(".grid-scroll");
  const pending = (page: Page) => work(page).locator(".tb-info");
  /** payments.data (jsonb) of row 1, as PostgreSQL sends it */
  const DATA_1 = '{"kind": "card", "meta": {"src": "web", "ip": "10.0.0.1"}, "items": [0, 1]}';

  test("↓ with the panel open moves the cell cursor and the panel follows", async ({ page }) => {
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    await expect(panel(page).locator(".ve-title")).toHaveText("user_name : varchar");
    await expect(text(page)).toHaveValue("user_1");
    await expect(text(page)).toBeFocused();
    // ⎋ goes back to the grid, the panel stays
    await page.keyboard.press("Escape");
    await expect(gridScroll(page)).toBeFocused();
    await expect(panel(page)).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await expect(cell(page, "user_2")).toHaveClass(/\bactive\b/);
    await expect(text(page)).toHaveValue("user_2");
    await page.keyboard.press("ArrowRight");
    await expect(panel(page).locator(".ve-title")).toHaveText("amount : numeric");
  });

  test("⇧↩ on another cell focuses the panel on that cell", async ({ page }) => {
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    await cell(page, "user_3").click();
    await expect(gridScroll(page)).toBeFocused();
    await expect(text(page)).toHaveValue("user_3");
    await page.keyboard.press("Shift+Enter");
    await expect(text(page)).toBeFocused();
    await expect(text(page)).toHaveValue("user_3");
  });

  test("the panel sits beside the grid and narrows it, never over it", async ({ page }) => {
    const before = (await work(page).locator(".grid-body").boundingBox())!;
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    const grid = (await work(page).locator(".grid-body").boundingBox())!;
    const side = (await panel(page).boundingBox())!;
    expect(grid.width).toBeLessThan(before.width - 100);
    expect(grid.x + grid.width).toBeLessThanOrEqual(side.x + 1);
    // Same top and bottom as the grid (below the toolbar), up to the wrap's right edge
    expect(Math.abs(side.y - grid.y)).toBeLessThanOrEqual(1);
    expect(Math.abs(side.y + side.height - (grid.y + grid.height))).toBeLessThanOrEqual(1);
    expect(Math.abs(side.x + side.width - (before.x + before.width))).toBeLessThanOrEqual(1);
  });

  test("⌘↩ applies and returns to the grid; leaving the cell applies too", async ({ page }) => {
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    await text(page).fill("kim");
    await page.keyboard.press("Meta+Enter");
    await expect(cell(page, "kim")).toBeVisible();
    await expect(pending(page)).toContainText("1 pending");
    await expect(gridScroll(page)).toBeFocused();
    await expect(panel(page)).toBeVisible();
    // An edit not applied yet goes in when the selection moves on (DataGrip)
    await page.keyboard.press("ArrowDown");
    await text(page).fill("lee");
    await cell(page, "user_3").click();
    await expect(cell(page, "lee")).toBeVisible();
    await expect(pending(page)).toContainText("2 pending");
    await expect(text(page)).toHaveValue("user_3");
  });

  test("⌘w closes the panel first (back on the same cell), then the tab", async ({ page }) => {
    await cell(page, "user_2").click();
    await page.keyboard.press("Shift+Enter");
    await page.keyboard.press("Meta+KeyW");
    await expect(panel(page)).toHaveCount(0);
    await expect(page.getByRole("tab", { name: /users/ })).toHaveCount(1);
    await expect(gridScroll(page)).toBeFocused();
    await expect(cell(page, "user_2")).toHaveClass(/\bactive\b/);
    await page.keyboard.press("Meta+KeyW");
    await expect(page.getByRole("tab", { name: /users/ })).toHaveCount(0);
  });

  test("⇧⎋ closes the panel, from the panel or from its grid", async ({ page }) => {
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    await expect(text(page)).toBeFocused();
    await page.keyboard.press("Shift+Escape");
    await expect(panel(page)).toHaveCount(0);
    await expect(gridScroll(page)).toBeFocused();
    await page.keyboard.press("Shift+Enter");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Shift+Escape");
    await expect(panel(page)).toHaveCount(0);
  });

  test("a jsonb cell opens pretty-printed and highlighted, and an edit goes back on one line", async ({ page }) => {
    await treeItem(page, "payments").first().dblclick();
    await cell(page, DATA_1).click();
    await page.keyboard.press("Shift+Enter");
    const ed = panel(page).locator(".monaco-editor");
    await expect(ed).toBeVisible();
    await expect(panel(page).locator(".ve-title")).toHaveText("data : jsonb");
    await expect(ed.locator(".view-line")).toHaveCount(11);
    // Monaco's JSON tokens: keys, strings, numbers and delimiters each get their own colour class
    // (Monaco tokenizes after it draws the text, so this waits for the colours rather than reading them once)
    const classes = () => ed.locator(".view-line span span").evaluateAll((els) => new Set(els.map((e) => e.className)).size);
    await expect.poll(classes).toBeGreaterThanOrEqual(3);
    // Line 2 is `  "kind": "card",`: replace card with bank in the pretty layout
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("End");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    for (let i = 0; i < 4; i++) await page.keyboard.press("Backspace");
    await page.keyboard.type("bank");
    await page.keyboard.press("Meta+Enter");
    await expect(cell(page, '{"kind":"bank","meta":{"src":"web","ip":"10.0.0.1"},"items":[0,1]}')).toBeVisible();
    await expect(pending(page)).toContainText("1 pending");
  });

  test("invalid JSON in a jsonb column is refused with the reason", async ({ page }) => {
    await treeItem(page, "payments").first().dblclick();
    await cell(page, DATA_1).click();
    await page.keyboard.press("Shift+Enter");
    // ⇧↩ puts the caret in Monaco once it has mounted: wait for that, not just for the editor to show
    await expect(panel(page).locator(".monaco-editor textarea")).toBeFocused();
    await page.keyboard.press("Meta+A");
    await page.keyboard.press("Backspace");
    await page.keyboard.type("x");
    await page.keyboard.press("Meta+Enter");
    await expect(panel(page).locator(".ve-error")).toContainText("Invalid JSON");
    await expect(pending(page)).not.toContainText("pending");
    await expect(cell(page, DATA_1)).toBeVisible();
  });

  test("console results the backend didn't tie to a table are view-only in the panel", async ({ page }) => {
    await openConsole(page);
    await typeSql(page, "select * from users where id > 0");
    await runAll(page);
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    await expect(text(page)).toHaveAttribute("readonly", "");
    await expect(panel(page).locator(".ve-foot")).toContainText("Read-only");
  });
});

test("console results the backend didn't check say why their edit buttons are greyed out", async ({ page }) => {
  await openConsole(page);
  await typeSql(page, "select * from users where id > 0");
  await runAll(page);
  const del = work(page).getByRole("button", { name: /^Delete Row/ });
  await expect(del).toBeDisabled();
  await del.hover({ force: true });
  await expect(del).toHaveAttribute("title", /Console results are read-only\. Open the table to edit its rows\.$/);
  await expect(del).toHaveAttribute("aria-description", "Console results are read-only. Open the table to edit its rows.");
});

test.describe("multi-range selection (⌘-click)", () => {
  const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
  const rowNum = (page: Page, n: number) => work(page).getByRole("rowheader", { name: String(n), exact: true });
  const selected = (page: Page) => work(page).locator('[role=gridcell][aria-selected="true"]').allTextContents();
  const ROW1 = `1,user_1,0.00,${T1},note`;
  const ROW5 = `5,user_5,4938.00,${T2},`;

  test("⌘-click rows 5 and 1: ⌘C copies both rows in grid order", async ({ page }) => {
    await rowNum(page, 5).click({ modifiers: ["Meta"] });
    await rowNum(page, 1).click({ modifiers: ["Meta"] });
    await expect(rowNum(page, 1)).toHaveClass(/row-sel/);
    await expect(rowNum(page, 3)).not.toHaveClass(/row-sel/);
    await page.keyboard.press("Meta+C");
    await expect.poll(() => clipboard(page)).toBe(`${ROW1}\n${ROW5}`);
  });

  test("⌘⌫ marks every selected row for deletion", async ({ page }) => {
    await rowNum(page, 1).click();
    await rowNum(page, 5).click({ modifiers: ["Meta"] });
    await page.keyboard.press("Meta+Backspace");
    await expect(work(page).locator(".grid-row.deleted .rownum")).toHaveText(["1", "5"]);
    await expect(work(page).locator(".tb-info")).toContainText("2 pending");
  });

  test("⇧-click extends the last range, a plain click starts over, ↓ collapses to one cell", async ({ page }) => {
    await cell(page, "user_1").click();
    await cell(page, "user_5").click({ modifiers: ["Meta"] });
    await cell(page, "user_7").click({ modifiers: ["Shift"] });
    await expect.poll(() => selected(page)).toEqual(["user_1", "user_5", "user_6", "user_7"]);
    await page.keyboard.press("Meta+C");
    await expect.poll(() => clipboard(page)).toBe("user_1\nuser_5\nuser_6\nuser_7");
    await cell(page, "user_2").click();
    await expect.poll(() => selected(page)).toEqual(["user_2"]);
    await cell(page, "user_4").click({ modifiers: ["Meta"] });
    await page.keyboard.press("ArrowDown");
    await expect.poll(() => selected(page)).toEqual(["user_5"]);
  });

  test("⌘-click on a selected cell takes it out; copy takes what is left", async ({ page }) => {
    await cell(page, "user_1").click();
    await cell(page, "user_3").click({ modifiers: ["Meta"] });
    await cell(page, "user_1").click({ modifiers: ["Meta"] });
    await expect.poll(() => selected(page)).toEqual(["user_3"]);
    await page.keyboard.press("Meta+C");
    await expect.poll(() => clipboard(page)).toBe("user_3");
  });

  test("ranges in different columns copy as the union of columns, other cells empty", async ({ page }) => {
    await cell(page, "user_1").click();
    await cell(page, "1234.50").click({ modifiers: ["Meta"] });
    await page.keyboard.press("Meta+C");
    await expect.poll(() => clipboard(page)).toBe("user_1,\n,1234.50");
  });

  test("ranges in different columns: SQL Updates and Where Clause leave the unselected cells out, never NULL", async ({ page }) => {
    const button = work(page).locator('[title="Data Extractor (copy format)"]');
    for (const [extractor, text] of [
      ["SQL Updates", "UPDATE public.users SET user_name = 'user_1' WHERE id = '1';\nUPDATE public.users SET amount = '1234.50' WHERE id = '2';"],
      ["Where Clause", "(user_name = 'user_1') OR\n(amount = '1234.50')"],
    ]) {
      await button.click();
      await menuItem(page, new RegExp(`^(✓ |\\s*)${extractor}$`)).click();
      await cell(page, "user_1").click();
      await cell(page, "1234.50").click({ modifiers: ["Meta"] });
      await page.keyboard.press("Meta+C");
      await expect.poll(() => clipboard(page)).toBe(text);
    }
  });

  test("the Value Editor follows the last clicked cell", async ({ page }) => {
    const panel = work(page).locator(".value-panel");
    await cell(page, "user_1").click();
    await page.keyboard.press("Shift+Enter");
    await cell(page, "2469.00").click({ modifiers: ["Meta"] });
    await expect(panel.locator(".ve-title")).toHaveText("amount : numeric");
    await expect(panel.locator("textarea.ve-text")).toHaveValue("2469.00");
    await cell(page, "user_4").click({ modifiers: ["Meta"] });
    await expect(panel.locator(".ve-title")).toHaveText("user_name : varchar");
    await expect(panel.locator("textarea.ve-text")).toHaveValue("user_4");
  });

  test("paste into several ranges says how to get back to one", async ({ page }) => {
    await cell(page, "user_1").click();
    await cell(page, "user_3").click({ modifiers: ["Meta"] });
    await page.keyboard.press("Meta+V");
    await expect(page.locator(".statusbar")).toContainText("Paste needs one selected range: click a cell to select just it (⌘-click adds cells to the selection)");
  });
});

// Submit's statements come from qurry_shared::result_edit (the TUI's rules): DELETE, then UPDATE, then INSERT; a new row's cell set to
// NULL is sent, one never set is left out; an all-default row is DEFAULT VALUES on PostgreSQL and () VALUES () on MySQL (#155)
const submitted = (page: Page) => page.evaluate(() => ((window as any).__mockSubmitted ?? []) as string[]);

test("Submit on PostgreSQL: the preview lists DELETE, UPDATE, INSERT in that order, and Submit sends them so", async ({ page }) => {
  const button = (name: RegExp) => work(page).getByRole("button", { name });
  await work(page).getByRole("gridcell", { name: "user_2", exact: true }).click();
  await page.keyboard.press("Meta+Alt+KeyN");
  await work(page).getByRole("rowheader", { name: "1", exact: true }).click();
  await button(/^Delete Row/).click();
  await button(/^Add Row/).click();
  // Scrolling to the new row loads the next rows before it (the view stays): scroll to it again
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows · 3 pending");
  await work(page).locator(".grid-scroll").first().evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".grid-row.added .cell.sel")).toHaveCount(1);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Meta+Alt+KeyN");
  await button(/^Add Row/).click();
  await button(/^Preview Pending Changes/).click();
  const want = [
    "DELETE FROM public.users WHERE id = '1';",
    "UPDATE public.users SET user_name = NULL WHERE id = '2';",
    "INSERT INTO public.users (user_name) VALUES (NULL);",
    "INSERT INTO public.users DEFAULT VALUES;",
  ];
  await expect(dialog(page).locator(".sql-preview")).toHaveText(want.join("\n"));
  await dialog(page).getByRole("button", { name: "Submit", exact: true }).click();
  await expect.poll(() => submitted(page)).toEqual(want);
});

test("Submit on MySQL: an empty new row is INSERT INTO … () VALUES (), listed by the Production confirm", async ({ page }) => {
  // [MySQL] shop (Production) lists its tables after [DEV] shop_app's
  await treeItem(page, "users").nth(1).dblclick();
  await expect(page.getByRole("tab", { name: /users/ })).toHaveCount(2);
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await work(page).getByRole("button", { name: /^Add Row/ }).click();
  await work(page).getByRole("button", { name: /^Submit/ }).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [MySQL] shop");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["INSERT INTO shop.users () VALUES ();"]);
  await dialog(page).getByRole("button", { name: "Save" }).click();
  await expect.poll(() => submitted(page)).toEqual(["INSERT INTO shop.users () VALUES ();"]);
});

test("pasted CSV rows: a quoted JSON value loses its CSV quotes, an empty field is NULL and \"\" an empty string (#160)", async ({ page }) => {
  const text = '10,"{""k"": ""a, b""}",,,\r\n11,,5,,""\r\n';
  await page.evaluate((t) => (navigator.clipboard.readText = async () => t), text);
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await work(page).getByRole("button", { name: /^Add Row/ }).click();
  // Scrolling to the new row loads the next rows before it (the view stays): scroll to it again
  const scroll = work(page).locator(".grid-scroll").first();
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows · 1 pending");
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".grid-row.added .cell.sel")).toHaveCount(1);
  await page.keyboard.press("Home");
  await page.keyboard.press("Meta+V");
  await work(page).getByRole("button", { name: /^Preview Pending Changes/ }).click();
  const want = [
    `INSERT INTO public.users (id, user_name, amount, created_at, memo) VALUES ('10', '{"k": "a, b"}', NULL, NULL, NULL);`,
    "INSERT INTO public.users (id, user_name, amount, created_at, memo) VALUES ('11', NULL, '5', NULL, '');",
  ];
  await expect(dialog(page).locator(".sql-preview")).toHaveText(want.join("\n"));
  await dialog(page).getByRole("button", { name: "Submit", exact: true }).click();
  await expect.poll(() => submitted(page)).toEqual(want);
});

test("one row copied as CSV pastes back as its cells on a new row, its NULL as NULL; a typed `a, b` stays one value (#160)", async ({ page }) => {
  // Row 2 (memo NULL) copied with the default extractor, CSV: one line, read back as CSV because the grid wrote it
  await work(page).getByRole("rowheader", { name: "2", exact: true }).click();
  await page.keyboard.press("Meta+C");
  await expect.poll(() => clipboard(page)).toBe(`2,user_2,1234.50,${T2},`);
  await work(page).getByRole("button", { name: /^Add Row/ }).click();
  const scroll = work(page).locator(".grid-scroll").first();
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows · 1 pending");
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".grid-row.added .cell.sel")).toHaveCount(1);
  await page.keyboard.press("Home");
  await page.keyboard.press("Meta+V");
  // Text the grid didn't copy: one line without a tab is one value
  await page.evaluate(() => (navigator.clipboard.readText = async () => "a, b"));
  await scroll.evaluate((el) => (el.scrollTop = 0));
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await page.keyboard.press("Meta+V");
  await work(page).getByRole("button", { name: /^Preview Pending Changes/ }).click();
  await expect(dialog(page).locator(".sql-preview")).toHaveText(
    [
      "UPDATE public.users SET user_name = 'a, b' WHERE id = '1';",
      `INSERT INTO public.users (id, user_name, amount, created_at, memo) VALUES ('2', 'user_2', '1234.50', '${T2}', NULL);`,
    ].join("\n"),
  );
});

test("Clone Row keeps the source's NULL (sent as NULL, not the column default), and a new row's untouched cells stay untouched", async ({ page }) => {
  // user_2's memo is NULL
  await work(page).getByRole("gridcell", { name: "user_2", exact: true }).click();
  await page.keyboard.press("Meta+KeyD");
  await work(page).getByRole("button", { name: /^Add Row/ }).click();
  await expect(work(page).locator(".grid-row.added .cell.sel")).toHaveCount(1);
  await page.keyboard.press("Meta+KeyD");
  await work(page).getByRole("button", { name: /^Preview Pending Changes/ }).click();
  await expect(dialog(page).locator(".sql-preview")).toHaveText(
    [
      "INSERT INTO public.users (user_name, amount, created_at, memo) VALUES ('user_2', '1234.50', '2026-10-15 12:00:00.5 +09:00', NULL);",
      "INSERT INTO public.users DEFAULT VALUES;",
      "INSERT INTO public.users DEFAULT VALUES;",
    ].join("\n"),
  );
});

test("a WebView that refuses WebAssembly: Submit and Preview Pending Changes say why nothing happens", async ({ page }) => {
  await page.route((u) => u.pathname.endsWith(".wasm") && !u.search, (r) => r.abort());
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await work(page).getByRole("button", { name: /^Add Row/ }).click();
  const why = "Submit needs WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk";
  await work(page).getByRole("button", { name: /^Submit/ }).click();
  await expect(page.locator(".statusbar")).toContainText(why);
  await work(page).getByRole("button", { name: /^Preview Pending Changes/ }).click();
  await expect(page.locator(".statusbar")).toContainText(why);
  await expect(dialog(page)).toHaveCount(0);
  expect(await submitted(page)).toEqual([]);
});
