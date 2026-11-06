import { describe, expect, it, vi } from "vitest";
import { KEYMAP, rowDesc } from "./keymap";
import { shared } from "./shared";
import { bindable, bindableIn, bindings, conflicts, keyLabel, Scope, SHORTCUTS } from "./shortcuts";

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

// The default key of every command that has one, by scope (keys joined with " / "). e2e/shortcuts.spec.ts presses only a few of them
// in the running app; this pins the rest. On failure: a default key changed, was added or removed in crates/shared/src/keymap-table.json.
// If that was intended, change it here too (and regenerate the docs with scripts/gen-keymap-docs.py)
const DEFAULT_KEYS: Record<Scope, Record<string, string>> = {
  global: {
    "goto.object": "Mod+O", "goto.symbol": "Mod+Alt+O", "goto.file": "Mod+Shift+O", "find.action": "Mod+Shift+A", fts: "Mod+Alt+Shift+F",
    recent: "Mod+E", selectIn: "Alt+F1", keymap: "Mod+Shift+/", explorer: "Mod+0", "find.window": "Mod+Shift+F", maximize: "Mod+Shift+F12",
    settings: "Mod+,", theme: "Ctrl+`", "font.bigger": "Ctrl+Shift+.", "font.smaller": "Ctrl+Shift+,", "console.new": "Mod+Shift+L",
    refresh: "Mod+Alt+Y", "tab.close": "Mod+W", "tab.reopen": "Mod+Shift+T", "tab.next": "Mod+Shift+]", "tab.prev": "Mod+Shift+[",
    // Workspaces are ⌘1–9 (not ⌥, this isn't a terminal), the explorer ⌘0
    "ws.1": "Mod+1", "ws.2": "Mod+2", "ws.3": "Mod+3", "ws.4": "Mod+4", "ws.5": "Mod+5", "ws.6": "Mod+6", "ws.7": "Mod+7", "ws.8": "Mod+8", "ws.9": "Mod+9",
    "split.other": "Alt+Tab / Alt+Shift+Tab",
  },
  tree: {
    "tree.editData": "Mod+Down / F4", "tree.jumpConsole": "Mod+Shift+F10", "tree.ddl": "Mod+B", "tree.sqlGen": "Mod+Alt+G",
    "tree.ddlCopy": "Mod+Alt+Shift+G", "tree.ddlConsole": "Mod+Alt+Shift+B", "tree.copyName": "Mod+C", "tree.copyRef": "Mod+Alt+Shift+C",
    "tree.quickDoc": "F1", "tree.modify": "Mod+F6", "tree.rename": "Shift+F6", "tree.properties": "Mod+;", "tree.newDs": "Mod+N",
    "tree.duplicate": "Mod+D", "tree.refresh": "Mod+R / Mod+Alt+Shift+R", "tree.disconnect": "Mod+F2", "tree.remove": "Delete / Mod+Backspace",
  },
  editor: {
    "editor.run": "Mod+Enter", "editor.cancel": "Mod+F2", "editor.history": "Mod+Alt+E", "console.rename": "Shift+F6", "editor.reformat": "Mod+Alt+L",
    "editor.blockComment": "Mod+Alt+/", "editor.duplicate": "Mod+D", "editor.deleteLine": "Mod+Backspace", "editor.moveUp": "Alt+Shift+Up",
    "editor.moveDown": "Alt+Shift+Down", "editor.extend": "Alt+Up", "editor.shrink": "Alt+Down", "editor.nextOcc": "Ctrl+G",
    "editor.allOcc": "Mod+Ctrl+G", "editor.toggleCase": "Mod+Shift+U", "editor.gotoLine": "Mod+L", "editor.replace": "Mod+R",
  },
  grid: {
    "grid.valueEditor": "Shift+Enter", "grid.setDefault": "Mod+Alt+D", "grid.cloneRow": "Mod+D", "grid.reload": "Mod+R", "grid.cancel": "Mod+F2",
    "grid.find": "Mod+F", "grid.ddl": "Mod+B", "grid.submit": "Mod+Enter", "grid.revert": "Mod+Alt+Z", "grid.addRow": "Mod+N",
    "grid.deleteRows": "Mod+Backspace", "grid.setNull": "Mod+Alt+N",
  },
};

/** Table notation → the key event of the same physical keys on macOS (the app reads e.code), as e2e/shortcuts.spec.ts presses them */
const CODES: Record<string, string> = { "/": "Slash", ",": "Comma", ".": "Period", ";": "Semicolon", "`": "Backquote", "[": "BracketLeft", "]": "BracketRight", Up: "ArrowUp", Down: "ArrowDown" };
function macEvent(spec: string) {
  const parts = spec.split("+");
  const k = parts.pop()!;
  const code = CODES[k] ?? (/^[A-Z]$/.test(k) ? `Key${k}` : /^\d$/.test(k) ? `Digit${k}` : k);
  return { code, metaKey: parts.includes("Mod"), ctrlKey: parts.includes("Ctrl"), altKey: parts.includes("Alt"), shiftKey: parts.includes("Shift") };
}

