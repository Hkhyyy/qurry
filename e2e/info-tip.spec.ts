import { dialog, expect, openApp, test, treeNode } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

// Long explanations sit behind an ⓘ right after the name they explain (Named · InfoTip)
test("ⓘ in Settings: click opens it, Esc closes only the popover and focus goes back to the ⓘ", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const d = dialog(page);
  const about = d.getByRole("button", { name: "About Keyword case" });
  await expect(about).toHaveAttribute("aria-expanded", "false");
  await about.click();
  const note = page.getByRole("note", { name: "About Keyword case" });
  await expect(note).toContainText("Never inside strings, comments or quoted names.");
  await expect(about).toHaveAttribute("aria-expanded", "true");
  // Kept inside the window
  const box = (await note.boundingBox())!;
  const view = page.viewportSize()!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(view.width);
  expect(box.y + box.height).toBeLessThanOrEqual(view.height);

  await page.keyboard.press("Escape");
  await expect(note).toHaveCount(0);
  await expect(d).toBeVisible();
  await expect(about).toBeFocused();
  // The next Esc closes Settings as usual
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("ⓘ from the keyboard: Space and Enter toggle it; a click outside closes it", async ({ page }) => {
  await page.keyboard.press("Meta+Comma");
  const d = dialog(page);
  const about = d.getByRole("button", { name: "About Channel" });
  await about.focus();
  await page.keyboard.press("Space");
  await expect(page.getByRole("note", { name: "About Channel" })).toContainText("Beta: also the pre-release built from every merge to main.");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("note")).toHaveCount(0);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("note")).toHaveCount(1);
  await d.locator(".ui-modal-title").click();
  await expect(page.getByRole("note")).toHaveCount(0);
  await expect(d).toBeVisible();
});

test("ⓘ beside a checkbox doesn't toggle it; Allow unencrypted warns while ticked", async ({ page }) => {
  await treeNode(page, "[DEV] shop_app").click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Properties/ }).click();
  const d = dialog(page);
  await d.getByRole("tab", { name: "Options" }).click();
  const prod = d.getByRole("checkbox", { name: "Production: show affected rows and confirm changes" });
  const was = await prod.isChecked();
  await d.getByRole("button", { name: "About Production: confirm changes" }).click();
  await expect(page.getByRole("note", { name: "About Production: confirm changes" })).toContainText("shows how many rows will change and asks");
  expect(await prod.isChecked()).toBe(was);
  await page.keyboard.press("Escape");
  await expect(d).toBeVisible();

  const plain = d.getByRole("checkbox", { name: "Allow unencrypted connection (servers without TLS)" });
  const warning = d.getByText("Passwords and data can travel unencrypted to this server.");
  await plain.uncheck();
  await expect(warning).toHaveCount(0);
  await d.getByRole("button", { name: "About Allow unencrypted connection" }).click();
  await expect(plain).not.toBeChecked();
  await plain.check();
  await expect(warning).toBeVisible();
  await expect(d.getByText("Applied on OK: the data source reconnects.")).toBeVisible();
});
