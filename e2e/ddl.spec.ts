import { expand, expect, openApp, openConsole, tabStrips, test, treeItem, work } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

const openDdl = async (page: import("@playwright/test").Page, name: string, nth = 0) => {
  await treeItem(page, name).nth(nth).click();
  await page.keyboard.press("Meta+B");
  await expect.poll(async () => (await tabStrips(page)).join(" | ")).toContain(`${name} [DDL]`);
};
const views = (page: import("@playwright/test").Page) => work(page).getByRole("tablist", { name: "DDL view" });
const section = (page: import("@playwright/test").Page, name: string) => work(page).getByRole("region", { name });
const loads = (page: import("@playwright/test").Page) => page.evaluate(() => (window as any).__mockStructureCalls ?? 0);
const ddlLoads = (page: import("@playwright/test").Page) => page.evaluate(() => (window as any).__mockDdlCalls ?? 0);
const reload = (page: import("@playwright/test").Page) => work(page).getByRole("button", { name: /^Reload/ });
/** x of each toolbar control (buttons and the view switch), left to right */
const toolbarXs = (page: import("@playwright/test").Page) =>
  work(page).getByRole("toolbar").first().locator("button, [role=tab]").evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().x)));

test("a table's DDL opens as SQL; Structure shows every section, and the choice is remembered", async ({ page }) => {
  await openDdl(page, "users");
  await expect(views(page).getByRole("tab", { name: "SQL" })).toHaveAttribute("aria-selected", "true");
  await expect(work(page).locator(".monaco-editor")).toBeVisible();
  const before = await work(page).boundingBox();

  await views(page).getByRole("tab", { name: "Structure" }).click();
  await expect(work(page).locator(".monaco-editor")).toBeHidden();
  for (const s of ["Columns 5", "Keys and constraints 4", "Indexes 3", "Foreign keys 1", "Referenced by 2", "Triggers 1", "Options and comment"])
    await expect(section(page, s)).toBeVisible();
  // Columns: name · type · nullable · default · comment, the primary key marked
  const id = section(page, "Columns 5").getByRole("row", { name: /^id\b/ });
  await expect(id).toContainText("nextval('users_id_seq'::regclass)");
  await expect(id.getByTitle("Primary key")).toBeVisible();
  await expect(section(page, "Columns 5").getByRole("row", { name: /user_name/ })).toContainText("이름");
  // Index columns, both foreign key directions, triggers, options
  await expect(section(page, "Indexes 3").getByRole("row", { name: /users_name_created/ })).toContainText("user_name, created_at");
  await expect(section(page, "Foreign keys 1")).toContainText("public.contracts (id)");
  await expect(section(page, "Foreign keys 1")).toContainText("SET NULL");
  await expect(section(page, "Referenced by 2").getByRole("row", { name: /payments_user_fk/ })).toContainText("public.payments (user_id)");
  await expect(section(page, "Triggers 1")).toContainText("BEFORE");
  await expect(section(page, "Options and comment")).toContainText("Members of the shop");
  // Same box as the SQL view: switching doesn't move the layout
  expect(await work(page).boundingBox()).toEqual(before);

  // Another table opens in the last choice
  await openDdl(page, "contracts");
  await expect(views(page).getByRole("tab", { name: "Structure" })).toHaveAttribute("aria-selected", "true");
  await expect(section(page, "Columns 5")).toBeVisible();
  // …also after a relaunch
  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await openDdl(page, "payments");
  await expect(views(page).getByRole("tab", { name: "Structure" })).toHaveAttribute("aria-selected", "true");
  await views(page).getByRole("tab", { name: "SQL" }).click();
  await expect(work(page).locator(".monaco-editor")).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("qurry.ddlView"))).toBe("sql");
});

test("a source that reports only the basics says which sections aren't available", async ({ page }) => {
  await expand(page, "billing");
  await expand(page, "tables 2");
  await openDdl(page, "invoices");
  await views(page).getByRole("tab", { name: "Structure" }).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  for (const s of ["Foreign keys", "Referenced by", "Triggers", "Options and comment"])
    await expect(section(page, s)).toContainText("Not available for this data source");
  // No index columns reported: no empty Columns column
  await expect(section(page, "Indexes 3").getByRole("columnheader")).toHaveText(["Name", "Kind", "Definition"]);
});

