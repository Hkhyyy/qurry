import type { Page } from "@playwright/test";
import { dialog, expect, menuItem, openApp, openConsole, test, treeItem, typeSql, work } from "./app";

// Hidden and pinned columns (#178, DataGrip's Hide Column / Pin Column / Columns…): the column header's menu, the Columns… list,
// a table tab's layout kept per table in the settings (the mock keeps it across a reload, as the file is), a console result's in memory

const header = (page: Page, name: string) => work(page).getByRole("columnheader", { name: new RegExp(`^(🔑 ?)?${name} `) });
const heads = (page: Page) => work(page).locator(".grid-head [role=columnheader] .col-name").allTextContents();
const cell = (page: Page, name: string) => work(page).getByRole("gridcell", { name, exact: true });
const segments = (page: Page) => page.locator(".statusbar .sb-seg").allTextContents();
const columns = (page: Page) => page.locator(".ui-checklist");
const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());

async function headerMenu(page: Page, name: string, item: RegExp) {
  await header(page, name).click({ button: "right" });
  await menuItem(page, item).click();
}

async function openUsers(page: Page) {
  await treeItem(page, "users").first().dblclick();
  await expect(cell(page, "user_1")).toBeVisible();
}

async function runWide(page: Page) {
  await openConsole(page);
  await typeSql(page, "select * from wide_demo");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  await expect(header(page, "column_1")).toBeVisible();
}

test("header menu hides a column (header, cells and the status bar's count); Columns… finds it and shows it again; Show All", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await header(page, "amount").click({ button: "right" });
  for (const label of ["Hide Column", "Pin Column", "Columns…"]) await expect(menuItem(page, new RegExp(`^${label}`))).toBeVisible();
  await menuItem(page, /^Hide Column/).click();
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "created_at", "memo"]);
  await expect(cell(page, "1234.50")).toHaveCount(0);
  await expect.poll(() => segments(page)).toContain("1 hidden");

  // Columns…: every column, the hidden one unchecked; the search narrows the list, a click shows it again
  await headerMenu(page, "memo", /^Columns…/);
  await expect(columns(page).getByRole("option")).toHaveCount(5);
  await expect(columns(page).getByRole("checkbox", { name: "amount" })).not.toBeChecked();
  await expect(columns(page).getByRole("checkbox", { name: "memo" })).toBeChecked();
  await page.keyboard.type("amo");
  await expect(columns(page).getByRole("option")).toHaveCount(1);
  await expect(columns(page).getByRole("button", { name: "Show All" })).toBeEnabled();
  await page.keyboard.press("Enter");
  await expect(columns(page).getByRole("checkbox", { name: "amount" })).toBeChecked();
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "amount", "created_at", "memo"]);
  await expect(columns(page).getByRole("button", { name: "Show All" })).toBeDisabled();

  // Two hidden from the list, then Show All
  await columns(page).getByPlaceholder("Search").fill("");
  await columns(page).getByRole("option", { name: /memo/ }).click();
  await columns(page).getByRole("checkbox", { name: "user_name" }).click();
  await expect.poll(() => heads(page)).toEqual(["id", "amount", "created_at"]);
  // ⎋ closes the list and gives the keys back to the grid, whose status says what is hidden
  await page.keyboard.press("Escape");
  await expect(columns(page)).toHaveCount(0);
  await expect.poll(() => segments(page)).toContain("2 hidden");
  await headerMenu(page, "id", /^Columns…/);
  await columns(page).getByRole("button", { name: "Show All" }).click();
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "amount", "created_at", "memo"]);
  await page.keyboard.press("Escape");
  await expect.poll(() => segments(page)).not.toContain("2 hidden");
});

test("the last shown column can't be hidden", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  for (const n of ["user_name", "amount", "created_at", "memo"]) await headerMenu(page, n, /^Hide Column/);
  await expect.poll(() => heads(page)).toEqual(["id"]);
  await header(page, "id").click({ button: "right" });
  await expect(menuItem(page, /^Hide Column/)).toBeDisabled();
  await page.keyboard.press("Escape");
  // Reset View shows every column again
  await work(page).getByTitle("Show Options Menu").click();
  await menuItem(page, /^Reset View/).click();
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "amount", "created_at", "memo"]);
});

