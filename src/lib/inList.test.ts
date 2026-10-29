import { describe, expect, it } from "vitest";
import { inList, isNumericType } from "./inList";

describe("inList", () => {
  it("one column: quotes, commas and parentheses; duplicates and NULL are dropped", () => {
    expect(inList([["1.2.3.4"], ["5.6.7.8"], ["1.2.3.4"], [null], ["it's"]])).toBe("('1.2.3.4', '5.6.7.8', 'it''s')");
  });
  it("numbers without quotes", () => {
    expect(inList([[1], [2], [2]])).toBe("(1, 2)");
  });
  it("several columns: one tuple per row ((a, b), …) — WHERE (x, y) IN …", () => {
    expect(inList([["a", 1], ["b", 2]])).toBe("(('a', 1), ('b', 2))");
  });
  it("numeric columns are decided by type: numeric / bigint arriving as text stay unquoted (value as is)", () => {
    expect(inList([["1234.50"], ["12345678901234567890"]], ["numeric"])).toBe("(1234.50, 12345678901234567890)");
    expect(inList([["1"]], ["int8"])).toBe("(1)");
    expect(inList([["NaN"]], ["numeric"])).toBe("('NaN')");
    // Quoted if it's a text column even if it looks numeric (phone numbers 010…, leading 0 in postal codes)
    expect(inList([["010"]], ["varchar"])).toBe("('010')");
    expect(isNumericType("interval")).toBe(false);
    expect(isNumericType("BIGINT UNSIGNED")).toBe(true);
    expect(isNumericType("double precision")).toBe(true);
  });
  it("NULL instead of an empty list when there are no values", () => {
    expect(inList([[null]])).toBe("(NULL)");
  });
});
