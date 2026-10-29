import { describe, expect, it } from "vitest";
import { keyLabel, SHORTCUTS } from "./shortcuts";

describe("keyLabel", () => {
  it("⌘/⌃/⌥ + letter without Shift is lowercase (uppercase = a key that needs Shift)", () => {
    expect(keyLabel("⌘K")).toBe("⌘k");
    expect(keyLabel("⌥⌘Y · ⌃⇧F · ⇧⌘A")).toBe("⌥⌘y · ⌃⇧F · ⇧⌘A");
    expect(keyLabel("Run (⌘↩) · ⌘F2 · ⌘[")).toBe("Run (⌘↩) · ⌘F2 · ⌘[");
    expect(keyLabel("Ctrl+K · Ctrl+Shift+K · Alt+Enter")).toBe("Ctrl+k · Ctrl+Shift+K · Alt+Enter");
    expect(keyLabel("⌘B / ⌘↓ · Delete")).toBe("⌘b / ⌘↓ · Delete");
  });

  it("no uppercase shortcuts are left in on-screen text in the source", () => {
    // "Mod+Alt+O" in shortcuts.ts is excluded: it is a format matched against key input, not display text (mock is fake data)
    const files = import.meta.glob<string>(["../**/*.{ts,tsx}", "!../**/*.test.ts", "!./shortcuts.ts", "!./mock.ts"], { query: "?raw", import: "default", eager: true });
    expect(Object.keys(files).length).toBeGreaterThan(10);
    const bad: string[] = [];
    for (const [f, text] of Object.entries(files)) {
      text.split("\n").forEach((line, i) => {
        if (keyLabel(line) !== line) bad.push(`${f}:${i + 1}: ${line.trim().slice(0, 100)}`);
      });
    }
    expect(bad).toEqual([]);
  });
});

describe("global shortcuts", () => {
  it("workspaces are ⌘1–9 (not ⌥, this isn't a terminal), the explorer is ⌘0, no clashing default keys", () => {
    const keys = (id: string) => SHORTCUTS.find((s) => s.id === id)?.keys;
    expect(keys("ws.1")).toEqual(["Mod+1"]);
    expect(keys("ws.9")).toEqual(["Mod+9"]);
    expect(keys("explorer")).toEqual(["Mod+0"]);
    const seen = new Map<string, string>();
    const dup: string[] = [];
    for (const s of SHORTCUTS.filter((x) => x.scope === "global"))
      for (const k of s.keys) {
        if (seen.has(k)) dup.push(`${k}: ${seen.get(k)} / ${s.id}`);
        seen.set(k, s.id);
      }
    expect(dup).toEqual([]);
  });
});