test("pinned columns come first and stay at the left while the grid scrolls right; Unpin puts a column back", async ({ page }) => {
  await openApp(page);
  await runWide(page);
  await headerMenu(page, "column_5", /^Pin Column/);
  await headerMenu(page, "column_3", /^Pin Column/);
  await expect.poll(async () => (await heads(page)).slice(0, 4)).toEqual(["column_5", "column_3", "column_1", "column_2"]);
  const scroll = work(page).locator(".grid-scroll");
  const lefts = () =>
    work(page)
      .locator(".grid-wrap")
      .evaluate((wrap) => {
        const at = (el: Element | null | undefined) => (el ? Math.round(el.getBoundingClientRect().left) : null);
        const row = wrap.querySelector(".grid-scroll [role=row]")!;
        const rownum = row.querySelector(".rownum")!.getBoundingClientRect();
        const head = (n: string) => [...wrap.querySelectorAll(".grid-head [role=columnheader]")].find((h) => h.querySelector(".col-name")!.textContent === n);
        const cells = row.querySelectorAll("[role=gridcell]");
        return { rownumRight: Math.round(rownum.right), head5: at(head("column_5")), head3: at(head("column_3")), cell5: at(cells[0]), cell3: at(cells[1]), text: [cells[0].textContent, cells[1].textContent] };
      });
  const before = await lefts();
  expect(before.head5).toBe(before.rownumRight);
  expect(before.cell5).toBe(before.rownumRight);
  expect(before.text).toEqual(["4", "2"]);
  await scroll.evaluate((el) => el.scrollTo(1e6, 0));
  await expect(header(page, "column_30")).toBeVisible();
  await expect.poll(lefts).toEqual(before);
  // The scrolled columns pass under the pinned ones: the last one drawn is past them
  await expect(header(page, "column_1")).toHaveCount(0);

  await scroll.evaluate((el) => el.scrollTo(0, 0));
  await headerMenu(page, "column_5", /^Unpin Column/);
  await expect.poll(async () => (await heads(page)).slice(0, 4)).toEqual(["column_3", "column_1", "column_2", "column_4"]);
});

test("a table tab's layout is kept when the tab opens again and after a restart", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await headerMenu(page, "memo", /^Hide Column/);
  await headerMenu(page, "amount", /^Pin Column/);
  const layout = ["amount", "id", "user_name", "created_at"];
  await expect.poll(() => heads(page)).toEqual(layout);
  await work(page).locator(".grid-scroll").focus();
  await page.keyboard.press("Meta+w");
  await openUsers(page);
  await expect.poll(() => heads(page)).toEqual(layout);
  await page.reload();
  await openApp(page);
  await openUsers(page);
  await expect.poll(() => heads(page)).toEqual(layout);
  // The status bar speaks for the grid with the focus
  await cell(page, "user_1").click();
  await expect.poll(() => segments(page)).toContain("1 hidden");
});

test("a console result's layout is in memory only: gone after the console closes", async ({ page }) => {
  await openApp(page);
  await runWide(page);
  await headerMenu(page, "column_2", /^Hide Column/);
  await expect(header(page, "column_2")).toHaveCount(0);
  await work(page).locator(".grid-scroll").focus();
  await page.keyboard.press("Meta+w");
  await runWide(page);
  await expect(header(page, "column_2")).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("mock-columnLayout"))).toBeNull();
});

test("Copy of a selection skips hidden columns and follows the pinned order; Export Data… of the result keeps every column", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await headerMenu(page, "user_name", /^Hide Column/);
  await headerMenu(page, "amount", /^Pin Column/);
  // Row 2, whole: a click on its number
  await work(page).locator(".grid-scroll [role=row]").nth(1).locator(".rownum").click();
  await page.keyboard.press("Meta+c");
  await expect.poll(() => clipboard(page)).toBe("1234.50,2,2026-10-15 12:00:00.5 +09:00,");

  // Export runs the tab's query, every column of it
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: any) => (cmd === "export_data" && (w.__exportSql = args.sql), orig(cmd, args));
  });
  await work(page).getByTitle(/^Export Data…/).click();
  await dialog(page).getByRole("radio", { name: /^Clipboard/ }).check();
  await dialog(page).getByRole("button", { name: "Export" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__exportSql as string | undefined)).toMatch(/^select \* from/i);
});

