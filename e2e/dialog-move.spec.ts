import type { Locator, Page } from "@playwright/test";
import { dialog, expect, openApp, test } from "./app";

// #189: a dialog (the shared Modal) and a floating popup (Popover: Search Everywhere, Quick Switch Theme, Switcher, Recent Files) are
// moved by dragging their title bar or header, kept inside the window, and open in their place again next time

const box = async (page: Page) => (await dialog(page).boundingBox())!;
/** Press at x, y and drag by dx, dy */
async function dragFrom(page: Page, x: number, y: number, dx: number, dy: number) {
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y + dy / 2, { steps: 4 });
  await page.mouse.move(x + dx, y + dy, { steps: 4 });
  await page.mouse.up();
}
/** Drag the title bar (right of the title) by dx, dy */
async function dragHead(page: Page, dx: number, dy: number) {
  const head = (await dialog(page).locator(".ui-modal-head").boundingBox())!;
  await dragFrom(page, head.x + head.width - 40, head.y + head.height / 2, dx, dy);
}
const at = async (l: Locator) => {
  const b = (await l.boundingBox())!;
  return { x: Math.round(b.x), y: Math.round(b.y), right: Math.round(b.x + b.width), bottom: Math.round(b.y + b.height) };
};
const settings = async (page: Page) => {
  await page.keyboard.press("Meta+Comma");
  await expect(dialog(page)).toBeVisible();
};

test("Settings moves by its title bar, stays inside the window, and opens in its place again", async ({ page }) => {
  await openApp(page);
  await settings(page);
  const start = await box(page);
  await dragHead(page, -120, 60);
  await expect.poll(async () => ({ x: Math.round((await box(page)).x), y: Math.round((await box(page)).y) })).toEqual({ x: Math.round(start.x - 120), y: Math.round(start.y + 60) });
  // Same size: only moved
  expect(Math.round((await box(page)).width)).toBe(Math.round(start.width));

  // Can't leave the window: dragged far past each corner it stops at the edge
  const view = page.viewportSize()!;
  await dragHead(page, 2000, 2000);
  let b = await box(page);
  expect(Math.round(b.x + b.width)).toBe(view.width);
  expect(Math.round(b.y + b.height)).toBe(view.height);
  await dragHead(page, -2000, -2000);
  b = await box(page);
  expect(Math.round(b.x)).toBe(0);
  expect(Math.round(b.y)).toBe(0);

  // A narrower window pulls a moved dialog back in
  await dragHead(page, 2000, 0);
  await page.setViewportSize({ width: 1100, height: view.height });
  await expect.poll(async () => Math.round((await box(page)).x + (await box(page)).width)).toBeLessThanOrEqual(1100);
  await page.setViewportSize(view);

  // Keys still go to the dialog: Escape closes it; it opens where it first did
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await settings(page);
  const again = await box(page);
  expect([Math.round(again.x), Math.round(again.y)]).toEqual([Math.round(start.x), Math.round(start.y)]);
});

test("a drag in the dialog's body doesn't move it", async ({ page }) => {
  await openApp(page);
  await settings(page);
  const start = await box(page);
  // Dragging over the page tree (in the body) selects, doesn't move
  const tree = (await dialog(page).getByRole("tree", { name: "Settings" }).boundingBox())!;
  await page.mouse.move(tree.x + 20, tree.y + 10);
  await page.mouse.down();
  await page.mouse.move(tree.x + 120, tree.y + 80, { steps: 4 });
  await page.mouse.up();
  const b = await box(page);
  expect([Math.round(b.x), Math.round(b.y)]).toEqual([Math.round(start.x), Math.round(start.y)]);
  await expect(dialog(page)).toBeVisible();
});

test("Search Everywhere moves by its header, not by its search field; typing and the keys still work after a drag", async ({ page }) => {
  await openApp(page);
  const se = page.locator(".search-everywhere");
  const input = se.locator("input");
  const open = async () => {
    await page.keyboard.press("Meta+KeyO");
    await expect(input).toBeFocused();
  };
  await open();
  const start = await at(se);
  // The empty part of the header, between the tabs and the scope
  const tabs = (await se.getByRole("tablist").boundingBox())!;
  const scope = (await se.getByRole("combobox", { name: "Search scope" }).boundingBox())!;
  const grab = { x: (tabs.x + tabs.width + scope.x) / 2, y: tabs.y + tabs.height / 2 };
  await dragFrom(page, grab.x, grab.y, -150, 80);
  await expect.poll(() => at(se)).toMatchObject({ x: start.x - 150, y: start.y + 80 });
  // The focus stayed in the search field: typing searches, ↓ moves the selection
  await expect(input).toBeFocused();
  await page.keyboard.type("users");
  await expect(input).toHaveValue("users");
  const options = se.getByRole("listbox").getByRole("option");
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowDown");
  await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");
  // A drag that starts on the search field doesn't move the popup
  const field = (await input.boundingBox())!;
  const moved = await at(se);
  await dragFrom(page, field.x + 20, field.y + field.height / 2, 200, 120);
  expect(await at(se)).toEqual(moved);
  // Clamped: dragged far past the bottom right, it stops at the window's edges
  const view = page.viewportSize()!;
  await dragFrom(page, grab.x - 150, grab.y + 80, 3000, 3000);
  await expect.poll(() => at(se)).toMatchObject({ right: view.width, bottom: view.height });
  // Esc closes it; it opens where it first did
  await page.keyboard.press("Escape");
  await expect(se).toHaveCount(0);
  await open();
  expect(await at(se)).toEqual(start);
});

test("Quick Switch Theme moves by its title, stays inside the window, opens in its place again; the keys still pick", async ({ page }) => {
  await openApp(page);
  const pop = page.locator(".ui-popover");
  const open = async () => {
    await page.keyboard.press("Control+Backquote");
    await expect(pop.getByRole("option").first()).toBeVisible();
  };
  await open();
  const start = await at(pop);
  const title = (await pop.locator(".ui-popover-title").boundingBox())!;
  const grab = { x: title.x + title.width / 2, y: title.y + title.height / 2 };
  await dragFrom(page, grab.x, grab.y, -3000, -3000);
  await expect.poll(() => at(pop)).toMatchObject({ x: 0, y: 0 });
  await page.keyboard.press("Escape");
  await expect(pop).toHaveCount(0);
  await open();
  expect(await at(pop)).toEqual(start);
  await dragFrom(page, grab.x, grab.y, 120, 60);
  await expect.poll(() => at(pop)).toMatchObject({ x: start.x + 120, y: start.y + 60 });
  // Typing still filters and ↩ still picks: "light" leaves Light, Solarized Light and Sync with OS (Dark / Light)
  await page.keyboard.type("light");
  await expect(pop.getByRole("option")).toHaveText([/^Light/, /Solarized Light/, /Sync with OS/]);
  await page.keyboard.press("ArrowDown");
  await expect(pop.getByRole("option").nth(1)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(pop).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe("light");
});
