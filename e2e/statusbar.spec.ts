import { dialog, editor, expect, openApp, openConsole, test, treeItem, typeSql, work } from "./app";
import type { Page } from "@playwright/test";

// Status bar items (#132): each its own segment, Position carrying the selection, the optional Not null / Sum / Avg / Min / Max / Size,
// chosen from the status bar's right-click checklist or Settings › Appearance › Status bar (one setting, settings.json — the mock's)

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
});

const bar = (page: Page) => page.locator(".statusbar");
/** The status bar's segments as they read, left to right (name: value) */
const segments = (page: Page) => bar(page).locator(".sb-seg").allTextContents();
const menu = (page: Page) => page.getByRole("menu", { name: "Status bar items" });
const check = (page: Page, name: string) => menu(page).getByRole("menuitemcheckbox", { name: new RegExp(`^✓? ?${name}\\b`) });
const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });

test("Position carries the selection: one cell, a range of rows, several columns; no hover title on the items", async ({ page }) => {
  await cell(page, "user_3").click();
  await expect.poll(() => segments(page)).toEqual(["Row 3, Col 2"]);
  await cell(page, "user_7").click({ modifiers: ["Shift"] });
  await expect.poll(() => segments(page)).toEqual(["Rows 3–7 (5) · Col 2"]);
  // Not right after the last ⇧: ⇧⇧ is Search Everywhere
  await page.waitForTimeout(600);
  await cell(page, "7407.00").click({ modifiers: ["Shift"] });
  await expect.poll(() => segments(page)).toEqual(["Rows 3–7 (5) · Cols 2–3"]);
  // ⌘-click a row far away: rows that aren't one range are counted
  await cell(page, "user_12").click({ modifiers: ["Meta"] });
  await expect.poll(() => segments(page)).toEqual(["6 rows · Cols 2–3"]);
  // No title pops over the items (the update button and the status text keep theirs)
  for (const it of await bar(page).locator(".sb-seg .ui-status-item").all()) expect(await it.getAttribute("title")).toBeNull();
  await expect(bar(page).locator("[title]")).toHaveCount(2);
});

test("right-click the bar: the checklist turns Not null and Sum on, stays open, keeps the choice after a restart; Esc gives the grid back", async ({ page }) => {
  // amount of rows 1–4: four numbers
  await work(page).getByRole("gridcell", { name: "0.00", exact: true }).click();
  await work(page).getByRole("gridcell", { name: "3703.50", exact: true }).click({ modifiers: ["Shift"] });
  await expect(bar(page)).toContainText("Rows 1–4 (4) · Col 3");
  await bar(page).click({ button: "right", position: { x: 400, y: 10 } });
  await expect(menu(page)).toBeFocused();
  await expect(check(page, "Position")).toHaveAttribute("aria-checked", "true");
  await expect(check(page, "Not null")).toHaveAttribute("aria-checked", "false");
  // ↑↓ and Space, then a click: both stay in the open list
  await page.keyboard.press("Space");
  await check(page, "Sum").click();
  await expect(menu(page)).toBeVisible();
  await expect(check(page, "Not null")).toHaveAttribute("aria-checked", "true");
  await expect(check(page, "Sum")).toHaveAttribute("aria-checked", "true");
  // The grid's items stay while the list has the focus
  await expect.poll(() => segments(page)).toEqual(["Not null:4", "Sum:7,407", "Rows 1–4 (4) · Col 3"]);
  await page.keyboard.press("Escape");
  await expect(menu(page)).toHaveCount(0);
  // Focus back in the grid: its keys work, and the items follow it
  await page.keyboard.press("Shift+ArrowDown");
  await expect(bar(page)).toContainText("Rows 1–5 (5) · Col 3");
  await expect.poll(() => segments(page)).toEqual(["Not null:5", "Sum:12,345", "Rows 1–5 (5) · Col 3"]);
  // Kept (settings.json): after a restart the same items are on
  await page.reload();
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("gridcell", { name: "0.00", exact: true }).click();
  await work(page).getByRole("gridcell", { name: "1234.50", exact: true }).click({ modifiers: ["Shift"] });
  await expect.poll(() => segments(page)).toEqual(["Not null:2", "Sum:1,234.5", "Rows 1–2 (2) · Col 3"]);
  // Clicking an item opens the list too; Restore Defaults
  await bar(page).locator(".sb-seg").first().click();
  await menu(page).getByRole("menuitem", { name: "Restore Defaults" }).click();
  await expect.poll(() => segments(page)).toEqual(["Rows 1–2 (2) · Col 3"]);
});

