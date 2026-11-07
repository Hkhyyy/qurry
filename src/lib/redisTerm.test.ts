import { describe, expect, it } from "vitest";
import { replyTable } from "./redisTerm";

describe("replyTable", () => {
  it("shows a hash as field / value and a sorted set with scores as member / score", () => {
    expect(replyTable("pairs", ["name", "kim", "tier", "gold"])).toEqual({ heads: ["field", "value"], rows: [["name", "kim"], ["tier", "gold"]] });
    expect(replyTable("scored", ["alice", "10", "bob", "20"])).toEqual({ heads: ["member", "score"], rows: [["alice", "10"], ["bob", "20"]] });
  });
  it("numbers list elements from 0 and lists set members one per row", () => {
    expect(replyTable("list", ["a", null])).toEqual({ heads: ["index", "value"], rows: [["0", "a"], ["1", "(nil)"]] });
    expect(replyTable("members", ["x", "y"])?.rows).toEqual([["x"], ["y"]]);
  });
  it("prints the rest as redis-cli does", () => {
    expect(replyTable("plain", ["a"])).toBeNull();
    expect(replyTable("pairs", ["odd"])).toBeNull();
    expect(replyTable("members", [])).toBeNull();
    expect(replyTable("list", [["nested"]])).toBeNull();
    expect(replyTable("pairs", "OK")).toBeNull();
  });
});
