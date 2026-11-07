import { readFileSync } from "node:fs";
import type { Locator, Page } from "@playwright/test";
import { dialog, editor, editorText, expect, menuItem, openApp, openConsole, resultTabs, spyCancels, status, tabStrips, test, treeItem, typeSql, work } from "./app";

// Every default GUI shortcut of the command table (crates/shared/src/keymap-table.json), pressed in its scope in the running app with
// its effect checked, so a handler of its own (useGlobalShortcuts, a Monaco action, a tree / grid / console menu entry by id) that
// breaks or loses its id fails here. One app start per scope: the checks run in order on the same page, each leaving it as the next one
// expects (starting the page, not pressing keys, is what costs time). A new row needs a check: the first test fails until it has one.
// That the default keys are the pinned ones and no two collide is checked by vitest (src/lib/shortcuts.test.ts, "default keys")
const TABLE: { gui?: string; scope?: string; guiKeys?: string[] }[] = JSON.parse(readFileSync(new URL("../crates/shared/src/keymap-table.json", import.meta.url), "utf8"));
const ROWS = TABLE.flatMap((r) => (r.gui ? [{ id: r.gui, scope: r.scope ?? "global", keys: r.guiKeys ?? [] }] : []));

/** Table notation ("Mod+Shift+/") → a Playwright key press on the same physical keys (the app reads e.code) */
const CODES: Record<string, string> = { "/": "Slash", ",": "Comma", ".": "Period", ";": "Semicolon", "`": "Backquote", "[": "BracketLeft", "]": "BracketRight", Up: "ArrowUp", Down: "ArrowDown", Left: "ArrowLeft", Right: "ArrowRight" };
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
const selectedTab = (page: Page) => page.locator(".editor-group [role=tab].tab[aria-selected=true]");
const focusedTabs = (page: Page) => page.locator(".editor-group.focused [role=tab] .tab-label");
const users = (page: Page) => treeItem(page, "users").first();
const conn = (page: Page) => treeItem(page, "[DEV] shop_app 3");
async function twoTabs(page: Page) {
  await users(page).dblclick();
  await treeItem(page, "contracts").first().dblclick();
  await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
}
const fontSize = (page: Page) => page.evaluate(() => localStorage.getItem("dbtool.editorFontSize"));
const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());
const gridCell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
const gridInfo = (page: Page) => work(page).locator(".tb-info");
/** Focus the user_1 cell of the users grid, scrolled back to the top (a new row scrolls to the end) */
async function firstRow(page: Page) {
  await work(page).locator(".grid-scroll").evaluate((e) => e.scrollTo(0, 0));
  await gridCell(page, "user_1").click();
}

type Check = (page: Page, key: string) => Promise<void>;
/** ⎋ closes what the key opened */
async function escapeFrom(page: Page, what: Locator) {
  await page.keyboard.press("Escape");
  await expect(what).toHaveCount(0);
}
const searchEverywhere = (tab: string): Check => async (page, key) => {
  await page.keyboard.press(key);
  await expect(seTab(page)).toHaveText(tab);
  await escapeFrom(page, page.locator(".search-everywhere"));
};
/** The key (after `select`) opens a dialog with this title */
const modal = (title: string, select?: (page: Page) => Promise<void>): Check => async (page, key) => {
  await select?.(page);
  await page.keyboard.press(key);
  await expect(dialog(page).locator(".ui-modal-title")).toContainText(title);
  await escapeFrom(page, dialog(page));
};
/** The key on the users table node opens (or shows) a tab with this name */
const opensTab = (name: string): Check => async (page, key) => {
  await users(page).click();
  await page.keyboard.press(key);
  await expect(selectedTab(page)).toContainText(name);
};
/** Console text `before` (the caret at its end, then `prep`) → the key → `after` */
const edits = (before: string, after: string | RegExp, prep?: (page: Page) => Promise<unknown>): Check => async (page, key) => {
  await editor(page).click();
  await page.keyboard.press("Escape");
  await typeSql(page, before);
  await prep?.(page);
  await page.keyboard.press(key);
  if (typeof after === "string") await expect.poll(() => editorText(page)).toBe(after);
  else await expect.poll(() => editorText(page)).toMatch(after);
};
/** Like `edits`, then typing X replaces what the key selected */
const selects = (before: string, after: string, prep?: (page: Page, key: string) => Promise<unknown>): Check => async (page, key) => {
  await edits(before, before, (page) => prep?.(page, key) ?? Promise.resolve())(page, key);
  await page.keyboard.type("X");
  await expect.poll(() => editorText(page)).toBe(after);
};
/** A grid edit on user_1 makes `n` changes pending; Revert Selected (⌘⌥Z) drops it again unless `revert` is false */
const pending = (n = 1, revert = true): Check => async (page, key) => {
  await firstRow(page);
  await page.keyboard.press(key);
  await expect(gridInfo(page)).toContainText(`${n} pending`);
  if (!revert) return;
  await page.keyboard.press("Meta+Alt+KeyZ");
  await expect(gridInfo(page)).not.toContainText("pending");
};

