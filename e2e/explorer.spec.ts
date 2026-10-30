import { editor, expect, menuItem, openApp, tabStrips, test, treeItem } from "./app";

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
