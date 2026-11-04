import type { Page } from "@playwright/test";
import { dialog, editor, expect, menuItem, openApp, status, tabStrips, test, treeItem } from "./app";

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

// ── Several nodes (⌘-click, ⇧-click) ──
const selectedRows = (page: Page) => page.locator(".ws-explorer.on .tree-node.selected");

test("⌘-click two tables: one Drop… confirmation, single-node commands greyed out with a tooltip", async ({ page }) => {
  await treeItem(page, "contracts").first().click();
  await treeItem(page, "users").first().click({ modifiers: ["Meta"] });
  await expect(selectedRows(page)).toHaveCount(2);
  await treeItem(page, "users").first().click({ button: "right" });
  await expect(menuItem(page, /^Modify Table…/)).toHaveAttribute("aria-disabled", "true");
  await expect(menuItem(page, /^Modify Table…/)).toHaveAttribute("title", /^Works on one node/);
  await menuItem(page, "Object Actions ▸").hover();
  await menuItem(page, /^Drop…/).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("[DEV] shop_app");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["DROP TABLE public.contracts, public.users;"]);
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  // ⌥⇧⌘C copies schema.name for each, as for one node
  await treeItem(page, "users").first().click({ button: "right" });
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Alt+Shift+C");
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("public.contracts\npublic.users");
});

test("⇧-click selects the range; ⌦ drops the view before the tables it reads", async ({ page }) => {
  // [DEV] shop_app's views folder (the one of [MySQL] shop follows)
  await treeItem(page, "views 1").first().click();
  await page.keyboard.press("ArrowRight");
  await treeItem(page, "contracts").first().click();
  await treeItem(page, "v_active_users").click({ modifiers: ["Shift"] });
  await expect(selectedRows(page)).toHaveCount(4);
  await page.keyboard.press("Delete");
  await expect(dialog(page)).toContainText("Execute these 2 statements?");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["DROP VIEW public.v_active_users;", "DROP TABLE public.contracts, public.payments, public.users;"]);
});

test("on a Production data source, the Drop… of several objects is the Production confirmation", async ({ page }) => {
  // [MySQL] shop (Production) lists its tables after [DEV] shop_app's
  await treeItem(page, "contracts").nth(1).click();
  await treeItem(page, "users").nth(1).click({ modifiers: ["Meta"] });
  await page.keyboard.press("Delete");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [MySQL] shop");
  // MySQL: one statement per table (its DDL can't roll back; a failure names what was dropped)
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["DROP TABLE shop.contracts;", "DROP TABLE shop.users;"]);
});

test("rows that leave the screen leave the selection: after Collapse All the keys act on nothing hidden", async ({ page }) => {
  await treeItem(page, "contracts").first().click();
  await treeItem(page, "users").first().click({ modifiers: ["Meta"] });
  await page.locator(".ws-explorer.on .panel-header").getByTitle(/^Collapse All/).click();
  await expect(selectedRows(page)).toHaveCount(0);
  await page.locator(".ws-explorer.on .tree").focus();
  await page.keyboard.press("Delete");
  await expect(dialog(page)).toHaveCount(0);
});

test("Enter with several tables opens a tab for each", async ({ page }) => {
  await treeItem(page, "contracts").first().click();
  await treeItem(page, "users").first().click({ modifiers: ["Meta"] });
  await page.keyboard.press("Enter");
  await expect.poll(() => tabStrips(page)).toEqual(["contracts | users"]);
});

test("⌦ on a range of Redis keys asks no Drop… (DROP isn't a Redis command)", async ({ page }) => {
  // [Redis] cache's db0 lists its keys (session and user are folders)
  await treeItem(page, "X events").click();
  await treeItem(page, "S tags").click({ modifiers: ["Shift"] });
  await expect(selectedRows(page)).toHaveCount(3);
  await page.keyboard.press("Delete");
  await expect(status(page)).toHaveText(/^Works on one node/);
  await expect(dialog(page)).toHaveCount(0);
});

test("Manage Shown Schemas: unchecking one schema under All schemas keeps the others; checking it again turns All back on", async ({ page }) => {
  await page.locator(".ws-explorer.on .scope-badge", { hasText: / of 3$/ }).first().click();
  const popup = page.locator(".scope-popup");
  const box = (name: string | RegExp) => popup.getByRole("treeitem", { name, exact: true }).getByRole("checkbox");
  await expect(box("All schemas")).toBeChecked();
  await box("billing").click();
  await expect(box("All schemas")).not.toBeChecked();
  await expect(box("billing")).not.toBeChecked();
  await expect(box(/^pg_catalog/)).toBeChecked();
  await expect(box("public (Default schema)")).toBeChecked();
  await expect(box("Default schema (public)")).not.toBeChecked();
  await box("billing").click();
  await expect(box("All schemas")).toBeChecked();
  await expect(box("billing")).toBeChecked();
});

test("Manage Shown Schemas: unchecking one database under All databases hides only that one; checking it again turns All back on", async ({ page }) => {
  const badge = page.locator(".ws-explorer.on .scope-badge").first();
  await badge.click();
  const popup = page.locator(".scope-popup");
  const box = (name: string | RegExp) => popup.getByRole("treeitem", { name, exact: true }).getByRole("checkbox");
  await expect(box("All databases")).toBeChecked();
  await box("shop_app_test").click();
  await expect(box("All databases")).not.toBeChecked();
  await expect(box("shop_app_test")).not.toBeChecked();
  await expect(box("postgres")).toBeChecked();
  await expect(box(/^shop_app \(Default database\)/)).toBeChecked();
  await page.keyboard.press("Enter");
  const db = (name: string) => page.locator(".ws-explorer.on").getByRole("treeitem", { name: new RegExp(`^${name}( \\d+ of \\d+)?$`) });
  await expect(db("shop_app_test")).toHaveCount(0);
  await expect(db("postgres")).toBeVisible();
  await expect(db("shop_app")).toBeVisible();
  await expect(badge).toHaveText("2 of 3");

  await badge.click();
  await box("shop_app_test").click();
  await expect(box("All databases")).toBeChecked();
  await page.keyboard.press("Enter");
  await expect(db("shop_app_test")).toBeVisible();
  await expect(badge).toHaveText("3");
});
