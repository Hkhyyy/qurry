import { readFileSync } from "node:fs";
import { dialog, expect, openApp, test, treeItem, work } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

test("Settings (⌘,): sections, theme, Escape closes", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Settings");
  await expect(dialog(page).getByRole("listbox").getByRole("option")).toHaveText(["Appearance", "Keymap", "Passwords"]);
  await expect(dialog(page).getByRole("listbox").getByRole("option", { name: "Appearance" })).toHaveAttribute("aria-selected", "true");
  await expect(dialog(page).getByRole("combobox", { name: /Theme/ }).locator("option")).toHaveText(["Dark", "Light", "High contrast"]);
  await expect(dialog(page).getByRole("combobox", { name: /Theme/ })).toHaveValue(/dark/i);
  await expect(dialog(page).getByRole("checkbox", { name: /Check for updates automatically/ })).toBeChecked();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("Settings › Updates › Channel: stable by default, beta is kept after reopening", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
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
  await page.keyboard.press("Meta+Comma");
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
  await page.keyboard.press("Meta+Comma");
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
  await page.keyboard.press("Meta+Comma");
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
  await page.keyboard.press("Meta+Comma");
  await dialog(page).getByRole("button", { name: "Check Now" }).click();
  await expect(page.locator(".statusbar")).toContainText("⬆ v0.1.2 available");
  await page.keyboard.press("Escape");
  await page.clock.fastForward("01:00:00");
  await page.clock.fastForward("01:00:00");
  await expect(page.locator(".statusbar")).toContainText("⬆ v0.1.2 available");
});

test("Settings › Updates › Channel: a channel that couldn't be saved isn't shown as picked", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = true));
  await page.keyboard.press("Meta+Comma");
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
