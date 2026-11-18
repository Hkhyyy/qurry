import { describe, expect, it, vi } from "vitest";

// The shared WASM module didn't load (or hasn't yet)
vi.mock("./shared", () => ({ sharedOr: <T>(fallback: T) => fallback }));
const { findParams, substituteParams } = await import("./params");

describe("params without the WASM module", () => {
  it("finds no parameters, so the console runs its text as before", () => {
    expect(findParams("select :a", "postgres")).toEqual([]);
  });

  it("substitutes as plain text, longer names first", () => {
    expect(substituteParams("select :a, :ab, :c", "postgres", { a: "1", ab: "2" })).toBe("select 1, 2, :c");
  });
});
