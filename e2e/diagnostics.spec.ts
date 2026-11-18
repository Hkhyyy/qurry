import type { Page } from "@playwright/test";
import { dialog, expect, openApp, status, test } from "./app";

// Help › Copy Diagnostics (#179): Find Action runs what the Help menu item runs, the mock backend's text onto the clipboard
test.beforeEach(async ({ page }) => openApp(page));

const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());

test("Find Action › Copy Diagnostics puts the backend's text on the clipboard", async ({ page }) => {
  await page.keyboard.press("Meta+Shift+A");
  await page.keyboard.type("Copy Diagnostics");
  await expect(page.locator(".search-everywhere")).toContainText("Copy Diagnostics");
  // In the Help group, as in the Help menu
  await expect(page.locator(".search-everywhere").getByText("Help", { exact: true }).first()).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(status(page)).toHaveText("Diagnostics copied");
  await expect.poll(() => clipboard(page)).toContain("Qurry gui 0.0.0-mock · core 0.0.0-mock\nOS: macos (aarch64)");
});

test("the Keymap Reference lists Copy Diagnostics after Show Log", async ({ page }) => {
  await page.keyboard.press("Meta+Shift+Slash");
  const rows = dialog(page).locator(".keymap-row .keymap-desc");
  await expect(rows.filter({ hasText: /^Copy Diagnostics$/ })).toHaveCount(1);
  const all = await rows.allTextContents();
  expect(all.indexOf("Copy Diagnostics")).toBe(all.indexOf("Show Log") + 1);
});
