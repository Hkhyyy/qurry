import { dialog, expect, menuItem, openApp, openBothUsers, tabStrips, test, treeItem } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

test("same-name tables in different schemas are labelled schema.table", async ({ page }) => {
  await openBothUsers(page);
  await expect.poll(() => tabStrips(page)).toEqual(["public.users | billing.users"]);
});

test("Split Right, Split Down, Move To Next Group", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await treeItem(page, "contracts").first().dblclick();
  await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);

  await page.getByRole("tab", { name: /contracts/ }).click({ button: "right" });
  await menuItem(page, "Split Right").click();
  // The clone of the same table in another group is told apart by db.schema.table
  await expect.poll(() => tabStrips(page)).toEqual(["users | shop_app.public.contracts", "shop_app.public.contracts"]);
  const [left, right] = await page.locator(".editor-group").evaluateAll((els) => els.map((e) => e.getBoundingClientRect()).map((r) => ({ x: r.x, y: r.y, w: r.width })));
  expect(right.x).toBeGreaterThan(left.x);
  expect(right.y).toBe(left.y);

  await page.getByRole("tab", { name: /users/ }).first().click({ button: "right" });
  await menuItem(page, "Split Down").click();
  await expect.poll(() => tabStrips(page)).toEqual(["shop_app.public.users | shop_app.public.contracts", "shop_app.public.users", "shop_app.public.contracts"]);
  const boxes = await page.locator(".editor-group").evaluateAll((els) => els.map((e) => e.getBoundingClientRect()).map((r) => ({ x: r.x, y: r.y })));
  expect(boxes[1].x).toBe(boxes[0].x);
  expect(boxes[1].y).toBeGreaterThan(boxes[0].y);

  await page.getByRole("tab", { name: /users/ }).first().click({ button: "right" });
  await expect(menuItem(page, "Unsplit All")).toBeVisible();
  await menuItem(page, "Move To Next Group").click();
  await expect.poll(() => tabStrips(page)).toEqual(["shop_app.public.contracts", "shop_app.public.users | shop_app.public.users", "shop_app.public.contracts"]);
});

test("workspaces keep their own tabs; ⌘1 switches back", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await page.getByRole("button", { name: "New Workspace" }).click();
  await expect(dialog(page)).toContainText("Workspace name");
  await expect(dialog(page).getByRole("textbox")).toHaveValue("Workspace 2");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "W2 Workspace 2" })).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => tabStrips(page)).toEqual([""]);
  await expect(page.locator(".ws-explorer.on")).toContainText("No data sources.");

  await page.keyboard.press("Meta+1");
  await expect(page.getByRole("button", { name: "DE Default" })).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => tabStrips(page)).toEqual(["users"]);
});

test("⇧⌘T reopens the closed tab", async ({ page }) => {
  await treeItem(page, "users").first().dblclick();
  await treeItem(page, "contracts").first().dblclick();
  await page.keyboard.press("Meta+W");
  await expect(page.getByRole("tab", { name: /contracts/ })).toHaveCount(0);
  await expect.poll(() => tabStrips(page)).toEqual(["users"]);
  await page.keyboard.press("Meta+Shift+T");
  await expect(page.getByRole("tab", { name: /contracts/ })).toHaveAttribute("aria-selected", "true");
  await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
});
