import type { Page } from "@playwright/test";
import TABLE from "../crates/shared/src/themes.json" with { type: "json" };
import { dialog, expect, openApp, openConsole, openInTheme, test, typeSql, work } from "./app";

// Themes (#103): the seven built-in ones, custom themes made in Settings › Appearance › Theme › Edit Colors…, Sync with OS with a dark
// and a light theme of the user's choice, Quick Switch Theme. The pick and the custom themes live in settings.json, shared with the TUI
// (the mock keeps it in localStorage "mock-themeSettings")

const rgb = (h: string) => `rgb(${[1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ")})`;
const theme = (id: string) => TABLE.themes.find((t) => t.id === id)!;
/** What the window draws: its background, the explorer's panel, data-theme */
const drawn = (page: Page) =>
  page.evaluate(() => ({ bg: getComputedStyle(document.documentElement).backgroundColor, panel: getComputedStyle(document.querySelector(".panel")!).backgroundColor, kind: document.documentElement.dataset.theme }));
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem("mock-themeSettings") ?? "null"));
const settings = async (page: Page) => {
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page)).toBeVisible();
};
const themeSelect = (page: Page) => dialog(page).getByRole("combobox", { name: "Theme", exact: true });

test("each built-in theme is picked in Settings, drawn at once and saved for the TUI", async ({ page }) => {
  await openApp(page);
  await settings(page);
  for (const t of TABLE.themes) {
    await themeSelect(page).selectOption(t.id);
    await expect.poll(() => drawn(page)).toEqual({ bg: rgb(t.colors.bg), panel: rgb(t.colors.panel), kind: t.kind });
    expect((await saved(page)).theme).toBe(t.id);
  }
  // Read again at the next start
  await page.reload();
  await expect.poll(() => drawn(page)).toMatchObject({ bg: rgb(theme("solarized-light").colors.bg), kind: "light" });
});

// #103: the pgsql tokenizer's string.sql / predefined.sql were drawn #FF0000 / #FF00FF by the base Monaco themes
for (const id of ["dark", "light", "dracula", "solarized-light"]) {
  test(`a PostgreSQL console draws 'x' and upper() in the theme's colors (${id})`, async ({ page }) => {
    await openInTheme(page, id);
    await openConsole(page);
    await typeSql(page, "select upper(a) from t where a = 'x'");
    const color = (text: string) => work(page).locator(".monaco-editor .view-line span span", { hasText: text }).first().evaluate((el) => getComputedStyle(el).color);
    await expect.poll(() => color("'x'")).toBe(rgb(theme(id).colors.string));
    expect(await color("upper")).toBe(rgb(theme(id).colors.function));
    expect([await color("'x'"), await color("upper")]).not.toContain("rgb(255, 0, 0)");
    expect(await color("upper")).not.toBe("rgb(255, 0, 255)");
  });
}

