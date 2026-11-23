import { describe, expect, it } from "vitest";
import { columnsLabel, columnView, headerColumns, hideKeys, pinKeys, unpinKeys } from "./columnLayout";
import { widthKeys } from "./columnWidths";

const none = { hidden: [], pinned: [] };

describe("columnView (qurry_shared::format::column_view, the TUI's order)", () => {
  it("shows every column in result order by default", () => {
    expect(columnView(["id", "name", "memo"], none)).toEqual({ view: [0, 1, 2], pinned: 0, hidden: 0 });
  });

  it("leaves hidden columns out and puts pinned ones first, in pinned order", () => {
    expect(columnView(["id", "name", "memo", "total"], { hidden: ["name"], pinned: ["total", "memo"] })).toEqual({ view: [3, 2, 0], pinned: 2, hidden: 1 });
  });

  it("ignores a renamed or dropped column, and a column both hidden and pinned is hidden", () => {
    expect(columnView(["id", "name"], { hidden: ["gone", "name"], pinned: ["renamed", "name"] })).toEqual({ view: [0], pinned: 0, hidden: 1 });
  });

  it("keys a repeated name by occurrence (a join's two id columns)", () => {
    expect(columnView(widthKeys(["id", "name", "id"]), { hidden: ["id"], pinned: ["id#2"] })).toEqual({ view: [2, 1], pinned: 1, hidden: 1 });
  });
});

describe("the header menu on several selected columns (#216)", () => {
  it("acts on every selected column when the clicked one is among them, else on the clicked one alone", () => {
    expect(headerColumns(3, [2, 3, 5], 0)).toEqual({ cols: [2, 3, 5], pin: [2, 3, 5], unpin: [] });
    expect(headerColumns(4, [2, 3, 5], 0)).toEqual({ cols: [4], pin: [4], unpin: [] });
    expect(headerColumns(1, [], 0).cols).toEqual([1]);
  });

  it("splits a mixed selection into the columns to pin and the pinned ones to unpin", () => {
    expect(headerColumns(0, [0, 1, 2, 3], 2)).toEqual({ cols: [0, 1, 2, 3], pin: [2, 3], unpin: [0, 1] });
  });

  it("says the count for several columns", () => {
    expect(columnsLabel("Pin", 1)).toBe("Pin Column");
    expect(columnsLabel("Hide", 3)).toBe("Hide 3 Columns");
  });

  it("pins after the pinned ones in the order given, keeping the pinned ones where they are; unpins and hides several", () => {
    const l = { hidden: ["x"], pinned: ["memo"] };
    expect(pinKeys(l, ["name", "memo", "id"])).toEqual({ hidden: ["x"], pinned: ["memo", "name", "id"] });
    expect(unpinKeys({ hidden: [], pinned: ["memo", "name", "id"] }, ["id", "memo"])).toEqual({ hidden: [], pinned: ["name"] });
    expect(hideKeys(l, ["x", "name"])).toEqual({ hidden: ["x", "name"], pinned: ["memo"] });
  });
});
