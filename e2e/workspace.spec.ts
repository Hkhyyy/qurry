import { readFileSync } from "node:fs";
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
