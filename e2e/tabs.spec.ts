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

/** The geometry of each inset shadow in a computed box-shadow ("<color> 1px 0px 0px 0px inset, …" → ["1px 0px 0px 0px inset", …]) */
const shadowShapes = (boxShadow: string) => boxShadow.split(/,(?![^(]*\))/).map((x) => x.match(/(-?[\d.]+px ){4}inset$/)?.[0]);

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
    // Both tabs are of [DEV] shop_app: the inactive one keeps the same Data Source stripe (hc adds side lines to the active one)
    expect(on.stripe).toContain(off.stripe);
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
    // Nothing (like an hc outline) covers the 2px stripe or the bar of either active tab: only the top stripe, plus side lines in hc
    for (const t of [focusedTab, otherTab]) {
      expect(t.stripe).toContain(off.stripe);
      expect(shadowShapes(t.stripe)).toEqual([...(theme === "hc" ? ["1px 0px 0px 0px inset", "-1px 0px 0px 0px inset"] : []), "0px 2px 0px 0px inset"]);
    }
  });
}

test("DDL tabs show their object's tree icon: a view's differs from a function's (#83)", async ({ page }) => {
  const iconOf = (l: Locator) => l.locator("svg").first().evaluate((s) => s.outerHTML);
  const tabIconOf = (name: RegExp) => iconOf(page.getByRole("tab", { name }).locator(".tab-icon"));
  // [DEV] shop_app's public schema (the MySQL source has the same groups further down)
  const node = (name: string) => treeItem(page, name).first();
  const open = async (name: string) => {
    await node(name).click();
    await page.keyboard.press("ArrowRight");
    await expect(node(name)).toHaveAttribute("aria-expanded", "true");
  };
  await open("views 1");
  await node("v_active_users").click();
  await page.keyboard.press("Meta+B");
  await expect(page.getByRole("tab", { name: /v_active_users \[DDL\]/ })).toBeVisible();
  await open("routines 1");
  await node("calc_fee(amount numeric) numeric").dblclick();
  await expect(page.getByRole("tab", { name: /calc_fee \[DDL\]/ })).toBeVisible();

  const [view, fn] = [await tabIconOf(/v_active_users \[DDL\]/), await tabIconOf(/calc_fee \[DDL\]/)];
  expect(view).not.toEqual(fn);
  // DDL tabs carry the DDL mark; a data tab of the same view does not
  await expect(page.getByRole("tab", { name: /\[DDL\]/ }).locator(".ui-ddl-mark")).toHaveCount(2);
  await node("v_active_users").dblclick();
  await expect(page.getByRole("tab", { name: /^v_active_users(?! \[DDL\])/ }).locator(".ui-ddl-mark")).toHaveCount(0);
  expect(await tabIconOf(/^v_active_users(?! \[DDL\])/)).toEqual(view);
  expect(view).toEqual(await iconOf(node("v_active_users").locator(".node-icon")));
  expect(fn).toEqual(await iconOf(node("calc_fee(amount numeric) numeric").locator(".node-icon")));
  // The same icon in Recent Files (⌘E)
  await page.keyboard.press("Meta+E");
  const recent = page.getByRole("option", { name: /v_active_users \[DDL\]/ }).locator(".ui-list-icon");
  expect(await iconOf(recent)).toEqual(view);
  await expect(recent.locator(".ui-ddl-mark")).toHaveCount(1);
});

test("Go to DDL from a view's data tab opens the view's DDL, with the view icon (#83)", async ({ page }) => {
  const node = treeItem(page, "v_active_users").first();
  await treeItem(page, "views 1").first().click();
  await page.keyboard.press("ArrowRight");
  await node.dblclick();
  await expect(page.getByRole("tab", { name: /^v_active_users/ })).toBeVisible();
  await page.locator(".work-layer:visible").getByTitle("Go to DDL").click();
  const ddl = page.getByRole("tab", { name: /v_active_users \[DDL\]/ });
  await expect(ddl.locator(".ui-ddl-mark")).toHaveCount(1);
  const svg = (l: Locator) => l.locator("svg").first().evaluate((s) => s.outerHTML);
  expect(await svg(ddl.locator(".tab-icon"))).toEqual(await svg(node.locator(".node-icon")));
});
