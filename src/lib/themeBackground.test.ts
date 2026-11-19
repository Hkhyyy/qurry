import { describe, expect, it } from "vitest";
import TABLE from "../../crates/shared/src/themes.json";
import { shared } from "./shared";
import { backdropStyle, mixHex, newBackgroundSlot, newCornerSlot, opacityCap, stickerStyle, warningsOver } from "./themeBackground";

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
  it("doesn't count a pair the palette already fails", () => {
    const p = { ...colors("dark"), comment: "#3a3c40" };
    expect(warn(p).map((w) => w.token)).toContain("comment");
    expect(warningsOver(p, 50, warn).map((w) => w.token)).not.toContain("comment");
  });
  it("draws the background's fill, placement and opacity as CSS", () => {
    const b = { file: "x.webp", opacity: 40, fill: "tile", anchor: "right bottom" } as const;
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
    expect(newBackgroundSlot("a.png")).toEqual({ file: "a.png", opacity: 15, fill: "cover", anchor: "center" });
    expect(newCornerSlot("a.gif")).toEqual({ file: "a.gif", corner: "right bottom", size: 20, opacity: 100 });
  });
});
