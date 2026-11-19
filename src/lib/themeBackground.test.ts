import { describe, expect, it } from "vitest";
import TABLE from "../../crates/shared/src/themes.json";
import { shared } from "./shared";
import { backdropStyle, canShow, mixHex, opacityCap, warningsOver, type ThemeBackground } from "./themeBackground";
import type { Kind } from "./theme";

const theme = (id: string) => TABLE.themes.find((t) => t.id === id)! as { colors: Record<string, string>; kind: Kind };
const warn = (p: Record<string, string>) => shared().themeWarnings(p);

describe("theme background image", () => {
  it("mixes colors as CSS opacity does", () => {
    expect(mixHex("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mixHex("#1e1f22", "#000000", 0)).toBe("#1e1f22");
    expect(mixHex("#1e1f22", "#ffffff", 1)).toBe("#ffffff");
  });
  it("one-way never costs contrast: the image only darkens a dark Background", () => {
    for (const id of ["dark", "dracula", "one-dark", "solarized-dark"]) {
      const t = theme(id);
      expect(opacityCap(t.colors, t.kind, "one-way", warn), id).toBe(100);
    }
  });
  it("drawn as it is, the image costs contrast above a cap that the palette's closest pair sets", () => {
    const dark = theme("dark");
    const cap = opacityCap(dark.colors, dark.kind, "both", warn);
    expect(cap).toBeGreaterThan(0);
    expect(cap).toBeLessThan(10);
    expect(warningsOver(dark.colors, dark.kind, "both", cap, warn)).toEqual([]);
    expect(warningsOver(dark.colors, dark.kind, "both", cap + 1, warn).length).toBeGreaterThan(0);
    // High contrast has more room than Dark
    const hc = theme("hc");
    expect(opacityCap(hc.colors, hc.kind, "both", warn)).toBeGreaterThan(cap);
  });
  it("doesn't count a pair the palette already fails", () => {
    const p = { ...theme("dark").colors, comment: "#3a3c40" };
    expect(warn(p).map((w) => w.token)).toContain("comment");
    expect(warningsOver(p, "dark", "both", 50, warn).map((w) => w.token)).not.toContain("comment");
  });
  it("one-way can't show on a black or white Background", () => {
    expect(canShow(theme("hc").colors, "hc", "one-way")).toBe(false);
    expect(canShow(theme("light").colors, "light", "one-way")).toBe(false);
    expect(canShow(theme("solarized-light").colors, "light", "one-way")).toBe(true);
    expect(canShow(theme("dark").colors, "dark", "one-way")).toBe(true);
    expect(canShow(theme("light").colors, "light", "both")).toBe(true);
  });
  it("draws the fill, placement, opacity and blend as CSS", () => {
    const b: ThemeBackground = { file: "ridges.webp", opacity: 40, fill: "tile", anchor: "right bottom" };
    expect(backdropStyle(b, "x.webp", "dark", "one-way")).toMatchObject({ backgroundRepeat: "repeat", backgroundSize: "auto", backgroundPosition: "right bottom", opacity: 0.4, mixBlendMode: "multiply" });
    expect(backdropStyle({ ...b, fill: "cover" }, "x.webp", "light", "one-way")).toMatchObject({ backgroundRepeat: "no-repeat", backgroundSize: "cover", mixBlendMode: "screen" });
    expect(backdropStyle(b, "x.webp", "light", "both").mixBlendMode).toBe("normal");
  });
});
