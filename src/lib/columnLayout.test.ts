import { describe, expect, it } from "vitest";
import { columnView } from "./columnLayout";
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
