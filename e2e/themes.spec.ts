import type { Page } from "@playwright/test";
import TABLE from "../crates/shared/src/themes.json" with { type: "json" };
import { dialog, expect, openApp, openConsole, openInTheme, test, typeSql, work } from "./app";

// Themes (#103): the seven built-in ones, custom themes made in Settings › Appearance › Theme › Edit Colors…, Sync with OS with a dark
// and a light theme of the user's choice, Quick Switch Theme. The pick and the custom themes live in settings.json, shared with the TUI
// (the mock keeps it in localStorage "mock-themeSettings")

const rgb = (h: string) => `rgb(${[1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ")})`;
const theme = (id: string) => TABLE.themes.find((t) => t.id === id)!;
/** What the window draws: its background, the explorer's panel (null until it is rendered, so a poll right after a reload retries), data-theme */
const drawn = (page: Page) =>
  page.evaluate(() => {
    const panel = document.querySelector(".panel");
    return { bg: getComputedStyle(document.documentElement).backgroundColor, panel: panel && getComputedStyle(panel).backgroundColor, kind: document.documentElement.dataset.theme };
  });
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
  await expect(row.locator(".theme-warn")).toHaveText(/⚠ 1\.\d:1 on Row numbers/);
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

// #188: the preview gets most of the width (a color's row is its name, swatch and hex), so its parts aren't cut
test("Edit Colors…: the preview is wider than the color list and its grid header, tree and console text aren't cut", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  const preview = dialog(page).locator(".theme-preview");
  await expect(preview.locator(".monaco-editor .view-line").first()).toBeVisible();
  const side = (await dialog(page).locator(".theme-editor-side").boundingBox())!;
  const shown = (await preview.boundingBox())!;
  expect(shown.width).toBeGreaterThanOrEqual(400);
  expect(shown.width).toBeGreaterThan(side.width);
  // Nothing in the tree, the grid header or the console is wider than its box
  const cut = await preview.evaluate((p) => {
    const box = (el: Element) => el.getBoundingClientRect();
    const out = [...p.querySelectorAll(".tree-node, .grid-header .cell, .col-name, .col-type")].filter((el) => el.scrollWidth > el.clientWidth + 1).map((el) => el.textContent);
    const editor = box(p.querySelector(".theme-preview-editor")!);
    // A console line's text ends inside the console
    for (const line of p.querySelectorAll(".view-line > span")) if (box(line).right > editor.right + 1) out.push(line.textContent);
    return out;
  });
  expect(cut).toEqual([]);
  // A color's row is one line: name, swatch, hex; what it colors is said above the preview when pointing at its name
  const row = dialog(page).locator('[data-token="keyword"]');
  expect((await row.boundingBox())!.height).toBeLessThanOrEqual(32);
  await expect(row.locator(".theme-token-label")).toHaveText("Keyword");
  await row.locator(".theme-token-label").hover();
  await expect(dialog(page).locator(".theme-editor-preview > .ui-text")).toHaveText(/^Preview — Keyword: SQL keywords/);
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

test("changing Based on with unsaved colors asks first: Cancel keeps them, Discard loads the other theme", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  const base = dialog(page).getByRole("combobox", { name: "Based on" });
  const textHex = dialog(page).getByRole("textbox", { name: "Text hex", exact: true });
  await base.selectOption("dracula");
  await textHex.fill("#ffeedd");
  const question = page.locator(".ui-modal", { hasText: "Discard them?" });
  await base.selectOption("one-dark");
  await expect(question).toBeVisible();
  await question.getByRole("button", { name: "Cancel" }).click();
  await expect(base).toHaveValue("dracula");
  await expect(textHex).toHaveValue("#ffeedd");
  await base.selectOption("one-dark");
  await question.getByRole("button", { name: "Discard" }).click();
  await expect(base).toHaveValue("one-dark");
  await expect(textHex).toHaveValue(theme("one-dark").colors.text);
  // Nothing unsaved: no question
  await base.selectOption("dark");
  await expect(question).toHaveCount(0);
  await expect(base).toHaveValue("dark");
});

test("saving a custom theme under Sync with OS keeps Sync with OS; Save as new theme switches and says Sync with OS is off", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await openInTheme(page, "system", [{ id: "custom-1", name: "Night shift", base: "dracula", colors: { bg: "#1d1e26" } }]);
  await page.evaluate(() => {
    const s = JSON.parse(localStorage.getItem("mock-themeSettings")!);
    localStorage.setItem("mock-themeSettings", JSON.stringify({ ...s, sync: { dark: "custom-1", light: "light" } }));
  });
  await page.reload();
  // ⌘, is handled once the app is up: pressed during the reload it goes nowhere
  await expect(page.locator(".tree-node", { hasText: "[DEV] shop_app" })).toBeVisible();
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await expect(dialog(page).getByRole("combobox", { name: "Based on" })).toHaveValue("custom-1");
  await dialog(page).getByRole("textbox", { name: "Accent hex", exact: true }).fill("#9580ff");
  await dialog(page).getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog(page).getByText('Saved "Night shift", Sync with OS stays on')).toBeVisible();
  expect((await saved(page)).theme).toBe("system");
  await dialog(page).getByRole("textbox", { name: "Theme name" }).fill("Night shift 2");
  await dialog(page).getByRole("button", { name: "Save as new theme" }).click();
  await expect(dialog(page).getByText('Saved "Night shift 2" and switched to it: Sync with OS is off')).toBeVisible();
  expect((await saved(page)).theme).toBe("custom-2");
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

