import { describe, expect, it, vi } from "vitest";

// The shared WASM module didn't load (or hasn't yet)
vi.mock("./shared", () => ({ sharedOr: <T>(fallback: T) => fallback }));
const { findParams, substituteParams } = await import("./params");

describe("params without the WASM module", () => {
  it("finds no parameters, so the console runs its text as before", () => {
    expect(findParams("select :a", "postgres")).toEqual([]);
  });

  it("substitutes nothing, so casts, strings and longer names stay", () => {
    const sql = "select x::a, ':a', :ab";
    expect(substituteParams(sql, "postgres", { a: "1" })).toBe(sql);
  });
});