type Step = [id: string, check: Check];
const SCOPES: { name: string; setup?: (page: Page) => Promise<void>; steps: Step[] }[] = [
  {
    name: "global",
    steps: [
      ["goto.object", searchEverywhere("Database")],
      ["goto.symbol", searchEverywhere("Symbols")],
      ["goto.file", searchEverywhere("Files")],
      ["find.action", searchEverywhere("Actions")],
      ["keymap", modal("Keymap Reference")],
      ["settings", modal("Settings")],
      ["theme", async (page, key) => {
        await page.keyboard.press(key);
        await expect(popoverTitle(page)).toHaveText("Theme");
        await escapeFrom(page, popoverTitle(page));
      }],
      ["font.bigger", async (page, key) => {
        await page.keyboard.press(key);
        await expect.poll(() => fontSize(page)).toBe("14");
      }],
      ["font.smaller", async (page, key) => {
        await page.keyboard.press(key);
        await page.keyboard.press(key);
        await expect.poll(() => fontSize(page)).toBe("12");
      }],
      ["find.window", async (page, key) => {
        await page.keyboard.press(key);
        await expect(page.locator("[data-toolwindow=find]")).toBeVisible();
        await page.keyboard.press(key);
        await expect(page.locator("[data-toolwindow=find]")).toHaveCount(0);
      }],
      ["fts", modal("Full-text Search", (page) => users(page).click())],
      ["explorer", async (page, key) => {
        await users(page).click();
        // Focused in the tree: hides it; again: shows it and focuses it
        await page.keyboard.press(key);
        await expect(page.locator(".ws-explorer.on")).toHaveCount(0);
        await page.keyboard.press(key);
        await expect(page.locator(".ws-explorer.on [data-tree]")).toBeFocused();
      }],
      // From here the users table is open
      ["selectIn", async (page, key) => {
        await users(page).dblclick();
        await conn(page).click();
        await expect(users(page)).toHaveAttribute("aria-selected", "false");
        await gridCell(page, "user_1").click();
        await page.keyboard.press(key);
        await expect(users(page)).toHaveAttribute("aria-selected", "true");
      }],
      ["recent", async (page, key) => {
        await page.keyboard.press(key);
        await expect(popoverTitle(page)).toHaveText("Recent Files");
        await escapeFrom(page, popoverTitle(page));
      }],
      ["maximize", async (page, key) => {
        await gridCell(page, "user_1").click();
        await page.keyboard.press(key);
        await expect(page.locator(".ws-explorer")).toHaveCount(0);
        await page.keyboard.press(key);
        await expect(page.locator(".ws-explorer.on")).toBeVisible();
      }],
      // users | contracts | payments, the middle one active: the next and the previous tab differ
      ["tab.next", async (page, key) => {
        await treeItem(page, "contracts").first().dblclick();
        await treeItem(page, "payments").first().dblclick();
        await expect.poll(() => tabStrips(page)).toEqual(["users | contracts | payments"]);
        await page.getByRole("tab", { name: /contracts/ }).click();
        await page.keyboard.press(key);
        await expect(selectedTab(page)).toContainText("payments");
      }],
      ["tab.prev", async (page, key) => {
        await page.getByRole("tab", { name: /contracts/ }).click();
        await page.keyboard.press(key);
        await expect(selectedTab(page)).toContainText("users");
      }],
      ["tab.close", async (page, key) => {
        await page.getByRole("tab", { name: /payments/ }).click();
        await page.keyboard.press(key);
        await expect.poll(() => tabStrips(page)).toEqual(["users | contracts"]);
      }],
      ["tab.reopen", async (page, key) => {
        await page.keyboard.press(key);
        await expect.poll(() => tabStrips(page)).toEqual(["users | contracts | payments"]);
      }],
      // payments split off to the right: each key goes to the other group and back
      ["split.other", async (page, key) => {
        if ((await tabStrips(page)).length === 1) {
          await page.getByRole("tab", { name: /payments/ }).click({ button: "right" });
          await menuItem(page, "Split and Move Right").click();
          await expect(focusedTabs(page)).toHaveText(["payments"]);
        }
        await page.keyboard.press(key);
        await expect(focusedTabs(page)).toHaveText(["users", "contracts"]);
        await page.keyboard.press(key);
        await expect(focusedTabs(page)).toHaveText(["payments"]);
      }],
      ["console.new", async (page, key) => {
        await conn(page).click();
        await page.keyboard.press(key);
        await expect(page.locator(".editor-group.focused [role=tab].tab[aria-selected=true]")).toContainText("console [DEV] shop_app");
      }],
      ["refresh", async (page, key) => {
        // Outside the tree, in a console: the Data Source list again
        await editor(page).click();
        const calls = await spyCalls(page);
        await page.keyboard.press(key);
        await expect.poll(calls).toContain("list_connections");
      }],
    ],
  },
  {
    name: "tree",
    steps: [
      ["tree.editData", async (page, key) => {
        const table = key === "F4" ? "contracts" : "users";
        await treeItem(page, table).first().click();
        await page.keyboard.press(key);
        await expect(selectedTab(page)).toContainText(table);
      }],
      ["tree.ddl", opensTab("users [DDL]")],
      ["tree.sqlGen", opensTab("SQL Generator: users")],
      ["tree.ddlConsole", async (page, key) => {
        await opensTab("console [DEV] shop_app")(page, key);
        await expect.poll(() => editorText(page)).toContain("CREATE TABLE");
      }],
      ["tree.ddlCopy", async (page, key) => {
        await users(page).click();
        await page.keyboard.press(key);
        await expect(status(page)).toHaveText("DDL of public.users copied to clipboard");
      }],
      ["tree.copyName", async (page, key) => {
        await users(page).click();
        await page.keyboard.press(key);
        await expect.poll(() => clipboard(page)).toBe("users");
      }],
      ["tree.copyRef", async (page, key) => {
        await users(page).click();
        await page.keyboard.press(key);
        await expect.poll(() => clipboard(page)).toBe("public.users");
      }],
      ["tree.quickDoc", async (page, key) => {
        await users(page).click();
        await page.keyboard.press(key);
        await expect(page.locator(".quick-doc .qd-title")).toHaveText("shop_app.public.users");
        await escapeFrom(page, page.locator(".quick-doc"));
      }],
      ["tree.modify", modal("Modify Table", (page) => users(page).click())],
      ["tree.rename", async (page, key) => {
        await users(page).click();
        await page.keyboard.press(key);
        await expect(dialog(page)).toContainText('Rename table "users"');
        await escapeFrom(page, dialog(page));
      }],
      ["tree.properties", async (page, key) => {
        await conn(page).click();
        await page.keyboard.press(key);
        await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Data Source Properties");
        await expect(dialog(page).getByRole("textbox").first()).toHaveValue("[DEV] shop_app");
        await escapeFrom(page, dialog(page));
      }],
      ["tree.newDs", modal("New Data Source", (page) => conn(page).click())],
      ["tree.duplicate", async (page, key) => {
        await conn(page).click();
        await page.keyboard.press(key);
        await expect(dialog(page).getByRole("textbox").first()).toHaveValue("[DEV] shop_app (copy)");
        await escapeFrom(page, dialog(page));
      }],
      ["tree.jumpConsole", async (page, key) => {
        await conn(page).click();
        await page.keyboard.press(key);
        await expect(menuItem(page, /^New Query Console/)).toBeVisible();
        // A click outside closes the menu
        await users(page).click();
        await expect(menuItem(page, /^New Query Console/)).toHaveCount(0);
      }],
      ["tree.refresh", async (page, key) => {
        await users(page).click();
        // A table node reloads its columns · keys · indexes (nothing in the background sends this)
        const calls = await spyCalls(page);
        await page.keyboard.press(key);
        await expect.poll(calls).toContain("table_detail");
      }],
      ["tree.remove", async (page, key) => {
        await users(page).click();
        await page.keyboard.press(key);
        await expect(dialog(page)).toContainText("DROP TABLE public.users;");
        await dialog(page).getByRole("button", { name: "Cancel" }).click();
        await conn(page).click();
        await page.keyboard.press(key);
        await expect(dialog(page)).toContainText('Delete data source "[DEV] shop_app"?');
        await dialog(page).getByRole("button", { name: "Cancel" }).click();
        await expect(dialog(page)).toHaveCount(0);
      }],
      // Last: it closes the connection
      ["tree.disconnect", async (page, key) => {
        await conn(page).click();
        const calls = await spyCalls(page);
        await page.keyboard.press(key);
        await expect.poll(calls).toContain("disconnect");
      }],
    ],
  },
  {
    name: "editor",
    setup: (page) => openConsole(page),
    steps: [
      ["editor.reformat", edits("select a from b", /^SELECT\s+a\s+FROM\s+b$/)],
      ["editor.blockComment", edits("select 1", "/* select 1 */", (page) => page.keyboard.press("Meta+A"))],
      // On an underlined unknown name: its quick fixes (the closest names)
      ["editor.quickFix", async (page, key) => {
        await editor(page).click();
        await typeSql(page, "select nme from users");
        await expect(editor(page).locator(".squiggly-warning")).toHaveCount(1);
        await page.keyboard.press("Home");
        for (let i = 0; i < 8; i++) await page.keyboard.press("ArrowRight");
        // Monaco's own code-action request for the moving caret can supersede the key's on a slow runner (Linux WebKit in CI:
        // "No code actions available"); the key is pressed again then, as a user would
        await expect(async () => {
          await page.keyboard.press(key);
          await expect(page.getByText("Change to user_name")).toBeVisible({ timeout: 1_500 });
        }).toPass({ timeout: 10_000 });
        await page.keyboard.press("Escape");
      }],
      ["editor.duplicate", edits("select 1", "select 1\nselect 1")],
      ["editor.deleteLine", edits("one\ntwo", "one")],
      // ⌥⇧↑ / ⌥⇧↓, ⌥↑ / ⌥↓ and ⌃G also cover monacoKey's ⌃ / ⌥ / ⇧ bits and arrow keys (Monaco actions get their keys from ConsoleEditor, not the global handler)
      ["editor.moveUp", edits("one\ntwo", "two\none")],
      ["editor.moveDown", edits("one\ntwo", "two\none", (page) => page.keyboard.press("ArrowUp"))],
      ["editor.extend", selects("select abc", "select X", (page) => page.keyboard.press("ArrowLeft"))],
      ["editor.shrink", selects("select abc", "select X", async (page) => {
        await page.keyboard.press("ArrowLeft");
        await page.keyboard.press("Alt+ArrowUp");
        await page.keyboard.press("Alt+ArrowUp");
      })],
      ["editor.nextOcc", selects("ab ab", "X X", (page, key) => page.keyboard.press(key))],
      ["editor.allOcc", selects("ab ab ab", "X X X")],
      ["editor.toggleCase", edits("select", "SELECT", (page) => page.keyboard.press("Meta+A"))],
      ["editor.gotoLine", async (page, key) => {
        await editor(page).click();
        await page.keyboard.press(key);
        await expect(page.locator(".quick-input-widget")).toBeVisible();
        await page.keyboard.press("Escape");
        await expect(page.locator(".quick-input-widget")).toBeHidden();
      }],
      ["editor.replace", async (page, key) => {
        await editor(page).click();
        await page.keyboard.press(key);
        await expect(page.locator(".find-widget.replaceToggled")).toBeVisible();
        await page.keyboard.press("Escape");
        await expect(page.locator(".find-widget.visible")).toHaveCount(0);
      }],
      ["editor.history", async (page, key) => {
        await editor(page).click();
        await page.keyboard.press(key);
        await expect(dialog(page)).toContainText("[DEV] shop_app Query History");
        await escapeFrom(page, dialog(page));
      }],
      ["console.rename", async (page, key) => {
        await page.getByRole("tab", { name: /console/ }).click();
        await page.keyboard.press(key);
        await expect(dialog(page)).toContainText("Rename console");
        await expect(dialog(page).getByRole("textbox")).toHaveValue("console [DEV] shop_app");
        await escapeFrom(page, dialog(page));
      }],
      ["editor.run", async (page, key) => {
        await editor(page).click();
        await typeSql(page, "select 1;");
        await page.keyboard.press(key);
        await expect(resultTabs(page)).toHaveCount(2);
      }],
      ["editor.cancel", async (page, key) => {
        await editor(page).click();
        await typeSql(page, "select pg_sleep(3);");
        await page.keyboard.press("Meta+Enter");
        await expect(page.locator(".statusbar .running-bar")).toBeVisible();
        const cancels = await spyCancels(page);
        await page.keyboard.press(key);
        await expect.poll(cancels).toBe(1);
      }],
    ],
  },
  {
    // The users table: editable, row 1 focused
    name: "grid",
    setup: (page) => users(page).dblclick(),
    steps: [
      ["grid.valueEditor", async (page, key) => {
        await firstRow(page);
        await page.keyboard.press(key);
        await expect(work(page).locator(".value-panel .ve-title")).toContainText("user_name : ");
      }],
      ["grid.find", async (page, key) => {
        await firstRow(page);
        await page.keyboard.press(key);
        await expect(work(page).locator(".grid-find input")).toBeFocused();
        await page.keyboard.press("Escape");
      }],
      ["grid.setNull", pending()],
      ["grid.setDefault", pending()],
      ["grid.deleteRows", pending()],
      ["grid.revert", async (page, key) => {
        await firstRow(page);
        await page.keyboard.press("Meta+Alt+KeyN");
        await expect(gridInfo(page)).toContainText("1 pending");
        await page.keyboard.press(key);
        await expect(gridInfo(page)).not.toContainText("pending");
      }],
      // A new row goes to the end and is selected; rows loaded while scrolling to it go before it and the selection follows it, so
      // Revert Selected takes it back. A clone isn't selected (the source row stays so): Submit sends it
      ["grid.addRow", pending(1)],
      ["grid.cloneRow", pending(1, false)],
      ["grid.submit", async (page, key) => {
        await firstRow(page);
        const calls = await spyCalls(page);
        await page.keyboard.press(key);
        await expect.poll(calls).toContain("submit_edits");
      }],
      ["grid.reload", async (page, key) => {
        await firstRow(page);
        const calls = await spyCalls(page);
        await page.keyboard.press(key);
        await expect.poll(calls).toContain("run_query");
      }],
      ["grid.ddl", async (page, key) => {
        await firstRow(page);
        await page.keyboard.press(key);
        await expect.poll(() => tabStrips(page)).toEqual(["users | users [DDL]"]);
        await page.locator(".tab-label", { hasText: /^users$/ }).click();
      }],
      // The current column (row 1's user_name cell) by one step of 4 characters
      ...(["grid.colWiden", "grid.colNarrow"] as const).map((id): Step => [id, async (page, key) => {
        await firstRow(page);
        const head = work(page).getByRole("columnheader", { name: /^user_name / });
        const w = await head.evaluate((e) => e.getBoundingClientRect().width);
        await page.keyboard.press(key);
        await expect.poll(() => head.evaluate((e) => e.getBoundingClientRect().width)).toBeCloseTo(w + (id === "grid.colWiden" ? 4 : -4) * 7.3, -1);
      }]),
      ["grid.cancel", async (page, key) => {
        // A filtered reload that still runs (the mock sleeps on pg_sleep)
        await work(page).locator(".dg-filter-field input").first().fill("pg_sleep(3) is not null");
        await page.keyboard.press("Enter");
        await firstRow(page);
        const calls = await spyCalls(page);
        await page.keyboard.press(key);
        await expect.poll(calls).toContain("cancel_query");
      }],
    ],
  },
  {
    // ⌘1~⌘9: nine saved workspaces, each key switches to its own
    name: "workspaces",
    steps: [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n): Step => [`ws.${n}`, async (page, key) => {
      await page.keyboard.press(key);
      await expect(page.getByRole("button", { name: `W${n} W${n}`, exact: true })).toHaveAttribute("aria-pressed", "true");
    }]),
  },
];
const NINE = JSON.stringify({ version: 2, shared: true, current: "w5", workspaces: [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({ id: `w${n}`, name: `W${n}`, connIds: n === 5 ? ["dev"] : [], tabs: [], panes: { t: "leaf", g: 0 }, activeIds: [-1, -1, -1, -1], focused: 0 })) });

test("every command with a default GUI key is pressed in its own scope (commands without one: e2e/keymap.spec.ts binds a few)", () => {
  const pressed = SCOPES.flatMap((s) => s.steps.map(([id]) => `${id} (${s.name === "workspaces" ? "global" : s.name})`));
  expect(pressed.sort()).toEqual(ROWS.filter((r) => r.keys.length).map((r) => `${r.id} (${r.scope})`).sort());
});

for (const scope of SCOPES)
  test(`${scope.name} shortcuts`, async ({ page }) => {
    // Several checks in one test: the default 30 s per test is for one
    test.setTimeout(30_000 + 5_000 * scope.steps.length);
    if (scope.name === "workspaces") await page.addInitScript((ws) => localStorage.setItem("mock-ws", ws), NINE);
    await openApp(page);
    await scope.setup?.(page);
    for (const [id, check] of scope.steps)
      for (const spec of ROWS.find((r) => r.id === id)!.keys) await test.step(`${id}: ${spec}`, () => check(page, press(spec)));
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
