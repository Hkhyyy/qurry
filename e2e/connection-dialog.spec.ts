import { dialog, expect, openApp, test, treeNode } from "./app";

test.beforeEach(async ({ page }) => openApp(page));

async function openProps(page: import("@playwright/test").Page) {
  await treeNode(page, "[PROD] shop_app").click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Properties/ }).click();
  await expect(dialog(page).locator(".ui-modal-title")).toHaveText("Data Source Properties");
  await dialog(page).getByRole("tab", { name: /SSH\/SSL/ }).click();
}

test("SSH/SSL tab: Tunnel None / SSH / AWS SSM, test and save through SSM", async ({ page }) => {
  await openProps(page);
  const d = dialog(page);
  const tunnel = d.getByRole("combobox", { name: "Tunnel" });
  await expect(tunnel.locator("option")).toHaveText(["None", "SSH", "AWS SSM (Session Manager)"]);
  await expect(tunnel).toHaveValue("ssh");
  await expect(d.getByRole("button", { name: "Test SSH Connection" })).toBeVisible();

  await tunnel.selectOption("ssm");
  await expect(d.getByPlaceholder("bastion.example.com")).toHaveCount(0);
  await expect(d.getByRole("textbox", { name: "AWS profile" })).toHaveAttribute("placeholder", "(default profile)");
  await expect(d.getByRole("textbox", { name: "Region" })).toHaveAttribute("placeholder", "(the profile's region)");
  // The SSM note sits behind the footer's ⓘ
  await d.getByRole("button", { name: "About AWS SSM" }).click();
  await expect(page.getByRole("note", { name: "About AWS SSM" })).toContainText("Qurry stores no AWS secrets");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("note")).toHaveCount(0);
  await d.getByRole("textbox", { name: "AWS profile" }).fill("prod");
  await d.getByRole("textbox", { name: "Region" }).fill("ap-northeast-2");
  await d.getByRole("textbox", { name: "Instance id" }).fill("i-0123456789abcdef0");
  // With SSM the tab tests the DB connection through a session (there is no separate SSM login to test)
  await d.getByRole("button", { name: "Test Connection" }).click();
  await expect(d.getByText("✓ PostgreSQL 16.11  (via AWS SSM i-0123456789abcdef0)")).toBeVisible();

  await tunnel.selectOption("none");
  await expect(d.getByText("(via AWS SSM i-0123456789abcdef0)"), "a result for the other tunnel is cleared").toHaveCount(0);
  await expect(d.getByRole("tab", { name: "SSH/SSL", exact: true })).toBeVisible();
  await expect(d.getByPlaceholder("bastion.example.com")).toBeDisabled();
  await tunnel.selectOption("ssm");
  await expect(d.getByRole("textbox", { name: "Instance id" }), "switching keeps the SSM fields").toHaveValue("i-0123456789abcdef0");
  await expect(d.getByRole("tab", { name: "SSH/SSL ●" })).toBeVisible();

  await d.getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  // The explorer row shows the tunnel like an SSH one
  const badge = treeNode(page, "[PROD] shop_app").locator(".ssh-badge");
  await expect(badge).toHaveText("SSM");
  await expect(badge).toHaveAttribute("title", "AWS SSM via i-0123456789abcdef0 (prod/ap-northeast-2)");
  await openProps(page);
  await expect(dialog(page).getByRole("combobox", { name: "Tunnel" })).toHaveValue("ssm");
  await expect(dialog(page).getByRole("textbox", { name: "AWS profile" })).toHaveValue("prod");
  await expect(dialog(page).getByRole("textbox", { name: "Instance id" })).toHaveValue("i-0123456789abcdef0");
});

