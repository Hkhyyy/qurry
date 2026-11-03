import type { Locator, Page } from "@playwright/test";
import { dialog, editor, expect, openApp, openConsole, test, treeItem, work } from "./app";

// Courier New ships with macOS, Windows and most Linux desktops (the specs run as on macOS)
const FONT = "Courier New";
const style = (l: Locator, prop: "fontFamily" | "fontSize") => l.evaluate((el, p) => getComputedStyle(el)[p], prop);
const cell = (page: Page) => work(page).getByRole("gridcell", { name: "user_1", exact: true });
const field = (page: Page, name: string) => dialog(page).getByRole("combobox", { name });
/** The open font list (popup under a font field) */
const fontList = (page: Page) => dialog(page).locator(".font-picker-pop");

test.beforeEach(async ({ page }) => openApp(page));

/** Open a font picker's list and pick a row by its name */
async function pick(page: Page, picker: string, option: string) {
  await field(page, picker).click();
  await fontList(page).getByRole("option", { name: option, exact: true }).click();
  await expect(fontList(page)).toHaveCount(0);
}

test("Settings › Grid font: a family picked from the list and the size apply to the grid live and are kept after a restart", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await expect(cell(page)).toBeVisible();
  const before = await style(cell(page), "fontFamily");
  expect(before).toContain("JetBrains Mono");
  expect(await style(cell(page), "fontSize")).toBe("12px");

  await page.keyboard.press("Meta+Comma");
  await expect(field(page, "Grid font family")).toHaveText(/Default monospace/);
  await pick(page, "Grid font family", FONT);
  await field(page, "Grid font size").selectOption("14");
  await expect.poll(() => style(cell(page), "fontFamily")).toMatch(new RegExp(`^"?${FONT}"?, .*monospace$`));
  expect(await style(cell(page), "fontSize")).toBe("14px");
  // The field shows the family in itself; installed: no warning
  await expect(field(page, "Grid font family")).toHaveText(new RegExp(`^${FONT}`));
  await expect.poll(() => style(field(page, "Grid font family").locator(".font-picker-name"), "fontFamily")).toContain(FONT);
  await expect(dialog(page).getByText("(not installed)")).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await treeItem(page, "users").first().dblclick();
  await expect.poll(() => style(cell(page), "fontFamily")).toContain(FONT);
  expect(await style(cell(page), "fontSize")).toBe("14px");
  await page.keyboard.press("Meta+Comma");
  await expect(field(page, "Grid font family")).toHaveText(new RegExp(`^${FONT}`));
  await expect(field(page, "Grid font size")).toHaveValue("14");

  // The default row goes back to the default
  await pick(page, "Grid font family", "Default monospace");
  await expect.poll(() => style(cell(page), "fontFamily")).toBe(before);
});

test("Settings › Editor font: picked with the keyboard (type to search, ↩), applies to open editors; ⌃⇧. still changes the size", async ({ page }) => {
  await openConsole(page);
  const lines = editor(page).locator(".view-lines");
  await page.keyboard.press("Meta+Comma");
  await field(page, "Editor font family").focus();
  // Typing on the field opens the list with that text in its search field
  await page.keyboard.type("cour");
  await expect(dialog(page).getByLabel("Search editor font family")).toHaveValue("cour");
  await expect(fontList(page).getByRole("option")).toHaveText([FONT]);
  await page.keyboard.press("Enter");
  await expect.poll(() => style(lines, "fontFamily")).toContain(FONT);
  // Picked: the list is closed, the field has the focus and the dialog is still open
  await expect(fontList(page)).toHaveCount(0);
  await expect(field(page, "Editor font family")).toBeFocused();
  // ⎋ in the list closes the list only
  await page.keyboard.press("ArrowDown");
  await expect(fontList(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(fontList(page)).toHaveCount(0);
  await expect(dialog(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);

  await editor(page).click();
  await page.keyboard.press("Control+Shift+Period");
  await expect.poll(() => style(lines, "fontSize")).toBe("14px");
  await page.keyboard.press("Meta+Comma");
  await expect(field(page, "Editor font size")).toHaveValue("14");
});

test("Settings › font list: installed monospaced families, the proportional ones with Monospaced only off", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await field(page, "Grid font family").click();
  const options = fontList(page).getByRole("option");
  await expect(options).toHaveText(["Default monospace", "Andale Mono", FONT, "Menlo", "Monaco", "PT Mono"]);
  await expect(fontList(page).getByRole("option", { selected: true })).toHaveText("Default monospace");
  await dialog(page).getByText("Monospaced only").click();
  await expect(options).toHaveText(["Default monospace", "Andale Mono", FONT, "Helvetica", "Menlo", "Monaco", "PT Mono"]);
  // The popup keeps its height whatever the search matches
  const pop = fontList(page);
  const h = (await pop.boundingBox())?.height;
  await dialog(page).getByLabel("Search grid font family").fill("men");
  await expect(options).toHaveText(["Menlo"]);
  expect((await pop.boundingBox())?.height).toBe(h);
  await dialog(page).getByLabel("Search grid font family").fill("zzz");
  await expect(dialog(page).getByText("No matching font")).toBeVisible();
  expect((await pop.boundingBox())?.height).toBe(h);
});

test("Settings › saved font that isn't installed: still loads, on top of the list marked (not installed), the grid uses the fallback", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("dbtool.gridFontFamily", "No Such Mono"));
  await page.reload();
  await treeItem(page, "users").first().dblclick();
  await expect(cell(page)).toBeVisible();
  // The saved family stays first in the stack; the browser falls back to the default monospace stack after it
  expect(await style(cell(page), "fontFamily")).toMatch(/^"?No Such Mono"?, "?JetBrains Mono"?/);
  await page.keyboard.press("Meta+Comma");
  await expect(field(page, "Grid font family")).toHaveText(/^No Such Mono\(not installed\)/);
  await field(page, "Grid font family").click();
  await expect(fontList(page).getByRole("option").nth(1)).toHaveText("No Such Mono(not installed)");
  await expect(fontList(page).getByRole("option", { selected: true })).toHaveText(/No Such Mono/);
  await fontList(page).getByRole("option", { name: "Menlo", exact: true }).click();
  await expect(dialog(page).getByText("(not installed)")).toHaveCount(0);
  await expect.poll(() => style(cell(page), "fontFamily")).toContain("Menlo");
});
