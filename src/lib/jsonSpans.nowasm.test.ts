import { describe, expect, it, vi } from "vitest";

// A WebView that refused WebAssembly: no column is a JSON one and a JSON cell has no colored pieces
vi.mock("./shared", () => ({ sharedOr: <T>(fallback: T) => fallback, sharedReady: () => false, shared: () => { throw new Error("not loaded"); } }));
const { isJsonType, jsonCell } = await import("./jsonSpans");

describe("JSON cells without the WASM module", () => {
  it("show plain text", () => {
    expect(isJsonType("jsonb")).toBe(false);
    expect(jsonCell('{"k":1}')).toEqual({ text: '{"k":1}', spans: [] });
  });
});
