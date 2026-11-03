import type { Locator, Page } from "@playwright/test";
import { dialog, editor, expect, openApp, openConsole, test, treeItem, work } from "./app";

// Courier New ships with macOS, Windows and most Linux desktops (the specs run as on macOS)
const FONT = "Courier New";
const style = (l: Locator, prop: "fontFamily" | "fontSize") => l.evaluate((el, p) => getComputedStyle(el)[p], prop);
const cell = (page: Page) => work(page).getByRole("gridcell", { name: "user_1", exact: true });
const field = (page: Page, name: string) => dialog(page).getByRole("combobox", { name });

test.beforeEach(async ({ page }) => openApp(page));

test("Settings › Grid font: family and size apply to the grid live and are kept after a restart", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await expect(cell(page)).toBeVisible();
  const before = await style(cell(page), "fontFamily");
  expect(before).toContain("JetBrains Mono");
  expect(await style(cell(page), "fontSize")).toBe("12px");

  await page.keyboard.press("Meta+Comma");
  await field(page, "Grid font family").fill(FONT);
  await field(page, "Grid font size").selectOption("14");
  await expect.poll(() => style(cell(page), "fontFamily")).toMatch(new RegExp(`^"?${FONT}"?, .*monospace$`));
  expect(await style(cell(page), "fontSize")).toBe("14px");
  // Installed: no warning
  await expect(dialog(page).getByText("isn't installed")).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await treeItem(page, "users").first().dblclick();
  await expect.poll(() => style(cell(page), "fontFamily")).toContain(FONT);
  expect(await style(cell(page), "fontSize")).toBe("14px");
  await page.keyboard.press("Meta+Comma");
  await expect(field(page, "Grid font family")).toHaveValue(FONT);
  await expect(field(page, "Grid font size")).toHaveValue("14");

  // Clearing the field goes back to the default
  await field(page, "Grid font family").fill("");
  await expect.poll(() => style(cell(page), "fontFamily")).toBe(before);
});

test("Settings › Editor font: applies to open editors; ⌃⇧. still changes the size and Settings shows it", async ({ page }) => {
  await openConsole(page);
  const lines = editor(page).locator(".view-lines");
  await page.keyboard.press("Meta+Comma");
  await field(page, "Editor font family").fill(FONT);
  await expect.poll(() => style(lines, "fontFamily")).toContain(FONT);
  await page.keyboard.press("Escape");

  await editor(page).click();
  await page.keyboard.press("Control+Shift+Period");
  await expect.poll(() => style(lines, "fontSize")).toBe("14px");
  await page.keyboard.press("Meta+Comma");
  await expect(field(page, "Editor font size")).toHaveValue("14");
});

test("Settings › font that isn't installed: falls back to monospace and says how to recover", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const updates = dialog(page).getByText("Check for updates automatically");
  const y = async () => (await updates.boundingBox())?.y;
  const top = await y();
  await field(page, "Grid font family").fill("No Such Mono");
  await expect(dialog(page).getByText(`"No Such Mono" isn't installed, so the default monospace font is used. Pick a font from the list, or clear the field for the default.`)).toBeVisible();
  // The dialog keeps its size, and the fields below the font fields stay put while the warning comes and goes
  const box = await dialog(page).boundingBox();
  expect(await y()).toBe(top);
  await field(page, "Editor font family").fill("No Such Mono");
  await expect(dialog(page).getByText("isn't installed")).toHaveCount(2);
  expect(await y()).toBe(top);
  await field(page, "Editor font family").fill("");
  await field(page, "Grid font family").fill("");
  await expect(dialog(page).getByText("isn't installed")).toHaveCount(0);
  expect(await dialog(page).boundingBox()).toEqual(box);
  expect(await y()).toBe(top);
  // A pasted stack is one family name, which isn't installed either: the warning says so instead of silently using the default
  await field(page, "Grid font family").fill(`${FONT}, monospace`);
  await expect(dialog(page).getByText(`"${FONT}, monospace" isn't installed`)).toBeVisible();
  await field(page, "Grid font family").fill("");
});

test("Settings › font list: suggests the installed monospace fonts", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await expect(page.locator(`#settings-fonts option[value="${FONT}"]`)).toHaveCount(1);
});
