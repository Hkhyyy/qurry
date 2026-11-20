import type { Page } from "@playwright/test";
import { dialog, expect, menuItem, openApp, openConsole, openInTheme, runAll, test, treeItem, typeSql, work } from "./app";

// Theme images (#202): a custom theme's Background (behind the SQL editor and the result grid) and Corner (scaled down in front of the
// window, under menus and dialogs, clicks passing through) images, set in Settings › Appearance › Theme › Edit Colors… › Images. The
// mock answers the picker with window.__mockThemeImagePick and the core's checks (src/lib/mock.ts)

const RIDGES = "a1b2c3d4e5f60718.png";
const BEACON = "0f1e2d3c4b5a6978.gif";
const BEACON_APNG = "1a2b3c4d5e6f7081.apng";
const pickNext = (page: Page, file: string | null) => page.evaluate((f) => ((window as any).__mockThemeImagePick = f), file);
const slot = (page: Page, name: "Background" | "Corner") => dialog(page).getByRole("group", { name, exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem("mock-themeSettings") ?? "null"));
const openEditor = async (page: Page) => {
  await page.keyboard.press("Meta+Comma");
  await dialog(page).getByRole("button", { name: "Edit Colors…" }).click();
};
/** The window's Background layer behind the open table, and the Corner sticker (not the preview's) */
/** The window's Background layer: the editor area's (span "area", the default) or the open tab's own (span "pane") */
const windowLayer = (page: Page) => page.locator(".theme-editor-area > .theme-backdrop").or(work(page).locator(".theme-backdrop"));
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
    background: { file: RIDGES, opacity: 15, fill: "cover", anchor: "center", span: "area", play: "focused" },
    corner: { file: BEACON, corner: "right bottom", size: 20, opacity: 100, play: "focused" },
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

/** A pixel of the page as drawn (a 1×1 screenshot read back in the page) */
async function pixel(page: Page, x: number, y: number): Promise<number[]> {
  const png = (await page.screenshot({ clip: { x: Math.round(x), y: Math.round(y), width: 1, height: 1 } })).toString("base64");
  return page.evaluate(async (d) => {
    const img = new Image();
    img.src = d;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = c.height = 1;
    const g = c.getContext("2d")!;
    g.drawImage(img, 0, 0);
    return [...g.getImageData(0, 0, 1, 1).data.slice(0, 3)];
  }, `data:image/png;base64,${png}`);
}
const far = (a: number[], b: number[]) => Math.max(...a.map((v, i) => Math.abs(v - b[i]))) > 8;
/** The same pixels drawn without the image (the host's image class off and its layer gone), then with it again */
async function withoutImage<T>(page: Page, f: () => Promise<T>): Promise<T> {
  const toggle = (on: boolean) =>
    page.evaluate((on) => document.querySelectorAll<HTMLElement>(".theme-backdrop-host").forEach((h) => {
      h.classList.toggle("has-image", on);
      h.querySelector<HTMLElement>(":scope > .theme-backdrop")?.style.setProperty("visibility", on ? "" : "hidden");
    }), on);
  await toggle(false);
  try {
    return await f();
  } finally {
    await toggle(true);
  }
}
/** A point in a cell's empty right part (its text is on the left) */
const blank = async (page: Page, name: string) => {
  const r = (await work(page).getByRole("gridcell", { name, exact: true }).first().boundingBox())!;
  return [r.x + r.width - 8, r.y + r.height / 2] as const;
};

