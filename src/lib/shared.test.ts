import { describe, expect, it, vi } from "vitest";
import { shared } from "./shared";

describe("shared wasm", () => {
  it("answers synchronously (initialized by the vitest setup file)", () => {
    expect(shared().ping("héllo 한글")).toEqual({ text: "héllo 한글", chars: 8 });
  });

  it("a freed completion index finds nothing instead of throwing (a request may still hold it)", () => {
    const idx = shared().completionIndex([{ schema: "public", name: "users", columns: [["id", "int8"]] }]);
    const text = "select * from users where status = ";
    expect(idx.suggest("select * from u", 15, "", false).items.length).toBeGreaterThan(0);
    idx.free();
    expect(idx.suggest("select * from u", 15, "", false).items).toEqual([]);
    expect(idx.hintAt(text, text.length, "", false)).toBeUndefined();
    idx.free();
  });

  it("throws before init, saying how to fix it", async () => {
    vi.resetModules();
    const fresh = await import("./shared");
    expect(() => fresh.shared()).toThrow(/not ready.*pnpm wasm/);
  });
});
