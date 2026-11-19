import type { Page } from "@playwright/test";
import { dialog, expect, openApp, openInTheme, test, treeItem, work } from "./app";

// Theme images (#202): a custom theme's Background (behind the SQL editor and the result grid) and Corner (scaled down in front of the
// window, under menus and dialogs, clicks passing through) images, set in Settings › Appearance › Theme › Edit Colors… › Images. The
// mock answers the picker with window.__mockThemeImagePick and the core's checks (src/lib/mock.ts)

const RIDGES = "a1b2c3d4e5f60718.png";
const BEACON = "0f1e2d3c4b5a6978.gif";
const pickNext = (page: Page, file: string | null) => page.evaluate((f) => ((window as any).__mockThemeImagePick = f), file);
const slot = (page: Page, name: "Background" | "Corner") => dialog(page).getByRole("group", { name, exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem("mock-themeSettings") ?? "null"));
const openEditor = async (page: Page) => {
  await page.keyboard.press("Meta+Comma");
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
};
/** The window's Background layer behind the open table, and the Corner sticker (not the preview's) */
const windowLayer = (page: Page) => work(page).locator(".theme-backdrop");
const windowSticker = (page: Page) => page.locator("img.theme-sticker:not(.in-preview)");
const pictures = (images: unknown) => [{ id: "custom-1", name: "Pictures", base: "dark", colors: {}, images }];
const CORNER = { file: BEACON, corner: "right bottom", size: 40, opacity: 100 };