for (const where of ["table tab", "console result"] as const) {
  test(`the Background image shows through the grid's rows and cells in a ${where}; the selection stays visible`, async ({ page }) => {
    await openInTheme(page, "custom-1", pictures({ background: { file: RIDGES, opacity: 60, fill: "cover", anchor: "center" } }));
    if (where === "table tab") {
      await treeItem(page, "users").first().dblclick();
    } else {
      await openConsole(page);
      await typeSql(page, "select * from users limit 10");
      await runAll(page);
    }
    await expect(work(page).getByRole("gridcell", { name: "user_2", exact: true })).toBeVisible();
    await expect(windowLayer(page)).toHaveCount(1);
    // An even row (user_2) and an odd row (user_3): the image, not the opaque row color
    const [a, b] = [await blank(page, "user_2"), await blank(page, "user_3")];
    const [even, odd] = [await pixel(page, ...a), await pixel(page, ...b)];
    const [evenPlain, oddPlain] = await withoutImage(page, async () => [await pixel(page, ...a), await pixel(page, ...b)]);
    expect(far(even, evenPlain), `even row ${even} vs ${evenPlain}`).toBe(true);
    expect(far(odd, oddPlain), `odd row ${odd} vs ${oddPlain}`).toBe(true);
    // The selection: a tint over the image, still set apart from the row
    await work(page).getByRole("gridcell", { name: "user_2", exact: true }).click();
    await expect(work(page).getByRole("gridcell", { name: "user_2", exact: true })).toHaveAttribute("aria-selected", "true");
    const sel = await pixel(page, ...a);
    const selPlain = await withoutImage(page, () => pixel(page, ...a));
    expect(far(sel, even), `selected ${sel} vs the row ${even}`).toBe(true);
    expect(far(sel, selPlain), `selected ${sel} vs opaque ${selPlain}`).toBe(true);
  });
}

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
  test("an animated PNG (kept as .apng) is drawn as its first frame too", async ({ page }) => {
    await openInTheme(page, "custom-1", pictures({ corner: { ...CORNER, file: BEACON_APNG } }));
    await expect(windowSticker(page)).toBeVisible();
    // The file is an image/apng blob; what is drawn is the canvas still, an image/png
    const type = await page.evaluate(async (u) => (await (await fetch(u)).blob()).type, (await windowSticker(page).getAttribute("src"))!);
    expect(type).toBe("image/png");
  });
  test("a PNG is drawn as it is, never re-encoded into a still", async ({ page }) => {
    await openInTheme(page, "custom-1", pictures({ background: { file: RIDGES, opacity: 15, fill: "cover", anchor: "center" } }));
    await treeItem(page, "users").first().dblclick();
    await expect(windowLayer(page)).toHaveCount(1);
    const src = await windowLayer(page).evaluate((e) => getComputedStyle(e).backgroundImage.slice(5, -2));
    // The mock's PNG file, byte for byte (a still would be a canvas copy)
    const size = await page.evaluate(async (u) => (await (await fetch(u)).arrayBuffer()).byteLength, src);
    expect(size).toBe(10949);
  });
});

