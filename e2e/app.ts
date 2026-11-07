import { test as base, expect, type Page } from "@playwright/test";

/** Every spec runs the app as on macOS (the shipped platform): ⌘⌥⌃⇧ labels and ⌘ shortcuts on any OS */
export const test = base.extend({
  page: async ({ page }, use) => {
    await page.addInitScript(() => {
      const ua = navigator.userAgent.replace(/\([^)]*\)/, "(Macintosh; Intel Mac OS X 10_15_7)");
      Object.defineProperty(navigator, "platform", { get: () => "MacIntel" });
      Object.defineProperty(navigator, "userAgent", { get: () => ua });
    });
    await use(page);
  },
});
export { expect };

/** Open the mock app and wait for the explorer */
export async function openApp(page: Page) {
  await page.goto("");
  await expect(page.locator(".tree-node", { hasText: "[DEV] shop_app" })).toBeVisible();
}

/** Open the mock app in a theme (settings.json "theme", which the mock keeps in localStorage), with custom themes if given */
export async function openInTheme(page: Page, theme: string, custom: unknown[] = []) {
  await page.goto("");
  await page.evaluate(([theme, custom]) => localStorage.setItem("mock-themeSettings", JSON.stringify({ theme, sync: { dark: "dark", light: "light" }, custom })), [theme, custom] as const);
  await openApp(page);
}

export const treeNode = (page: Page, text: string) => page.locator(".ws-explorer.on .tree-node", { hasText: text }).first();
/** Explorer row by its exact accessible name ("users", "tables 2", "[DEV] shop_app 3") */
export const treeItem = (page: Page, name: string) => page.locator(".ws-explorer.on").getByRole("treeitem", { name, exact: true });

/** Expand an explorer node with → */
export async function expand(page: Page, name: string) {
  await treeItem(page, name).click();
  await page.keyboard.press("ArrowRight");
  await expect(treeItem(page, name)).toHaveAttribute("aria-expanded", "true");
}

/** public.users, then billing.users (same table name in another schema) */
export async function openBothUsers(page: Page) {
  await treeItem(page, "users").first().dblclick();
  await expand(page, "billing");
  await expand(page, "tables 2");
  await treeItem(page, "users").nth(1).dblclick();
  await expect(page.getByRole("tab", { name: /billing\.users/ })).toBeVisible();
}

/** New Query Console (⇧⌘L) on a data source, focused and empty */
export async function openConsole(page: Page, conn = "[DEV] shop_app") {
  await treeNode(page, conn).click();
  await page.keyboard.press("Meta+Shift+L");
  await expect(editor(page)).toBeVisible();
  await editor(page).click();
}

/** The visible console editor */
export const editor = (page: Page) => page.locator(".work-layer:visible .monaco-editor").first();

/** Replace the console text (typed, so completion and auto-closing quotes behave as for a user) */
export async function typeSql(page: Page, sql: string) {
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(sql);
}

/** Select everything and execute (⌘↩): each statement runs */
export async function runAll(page: Page) {
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Meta+Enter");
}

/** Tab labels of each visible editor group, in DOM order ("users | contracts"; "" for an empty group) */
export const tabStrips = (page: Page) =>
  page.locator(".editor-group").evaluateAll((els) =>
    els.filter((e) => (e as HTMLElement).offsetParent).map((e) => [...e.querySelectorAll("[role=tab].tab .tab-label")].map((t) => t.textContent).join(" | ")),
  );

export const work = (page: Page) => page.locator(".work-layer:visible");
export const suggestions = (page: Page) => page.locator(".suggest-widget.visible .monaco-list-row");
export const resultTabs = (page: Page) => page.locator(".work-layer:visible .result-tabs [role=tab]");
export const dialog = (page: Page) => page.getByRole("dialog");
export const menuItem = (page: Page, name: string | RegExp) => page.getByRole("menuitem", { name });
export const status = (page: Page) => page.locator(".statusbar .sb-text");

/** Counts cancel_query calls from now on (the status bar shows a run instead of its text, so a cancel is seen reaching the backend) */
export async function spyCancels(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__cancels = 0;
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: any) => (cmd === "cancel_query" && w.__cancels++, orig(cmd, args));
  });
  return () => page.evaluate(() => (window as any).__cancels as number);
}
