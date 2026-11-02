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

test("the keymap editor records shortcuts when the WebAssembly module fails to load", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await openApp(page);
  await page.keyboard.press("Meta+Comma");
  await dialog(page).locator(".settings-nav").getByText("Keymap", { exact: true }).click();
  const row = dialog(page).locator(".keymap-edit-row", { hasText: "Go to Database Object" });
  await row.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Meta+KeyJ");
  // Labels stay in the stored notation without the module, but the key is bound
  await expect(dialog(page).locator(".keymap-editor-foot .hint")).toHaveText("Go to Database Object: Mod+J");
  await expect(row.locator(".keymap-chip")).toHaveText(["Mod+J×"]);
});
