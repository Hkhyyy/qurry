import { expect, openApp, openConsole, resultTabs, status, test, typeSql } from "./app";

// ⌘↩ splits and plans the console in the shared WebAssembly module, which main.tsx loads without waiting
// The module's fetch only, not the `?url` import Vite serves as a script
const WASM = (url: URL) => url.pathname.endsWith("/qurry_shared_wasm_bg.wasm") && !url.search;

test("⌘↩ while the WebAssembly module is still loading runs once it has loaded", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>((r) => (release = r));
  await page.route(WASM, async (route) => {
    await gate;
    await route.continue();
  });
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+Enter");
  release();
  await expect(resultTabs(page)).toHaveText(["Output", /^users/]);
  await expect(status(page)).toHaveText("1 statement executed");
});

test("⌘↩ without the WebAssembly module says why and how to run anyway, in user words", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await openApp(page);
  await openConsole(page, "[PROD] shop_app");
  await typeSql(page, "select 1");
  await page.keyboard.press("Meta+Enter");
  const output = page.locator(".work-layer:visible .output-log");
  await expect(output).toContainText("the system WebView is too old (update macOS, or webkit2gtk on Linux)");
  await expect(output).toContainText("For selection execute › Exactly as a single statement");
  await expect(output).not.toContainText("pnpm wasm");
  await expect(output).not.toContainText("initShared");
});

test("without the WebAssembly module a selection run as one statement still runs", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await page.addInitScript(() => localStorage.setItem("dbtool.execSettings", JSON.stringify({ inside: "ask", outside: "nothing", selection: "single" })));
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Meta+Enter");
  await expect(resultTabs(page)).toHaveText(["Output", /^users/]);
});
