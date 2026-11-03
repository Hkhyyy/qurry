import type { Page } from "@playwright/test";
import { dialog, expect, menuItem, openApp, test, treeItem, work } from "./app";

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

/** text-align of the first three data cells of a column */
const aligns = (page: Page, col: number) =>
  work(page)
    .locator(".grid-row")
    .evaluateAll((rows, c) => rows.slice(0, 3).map((r) => getComputedStyle(r.querySelectorAll(".cell:not(.rownum)")[c]).textAlign), col);

test("NULL is right-aligned by default; Settings › Appearance › Alignment puts it back on the left (#75)", async ({ page }) => {
  // memo (TEXT): "note", <null>, <null>; amount (NUMERIC, arrives as text): a number
  await expect.poll(() => aligns(page, 4)).toEqual(["left", "right", "right"]);
  await expect.poll(() => aligns(page, 2)).toEqual(["right", "right", "right"]);
  await page.keyboard.press("Meta+Comma");
  const nul = dialog(page).getByRole("combobox", { name: "NULL alignment" });
  await expect(nul).toHaveValue("right");
  await nul.selectOption("left");
  await page.keyboard.press("Escape");
  await expect.poll(() => aligns(page, 4)).toEqual(["left", "left", "left"]);
  await expect.poll(() => aligns(page, 2)).toEqual(["right", "right", "right"]);
  // Saved (the mock's settings.json) and marked as changed
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page).getByRole("combobox", { name: "NULL alignment" })).toHaveValue("left");
  await expect(dialog(page).locator(".align-label.custom")).toHaveText(["NULL"]);
});

test("Settings › Appearance › Alignment that can't be saved goes back and says so", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = true));
  await page.keyboard.press("Meta+Comma");
  await dialog(page).getByRole("combobox", { name: "NULL alignment" }).selectOption("left");
  await expect(dialog(page).getByText(/Couldn't save the alignment \(.*permission denied\)\. It is back to what it was\./)).toBeVisible();
  await expect(dialog(page).getByRole("combobox", { name: "NULL alignment" })).toHaveValue("right");
});

test("a WebView that refuses WebAssembly: numbers stay on the right and Settings › Alignment says why it can't be changed", async ({ page }) => {
  await page.route((u) => u.pathname.endsWith(".wasm") && !u.search, (r) => r.abort());
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  // id (JSON numbers) on the right as before the setting; user_name and memo (with its NULLs) where text starts (no al-* class)
  await expect.poll(() => aligns(page, 0)).toEqual(["right", "right", "right"]);
  await expect.poll(() => aligns(page, 1)).toEqual(["start", "start", "start"]);
  await expect.poll(() => aligns(page, 4)).toEqual(["start", "start", "start"]);
  await page.keyboard.press("Meta+Comma");
  const nul = dialog(page).getByRole("combobox", { name: "NULL alignment" });
  await expect(nul).toBeDisabled();
  await expect(nul).toHaveValue("");
  await expect(dialog(page).getByText("Alignment needs WebAssembly, which this WebView refused")).toBeVisible();
});

test("Settings › Alignment: clicking a kind's name focuses its own select, not the first one", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await dialog(page).locator(".align-label", { hasText: "NULL" }).click();
  await expect(dialog(page).getByRole("combobox", { name: "NULL alignment" })).toBeFocused();
  await dialog(page).getByText("Alignment", { exact: true }).click();
  await expect(dialog(page).getByRole("combobox", { name: "String alignment" })).not.toBeFocused();
});