test("AWS SSM: a local Host gets a hint on the General tab, not an error", async ({ page }) => {
  await openProps(page);
  const d = dialog(page);
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  await d.getByRole("tab", { name: "General" }).click();
  const host = d.locator(".ui-row").filter({ hasText: "Port" }).getByRole("textbox").first();
  const hint = d.getByText("is the instance itself");
  await host.fill("127.0.0.1");
  await expect(hint).toHaveText("With AWS SSM, 127.0.0.1 is the instance itself: Host and Port are the database as seen from the instance (e.g. an RDS endpoint and 5432), not a local port. Keep it if the database runs on the instance.");
  await host.fill("orders.cluster-abc.ap-northeast-2.rds.amazonaws.com");
  await expect(hint).toHaveCount(0);
  await host.fill("localhost");
  await expect(hint).toBeVisible();
  // Only with SSM, and it doesn't block saving
  await d.getByRole("tab", { name: /SSH\/SSL/ }).click();
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("none");
  await d.getByRole("tab", { name: "General" }).click();
  await expect(hint).toHaveCount(0);
  await d.getByRole("tab", { name: /SSH\/SSL/ }).click();
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  await d.getByRole("textbox", { name: "Instance id" }).fill("i-0123456789abcdef0");
  await d.getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
});

test("AWS SSM: a long Test Connection error is scrolled into view in the fixed-size dialog", async ({ page }) => {
  // Short enough that the error ends below the body with macOS fonts
  await page.setViewportSize({ width: 1100, height: 640 });
  await openProps(page);
  const d = dialog(page);
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  // The mock's instance whose test fails with the missing-plugin message
  await d.getByRole("textbox", { name: "Instance id" }).fill("i-0000000000000dead");
  const box = await d.boundingBox();
  await d.getByRole("button", { name: "Test Connection" }).click();
  const notice = d.locator(".ui-notice.tone-danger");
  await expect(notice).toContainText("session-manager-working-with-install-plugin.html), then try again.");
  expect(await d.boundingBox(), "the dialog keeps its size").toEqual(box);
  const body = d.locator(".ui-modal-body");
  await expect
    .poll(async () => {
      const [n, b] = [await notice.boundingBox(), await body.boundingBox()];
      return !!n && !!b && n.y >= b.y && n.y + n.height <= b.y + b.height;
    }, { message: "the whole error, its last line included, is inside the dialog body" })
    .toBe(true);
  // Where the error didn't fit (it depends on the fonts: CI's Linux ones are shorter), the body scrolled to it; the tab strip stays
  // pinned at its top either way: the labels are still shown, not only the active underline
  const [overflows, scrolled] = await body.evaluate((el) => [el.scrollHeight > el.clientHeight + 1, el.scrollTop > 0]);
  expect(scrolled, "the body scrolled to show the error").toBe(overflows);
  const tab = d.getByRole("tab", { name: /SSH\/SSL/ });
  const [t, b] = [await tab.boundingBox(), await body.boundingBox()];
  expect(t && b && t.y >= b.y && t.y + t.height <= b.y + b.height, "the SSH/SSL tab is inside the dialog body").toBe(true);
  expect(await tab.evaluate((el) => {
    const r = el.getBoundingClientRect();
    return el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
  }), "nothing covers the SSH/SSL tab").toBe(true);
});

