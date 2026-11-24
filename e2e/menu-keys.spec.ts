import type { Page } from "@playwright/test";
import { dialog, editor, expect, menuItem, openApp, test, treeItem, work } from "./app";

// The shared menu's keys (#215, DataGrip and macOS menus): ↑↓ round past separators and disabled items, Home End, ↩ or Space runs,
// → opens a submenu and ← closes it, a letter jumps; opened from the keyboard its first item has the focus, from the pointer none
// until ↑↓; ⎋ closes and gives the focus back

test.beforeEach(async ({ page }) => openApp(page));

const menu = (page: Page) => page.getByRole("menu").first();
const tree = (page: Page) => page.locator(".ws-explorer.on .tree");
const focusedItem = (page: Page) => page.evaluate(() => (document.activeElement?.getAttribute("role") === "menuitem" ? document.activeElement.querySelector(".ui-menu-label")?.textContent : null));

test("⋮ opened with ↩: the first item has the focus; ↑↓ go round past the separator, Home End; → and ← through the submenu; ⎋ closes the submenu, then the menu, the focus back", async ({ page }) => {
  const more = page.locator(".ws-explorer.on").getByTitle("More", { exact: true });
  await more.focus();
  await page.keyboard.press("Enter");
  await expect(menuItem(page, /Show System Schemas/)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, /^Export Data Sources…/)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, /^Import Data Sources/)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, /Show System Schemas/)).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(menuItem(page, /^Import Data Sources/)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(menuItem(page, /Show System Schemas/)).toBeFocused();
  await page.keyboard.press("End");
  await expect(menuItem(page, /^Import Data Sources/)).toBeFocused();
  // The submenu: → opens it with its first item focused, ← closes it back to its item
  await page.keyboard.press("ArrowRight");
  const sub = page.locator(".ui-menu.sub");
  await expect(sub).toBeVisible();
  await expect(sub.getByRole("menuitem").first()).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await expect(sub).toHaveCount(0);
  await expect(menuItem(page, /^Import Data Sources/)).toBeFocused();
  // The keyboard focus shows the shared ring
  await expect(menuItem(page, /^Import Data Sources/)).toHaveCSS("outline-style", "solid");
  // ⎋ in the submenu (DataGrip): only the submenu closes, the focus back on its item; ⎋ again closes the menu
  await page.keyboard.press("ArrowRight");
  await expect(sub.getByRole("menuitem").first()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(sub).toHaveCount(0);
  await expect(menu(page)).toBeVisible();
  await expect(menuItem(page, /^Import Data Sources/)).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(more).toBeFocused();
});

test("a menu opened by the pointer has no focus until ↓; the swatch row is one stop; a letter jumps; ⎋ gives the focus back to the tree", async ({ page }) => {
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await expect(menu(page)).toBeVisible();
  expect(await focusedItem(page)).toBeNull();
  await expect(tree(page)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, /^New/)).toBeFocused();
  // ↑ from the first: round to the swatch row (its picked color), then the last item
  const colors = menu(page).getByRole("radiogroup", { name: "Color" });
  await page.keyboard.press("ArrowUp");
  await expect(colors.getByRole("radio", { name: "Yellow" })).toBeFocused();
  // ←→ in the row move between colors without picking, ↓ leaves the row as one stop
  await page.keyboard.press("ArrowRight");
  await expect(colors.getByRole("radio", { name: "Green" })).toBeFocused();
  await expect(colors.getByRole("radio", { name: "Yellow" })).toBeChecked();
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, /^New/)).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await expect(menuItem(page, /^Remove Data Source/)).toBeFocused();
  // Type-ahead: the next item starting with the letter, round
  await page.keyboard.press("r");
  await expect(menuItem(page, /^Refresh/)).toBeFocused();
  await page.keyboard.press("r");
  await expect(menuItem(page, /^Remove from This Workspace/)).toBeFocused();
  await page.keyboard.press("d");
  await expect(menuItem(page, /^Deactivate/)).toBeFocused();
  // No match: no move, and the letter doesn't reach the tree's speed search behind the menu
  await page.keyboard.press("z");
  await expect(menuItem(page, /^Deactivate/)).toBeFocused();
  await expect(page.locator(".speed-search")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(tree(page)).toBeFocused();
});

test("Space on a color in the menu picks it and closes the menu, the focus back on the tree", async ({ page }) => {
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await page.keyboard.press("End");
  const colors = menu(page).getByRole("radiogroup", { name: "Color" });
  await expect(colors.getByRole("radio", { name: "Yellow" })).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Space");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(tree(page)).toBeFocused();
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await expect(colors.getByRole("radio", { name: "Green" })).toBeChecked();
});

test("↩ runs the focused item; → ↩ through a submenu; a menu opened from that item's key has its first item focused", async ({ page }) => {
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await page.keyboard.press("p");
  await expect(menuItem(page, /^Properties/)).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(dialog(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);

  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await expect(menu(page)).toBeVisible();
  await page.keyboard.press("n");
  await page.keyboard.press("n");
  await expect(menuItem(page, /^Navigation/)).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(menuItem(page, /^Jump to Query Console…/)).toBeFocused();
  await page.keyboard.press("Enter");
  // Jump to Query Console… opens its own menu: from the keyboard, so its first item has the focus; Space runs it
  await expect(menuItem(page, /^New Query Console/)).toBeFocused();
  await page.keyboard.press("Space");
  await expect(editor(page)).toBeVisible();
});

test("↑↓ skip disabled items (several nodes selected: the one-node commands are greyed out)", async ({ page }) => {
  await treeItem(page, "contracts").first().click();
  await treeItem(page, "users").first().click({ modifiers: ["Meta"] });
  await treeItem(page, "users").first().click({ button: "right" });
  await expect(menuItem(page, /^Modify Table…/)).toHaveAttribute("aria-disabled", "true");
  const seen: (string | null | undefined)[] = [];
  for (let i = 0; i < 16; i++) {
    await page.keyboard.press("ArrowDown");
    seen.push(await focusedItem(page));
  }
  expect(seen).not.toContain(null);
  const disabled = await menu(page).locator('> [role=menuitem][aria-disabled="true"] .ui-menu-label').allTextContents();
  expect(disabled.length).toBeGreaterThan(0);
  for (const d of disabled) expect(seen).not.toContain(d);
  await page.keyboard.press("Escape");
});

test("the grid header menu: End, ↑ past the disabled Reset Widths, Home, ↩ hides the column", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  const header = work(page).getByRole("columnheader", { name: /^amount / });
  await header.click({ button: "right" });
  await expect(menuItem(page, /^Reset Widths/)).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("End");
  await expect(menuItem(page, /^Widen Column/)).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await expect(menuItem(page, /^Fit All Columns/)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(menuItem(page, /^Hide Column/)).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect.poll(() => page.locator(".statusbar .sb-seg").allTextContents()).toContain("1 hidden");
});
