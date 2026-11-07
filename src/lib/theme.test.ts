import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const setTheme = vi.fn(async (_t: "light" | "dark" | null) => {});
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme }) }));
// The shared settings (settings.json through the core)
const backend = {
  themeSettings: vi.fn(async () => ({ theme: null as string | null, sync: { dark: "dark", light: "light" }, custom: [] as unknown[] })),
  setTheme: vi.fn(async (_id: string) => {}),
  setThemeSync: vi.fn(async () => {}),
};
vi.mock("./api", () => ({ api: backend }));

let theme: typeof import("./theme");
const storage = new Map<string, string>();
const style = new Map<string, string>();
const root = { dataset: {} as Record<string, string>, style: { setProperty: (k: string, v: string) => style.set(k, v) } };
beforeAll(async () => {
  // OS in light mode: the native title bar is white unless the window theme is set
  vi.stubGlobal("window", { matchMedia: () => ({ matches: false, addEventListener: () => {} }) });
  vi.stubGlobal("document", { documentElement: root });
  vi.stubGlobal("localStorage", { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => storage.set(k, v), removeItem: (k: string) => storage.delete(k) });
  theme = await import("./theme");
});
beforeEach(() => {
  storage.clear();
  backend.setTheme.mockClear();
});

const last = async () => {
  await vi.waitFor(() => expect(setTheme).toHaveBeenCalled());
  return setTheme.mock.lastCall?.[0];
};

describe("window theme (macOS title bar)", () => {
  it("follows the app theme, not the OS, when a theme is picked", async () => {
    for (const [pick, want] of [["dark", "dark"], ["hc", "dark"], ["dracula", "dark"], ["light", "light"], ["solarized-light", "light"]] as const) {
      setTheme.mockClear();
      await theme.setThemePick(pick);
      expect(await last()).toBe(want);
      expect(backend.setTheme).toHaveBeenLastCalledWith(pick);
    }
  });
  it("is left to the OS when syncing with it", async () => {
    setTheme.mockClear();
    await theme.setThemePick("system");
    expect(await last()).toBeNull();
  });
});

describe("themes", () => {
  it("applies every token's variables, the kind, and the derived styles for themes other than the three built in before", async () => {
    await theme.setThemePick("solarized-light");
    expect(root.dataset.theme).toBe("light");
    expect("derived" in root.dataset).toBe(true);
    const sol = theme.BUILT_IN.find((t) => t.id === "solarized-light")!;
    expect(style.get("--bg")).toBe(sol.colors.bg);
    expect(style.get("--grid-changed")).toBe(sol.colors.changed);
    expect(style.get("--theme-prod-stripe")).toBe(sol.colors.prodStripe);
    await theme.setThemePick("hc");
    expect(root.dataset.theme).toBe("hc");
    expect("derived" in root.dataset).toBe(false);
    // Every token of themes.json has its variables (the grid states and tints among them)
    for (const t of theme.TOKENS) expect(theme.TOKEN_VARS[t.key], t.key).toBeDefined();
    expect(Object.keys(theme.TOKEN_VARS).sort()).toEqual(theme.TOKENS.map((t) => t.key).sort());
  });

  it("resolves custom themes over their base, Sync with OS by the appearance, and a deleted pick to Dark", () => {
    const s = { theme: "custom-1", sync: { dark: "one-dark", light: "custom-1" }, custom: [{ id: "custom-1", name: "Paper", base: "solarized-light", colors: { text: "#3d4f55", nope: "#000000" } }] };
    const t = theme.resolveTheme(s, true);
    expect([t.id, t.kind, t.custom, t.colors.text, t.colors.bg, "nope" in t.colors]).toEqual(["custom-1", "light", true, "#3d4f55", "#fdf6e3", false]);
    expect(theme.resolveTheme({ ...s, theme: "system" }, true).id).toBe("one-dark");
    expect(theme.resolveTheme({ ...s, theme: "system" }, false).id).toBe("custom-1");
    expect(theme.resolveTheme({ ...s, theme: "custom-9" }, true).id).toBe("dark");
    expect(theme.resolveTheme({ ...s, theme: null }, false).id).toBe("dark");
    expect(theme.allThemes(s).map((x) => x.id)).toEqual(["dark", "light", "hc", "dracula", "one-dark", "solarized-dark", "solarized-light", "custom-1"]);
  });

  // #103: the pgsql tokenizer emits string.sql / predefined.sql, which vs and vs-dark color #FF0000 / #FF00FF over a plain "string" rule
  it("gives every Monaco theme, built-in or custom, string.sql and predefined.sql rules in the theme's colors", () => {
    const custom = theme.findTheme({ theme: null, sync: { dark: "dark", light: "light" }, custom: [{ id: "custom-1", name: "Mine", base: "dark", colors: { string: "#123456" } }] }, "custom-1")!;
    for (const t of [...theme.BUILT_IN, custom]) {
      const data = theme.monacoThemeData(t);
      const rule = (tok: string) => data.rules.find((r) => r.token === tok)?.foreground;
      expect(rule("string.sql"), t.id).toBe(t.colors.string.slice(1));
      expect(rule("predefined.sql"), t.id).toBe(t.colors.function.slice(1));
      expect(rule("string"), t.id).toBe(t.colors.string.slice(1));
      expect(rule("string.value.json"), t.id).toBe(t.colors.string.slice(1));
      expect(data.colors["editor.background"]).toBe(t.colors.bg);
      expect(data.base).toBe(t.kind === "light" ? "vs" : t.kind === "hc" ? "hc-black" : "vs-dark");
    }
    expect(theme.monacoThemeData(custom).rules.find((r) => r.token === "string.sql")?.foreground).toBe("123456");
  });

  it("gives Red and Green Data Sources the theme's tints, the other colors a mix", () => {
    expect(theme.connBg("#6E2A2A")).toBe("var(--theme-prod-tint)");
    expect(theme.connStripe("#6e2a2a")).toBe("var(--theme-prod-stripe)");
    expect(theme.connBg("#2f5634")).toBe("var(--theme-tint)");
    expect(theme.connBg("#28406e")).toContain("var(--conn-tint-amount)");
    expect(theme.connBg(null)).toBeUndefined();
  });
});

describe("the pick the GUI kept before 0.3.2 (webview storage)", () => {
  it("moves to the shared settings once, unless they have one", async () => {
    storage.set("dbtool.theme", "light");
    await theme.initTheme();
    expect(backend.setTheme).toHaveBeenCalledWith("light");
    expect(storage.has("dbtool.theme")).toBe(false);
    expect(theme.currentTheme().id).toBe("light");
    expect(theme.takeThemeNotice()).toBeNull();
    // The TUI (or this app elsewhere) picked one already: it stays
    backend.setTheme.mockClear();
    storage.set("dbtool.theme", "hc");
    backend.themeSettings.mockResolvedValueOnce({ theme: "dracula", sync: { dark: "dark", light: "light" }, custom: [] });
    await theme.initTheme();
    expect(backend.setTheme).not.toHaveBeenCalled();
    expect(theme.currentTheme().id).toBe("dracula");
  });

  it("is used for now and says how to keep it when it can't be saved", async () => {
    storage.set("dbtool.theme", "system");
    backend.setTheme.mockRejectedValueOnce("permission denied");
    await theme.initTheme();
    expect(theme.getThemeSettings().theme).toBe("system");
    expect(storage.get("dbtool.theme")).toBe("system");
    expect(theme.takeThemeNotice()).toMatch(/permission denied.*Pick it again in Settings › Appearance/);
    expect(theme.takeThemeNotice()).toBeNull();
  });
});
