import { describe, expect, it } from "vitest";
import type { Cell, QueryResult, ResultEditTarget } from "../../lib/api";
import { editStatements, type Changes } from "../../lib/gridEdit";
import { appendRows, applyFull, cutSet, fullRequest, keyColumns, keyed, ofRun, sameRun } from "./shared";
import { changeCell, refusesCut } from "./useGridEdits";

// Cut values (Settings › Long values): a cell holding only the start of its value is never written back as it is
const to: ResultEditTarget = { schema: "public", table: "cut_demo", pk: ["id"], columns: ["id", "note"] };
const CUT = "long note long no";
const rows = (): Cell[][] => [
  [1, "short"],
  [2, CUT],
];
const res = (): QueryResult => ({ columns: [{ name: "id", typeName: "INT8" }, { name: "note", typeName: "TEXT" }], rows: rows(), rowsAffected: 0, truncated: false, elapsedMs: 1, hasResultSet: true, cut: [[1, 1]] });

/** What the grid's setCell does with one value: refused on a cut cell unless it isn't made from the cell's text */
const set = (ch: Changes, cut: Set<string>, r: number, c: number, next: Cell, whole = false) => (refusesCut(cut, ch, r, c, whole) ? ch : changeCell(ch, rows(), r, c, next));

describe("a change on a cut cell", () => {
  const cut = cutSet([[1, 1]]);

  it("made from its text is refused and never reaches editStatements", () => {
    const ch = set({}, cut, 1, 1, CUT + " edited");
    expect(ch).toEqual({});
    expect(editStatements("postgres", to, rows(), ch, new Set(), [])).toEqual([]);
  });

  it("that replaces it (paste, NULL, DEFAULT) is taken; its UPDATE has only that value", () => {
    const ch = set({}, cut, 1, 1, "pasted", true);
    expect(editStatements("postgres", to, rows(), ch, new Set(), [])).toEqual(["UPDATE public.cut_demo SET note = 'pasted' WHERE id = '2';"]);
    // Changed once, the cell holds the change, not the cut text: editing it again is fine
    expect(refusesCut(cut, ch, 1, 1, false)).toBe(false);
  });

  it("cells that aren't cut take edits as before", () => {
    expect(set({}, cut, 0, 1, "x")).toEqual({ 0: { 1: "x" } });
    expect(set({ 0: { 1: "x" } }, cut, 0, 1, "short")).toEqual({});
  });
});

describe("cut cells and their rows", () => {
  it("a load-more's cut cells move down by the rows already loaded, and it stays the same run", () => {
    const x = res();
    const more: QueryResult = { ...res(), rows: [[3, "a"], [4, CUT], [5, CUT]], cut: [[1, 1], [2, 1]] };
    const y = appendRows(x, more);
    expect(y.cut).toEqual([[1, 1], [3, 1], [4, 1]]);
    expect(y.rows).toHaveLength(5);
    expect(sameRun(y, x)).toBe(true);
    expect(sameRun(res(), x)).toBe(false);
  });

  it("a patch of a result through ofRun (a failed load-more's error) stays the same run, so a read in flight still lands", () => {
    const x = appendRows(res(), { ...res(), rows: [] });
    expect(sameRun(ofRun({ ...x, moreError: "boom" }, x), res())).toBe(false);
    expect(sameRun(ofRun({ ...x, moreError: "boom" }, x), x)).toBe(true);
  });

  it("keyed is false without a key, or when one of the row's key cells is cut", () => {
    const pk = keyColumns(to, res().columns);
    expect(keyed(pk, cutSet([[1, 1]]), 1)).toBe(true);
    expect(keyed(pk, cutSet([[1, 0]]), 1)).toBe(false);
    expect(keyed(keyColumns({ ...to, pk: [] }, res().columns), cutSet([]), 1)).toBe(false);
    expect(keyed(keyColumns({ ...to, pk: ["gone"] }, res().columns), cutSet([]), 1)).toBe(false);
  });

  it("the whole values are asked by key and written into the rows in place; a row not found stays cut", () => {
    const x: QueryResult = { ...res(), rows: [[1, CUT], [2, CUT]], cut: [[0, 1], [1, 1]] };
    const first = x.rows[0];
    const { rows: asked, cols, req } = fullRequest(x, to, [[0, 1], [1, 1]]);
    expect(req).toEqual({ schema: "public", table: "cut_demo", key: ["id"], columns: ["note"], rows: [["1"], ["2"]] });
    const { result, missing } = applyFull(x, [[0, 1], [1, 1]], asked, cols, [["whole one"], null]);
    expect(result.rows[0]).toBe(first);
    expect(result.rows[0][1]).toBe("whole one");
    expect(result.cut).toEqual([[1, 1]]);
    expect(missing).toBe(1);
    expect(sameRun(result, x)).toBe(true);
  });
});
