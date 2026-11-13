import { expect, openApp, openConsole, test, typeSql, work } from "./app";

// The server's notices (PostgreSQL RAISE NOTICE / WARNING) in Output under their statement (#168). The mock answers a DO block
// with two notices and the cap's "… N more" line

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => {
    throw e;
  });
  await openApp(page);
});

test("a statement's notices are written in Output under it, warnings in the warn style, before its result line", async ({ page }) => {
  await openConsole(page);
  await typeSql(page, "DO $$ BEGIN RAISE NOTICE 'x'; END $$");
  await page.keyboard.press("Meta+Enter");
  const lines = work(page).locator(".output-log .ol");
  await expect(lines.last()).toContainText("0 rows affected");
  const texts = (await lines.allTextContents()).map((t) => t.replace(/^\[[^\]]*\]\s*/, ""));
  const at = texts.findIndex((t) => t.includes("> DO $$"));
  expect(texts.slice(at + 1)).toEqual([
    "NOTICE: role app_owner isn't a member, owner change skipped",
    "WARNING: table shop.orders has no primary key",
    "… 3 more",
    expect.stringMatching(/^0 rows affected in \d+ ms$/),
  ]);
  await expect(lines.nth(at + 1)).toHaveClass(/\binfo\b/);
  await expect(lines.nth(at + 2)).toHaveClass(/\bwarn\b/);
  await expect(lines.nth(at + 3)).toHaveClass(/\binfo\b/);
});