test("AWS SSM: a Test Connection that keeps starting shows the elapsed time and can be cancelled", async ({ page }) => {
  await openProps(page);
  const d = dialog(page);
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  // The mock's instance that never connects (like a stopped one)
  await d.getByRole("textbox", { name: "Instance id" }).fill("i-00000000000000000");
  const box = await d.boundingBox();
  await d.getByRole("button", { name: "Test Connection" }).click();
  await expect(d.getByRole("status")).toContainText(/Testing… \d+\.\d s/);
  await expect(d.getByRole("button", { name: "Test Connection" })).toHaveCount(0);
  expect(await d.boundingBox(), "the dialog keeps its size while testing").toEqual(box);
  await d.getByRole("button", { name: "Cancel Test" }).click();
  await expect(d.getByText("Test cancelled")).toBeVisible();
  await expect(d.getByRole("status")).toHaveCount(0);
  await expect(d.getByRole("button", { name: "Test Connection" })).toBeEnabled();

  // Esc while testing cancels the test and keeps the dialog and its edits (like the TUI); the next Esc closes it
  await d.getByRole("button", { name: "Test Connection" }).click();
  await expect(d.getByRole("status")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(d.getByText("Test cancelled")).toBeVisible();
  await expect(d.getByRole("textbox", { name: "Instance id" })).toHaveValue("i-00000000000000000");

  // Choosing another Tunnel stops the test: no result for the old tunnel shows up afterwards
  await d.getByRole("button", { name: "Test Connection" }).click();
  await expect(d.getByRole("status")).toBeVisible();
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("ssh");
  await expect(d.getByRole("status")).toHaveCount(0);
  await expect(d.getByRole("button", { name: "Test SSH Connection" })).toBeEnabled();
  // An empty private key uses ssh-agent, then the ~/.ssh/config IdentityFile keys (core tunnel.rs)
  await expect(d.getByRole("textbox", { name: "Private key" })).toHaveAttribute("placeholder", /ssh-agent/);
  await expect(d.getByText("Host and port on General are reached from the SSH server.")).toBeVisible();
  await d.getByRole("button", { name: "About SSH tunnels" }).click();
  await expect(page.getByRole("note", { name: "About SSH tunnels" })).toContainText("~/.ssh/config Host");
  await d.getByRole("button", { name: "About SSH tunnels" }).click();
  await page.waitForTimeout(300);
  await expect(d.getByText("Test cancelled")).toHaveCount(0);
  await expect(d.locator(".conn-test-actions")).toHaveCount(0);

  await d.getByRole("button", { name: "Cancel", exact: true }).focus();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

// Same rule as Manage Shown Schemas: under All, every database / schema shows checked; unchecking one keeps the others, checking
// every one is All again
test("Schemas tab: unchecking one database or schema under All keeps the others; checking every one is All again", async ({ page }) => {
  await treeNode(page, "[DEV] shop_app").click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Properties/ }).click();
  const d = dialog(page);
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  const box = (name: string) => d.locator(".scope-list").getByRole("checkbox", { name, exact: true });
  await expect(box("All databases")).toBeChecked();
  await expect(box("shop_app_test")).toBeChecked();
  await box("shop_app_test").click();
  await expect(box("All databases")).not.toBeChecked();
  await expect(box("shop_app_test")).not.toBeChecked();
  await expect(box("postgres")).toBeChecked();
  await expect(box("shop_app")).toBeChecked();
  await expect(d.getByRole("tab", { name: "Schemas ●" })).toBeVisible();
  await box("shop_app_test").click();
  await expect(box("All databases")).toBeChecked();
  await expect(d.getByRole("tab", { name: "Schemas", exact: true })).toBeVisible();

  const row = d.locator(".scope-row", { has: page.getByRole("checkbox", { name: "shop_app", exact: true }) });
  await row.locator(".twisty").click();
  await expect(box("billing")).toBeChecked();
  await box("billing").click();
  await expect(box("billing")).not.toBeChecked();
  await expect(box("public")).toBeChecked();
  await expect(row).toContainText("1 schemas");
  await box("billing").click();
  await expect(row).toContainText("all schemas");
});

async function newSource(page: import("@playwright/test").Page) {
  await page.locator(".ws-explorer.on").getByTitle("Add Data Source").click();
  await page.getByRole("menuitem", { name: "New Data Source…" }).click();
  const d = dialog(page);
  await expect(d.locator(".ui-modal-title")).toHaveText("New Data Source");
  return d;
}

// A half-filled new source isn't connected (the server would only answer with an error): the tab says what's left, then lists
test("Schemas tab of a new data source that isn't filled in says what's missing and doesn't connect", async ({ page }) => {
  await page.evaluate(() => {
    const w = window as any;
    const orig = w.__TAURI_INTERNALS__.invoke;
    w.__lists = 0;
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: unknown) => (cmd === "form_databases" && w.__lists++, orig(cmd, args));
  });
  const d = await newSource(page);
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  // The form starts on localhost: the user is what's left
  await expect(d.getByText("Fill in User on the General tab to list the databases here.")).toBeVisible();
  await expect(d.locator(".ui-notice.tone-danger")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__lists)).toBe(0);
  // Filled in: listed when the tab is shown again
  await d.getByRole("tab", { name: "General" }).click();
  await d.getByPlaceholder("user@localhost").fill("orders");
  await d.locator(".ui-row").filter({ hasText: "Port" }).getByRole("textbox").first().fill("orders-db.example.com");
  await d.getByRole("textbox", { name: "User", exact: true }).fill("app");
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  await expect(d.locator(".scope-list").getByRole("checkbox", { name: "All databases", exact: true })).toBeChecked();
  // (twice in development: React's StrictMode mounts the tab twice, the first one is cancelled)
  expect(await page.evaluate(() => (window as any).__lists)).toBeGreaterThan(0);
});

