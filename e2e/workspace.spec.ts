import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";
import { expect, openApp, tabStrips, test } from "./app";

// Captured from the app as it saves today: two workspaces, same-name tables, a console with SQL, a right split
const fixture = readFileSync(new URL("fixtures/workspace-gui.json", import.meta.url), "utf8");

test("a saved workspace restores and saves back unchanged", async ({ page }) => {
  await page.addInitScript((ws) => {
    // The mock keeps the workspace file in localStorage; count the app's saves
    localStorage.setItem("mock-ws", ws);
    const set = Storage.prototype.setItem;
    (window as any).__wsSaves = 0;
    Storage.prototype.setItem = function (k: string, v: string) {
      if (k === "mock-ws") (window as any).__wsSaves++;
      return set.call(this, k, v);
    };
  }, fixture);
  await openApp(page);
  await expect.poll(() => tabStrips(page)).toEqual(["public.users | shop_app.billing.users | console [DEV] shop_app", "shop_app.billing.users"]);
  await expect(page.getByRole("button", { name: /Reports$/ })).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as any).__wsSaves)).toBeGreaterThan(0);
  const saved = await page.evaluate(() => localStorage.getItem("mock-ws"));
  expect(JSON.parse(saved!)).toEqual(JSON.parse(fixture));
});

test("tab labels catch up once WebAssembly loads after the tabs restored", async ({ page }) => {
  await page.addInitScript((ws) => localStorage.setItem("mock-ws", ws), fixture);
  // Hold the module fetch until the restored tabs have drawn with the fallback labels
  let release = () => {};
  const held = new Promise<void>((r) => (release = r));
  await page.route((u) => u.pathname.endsWith(".wasm") && !u.search, async (r) => {
    await held;
    await r.continue();
  });
  await openApp(page);
  await expect.poll(() => tabStrips(page)).toEqual(["users | users | console [DEV] shop_app", "users"]);
  release();
  await expect.poll(() => tabStrips(page)).toEqual(["public.users | shop_app.billing.users | console [DEV] shop_app", "shop_app.billing.users"]);
});

// ── Each workspace keeps its own explorer, the hidden ones mounted ──
const tree = (page: Page) => page.locator(".ws-explorer.on");
const row = (page: Page, name: string) => tree(page).getByRole("treeitem", { name, exact: true });

test("the keys act on the shown workspace's selection, not a hidden one's", async ({ page }) => {
  await page.addInitScript((ws) => localStorage.setItem("mock-ws", ws), fixture);
  await openApp(page);
  // Reports: [MySQL] shop's contracts selected
  await page.getByRole("button", { name: /Reports$/ }).click();
  await row(page, "contracts").click();
  await expect(tree(page).locator(".tree-node.selected")).toHaveCount(1);
  // Default has nothing selected: ⌦ in its explorer asks nothing (not a Drop… of the hidden workspace's contracts)
  await page.getByRole("button", { name: /Default$/ }).click();
  await tree(page).locator(".tree").focus();
  await page.keyboard.press("Delete");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  // Back in Reports, its own selection is what ⌦ drops
  await page.getByRole("button", { name: /Reports$/ }).click();
  await tree(page).locator(".tree").focus();
  await page.keyboard.press("Delete");
  await expect(page.getByRole("dialog").locator(".ui-detail-text")).toHaveText(["DROP TABLE shop.contracts;"]);
});
