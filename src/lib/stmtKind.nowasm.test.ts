import { describe, expect, it, vi } from "vitest";

// A WebView that refused WebAssembly: shared() throws, as it does before the module loaded or after it failed
vi.mock("./shared", () => ({
  shared: () => {
    throw new Error("Shared WASM is not ready");
  },
}));
const { unsafeReason } = await import("./stmtKind");

describe("unsafeReason without the WASM module", () => {
  it("asks for every statement, never takes one as safe", () => {
    for (const [kind, sql] of [["postgres", "select 1"], ["postgres", "delete from t"], ["mysql", "update t set a = 1 where id = 1"], ["redis", "GET a"]] as const)
      expect(unsafeReason(kind, sql), `${kind}: ${sql}`).toBeTruthy();
  });
});
