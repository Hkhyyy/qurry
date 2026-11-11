import type { Locator, Page } from "@playwright/test";
import { dialog, editor, expect, openApp, openConsole, openSettings, settingsRow, settingsTree, test, treeItem, work } from "./app";

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

/** A confirm or notice box over Settings, by its text */
const box = (page: Page, text: string) => page.getByRole("dialog").filter({ hasText: text });
const settingsField = (page: Page, name: string) => page.locator(".settings").getByRole("combobox", { name });

test("Settings › Add Font File…: registered and picked, listed as custom in the other picker, removing it sets both back to the default", async ({ page }) => {
  // The mock adds "Fira Code" (Monaco's icon font as its bytes)
  const registered = () => page.evaluate(() => [...document.fonts].filter((f) => f.family.replace(/"/g, "") === "Fira Code" && f.status === "loaded").length);
  await openConsole(page);
  const lines = editor(page).locator(".view-lines");
  const before = await style(lines, "fontFamily");
  await page.keyboard.press("Meta+Comma");
  await field(page, "Editor font family").click();
  await fontList(page).getByRole("button", { name: "Add Font File…" }).click();
  await expect(fontList(page)).toHaveCount(0);
  await expect(field(page, "Editor font family")).toHaveText(/^Fira Code/);
  await expect(field(page, "Editor font family")).toBeFocused();
  await expect.poll(registered).toBe(1);
  await expect.poll(() => style(lines, "fontFamily")).toMatch(/^"?Fira Code"?, /);
  await page.keyboard.press("Escape");

  // In the grid picker: first after the default, marked custom, with Monospaced only on (it was added on purpose)
  await treeItem(page, "users").first().dblclick();
  await expect(cell(page)).toBeVisible();
  const gridBefore = await style(cell(page), "fontFamily");
  await page.keyboard.press("Meta+Comma");
  await field(page, "Grid font family").click();
  await expect(fontList(page).getByRole("option").nth(1)).toHaveText("Fira Codecustom");
  await fontList(page).getByRole("option").nth(1).click();
  await expect.poll(() => style(cell(page), "fontFamily")).toMatch(/^"?Fira Code"?, /);

  // Remove: asks first (Cancel keeps it)
  await field(page, "Grid font family").click();
  await fontList(page).getByRole("button", { name: "Remove font file Fira Code" }).click();
  const ask = box(page, "Remove Font File");
  await expect(ask).toContainText('Remove the font file "Fira Code" from Qurry?');
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(ask).toHaveCount(0);
  await expect(settingsField(page, "Grid font family")).toHaveText(/^Fira Code/);
  await settingsField(page, "Grid font family").click();
  await fontList(page).getByRole("button", { name: "Remove font file Fira Code" }).click();
  await ask.getByRole("button", { name: "Remove" }).click();
  await expect(ask).toHaveCount(0);
  await expect(settingsField(page, "Grid font family")).toHaveText(/^Default monospace/);
  await expect(settingsField(page, "Editor font family")).toHaveText(/^Default monospace/);
  await expect(settingsField(page, "Grid font family")).toBeFocused();
  await expect.poll(registered).toBe(0);
  await expect.poll(() => style(cell(page), "fontFamily")).toBe(gridBefore);
  await settingsField(page, "Grid font family").click();
  await expect(fontList(page).getByText("custom")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.getByRole("tab", { name: /^console / }).click();
  await expect.poll(() => style(editor(page).locator(".view-lines"), "fontFamily")).toBe(before);
});

test("Settings › Add Font File…: a file that isn't a font is refused with the reason, nothing changes", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFontAddFails = true));
  await page.keyboard.press("Meta+Comma");
  await field(page, "Grid font family").click();
  await fontList(page).getByRole("button", { name: "Add Font File…" }).click();
  const notice = box(page, "Add Font File");
  await expect(notice).toContainText("notes.ttf isn't a font file (TrueType, OpenType, WOFF or WOFF2)");
  await notice.getByRole("button", { name: "OK" }).click();
  await expect(notice).toHaveCount(0);
  await expect(settingsField(page, "Grid font family")).toHaveText(/^Default monospace/);
  await expect(settingsField(page, "Grid font family")).toBeFocused();
});

test("Settings › Add Font File…: a font file that can't be loaded (damaged) is removed again with the reason, nothing is picked", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockFontBroken = true));
  await page.keyboard.press("Meta+Comma");
  await field(page, "Grid font family").click();
  await fontList(page).getByRole("button", { name: "Add Font File…" }).click();
  const notice = box(page, "Add Font File");
  await expect(notice).toContainText("Fira Code couldn't be loaded as a font");
  await notice.getByRole("button", { name: "OK" }).click();
  await expect(notice).toHaveCount(0);
  await expect(settingsField(page, "Grid font family")).toHaveText(/^Default monospace/);
  await expect(settingsField(page, "Grid font family")).toBeFocused();
  await settingsField(page, "Grid font family").click();
  await expect(fontList(page).getByText("custom")).toHaveCount(0);
});

