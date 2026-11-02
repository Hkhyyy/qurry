import { describe, expect, it } from "vitest";
import { dialectOf } from "./api";
import { extractRows } from "./extract";
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
  it("MySQL and ClickHouse also double \\: a value can't end the string early", () => {
    const evil = [["x\\') OR 1=1 -- "]];
    expect(inList(evil, [], "mysql")).toBe("('x\\\\'') OR 1=1 -- ')");
    const ch = { names: ["s"], types: ["String"], vals: evil, keys: [], tableRef: "t", header: false };
    expect(extractRows("SQL IN List", { ...ch, dialect: dialectOf("clickhouse") })).toBe("('x\\\\'') OR 1=1 -- ')");
    expect(extractRows("SQL Inserts", { ...ch, dialect: dialectOf("clickhouse") })).toBe("INSERT INTO t (s) VALUES ('x\\\\'') OR 1=1 -- ');");
    expect(dialectOf("clickhouse").lit("a\\'b")).toBe("'a\\\\''b'");
    expect(inList(evil)).toBe("('x\\'') OR 1=1 -- ')");
  });
  it("numbers as the grid shows them (String())", () => {
    const nums = [2 ** 53, 1e16, 1e20, 2 ** 60, 1e21, 1.5e-7, 0.1 + 0.2, -0];
    expect(inList(nums.map((n) => [n]))).toBe(`(${nums.map(String).join(", ")})`);
    const x = { names: ["v"], types: ["float8"], vals: nums.map((n) => [n]), keys: [], tableRef: "t", header: false, dialect: dialectOf("postgres") };
    expect(extractRows("TSV", x)).toBe(nums.map(String).join("\n"));
    expect(extractRows("JSON", x)).toBe(JSON.stringify(nums.map((v) => ({ v })), null, 2));
  });
});
