import { describe, expect, it } from "vitest";
import { noticeLines } from "./notices";

describe("noticeLines", () => {
  it("writes each notice with its level, warnings in the warn style, the cap's line as it is", () => {
    expect(noticeLines([{ level: "NOTICE", message: "a" }, { level: "WARNING", message: "b" }, { level: "INFO", message: "c" }, { level: "", message: "… 2 more" }])).toEqual([
      { level: "info", text: "NOTICE: a" },
      { level: "warn", text: "WARNING: b" },
      { level: "info", text: "INFO: c" },
      { level: "info", text: "… 2 more" },
    ]);
  });
  it("has no lines without notices", () => {
    expect(noticeLines(undefined)).toEqual([]);
  });
});
