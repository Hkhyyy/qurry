import type { Page } from "@playwright/test";
import { dialog, editor, expect, openApp, openSettings, test, treeItem, treeNode, work } from "./app";

// #196: in a small window every dialog keeps its title and its footer's buttons inside the window, and only its body scrolls (or,
// where the dialog lays out its own lists, that list), also after it was moved and its content grew

const VIEW = { width: 800, height: 500 };
test.use({ viewport: VIEW });
test.beforeEach(async ({ page }) => openApp(page));

/** The open dialog's title and footer buttons are inside the window */
async function fits(page: Page) {
  const d = dialog(page).last();
  await expect(d).toBeVisible();
  const head = (await d.locator(".ui-modal-head").boundingBox())!;
  expect(head.y, "the title is inside the window").toBeGreaterThanOrEqual(0);
  const buttons = d.locator(".ui-modal-foot button");
  expect(await buttons.count()).toBeGreaterThan(0);
  for (const b of await buttons.all()) {
    const r = (await b.boundingBox())!;
    expect(r.y >= 0 && r.y + r.height <= VIEW.height && r.x >= 0 && r.x + r.width <= VIEW.width, `${await b.textContent()} is inside the window`).toBe(true);
  }
}
/** `sel` (in the open dialog) is what scrolls: its content is taller than it */
async function scrolls(page: Page, sel = ".ui-modal-body") {
  const [client, scroll] = await dialog(page).last().locator(sel).evaluate((el) => [el.clientHeight, el.scrollHeight]);
  expect(scroll, `${sel} scrolls`).toBeGreaterThan(client);
}
async function fromMenu(page: Page, node: string, item: string | RegExp, sub?: string) {
  await treeNode(page, node).click({ button: "right" });
  if (sub) await page.getByRole("menuitem", { name: sub }).hover();
  await page.getByRole("menuitem", { name: item }).first().click();
}
const props = (page: Page) => fromMenu(page, "[PROD] shop_app", /^Properties/);
async function console_(page: Page) {
  await treeNode(page, "[DEV] shop_app").click();
  await page.keyboard.press("Meta+Shift+L");
  await editor(page).click();
}
/** Drag the dialog's title bar down past the window's bottom edge (it stops there) */
async function dragToBottom(page: Page) {
  const head = (await dialog(page).locator(".ui-modal-head").boundingBox())!;
  const [x, y] = [head.x + head.width - 40, head.y + head.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y + 400, { steps: 6 });
  await page.mouse.up();
}

test("Data Source Properties: every tab, and a long Test Connection error, keep the buttons in the window; the body scrolls", async ({ page }) => {
  await props(page);
  for (const tab of ["General", /SSH\/SSL/, "Schemas", "Options"]) {
    await dialog(page).getByRole("tab", { name: tab }).click();
    await fits(page);
  }
  await scrolls(page);
  await dialog(page).getByRole("tab", { name: /SSH\/SSL/ }).click();
  await dialog(page).getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  await dialog(page).getByRole("textbox", { name: "Instance id" }).fill("i-0000000000000dead");
  await dialog(page).getByRole("button", { name: "Test Connection" }).click();
  await expect(dialog(page).locator(".ui-notice.tone-danger")).toContainText("then try again.");
  await fits(page);
  await scrolls(page);
});

test("a dialog moved to the bottom edge stays inside when its content grows", async ({ page }) => {
  // Data Source Properties (fixed size) dragged down, then a long Test Connection error
  await props(page);
  await dialog(page).getByRole("tab", { name: /SSH\/SSL/ }).click();
  await dragToBottom(page);
  await dialog(page).getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  await dialog(page).getByRole("textbox", { name: "Instance id" }).fill("i-0000000000000dead");
  await dialog(page).getByRole("button", { name: "Test Connection" }).click();
  await expect(dialog(page).locator(".ui-notice.tone-danger")).toBeVisible();
  await fits(page);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);

  // Full-text Search (sized by its content) dragged to the bottom, then 300px more content: it moves up to keep its buttons in view
  await treeNode(page, "[DEV] shop_app").click();
  await page.keyboard.press("Meta+Alt+Shift+KeyF");
  await dragToBottom(page);
  const before = (await dialog(page).boundingBox())!;
  expect(Math.round(before.y + before.height)).toBe(VIEW.height);
  await dialog(page).locator(".ui-modal-body").evaluate((el) => el.append(Object.assign(document.createElement("div"), { style: "height: 300px" })));
  await expect.poll(async () => Math.round((await dialog(page).boundingBox())!.y)).toBeLessThan(Math.round(before.y));
  await fits(page);
});

test("Settings on its long pages and Keymap Reference keep their buttons in the window; their lists scroll", async ({ page }) => {
  await openSettings(page, "Data grid");
  await fits(page);
  await scrolls(page, ".settings-form");
  await openSettings(page, "Keymap");
  await fits(page);
  await scrolls(page, ".keymap-editor-list");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+Shift+Slash");
  await fits(page);
  // Every section is reachable by scrolling down, none goes to a column out of view at the right
  const body = dialog(page).locator(".keymap-body");
  await scrolls(page, ".keymap-body");
  const [scrollW, clientW] = await body.evaluate((el) => [el.scrollWidth, el.clientWidth]);
  expect(scrollW).toBeLessThanOrEqual(clientW);
});

test("Query History with many queries, Parameters with 10 names, Export, Generate Test Data, Import and confirms keep their buttons in the window", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockHistoryRows = 60));
  await console_(page);
  await page.keyboard.press("Meta+Alt+KeyE");
  await fits(page);
  await scrolls(page, ".hist-list");
  await page.keyboard.press("Escape");

  await editor(page).click();
  await page.keyboard.type("select " + Array.from({ length: 10 }, (_, i) => `:name_${i + 1}`).join(", "));
  await page.keyboard.press("Meta+Enter");
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Parameters");
  await fits(page);
  await scrolls(page);
  await page.keyboard.press("Escape");

  await treeItem(page, "users").first().dblclick();
  await work(page).getByRole("button", { name: "Export Data…" }).click();
  await fits(page);
  await page.keyboard.press("Escape");

  await fromMenu(page, "users", "Generate Test Data…", "Import/Export");
  await fits(page);
  await page.keyboard.press("Escape");
  await fromMenu(page, "users", "Import Data from File…", "Import/Export");
  await fits(page);
  await page.keyboard.press("Escape");
  await fromMenu(page, "users", "Drop…", "Object Actions");
  await fits(page);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});
