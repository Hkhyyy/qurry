import type { Locator, Page } from "@playwright/test";
import { expect, menuItem, openApp, openConsole, test, treeItem, typeSql, work } from "./app";

// Resizing result grid columns (#109): drag a header's right edge, double-click it to fit, the header menu, ⌃⌥← / ⌃⌥→. A table
// tab's widths are kept per table in the settings (the mock keeps them across a reload, as the file is); widths are whole characters
// (7.3 px each at the default grid font, plus 20 px of padding: lib/columnWidths.ts)

const px = (chars: number) => Math.round(chars * 7.3 + 20);
const chars = (w: number) => Math.round((w - 20) / 7.3);

const header = (page: Page, name: string) => work(page).getByRole("columnheader", { name: new RegExp(`^(🔑 ?)?${name} `) });
const width = (l: Locator) => l.evaluate((el) => el.getBoundingClientRect().width);
/** Text cut with … somewhere in the element (its own box or a child's) */
const cut = (l: Locator) => l.evaluate((el) => [el, ...el.querySelectorAll<HTMLElement>("*")].some((e) => e.scrollWidth > e.clientWidth + 1));

async function drag(page: Page, name: string, dx: number) {
  const b = (await header(page, name).locator(".col-resize").boundingBox())!;
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2 + dx / 2, b.y + b.height / 2, { steps: 4 });
  await page.mouse.move(b.x + b.width / 2 + dx, b.y + b.height / 2, { steps: 4 });
  await page.mouse.up();
}

async function openUsers(page: Page) {
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
}

test("table tab: drag resizes, double-click fits (again: back); the width is kept when the tab opens again and after a restart", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  const name = header(page, "user_name");
  const cell = work(page).getByRole("gridcell", { name: "user_1", exact: true });
  const w0 = await width(name);
  expect(await width(cell)).toBe(w0);

  await drag(page, "user_name", 100);
  const w1 = await width(name);
  expect(Math.abs(w1 - (w0 + 100))).toBeLessThanOrEqual(4);
  expect(w1).toBe(px(chars(w1)));
  expect(await width(cell)).toBe(w1);
  // Dragging doesn't select the column or sort it
  await expect(name).not.toHaveClass(/col-sel|sorted/);

  await work(page).locator(".grid-scroll").focus();
  await page.keyboard.press("Meta+w");
  await openUsers(page);
  expect(await width(header(page, "user_name"))).toBe(w1);
  await page.reload();
  await openApp(page);
  await openUsers(page);
  expect(await width(header(page, "user_name"))).toBe(w1);

  // As narrow as it goes (3 characters), then double-click the edge: it fits the header (the widest here) and its values
  await drag(page, "user_name", -1000);
  expect(await width(header(page, "user_name"))).toBe(px(3));
  expect(await cut(header(page, "user_name"))).toBe(true);
  await header(page, "user_name").locator(".col-resize").dblclick();
  const fit = await width(header(page, "user_name"));
  expect(fit).toBe(px(20));
  expect(await cut(header(page, "user_name"))).toBe(false);
  expect(await cut(work(page).getByRole("gridcell", { name: "user_1", exact: true }))).toBe(false);
  // Double-clicked again on the fitted column: back to the width before the fit (TUI: = again)
  await header(page, "user_name").locator(".col-resize").dblclick();
  expect(await width(header(page, "user_name"))).toBe(px(3));
});

test("double-click on a column without a set width, twice: fitted, then exactly its computed width again (not rounded to characters)", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  for (const n of ["id", "user_name", "amount", "memo"]) {
    const before = await width(header(page, n));
    await header(page, n).locator(".col-resize").dblclick();
    await header(page, n).locator(".col-resize").dblclick();
    expect(await width(header(page, n)), n).toBe(before);
  }
});

