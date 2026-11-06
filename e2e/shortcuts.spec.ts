import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";
import { dialog, editor, expect, openApp, openConsole, resultTabs, spyCancels, tabStrips, test, treeItem, typeSql, work } from "./app";

// A few default GUI shortcuts of the command table (crates/shared/src/keymap-table.json) per scope, pressed in the running app with
// their effect checked: each way a key reaches its command (the global key handler, a tree / grid / console menu entry by id, a Monaco
// action, Search Everywhere) and the keys several scopes share (⌘b, ⌘d, ⌘↩, ⌘F2, ⇧F6), where the focus decides which command runs.
// Every command's id and default keys, and that no two collide, are checked by vitest (src/lib/shortcuts.test.ts, "default keys")
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
/** users | contracts | payments, the middle one active: the next and the previous tab differ */
async function threeTabs(page: Page) {
  await twoTabs(page);
  await treeItem(page, "payments").first().dblclick();
  await expect.poll(() => tabStrips(page)).toEqual(["users | contracts | payments"]);
  await page.getByRole("tab", { name: /contracts/ }).click();
  await expect(selectedTab(page)).toContainText("contracts");
}

type Check = (page: Page, key: string) => Promise<void>;
const gridCell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
const gridInfo = (page: Page) => work(page).locator(".tb-info");
/** The users table open, its user_1 cell focused */
async function usersGrid(page: Page) {
  await users(page).dblclick();
  await gridCell(page, "user_1").click();
}

const CHECKS: Record<string, Check> = {
  // ── Global ──
  "goto.object": async (page, key) => {
    await page.keyboard.press(key);
    await expect(seTab(page)).toHaveText("Database");
  },
  keymap: async (page, key) => {
    await page.keyboard.press(key);
    await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Keymap Reference");
  },
  theme: async (page, key) => {
    await page.keyboard.press(key);
    await expect(popoverTitle(page)).toHaveText("Theme");
  },
  "selectIn": async (page, key) => {
    await users(page).dblclick();
    await conn(page).click();
    await expect(users(page)).toHaveAttribute("aria-selected", "false");
    await gridCell(page, "user_1").click();
    await page.keyboard.press(key);
    await expect(users(page)).toHaveAttribute("aria-selected", "true");
  },
  "tab.next": async (page, key) => {
    await threeTabs(page);
    await page.keyboard.press(key);
    await expect(selectedTab(page)).toContainText("payments");
  },
  "tab.reopen": async (page, key) => {
    await twoTabs(page);
    await page.keyboard.press("Meta+KeyW");
    await expect.poll(() => tabStrips(page)).toEqual(["users"]);
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
  },
  // ── Database Explorer ──
  "tree.ddl": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users [DDL]"]);
  },
  "tree.copyName": async (page, key) => {
    await users(page).click();
    await page.keyboard.press(key);
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("users");
  },
  "tree.duplicate": async (page, key) => {
    await conn(page).click();
    await page.keyboard.press(key);
    await expect(dialog(page).getByRole("textbox").first()).toHaveValue("[DEV] shop_app (copy)");
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
    await expect(page.locator(".statusbar [title^='Cancel Running Statements']")).toBeEnabled();
    const cancels = await spyCancels(page);
    await page.keyboard.press(key);
    await expect.poll(cancels).toBe(1);
  },
  "editor.duplicate": async (page, key) => {
    await consoleWith(page, "select 1");
    await page.keyboard.press(key);
    await expect.poll(() => editorText(page)).toBe("select 1\nselect 1");
  },
  "console.rename": async (page, key) => {
    await openConsole(page);
    await page.getByRole("tab", { name: /console/ }).click();
    await page.keyboard.press(key);
    await expect(dialog(page)).toContainText("Rename console");
    await expect(dialog(page).getByRole("textbox")).toHaveValue("console [DEV] shop_app");
  },
  // ── Data grid (the users table: editable, row 1 focused) ──
  "grid.cloneRow": async (page, key) => {
    await usersGrid(page);
    await page.keyboard.press(key);
    await expect(gridInfo(page)).toContainText("1 pending");
  },
  "grid.submit": async (page, key) => {
    await usersGrid(page);
    await page.keyboard.press("Meta+Alt+KeyN");
    const calls = await spyCalls(page);
    await page.keyboard.press(key);
    await expect.poll(calls).toContain("execute_batch");
  },
  "grid.ddl": async (page, key) => {
    await usersGrid(page);
    await page.keyboard.press(key);
    await expect.poll(() => tabStrips(page)).toEqual(["users | users [DDL]"]);
  },
  "grid.cancel": async (page, key) => {
    // The users table while a filtered reload still runs (the mock sleeps on pg_sleep)
    await usersGrid(page);
    await work(page).locator(".dg-filter-field input").first().fill("pg_sleep(3) is not null");
    await page.keyboard.press("Enter");
    await gridCell(page, "user_1").click();
    const calls = await spyCalls(page);
    await page.keyboard.press(key);
    await expect.poll(calls).toContain("cancel_query");
  },
  // ⌘1~⌘9 are built from `ws.${n}` (one handler): the last one stands for all nine
  "ws.9": async (page, key) => {
    await page.keyboard.press(key);
    await expect(page.getByRole("button", { name: "W9 W9", exact: true })).toHaveAttribute("aria-pressed", "true");
  },
};
const NINE = JSON.stringify({ version: 2, shared: true, current: "w5", workspaces: [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({ id: `w${n}`, name: `W${n}`, connIds: n === 5 ? ["dev"] : [], tabs: [], panes: { t: "leaf", g: 0 }, activeIds: [-1, -1, -1, -1], focused: 0 })) });

