import { afterEach, describe, expect, it, vi } from "vitest";
import { ask, askText, askTextChoice, notice, registerTextDialog } from "./dialogs";

describe("dialogs", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    registerTextDialog(null);
  });
  it("waits for Tauri's async confirm too (a Promise is not taken as true)", async () => {
    vi.stubGlobal("window", { confirm: () => Promise.resolve(false) });
    expect(await ask("drop?")).toBe(false);
    vi.stubGlobal("window", { confirm: () => Promise.resolve(true) });
    expect(await ask("drop?")).toBe(true);
    vi.stubGlobal("window", { confirm: () => false });
    expect(await ask("drop?")).toBe(false);
  });
  it("a notice (e.g. a change that ran but couldn't be logged) waits until it is acknowledged", async () => {
    const seen: string[] = [];
    vi.stubGlobal("window", { confirm: (m: string) => (seen.push(m), Promise.resolve(false)) });
    await expect(notice("It ran, but the result couldn't be written", "Submitted, with a warning")).resolves.toBeUndefined();
    expect(seen).toEqual(["It ran, but the result couldn't be written"]);
  });
  it("text input goes to the registered in-app dialog (Tauri has no prompt)", async () => {
    registerTextDialog((r) => r.resolve(r.value + "!"));
    expect(await askText("Name", "a")).toBe("a!");
    registerTextDialog((r) => r.resolve(null));
    expect(await askText("Name")).toBeNull();
  });
  it("choose along with a name (snippet: this data source only / all) — not chosen with OK / Cancel", async () => {
    registerTextDialog((r) => {
      expect(r.choices).toEqual(["Only this data source", "All data sources"]);
      expect(r.choice).toBe(0);
      r.resolve("users", 1);
    });
    expect(await askTextChoice("Snippet name", "sel", ["Only this data source", "All data sources"])).toEqual({ text: "users", choice: 1 });
    registerTextDialog((r) => r.resolve(null));
    expect(await askTextChoice("Snippet name", "sel", ["a", "b"])).toBeNull();
  });
});
