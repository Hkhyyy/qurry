import type { Page } from "@playwright/test";
import { expect, menuItem, openApp, test, treeItem, work } from "./app";

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
  const rows = work(page).getByRole("row");
  await expect(rows.nth(1).getByRole("gridcell").nth(3)).toHaveText("2026-10-15 12:00:00.000 +09:00");
  await expect(rows.nth(2).getByRole("gridcell").nth(3)).toHaveText("2026-10-15 12:00:00.500 +09:00");
  await expect(rows.nth(3).getByRole("gridcell").nth(3)).toHaveText("2026-10-15 12:00:00.125 +09:00");
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
  // The TUI's words (qurry_shared::errors::more_rows_failed), in the grid, not only in the status bar
  await expect(work(page).locator(".grid-more-error")).toHaveText(`Couldn't load more rows: ${why}`);
  await expect(work(page).locator(".tb-info").first()).toHaveText("500+ rows");
  // Scrolling on doesn't ask again
  await scroll.evaluate((el) => (el.scrollTop = 0));
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => (window as any).__mockFetchMoreCalls)).toBe(1);
  // Running it again (⌘R) starts over: the line goes away and loading more works again
  await page.evaluate(() => ((window as any).__mockFetchMoreError = undefined));
  await scroll.focus();
  await page.keyboard.press("Meta+R");
  await expect(work(page).locator(".grid-more-error")).toHaveCount(0);
  await scroll.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(work(page).locator(".tb-info").first()).toHaveText("620 rows");
});
