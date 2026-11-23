import { describe, expect, it } from "vitest";
import TABLE from "../../crates/shared/src/themes.json";

// --ui-focus (tokens.css) is the accent mixed toward the text; read the share from the stylesheet so the check follows it
const fs: { readFileSync(p: URL, enc: "utf8"): string } = await import("node:" + "fs");
const css = fs.readFileSync(new URL("../ui/tokens.css", import.meta.url), "utf8");
const share = Number(/--ui-focus: color-mix\(in srgb, var\(--ui-accent\) (\d+)%, var\(--ui-text\)\)/.exec(css)?.[1]) / 100;

const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const lum = (c: number[]) => {
  const [r, g, b] = c.map((v) => ((v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const ratio = (a: number[], b: number[]) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};

describe("the focus ring color (--ui-focus)", () => {
  it("is read from tokens.css", () => expect(share).toBeGreaterThan(0));
  // Window, panels and dialogs, the second panel color, a hovered row (the theme editor's), a selected row or active icon button
  for (const t of TABLE.themes)
    it(`reaches 3:1 on every surface a ring sits on: ${t.id}`, () => {
      const c = t.colors as Record<string, string>;
      const focus = rgb(c.accent).map((v, i) => v * share + rgb(c.text)[i] * (1 - share));
      for (const s of ["bg", "panel", "panel2", "hover", "sel"]) expect(ratio(focus, rgb(c[s])), `${t.id} on ${s}`).toBeGreaterThanOrEqual(3);
    });
});
