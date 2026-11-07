import { describe, expect, it } from "vitest";
import TABLE from "../../crates/shared/src/themes.json";
import { shared } from "./shared";

// The editor's warnings come from the shared WASM (qurry_shared::theme, the Rust test checks the same table)
describe("theme contrast (WASM)", () => {
  it("finds no pair below its minimum in any built-in theme", () => {
    for (const t of TABLE.themes) expect(shared().themeWarnings(t.colors), t.id).toEqual([]);
  });
  it("warns about a custom palette's low pairs, by token", () => {
    const colors = { ...TABLE.themes[0].colors, comment: "#3a3c40", text: "#55585e" };
    const w = shared().themeWarnings(colors);
    expect(w.map((x) => x.token)).toEqual(expect.arrayContaining(["text", "comment", "sel"]));
    const c = w.find((x) => x.token === "comment")!;
    expect([c.fg, c.bg, c.min, c.ratio < 3]).toEqual(["comment", "panel2", 3, true]);
  });
  it("checks a color on the surfaces it is drawn on, not only the background", () => {
    const dracula = TABLE.themes.find((t) => t.id === "dracula")!.colors;
    const w = shared().themeWarnings({ ...dracula, muted: "#6272a4" });
    expect(w.map((x) => [x.token, x.bg])).toEqual([["muted", "sel"]]);
  });
  it("refuses theme files that aren't one, saying why", () => {
    const ok = shared().parseThemeFile(JSON.stringify({ qurryTheme: 1, name: "Mine", base: "dracula", colors: { bg: "#101010" } }));
    expect(ok).toEqual({ id: "", name: "Mine", base: "dracula", colors: { bg: "#101010" } });
    expect(shared().parseThemeFile(shared().themeFileText({ ...ok, name: "Again" })).name).toBe("Again");
    expect(() => shared().parseThemeFile('{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "#12"}}')).toThrow(/#rrggbb/);
    expect(() => shared().parseThemeFile('{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"glow": "#123456"}}')).toThrow(/Unknown color "glow"/);
    expect(() => shared().parseThemeFile("x".repeat(20000))).toThrow(/at most 16 KB/);
  });
});
