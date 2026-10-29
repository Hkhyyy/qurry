import { describe, expect, it } from "vitest";
import { splitScript, splitStatements, statementAt } from "./sqlSplit";

const parts = (sql: string, bs = false) => splitStatements(sql, bs).map((s) => sql.slice(s.start, s.end).trim());

describe("splitStatements", () => {
  it("splits on semicolons and drops empty statements", () => {
    expect(parts("select 1; select 2;;\n")).toEqual(["select 1", "select 2"]);
  });

  it("skips semicolons inside strings, comments and dollar quotes", () => {
    expect(parts("select ';'; -- a;b\nselect 2 /* ; */; select $f$ ; $f$")).toEqual(["select ';'", "-- a;b\nselect 2 /* ; */", "select $f$ ; $f$"]);
  });

  it("MySQL follows backslash escapes", () => {
    expect(parts("select 'a\\';b'; select 2", true)).toEqual(["select 'a\\';b'", "select 2"]);
  });

  it("a \\ inside a MySQL backtick name is not an escape (split like the server)", () => {
    expect(parts("select 1 as `a\\`, 1 as `b`; delete from t; -- `", true)).toHaveLength(3);
    expect(splitScript("select 1 as `a\\`\nselect 2", true)).toHaveLength(2);
  });
});

describe("statementAt", () => {
  it("the statement under the cursor, or the one just before when between statements", () => {
    const sql = "select 1;\n\nselect 2;";
    expect(statementAt(sql, 3)?.sql).toBe("select 1");
    expect(statementAt(sql, sql.indexOf("2"))?.sql).toBe("select 2");
    expect(statementAt("", 0)).toBeNull();
  });
});

describe("splitScript", () => {
  const sp = (sql: string, mysql = false) => splitScript(sql, mysql).map((s) => sql.slice(s.start, s.end).trim());
  it("also splits statements written on new lines without semicolons (same rule as core sql::split_script)", () => {
    expect(sp("select * from a where x = 3\nselect * from public.users")).toEqual(["select * from a where x = 3", "select * from public.users"]);
    expect(sp("delete from t where id = 1\nselect 1;\nselect 2")).toEqual(["delete from t where id = 1", "select 1", "select 2"]);
    expect(sp("insert into t values (1)\ninsert into t values (2)")).toHaveLength(2);
    expect(sp("grant select on a to x\ngrant select on b to x")).toHaveLength(2);
    expect(sp("with x as (\n  select 1\n)\nselect * from x\nselect 2")).toEqual(["with x as (\n  select 1\n)\nselect * from x", "select 2"]);
    expect(sp("1. 공격 탐지 (SSRF)\nselect 1\nselect 2")).toEqual(["1. 공격 탐지 (SSRF)\nselect 1", "select 2"]);
  });
  it("doesn't split what is one statement", () => {
    for (const one of [
      "select 1\nunion all\nselect 2",
      "insert into t (a)\nselect a from u",
      "insert into t (a)\nwith x as (select 1) select * from x",
      "update t\nset a = 1\nwhere id in (\nselect id from u\n)",
      "create view v as\nselect 1",
      "alter table t\ndrop column a",
      "explain\nselect 1",
      "grant select,\ninsert on t to u",
      "grant\nselect on t to u",
      "select * from t for\nupdate",
      "insert into t values (1) on conflict (id) do\nupdate set a = 1",
      "select 'a\nselect b'",
      "select a -- x\n-- select y\nfrom t",
      "select a /* \nselect */ from t",
      "select a,\nselect_count from t",
    ]) {
      expect(sp(one), one).toHaveLength(1);
    }
    expect(sp("insert into t values (1) on duplicate key\nupdate a = 1", true)).toHaveLength(1);
  });
});
