import { dialog, editor, expect, openApp, openConsole, test } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

test("the query history list scrolls with the selection when the arrow keys move it past the visible rows", async ({ page }) => {
  await openConsole(page);
  // Forty entries, more than the list shows at once (the mock keeps what history_add gets)
  await page.evaluate(async () => {
    const { invoke } = (window as any).__TAURI_INTERNALS__ ?? {};
    for (let i = 0; i < 40; i++) await invoke("history_add", { connId: "dev", sql: `select ${i} as n` });
  });
  await editor(page).click();
  await page.keyboard.press("Meta+Alt+KeyE");
  const list = dialog(page).locator(".hist-list");
  await expect(list.locator(".ui-list-item").first()).toBeVisible();
  for (let i = 0; i < 39; i++) await page.keyboard.press("ArrowDown");
  const selected = list.locator(".ui-list-item.selected");
  await expect(selected).toBeInViewport();
  const inList = async () => {
    const [l, r] = await Promise.all([list.boundingBox(), selected.boundingBox()]);
    return !!l && !!r && r.y >= l.y - 1 && r.y + r.height <= l.y + l.height + 1;
  };
  await expect.poll(inList).toBe(true);
  // And back up to the first
  for (let i = 0; i < 39; i++) await page.keyboard.press("ArrowUp");
  await expect.poll(inList).toBe(true);
  await expect(selected).toContainText("select 39 as n");
});
