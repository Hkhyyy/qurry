import { describe, expect, it, vi } from "vitest";

// The shared WASM module didn't load (or hasn't yet)
vi.mock("./shared", () => ({
  sharedOr: <T>(fallback: T) => fallback,
  shared: () => {
    throw new Error("not ready");
  },
}));
const { columnView } = await import("./columnLayout");

describe("columnView without the WASM module", () => {
  it("shows every column in result order, whatever the layout", () => {
    expect(columnView(["id", "name", "memo"], { hidden: ["name"], pinned: ["memo"] })).toEqual({ view: [0, 1, 2], pinned: 0, hidden: 0 });
    expect(columnView([], { hidden: [], pinned: [] })).toEqual({ view: [], pinned: 0, hidden: 0 });
  });
});
