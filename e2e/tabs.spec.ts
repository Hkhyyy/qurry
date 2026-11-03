import type { Locator } from "@playwright/test";
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

/** Computed style of a tab, and the theme's accent / dimmed-bar colors resolved the same way */
const tabStyle = (tab: Locator) =>
  tab.evaluate((e) => {
    const s = getComputedStyle(e);
    const resolve = (v: string) => {
      const probe = document.body.appendChild(document.createElement("i"));
      probe.style.color = v;
      const c = getComputedStyle(probe).color;
      probe.remove();
      return c;
    };
    return {
      weight: getComputedStyle(e.querySelector(".tab-label")!).fontWeight,
      color: s.color,
      bg: s.backgroundColor,
      bar: s.borderBottomColor,
      stripe: s.boxShadow,
      width: e.getBoundingClientRect().width,
      accent: resolve("var(--accent)"),
      dim: resolve("var(--c-6f737a)"),
    };
  });

for (const theme of ["dark", "light", "hc"]) {
  test(`the active tab stands out (${theme}): bold, own background, accent bar; gray bar in the other split pane`, async ({ page }) => {
    await page.evaluate((t) => localStorage.setItem("dbtool.theme", t), theme);
    await openApp(page);
    await treeItem(page, "users").first().dblclick();
    await treeItem(page, "contracts").first().dblclick();
    const users = page.getByRole("tab", { name: /users/ });
    const contracts = page.getByRole("tab", { name: /contracts/ });
    await expect(contracts).toHaveAttribute("aria-selected", "true");
    const [on, off] = [await tabStyle(contracts), await tabStyle(users)];
    expect([on.weight, off.weight]).toEqual(["600", "400"]);
    expect(on.color).not.toBe(off.color);
    expect(on.bg).not.toBe(off.bg);
    expect(on.bar).toBe(on.accent);
    expect(off.bar).toBe("rgba(0, 0, 0, 0)");
    // Both tabs are of [DEV] shop_app: the inactive one keeps the same Data Source stripe
    expect(off.stripe).toBe(on.stripe);
    expect(on.stripe).toContain("inset");
    // Becoming active (bold) doesn't change a tab's width
    await users.click();
    await expect(users).toHaveAttribute("aria-selected", "true");
    expect((await tabStyle(users)).width).toBe(off.width);

    await contracts.click({ button: "right" });
    await menuItem(page, "Split Right").click();
    await expect(page.locator(".editor-group")).toHaveCount(2);
    // Focus the left pane: its active tab keeps the accent bar, the right pane's active tab gets the dimmed one
    await page.locator(".editor-group").first().getByRole("tab").first().click();
    const [left, right] = [page.locator(".editor-group").first(), page.locator(".editor-group").last()];
    const focusedTab = await tabStyle(left.locator(".tab.active"));
    const otherTab = await tabStyle(right.locator(".tab.active"));
    expect(focusedTab.bar).toBe(focusedTab.accent);
    expect(otherTab.bar).toBe(otherTab.dim);
    expect(otherTab.weight).toBe("600");
  });
}
