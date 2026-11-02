import { describe, expect, it, vi } from "vitest";

// A WebView that refused WebAssembly: the module never becomes ready
vi.mock("./shared", () => ({
  sharedReady: () => false,
  shared: () => {
    throw new Error("not ready");
  },
}));
const { fuzzyScore } = await import("./fuzzy");

describe("fuzzyScore without the WASM module", () => {
  it("search popups still match by case-insensitive substring", () => {
    expect(fuzzyScore("PAY", "user_payment")).toBe(0);
    expect(fuzzyScore("", "users")).toBe(0);
    expect(fuzzyScore("upy", "user_payment")).toBeNull();
  });
});