test("header menu: Fit Column, Fit All Columns, Reset Widths (back to the computed widths, also after a restart)", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  const names = ["id", "user_name", "amount", "created_at", "memo"];
  const computed = await Promise.all(names.map((n) => width(header(page, n))));

  await header(page, "created_at").click({ button: "right" });
  for (const [label, key] of [["Fit Column", "double-click the edge"], ["Fit All Columns", ""], ["Reset Widths", "computed widths"], ["Narrow Column", "⌃⌥←"], ["Widen Column", "⌃⌥→"]])
    await expect(menuItem(page, new RegExp(`^${label}`))).toContainText(key);
  await expect(menuItem(page, /^Reset Widths/)).toBeDisabled();
  await menuItem(page, /^Fit Column/).click();
  // created_at holds 2026-10-15 12:00:00.125 +09:00: the time column's padded values are what it fits
  expect(await cut(work(page).locator(".grid-row").first().locator("[role=gridcell]").nth(3))).toBe(false);
  expect(await width(header(page, "created_at"))).toBe(px("2026-10-15 12:00:00.125 +09:00".length));

  await header(page, "id").click({ button: "right" });
  await menuItem(page, /^Fit All Columns/).click();
  for (const n of names) expect(await cut(header(page, n)), n).toBe(false);
  const row = work(page).locator(".grid-row").first().locator("[role=gridcell]");
  for (let i = 0; i < names.length; i++) expect(await cut(row.nth(i)), names[i]).toBe(false);
  expect(await Promise.all(names.map((n) => width(header(page, n))))).not.toEqual(computed);

  await header(page, "memo").click({ button: "right" });
  await menuItem(page, /^Reset Widths/).click();
  expect(await Promise.all(names.map((n) => width(header(page, n))))).toEqual(computed);
  await page.reload();
  await openApp(page);
  await openUsers(page);
  expect(await Promise.all(names.map((n) => width(header(page, n))))).toEqual(computed);
});

test("⌃⌥← / ⌃⌥→ narrow / widen the current column (a cell's or a clicked header's); Transpose is unaffected", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  const name = header(page, "user_name");
  const c0 = chars(await width(name));

  // Transposed before any change, to compare with
  const transposed = async () => {
    await work(page).locator('[title="View as"]').click();
    await menuItem(page, /Transpose$/).click();
    const ws = await work(page).locator(".grid-scroll.transposed tr").first().locator("th, td").evaluateAll((els) => els.slice(0, 6).map((e) => e.getBoundingClientRect().width));
    await work(page).locator('[title="View as"]').click();
    await menuItem(page, /Table$/).click();
    return ws;
  };
  const before = await transposed();

  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await page.keyboard.press("Control+Alt+ArrowRight");
  expect(await width(name)).toBe(px(c0 + 4));
  await page.keyboard.press("Control+Alt+ArrowLeft");
  await page.keyboard.press("Control+Alt+ArrowLeft");
  expect(await width(name)).toBe(px(c0 - 4));
  // The keys don't move the cursor
  await expect(work(page).locator(".cell.active")).toHaveText("user_1");

  const amount = header(page, "amount");
  const a0 = chars(await width(amount));
  await amount.click();
  await page.keyboard.press("Control+Alt+ArrowRight");
  expect(await width(amount)).toBe(px(a0 + 4));

  // Transpose draws as before and the keys don't change the table's widths there
  expect(await transposed()).toEqual(before);
  await work(page).locator('[title="View as"]').click();
  await menuItem(page, /Transpose$/).click();
  await work(page).locator(".grid-scroll.transposed").focus();
  await page.keyboard.press("Control+Alt+ArrowRight");
  await work(page).locator('[title="View as"]').click();
  await menuItem(page, /Table$/).click();
  expect(await width(header(page, "amount"))).toBe(px(a0 + 4));
});

