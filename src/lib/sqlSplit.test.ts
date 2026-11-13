import { describe, expect, it } from "vitest";
import type { DbKind } from "./api";
import { splitScript, splitStatements, statementAt, withoutComments } from "./sqlSplit";

const parts = (sql: string, kind: DbKind = "postgres") => splitStatements(sql, kind).map((s) => sql.slice(s.start, s.end).trim());

describe("splitStatements", () => {
  it("splits on semicolons and drops empty statements", () => {
    expect(parts("select 1; select 2;;\n")).toEqual(["select 1", "select 2"]);
  });

  it("skips semicolons inside strings, comments and dollar quotes", () => {
    expect(parts("select ';'; -- a;b\nselect 2 /* ; */; select $f$ ; $f$")).toEqual(["select ';'", "-- a;b\nselect 2 /* ; */", "select $f$ ; $f$"]);
  });

  it("MySQL follows backslash escapes", () => {
    expect(parts("select 'a\\';b'; select 2", "mysql")).toEqual(["select 'a\\';b'", "select 2"]);
  });

  it("MySQL --x is arithmetic, not a comment, so what follows it is its own statement (split like the server)", () => {
    expect(parts("select 1--1; delete from orders where status = 'x'", "mysql")).toEqual(["select 1--1", "delete from orders where status = 'x'"]);
  });

  it("a \\ inside a MySQL backtick name is not an escape (split like the server)", () => {
    expect(parts("select 1 as `a\\`, 1 as `b`; delete from t; -- `", "mysql")).toHaveLength(3);
    expect(splitScript("select 1 as `a\\`\nselect 2", "mysql")).toHaveLength(2);
  });

  it("commented-out or quoted code is never its own statement (MySQL # comments, PostgreSQL E'\\'')", () => {
    const my = "select * from orders where id = 6 # was; delete from orders where id = 6\nselect 2;";
    expect(parts(my, "mysql")).toEqual([my.slice(0, -1)]);
    expect(splitScript(my, "mysql").map((s) => my.slice(s.start, s.end).trim())).toEqual(["select * from orders where id = 6 # was; delete from orders where id = 6", "select 2"]);
    const pg = "select E'\\'; delete from t where id = 1; --'";
    expect(parts(pg)).toEqual([pg]);
    expect(splitScript(pg).map((s) => pg.slice(s.start, s.end))).toEqual([pg]);
    expect(parts("select e'\\''; select 2")).toEqual(["select e'\\''", "select 2"]);
    // name'…' is not an E-string: \ is a plain char there
    expect(parts("select 1 as name'\\'; select 2")).toHaveLength(2);
  });
});

describe("statementAt", () => {
  it("the statement under the cursor, or the one just before when between statements", () => {
    const sql = "select 1;\n\nselect 2;";
    expect(statementAt(sql, 3)).toBe("select 1");
    expect(statementAt(sql, sql.indexOf("2"))).toBe("select 2");
    expect(statementAt("", 0)).toBeNull();
  });
});

describe("splitScript", () => {
  const sp = (sql: string, kind: DbKind = "postgres") => splitScript(sql, kind).map((s) => sql.slice(s.start, s.end).trim());
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
    expect(sp("insert into t values (1) on duplicate key\nupdate a = 1", "mysql")).toHaveLength(1);
  });
});

describe("withoutComments (what the confirm dialogs show, #171)", () => {
  it("leaves out the comments the server reads, keeping the SQL around and after them", () => {
    expect(withoutComments("--\n-- Name: orders; Type: TABLE\n--\n\nCREATE TABLE orders (\n  id int\n)")).toBe("CREATE TABLE orders ( id int )");
    expect(withoutComments("/* note */ DROP TABLE t")).toBe("DROP TABLE t");
    expect(withoutComments("DELETE FROM t /* WHERE id = 1 */")).toBe("DELETE FROM t");
    expect(withoutComments("DROP/**/TABLE t")).toBe("DROP TABLE t");
  });

  it("not inside strings, quoted names or dollar quotes; by the kind's rules", () => {
    expect(withoutComments("UPDATE t SET a = '--x' WHERE \"b/*\" = 1")).toBe("UPDATE t SET a = '--x' WHERE \"b/*\" = 1");
    expect(withoutComments("DO $$ -- body\nDELETE FROM t $$")).toBe("DO $$ -- body DELETE FROM t $$");
    expect(withoutComments("# note\nDELETE FROM t # all", "mysql")).toBe("DELETE FROM t");
    expect(withoutComments("select 1 # 2")).toBe("select 1 # 2");
    expect(withoutComments("/* a /* b */ DELETE FROM t */", "mysql")).toBe("DELETE FROM t */");
    expect(withoutComments("/*!40101 SET NAMES utf8 */", "mysql")).toBe("/*!40101 SET NAMES utf8 */");
  });

  it("keeps a comment with a quote in it when a backslash lets the server's settings move where strings end", () => {
    expect(withoutComments("select 'a\\' -- '; drop table x; --'")).toContain("drop table x");
  });
});