// #152: the pages tree (DataGrip's settings tree). Each page shows only its own fields; the dialog keeps one size
const PAGES: [string, string[]][] = [
  ["Appearance", ["Theme", "Editor font", "Grid font"]],
  ["Console", ["Keyword case"]],
  ["Data grid", ["Alignment", "Status bar"]],
  ["Updates", ["Updates", "Channel"]],
  ["Keymap", []],
  ["Passwords", ["Key"]],
];
const labels = (page: Page) => dialog(page).locator(".settings-form .ui-field-label");
const rows = (page: Page) => settingsTree(page).getByRole("treeitem");

test("Settings › pages tree: each page shows exactly its fields, the label column is 108 px and the dialog keeps one size", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await expect(rows(page)).toHaveText(["Appearance", "Console", "Data grid", "Updates", "Keymap", "Passwords"]);
  let box: { width: number; height: number; x: number; y: number } | null = null;
  for (const [name, fields] of PAGES) {
    await settingsRow(page, name).click();
    await expect(settingsRow(page, name)).toHaveAttribute("aria-selected", "true");
    await expect(settingsTree(page).locator("[aria-selected=true]")).toHaveCount(1);
    await expect(labels(page)).toHaveText(fields);
    if (fields.length) expect((await labels(page).first().boundingBox())?.width).toBe(108);
    const b = await page.locator(".ui-modal.settings").boundingBox();
    if (box) expect(b).toEqual(box);
    box = b;
  }
  // Keymap is the keymap editor; Updates has its checkbox and buttons, the GitHub one icon-only
  await settingsRow(page, "Keymap").click();
  await expect(dialog(page).locator(".keymap-editor")).toBeVisible();
  await settingsRow(page, "Updates").click();
  const d = dialog(page);
  await expect(d.getByRole("checkbox", { name: "Check for updates automatically" })).toBeVisible();
  await expect(d.getByRole("button", { name: "Check Now" })).toBeVisible();
  await expect(d.getByRole("button", { name: "Open GitHub" })).toHaveText("");
  await expect(d.getByRole("button", { name: "Open GitHub" })).toHaveAttribute("title", "Open GitHub");
});

test("Settings › Updates › Open GitHub opens the project page (window.open where the opener plugin isn't there)", async ({ page }) => {
  await page.evaluate(() => {
    (window as any).__opened = [];
    window.open = (u?: string | URL) => ((window as any).__opened.push(String(u)), null);
  });
  await openSettings(page, "Updates");
  await dialog(page).getByRole("button", { name: "Open GitHub" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__opened)).toEqual(["https://github.com/Hkhyyy/qurry"]);
  await expect(dialog(page)).toBeVisible();
});

test("Settings › pages tree keys: ↑↓ move, → opens then goes to the first child, ← goes to the parent then closes it, Home / End", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const tree = settingsTree(page);
  await tree.focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(settingsRow(page, "Data grid")).toHaveAttribute("aria-selected", "true");
  await expect(labels(page)).toHaveText(["Alignment", "Status bar"]);
  // Focused from the keyboard: the ring shows
  expect(await tree.evaluate((e) => getComputedStyle(e).boxShadow)).not.toBe("none");
  await page.keyboard.press("ArrowUp");
  await expect(labels(page)).toHaveText(["Keyword case"]);
  await page.keyboard.press("ArrowLeft");
  await expect(settingsRow(page, "Appearance")).toHaveAttribute("aria-selected", "true");
  await expect(settingsRow(page, "Appearance")).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("ArrowLeft");
  await expect(settingsRow(page, "Appearance")).toHaveAttribute("aria-expanded", "false");
  await expect(rows(page)).toHaveText(["Appearance", "Updates", "Keymap", "Passwords"]);
  await page.keyboard.press("ArrowRight");
  await expect(rows(page)).toHaveText(["Appearance", "Console", "Data grid", "Updates", "Keymap", "Passwords"]);
  await expect(settingsRow(page, "Appearance")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowRight");
  await expect(settingsRow(page, "Console")).toHaveAttribute("aria-selected", "true");
  await expect(labels(page)).toHaveText(["Keyword case"]);
  await page.keyboard.press("End");
  await expect(settingsRow(page, "Passwords")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Home");
  await expect(settingsRow(page, "Appearance")).toHaveAttribute("aria-selected", "true");
  // The keys stay in the tree (the dialog and the focus with it); ⎋ closes the dialog
  await expect(tree).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("Settings opens on the last page used: Keyword case set to UPPER on Console, after a restart Settings opens on Console", async ({ page }) => {
  await openSettings(page, "Console");
  await dialog(page).getByRole("combobox", { name: "Keyword case" }).selectOption("upper");
  await page.keyboard.press("Escape");
  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await page.keyboard.press("Meta+Comma");
  await expect(settingsRow(page, "Console")).toHaveAttribute("aria-selected", "true");
  await expect(dialog(page).getByRole("combobox", { name: "Keyword case" })).toHaveValue("upper");
});
