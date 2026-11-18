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
