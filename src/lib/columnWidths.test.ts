import { describe, expect, it } from "vitest";
import { CELL_PAD, CHAR_PX, fitChars, saveError, stepWidth, toChars, toPx, widthKeys } from "./columnWidths";

describe("column widths (characters ↔ pixels, the shared step and range)", () => {
  it("turns characters into pixels and back at any grid font size", () => {
    expect(toPx(10, 1)).toBe(Math.round(10 * CHAR_PX + CELL_PAD));
    for (const em of [1, 14 / 12, 20 / 12]) for (const c of [3, 16, 53, 200]) expect(toChars(toPx(c, em), em)).toBe(c);
    // A drag lands on the nearest whole character
    expect(toChars(toPx(16, 1) + 3, 1)).toBe(16);
    expect(toChars(toPx(16, 1) + 4, 1)).toBe(17);
  });

  it("steps by 4 characters and stays between 3 and 200 (qurry_shared::format::column_width, the TUI's < >)", () => {
    expect(stepWidth(16, 3)).toBe(28);
    expect(stepWidth(28, -1)).toBe(24);
    expect(stepWidth(5, -1)).toBe(3);
    expect(stepWidth(198, 1)).toBe(200);
    expect(toChars(0, 1)).toBe(3);
    expect(toChars(1e6, 1)).toBe(200);
  });

  it("fits the longest text, or the header when that is wider", () => {
    expect(fitChars(60, ["gift wrap", "first order after the promotion; call before delivery"], 1)).toBe(53);
    expect(fitChars(12 * CHAR_PX + CELL_PAD, ["a", "bb"], 1)).toBe(12);
    expect(fitChars(12 * CHAR_PX + CELL_PAD + 1, [], 1)).toBe(13);
    expect(fitChars(60, ["x".repeat(5000)], 1)).toBe(200);
  });

  it("counts a wide character (Hangul, CJK, emoji) as 2, as the TUI draws it", () => {
    // 20 Hangul characters draw at about 10.4 px each at 12px: 40 characters (312 px of text) hold them, 20 (146 px) don't
    expect(fitChars(60, ["가".repeat(20)], 1)).toBe(40);
    expect(fitChars(60, ["고객 요청 🙏", "memo"], 1)).toBe(12);
    expect(toPx(fitChars(60, ["가".repeat(20)], 1), 1)).toBeGreaterThanOrEqual(Math.ceil(20 * 10.4) + CELL_PAD);
  });

  it("a name the result repeats (a join's two id columns) gets its own key, so each column resizes on its own", () => {
    expect(widthKeys(["id", "name", "id", "id"])).toEqual(["id", "name", "id#2", "id#3"]);
    expect(widthKeys(["id", "memo"])).toEqual(["id", "memo"]);
  });

  it("a failed save says the widths still apply and how to go back", () => {
    expect(saveError("permission denied")).toBe(
      "Couldn't save the column widths (permission denied). They apply until the tab closes; the column header's menu › Reset Widths goes back to the computed widths.",
    );
  });
});
