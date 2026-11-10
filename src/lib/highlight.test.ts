import { describe, expect, it } from "vitest";
import { shared } from "./shared";

/** Each colored piece of `text`: [text, class] (names and symbols left out) */
const pieces = (text: string, kind: "postgres" | "mysql" = "postgres") =>
  shared()
    .highlight(text, kind)
    .filter((s) => s.class !== "other")
    .map((s) => [text.slice(s.start, s.end), s.class]);

// The table tab's WHERE / ORDER BY colors come from qurry_shared::highlight (the TUI's too); its own tests are in Rust
describe("highlight (WASM)", () => {
  it("gives UTF-16 ranges: text after an emoji or Korean lands on the right characters", () => {
    expect(pieces("name = '😀' AND 값 IN (1)")).toEqual([
      ["'😀'", "string"],
      ["AND", "keyword"],
      ["IN", "keyword"],
      ["1", "number"],
    ]);
  });
  it("reads the dialect: # is a comment and \"…\" a string in MySQL only", () => {
    expect(pieces(`a = "x" # c`, "mysql")).toEqual([
      ['"x"', "string"],
      ["# c", "comment"],
    ]);
    expect(pieces(`a = "x"`)).toEqual([]);
  });
});
