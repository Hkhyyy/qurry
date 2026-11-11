import { describe, expect, it } from "vitest";
import type { QueryResult } from "./api";
import { caret, edited, prune, ran, remap, show, tabOf, type Content, type Tabs } from "./resultTabs";

const rows = (n: number): QueryResult => ({ columns: [{ name: "id", typeName: "INT8" }], rows: Array.from({ length: n }, (_, i) => [i + 1]), rowsAffected: 0, truncated: false, elapsedMs: 1, hasResultSet: true });
const res = (title: string, n = 1, sql = `select * from ${title}`): Content => ({ title, sql, kind: "result", result: rows(n) });
const empty: Tabs = { results: [], activeResult: "output" };
const labels = (s: Tabs) => s.results.map((r) => `${r.num}: ${r.title}${r.pinned ? " (pinned)" : ""}`);
const S3 = ["select * from customers", "select * from orders where status = 'paid'", "select * from payments limit 5"];

/** Run statements 0 and 1 of S3 (ids a, b) */
const twoRuns = () => ran(ran(empty, 10, 0, "a", res("customers", 8)), 10, 1, "b", res("orders", 8));

describe("result tabs (#151)", () => {
  it("run 1, then 2: one tab per statement, the one just run shown", () => {
    const s = twoRuns();
    expect(s.results.map((r) => r.stmt)).toEqual([0, 1]);
    expect(labels(s)).toEqual(["1: customers", "2: orders"]);
    expect(s.activeResult).toBe("b");
  });

  it("caret back on 1 shows its tab; nothing to change returns the same state", () => {
    const s = twoRuns();
    const c = caret(s, 0);
    expect(c.activeResult).toBe("a");
    expect(caret(c, 0)).toBe(c);
    // A statement without a tab, or none at all: the selection stays
    expect(caret(c, 2)).toBe(c);
    expect(caret(c, null)).toBe(c);
    // The tab shown last is the most recently used
    expect(c.results.find((r) => r.id === "a")!.used).toBeGreaterThan(c.results.find((r) => r.id === "b")!.used);
  });

  it("edit 1 and run it again: its tab keeps its id and gets the new result", () => {
    const before = twoRuns();
    const after = [`select id, name from customers where city = 'Busan'`, S3[1], S3[2]];
    const e = edited(before, S3, after);
    expect(e).toBe(before);
    const r = ran(e, 10, 0, "new", res("customers", 3, after[0]));
    expect(r.results.map((x) => x.id)).toEqual(["a", "b"]);
    expect(r.results[0].result!.rows).toHaveLength(3);
    expect(r.results[0].sql).toBe(after[0]);
    expect(r.activeResult).toBe("a");
  });

  it("a plan is the statement's tab content too", () => {
    const s = ran(twoRuns(), 10, 0, "p", { title: "customers", sql: S3[0], kind: "plan", result: null, planJson: "{}" });
    expect(s.results.map((r) => [r.id, r.kind])).toEqual([["a", "plan"], ["b", "result"]]);
    const back = ran(s, 10, 0, "x", res("customers"));
    expect(back.results[0]).toMatchObject({ id: "a", kind: "result" });
  });

  it("pinned tab, run again: a new tab follows the statement, the pinned one stays and stops following", () => {
    const s0 = ran(empty, 10, 0, "a", res("customers", 8));
    const pinned = { ...s0, results: s0.results.map((r) => ({ ...r, pinned: true })) };
    const s = ran(pinned, 10, 0, "c", res("customers", 3));
    expect(s.results.map((r) => [r.id, r.stmt, r.pinned])).toEqual([["a", null, true], ["c", 0, false]]);
    expect(labels(s)).toEqual(["1: customers (pinned)", "1: customers"]);
    expect(s.results[0].result!.rows).toHaveLength(8);
    expect(tabOf(s, 0)!.id).toBe("c");
  });

  it("statement deleted: its unpinned tab closes, the ones after it renumber; a pinned one stays with its number frozen", () => {
    const s = ran(twoRuns(), 10, 2, "c", res("payments", 5));
    const d = edited(s, S3, [S3[0], S3[2]]);
    expect(labels(d)).toEqual(["1: customers", "2: payments"]);
    expect(d.results.map((r) => r.stmt)).toEqual([0, 1]);
    // The shown tab was closed: Output
    const shownB = show(s, "b");
    expect(edited(shownB, S3, [S3[0], S3[2]]).activeResult).toBe("output");
    // Pinned: it survives the delete, no longer follows, keeps "2"
    const pinB = { ...s, results: s.results.map((r) => (r.id === "b" ? { ...r, pinned: true } : r)) };
    const p = edited(pinB, S3, [S3[0], S3[2]]);
    expect(p.results.map((r) => [r.id, r.stmt, r.num])).toEqual([["a", 0, 1], ["b", null, 2], ["c", 1, 2]]);
    // A tab that stopped following stays put through later edits
    expect(edited(p, [S3[0], S3[2]], [S3[2]]).results.map((r) => [r.id, r.stmt, r.num])).toEqual([["b", null, 2], ["c", 0, 1]]);
  });

  it("a statement inserted above moves the tabs after it down", () => {
    const s = twoRuns();
    const e = edited(s, S3, ["select 1", ...S3]);
    expect(e.results.map((r) => [r.stmt, r.num])).toEqual([[1, 2], [2, 3]]);
  });

  it("a statement typed right above another: once its ; splits them, the tab stays with the statement it ran", () => {
    const s = ran(ran(empty, 10, 0, "a", res("customers")), 10, 1, "b", res("payments", 5, S3[2]));
    // Typed on a line of its own above statement 2: until its `;` it is part of statement 2
    const typing = edited(s, [S3[0], S3[2] + ";"], [S3[0], "select * from contracts\n" + S3[2] + ";"]);
    expect(typing).toBe(s);
    const split = edited(typing, [S3[0], "select * from contracts\n" + S3[2] + ";"], [S3[0], "select * from contracts;", S3[2] + ";"]);
    expect(split.results.map((r) => [r.id, r.stmt, r.num])).toEqual([["a", 0, 1], ["b", 2, 3]]);
    // Editing a statement never moves its tab to a copy of it elsewhere (outside the changed part)
    const copy = ran(empty, 10, 0, "a", res("customers", 1, S3[0]));
    expect(edited(copy, [S3[0], S3[1], S3[0]], [S3[0] + " where id = 1", S3[1], S3[0]]).results[0].stmt).toBe(0);
  });

  it("remap: same leading and trailing texts keep their place, the changed middle pairs in order, leftovers are gone", () => {
    expect(remap(["a", "b", "c"], ["a", "b", "c"])).toEqual([0, 1, 2]);
    expect(remap(["a", "b", "c"], ["a", "B", "c"])).toEqual([0, 1, 2]);
    expect(remap(["a", "b", "c"], ["a", "c"])).toEqual([0, null, 1]);
    expect(remap(["a", "b", "c"], ["x", "a", "b", "c"])).toEqual([1, 2, 3]);
    expect(remap(["a", "b", "c", "d"], ["a", "X", "d"])).toEqual([0, 1, null, 2]);
    expect(remap(["a"], [])).toEqual([null]);
    // Merged while typed (no `;` yet): the statement now holding the old one's text
    expect(remap(["a", "select", "select 2"], ["a", "select *\nselect 2"])).toEqual([0, 1, 1]);
  });

  it("11th tab: past the cap the least recently shown unpinned tab closes, never the shown one; pinned don't count", () => {
    let s = empty;
    for (let i = 0; i < 10; i++) s = ran(s, 10, i, `t${i}`, res(`t${i}`));
    s = caret(s, 0);
    s = ran(s, 10, 10, "t10", res("t10"));
    // Statement 2's tab (t1) was the least recently shown; statement 1's was shown just before
    expect(s.results.map((r) => r.id)).toEqual(["t0", ...Array.from({ length: 8 }, (_, i) => `t${i + 2}`), "t10"]);
    expect(s.activeResult).toBe("t10");

    const two = ran(ran(empty, 2, 0, "a", res("a")), 2, 1, "b", res("b"));
    const pinned = { ...two, results: two.results.map((r) => (r.id === "a" ? { ...r, pinned: true } : r)) };
    const three = ran(pinned, 2, 2, "c", res("c"));
    expect(three.results.map((r) => r.id)).toEqual(["a", "b", "c"]);
    const four = ran(three, 2, 3, "d", res("d"));
    expect(four.results.map((r) => r.id)).toEqual(["a", "c", "d"]);
    // The shown tab is never the one closed, even when it is the oldest
    const old = { ...four, activeResult: "c" };
    expect(prune(old, 1).results.map((r) => r.id)).toEqual(["a", "c"]);
    expect(prune(four, 5)).toBe(four);
  });

  it("a run that isn't one of the script's statements gets a tab of its own, numbered after the last", () => {
    const s = ran(twoRuns(), 10, null, "sel", res("orders"));
    expect(s.results.map((r) => [r.id, r.stmt])).toEqual([["a", 0], ["b", 1], ["sel", null]]);
    expect(labels(s)[2]).toBe("3: orders");
    // Run again: another tab (nothing to follow)
    expect(ran(s, 10, null, "sel2", res("orders")).results).toHaveLength(4);
    // Caret moves never pick it
    expect(caret(show(s, "a"), 1).activeResult).toBe("b");
  });

  it("all statements run: each gets its own tab, the last shown", () => {
    const s = ran(twoRuns(), 10, 2, "c", res("payments", 5));
    expect(labels(s)).toEqual(["1: customers", "2: orders", "3: payments"]);
    expect(s.activeResult).toBe("c");
  });

  it("error: the statement's tab keeps its last result (the console shows Output)", () => {
    const s = twoRuns();
    const e = edited(s, S3, [S3[0], "select * from order where status = 'paid'", S3[2]]);
    const failed = { ...e, activeResult: "output" };
    expect(failed.results[1]).toMatchObject({ id: "b", stmt: 1, title: "orders" });
    expect(failed.results[1].result!.rows).toHaveLength(8);
    expect(caret(failed, 1).activeResult).toBe("b");
  });
});