test("Edit Colors…: live preview, contrast warnings kept, an invalid hex not applied, outlines on hover, saved as a custom theme", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("combobox", { name: "Based on" }).selectOption("dracula");
  // The whole window draws the theme being edited
  await expect.poll(() => drawn(page)).toMatchObject({ bg: rgb(theme("dracula").colors.bg) });
  const hex = (label: string) => dialog(page).getByRole("textbox", { name: `${label} hex`, exact: true });
  await hex("Background, editor").fill("#101018");
  await expect.poll(() => drawn(page)).toMatchObject({ bg: "rgb(16, 16, 24)" });
  // Not a color: marked, not applied
  await hex("Background, editor").fill("#10101");
  await expect(hex("Background, editor")).toHaveAttribute("aria-invalid", "true");
  expect((await drawn(page)).bg).toBe("rgb(16, 16, 24)");
  await hex("Background, editor").blur();
  await expect(hex("Background, editor")).toHaveValue("#101018");
  // Too dark to read: warned next to the color and in the header, kept as typed
  await hex("Comment").fill("#2a2b33");
  const row = dialog(page).locator('[data-token="comment"]');
  await expect(row.locator(".theme-warn")).toHaveText(/⚠ 1\.\d:1 on Background/);
  await expect(dialog(page).locator(".theme-editor-head")).toContainText("1 below the contrast minimum");
  await expect(hex("Comment")).toHaveValue("#2a2b33");
  // Pointing at a color's name outlines what draws it in the preview (the comment in the console sample)
  await row.locator(".theme-token-label").hover();
  await expect(dialog(page).locator(".theme-preview .tok-hot").first()).toBeVisible();
  await expect(dialog(page).locator(".theme-preview .monaco-editor .tok-hot").first()).toHaveText(/users\sto\scheck/);
  await page.mouse.move(0, 0);
  await expect(dialog(page).locator(".theme-preview .tok-hot")).toHaveCount(0);
  // Advanced colors: the rest, with "Row numbers" for the row-number column
  await dialog(page).getByRole("button", { name: /Advanced colors/ }).click();
  await expect(dialog(page).locator('[data-token="panel2"] .theme-token-name')).toHaveText("Row numbers");
  await dialog(page).getByRole("textbox", { name: "Theme name" }).fill("Night shift");
  await dialog(page).getByRole("button", { name: "Save as new theme" }).click();
  await expect(dialog(page).getByText(/Saved "Night shift" and switched to it \(1 below the contrast minimum/)).toBeVisible();
  expect(await saved(page)).toEqual({ theme: "custom-1", sync: { dark: "dark", light: "light" }, custom: [{ id: "custom-1", name: "Night shift", base: "dracula", colors: { bg: "#101018", comment: "#2a2b33" } }] });
  // Save replaces it (same id); nothing left unsaved
  await expect(dialog(page).getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  await hex("Accent").fill("#9580ff");
  await dialog(page).getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(async () => (await saved(page)).custom).toEqual([{ id: "custom-1", name: "Night shift", base: "dracula", colors: { accent: "#9580ff", bg: "#101018", comment: "#2a2b33" } }]);
  await expect(dialog(page).getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  // Back on Appearance: the picker lists it under Custom; closing keeps it
  await dialog(page).getByRole("button", { name: "‹ Appearance" }).click();
  await expect(themeSelect(page)).toHaveValue("custom-1");
  await expect(themeSelect(page).locator("optgroup[label=Custom] option")).toHaveText(["Night shift"]);
  await page.keyboard.press("Escape");
  await expect.poll(() => drawn(page)).toMatchObject({ bg: "rgb(16, 16, 24)", kind: "dark" });
});

test("leaving the editor with unsaved colors asks; discarding goes back to the theme picked", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("textbox", { name: "Text hex", exact: true }).fill("#ff0000");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Theme Colors" }).or(page.locator(".ui-modal", { hasText: "Discard them?" }))).toBeVisible();
  await page.getByRole("button", { name: "Discard" }).click();
  await expect(dialog(page)).toHaveCount(0);
  const text = await page.evaluate(() => getComputedStyle(document.body).color);
  expect(text).toBe(rgb(theme("dark").colors.text));
});

test("Import… / Export… a theme file: checked strictly, loaded into the editor to save", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  const message = dialog(page).locator(".theme-editor-actions .ui-notice");
  // Refused, with why: a bad hex, an unknown color, a file too large
  for (const [file, why] of [
    ['{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "red"}}', /"bg" is "red": colors are written #rrggbb/],
    ['{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"glow": "#123456"}}', /Unknown color "glow"/],
    ["x".repeat(20000), /at most 16 KB/],
  ] as const) {
    await page.evaluate((f) => ((window as any).__mockThemeFile = f), file);
    await dialog(page).getByRole("button", { name: "Import…" }).click();
    await expect(message).toHaveText(why);
  }
  await page.evaluate(() => ((window as any).__mockThemeFile = JSON.stringify({ qurryTheme: 1, name: "Paper", base: "solarized-light", colors: { text: "#3D4F55" } })));
  await dialog(page).getByRole("button", { name: "Import…" }).click();
  await expect(message).toHaveText(/Imported "Paper" \(based on Solarized Light\)/);
  await expect(dialog(page).getByRole("textbox", { name: "Theme name" })).toHaveValue("Paper");
  await expect(dialog(page).getByRole("textbox", { name: "Text hex", exact: true })).toHaveValue("#3d4f55");
  await expect.poll(() => drawn(page)).toMatchObject({ bg: rgb(theme("solarized-light").colors.bg), kind: "light" });
  // Not saved until asked
  expect((await saved(page))?.custom ?? []).toEqual([]);
  await dialog(page).getByRole("button", { name: "Export…" }).click();
  await expect(message).toHaveText("Exported to /Users/me/Downloads/Paper.qurry-theme.json");
  expect(JSON.parse(await page.evaluate(() => (window as any).__mockThemeExported))).toEqual({ qurryTheme: 1, name: "Paper", base: "solarized-light", colors: { text: "#3d4f55" } });
});

test("Sync with OS: the dark and the light theme picked follow the system appearance", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("checkbox", { name: "Sync with OS" }).check();
  await dialog(page).getByRole("combobox", { name: "Theme for a dark appearance" }).selectOption("one-dark");
  await dialog(page).getByRole("combobox", { name: "Theme for a light appearance" }).selectOption("solarized-light");
  // Dark offers dark and high-contrast themes, light the light ones
  await expect(dialog(page).getByRole("combobox", { name: "Theme for a light appearance" }).locator("option")).toHaveText(["Light", "Solarized Light"]);
  await expect.poll(() => drawn(page)).toMatchObject({ bg: rgb(theme("one-dark").colors.bg) });
  await page.emulateMedia({ colorScheme: "light" });
  await expect.poll(() => drawn(page)).toMatchObject({ bg: rgb(theme("solarized-light").colors.bg), kind: "light" });
  await expect(themeSelect(page)).toBeDisabled();
  expect(await saved(page)).toMatchObject({ theme: "system", sync: { dark: "one-dark", light: "solarized-light" } });
});

test("Quick Switch Theme (⌃`) lists the built-in and custom themes and Sync with OS", async ({ page }) => {
  await openInTheme(page, "dark", [{ id: "custom-1", name: "Night shift", base: "dracula", colors: { bg: "#1d1e26" } }]);
  await page.keyboard.press("Control+Backquote");
  const list = page.locator(".ui-popover").getByRole("option");
  await expect(list).toHaveText([/Dark/, /Light/, /High contrast/, /Dracula/, /One Dark/, /Solarized Dark/, /Solarized Light/, /Night shift.*custom/, /Sync with OS.*Dark \/ Light, now Dark/, /Edit Colors…/]);
  await list.filter({ hasText: "Night shift" }).click();
  await expect.poll(() => drawn(page)).toMatchObject({ bg: "rgb(29, 30, 38)", kind: "dark" });
  expect((await saved(page)).theme).toBe("custom-1");
});

test("a theme the GUI kept in the webview before 0.3.2 moves to the shared settings once", async ({ page }) => {
  await page.goto("");
  await page.evaluate(() => {
    localStorage.removeItem("mock-themeSettings");
    localStorage.setItem("dbtool.theme", "light");
  });
  await openApp(page);
  await expect.poll(() => drawn(page)).toMatchObject({ bg: rgb(theme("light").colors.bg), kind: "light" });
  expect((await saved(page)).theme).toBe("light");
  expect(await page.evaluate(() => localStorage.getItem("dbtool.theme"))).toBeNull();
});
