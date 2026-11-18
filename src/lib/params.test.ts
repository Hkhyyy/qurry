import { describe, expect, it } from "vitest";
import { findParams, substituteParams } from "./params";

describe("findParams", () => {
  it("finds `:name` in code with UTF-16 ranges, not casts, strings or comments", () => {
    const sql = "select '😀', x::int, :이름 -- :c\n where id = :id";
    expect(findParams(sql, "postgres").map((p) => [p.name, sql.slice(p.start, p.end)])).toEqual([
      ["이름", ":이름"],
      ["id", ":id"],
    ]);
  });

  it("finds none in a console that isn't SQL", () => {
    expect(findParams("GET :a", "redis")).toEqual([]);
  });
});

describe("substituteParams", () => {
  it("puts the values in as typed and keeps a name without one", () => {
    expect(substituteParams("select :a, :b, ':a'", "postgres", { a: "'x'" })).toBe("select 'x', :b, ':a'");
  });
});