// #202: OK saves the theme editor's changes, as DataGrip's OK applies them; Esc and × ask to discard them, as leaving the editor does
test("OK saves colors changed from a built-in theme as a new custom theme, picks it and closes Settings", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("textbox", { name: "Background, editor hex", exact: true }).fill("#101018");
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  const s = await saved(page);
  expect(s.custom).toEqual([expect.objectContaining({ name: "Dark custom", base: "dark", colors: { bg: "#101018" } })]);
  expect(s.theme).toBe(s.custom[0].id);
  await expect.poll(() => drawn(page)).toMatchObject({ bg: "rgb(16, 16, 24)" });
  // Again from Dark: the next free name
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("combobox", { name: "Based on" }).selectOption("dark");
  await dialog(page).getByRole("textbox", { name: "Accent hex", exact: true }).fill("#7a5af8");
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await saved(page)).custom.map((c: { name: string }) => c.name)).toEqual(["Dark custom", "Dark custom 2"]);
});

test("OK with a save that fails keeps Settings open and says why", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("textbox", { name: "Background, editor hex", exact: true }).fill("#101018");
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = true));
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page).locator(".theme-editor-msg")).toHaveText(/Not saved: Couldn't save the settings: permission denied/);
  await expect(dialog(page).locator(".theme-editor-msg")).toBeInViewport();
  expect((await saved(page))?.custom ?? []).toEqual([]);
  // Saved once the file can be written
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = false));
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await saved(page)).custom).toHaveLength(1);
});

// Esc and × with unsaved colors ask to discard them (the test above); with nothing changed they close at once
test("Esc in the theme editor with nothing changed closes Settings at once", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("textbox", { name: "Theme name" }).press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

// The button that opened the editor goes away with the page, leaving the focus on the body: Esc still reaches Settings
test("Esc closes Settings when nothing has the focus, and asks first with unsaved colors", async ({ page }) => {
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  // Chromium leaves it there by itself; WebKit doesn't focus a clicked button, so it is put there
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  await dialog(page).getByRole("textbox", { name: "Text hex", exact: true }).fill("#ff0000");
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("Escape");
  await expect(page.locator(".ui-modal", { hasText: "Discard them?" })).toBeVisible();
  await page.getByRole("button", { name: "Discard" }).click();
  await expect(dialog(page)).toHaveCount(0);
});

// #204: the editor's action row wraps instead of being cut, also for a saved theme (Delete, Save) in the smallest window
for (const theme of ["dark", "light"]) {
  test(`the theme editor's buttons, name and message all show for a saved theme at 800×500 (${theme})`, async ({ page }) => {
    await page.setViewportSize({ width: 800, height: 500 });
    await openInTheme(page, "custom-1", [{ id: "custom-1", name: "Night shift", base: theme, colors: { accent: "#9580ff" } }]);
    await settings(page);
    await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
    const actions = dialog(page).locator(".theme-editor-actions");
    await dialog(page).getByRole("textbox", { name: "Theme name" }).fill("Night shift 2");
    for (const name of ["Import…", "Export…", "Reset to base", "Delete", "Save", "Save as new theme"]) {
      const b = actions.getByRole("button", { name, exact: true });
      await expect(b, name).toBeInViewport({ ratio: 1 });
      // Not covered or cut: a click at its middle lands on it
      const box = (await b.boundingBox())!;
      expect(await page.evaluate(([x, y]) => document.elementFromPoint(x, y)?.textContent?.trim(), [box.x + box.width / 2, box.y + box.height / 2]), name).toBe(name);
    }
    await expect(actions.getByRole("textbox", { name: "Theme name" })).toBeInViewport({ ratio: 1 });
    await actions.getByRole("button", { name: "Save", exact: true }).click();
    await expect(dialog(page).locator(".theme-editor-msg")).toHaveText(/Saved "Night shift 2"/);
    await expect(dialog(page).locator(".theme-editor-msg .ui-notice")).toBeInViewport({ ratio: 1 });
  });
}

// beta.1's "can't scroll here": the wheel over a color's swatch (its hidden color input) didn't scroll the list in Chromium
test("the color list scrolls with the wheel, over the swatches too, down to the last basic color and Advanced colors at 800×500", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 500 });
  await openApp(page);
  await settings(page);
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  const list = dialog(page).locator(".theme-tokens");
  const swatch = (await dialog(page).locator(".theme-swatch").first().boundingBox())!;
  await page.mouse.move(swatch.x + swatch.width / 2, swatch.y + swatch.height / 2);
  for (let i = 0; i < 8; i++) await page.mouse.wheel(0, 150);
  await expect.poll(() => list.evaluate((e) => e.scrollTop)).toBeGreaterThan(0);
  // Scrolled to the end: the last basic color and Advanced colors inside the list's box
  const inList = async (l: import("@playwright/test").Locator) => {
    const [r, b] = [(await l.boundingBox())!, (await list.boundingBox())!];
    return r.y >= b.y - 1 && r.y + r.height <= b.y + b.height + 1;
  };
  await expect.poll(() => inList(dialog(page).locator('[data-token="prodTint"]'))).toBe(true);
  await expect.poll(() => inList(dialog(page).getByRole("button", { name: /Advanced colors/ }))).toBe(true);
  // The actions under the list stay where they are
  for (const name of ["Import…", "Export…", "Save as new theme"]) await expect(dialog(page).getByRole("button", { name, exact: true })).toBeInViewport({ ratio: 1 });
  // A click on the swatch still reaches its color input (the system picker)
  const clicked = page.evaluate(() => new Promise<boolean>((ok) => document.querySelector<HTMLInputElement>('[data-token="bg"] input[type=color]')!.addEventListener("click", (e) => (e.preventDefault(), ok(true)), { once: true })));
  await dialog(page).locator('[data-token="bg"] .theme-swatch').click();
  expect(await clicked).toBe(true);
});
