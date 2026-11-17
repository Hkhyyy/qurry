import type { Page } from "@playwright/test";
import { dialog, expect, openApp, test } from "./app";

// #189: a dialog (the shared Modal) is moved by dragging its title bar, kept inside the window, and opens in its place again next time

const box = async (page: Page) => (await dialog(page).boundingBox())!;
/** Drag the title bar (right of the title) by dx, dy */
async function dragHead(page: Page, dx: number, dy: number) {
  const head = (await dialog(page).locator(".ui-modal-head").boundingBox())!;
  const x = head.x + head.width - 40;
  const y = head.y + head.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y + dy / 2, { steps: 4 });
  await page.mouse.move(x + dx, y + dy, { steps: 4 });
  await page.mouse.up();
}
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
