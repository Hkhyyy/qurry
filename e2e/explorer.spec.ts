import { dialog, editor, expect, menuItem, openApp, tabStrips, test, treeItem } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

test("table context menu", async ({ page }) => {
  await treeItem(page, "users").first().click({ button: "right" });
  await expect(page.getByRole("menu")).toMatchAriaSnapshot(`
    - menu:
      - /children: equal
      - menuitem "New ▸"
      - menuitem "Modify Table… ⌘F6"
      - menuitem "Rename… ⇧F6"
      - menuitem "Edit Data ⌘↓"
      - separator
      - menuitem "Copy/Paste ▸"
      - menuitem "Refresh ⌘r"
      - menuitem "Quick Documentation F1"
      - separator
      - menuitem "Navigation ▸"
      - menuitem "AI Search (RAG) — Index This Table…"
      - menuitem "SQL Scripts ▸"
      - menuitem "Object Actions ▸"
      - menuitem "Import/Export ▸"
      - menuitem "Tools ▸"
  `);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
});

test("data source context menu", async ({ page }) => {
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await expect(page.getByRole("menu")).toMatchAriaSnapshot(`
    - menu:
      - /children: equal
      - menuitem "New ▸"
      - menuitem "Properties ⌘;"
      - separator
      - menuitem "Copy/Paste ▸"
      - menuitem "Refresh ⌘r"
      - menuitem "Deactivate ⌘F2"
      - menuitem "Quick Documentation F1"
      - separator
      - menuitem "Navigation ▸"
      - menuitem "Tools ▸"
      - menuitem "Diagnostics ▸"
      - separator
      - menuitem "Remove from This Workspace"
      - menuitem "Remove Data Source ⌦"
      - separator
      - text: Color
  `);
});

test("Navigation → Jump to Query Console… offers New Query Console, then the open consoles", async ({ page }) => {
  await treeItem(page, "[DEV] shop_app 3").click({ button: "right" });
  await menuItem(page, "Navigation ▸").hover();
  await menuItem(page, /^Jump to Query Console…/).click();
  await expect(page.getByRole("menu")).toMatchAriaSnapshot(`
    - menu:
      - /children: equal
      - menuitem "New Query Console ⇧⌘L"
  `);
  await menuItem(page, /^New Query Console/).click();
  await expect(editor(page)).toBeVisible();
  await expect.poll(() => tabStrips(page)).toEqual(["console [DEV] shop_app"]);

  await treeItem(page, "[DEV] shop_app 3").click();
  await page.keyboard.press("Meta+Shift+F10");
  await expect(page.getByRole("menu")).toMatchAriaSnapshot(`
    - menu:
      - /children: equal
      - menuitem "New Query Console ⇧⌘L"
      - separator
      - menuitem "console [DEV] shop_app"
  `);
});

test("Rename… shows the warning when it ran but the Production log couldn't record it", async ({ page }) => {
  await page.evaluate(() => ((globalThis as any).__mockDdlWarning = "It ran, but the result couldn't be written to the Production log"));
  await treeItem(page, "users").first().click({ button: "right" });
  await menuItem(page, /^Rename…/).click();
  await expect(dialog(page).getByRole("textbox")).toHaveValue("users");
  await dialog(page).getByRole("textbox").fill("members");
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await dialog(page).getByRole("button", { name: "Rename" }).click();
  await expect(dialog(page)).toContainText("Ran, with a warning");
  await expect(dialog(page)).toContainText("It ran, but the result couldn't be written to the Production log");
});

test("Enter in the Rename… name field doesn't also answer the confirmation that follows", async ({ page }) => {
  await treeItem(page, "users").first().click({ button: "right" });
  await menuItem(page, /^Rename…/).click();
  await dialog(page).getByRole("textbox").fill("members");
  await page.keyboard.press("Enter");
  await expect(dialog(page)).toContainText('Rename "users" to "members"?');
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(treeItem(page, "users").first()).toBeVisible();
});
