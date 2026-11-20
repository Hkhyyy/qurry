import { dialog, editor, editorText, expect, openApp, openConsole, resultTabs, runAll, status, test, typeSql } from "./app";

// User parameters (#173): `:name` in code asks for its value in the Parameters dialog before the run; everything after it sees the
// final SQL, the editor keeps its text
const params = (page: Parameters<typeof dialog>[0]) => dialog(page).filter({ has: page.locator(".ui-modal-title", { hasText: /^Parameters$/ }) });
const output = (page: Parameters<typeof dialog>[0]) => page.locator(".work-layer:visible .output-log");
const SQL = "select * from users where id = :id and name = ':notaparam' -- :c";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => {
    throw e;
  });
});

test("asks for each name in code only; Cancel runs nothing and leaves no mark", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, SQL);
  await page.evaluate(() => ((window as any).__mockRan = []));
  await runAll(page);
  await expect(params(page).locator(".ui-field-label")).toHaveText([":id"]);
  await expect(params(page).locator(".ui-hl-param")).toHaveText([":id"]);
  await expect(params(page).getByRole("textbox")).toBeFocused();
  await params(page).getByRole("button", { name: "Cancel" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__mockRan)).toEqual([]);
  await expect(editor(page).locator(".stmt-mark")).toHaveCount(0);
  await expect(resultTabs(page)).toHaveText(["Output"]);
});

test("Run sends the substituted statement, Output echoes it, and the next run offers the value again; Esc cancels", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, SQL);
  await page.evaluate(() => ((window as any).__mockRan = []));
  await runAll(page);
  await params(page).getByRole("textbox").fill("42");
  // A stray click outside the box keeps the dialog and the typed value
  await page.mouse.click(5, 5);
  await expect(params(page).getByRole("textbox")).toHaveValue("42");
  await params(page).getByRole("textbox").focus();
  await page.keyboard.press("Enter");
  await expect(resultTabs(page)).toHaveText(["Output", /^1: users/]);
  const sent = "select * from users where id = 42 and name = ':notaparam' -- :c";
  expect(await page.evaluate(() => (window as any).__mockRan)).toEqual([sent]);
  // The editor keeps :id
  expect(await editorText(page)).toBe(SQL);
  await resultTabs(page).filter({ hasText: "Output" }).click();
  await expect(output(page)).toContainText(sent);
  await editor(page).click();
  await runAll(page);
  await expect(params(page).getByRole("textbox")).toHaveValue("42");
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__mockRan)).toEqual([sent]);
});

test("::int casts don't ask", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select id::int from users");
  await runAll(page);
  await expect(resultTabs(page)).toHaveText(["Output", /^1: users/]);
  await expect(dialog(page)).toHaveCount(0);
});

test("on Production the confirmation lists the statement with the value in", async ({ page }) => {
  await openApp(page);
  await openConsole(page, "[PROD] shop_app");
  await typeSql(page, "delete from users where id = :id");
  await runAll(page);
  await params(page).getByRole("textbox").fill("7");
  await params(page).getByRole("button", { name: "Run" }).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Production: [PROD] shop_app");
  await expect(dialog(page).locator(".ui-detail-text")).toHaveText(["delete from users where id = 7"]);
  await dialog(page).getByRole("button", { name: "Cancel" }).click();
  await expect(status(page)).toHaveText("Ready");
});

test("without the WebAssembly module the console runs its text as before, with no parameter dialog or notice", async ({ page }) => {
  await page.route((url) => url.pathname.endsWith("/qurry_shared_wasm_bg.wasm") && !url.search, (route) => route.abort());
  await page.addInitScript(() => localStorage.setItem("dbtool.execSettings", JSON.stringify({ inside: "ask", outside: "nothing", selection: "single" })));
  await openApp(page);
  // openConsoleNoWasm (wasm.spec.ts): the autocomplete notice when the console opens, acknowledged
  await page.locator(".ws-explorer.on .tree-node", { hasText: "[DEV] shop_app" }).first().click();
  await page.keyboard.press("Meta+Shift+L");
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await editor(page).click();
  await typeSql(page, "select * from users where id = :id");
  await page.evaluate(() => ((window as any).__mockRan = []));
  await runAll(page);
  // The one question: nothing can be checked for danger without the module (stmtKind unsafeReason), so it asks before running
  await dialog(page).filter({ hasText: "can't check this statement" }).getByRole("button", { name: "Run Anyway" }).click();
  await expect(resultTabs(page)).toHaveText(["Output", /^1: users/]);
  await expect(dialog(page)).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__mockRan)).toEqual(["select * from users where id = :id"]);
});
