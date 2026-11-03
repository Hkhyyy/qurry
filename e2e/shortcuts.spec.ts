import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";
import { dialog, editor, expect, menuItem, openApp, openConsole, resultTabs, status, tabStrips, test, treeItem, typeSql } from "./app";

// Every default GUI shortcut of the command table (crates/shared/src/keymap-table.json), pressed in its scope, with its effect checked.
// A new row needs a check here: the first test fails until it has one
const TABLE: { gui?: string; scope?: string; guiKeys?: string[] }[] = JSON.parse(readFileSync(new URL("../crates/shared/src/keymap-table.json", import.meta.url), "utf8"));
const ROWS = TABLE.flatMap((r) => (r.gui ? [{ id: r.gui, scope: r.scope ?? "global", keys: r.guiKeys ?? [] }] : []));

/** Table notation ("Mod+Shift+/") → a Playwright key press on the same physical keys (the app reads e.code) */
const CODES: Record<string, string> = { "/": "Slash", ",": "Comma", ".": "Period", ";": "Semicolon", "`": "Backquote", "[": "BracketLeft", "]": "BracketRight", Up: "ArrowUp", Down: "ArrowDown" };
const press = (spec: string) =>
  spec
    .split("+")
    .map((k, i, a) => (i < a.length - 1 ? { Mod: "Meta", Ctrl: "Control", Alt: "Alt", Shift: "Shift" }[k]! : CODES[k] ?? (/^[A-Z]$/.test(k) ? `Key${k}` : /^\d$/.test(k) ? `Digit${k}` : k)))
    .join("+");

/** Backend commands the app sends from now on */
async function spyCalls(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__calls = [];
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: unknown) => (w.__calls.push(cmd), orig(cmd, args));
  });
  return () => page.evaluate(() => (window as any).__calls as string[]);
}

const popoverTitle = (page: Page) => page.locator(".ui-popover-title");
const seTab = (page: Page) => page.locator(".search-everywhere [role=tab][aria-selected=true]");
const selectedTab = (page: Page) => page.locator(".editor-group [role=tab][aria-selected=true]");
/** The console's text: its rendered lines in order (Monaco keeps them in any DOM order; WebKit's innerText drops the breaks) */
const editorText = (page: Page) =>
  editor(page)
    .locator(".view-line")
    .evaluateAll((els) =>
      els
        .map((e) => [parseFloat((e as HTMLElement).style.top), e.textContent ?? ""] as const)
        .sort((x, y) => x[0] - y[0])
        .map(([, t]) => t.replace(/\u00a0/g, " "))
        .join("\n"),
    );
/** Console with this text, the caret at its end */
async function consoleWith(page: Page, sql: string) {
  await openConsole(page);
  await typeSql(page, sql);
}
const users = (page: Page) => treeItem(page, "users").first();
const conn = (page: Page) => treeItem(page, "[DEV] shop_app 3");
async function twoTabs(page: Page) {
  await users(page).dblclick();
  await treeItem(page, "contracts").first().dblclick();
  await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
}
const fontSize = (page: Page) => page.evaluate(() => localStorage.getItem("dbtool.editorFontSize"));

type Check = (page: Page, key: string) => Promise<void>;
const searchEverywhere = (tab: string): Check => async (page, key) => {
  await page.keyboard.press(key);
  await expect(seTab(page)).toHaveText(tab);
};

