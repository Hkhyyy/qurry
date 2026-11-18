import { afterEach, describe, expect, it, vi } from "vitest";
import { ask, askParams, askText, askTextChoice, notice, registerParamsDialog, registerTextDialog } from "./dialogs";

describe("dialogs", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    registerTextDialog(null);
    registerParamsDialog(null);
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
  it("parameters: the registered dialog gets the names and last values; without it one prompt per name, and a cancel cancels all", async () => {
    registerParamsDialog((r) => {
      expect([r.names, r.last, r.sql, r.kind]).toEqual([["id", "s"], { id: "42" }, "select :id, :s", "postgres"]);
      r.resolve({ id: "1", s: "'a'" });
    });
    expect(await askParams(["id", "s"], { id: "42" }, "select :id, :s", "postgres")).toEqual({ id: "1", s: "'a'" });
    registerParamsDialog(null);
    const asked: string[] = [];
    vi.stubGlobal("window", { prompt: (m: string, v: string) => (asked.push(`${m}=${v}`), v || "7") });
    expect(await askParams(["id", "s"], { id: "42" }, "", "postgres")).toEqual({ id: "42", s: "7" });
    expect(asked).toEqual([":id=42", ":s="]);
    vi.stubGlobal("window", { prompt: () => null });
    expect(await askParams(["id"], {}, "", "postgres")).toBeNull();
  });
});
