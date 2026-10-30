import { describe, expect, it } from "vitest";
import { DEFAULT_EXEC, planExecution } from "./execPlan";
import { splitStatements } from "./sqlSplit";

const SQL = "BEGIN;\nGRANT a TO x;\n\nGRANT b TO x;\nCOMMIT;";
const plan = (offset: number, s = DEFAULT_EXEC) => planExecution(SQL, offset, splitStatements(SQL), s);
const at = (needle: string) => SQL.indexOf(needle) + 1;

describe("planExecution (DataGrip Query Execution)", () => {
  it("default: asks what to run when the cursor is inside a statement", () => {
    expect(DEFAULT_EXEC.inside).toBe("ask");
    const p = plan(at("GRANT b"));
    expect(p && "ask" in p && p.ask.map((o) => o.label)).toEqual(["GRANT b TO x", "All statements (4)", "From caret to end"]);
    expect(p && "ask" in p && p.ask[2].stmts.map((s) => s.sql)).toEqual(["GRANT b TO x", "COMMIT"]);
  });

  it("doesn't ask when there is only one statement", () => {
    expect(planExecution("select 1", 3, splitStatements("select 1"), DEFAULT_EXEC)).toEqual({ run: [{ start: 0, end: 8, sql: "select 1" }] });
  });

  it("in the first statement 'from cursor to end' equals everything, so it is left out", () => {
    const p = plan(at("BEGIN"));
    expect(p && "ask" in p && p.ask.map((o) => o.label)).toEqual(["BEGIN", "All statements (4)"]);
  });

  it("smallest / whole / fromCaret", () => {
    const sql = (p: ReturnType<typeof plan>) => (p && "run" in p ? p.run.map((s) => s.sql) : null);
    expect(sql(plan(at("GRANT b"), { ...DEFAULT_EXEC, inside: "smallest" }))).toEqual(["GRANT b TO x"]);
    expect(sql(plan(at("GRANT b"), { ...DEFAULT_EXEC, inside: "whole" }))).toHaveLength(4);
    expect(sql(plan(at("GRANT b"), { ...DEFAULT_EXEC, inside: "fromCaret" }))).toEqual(["GRANT b TO x", "COMMIT"]);
  });

  it("cursor outside a statement (blank line): nothing / whole / below", () => {
    const blank = SQL.indexOf("\n\n") + 1;
    const sql = (p: ReturnType<typeof plan>) => (p && "run" in p ? p.run.map((s) => s.sql) : null);
    expect(plan(blank)).toBeNull();
    expect(sql(plan(blank, { ...DEFAULT_EXEC, outside: "whole" }))).toHaveLength(4);
    expect(sql(plan(blank, { ...DEFAULT_EXEC, outside: "below" }))).toEqual(["GRANT b TO x", "COMMIT"]);
  });

  it("a cursor right after a semicolon belongs to that statement", () => {
    const end = SQL.indexOf("GRANT a TO x;") + "GRANT a TO x;".length;
    const p = plan(end, { ...DEFAULT_EXEC, inside: "smallest" });
    expect(p && "run" in p && p.run.map((s) => s.sql)).toEqual(["GRANT a TO x"]);
  });

  it("non-SQL lines before a statement (numbered titles etc.) are left out when running", () => {
    const sql = "1. 공격 페이로드 탐지 (SSRF)\nSELECT 1\nFROM t;\n\n2. 거부 에러코드\n-- 설명\nSELECT 2;";
    const segs = splitStatements(sql);
    const run = (o: number) => planExecution(sql, o, segs, { ...DEFAULT_EXEC, inside: "smallest" });
    expect(run(3)).toEqual({ run: [{ start: sql.indexOf("SELECT 1"), end: sql.indexOf(";"), sql: "SELECT 1\nFROM t" }] });
    const p = run(sql.indexOf("2. 거부") + 1);
    expect(p && "run" in p && p.run[0].sql).toBe("-- 설명\nSELECT 2");
    // If there's no keyword line, unchanged
    expect(planExecution("foo bar", 1, splitStatements("foo bar"), DEFAULT_EXEC)).toEqual({ run: [{ start: 0, end: 7, sql: "foo bar" }] });
  });

  it("inside BEGIN … COMMIT the whole transaction can be chosen", () => {
    const sql = "BEGIN;\nGRANT a TO x;\nGRANT b TO x;\nCOMMIT;\n\nSELECT 1;";
    const p = planExecution(sql, sql.indexOf("GRANT b") + 1, splitStatements(sql), DEFAULT_EXEC);
    expect(p && "ask" in p && p.ask.map((o) => o.label)).toEqual(["GRANT b TO x", "Transaction (BEGIN … COMMIT, 4 statements)", "All statements (5)", "From caret to end"]);
    expect(p && "ask" in p && p.ask[1].stmts.map((s) => s.sql)).toEqual(["BEGIN", "GRANT a TO x", "GRANT b TO x", "COMMIT"]);
    // None outside a transaction
    const q = planExecution(sql, sql.indexOf("SELECT 1") + 1, splitStatements(sql), DEFAULT_EXEC);
    expect(q && "ask" in q && q.ask.map((o) => o.label)).toEqual(["SELECT 1", "All statements (5)"]);
  });
});