test("Settings › Appearance › Status bar changes the same setting as the checklist", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const d = dialog(page);
  await d.getByRole("checkbox", { name: "Not null" }).check();
  await d.getByRole("checkbox", { name: "Position" }).uncheck();
  await d.getByRole("button", { name: "OK" }).click();
  await cell(page, "user_3").click();
  await cell(page, "user_4").click({ modifiers: ["Shift"] });
  await expect.poll(() => segments(page)).toEqual(["Not null:2"]);
  await bar(page).click({ button: "right", position: { x: 400, y: 10 } });
  await expect(check(page, "Not null")).toHaveAttribute("aria-checked", "true");
  await expect(check(page, "Position")).toHaveAttribute("aria-checked", "false");
  // The other way: the checklist's change shows in Settings
  await check(page, "Position").click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Comma");
  await expect(d.getByRole("checkbox", { name: "Position" })).toBeChecked();
  await d.getByRole("button", { name: "Restore Defaults" }).click();
  await expect(d.getByRole("checkbox", { name: "Not null" })).not.toBeChecked();
});

test("Size: a varchar cell in a table tab has its bytes and the column's limit; a console result's cell the bytes only; a number none", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("mock-statusBarItems", JSON.stringify(["size", "position"])));
  await page.reload();
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await cell(page, "user_3").click();
  await expect(bar(page)).toContainText("varchar(50):");
  await expect.poll(() => segments(page)).toEqual(["Size:6 bytes", "varchar(50):up to 50 chars (200 bytes in UTF-8)", "Row 3, Col 2"]);
  await page.keyboard.press("ArrowLeft");
  await expect.poll(() => segments(page)).toEqual(["Row 3, Col 1"]);
  // A console result not mapped to a table: the bytes only
  await openConsole(page);
  await typeSql(page, "select * from users where id < 10");
  await page.keyboard.press("Meta+Enter");
  await cell(page, "user_3").click();
  await expect.poll(() => segments(page)).toEqual(["Size:6 bytes", "Row 3, Col 2"]);
  await editor(page).click();
  await expect(bar(page).locator(".sb-seg")).toHaveCount(0);
});

test("a narrow window drops the optional items, then keeps Position (and Read-only)", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("mock-statusBarItems", JSON.stringify(["notNull", "sum", "avg", "min", "max", "size", "shownColumns", "position"])));
  await page.reload();
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("gridcell", { name: "0.00", exact: true }).click();
  await work(page).getByRole("gridcell", { name: "3703.50", exact: true }).click({ modifiers: ["Shift"] });
  await expect.poll(() => segments(page)).toEqual(["Not null:4", "Sum:7,407", "Avg:1,851.75", "Min:0", "Max:3,703.5", "Rows 1–4 (4) · Col 3"]);
  await expect(bar(page)).toContainText("UTF-8");
  await page.setViewportSize({ width: 560, height: 900 });
  await expect(bar(page)).not.toContainText("UTF-8");
  await expect(bar(page).locator(".sb-seg").last()).toHaveText("Rows 1–4 (4) · Col 3");
  await expect.poll(() => segments(page)).not.toContain("Max:3,703.5");
  // Nothing spills out of the bar
  expect(await bar(page).evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  await page.setViewportSize({ width: 1400, height: 900 });
  await expect(bar(page)).toContainText("UTF-8");
  await expect.poll(() => segments(page)).toContain("Max:3,703.5");
});
