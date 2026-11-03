import type { Page } from "@playwright/test";
import { expect, openApp, openConsole, test, treeItem, typeSql, work } from "./app";

// The column header sits above the grid's scroller, not sticky inside it (issue #82): its bottom edge is the scroller's
// top whatever the scroll position, and it follows the body's scrollLeft

/** Header strip and body of the visible grid: header bottom vs body top, header scrollLeft vs body scrollLeft */
const geometry = (page: Page) =>
  work(page)
    .locator(".grid-wrap")
    .evaluate((wrap) => {
      const head = wrap.querySelector<HTMLElement>(".grid-head")!;
      const body = wrap.querySelector<HTMLElement>(".grid-scroll")!;
      const firstHead = head.querySelectorAll<HTMLElement>("[role=columnheader]")[0];
      const firstCell = body.querySelector<HTMLElement>("[role=row] [role=gridcell]");
      return {
        gap: body.getBoundingClientRect().top - head.getBoundingClientRect().bottom,
        headInside: body.contains(head),
        headLeft: head.scrollLeft,
        bodyLeft: body.scrollLeft,
        bodyTop: body.scrollTop,
        // The header cell of the first rendered column sits right above its cells
        colShift: firstHead && firstCell ? firstHead.getBoundingClientRect().left - firstCell.getBoundingClientRect().left : 0,
      };
    });

/** Scroll events (and the re-render of the visible columns) land a frame after the scroll: wait for the header to settle */
async function expectAligned(page: Page) {
  await expect
    .poll(async () => {
      const g = await geometry(page);
      return { inside: g.headInside, gap: g.gap, left: g.headLeft - g.bodyLeft, colShift: g.colShift };
    })
    .toEqual({ inside: false, gap: 0, left: 0, colShift: 0 });
  return geometry(page);
}

const scrollBody = (page: Page, top: number, left: number) =>
  work(page)
    .locator(".grid-scroll")
    .evaluate((el, [t, l]) => el.scrollTo(l, t), [top, left]);

test("console result with many columns: header stays on the scroller's top edge and follows it sideways", async ({ page }) => {
  await openApp(page);
  await openConsole(page);
  await typeSql(page, "select * from wide_demo");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  await expect(work(page).getByRole("columnheader", { name: /^column_1 / })).toBeVisible();
  await expectAligned(page);

  // Vertical, horizontal, both
  await scrollBody(page, 3000, 0);
  await expect.poll(async () => (await geometry(page)).bodyTop).toBeGreaterThan(0);
  await expectAligned(page);
  await scrollBody(page, 3000, 1500);
  await expect.poll(async () => (await geometry(page)).bodyLeft).toBeGreaterThan(0);
  await expectAligned(page);
  // Far right: the header can scroll as far as the body (room for the body's scrollbar)
  await scrollBody(page, 3000, 1e6);
  const end = await expectAligned(page);
  expect(end.bodyLeft).toBeGreaterThan(1500);

  // Wheel over the rows, then over the header (the header forwards it to the body)
  await scrollBody(page, 0, 0);
  const box = (await work(page).locator(".grid-scroll").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(300, 2000);
  await expect.poll(async () => (await geometry(page)).bodyTop).toBeGreaterThan(0);
  await expectAligned(page);
  const before = await geometry(page);
  const head = (await work(page).locator(".grid-head").boundingBox())!;
  await page.mouse.move(head.x + head.width / 2, head.y + head.height / 2);
  await page.mouse.wheel(200, 400);
  await expect.poll(async () => (await geometry(page)).bodyTop).toBeGreaterThan(before.bodyTop);
  await expectAligned(page);
});

test("header click selects the column and keeps the keyboard on the grid", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  const header = work(page).getByRole("columnheader", { name: /^user_name/ });
  await header.click();
  await expect(work(page).locator(".grid-scroll")).toBeFocused();
  await expect(header).toHaveClass(/col-sel/);

  // The cursor is on the column's last row: ↓ scrolls it into view, fully below the header (table data editor, same layout)
  await page.keyboard.press("ArrowDown");
  await expect.poll(async () => (await geometry(page)).bodyTop).toBeGreaterThan(0);
  await expectAligned(page);
  const cell = (await work(page).locator(".cell.active").boundingBox())!;
  const head = (await work(page).locator(".grid-head").boundingBox())!;
  const body = (await work(page).locator(".grid-scroll").boundingBox())!;
  expect(cell.y).toBeGreaterThanOrEqual(head.y + head.height);
  expect(cell.y + cell.height).toBeLessThanOrEqual(body.y + body.height);
});

// The issue's case: a result panel a few rows high (classic scroll bars), where rows showed in a strip between the toolbar and
// the header. Besides the geometry, what is painted there must not be a row: the scroller clips its rows (contain: paint)
const SHORT = { width: 760, height: 400 };

/** Points across the header strip and its top edge where a row is what's painted (hit testing follows painting and clipping) */
const rowsOverHeader = (page: Page) =>
  work(page)
    .locator(".grid-head")
    .evaluate((head) => {
      const b = head.getBoundingClientRect();
      const out: string[] = [];
      for (const y of [b.top, b.top + b.height / 2, b.bottom - 1])
        for (let i = 0; i < 8; i++) {
          const x = b.left + 4 + (i * (b.width - 8)) / 7;
          if (document.elementFromPoint(x, y)?.closest(".grid-row")) out.push(`${Math.round(x)},${Math.round(y)}`);
        }
      return out;
    });

/** Toolbar (or the find bar) bottom = header top, and no row painted over the header, at each scroll position */
async function expectNoStrip(page: Page) {
  const toolbarToHead = () =>
    work(page)
      .locator(".grid-head")
      .evaluate((head) => head.getBoundingClientRect().top - head.previousElementSibling!.getBoundingClientRect().bottom);
  for (const [top, left] of [[0, 0], [2000, 0], [2000, 400], [0, 1e6], [37, 123]]) {
    await scrollBody(page, top, left);
    await expectAligned(page);
    expect(await toolbarToHead()).toBe(0);
    expect(await rowsOverHeader(page)).toEqual([]);
  }
  // The panel really is short and narrow: the body scrolled both ways
  const g = await geometry(page);
  expect([g.bodyTop, g.bodyLeft]).toEqual([37, 123]);
}

test("short panel: no row shows between the toolbar and the header after scrolling (table data editor and console result)", async ({ page }) => {
  await page.setViewportSize(SHORT);
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await expectNoStrip(page);

  await openConsole(page);
  await typeSql(page, "select * from wide_demo");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Enter");
  await expect(work(page).getByRole("columnheader", { name: /^column_1 / })).toBeVisible();
  await expectNoStrip(page);
});

test("transposed view: the column names stay on the left edge, the cells inside the scroller", async ({ page }) => {
  await page.setViewportSize(SHORT);
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await work(page).locator('[title="View as"]').click();
  await page.getByRole("menuitem", { name: /Transpose$/ }).click();
  const scroll = work(page).locator(".grid-scroll.transposed");
  await scroll.evaluate((el) => el.scrollTo(300, 0));
  await expect.poll(() => scroll.evaluate((el) => el.scrollLeft)).toBe(300);
  const edge = await scroll.evaluate((el) => el.querySelectorAll("th")[2].getBoundingClientRect().left - el.getBoundingClientRect().left);
  expect(edge).toBe(0);
  expect(await scroll.evaluate((el) => getComputedStyle(el).contain)).toContain("paint");
});
