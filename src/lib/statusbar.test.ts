import { describe, expect, it } from "vitest";
import { shared } from "./shared";
import { caretText, cellText, shownColumns, spanOf } from "./statusbar";

describe("status bar widgets", () => {
  it("says the caret, the selection's length and a grid cell in words", () => {
    expect(caretText(2, 11, 0)).toBe("Ln 2, Col 11");
    expect(caretText(2, 11, 8)).toBe("Ln 2, Col 11 (8 chars)");
    expect(cellText(3, 5)).toBe("Row 3, Col 5");
  });

  // Columns of 100 px after a 40 px row-number gutter: 0 at 40–140, 1 at 140–240, …
  const cols = Array.from({ length: 14 }, (_, i) => ({ start: 40 + i * 100, end: 140 + i * 100 }));

  it("counts the columns wholly in view", () => {
    expect(shownColumns(cols, 40, 640)).toEqual([0, 5]);
    // Scrolled 250 px: the data area is 290–890, column 2 (240–340) is cut at the left, column 8 (840–940) at the right
    expect(shownColumns(cols, 290, 890)).toEqual([3, 7]);
    expect(shownColumns(cols, 1040, 1640)).toEqual([10, 13]);
  });

  it("falls back to the columns cut at the edges when none is whole, and has none for an empty view", () => {
    expect(shownColumns(cols, 60, 120)).toEqual([0, 0]);
    expect(shownColumns(cols, 60, 200)).toEqual([0, 1]);
    expect(shownColumns([], 0, 100)).toBeNull();
  });
});

describe("status bar items (qurry_shared::statusbar through WebAssembly, the TUI's words)", () => {
  it("spans rows and columns 1-based", () => {
    expect(spanOf([4, 5, 6])).toEqual({ first: 5, last: 7, count: 3 });
    expect(spanOf([0, 9])).toEqual({ first: 1, last: 10, count: 2 });
  });

  it("words Position and the selection's items as separate segments, without Count", () => {
    const stats = shared().selectionStats([null, 5, "7.5", "abc", true, null, -2]);
    expect(stats).toEqual({ notNull: 5, nums: 3, sum: 10.5, min: -2, max: 7.5 });
    const segs = shared().statusSegments(["notNull", "sum", "avg", "min", "max", "shownColumns", "position"], { pos: [spanOf([4, 5, 6]), spanOf([3])], stats, shown: [3, 8, 14] });
    expect(segs.map((s) => [s.label ?? null, s.value])).toEqual([
      ["Not null", "5"],
      ["Sum", "10.5"],
      ["Avg", "3.5"],
      ["Min", "-2"],
      ["Max", "7.5"],
      [null, "cols 3–8 of 14"],
      [null, "Rows 5–7 (3) · Col 4"],
    ]);
  });

  it("words Size and its column's limit, bytes only without one, nothing for a number", () => {
    const cell = (value: string | number, typeName: string, column?: { dataType: string; charset?: string }) => shared().statusSegments(["size"], { kind: "mysql", cell: { typeName, value, column } });
    expect(cell("Grüße ☕", "VARCHAR", { dataType: "varchar(8)", charset: "utf8mb4" })).toEqual([
      { id: "size", label: "Size", value: "11 bytes (7 chars)" },
      { id: "limit", label: "varchar(8)", value: "up to 8 chars (32 bytes in utf8mb4)" },
    ]);
    expect(cell("ab", "VARCHAR")).toEqual([{ id: "size", label: "Size", value: "2 bytes" }]);
    expect(cell(18, "INT")).toEqual([]);
    expect(cell("true", "BOOL")).toEqual([]);
  });

  it("knows the items, their defaults and the order they go when narrow", () => {
    const info = shared().statusBarInfo();
    expect(info.items.map((i) => i.id)).toEqual(["notNull", "sum", "avg", "min", "max", "size", "shownColumns", "position"]);
    expect(info.defaults).toEqual(["shownColumns", "position"]);
    expect(info.drop).toEqual(["limit", "max", "min", "avg", "sum", "notNull", "size", "shownColumns"]);
    expect(info.maxCells).toBe(200000);
  });
});