test("Structure loads once, when first shown: not for restored or background tabs, not again on each switch", async ({ page }) => {
  await openDdl(page, "users");
  await views(page).getByRole("tab", { name: "Structure" }).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  await openDdl(page, "contracts");
  await expect(section(page, "Columns 5")).toBeVisible();
  expect(await loads(page)).toBe(2);
  // SQL and back (keyboard: ← → on the switch) keeps what was loaded
  await views(page).getByRole("tab", { name: "Structure" }).focus();
  await page.keyboard.press("ArrowLeft");
  await expect(views(page).getByRole("tab", { name: "SQL" })).toHaveAttribute("aria-selected", "true");
  await expect(views(page).getByRole("tab", { name: "SQL" })).toBeFocused();
  await expect(work(page).locator(".monaco-editor")).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(section(page, "Columns 5")).toBeVisible();
  expect(await loads(page)).toBe(2);
  // Relaunch with a console in front: the DDL tabs come back in Structure but don't connect until shown
  await openConsole(page);
  await page.waitForTimeout(800);
  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await page.waitForTimeout(500);
  expect(await loads(page)).toBe(0);
  await page.getByRole("tab", { name: /contracts \[DDL\]/ }).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  expect(await loads(page)).toBe(1);
});

test("restored DDL tabs keep their own view when another tab switches; Reload loads the structure again", async ({ page }) => {
  await openDdl(page, "users");
  await views(page).getByRole("tab", { name: "Structure" }).click();
  await openDdl(page, "contracts");
  await expect(section(page, "Columns 5")).toBeVisible();
  await page.waitForTimeout(800);
  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  // Both come back in Structure (the last choice); switching one to SQL leaves the other alone
  await page.getByRole("tab", { name: /^users \[DDL\]/ }).click();
  await views(page).getByRole("tab", { name: "SQL" }).click();
  await page.getByRole("tab", { name: /contracts \[DDL\]/ }).click();
  await expect(views(page).getByRole("tab", { name: "Structure" })).toHaveAttribute("aria-selected", "true");
  await expect(section(page, "Columns 5")).toBeVisible();
  // Reload asks the database for the structure again
  const before = await loads(page);
  await reload(page).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  expect(await loads(page)).toBe(before + 1);
});

test("Reload is in both views, in the same place, and reloads what the tab shows", async ({ page }) => {
  await openDdl(page, "users");
  await expect(views(page).getByRole("tab", { name: "SQL" })).toHaveAttribute("aria-selected", "true");
  await expect(reload(page)).toBeVisible();
  // An icon button like the grid's, named by its tooltip
  await expect(reload(page)).toHaveAttribute("title", /^Reload/);
  await expect(reload(page).locator("svg")).toHaveCount(1);
  const xs = await toolbarXs(page);
  // SQL: the DDL text is fetched again (and shown); the structure isn't touched
  const [ddl0, st0] = [await ddlLoads(page), await loads(page)];
  await page.evaluate(() => ((window as any).__mockDdlExtra = "-- reloaded\n"));
  await reload(page).click();
  await expect.poll(() => ddlLoads(page)).toBe(ddl0 + 1);
  await expect(work(page).locator(".monaco-editor")).toContainText("-- reloaded");
  expect(await loads(page)).toBe(st0);
  // Structure: the structure is fetched again, not the DDL; nothing in the toolbar moves
  await views(page).getByRole("tab", { name: "Structure" }).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  expect(await toolbarXs(page)).toEqual(xs);
  const st1 = await loads(page);
  await reload(page).click();
  await expect.poll(() => loads(page)).toBe(st1 + 1);
  await expect(section(page, "Columns 5")).toBeVisible();
  expect(await ddlLoads(page)).toBe(ddl0 + 1);
});

