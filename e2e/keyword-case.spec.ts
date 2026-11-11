import type { Page } from "@playwright/test";
import { dialog, editor, editorText, expect, openApp, openConsole, openSettings, test, typeSql } from "./app";

// Settings › Appearance › Console › Keyword case (settings.json "keywordCase", shared with the TUI): keywords as you type in a console and in
// Reformat Code (⌘⌥l). The rule (strings, comments, quoted names, names after a dot) is qurry_shared::format's, tested there

const field = (page: Page) => dialog(page).getByRole("combobox", { name: "Keyword case" });

async function pickCase(page: Page, value: "off" | "upper" | "lower") {
  await openSettings(page, "Console");
  await field(page).selectOption(value);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
}

test.beforeEach(async ({ page }) => openApp(page));

test("Keyword case UPPER: keywords change as you type, strings and names after a dot don't; kept after a restart", async ({ page }) => {
  await openSettings(page, "Console");
  await expect(field(page)).toHaveValue("off");
  await page.keyboard.press("Escape");
  await pickCase(page, "upper");
  await openConsole(page);
  await typeSql(page, "select id, 'select from' from users u where u.order = 1 and id in(");
  await page.keyboard.press("Escape");
  await expect.poll(() => editorText(page)).toBe("SELECT id, 'select from' FROM users u WHERE u.order = 1 AND id IN()");
  // A word typed into: nothing ends a keyword yet
  await typeSql(page, "selection");
  await expect.poll(() => editorText(page)).toBe("selection");

  await page.reload();
  await expect(page.locator(".tree-node", { hasText: "[DEV] shop_app" })).toBeVisible();
  await page.keyboard.press("Meta+Comma");
  await expect(field(page)).toHaveValue("upper");
});

test("Keyword case lower: Reformat Code lays out the statement at the caret in lower case, one ⌘Z undoes it; Off leaves typing alone", async ({ page }) => {
  await pickCase(page, "lower");
  await openConsole(page);
  await typeSql(page, "SELECT 1;\nSELECT a FROM b WHERE c = 'X'");
  await page.keyboard.press("Escape");
  const typed = "select 1;\nselect a from b where c = 'X'";
  await expect.poll(() => editorText(page)).toBe(typed);
  await page.keyboard.press("Meta+Alt+KeyL");
  await expect.poll(() => editorText(page)).toBe("select 1;\nselect\n    a\nfrom\n    b\nwhere\n    c = 'X'");
  await page.keyboard.press("Meta+KeyZ");
  await expect.poll(() => editorText(page)).toBe(typed);

  await pickCase(page, "off");
  await editor(page).click();
  await typeSql(page, "select a FROM b ");
  await expect.poll(() => editorText(page)).toBe("select a FROM b ");
});