const PRESSED = ROWS.filter((r) => r.id in CHECKS);

test("the checks press default keys of commands in every scope (commands without a default key: e2e/keymap.spec.ts binds a few)", () => {
  expect(PRESSED.filter((r) => r.keys.length).map((r) => r.id).sort()).toEqual(Object.keys(CHECKS).sort());
  expect([...new Set(PRESSED.map((r) => r.scope))].sort()).toEqual(["editor", "global", "grid", "tree"]);
});

for (const r of PRESSED)
  for (const spec of r.keys)
    test(`${r.scope} ${r.id}: ${spec}`, async ({ page }) => {
      if (r.id.startsWith("ws.")) await page.addInitScript((ws) => localStorage.setItem("mock-ws", ws), NINE);
      await openApp(page);
      await CHECKS[r.id](page, press(spec));
    });

// The fixed keys outside the table (as in DataGrip)
test.describe("fixed keys", () => {
  test.beforeEach(async ({ page }) => openApp(page));

  test("⇧⇧ opens Search Everywhere on All", async ({ page }) => {
    await page.keyboard.press("Shift");
    await page.keyboard.press("Shift");
    await expect(seTab(page)).toHaveText("All");
  });

  test("⇧⎋ hides the Find tool window, or the explorer, whichever has the focus", async ({ page }) => {
    await page.keyboard.press("Meta+Shift+KeyF");
    await page.locator("[data-toolwindow=find]").focus();
    await page.keyboard.press("Shift+Escape");
    await expect(page.locator("[data-toolwindow=find]")).toHaveCount(0);
    await users(page).click();
    await page.keyboard.press("Shift+Escape");
    await expect(page.locator(".ws-explorer.on")).toHaveCount(0);
  });

  test("⌃⇥ opens the Switcher on the tab used before, and releasing ⌃ shows it; ⌃⇧⇥ too", async ({ page }) => {
    await twoTabs(page);
    for (const key of ["Tab", "Shift+Tab"]) {
      await expect(selectedTab(page)).toContainText("contracts");
      await page.keyboard.down("Control");
      await page.keyboard.press(key);
      await expect(popoverTitle(page)).toHaveText("Switcher");
      await page.keyboard.up("Control");
      await expect(popoverTitle(page)).toHaveCount(0);
      await expect(selectedTab(page)).toContainText("users");
      await page.getByRole("tab", { name: /contracts/ }).click();
    }
  });
});
