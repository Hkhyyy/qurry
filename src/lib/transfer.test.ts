import { describe, expect, it } from "vitest";
import { autoMap, progressText } from "./transfer";

describe("transfer", () => {
  it("maps file columns to columns of the same name (ignoring case, spaces and _), skips the rest", () => {
    expect(autoMap(["ID", "User Name", "extra"], ["id", "user_name", "created_at"])).toEqual(["id", "user_name", ""]);
  });
  it("a new table keeps the file's column names", () => {
    expect(autoMap(["a", "b"], null)).toEqual(["a", "b"]);
  });
  it("progress: % when the total is known, otherwise just the row count", () => {
    expect(progressText({ rows: 1234, done: 1234, total: 5000 })).toEqual({ pct: 24, text: "1,234 rows · 24%" });
    expect(progressText({ rows: 1234, done: 1234, total: 0 })).toEqual({ pct: null, text: "1,234 rows" });
    // Stops at 99% when the estimate is exceeded
    expect(progressText({ rows: 9000, done: 9000, total: 5000 }).pct).toBe(99);
    // Backup / restore: bytes, not rows
    expect(progressText({ rows: 0, done: 5 * 1048576, total: 0 })).toEqual({ pct: null, text: "5.0 MB" });
    expect(progressText({ rows: 0, done: 1048576, total: 4 * 1048576 })).toEqual({ pct: 25, text: "1.0 MB · 25%" });
  });
});
