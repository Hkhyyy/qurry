import { describe, expect, it, vi } from "vitest";
import { KEYMAP } from "./keymap";
import { shared } from "./shared";
import { bindable, bindings, conflicts, keyLabel, SHORTCUTS } from "./shortcuts";

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

describe("bindable / conflicts", () => {
  const specs = [...new Set(SHORTCUTS.flatMap((s) => s.keys)), "L", "Enter", "Shift+Tab", "Up", "Shift+F", "Shift+F1", "F12", "Mod+K", "Ctrl+Shift+K", "Alt+\\"];

  it("answer as qurry_shared::keymap does (through WASM)", () => {
    for (const spec of specs) expect([spec, bindable(spec)]).toEqual([spec, shared().bindable(spec)]);
    for (const s of SHORTCUTS)
      for (const spec of specs) {
        const wasm = shared().keyConflicts(s.id, s.scope, spec, SHORTCUTS.map((x) => ({ id: x.id, scope: x.scope, keys: bindings(x.id) })));
        expect([s.id, spec, conflicts(s.id, spec).map((x) => x.id)]).toEqual([s.id, spec, wasm]);
      }
  });

  it("work before the WASM module has loaded (keymap editor can still record)", async () => {
    vi.resetModules();
    const fresh = await import("./shortcuts");
    expect((await import("./shared")).sharedReady()).toBe(false);
    expect(fresh.bindable("Mod+K")).toBe(true);
    expect(fresh.bindable("K")).toBe(false);
    expect(fresh.conflicts("tree.duplicate", "Mod+D").map((s) => s.id)).toEqual([]);
    expect(fresh.conflicts("goto.file", "Mod+D").map((s) => s.id)).toEqual(["tree.duplicate", "editor.duplicate"]);
  });
});

describe("docs/shortcuts-gui.md", () => {
  it("shows each rebindable row's default keys as qurry_shared::keymap::format_spec draws them (scripts/gen-keymap-docs.py has its own copy)", () => {
    const esc = (s: string) => s.replace(/\|/g, "\\|");
    const cell = (k: string) => (k.includes("`") ? `\`\` ${k} \`\`` : `\`${k}\``);
    const docs = import.meta.glob<string>(["../../docs/shortcuts-gui.md", "../../docs/ko/shortcuts-gui.md"], { query: "?raw", import: "default", eager: true });
    expect(Object.keys(docs)).toHaveLength(2);
    for (const text of Object.values(docs)) {
      for (const row of KEYMAP.flatMap((s) => s.rows)) {
        if (row[0] !== null) continue;
        const keys = SHORTCUTS.find((s) => s.id === row[2])!.keys.map((k) => shared().formatSpec(k, true)).join(" / ");
        expect(text).toContain(`| ${cell(esc(keys))} | ${esc(keyLabel(row[1]))} |`);
      }
    }
  });
});
