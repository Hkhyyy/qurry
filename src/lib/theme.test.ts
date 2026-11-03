import { beforeAll, describe, expect, it, vi } from "vitest";

const setTheme = vi.fn(async (_t: "light" | "dark" | null) => {});
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme }) }));

let theme: typeof import("./theme");
beforeAll(async () => {
  // OS in light mode: the native title bar is white unless the window theme is set
  vi.stubGlobal("window", { matchMedia: () => ({ matches: false, addEventListener: () => {} }) });
  vi.stubGlobal("document", { documentElement: { dataset: {} } });
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {} });
  theme = await import("./theme");
});

const last = async () => {
  await vi.waitFor(() => expect(setTheme).toHaveBeenCalled());
  return setTheme.mock.lastCall?.[0];
};

describe("window theme (macOS title bar)", () => {
  it("follows the app theme, not the OS, when a theme is picked", async () => {
    for (const [pref, want] of [["dark", "dark"], ["hc", "dark"], ["light", "light"]] as const) {
      setTheme.mockClear();
      theme.setThemePref(pref);
      expect(await last()).toBe(want);
    }
  });
  it("is left to the OS when syncing with it", async () => {
    setTheme.mockClear();
    theme.setThemePref("system");
    expect(await last()).toBeNull();
  });
});