test("the selection stays on its column after Pin, Unpin and Hide", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  const selHeads = () => work(page).locator(".grid-head [role=columnheader].col-sel .col-name").allTextContents();
  // The right-click selects memo; pinned, it is still the selected one at the left, and after Unpin back in its place
  await headerMenu(page, "memo", /^Pin Column/);
  await expect.poll(() => heads(page)).toEqual(["memo", "id", "user_name", "amount", "created_at"]);
  await expect.poll(selHeads).toEqual(["memo"]);
  await headerMenu(page, "memo", /^Unpin Column/);
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "amount", "created_at", "memo"]);
  await expect.poll(selHeads).toEqual(["memo"]);
  // Its own column hidden: nothing stays selected (no edit or copy lands on the neighbour)
  await headerMenu(page, "amount", /^Hide Column/);
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "created_at", "memo"]);
  await expect.poll(selHeads).toEqual([]);
  await expect(work(page).locator('[role=gridcell][aria-selected="true"]')).toHaveCount(0);
});

test("the status bar's shown columns skip those under the pinned ones once the grid scrolls", async ({ page }) => {
  await openApp(page);
  await runWide(page);
  for (const n of ["column_1", "column_2", "column_3"]) await headerMenu(page, n, /^Pin Column/);
  const cols = page.locator(".statusbar .ui-status-item").filter({ hasText: /^cols / });
  await work(page).locator(".grid-wrap .cell:not(.rownum)").first().click();
  await expect(cols).toHaveText(/^cols 1–\d+ of 30$/);
  // Scrolled by column_4..7: the first one at the pinned edge is column_8, those four are under the pinned ones
  let by = 0;
  for (const n of ["column_4", "column_5", "column_6", "column_7"]) by += await header(page, n).evaluate((h) => h.getBoundingClientRect().width);
  await work(page).locator(".grid-scroll").evaluate((el, x) => (el.scrollLeft = x), by);
  await expect(cols).toHaveText(/^cols 8–\d+ of 30$/);
});

// Several selected columns (#216, DataGrip): the header menu's Hide, Pin and Unpin act on all of them when the right-click lands on one
const selHeads = (page: Page) => work(page).locator(".grid-head [role=columnheader].col-sel .col-name").allTextContents();
/** A ⇧-click, with no pause: quick ⇧-clicks never count as ⇧⇧ (#217) */
async function shiftClick(page: Page, target: ReturnType<Page["locator"]>) {
  await target.click({ modifiers: ["Shift"] });
}
async function selectHeads(page: Page, from: string, to: string) {
  await header(page, from).locator(".col-name").click();
  await shiftClick(page, header(page, to).locator(".col-name"));
}
const ALL = ["id", "user_name", "amount", "created_at", "memo"];

test("Pin 3 Columns pins the selected ones in their order and the table keeps them after a restart; Unpin 3 Columns; Hide 2 Columns", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await selectHeads(page, "user_name", "created_at");
  await header(page, "amount").click({ button: "right" });
  // The right-click keeps the selection it lands in
  await expect.poll(() => selHeads(page)).toEqual(["user_name", "amount", "created_at"]);
  await expect(menuItem(page, /^Hide 3 Columns/)).toBeEnabled();
  await expect(menuItem(page, /^Unpin/)).toHaveCount(0);
  await menuItem(page, /^Pin 3 Columns/).click();
  const pinned = ["user_name", "amount", "created_at", "id", "memo"];
  await expect.poll(() => heads(page)).toEqual(pinned);
  await expect.poll(() => selHeads(page)).toEqual(["user_name", "amount", "created_at"]);
  expect(await work(page).locator(".grid-head [role=columnheader].pin-edge .col-name").allTextContents()).toEqual(["created_at"]);

  await page.reload();
  await openApp(page);
  await openUsers(page);
  await expect.poll(() => heads(page)).toEqual(pinned);

  await selectHeads(page, "user_name", "created_at");
  await header(page, "user_name").click({ button: "right" });
  await expect(menuItem(page, /^Pin/)).toHaveCount(0);
  await menuItem(page, /^Unpin 3 Columns/).click();
  await expect.poll(() => heads(page)).toEqual(ALL);

  // ⌘-click adds a column
  await header(page, "id").locator(".col-name").click();
  await header(page, "user_name").locator(".col-name").click({ modifiers: ["Meta"] });
  await header(page, "id").click({ button: "right" });
  await menuItem(page, /^Hide 2 Columns/).click();
  await expect.poll(() => heads(page)).toEqual(["amount", "created_at", "memo"]);
  await expect.poll(() => segments(page)).toContain("2 hidden");
});

