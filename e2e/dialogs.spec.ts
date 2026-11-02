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

test("Settings › Update channel: stable by default, beta is kept after reopening", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const channel = dialog(page).getByRole("combobox", { name: /Update channel/ });
  await expect(channel.locator("option")).toHaveText(["Stable", "Beta"]);
  await expect(channel).toHaveValue("stable");
  await channel.selectOption("beta");
  // The mock offers 0.1.2 over 0.1.0
  await expect(dialog(page).getByText("Qurry 0.1.2 is available on this channel.")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page).getByRole("combobox", { name: /Update channel/ })).toHaveValue("beta");
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
