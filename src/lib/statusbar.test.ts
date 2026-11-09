import { describe, expect, it } from "vitest";
import { caretText, cellText, shownColumns } from "./statusbar";

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