// DataGrip lists a new data source's databases and schemas with the form's settings, before it is saved (as Test Connection connects)
test("Schemas tab of a new data source lists with the form's settings; the scope is saved with OK", async ({ page }) => {
  const d = await newSource(page);
  await d.getByPlaceholder("user@localhost").fill("orders");
  await d.locator(".ui-row").filter({ hasText: "Port" }).getByRole("textbox").first().fill("orders-db.example.com");
  await d.getByRole("textbox", { name: "User", exact: true }).fill("app");
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  const box = (name: string) => d.locator(".scope-list").getByRole("checkbox", { name, exact: true });
  await expect(box("All databases")).toBeChecked();
  await expect(d.getByText("Save the data source first")).toHaveCount(0);
  await expect(d.getByText("Only checked items show in the Database Explorer; none checked shows all.")).toBeVisible();
  await d.getByRole("button", { name: "About the Schemas tab" }).click();
  await expect(page.getByRole("note", { name: "About the Schemas tab" })).toHaveText("Listed with this dialog's settings, as Test Connection connects; nothing is saved until OK.");
  await page.keyboard.press("Escape");
  await box("shop_app_test").click();
  const row = d.locator(".scope-row", { has: page.getByRole("checkbox", { name: "shop_app", exact: true }) });
  await row.locator(".twisty").click();
  await box("billing").click();
  await expect(row).toContainText("1 schemas");
  await d.getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);

  // Saved with the source: Properties shows the same choice, listed through the saved source
  await treeNode(page, "orders").click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Properties/ }).click();
  await d.getByRole("tab", { name: "Schemas ●" }).click();
  await expect(box("shop_app_test")).not.toBeChecked();
  await expect(box("postgres")).toBeChecked();
  await expect(row).toContainText("1 schemas");
  await expect(d.getByText("nothing is saved until OK")).toHaveCount(0);
});

test("Schemas tab of a new data source: a failed connect says why and tries again with the dialog's settings then", async ({ page }) => {
  const d = await newSource(page);
  await d.getByRole("textbox", { name: "User", exact: true }).fill("app");
  // The mock's server without TLS: refused until Allow unencrypted is ticked
  await d.locator(".ui-row").filter({ hasText: "Port" }).getByRole("textbox").first().fill("10.0.0.12");
  const size = await d.boundingBox();
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  const list = d.locator(".scope-list");
  await expect(d.locator(".ui-notice.tone-danger")).toContainText("✕ ");
  await expect(list).toHaveCount(0);
  expect(await d.boundingBox(), "the dialog keeps its size").toEqual(size);
  await d.getByRole("button", { name: "Try Again" }).click();
  await expect(d.getByRole("button", { name: "Try Again" })).toBeVisible();

  // Fixed through Test Connection's Allow unencrypted: Try Again uses the dialog's settings as they are now
  await d.getByRole("button", { name: "Test Connection" }).click();
  await d.getByRole("button", { name: "Allow unencrypted for this data source" }).last().click();
  await expect(d.getByText(/^✓ /)).toBeVisible();
  await d.getByRole("button", { name: "Try Again" }).click();
  await expect(list.getByRole("checkbox", { name: "shop_app", exact: true })).toBeVisible();

  // Or right under the Schemas error: it ticks the setting in the form and lists again
  await d.getByRole("tab", { name: "Options" }).click();
  await d.getByText("Allow unencrypted connection").click();
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  await expect(list).toHaveCount(0);
  await d.locator(".ui-modal-body").getByRole("button", { name: "Allow unencrypted for this data source" }).first().click();
  await expect(list.getByRole("checkbox", { name: "shop_app", exact: true })).toBeVisible();

  // A database whose schemas can't be listed says why in its place; expanding it again tries again
  await page.evaluate(() => ((window as any).__mockDbRefuse = "shop_app_test"));
  const row = d.locator(".scope-row", { has: page.getByRole("checkbox", { name: "shop_app_test", exact: true }) });
  await row.locator(".twisty").click();
  await expect(d.locator(".scope-children")).toHaveText('permission denied for database "shop_app_test"');
  await row.locator(".twisty").click();
  await page.evaluate(() => delete (window as any).__mockDbRefuse);
  await row.locator(".twisty").click();
  await expect(list.getByRole("checkbox", { name: "billing", exact: true })).toBeVisible();
});