describe("default keys (the command table)", () => {
  it("every command with a default key has the one pinned here, in its scope", () => {
    const got: Record<string, Record<string, string>> = { global: {}, tree: {}, editor: {}, grid: {} };
    for (const s of SHORTCUTS) if (s.keys.length) got[s.scope][s.id] = s.keys.join(" / ");
    expect(got).toEqual(DEFAULT_KEYS);
  });

  it("no default key is taken twice where the scopes overlap (the same scope, or global and any scope)", () => {
    const clash = SHORTCUTS.flatMap((s) => s.keys.flatMap((k) => conflicts(s.id, k).map((o) => `${k}: ${s.id} (${s.scope}) / ${o.id} (${o.scope})`)));
    expect(clash).toEqual([]);
  });

  it("every default key can be recorded again in the keymap editor of its scope", () => {
    expect(SHORTCUTS.flatMap((s) => s.keys.filter((k) => !bindableIn(k, s.scope)).map((k) => `${s.id}: ${k}`))).toEqual([]);
  });

  it("pressing a default key on macOS reads back as that key and finds its command in its scope", async () => {
    const platform = Object.getOwnPropertyDescriptor(navigator, "platform");
    Object.defineProperty(navigator, "platform", { value: "MacIntel", configurable: true });
    try {
      vi.resetModules();
      const mac = await import("./shortcuts");
      for (const s of mac.SHORTCUTS)
        for (const k of s.keys) {
          const spec = mac.eventSpec(macEvent(k));
          expect([s.id, k, spec, mac.commandsFor(spec, s.scope)]).toEqual([s.id, k, k, [s.id]]);
        }
    } finally {
      if (platform) Object.defineProperty(navigator, "platform", platform);
      else delete (navigator as { platform?: string }).platform;
    }
  });
});

describe("bindable / conflicts", () => {
  const specs = [...new Set(SHORTCUTS.flatMap((s) => s.keys)), "L", "Enter", "Shift+Tab", "Up", "Shift+F", "Shift+F1", "F12", "Mod+K", "Ctrl+Shift+K", "Alt+\\"];

  it("answer as qurry_shared::keymap does (through WASM)", () => {
    for (const spec of specs) expect([spec, bindable(spec)]).toEqual([spec, shared().bindable(spec)]);
    const scopes: Scope[] = ["global", "tree", "editor", "grid"];
    for (const spec of [...specs, "Delete", "Mod+Backspace", "Insert", "Shift+Enter", "Shift+Delete", "N"])
      for (const scope of scopes) expect([spec, scope, bindableIn(spec, scope)]).toEqual([spec, scope, shared().bindableIn(spec, scope)]);
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
    expect(fresh.conflicts("goto.file", "Mod+D").map((s) => s.id)).toEqual(["tree.duplicate", "editor.duplicate", "grid.cloneRow"]);
    // Labels as the module draws them, not the stored "Mod+O"
    const mac = navigator.platform.toLowerCase().includes("mac");
    for (const spec of specs) expect([spec, fresh.formatSpec(spec)]).toEqual([spec, shared().formatSpec(spec, mac)]);
  });

  it("before the WASM module has loaded, labels on the other platform match the module too", async () => {
    const mac = navigator.platform.toLowerCase().includes("mac");
    const platform = Object.getOwnPropertyDescriptor(navigator, "platform");
    Object.defineProperty(navigator, "platform", { value: mac ? "Linux x86_64" : "MacIntel", configurable: true });
    try {
      vi.resetModules();
      const fresh = await import("./shortcuts");
      expect((await import("./shared")).sharedReady()).toBe(false);
      for (const spec of specs) expect([spec, fresh.formatSpec(spec)]).toEqual([spec, shared().formatSpec(spec, !mac)]);
    } finally {
      if (platform) Object.defineProperty(navigator, "platform", platform);
      else delete (navigator as { platform?: string }).platform;
    }
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
        const keys = SHORTCUTS.find((s) => s.id === row[1])!.keys.map((k) => shared().formatSpec(k, true)).join(" / ") || "—";
        expect(text).toContain(`| ${cell(esc(keys))} | ${esc(keyLabel(rowDesc(row)))} |`);
      }
    }
  });
});

describe("command ids", () => {
  it("every id the GUI runs or labels is a row of the table, and every GUI row is run from a key handler, menu, toolbar or Find Action", () => {
    // keymap.ts only lists rows for the Keymap Reference, so it doesn't count as running them
    const files = import.meta.glob<string>(["../**/*.{ts,tsx}", "!../**/*.test.ts", "!./keymap.ts", "!./shortcuts.ts", "!../ui/stories/**"], { query: "?raw", import: "default", eager: true });
    const ids = new Set(SHORTCUTS.map((s) => s.id));
    const used = new Set<string>();
    const patterns = [
      // cmd("tab.pin"), hit(spec, "grid.find"), is("tab.prev"), keysOf("editor.run"), sk("theme"), bindings("tree.refresh")
      /\b(?:cmd|hit|is|keysOf|sk|bindings)\((?:spec, )?"([\w.]+)"\)/g,
      /\bkeyTitle\("[^"]*", "([\w.]+)"\)/g,
      // The console editor's Monaco actions: act("dbtool.reformat", "Reformat Code", "editor.reformat", …)
      /\bact\("[^"]*", "[^"]*", "([\w.]+)"/g,
      // The global key handler's table: "tree.ddl": () => …, refresh: () => …, "tab.next": tabStep
      /^\s+"?([\w.]+)"?: (?:\(\) =>|tabStep)/gm,
      // FIXED keys: ["switcher", (e) => …]
      /^\s+\["([\w.]+)", \(e\) =>/gm,
    ];
    for (const text of Object.values(files)) {
      for (const re of patterns) for (const m of text.matchAll(re)) used.add(m[1]);
      // ⌘1~9: built as `ws.${n}`
      if (text.includes("`ws.${")) for (let n = 1; n <= 9; n++) used.add(`ws.${n}`);
    }
    const commandLike = [...used].filter((id) => id.includes(".") || ids.has(id));
    expect(commandLike.filter((id) => !ids.has(id) && !["toolwindow.hide", "switcher"].includes(id))).toEqual([]);
    expect(SHORTCUTS.map((s) => s.id).filter((id) => !used.has(id))).toEqual([]);
  });
});
