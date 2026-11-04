import { describe, expect, it } from "vitest";
import { markHover } from "./stmtMark";

describe("markHover", () => {
  it("says when it ran and its result, or since when it runs", () => {
    expect(markHover({ state: "ok", out: "a", ts: "2026-11-04 10:00:00", text: "3 rows affected in 7 ms" })).toBe("Executed 2026-11-04 10:00:00  \n3 rows affected in 7 ms");
    expect(markHover({ state: "running", out: "a", ts: "2026-11-04 10:00:00" })).toBe("Running since 2026-11-04 10:00:00");
  });
  it("shows an error as plain text, not Markdown", () => {
    expect(markHover({ state: "error", out: "a", ts: "t", text: 'ERROR: relation "a_b" does not exist <x>' })).toBe('Failed t  \nERROR: relation "a\\_b" does not exist \\<x\\>');
  });
});
