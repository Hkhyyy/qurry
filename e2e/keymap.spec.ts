import { dialog, expect, openApp, test } from "./app";

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
