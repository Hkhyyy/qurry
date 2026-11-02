import { dialog, expect, openApp, test } from "./app";

// The command table is imported as JSON, the key labels are drawn by the shared WebAssembly module (main.tsx loads it without waiting)
const WASM = (url: URL) => url.pathname.endsWith("/qurry_shared_wasm_bg.wasm") && !url.search;

test("shortcuts work while the WebAssembly module loads, and their labels turn to ⌘⌥⌃⇧ once it has", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>((r) => (release = r));
  await page.route(WASM, async (route) => {
    await gate;
    await route.continue();
  });
  await openApp(page);
  await page.keyboard.press("Meta+Shift+Slash");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Keymap Reference");
  const row = dialog(page).locator(".keymap-row", { hasText: "Go to Database Object" }).locator(".keymap-keys");
  // Until then the key as keymap.json stores it
  await expect(row).toHaveText("Mod+O");
  release();
  await expect(row).toHaveText("⌘o");
});
