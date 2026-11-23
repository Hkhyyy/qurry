import { readFileSync } from "node:fs";
import type { Locator, Page } from "@playwright/test";
import { dialog, expect, openApp, openSettings, settingsRow, settingsTree, test, treeItem, work } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

test("Settings (⌘,): pages, theme, Escape closes", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Settings");
  await expect(settingsTree(page).getByRole("treeitem")).toHaveText(["Appearance", "Console", "Data grid", "Updates", "Keymap", "Passwords"]);
  await expect(settingsRow(page, "Appearance")).toHaveAttribute("aria-selected", "true");
  await expect(dialog(page).getByRole("combobox", { name: "Theme", exact: true }).locator("option")).toHaveText(["Dark", "Light", "High contrast", "Dracula", "One Dark", "Solarized Dark", "Solarized Light"]);
  await expect(dialog(page).getByRole("combobox", { name: "Theme", exact: true })).toHaveValue("dark");
  await settingsRow(page, "Updates").click();
  await expect(dialog(page).getByRole("checkbox", { name: /Check for updates automatically/ })).toBeChecked();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("Settings › Updates › Channel: stable by default, beta is kept after reopening", async ({ page }) => {
  await openSettings(page, "Updates");
  const channel = dialog(page).getByRole("combobox", { name: /Channel/ });
  await expect(channel.locator("option")).toHaveText(["Stable", "Beta"]);
  await expect(channel).toHaveValue("stable");
  await channel.selectOption("beta");
  // The mock offers 0.1.2 over 0.1.0
  await expect(dialog(page).getByText("Qurry 0.1.2 is available on this channel.")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page).getByRole("combobox", { name: /Channel/ })).toHaveValue("beta");
});

test("Settings › Updates › Channel with automatic checks off: saved without a request, Check Now looks", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("dbtool.updateCheck", "off"));
  await openApp(page);
  await openSettings(page, "Updates");
  const field = dialog(page).locator(".ui-field-label", { hasText: "Channel" });
  // The label fits its column (not "Chan…")
  expect(await field.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true);
  await dialog(page).getByRole("combobox", { name: /Channel/ }).selectOption("beta");
  await expect(dialog(page).getByText("Saved. Check Now looks for updates on this channel.")).toBeVisible();
  await expect(page.locator(".statusbar")).not.toContainText("available");
});

test("Settings › Updates › Check Now that finds nothing clears the status bar button and names the channel", async ({ page }) => {
  // The startup check offers 0.1.2
  await expect(page.locator(".statusbar")).toContainText("⬆ v0.1.2 available");
  // The TUI switched the shared channel: nothing newer now
  await page.evaluate(() => ((window as any).__mockNoUpdate = true));
  await openSettings(page, "Updates");
  await dialog(page).getByRole("button", { name: "Check Now" }).click();
  await expect(dialog(page).getByText("You have the latest stable version (0.1.0).")).toBeVisible();
  await expect(page.locator(".statusbar")).not.toContainText("available");
  // Picking a channel drops the answer for the channel before
  await dialog(page).getByRole("combobox", { name: /Channel/ }).selectOption("beta");
  await expect(dialog(page).getByText("You have the latest beta version (0.1.0).")).toBeVisible();
  await expect(dialog(page).getByText("You have the latest stable version (0.1.0).")).toHaveCount(0);
});

test("An update offer that goes away closes its window, and a later offer doesn't open it again", async ({ page }) => {
  await page.clock.install();
  await openApp(page);
  await page.locator(".statusbar").getByText("⬆ v0.1.2 available").click();
  await expect(dialog(page).getByText("Qurry 0.1.2 is available")).toBeVisible();
  // The TUI switched the shared channel: the hourly check finds nothing
  await page.evaluate(() => ((window as any).__mockNoUpdate = true));
  await page.clock.fastForward("01:00:00");
  await expect(page.locator(".statusbar")).not.toContainText("available");
  await expect(dialog(page)).toHaveCount(0);
  // Beta offers 0.1.2 again: the status bar shows it, the window stays closed
  await page.evaluate(() => ((window as any).__mockNoUpdate = false));
  await openSettings(page, "Updates");
  await dialog(page).getByRole("combobox", { name: /Channel/ }).selectOption("beta");
  await expect(dialog(page).getByText("Qurry 0.1.2 is available on this channel.")).toBeVisible();
  await expect(page.locator(".statusbar")).toContainText("⬆ v0.1.2 available");
  await expect(page.getByText("Qurry 0.1.2 is available", { exact: true })).toHaveCount(0);
});