test("DDL tabs saved before they kept their object: a table's reloads in both views, a view's says to reopen it", async ({ page }) => {
  await openDdl(page, "users");
  await treeItem(page, "views 1").first().click();
  await page.keyboard.press("ArrowRight");
  await openDdl(page, "v_active_users");
  await page.waitForTimeout(800);
  // As the previous build saved them: no ddlOf (a table's DDL tab still has edit)
  await page.evaluate(() => {
    const ws = JSON.parse(localStorage.getItem("mock-ws")!);
    for (const w of ws.workspaces) for (const t of w.tabs) delete t.ddlOf;
    localStorage.setItem("mock-ws", JSON.stringify(ws));
  });
  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await page.getByRole("tab", { name: /^users \[DDL\]/ }).click();
  await views(page).getByRole("tab", { name: "SQL" }).click();
  const ddl0 = await ddlLoads(page);
  await reload(page).click();
  await expect.poll(() => ddlLoads(page)).toBe(ddl0 + 1);
  await views(page).getByRole("tab", { name: "Structure" }).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  const st0 = await loads(page);
  await reload(page).click();
  await expect.poll(() => loads(page)).toBe(st0 + 1);
  await page.getByRole("tab", { name: /v_active_users \[DDL\]/ }).click();
  await expect(work(page).getByRole("button", { name: "Reopen the DDL to reload" })).toBeDisabled();
});

test("a view's DDL tab reloads its DDL", async ({ page }) => {
  await treeItem(page, "views 1").first().click();
  await page.keyboard.press("ArrowRight");
  await openDdl(page, "v_active_users");
  const before = await ddlLoads(page);
  await page.evaluate(() => ((window as any).__mockDdlExtra = "-- view reloaded\n"));
  await reload(page).click();
  await expect.poll(() => ddlLoads(page)).toBe(before + 1);
  await expect(work(page).locator(".monaco-editor")).toContainText("-- view reloaded");
});

test("a long default wraps so the comment stays in view", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 800 });
  await openDdl(page, "users");
  await views(page).getByRole("tab", { name: "Structure" }).click();
  const cols = section(page, "Columns 5");
  await expect(cols.getByRole("row", { name: /email/ })).toContainText("current_setting('app.mail_domain')");
  const box = await work(page).locator(".ddl-structure").evaluate((e) => ({ scroll: e.scrollWidth, client: e.clientWidth }));
  expect(box.scroll).toBeLessThanOrEqual(box.client);
  await expect(cols.getByRole("row", { name: /email/ }).getByText("Login and receipts")).toBeInViewport();
});

test("a failed load says how to get back to the SQL", async ({ page }) => {
  await page.evaluate(() => ((window as any).__mockStructureFail = "connection reset by peer"));
  await openDdl(page, "users");
  await views(page).getByRole("tab", { name: "Structure" }).click();
  const alert = work(page).getByRole("alert");
  await expect(alert).toContainText("Couldn't load the structure of public.users: connection reset by peer");
  await page.evaluate(() => delete (window as any).__mockStructureFail);
  await alert.getByRole("button", { name: "Retry" }).click();
  await expect(section(page, "Columns 5")).toBeVisible();
  // After a relaunch the table opens in Structure (the last choice) and fails: Show SQL gets back
  await page.reload();
  await expect(treeItem(page, "users").first()).toBeVisible();
  await page.evaluate(() => ((window as any).__mockStructureFail = "gone"));
  await openDdl(page, "contracts");
  await work(page).getByRole("alert").getByRole("button", { name: "Show SQL" }).click();
  await expect(work(page).locator(".monaco-editor")).toBeVisible();
  await expect(views(page).getByRole("tab", { name: "SQL" })).toHaveAttribute("aria-selected", "true");
});

test("views have no Structure view", async ({ page }) => {
  await treeItem(page, "views 1").first().click();
  await page.keyboard.press("ArrowRight");
  await openDdl(page, "v_active_users");
  await expect(views(page)).toHaveCount(0);
  await expect(work(page).locator(".monaco-editor")).toBeVisible();
});
