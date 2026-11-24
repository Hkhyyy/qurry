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

/** The element's outline next to the focus ring (2px solid --ui-focus, resolved) */
const ring = (l: Locator) =>
  l.evaluate((el) => {
    const probe = document.body.appendChild(document.createElement("i"));
    probe.style.color = "var(--ui-focus)";
    const focus = getComputedStyle(probe).color;
    probe.remove();
    const cs = getComputedStyle(el);
    return { outline: `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}`, focus: `solid 2px ${focus}` };
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
  expect(r.outline).toBe(r.focus);
  // Another one clicked (the focused one keeps :focus-visible when clicked)
  const avg = dialog(page).getByRole("checkbox", { name: "Avg", exact: true });
  await avg.click();
  expect((await ring(avg)).outline).toMatch(/^none /);
  await page.keyboard.press("Escape");

  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "Export Data…" }).click();
  // Back from the dialog's primary button to the last group's picked radio
  const file = dialog(page).getByRole("radio", { name: /^File…/ });
  await tabTo(page, browserName, file, true);
  const rr = await ring(file);
  expect(rr.outline).toBe(rr.focus);
  const json = dialog(page).getByRole("radio", { name: /^JSON/ });
  await json.click();
  expect((await ring(json)).outline).toMatch(/^none /);
});

/** The keys Tab and ⇧Tab press `n` times, the focus checked to be inside `inside` after each (⌥ in WebKit as well: plain Tab skips buttons there) */
async function staysInside(page: Page, browserName: string, inside: Locator, n: number) {
  const keys = browserName === "webkit" ? ["Tab", "Alt+Tab", "Shift+Tab", "Shift+Alt+Tab"] : ["Tab", "Shift+Tab"];
  for (const key of keys)
    for (let i = 0; i < n; i++) {
      await page.keyboard.press(key);
      expect(await inside.evaluate((el) => el.contains(document.activeElement) && document.activeElement !== el), `${key} ×${i + 1}`).toBe(true);
    }
}

test("a dialog keeps Tab and ⇧Tab inside and, closed, gives the focus back to its opener (#211)", async ({ page, browserName }) => {
  await treeItem(page, "users").first().dblclick();
  const opener = work(page).getByRole("button", { name: "Export Data…" });
  await opener.focus();
  await page.keyboard.press("Enter");
  const exp = page.locator(".ui-modal[aria-modal]");
  await expect(exp.getByRole("button", { name: "Export", exact: true })).toBeFocused();
  // Past the primary button (the last control) round to the first one, and back
  await staysInside(page, browserName, exp, 12);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await expect(opener).toBeFocused();

  await page.keyboard.press("Meta+Comma");
  const settings = page.locator(".ui-modal[aria-modal]");
  await staysInside(page, browserName, settings, 30);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("a confirm over Settings keeps the focus, then gives it back to the button that opened it (#211)", async ({ page, browserName }) => {
  await page.evaluate(() => localStorage.setItem("mock-themeSettings", JSON.stringify({ theme: "custom-1", sync: { dark: "dark", light: "light" }, custom: [{ id: "custom-1", name: "Night shift", base: "dracula", colors: { accent: "#9580ff" } }] })));
  await openApp(page);
  await page.keyboard.press("Meta+Comma");
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
  const del = dialog(page).getByRole("button", { name: "Delete", exact: true });
  await del.focus();
  await page.keyboard.press("Enter");
  const ask = page.locator(".ui-modal[aria-modal]", { hasText: "Delete the custom theme" });
  await expect(ask).toBeVisible();
  await staysInside(page, browserName, ask, 5);
  await page.keyboard.press("Escape");
  await expect(ask).toHaveCount(0);
  await expect(dialog(page)).toHaveCount(1);
  await expect(del).toBeFocused();
});

test("a dialog opened under a confirm (the Update dialog) leaves the focus in the confirm (#211)", async ({ page }) => {
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Remove Data Source/ }).click();
  const ask = page.locator(".ui-modal[aria-modal]", { hasText: "Delete data source" });
  const focused = () => page.evaluate(() => document.activeElement?.closest(".ui-modal")?.textContent ?? "");
  const before = await page.evaluate(() => document.activeElement?.outerHTML);
  expect(await focused()).toContain("Delete data source");
  // The offer arrives while the confirm is open (Qurry › Check for Updates…): its window mounts below, with an autoFocus inside
  await page.evaluate(async () => (await import(/* @vite-ignore */ "/src/features/dialogs/store.ts")).setUpdateOpen(true));
  await expect(page.getByText("Qurry 0.1.2 is available")).toBeVisible();
  expect(await page.evaluate(() => document.activeElement?.outerHTML)).toBe(before);
  // Closed, the confirm hands the focus to the window left on top
  await page.keyboard.press("Escape");
  await expect(ask).toHaveCount(0);
  expect(await focused()).toContain("Qurry 0.1.2 is available");
});

test("a radio group is one Tab stop and the arrow keys pick in it (#212)", async ({ page, browserName }) => {
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "Export Data…" }).click();
  const format = dialog(page).getByRole("radiogroup").nth(0);
  const csv = format.getByRole("radio", { name: /^CSV/ });
  // From the dialog's window, one stop lands on the picked format
  await page.locator(".ui-modal[aria-modal]").focus();
  await page.keyboard.press(browserName === "webkit" ? "Alt+Tab" : "Tab");
  await expect(csv).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(format.getByRole("radio", { name: /^TSV/ })).toBeChecked();
  await expect(format.getByRole("radio", { name: /^TSV/ })).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(format.getByRole("radio", { name: /^JSON/ })).toBeChecked();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowLeft");
  await expect(csv).toBeChecked();
  // The next stop is the other group's picked radio, not the next format
  await page.keyboard.press(browserName === "webkit" ? "Alt+Tab" : "Tab");
  await expect(dialog(page).getByRole("radio", { name: /^File…/ })).toBeFocused();
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