test("With update checks off, the hourly check keeps what Check Now found", async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => ((window as any).__mockChecksOff = true));
  await openApp(page);
  await expect(page.locator(".statusbar")).not.toContainText("available");
  await openSettings(page, "Updates");
  await dialog(page).getByRole("button", { name: "Check Now" }).click();
  await expect(page.locator(".statusbar")).toContainText("⬆ v0.1.2 available");
  await page.keyboard.press("Escape");
  await page.clock.fastForward("01:00:00");
  await page.clock.fastForward("01:00:00");
  await expect(page.locator(".statusbar")).toContainText("⬆ v0.1.2 available");
});

test("Settings › Updates › Channel: a channel that couldn't be saved isn't shown as picked", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = true));
  await openSettings(page, "Updates");
  const channel = dialog(page).getByRole("combobox", { name: /Channel/ });
  await channel.selectOption("beta");
  await expect(dialog(page).getByText("Couldn't save the settings: permission denied")).toBeVisible();
  await expect(channel).toHaveValue("stable");
});

test("Keymap Reference (⇧⌘/) lists every row with ⌘⌥⌃⇧ keys", async ({ page }) => {
  await page.keyboard.press("Meta+Shift+Slash");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Keymap Reference");
  const rows = await dialog(page).locator(".keymap-section").evaluateAll((secs) =>
    secs.map((s) => [`## ${s.querySelector(".keymap-title")!.textContent}`, ...[...s.querySelectorAll(".keymap-row")].map((r) => `${r.querySelector(".keymap-keys")!.textContent}\t${r.querySelector(".keymap-desc")!.textContent}`)].join("\n")).join("\n") + "\n",
  );
  expect(rows).toBe(readFileSync(new URL("fixtures/keymap-reference.txt", import.meta.url), "utf8"));
  expect(rows).not.toMatch(/Ctrl|Alt\+|Cmd|Shift\+/);
});

/** The element's outline next to the accent ring (2px solid --ui-accent, resolved to rgb) */
const ring = (l: Locator) =>
  l.evaluate((el) => {
    const probe = document.body.appendChild(document.createElement("i"));
    probe.style.color = "var(--ui-accent)";
    const accent = getComputedStyle(probe).color;
    probe.remove();
    const cs = getComputedStyle(el);
    return { outline: `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}`, accent: `solid 2px ${accent}` };
  });
/** Tab (⌥Tab in WebKit, which otherwise skips checkboxes and radios) until the control has the focus */
async function tabTo(page: Page, browserName: string, target: Locator, back = false) {
  const key = `${back ? "Shift+" : ""}${browserName === "webkit" ? "Alt+" : ""}Tab`;
  for (let i = 0; i < 20 && !(await target.evaluate((e) => e === document.activeElement)); i++) await page.keyboard.press(key);
  await expect(target).toBeFocused();
}

test("a checkbox and a radio reached with the keyboard show the accent focus ring; clicked with the mouse, none (#203)", async ({ page, browserName }) => {
  await openSettings(page, "Data grid");
  const check = dialog(page).getByRole("checkbox", { name: "Sum", exact: true });
  await tabTo(page, browserName, check);
  const r = await ring(check);
  expect(r.outline).toBe(r.accent);
  // Another one clicked (the focused one keeps :focus-visible when clicked)
  const avg = dialog(page).getByRole("checkbox", { name: "Avg", exact: true });
  await avg.click();
  expect((await ring(avg)).outline).toMatch(/^none /);
  await page.keyboard.press("Escape");

  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "Export Data…" }).click();
  // Back from the dialog's primary button to its last radio
  const clipboard = dialog(page).getByRole("radio", { name: /^Clipboard/ });
  await tabTo(page, browserName, clipboard, true);
  const rr = await ring(clipboard);
  expect(rr.outline).toBe(rr.accent);
  const json = dialog(page).getByRole("radio", { name: /^JSON/ });
  await json.click();
  expect((await ring(json)).outline).toMatch(/^none /);
});

test("Export Data… offers every format and destination", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "Export Data…" }).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Export users");
  const format = dialog(page).getByRole("radiogroup").nth(0);
  const to = dialog(page).getByRole("radiogroup").nth(1);
  await expect(format.getByRole("radio")).toHaveCount(5);
  for (const n of ["CSV Excel, spreadsheets", "TSV tab separated — pastes into spreadsheets as cells", "JSON array of objects", "SQL INSERT one INSERT statement per row", "Markdown table for docs, issues, PRs"])
    await expect(format.getByRole("radio", { name: n, exact: true })).toBeVisible();
  await expect(format.getByRole("radio", { name: /^CSV/ })).toBeChecked();
  await expect(to.getByRole("radio")).toHaveCount(2);
  await expect(to.getByRole("radio", { name: "File… all rows, choose where to save", exact: true })).toBeChecked();
  await expect(to.getByRole("radio", { name: "Clipboard all rows, paste anywhere (up to 64 MB)", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});