test("a mixed selection gets Pin for the rest and Unpin for the pinned ones; every column selected can't be hidden", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await headerMenu(page, "amount", /^Pin Column/);
  await expect.poll(() => heads(page)).toEqual(["amount", "id", "user_name", "created_at", "memo"]);
  await selectHeads(page, "amount", "user_name");
  await header(page, "id").click({ button: "right" });
  await expect(menuItem(page, /^Pin 2 Columns/)).toBeVisible();
  await expect(menuItem(page, /^Unpin Column/)).toBeVisible();
  await menuItem(page, /^Pin 2 Columns/).click();
  // The pinned one stays first, the rest follow in their display order
  await expect.poll(() => heads(page)).toEqual(["amount", "id", "user_name", "created_at", "memo"]);
  await header(page, "id").click({ button: "right" });
  await expect(menuItem(page, /^Pin/)).toHaveCount(0);
  await menuItem(page, /^Unpin 3 Columns/).click();
  await expect.poll(() => heads(page)).toEqual(ALL);

  // Every column selected on the headers: Hide is greyed out and says why
  await selectHeads(page, "id", "memo");
  await header(page, "memo").click({ button: "right" });
  await expect(menuItem(page, /^Hide 5 Columns/)).toBeDisabled();
  await expect(menuItem(page, /^Hide 5 Columns/)).toHaveAttribute("title", "At least one column stays shown");
});

test("a right-click outside the selection acts on that column alone", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await selectHeads(page, "id", "user_name");
  await header(page, "memo").click({ button: "right" });
  await expect.poll(() => selHeads(page)).toEqual(["memo"]);
  await menuItem(page, /^Pin Column/).click();
  await expect.poll(() => heads(page)).toEqual(["memo", "id", "user_name", "amount", "created_at"]);
});

test("the Pin Column and Hide Column keys act on the selected columns", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() =>
    import(/* @vite-ignore */ "/src/lib/shortcuts.ts").then((m) => {
      m.bind("grid.colPin", "Ctrl+Alt+P", false);
      m.bind("grid.colHide", "Ctrl+Alt+H", false);
    }),
  );
  await openUsers(page);
  await selectHeads(page, "amount", "memo");
  await page.keyboard.press("Control+Alt+KeyP");
  await expect.poll(() => heads(page)).toEqual(["amount", "created_at", "memo", "id", "user_name"]);
  // Again: all pinned now, so the key unpins them
  await page.keyboard.press("Control+Alt+KeyP");
  await expect.poll(() => heads(page)).toEqual(ALL);
  await page.keyboard.press("Control+Alt+KeyH");
  await expect.poll(() => heads(page)).toEqual(["id", "user_name"]);
});

/** Row r's display column c (0-based) */
const at = (page: Page, r: number, c: number) => work(page).locator(".grid-scroll [role=row]").nth(r).locator("[role=gridcell]").nth(c);

test("a row, a cell range or ⌘A is no column selection: the header menu and the keys act on one column", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() =>
    import(/* @vite-ignore */ "/src/lib/shortcuts.ts").then((m) => {
      m.bind("grid.colPin", "Ctrl+Alt+P", false);
      m.bind("grid.colHide", "Ctrl+Alt+H", false);
    }),
  );
  await openUsers(page);
  // A whole row: Hide Column is there and hides the right-clicked column alone
  await work(page).locator(".grid-scroll [role=row]").nth(1).locator(".rownum").click();
  await header(page, "amount").click({ button: "right" });
  await expect(menuItem(page, /^Hide Column/)).toBeEnabled();
  await expect(menuItem(page, /^Hide \d/)).toHaveCount(0);
  await menuItem(page, /^Hide Column/).click();
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "created_at", "memo"]);

  // A cell range over three columns: Pin Column pins the right-clicked one
  await at(page, 0, 0).click();
  await shiftClick(page, at(page, 1, 2));
  await header(page, "user_name").click({ button: "right" });
  await expect(menuItem(page, /^Pin \d/)).toHaveCount(0);
  await menuItem(page, /^Pin Column/).click();
  await expect.poll(() => heads(page)).toEqual(["user_name", "id", "created_at", "memo"]);

  // The keys on a cell range act on the cursor's column (the last clicked cell's)
  await at(page, 0, 1).click();
  await shiftClick(page, at(page, 1, 0));
  await page.keyboard.press("Control+Alt+KeyP");
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "created_at", "memo"]);
  // ⌘A (the cursor on the last cell): the key hides memo alone
  await page.keyboard.press("Meta+a");
  await page.keyboard.press("Control+Alt+KeyH");
  await expect.poll(() => heads(page)).toEqual(["id", "user_name", "created_at"]);
});
