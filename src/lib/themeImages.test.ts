import { describe, expect, it, vi } from "vitest";
import TABLE from "../../crates/shared/src/themes.json";
import { shared } from "./shared";
import { backdropStyle, holdImage, TINTS, mayMove, mixHex, releaseImage, newBackgroundSlot, newCornerSlot, opacityCap, sameImages, stickerStyle, warningsOver } from "./themeImages";

const api = vi.hoisted(() => ({ themeImage: vi.fn() }));
vi.mock("./api", () => ({ api }));

const colors = (id: string) => TABLE.themes.find((t) => t.id === id)!.colors as Record<string, string>;
const warn = (p: Record<string, string>) => shared().themeWarnings(p);

describe("theme background image", () => {
  it("mixes colors as CSS opacity does", () => {
    expect(mixHex("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mixHex("#1e1f22", "#000000", 0)).toBe("#1e1f22");
    expect(mixHex("#1e1f22", "#ffffff", 1)).toBe("#ffffff");
  });
  it("in the background, the image costs contrast above a cap that the palette's closest pair sets", () => {
    const dark = colors("dark");
    const cap = opacityCap(dark, warn);
    expect(cap).toBeGreaterThan(0);
    expect(cap).toBeLessThan(10);
    expect(warningsOver(dark, cap, warn)).toEqual([]);
    expect(warningsOver(dark, cap + 1, warn).length).toBeGreaterThan(0);
    // High contrast has more room than Dark
    expect(opacityCap(colors("hc"), warn)).toBeGreaterThan(cap);
  });
  it("checks the grid's tints as they are drawn: each laid over Background mixed toward the image's worst pixels", () => {
    const p = colors("dark");
    const seen: Record<string, string>[] = [];
    warningsOver(p, 40, (q) => (seen.push(q), []));
    // The palette as is, then over a black and a white pixel
    expect(seen).toHaveLength(3);
    for (const [q, x] of [[seen[1], "#000000"], [seen[2], "#ffffff"]] as const) {
      const bg = mixHex(p.bg, x, 0.4);
      expect(q.bg).toBe(bg);
      for (const s of ["odd", "panel", "panel2", "sel", "changed", "added", "deleted", "match"]) expect(q[s], s).toBe(mixHex(bg, p[s], TINTS[s]));
      // A surface not drawn over the image is left as it is
      expect(q.hover).toBe(p.hover);
    }
  });
  it("doesn't count a pair the palette already fails", () => {
    const p = { ...colors("dark"), comment: "#3a3c40" };
    expect(warn(p).map((w) => w.token)).toContain("comment");
    expect(warningsOver(p, 50, warn).map((w) => w.token)).not.toContain("comment");
  });
  it("draws the background's fill, placement and opacity as CSS", () => {
    const b = { file: "x.webp", opacity: 40, fill: "tile", anchor: "right bottom", span: "area" } as const;
    expect(backdropStyle(b, "x.webp")).toMatchObject({ backgroundRepeat: "repeat", backgroundSize: "auto", backgroundPosition: "right bottom", opacity: 0.4 });
    expect(backdropStyle({ ...b, fill: "cover" }, "x.webp")).toMatchObject({ backgroundRepeat: "no-repeat", backgroundSize: "cover" });
  });
  it("puts the corner sticker in its corner, sized by the window's height", () => {
    expect(stickerStyle(newCornerSlot("a.gif"))).toMatchObject({ right: 16, bottom: 16, height: "20vh", opacity: 1 });
    const s = stickerStyle({ file: "a.gif", corner: "left top", size: 35, opacity: 60 });
    expect(s).toMatchObject({ left: 16, top: 16, height: "35vh", opacity: 0.6 });
    expect(s).not.toHaveProperty("right");
  });
  it("starts a new background image at 15%, a new corner image bottom right at 20% and fully opaque", () => {
    expect(newBackgroundSlot("a.png")).toEqual({ file: "a.png", opacity: 15, fill: "cover", anchor: "center", span: "area" });
    expect(newCornerSlot("a.gif")).toEqual({ file: "a.gif", corner: "right bottom", size: 20, opacity: 100 });
  });
  it("compares two themes' images whatever order their fields come in", () => {
    const bg = newBackgroundSlot("0123456789abcdef.png");
    expect(sameImages({ background: bg }, { background: { span: bg.span, anchor: bg.anchor, fill: bg.fill, opacity: bg.opacity, file: bg.file } })).toBe(true);
    expect(sameImages(undefined, {})).toBe(true);
    expect(sameImages({}, { background: undefined })).toBe(true);
    expect(sameImages({ background: bg }, { background: { ...bg, opacity: 16 } })).toBe(false);
    expect(sameImages({ background: bg }, { corner: newCornerSlot(bg.file) })).toBe(false);
  });
  it("reads an image once while it is used, frees its blob URL when the last use ends, and reads it again after a failure", async () => {
    const made: Blob[] = [];
    const revoked: string[] = [];
    vi.stubGlobal("URL", { createObjectURL: (b: Blob) => (made.push(b), `blob:${made.length}`), revokeObjectURL: (u: string) => revoked.push(u) });
    api.themeImage.mockReset().mockResolvedValue(new ArrayBuffer(4));
    const gif = "0123456789abcdef.gif";
    expect(await holdImage(gif)).toBe("blob:1");
    expect(await holdImage(gif)).toBe("blob:1");
    expect(api.themeImage).toHaveBeenCalledTimes(1);
    expect(made[0].type).toBe("image/gif");
    releaseImage(gif);
    await Promise.resolve();
    expect(revoked).toEqual([]);
    releaseImage(gif);
    await new Promise((r) => setTimeout(r));
    expect(revoked).toEqual(["blob:1"]);
    // Used again: read again
    expect(await holdImage(gif)).toBe("blob:2");
    expect(api.themeImage).toHaveBeenCalledTimes(2);
    releaseImage(gif);
    releaseImage(gif);
    const png = "1111111111111111.png";
    api.themeImage.mockRejectedValueOnce(`The theme image ${png} is missing: choose it again`);
    await expect(holdImage(png)).rejects.toMatch(/missing/);
    expect(await holdImage(png)).toBe("blob:3");
    expect(made[2].type).toBe("image/png");
    releaseImage(png);
    expect(await holdImage("2222222222222222.apng")).toBe("blob:4");
    expect(made[3].type).toBe("image/apng");
    vi.unstubAllGlobals();
  });
  it("freezes only a GIF or a WebP: a PNG or JPEG is drawn as it is", () => {
    expect(["a.gif", "a.webp", "a.apng", "a.png", "a.jpg"].map(mayMove)).toEqual([true, true, true, false, false]);
  });
});
