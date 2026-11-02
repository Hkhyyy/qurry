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
  await expect(d.getByText("Qurry stores no AWS secrets")).toBeVisible();
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

test("AWS SSM: a long Test Connection error is scrolled into view in the fixed-size dialog", async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 700 });
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
  await page.waitForTimeout(300);
  await expect(d.getByText("Test cancelled")).toHaveCount(0);
  await expect(d.locator(".conn-test-actions")).toHaveCount(0);

  await d.getByRole("button", { name: "Cancel", exact: true }).focus();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});
