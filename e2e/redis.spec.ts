import { expect, openApp, test, treeItem, work } from "./app";

test("key browser groups keys into : folders", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "db0").click();
  await page.keyboard.press("Meta+ArrowDown");
  const keys = work(page).getByRole("treeitem");
  await expect(keys).toHaveText(["queue1", "session1", "user2", "STREAMevents", "ZSETrank1d 0h", "SETtags"]);
  await expect(work(page)).toContainText("7 keys");

  await work(page).getByRole("treeitem", { name: "user 2" }).click();
  await expect(work(page).getByRole("treeitem", { name: "user 2" })).toHaveAttribute("aria-expanded", "true");
  await expect(keys).toHaveText(["queue1", "session1", "user2", "11", "HASHuser:1", "STREAMevents", "ZSETrank1d 0h", "SETtags"]);

  await work(page).getByRole("treeitem", { name: "HASH user:1" }).click();
  await expect(work(page)).toContainText("a@b.c");
});
