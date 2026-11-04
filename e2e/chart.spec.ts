import { expect, menuItem, openApp, test, treeItem, work } from "./app";

test("View as Chart: picked axes, Y ticks and X labels", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "View as" }).click();
  await expect(page.getByRole("menu").getByRole("menuitem")).toHaveText(["✓ Table", "Transpose", "Chart"]);
  await menuItem(page, "Chart").click();

  const bar = work(page).getByRole("toolbar").last();
  await expect(bar.getByRole("combobox").locator("option:checked")).toHaveText("user_name");
  await expect(bar.getByRole("button")).toHaveText(["Bar", "Line", "Pie", "id", "amount", "count"]);
  await expect(bar).toContainText("500+ loaded rows · 500 points");
  const labels = await work(page).locator("svg text").allTextContents();
  expect(labels).toEqual(["0", "200K", "400K", "600K", "800K", ...Array.from({ length: 15 }, (_, i) => `user_${1 + i * 35}`)]);
});

test("Chart without the shared WASM says how to recover, and the rest of the window keeps working", async ({ page }) => {
  await page.route("**/*.wasm", (r) => r.abort());
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "View as" }).click();
  await menuItem(page, "Chart").click();
  const msg = work(page).locator(".chart-empty");
  await expect(msg).toContainText("Charts need Qurry's WebAssembly module, which did not load");
  await expect(msg).toContainText("Restart Qurry");
  await expect(treeItem(page, "users").first()).toBeVisible();
});