test("console result: widths kept in memory, horizontal scrolling and the virtualized header follow them; a failed save says how to go back", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select * from wide_demo");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  await expect(header(page, "column_1")).toBeVisible();
  const scroll = work(page).locator(".grid-scroll");
  const full0 = await scroll.evaluate((el) => el.scrollWidth);
  const w0 = await width(header(page, "column_1"));

  await drag(page, "column_1", 300);
  const w1 = await width(header(page, "column_1"));
  expect(Math.abs(w1 - (w0 + 300))).toBeLessThanOrEqual(4);
  await expect.poll(async () => Math.abs((await scroll.evaluate((el) => el.scrollWidth)) - (full0 + (w1 - w0)))).toBeLessThanOrEqual(1);
  // Far right: the last column is drawn (header virtualization) and sits above its cells
  await scroll.evaluate((el) => el.scrollTo(1e6, 0));
  await expect(header(page, "column_30")).toBeVisible();
  await expect
    .poll(() =>
      work(page)
        .locator(".grid-wrap")
        .evaluate((wrap) => {
          const head = wrap.querySelector<HTMLElement>(".grid-head")!;
          const body = wrap.querySelector<HTMLElement>(".grid-scroll")!;
          return body.scrollLeft > 0 && head.scrollLeft === body.scrollLeft;
        }),
    )
    .toBe(true);
  const g = await work(page)
    .locator(".grid-wrap")
    .evaluate((wrap) => {
      const heads = [...wrap.querySelectorAll<HTMLElement>(".grid-head [role=columnheader]")];
      const cells = [...wrap.querySelector(".grid-scroll [role=row]")!.querySelectorAll<HTMLElement>("[role=gridcell]")];
      return [Math.round(heads.at(-1)!.getBoundingClientRect().right), Math.round(cells.at(-1)!.getBoundingClientRect().right)];
    });
  expect(g[0]).toBe(g[1]);
  // The row-number corner sits beside the scrolled header, not in it (sticky in it, it scrolled away in the app's WKWebView)
  const corner = await work(page)
    .locator(".grid-wrap")
    .evaluate((wrap) => {
      const head = wrap.querySelector(".grid-head-wrap > .grid-header .rownum")!.getBoundingClientRect();
      const body = wrap.querySelector(".grid-scroll [role=row] .rownum")!.getBoundingClientRect();
      return [wrap.querySelectorAll(".grid-head .rownum").length, Math.round(head.left), Math.round(head.width), Math.round(body.left), Math.round(body.width)];
    });
  expect(corner).toEqual([0, corner[3], corner[4], corner[3], corner[4]]);
  // Nothing saved for a console result
  expect(await page.evaluate(() => localStorage.getItem("mock-columnWidths"))).toBeNull();
  await scroll.evaluate((el) => el.scrollTo(0, 0));
  expect(await width(header(page, "column_1"))).toBe(w1);

  // A table tab's settings can't be written: the width still applies, and the status bar says so and how to go back
  await openUsers(page);
  await page.evaluate(() => ((window as any).__mockChannelSaveFails = true));
  const saved = await width(header(page, "user_name"));
  await drag(page, "user_name", 50);
  await expect(page.locator(".statusbar")).toContainText("Couldn't save the column widths (Couldn't save the settings: permission denied). They apply until the tab closes; the column header's menu › Reset Widths goes back to the computed widths.");
  const unsaved = await width(header(page, "user_name"));
  expect(unsaved).toBeGreaterThan(saved);
  // Kept while the tab reloads (⌘R), gone with the tab
  await work(page).getByRole("gridcell", { name: "user_1", exact: true }).click();
  await page.keyboard.press("Meta+r");
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  expect(await width(header(page, "user_name"))).toBe(unsaved);
  await page.keyboard.press("Meta+w");
  await openUsers(page);
  expect(await width(header(page, "user_name"))).toBe(saved);
});

test("Fit counts a wide character (Hangul) as 2, as the TUI does; a join's two id columns resize on their own", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select * from cjk_demo");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  const memo = header(page, "memo");
  await expect(memo).toBeVisible();
  const long = work(page).getByRole("gridcell", { name: "배송 전에 꼭 전화 주세요 고객 요청 사항", exact: true });
  expect(await cut(long)).toBe(true);
  await memo.locator(".col-resize").dblclick();
  // 16 Hangul characters and 7 spaces: 39 characters, as the TUI's fit
  expect(await width(memo)).toBe(px(39));
  expect(await cut(long)).toBe(false);

  const ids = header(page, "id");
  await expect(ids).toHaveCount(2);
  const [a0, b0] = [await width(ids.nth(0)), await width(ids.nth(1))];
  const b = (await ids.nth(1).locator(".col-resize").boundingBox())!;
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2 + 80, b.y + b.height / 2, { steps: 6 });
  await page.mouse.up();
  expect(await width(ids.nth(1))).toBeGreaterThan(b0 + 70);
  expect(await width(ids.nth(0))).toBe(a0);
  await ids.nth(0).click({ button: "right" });
  await menuItem(page, /^Widen Column/).click();
  expect(await width(ids.nth(0))).toBe(px(chars(a0) + 4));
  expect(await width(ids.nth(1))).toBeGreaterThan(b0 + 70);
});

test("a table tab reads the saved widths each time it opens: a change the TUI made meanwhile shows", async ({ page }) => {
  await openApp(page);
  await openUsers(page);
  await drag(page, "user_name", 60);
  const w = await width(header(page, "user_name"));
  // The TUI widens memo to 40 characters in the same settings file
  await page.evaluate(() => {
    const all = JSON.parse(localStorage.getItem("mock-columnWidths")!);
    const k = Object.keys(all)[0];
    all[k] = { ...all[k], memo: 40 };
    localStorage.setItem("mock-columnWidths", JSON.stringify(all));
  });
  await work(page).locator(".grid-scroll").focus();
  await page.keyboard.press("Meta+w");
  await openUsers(page);
  expect(await width(header(page, "memo"))).toBe(px(40));
  expect(await width(header(page, "user_name"))).toBe(w);
});
