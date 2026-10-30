import { describe, expect, it, vi } from "vitest";
import { shared } from "./shared";

describe("shared wasm", () => {
  it("answers synchronously (initialized by the vitest setup file)", () => {
    expect(shared().ping("héllo 한글")).toEqual({ text: "héllo 한글", chars: 8 });
  });

  it("throws before init, saying how to fix it", async () => {
    vi.resetModules();
    const fresh = await import("./shared");
    expect(() => fresh.shared()).toThrow(/await initShared\(\).*pnpm wasm/);
  });
});