test("an image in each slot is drawn live in the window and the preview, saved with the theme, and removed", async ({ page }) => {
  await openApp(page);
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  await openEditor(page);
  await slot(page, "Background").getByRole("button", { name: "Choose Background image" }).click();
  await expect(slot(page, "Background")).toContainText("PNG image");
  await pickNext(page, "beacon.gif");
  await slot(page, "Corner").getByRole("button", { name: "Choose Corner image" }).click();
  await expect(slot(page, "Corner")).toContainText("GIF image");
  // Live: the window behind Settings (behind the grid, the sticker in its corner) and the preview
  await expect(windowLayer(page)).toHaveCount(1);
  expect(await windowLayer(page).evaluate((e) => getComputedStyle(e).backgroundImage)).toMatch(/^url\("blob:/);
  await expect(windowSticker(page)).toBeVisible();
  expect(await windowSticker(page).evaluate((e) => [getComputedStyle(e).pointerEvents, getComputedStyle(e).zIndex])).toEqual(["none", "20"]);
  await expect(dialog(page).locator(".theme-preview .theme-backdrop")).toHaveCount(1);
  await expect(dialog(page).locator(".theme-preview img.theme-sticker.in-preview")).toBeVisible();
  // Peek, held with Space: Settings fades; let go, it comes back
  const modal = page.locator(".ui-modal");
  await dialog(page).getByRole("button", { name: "Peek" }).focus();
  await page.keyboard.down(" ");
  await expect(modal).toHaveCSS("opacity", "0");
  await page.keyboard.up(" ");
  await expect(modal).toHaveCSS("opacity", "1");
  await expect(dialog(page).getByRole("button", { name: "Peek" })).toBeFocused();
  // Saved with the theme (the core's names)
  await dialog(page).getByRole("textbox", { name: "Theme name" }).fill("Pictures");
  await dialog(page).getByRole("button", { name: "Save as new theme" }).click();
  await expect.poll(async () => (await saved(page))?.custom?.[0]?.images).toEqual({
    background: { file: RIDGES, opacity: 15, fill: "cover", anchor: "center" },
    corner: { file: BEACON, corner: "right bottom", size: 20, opacity: 100 },
  });
  // Removed: gone from the window and the preview
  await slot(page, "Background").getByRole("button", { name: "Remove Background image" }).click();
  await slot(page, "Corner").getByRole("button", { name: "Remove Corner image" }).click();
  await expect(windowLayer(page)).toHaveCount(0);
  await expect(windowSticker(page)).toHaveCount(0);
  await expect(dialog(page).locator(".theme-preview img.theme-sticker")).toHaveCount(0);
  await expect(slot(page, "Background")).toContainText("No image");
});

test("an SVG or a file over the size cap is refused with why; nothing is set", async ({ page }) => {
  await openApp(page);
  await openEditor(page);
  await pickNext(page, "logo.svg");
  await slot(page, "Background").getByRole("button", { name: "Choose Background image" }).click();
  await expect(slot(page, "Background").getByRole("alert")).toHaveText("logo.svg isn't a PNG, JPEG, GIF or WebP image");
  await pickNext(page, "big.png");
  await slot(page, "Corner").getByRole("button", { name: "Choose Corner image" }).click();
  await expect(slot(page, "Corner").getByRole("alert")).toHaveText("big.png is 5.0 MB: a still image can be at most 4 MB");
  await expect(slot(page, "Background").getByRole("button", { name: "Remove Background image" })).toHaveCount(0);
  await expect(windowSticker(page)).toHaveCount(0);
  // Cancelled: nothing changes, the message stays until a pick works
  await pickNext(page, null);
  await slot(page, "Corner").getByRole("button", { name: "Choose Corner image" }).click();
  await expect(slot(page, "Corner").getByRole("alert")).toBeVisible();
  await pickNext(page, "beacon.gif");
  await slot(page, "Corner").getByRole("button", { name: "Choose Corner image" }).click();
  await expect(slot(page, "Corner").getByRole("alert")).toHaveCount(0);
});

test("the Corner image doesn't take a click on the grid cell under it, and stays under menus", async ({ page }) => {
  await openInTheme(page, "custom-1", pictures({ corner: CORNER }));
  await treeItem(page, "users").first().dblclick();
  await expect(work(page).getByRole("gridcell", { name: "user_1", exact: true })).toBeVisible();
  const sticker = await windowSticker(page).boundingBox();
  expect(sticker).not.toBeNull();
  // A cell whose middle is under the sticker
  const under = await work(page)
    .getByRole("gridcell")
    .evaluateAll((cells, s) => {
      const i = cells.findIndex((c) => {
        const r = c.getBoundingClientRect();
        const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
        return r.width > 0 && x > s.x && x < s.x + s.width && y > s.y && y < s.y + s.height;
      });
      if (i < 0) return null;
      const r = cells[i].getBoundingClientRect();
      return { i, x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }, sticker!);
  expect(under, "a grid cell under the sticker").not.toBeNull();
  await page.mouse.click(under!.x, under!.y);
  await expect(work(page).getByRole("gridcell").nth(under!.i)).toHaveAttribute("aria-selected", "true");
  // Its context menu opens over the sticker
  await page.mouse.click(under!.x, under!.y, { button: "right" });
  const menu = page.getByRole("menu").first();
  await expect(menu).toBeVisible();
  const [menuZ, stickerZ] = await Promise.all([menu.evaluate((e) => +getComputedStyle(e).zIndex), windowSticker(page).evaluate((e) => +getComputedStyle(e).zIndex)]);
  expect(menuZ).toBeGreaterThan(stickerZ);
});

test.describe("with Reduce motion on", () => {
  test.use({ reducedMotion: "reduce" });
  test("an animated image is drawn as its first frame, a still", async ({ page }) => {
    await openInTheme(page, "custom-1", pictures({ corner: CORNER, background: { file: BEACON, opacity: 15, fill: "tile", anchor: "center" } }));
    await treeItem(page, "users").first().dblclick();
    await expect(windowSticker(page)).toBeVisible();
    await expect(windowLayer(page)).toHaveCount(1);
    // The still is a PNG made from the GIF's first frame (a blob URL of type image/png), never the GIF itself
    const type = (src: string) => page.evaluate((u) => fetch(u).then((r) => r.blob()).then((b) => b.type), src);
    expect(await type((await windowSticker(page).getAttribute("src"))!)).toBe("image/png");
    const bg = await windowLayer(page).evaluate((e) => getComputedStyle(e).backgroundImage.slice(5, -2));
    expect(await type(bg)).toBe("image/png");
    // Its first frame: the ring sits 18 px from the middle (frame 0; later frames draw it wider and fainter)
    const at = await page.evaluate(
      (u) =>
        new Promise<number[]>((ok) => {
          const img = new Image();
          img.onload = () => {
            const c = document.createElement("canvas");
            [c.width, c.height] = [img.naturalWidth, img.naturalHeight];
            const g = c.getContext("2d")!;
            g.drawImage(img, 0, 0);
            ok([img.naturalWidth, ...g.getImageData(60 + 18, 60, 1, 1).data.slice(0, 3), g.getImageData(60 + 40, 60, 1, 1).data[0]]);
          };
          img.src = u;
        }),
      (await windowSticker(page).getAttribute("src"))!,
    );
    expect(at[0]).toBe(120);
    // #f4d35e, the ring's yellow (not the badge's green)
    expect(at[1]).toBeGreaterThan(200);
    expect(at[2]).toBeGreaterThan(170);
    // and nothing yet where the late frames draw it: the badge's green
    expect(at[4]).toBeLessThan(100);
  });
});

test("Export… leaves the images out and Import… says theme files don't include them", async ({ page }) => {
  await openInTheme(page, "custom-1", pictures({ corner: CORNER }));
  await openEditor(page);
  const message = dialog(page).locator(".theme-editor-actions .ui-notice");
  await dialog(page).getByRole("button", { name: "Export…" }).click();
  await expect(message).toHaveText("Exported to /Users/me/Downloads/Pictures.qurry-theme.json (without its images: theme files don't include them)");
  const file = JSON.parse(await page.evaluate(() => (window as any).__mockThemeExported));
  expect(file).toEqual({ qurryTheme: 1, name: "Pictures", base: "dark", colors: {} });
  await page.evaluate((f) => ((window as any).__mockThemeFile = f), JSON.stringify(file));
  await dialog(page).getByRole("button", { name: "Import…" }).click();
  await expect(message).toHaveText(/^Imported "Pictures" \(based on Dark\): theme files don't include images/);
  await expect(slot(page, "Corner")).toContainText("No image");
});
