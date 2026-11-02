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
});
