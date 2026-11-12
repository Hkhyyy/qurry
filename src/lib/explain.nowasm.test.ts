import { describe, expect, it, vi } from "vitest";

// The shared WASM module didn't load (or hasn't yet)
vi.mock("./shared", () => ({
  sharedOr: <T>(fallback: T) => fallback,
  shared: () => {
    throw new Error("not ready");
  },
}));
const { EXPLAIN_NOT_LOADED, explainSql } = await import("./explain");

describe("explainSql without the WASM module", () => {
  it("sends nothing and says why", () => {
    expect(explainSql("select 1", "postgres", false, true)).toEqual({ err: EXPLAIN_NOT_LOADED });
  });
});
