import { describe, expect, it } from "vitest";
import { contains, copyTable, extend, forEachCell, isSingle, selCols, selRows, single, toggle, type Selection } from "./gridSelection";

const cells = (sel: Selection | null) => {
  const out: string[] = [];
  forEachCell(sel, (r, c) => out.push(`${r},${c}`));
  return out;
};

describe("grid selection", () => {
  it("a plain click starts over with one cell", () => {
    const sel = single({ r: 2, c: 1 });
    expect(isSingle(sel)).toBe(true);
    expect(cells(sel)).toEqual(["2,1"]);
    expect(sel.cur).toEqual({ r: 2, c: 1 });
  });

  it("⌘-click adds cells and takes a selected one out again", () => {
    let sel = toggle(single({ r: 0, c: 0 }), { r: 4, c: 0 });
    expect(cells(sel)).toEqual(["0,0", "4,0"]);
    expect(isSingle(sel)).toBe(false);
    expect(sel.cur).toEqual({ r: 4, c: 0 });
    sel = toggle(sel, { r: 0, c: 0 });
    expect(cells(sel)).toEqual(["4,0"]);
    // the last clicked cell stays the active one, even when it was just taken out
    expect(sel.cur).toEqual({ r: 0, c: 0 });
    expect(toggle(null, { r: 1, c: 1 })).toEqual(single({ r: 1, c: 1 }));
  });

  it("⌘-click inside a range cuts the cell out of it", () => {
    const sel = toggle(single({ r: 0, c: 0 }, { r: 2, c: 2 }), { r: 1, c: 1 });
    expect(cells(sel)).toEqual(["0,0", "0,1", "0,2", "1,0", "1,2", "2,0", "2,1", "2,2"]);
  });

  it("⌘-click on a row number or a header adds a whole row or column", () => {
    let sel = toggle(single({ r: 0, c: 0 }), { r: 3, c: 0 }, { r: 3, c: 2 });
    expect(selRows(sel)).toEqual([0, 3]);
    sel = toggle(sel, { r: 0, c: 4 }, { r: 5, c: 4 });
    expect(selCols(sel)).toEqual([0, 1, 2, 4]);
    expect(contains(sel, 5, 4)).toBe(true);
    expect(contains(sel, 5, 3)).toBe(false);
    // the same row again: every cell of it is selected, so it goes
    sel = toggle(sel, { r: 3, c: 0 }, { r: 3, c: 2 });
    expect(contains(sel, 3, 0)).toBe(false);
    expect(contains(sel, 3, 4)).toBe(true);
  });

  it("⇧-click extends the last range from its anchor and keeps the others", () => {
    let sel = toggle(single({ r: 0, c: 0 }), { r: 4, c: 1 });
    sel = extend(sel, { r: 6, c: 2 });
    expect(sel.ranges).toEqual([{ r0: 0, r1: 0, c0: 0, c1: 0 }, { r0: 4, r1: 6, c0: 1, c1: 2 }]);
    sel = extend(sel, { r: 5, c: 1 });
    expect(sel.ranges).toEqual([{ r0: 0, r1: 0, c0: 0, c1: 0 }, { r0: 4, r1: 5, c0: 1, c1: 1 }]);
    expect(sel.cur).toEqual({ r: 5, c: 1 });
    expect(extend(null, { r: 2, c: 2 })).toEqual(single({ r: 2, c: 2 }));
  });

  it("⇧-click after a cell was taken out starts a new range there instead of reshaping another", () => {
    let sel = toggle(single({ r: 0, c: 0 }), { r: 2, c: 0 });
    sel = toggle(sel, { r: 2, c: 0 });
    sel = extend(sel, { r: 3, c: 0 });
    expect(sel.ranges).toEqual([{ r0: 0, r1: 0, c0: 0, c1: 0 }, { r0: 2, r1: 3, c0: 0, c1: 0 }]);
  });

  it("copies rows and columns in grid order, whatever order they were clicked in", () => {
    let sel = single({ r: 4, c: 0 }, { r: 4, c: 1 });
    sel = toggle(sel, { r: 0, c: 0 }, { r: 0, c: 1 });
    const t = copyTable(sel, (r, c) => `${r}:${c}`);
    expect(t.rows).toEqual([0, 4]);
    expect(t.cols).toEqual([0, 1]);
    expect(t.vals).toEqual([["0:0", "0:1"], ["4:0", "4:1"]]);
  });

  it("ranges with different columns copy as the union of columns, cells outside every range empty", () => {
    let sel = single({ r: 0, c: 2 });
    sel = toggle(sel, { r: 1, c: 0 });
    sel = toggle(sel, { r: 1, c: 3 });
    const t = copyTable(sel, (r, c) => `${r}:${c}`);
    expect(t.cols).toEqual([0, 2, 3]);
    expect(t.vals).toEqual([[null, "0:2", null], ["1:0", null, "1:3"]]);
  });

  it("overlapping ranges list each cell once", () => {
    const sel = toggle(single({ r: 0, c: 0 }, { r: 1, c: 1 }), { r: 1, c: 1 }, { r: 2, c: 2 });
    expect(cells(sel)).toEqual(["0,0", "0,1", "1,0", "1,1", "1,2", "2,1", "2,2"]);
  });
});
