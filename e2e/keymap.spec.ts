import type { Page } from "@playwright/test";
import { dialog, expect, openApp, tabStrips, test, treeItem, work } from "./app";

// The command table is imported as JSON, the key labels are drawn by the shared WebAssembly module (main.tsx loads it without waiting),
// and by a small TS copy of its rule until then
const WASM = (url: URL) => url.pathname.endsWith("/qurry_shared_wasm_bg.wasm") && !url.search;

test("shortcuts work while the WebAssembly module loads, with ⌘⌥⌃⇧ labels before and after", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>((r) => (release = r));
  await page.route(WASM, async (route) => {
    await gate;
    await route.continue();
  });
  // The app's own instance of src/lib/shared.ts (Vite serves it at this path): which side drew the label
  const ready = () => page.evaluate(() => import(/* @vite-ignore */ "/src/lib/shared.ts").then((m) => m.sharedReady() as boolean));
  await openApp(page);
  const row = dialog(page).locator(".keymap-row", { hasText: "Go to Database Object" }).locator(".keymap-keys");
  const open = async () => {
    await page.keyboard.press("Meta+Shift+Slash");
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Keymap Reference");
  };
  await open();
  // Drawn by the TS copy while it loads, not the stored "Mod+O"
  expect(await ready()).toBe(false);
  await expect(row).toHaveText("⌘o");
  release();
  await expect.poll(ready).toBe(true);
  // Drawn by the module once it has loaded
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await open();
  await expect(row).toHaveText("⌘o");
});

test("the keymap editor records shortcuts when the WebAssembly module fails to load", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await openApp(page);
  await page.keyboard.press("Meta+Comma");
  await dialog(page).locator(".settings-nav").getByText("Keymap", { exact: true }).click();
  const row = dialog(page).locator(".keymap-edit-row", { hasText: "Go to Database Object" });
  await row.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Meta+KeyJ");
  // The key is bound, and labeled ⌘⌥⌃⇧ without the module too
  await expect(dialog(page).locator(".keymap-editor-foot .hint")).toHaveText("Go to Database Object: ⌘j");
  await expect(row.locator(".keymap-chip")).toHaveText(["⌘j×"]);
});

// Every menu, toolbar and Find Action command is in Settings › Keymap, the ones without a default key too: a key given to one runs it
// where its section says (Global: anywhere; Database Explorer: on the selected node; Data Grid: in the focused grid)
async function bind(page: Page, section: string, title: string, key: string, label: string) {
  await page.keyboard.press("Meta+Comma");
  await dialog(page).locator(".settings-nav").getByText("Keymap", { exact: true }).click();
  const exact = (s: string) => new RegExp(`^${s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  const scope = dialog(page).locator(".keymap-section", { has: page.locator(".keymap-title", { hasText: exact(section) }) });
  const row = scope.locator(".keymap-edit-row", { has: page.locator(".keymap-desc", { hasText: exact(title) }) });
  await expect(row.locator(".keymap-chips")).toHaveText("none");
  await row.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press(key);
  await expect(row.locator(".keymap-chip")).toHaveText([`${label}×`]);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
}

test("a key given to a Find Action command that had none runs it, and Find Action shows it", async ({ page }) => {
  await openApp(page);
  await bind(page, "Global", "Theme: High Contrast", "Meta+KeyJ", "⌘j");
  await page.keyboard.press("Meta+KeyJ");
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe("hc");
  await page.keyboard.press("Meta+Shift+KeyA");
  await page.keyboard.type("High contrast");
  await expect(page.locator(".search-everywhere").getByText("⌘j", { exact: true })).toBeVisible();
});

test("a key given to a tree menu command runs it on the selected node, a tab menu command on the active tab", async ({ page }) => {
  await openApp(page);
  await bind(page, "Database Explorer", "Truncate…", "Meta+KeyJ", "⌘j");
  await bind(page, "Global", "Close Other Tabs", "Meta+KeyK", "⌘k");
  await treeItem(page, "users").first().click();
  await page.keyboard.press("Meta+KeyJ");
  await expect(dialog(page)).toContainText("TRUNCATE public.users;");
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await treeItem(page, "users").first().dblclick();
  await treeItem(page, "contracts").first().dblclick();
  await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
  await page.keyboard.press("Meta+KeyK");
  await expect.poll(() => tabStrips(page)).toEqual(["contracts"]);
});

test("a key given to a grid toolbar command runs it in the focused grid", async ({ page }) => {
  await openApp(page);
  await bind(page, "Data Grid", "View as Transposed Table", "Meta+KeyJ", "⌘j");
  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await page.keyboard.press("Meta+KeyJ");
  await expect(work(page).locator(".grid-scroll.transposed")).toBeVisible();
});