const CHECKS: Record<string, Check> = {
  // ── Global ──
  "goto.object": searchEverywhere("Database"),
  "goto.symbol": searchEverywhere("Symbols"),
  "goto.file": searchEverywhere("Files"),
  "find.action": searchEverywhere("Actions"),
  fts: async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Full-text Search");
  },
  recent: async (page, key) => {
    await users(page).dblclick();
    await page.keyboard.press(key);
    await expect(popoverTitle(page)).toHaveText("Recent Files");
  },
  keymap: async (page, key) => {
    await page.keyboard.press(key);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Keymap Reference");
  },
  explorer: async (page, key) => {
    await users(page).click();
    // Focused in the tree: hides it; again: shows it and focuses it
    await page.keyboard.press(key);
    await expect(page.locator(".ws-explorer.on")).toHaveCount(0);
    await page.keyboard.press(key);
    await expect(page.locator(".ws-explorer.on [data-tree]")).toBeFocused();
  },
  "find.window": async (page, key) => {
    await page.keyboard.press(key);
    await expect(page.locator("[data-toolwindow=find]")).toBeVisible();
    await page.keyboard.press(key);
    await expect(page.locator("[data-toolwindow=find]")).toHaveCount(0);
  },
  maximize: async (page, key) => {
    await users(page).dblclick();
    await page.keyboard.press(key);
    await expect(page.locator(".ws-explorer")).toHaveCount(0);
    await page.keyboard.press(key);
    await expect(page.locator(".ws-explorer.on")).toBeVisible();
  },
  settings: async (page, key) => {
    await page.keyboard.press(key);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Settings");
  },
  theme: async (page, key) => {
    await page.keyboard.press(key);
    await expect(popoverTitle(page)).toHaveText("Theme");
  },
  "font.bigger": async (page, key) => {
    await page.keyboard.press(key);
    await expect.poll(() => fontSize(page)).toBe("14");
  },
  "font.smaller": async (page, key) => {
    await page.keyboard.press(key);
    await expect.poll(() => fontSize(page)).toBe("12");
  },
  "console.new": async (page, key) => {
    await conn(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["console [DEV] shop_app"]);
  },
  refresh: async (page, key) => {
    // Outside the tree, with no table tab: the Data Source list again
    await openConsole(page);
    const calls = await spyCalls(page);
    await page.keyboard.press(key);
    await expect.poll(calls).toContain("list_connections");
  },
  "tab.close": async (page, key) => {
    await twoTabs(page);
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users"]);
  },
  "tab.reopen": async (page, key) => {
    await twoTabs(page);
    await page.keyboard.press("Meta+KeyW");
    await expect.poll(() => tabStrips(page)).toEqual(["users"]);
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
  },
  "tab.next": async (page, key) => {
    await twoTabs(page);
    await page.keyboard.press(key);
    await expect(selectedTab(page)).toContainText("users");
  },
  "tab.prev": async (page, key) => {
    await twoTabs(page);
    await page.keyboard.press(key);
    await expect(selectedTab(page)).toContainText("users");
  },
  "split.other": async (page, key) => {
    await twoTabs(page);
    await page.getByRole("tab", { name: /contracts/ }).click({ button: "right" });
    await menuItem(page, "Split and Move Right").click();
    await expect(page.locator(".editor-group.focused")).toContainText("contracts");
    await page.keyboard.press(key);
    await expect(page.locator(".editor-group.focused [role=tab] .tab-label")).toHaveText(["users"]);
  },
  // ── Database Explorer ──
  "tree.editData": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users"]);
  },
  "tree.jumpConsole": async (page, key) => {
    await conn(page).click();
    await page.keyboard.press(key);
    await expect(menuItem(page, /^New Query Console/)).toBeVisible();
  },
  "tree.ddl": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users [DDL]"]);
  },
  "tree.sqlGen": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["SQL Generator: users"]);
  },
  "tree.ddlCopy": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect(status(page)).toHaveText("DDL of public.users copied to clipboard");
  },
  "tree.ddlConsole": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["console [DEV] shop_app"]);
    await expect.poll(() => editorText(page)).toContain("CREATE TABLE");
  },
  "tree.copyRef": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("public.users");
  },
  "tree.quickDoc": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect(page.locator(".quick-doc .qd-title")).toHaveText("shop_app.public.users");
  },
  "tree.modify": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page)).toContainText("users");
    await expect(dialog(page).locator(".ui-modal-title")).toContainText("Modify Table");
  },
  "tree.rename": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page)).toContainText('Rename table "users"');
  },
  "tree.properties": async (page, key) => {
    await conn(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Data Source Properties");
    await expect(dialog(page).getByRole("textbox").first()).toHaveValue("[DEV] shop_app");
  },
  "tree.newDs": async (page, key) => {
    await conn(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("New Data Source");
  },
  "tree.duplicate": async (page, key) => {
    await conn(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page).getByRole("textbox").first()).toHaveValue("[DEV] shop_app (copy)");
  },
  "tree.refresh": async (page, key) => {
    await users(page).click();
    const calls = await spyCalls(page);
    await page.keyboard.press(key);
    await expect.poll(calls).not.toEqual([]);
  },
  "tree.disconnect": async (page, key) => {
    await conn(page).click();
    const calls = await spyCalls(page);
    await page.keyboard.press(key);
    await expect.poll(calls).toContain("disconnect");
  },
  "tree.remove": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page)).toContainText('DROP TABLE public.users;');
    await dialog(page).getByRole("button", { name: "Cancel" }).click();
    await conn(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page)).toContainText('Delete data source "[DEV] shop_app"?');
  },
  // ── Query console ──
  "editor.run": async (page, key) => {
    await consoleWith(page, "select 1;");
    await page.keyboard.press(key);
    await expect(resultTabs(page)).toHaveCount(2);
  },
  "editor.cancel": async (page, key) => {
    await consoleWith(page, "select pg_sleep(3);");
    await page.keyboard.press("Meta+Enter");
    await expect(page.locator(".statusbar [title='Cancel Running Statements']")).toBeEnabled();
    await page.keyboard.press(key);
    await expect(status(page)).toHaveText("Cancel requested");
  },
  "editor.history": async (page, key) => {
    await openConsole(page);
    await page.keyboard.press(key);
    await expect(dialog(page)).toContainText("[DEV] shop_app Query History");
  },
  "editor.reformat": async (page, key) => {
    await consoleWith(page, "select a from b");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toMatch(/^SELECT\s+a\s+FROM\s+b$/);
  },
  "editor.blockComment": async (page, key) => {
    await consoleWith(page, "select 1");
    await page.keyboard.press("Meta+A");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("/* select 1 */");
  },
  "editor.duplicate": async (page, key) => {
    await consoleWith(page, "select 1");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("select 1\nselect 1");
  },
  "editor.deleteLine": async (page, key) => {
    await consoleWith(page, "one\ntwo");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("one");
  },
  "editor.moveUp": async (page, key) => {
    await consoleWith(page, "one\ntwo");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("two\none");
  },
  "editor.moveDown": async (page, key) => {
    await consoleWith(page, "one\ntwo");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("two\none");
  },
  "editor.extend": async (page, key) => {
    await consoleWith(page, "select abc");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press(key);
    await page.keyboard.type("X");
    await expect.poll(() => editorText(page)).toBe("select X");
  },
  "editor.shrink": async (page, key) => {
    await consoleWith(page, "select abc");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Alt+ArrowUp");
    await page.keyboard.press("Alt+ArrowUp");
    await page.keyboard.press(key);
    await page.keyboard.type("X");
    await expect.poll(() => editorText(page)).toBe("select X");
  },
  "editor.nextOcc": async (page, key) => {
    await consoleWith(page, "ab ab");
    await page.keyboard.press(key);
    await page.keyboard.press(key);
    await page.keyboard.type("X");
    await expect.poll(() => editorText(page)).toBe("X X");
  },
  "editor.allOcc": async (page, key) => {
    await consoleWith(page, "ab ab ab");
    await page.keyboard.press(key);
    await page.keyboard.type("X");
    await expect.poll(() => editorText(page)).toBe("X X X");
  },
  "editor.toggleCase": async (page, key) => {
    await consoleWith(page, "select");
    await page.keyboard.press("Meta+A");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("SELECT");
  },
  "editor.gotoLine": async (page, key) => {
    await openConsole(page);
    await page.keyboard.press(key);
    await expect(page.locator(".quick-input-widget")).toBeVisible();
  },
  "editor.replace": async (page, key) => {
    await openConsole(page);
    await page.keyboard.press(key);
    await expect(page.locator(".find-widget.replaceToggled")).toBeVisible();
  },
};
// ⌘1~⌘9: nine saved workspaces, each key switches to its own
for (let n = 1; n <= 9; n++)
  CHECKS[`ws.${n}`] = async (page, key) => {
    await page.keyboard.press(key);
    await expect(page.getByRole("button", { name: `W${n} W${n}`, exact: true })).toHaveAttribute("aria-pressed", "true");
  };
const NINE = JSON.stringify({ version: 2, shared: true, current: "w5", workspaces: [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({ id: `w${n}`, name: `W${n}`, connIds: n === 5 ? ["dev"] : [], tabs: [], panes: { t: "leaf", g: 0 }, activeIds: [-1, -1, -1, -1], focused: 0 })) });

test("every default GUI shortcut has a check", () => {
  expect(Object.keys(CHECKS).sort()).toEqual(ROWS.map((r) => r.id).sort());
});

for (const r of ROWS)
  for (const spec of r.keys)
    test(`${r.scope} ${r.id}: ${spec}`, async ({ page }) => {
      if (r.id.startsWith("ws.")) await page.addInitScript((ws) => localStorage.setItem("mock-ws", ws), NINE);
      await openApp(page);
      await CHECKS[r.id](page, press(spec));
    });
