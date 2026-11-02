import { describe, expect, it, vi } from "vitest";

// A WebView that refused WebAssembly: every shared helper gets its fallback
vi.mock("./shared", () => ({ sharedOr: <T>(fallback: T) => fallback }));
const { dialectOf } = await import("./api");

describe("Dialect.lit without the WASM module", () => {
  it("writes what qurry_shared::sql::literal writes (inList.test.ts checks that one through WASM)", () => {
    const v = "x\\') OR 1=1 -- ";
    expect(dialectOf("postgres").lit(v)).toBe("E'x\\\\'') OR 1=1 -- '");
    expect(dialectOf("postgres").lit("it's")).toBe("'it''s'");
    expect(dialectOf("mysql").lit(v)).toBe("'x\\\\'') OR 1=1 -- '");
    expect(dialectOf("clickhouse").lit(v)).toBe("'x\\\\'') OR 1=1 -- '");
    expect(dialectOf("mssql").lit(v)).toBe("'x\\'') OR 1=1 -- '");
    expect(dialectOf("postgres").lit(null)).toBe("NULL");
  });
});
