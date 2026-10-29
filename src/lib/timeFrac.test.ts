import { describe, expect, it } from "vitest";
import { fracWidth, isTimeType, padFrac } from "./timeFrac";

describe("timeFrac", () => {
  const col = ["2026-06-06 03:19:15 +09:00", "2026-06-08 09:38:06.521101 +09:00", null, "2026-06-08 10:52:05.27 +09:00"];
  it("pads to the longest fraction in the column", () => {
    const w = fracWidth(col);
    expect(w).toBe(6);
    expect(col.map((v) => (typeof v === "string" ? padFrac(v, w) : v))).toEqual([
      "2026-06-06 03:19:15.000000 +09:00",
      "2026-06-08 09:38:06.521101 +09:00",
      null,
      "2026-06-08 10:52:05.270000 +09:00",
    ]);
  });
  it("leaves whole-second columns and other text alone", () => {
    expect(fracWidth(["2026-06-06 03:19:15", "2026-06-06 03:19:16"])).toBe(0);
    expect(padFrac("<null>", 6)).toBe("<null>");
    expect(padFrac("12:00:01", 3)).toBe("12:00:01.000");
  });
  it("knows time types", () => {
    expect(["TIMESTAMPTZ", "TIMESTAMP", "TIME", "DATETIME", "TIMETZ"].every(isTimeType)).toBe(true);
    expect(["DATE", "INTERVAL", "TEXT"].some(isTimeType)).toBe(false);
  });
});