test("Export… carries the images (version 2) and Import… on another computer brings both back and draws them", async ({ page, browser }) => {
  // Two app loads (this computer and another one) and two image round trips: past the default 30 s on CI's shared runners
  test.slow();
  const bg = { file: RIDGES, opacity: 25, fill: "contain", anchor: "right bottom", span: "pane" };
  await openInTheme(page, "custom-1", pictures({ background: bg, corner: CORNER }));
  await openEditor(page);
  const message = dialog(page).locator(".theme-editor-actions .ui-notice");
  await dialog(page).getByRole("button", { name: "Export…" }).click();
  await expect(message).toHaveText("Exported to /Users/me/Downloads/Pictures.qurry-theme.json with its images (Qurry 0.3.5-beta.1 and older can't import such a file)");
  const text: string = await page.evaluate(() => (window as any).__mockThemeExported);
  const file = JSON.parse(text);
  expect(file.qurryTheme).toBe(2);
  expect(Object.keys(file.images).sort()).toEqual(["background", "corner"]);
  expect(file.images.background).toMatchObject({ type: "png", opacity: 25, fill: "contain", anchor: "right bottom", span: "pane" });
  // Never the local file names
  expect(text).not.toContain(RIDGES);
  expect(text).not.toContain(BEACON);
  // Another computer: a fresh mock without these images
  const other = await (await browser.newContext()).newPage();
  await openApp(other);
  await openEditor(other);
  await other.evaluate((f) => ((window as any).__mockThemeFile = f), text);
  await dialog(other).getByRole("button", { name: "Import…" }).click();
  await expect(dialog(other).locator(".theme-editor-actions .ui-notice")).toHaveText(/^Imported "Pictures" \(based on Dark\) with its background and corner images\. Check it/);
  await expect(slot(other, "Background")).toContainText("PNG image");
  await expect(slot(other, "Corner")).toContainText("GIF image");
  await expect(slot(other, "Background").getByRole("combobox", { name: "Span" })).toHaveValue("pane");
  // Drawn: the preview's layer and sticker from the imported bytes
  await expect(dialog(other).locator(".theme-preview .theme-backdrop")).toHaveCount(1);
  await expect(dialog(other).locator(".theme-preview img.theme-sticker")).toBeVisible();
  expect(await dialog(other).locator(".theme-preview img.theme-sticker").evaluate((i: HTMLImageElement) => i.naturalWidth)).toBe(120);
  // A slot that fails is left out and named; the rest comes in
  const bad = JSON.parse(text);
  bad.images.background.data = btoa('<svg xmlns="http://www.w3.org/2000/svg"/>');
  await other.evaluate((f) => ((window as any).__mockThemeFile = f), JSON.stringify(bad));
  await dialog(other).getByRole("button", { name: "Import…" }).click();
  // The first import isn't saved: replacing it asks first
  await other.getByRole("button", { name: "Discard" }).click();
  await expect(dialog(other).locator(".theme-editor-actions .ui-notice")).toHaveText(/with its corner image; left out: The background image isn't a PNG, JPEG, GIF or WebP image/);
  await expect(slot(other, "Background")).toContainText("No image");
});

test("Export… of a theme without images writes version 1, as before", async ({ page }) => {
  await openInTheme(page, "custom-1", pictures({}));
  await openEditor(page);
  await dialog(page).getByRole("button", { name: "Export…" }).click();
  await expect(dialog(page).locator(".theme-editor-actions .ui-notice")).toHaveText("Exported to /Users/me/Downloads/Pictures.qurry-theme.json");
  expect(JSON.parse(await page.evaluate(() => (window as any).__mockThemeExported))).toEqual({ qurryTheme: 1, name: "Pictures", base: "dark", colors: {} });
});

test("OK saves an image picked while editing a saved custom theme; it is there when the editor opens again", async ({ page }) => {
  await openInTheme(page, "custom-1", pictures({}));
  await openEditor(page);
  await slot(page, "Background").getByRole("button", { name: "Choose Background image" }).click();
  await expect(slot(page, "Background")).toContainText("PNG image");
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect.poll(async () => (await saved(page))?.custom?.[0]?.images?.background?.file).toBe(RIDGES);
  expect((await saved(page)).custom).toHaveLength(1);
  await openEditor(page);
  await expect(slot(page, "Background")).toContainText("PNG image");
});

test("started from a built-in theme, Images says it is saved with a new custom theme (its ⓘ how); a custom theme being changed says nothing", async ({ page }) => {
  await openApp(page);
  await openEditor(page);
  const title = dialog(page).locator(".theme-bg-bar-title");
  await expect(title).toContainText("Saved with a new custom theme");
  await title.getByRole("button", { name: "About Theme images" }).click();
  await expect(page.getByRole("note", { name: "About Theme images" })).toContainText("Images belong to a custom theme: OK saves these edits as a new custom theme and picks it.");
  await page.keyboard.press("Escape");
  // A saved custom theme
  await openInTheme(page, "custom-1", pictures({}));
  await openEditor(page);
  await expect(dialog(page).locator(".theme-bg-bar-title")).not.toContainText("Saved with");
  await dialog(page).locator(".theme-bg-bar-title").getByRole("button", { name: "About Theme images" }).click();
  await expect(page.getByRole("note", { name: "About Theme images" })).not.toContainText("Images belong to a custom theme");
});

// The Background's span: "area" (the default) is one picture behind every split together, "pane" one in each console or table
const split = async (page: Page) => {
  // The tabs and splits are kept across a reload: already split (the test's first half), nothing to do
  await expect(page.locator(".editor-group").first()).toBeVisible();
  if ((await page.locator(".editor-group").count()) === 2) return;
  await treeItem(page, "users").first().dblclick();
  await treeItem(page, "contracts").first().dblclick();
  await page.getByRole("tab", { name: /contracts/ }).click({ button: "right" });
  await menuItem(page, "Split Right").click();
  await expect(page.locator(".editor-group")).toHaveCount(2);
};
/** The image layers drawn (not those of hidden tabs) */
const layers = (page: Page) => page.locator(".theme-backdrop:visible");
const box = async (l: import("@playwright/test").Locator) => (await l.boundingBox())!;

test("spanning the editor area, one picture lies behind both split panes; per pane, each visible pane has its own", async ({ page }) => {
  const bg = { file: RIDGES, opacity: 40, fill: "cover", anchor: "center" };
  await openInTheme(page, "custom-1", pictures({ background: bg }));
  await split(page);
  // Saved without a span (0.3.5-beta.1): the whole area. One layer, covering both groups
  await expect(layers(page)).toHaveCount(1);
  const [layer, left, right] = [await box(layers(page)), await box(page.locator(".editor-group").first()), await box(page.locator(".editor-group").last())];
  expect(layer.x).toBeLessThanOrEqual(left.x + 1);
  expect(layer.x + layer.width).toBeGreaterThanOrEqual(right.x + right.width - 1);
  expect(layer.y).toBeLessThanOrEqual(Math.min(left.y, right.y) + 1);
  // The panes' grids let it through: their hosts are clear
  for (const host of await page.locator(".table-body.theme-backdrop-host:visible").all()) await expect(host).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  // Per pane: a layer in each visible table, inside its own pane
  await openInTheme(page, "custom-1", pictures({ background: { ...bg, span: "pane" } }));
  await split(page);
  await expect(layers(page)).toHaveCount(2);
  const [a, b] = [await box(layers(page).first()), await box(layers(page).last())];
  expect(a.x + a.width).toBeLessThanOrEqual(b.x + 1);
});

test("Span switches live in the editor and OK keeps it", async ({ page }) => {
  await openInTheme(page, "custom-1", pictures({ background: { file: RIDGES, opacity: 40, fill: "cover", anchor: "center", span: "area" } }));
  await split(page);
  await openEditor(page);
  const span = slot(page, "Background").getByRole("combobox", { name: "Span" });
  await expect(span).toHaveValue("area");
  await span.selectOption("pane");
  // Drawn at once in the window behind Settings
  await expect(page.locator(".work-area .theme-backdrop:visible")).toHaveCount(2);
  await expect(page.locator(".theme-editor-area > .theme-backdrop")).toHaveCount(0);
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect.poll(async () => (await saved(page)).custom[0].images.background.span).toBe("pane");
  await expect(layers(page)).toHaveCount(2);
  await openEditor(page);
  await expect(slot(page, "Background").getByRole("combobox", { name: "Span" })).toHaveValue("pane");
});

// When each slot's animated image plays: while focused (the default) or always; Reduce motion stops both either way
const blobType = (page: Page, src: string) => page.evaluate(async (u) => (await (await fetch(u)).blob()).type, src);
const blur = (page: Page) =>
  page.evaluate(() => {
    Object.defineProperty(document, "hasFocus", { value: () => false, configurable: true });
    window.dispatchEvent(new Event("blur"));
  });
/** The Background layer's image type and the Corner sticker's, as drawn now */
const drawnTypes = async (page: Page) => {
  const bg = await windowLayer(page).evaluate((e) => getComputedStyle(e).backgroundImage.slice(5, -2));
  const corner = (await windowSticker(page).getAttribute("src"))!;
  return [await blobType(page, bg), await blobType(page, corner)];
};
const BEACON_BG = { file: BEACON, opacity: 40, fill: "tile", anchor: "center", span: "area" };
for (const [bgPlay, cornerPlay] of [["always", "focused"], ["focused", "always"]] as const) {
  test(`background plays ${bgPlay}, corner ${cornerPlay}: with the window not focused each follows its own setting`, async ({ page }) => {
    await openInTheme(page, "custom-1", pictures({ background: { ...BEACON_BG, play: bgPlay }, corner: { ...CORNER, play: cornerPlay } }));
    await treeItem(page, "users").first().dblclick();
    await expect(windowLayer(page)).toHaveCount(1);
    await expect(windowSticker(page)).toBeVisible();
    await expect.poll(() => drawnTypes(page)).toEqual(["image/gif", "image/gif"]);
    await blur(page);
    const want = (p: string) => (p === "always" ? "image/gif" : "image/png");
    await expect.poll(() => drawnTypes(page)).toEqual([want(bgPlay), want(cornerPlay)]);
  });
}

test.describe("Reduce motion with both slots playing always", () => {
  test.use({ reducedMotion: "reduce" });
  test("both animated images show their first frame anyway", async ({ page }) => {
    await openInTheme(page, "custom-1", pictures({ background: { ...BEACON_BG, play: "always" }, corner: { ...CORNER, play: "always" } }));
    await treeItem(page, "users").first().dblclick();
    await expect(windowSticker(page)).toBeVisible();
    await expect.poll(() => drawnTypes(page)).toEqual(["image/png", "image/png"]);
  });
});

test("each slot's Play shows with an animated image only, applies live, and OK keeps it", async ({ page }) => {
  await openInTheme(page, "custom-1", pictures({ background: { file: RIDGES, opacity: 15, fill: "cover", anchor: "center", span: "area" } }));
  await openEditor(page);
  // A PNG background: no Play
  await expect(dialog(page).getByRole("combobox", { name: "Background play" })).toHaveCount(0);
  await pickNext(page, "beacon.gif");
  await slot(page, "Corner").getByRole("button", { name: "Choose Corner image" }).click();
  const play = slot(page, "Corner").getByRole("combobox", { name: "Corner play" });
  await expect(play).toHaveValue("focused");
  await expect(dialog(page).getByRole("combobox", { name: "Background play" })).toHaveCount(0);
  await play.selectOption("always");
  // Live: the window behind Settings keeps the GIF playing when the window isn't focused
  await blur(page);
  await page.waitForTimeout(500);
  expect(await blobType(page, (await windowSticker(page).getAttribute("src"))!)).toBe("image/gif");
  await dialog(page).getByRole("button", { name: "OK" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect.poll(async () => (await saved(page)).custom[0].images.corner.play).toBe("always");
  expect((await saved(page)).custom[0].images.play).toBeUndefined();
});
