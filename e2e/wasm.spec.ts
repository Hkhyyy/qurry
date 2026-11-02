import type { Page } from "@playwright/test";
import { dialog, editor, expect, openApp, openConsole, resultTabs, status, test, treeNode, typeSql, work } from "./app";

// ⌘↩ splits and plans the console in the shared WebAssembly module, which main.tsx loads without waiting
// The module's fetch only, not the `?url` import Vite serves as a script
const WASM = (url: URL) => url.pathname.endsWith("/qurry_shared_wasm_bg.wasm") && !url.search;

/** openConsole without the module: opening a console says once that autocomplete is off, acknowledged before typing */
async function openConsoleNoWasm(page: Page, conn = "[DEV] shop_app") {
  await treeNode(page, conn).click();
  await page.keyboard.press("Meta+Shift+L");
  const notice = dialog(page).filter({ hasText: "Autocomplete is off" });
  await notice.getByRole("button", { name: "OK" }).click();
  await expect(notice).toHaveCount(0);
  await editor(page).click();
}

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

test("while the WebAssembly module loads, ⌘↩ shows it and runs once, the editor as it is when the module arrives", async ({ page }) => {
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
  await expect(work(page).locator(".running-bar")).toContainText("Loading the statement splitter");
  await expect(work(page).getByTitle(/^Execute/)).toBeDisabled();
  // Pressed again after an edit: still one run, of the current text
  await page.keyboard.press("Meta+Enter");
  await typeSql(page, "select * from contracts");
  await page.keyboard.press("Meta+Enter");
  release();
  await expect(resultTabs(page)).toHaveText(["Output", /^contracts/]);
  await expect(work(page).locator(".running-bar")).toHaveCount(0);
  await resultTabs(page).first().click();
  await expect(work(page).locator(".output-log .ol.sql")).toHaveCount(1);
  await expect(work(page).locator(".output-log .ol.sql")).toContainText("select * from contracts");
});

test("⌘↩ without the WebAssembly module says why and how to run anyway, in user words", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await openApp(page);
  await openConsoleNoWasm(page, "[PROD] shop_app");
  await typeSql(page, "select 1");
  await page.keyboard.press("Meta+Enter");
  const output = page.locator(".work-layer:visible .output-log");
  await expect(output).toContainText("the system WebView is too old (update macOS, or webkit2gtk on Linux)");
  await expect(output).toContainText("For selection execute › Exactly as a single statement");
  await expect(output).not.toContainText("pnpm wasm");
  await expect(output).not.toContainText("initShared");
  // Nothing reached the server: the status bar must not read as a failed query
  await expect(status(page)).toHaveText("Not run: the statement splitter couldn't load (see Output)");
});

test("without the WebAssembly module a selection run as one statement still runs", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await page.addInitScript(() => localStorage.setItem("dbtool.execSettings", JSON.stringify({ inside: "ask", outside: "nothing", selection: "single" })));
  await openApp(page);
  await openConsoleNoWasm(page);
  await typeSql(page, "select * from users");
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Meta+Enter");
  await expect(resultTabs(page)).toHaveText(["Output", /^users/]);
});

test("⌘F2 cancels a ⌘↩ still waiting for the WebAssembly module, like a run", async ({ page }) => {
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
  await expect(work(page).locator(".running-bar")).toContainText("Loading the statement splitter");
  await page.keyboard.press("Meta+F2");
  await expect(work(page).locator(".running-bar")).toHaveCount(0);
  // The cancelled run never starts once the module is there
  const loaded = page.waitForResponse((r) => WASM(new URL(r.url())));
  release();
  await loaded;
  await page.waitForTimeout(500);
  await expect(resultTabs(page)).toHaveText(["Output"]);
  await expect(work(page).locator(".output-log .ol.sql")).toHaveCount(0);
});

test("without the WebAssembly module, a DDL on Production may change the schema (not just can't count)", async ({ page }) => {
  await page.route(WASM, (route) => route.abort());
  await page.addInitScript(() => localStorage.setItem("dbtool.execSettings", JSON.stringify({ inside: "ask", outside: "nothing", selection: "single" })));
  await openApp(page);
  await openConsoleNoWasm(page, "[PROD] shop_app");
  await typeSql(page, "alter table users add column x int");
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Meta+Enter");
  await expect(dialog(page).locator(".ui-detail-value")).toHaveText("can't count · may change the schema");
});
