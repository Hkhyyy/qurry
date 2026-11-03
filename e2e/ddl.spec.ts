import { expand, expect, openApp, tabStrips, test, treeItem, work } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

const openDdl = async (page: import("@playwright/test").Page, name: string, nth = 0) => {
  await treeItem(page, name).nth(nth).click();
  await page.keyboard.press("Meta+B");
  await expect.poll(async () => (await tabStrips(page)).join(" | ")).toContain(`${name} [DDL]`);
};
const views = (page: import("@playwright/test").Page) => work(page).getByRole("tablist", { name: "DDL view" });
const section = (page: import("@playwright/test").Page, name: string) => work(page).getByRole("region", { name });

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
