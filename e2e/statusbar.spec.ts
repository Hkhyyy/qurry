import { dialog, editor, expect, openApp, openConsole, openSettings, test, treeItem, typeSql, work } from "./app";
import type { Page } from "@playwright/test";

// Status bar items (#132): each its own segment, Position carrying the selection, the optional Not null / Sum / Avg / Min / Max / Size,
// chosen from the status bar's right-click checklist or Settings › Appearance › Data grid › Status bar (one setting, settings.json — the mock's)

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
  // Another key between the two ⇧ (⇧⇧ is Search Everywhere)
  await page.keyboard.press("Control");
  await cell(page, "7407.00").click({ modifiers: ["Shift"] });
  await expect.poll(() => segments(page)).toEqual(["Rows 3–7 (5) · Cols 2–3"]);
  // ⌘-click a row far away: rows that aren't one range are counted
  await cell(page, "user_12").click({ modifiers: ["Meta"] });
  await expect.poll(() => segments(page)).toEqual(["6 rows · Cols 2–3"]);
  // No title pops over the status items (only the update button has one)
  await expect(bar(page).locator(".ui-status-item:not(.accent)[title], .ui-status-item:not(.accent) [title]")).toHaveCount(0);
});

test("right-click the bar: the checklist turns Not null and Sum on, stays open, keeps the choice after a restart; Esc gives the grid back", async ({ page }) => {
  // amount of rows 1–4: four numbers
  await work(page).getByRole("gridcell", { name: "0.00", exact: true }).click();
  await work(page).getByRole("gridcell", { name: "3703.50", exact: true }).click({ modifiers: ["Shift"] });
  await expect(bar(page)).toContainText("Rows 1–4 (4) · Col 3");
  await bar(page).click({ button: "right", position: { x: 400, y: 10 } });
  await expect(menu(page)).toBeFocused();
  // Above the bar, inside the window
  const [m, b] = [(await menu(page).boundingBox())!, (await bar(page).boundingBox())!];
  expect(m.y + m.height).toBeLessThanOrEqual(b.y);
  expect(m.x).toBeGreaterThanOrEqual(0);
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
  // End, Home, then ⇥ closes as ⎋ does
  await page.keyboard.press("End");
  await expect(menu(page)).toHaveAttribute("aria-activedescendant", /-8$/);
  await page.keyboard.press("Home");
  await expect(menu(page)).toHaveAttribute("aria-activedescendant", /-0$/);
  await page.keyboard.press("Tab");
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

test("the checklist opened near the window's right edge stays inside it and above the bar", async ({ page }) => {
  await cell(page, "user_3").click();
  const b = (await bar(page).boundingBox())!;
  const pos = (await bar(page).locator(".sb-seg .ui-status-item").last().boundingBox())!;
  await page.mouse.click(pos.x + pos.width / 2, pos.y + pos.height / 2, { button: "right" });
  const m = (await menu(page).boundingBox())!;
  expect(m.x + m.width).toBeLessThanOrEqual(1400 - 4);
  expect(m.x).toBeGreaterThanOrEqual(4);
  expect(m.y).toBeGreaterThanOrEqual(0);
  expect(m.y + m.height).toBeLessThanOrEqual(b.y);
  // The grid's items stay on the bar while it is open
  await expect.poll(() => segments(page)).toEqual(["Row 3, Col 2"]);
});

test("Settings › Appearance › Data grid › Status bar changes the same setting as the checklist", async ({ page }) => {
  await openSettings(page, "Data grid");
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