test("Schemas tab of a new data source: a slow connect shows the time and Cancel Listing (or Esc) stops it", async ({ page }) => {
  const d = await newSource(page);
  await d.getByRole("textbox", { name: "User", exact: true }).fill("app");
  const size = await d.boundingBox();
  // An AWS SSM instance that never connects: elapsed time, Esc and Cancel Listing stop it and keep the dialog
  await d.getByRole("tab", { name: /SSH\/SSL/ }).click();
  await d.getByRole("combobox", { name: "Tunnel" }).selectOption("ssm");
  await d.getByRole("textbox", { name: "Instance id" }).fill("i-00000000000000000");
  await d.getByRole("tab", { name: /^Schemas/ }).click();
  await expect(d.getByRole("status")).toContainText(/Connecting with this dialog's settings… \d+\.\d s/);
  expect(await d.boundingBox(), "the dialog keeps its size while connecting").toEqual(size);
  await page.keyboard.press("Escape");
  await expect(d.getByText("✕ Cancelled.")).toBeVisible();
  await d.getByRole("button", { name: "Try Again" }).click();
  await expect(d.getByRole("status")).toBeVisible();
  await d.getByRole("button", { name: "Cancel Listing" }).click();
  await expect(d.getByText("✕ Cancelled.")).toBeVisible();
  await expect(dialog(page)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("Options › MCP: admin reports for AI tools ask on Production only, and MCP Off disables them", async ({ page }) => {
  const label = "AI tools can read admin reports";
  const props = async (name: string) => {
    await treeNode(page, name).click({ button: "right" });
    await page.getByRole("menuitem", { name: /^Properties/ }).click();
    await dialog(page).getByRole("tab", { name: "Options" }).click();
    return dialog(page);
  };
  const saved = () => page.evaluate(() => (window as any).__mockSaved.options);
  // Production that never set MCP: Off, so the row is disabled
  let d = await props("[PROD] shop_app");
  const box = d.getByRole("checkbox", { name: label });
  await expect(box).toBeDisabled();
  await expect(box).not.toBeChecked();
  const size = await d.boundingBox();
  // Read: editable and off by default, and OK sends it
  await d.getByRole("combobox", { name: "MCP (AI tools)" }).selectOption("read");
  await expect(box).toBeEnabled();
  await expect(box).not.toBeChecked();
  await box.check();
  expect(await d.boundingBox(), "the dialog keeps its size").toEqual(size);
  await d.getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(await saved()).toMatchObject({ confirmChanges: true, mcp: "read", mcpDbaReports: true });

  // Production turned off: disabled and checked (other sources always answer); MCP Off: disabled and unchecked.
  // Unchecking it on Production first is what the save keeps: a disabled row saves the value chosen, not the one shown
  d = await props("[PROD] shop_app");
  await expect(box).toBeChecked();
  await box.uncheck();
  await d.getByRole("checkbox", { name: "Production: show affected rows and confirm changes" }).uncheck();
  await expect(box).toBeDisabled();
  await expect(box).toBeChecked();
  await d.getByRole("combobox", { name: "MCP (AI tools)" }).selectOption("off");
  await expect(box).toBeDisabled();
  await expect(box).not.toBeChecked();
  await d.getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect(await saved()).toMatchObject({ confirmChanges: false, mcp: "off", mcpDbaReports: false });

  // Not Production, shared with MCP: disabled and checked, nothing written
  d = await props("[DEV] shop_app");
  await d.getByRole("combobox", { name: "MCP (AI tools)" }).selectOption("read");
  await expect(box).toBeDisabled();
  await expect(box).toBeChecked();
  await d.getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await saved()).mcpDbaReports).toBeUndefined();
});
