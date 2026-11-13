import { describe, expect, it } from "vitest";
import type { JoinEdit, ResultEditTarget } from "./api";
import { DEFAULT_TOKEN } from "./extract";
import { editStatements, joinStatements, submitFix } from "./gridEdit";

// Submit's statements through WASM (qurry_shared::result_edit, the TUI's rules), with the `;` the GUI adds
const users: ResultEditTarget = { schema: "public", table: "users", pk: ["id"], columns: ["id", "name", "memo"] };
const rows = [
  [1, "a", "x"],
  [2, "b", "y"],
  [3, "c", null],
];

describe("editStatements", () => {
  it("an all-default new row: () VALUES () on MySQL, DEFAULT VALUES on PostgreSQL and SQLite", () => {
    const empty = [[undefined, undefined, undefined]];
    expect(editStatements("mysql", users, rows, {}, new Set(), empty)).toEqual(["INSERT INTO public.users () VALUES ();"]);
    expect(editStatements("postgres", users, rows, {}, new Set(), empty)).toEqual(["INSERT INTO public.users DEFAULT VALUES;"]);
    expect(editStatements("sqlite", users, rows, {}, new Set(), empty)).toEqual(["INSERT INTO public.users DEFAULT VALUES;"]);
  });

  it("a new row's cell set to NULL is sent, one never set is left out; Set DEFAULT is DEFAULT", () => {
    expect(editStatements("mysql", users, rows, {}, new Set(), [[undefined, null, undefined]])).toEqual(["INSERT INTO public.users (name) VALUES (NULL);"]);
    expect(editStatements("postgres", users, rows, {}, new Set(), [[7, undefined, DEFAULT_TOKEN]])).toEqual(["INSERT INTO public.users (id, memo) VALUES ('7', DEFAULT);"]);
  });

  it("DELETE, then UPDATE, then INSERT; a row both edited and deleted is only deleted", () => {
    const changes = { 0: { 1: "a2", 2: null }, 1: { 1: "gone" }, 2: { 2: "z" } };
    expect(editStatements("postgres", users, rows, changes, new Set([1]), [[undefined, "n", undefined]])).toEqual([
      "DELETE FROM public.users WHERE id = '2';",
      "UPDATE public.users SET name = 'a2', memo = NULL WHERE id = '1';",
      "UPDATE public.users SET memo = 'z' WHERE id = '3';",
      "INSERT INTO public.users (name) VALUES ('n');",
    ]);
  });

  it("names and values quoted per kind; a result column's table column (an alias) is what the SQL names", () => {
    const t: ResultEditTarget = { schema: "S", table: "My T", pk: ["Id"], columns: ["Id", "select"] };
    expect(editStatements("mysql", t, [["it's", "a"]], { 0: { 1: "x\\y" } }, new Set(), [])).toEqual(["UPDATE S.`My T` SET `select` = 'x\\\\y' WHERE Id = 'it''s';"]);
    expect(editStatements("postgres", t, [["it's", "a"]], { 0: { 1: "x\\y" } }, new Set(), [])).toEqual([`UPDATE "S"."My T" SET "select" = E'x\\\\y' WHERE "Id" = 'it''s';`]);
  });

  it("no key: updates and deletes throw the TUI's words", () => {
    const noKey = { ...users, pk: [] };
    expect(() => editStatements("postgres", noKey, rows, {}, new Set([0]), [])).toThrow("Table has no primary key — cannot update/delete rows");
  });
});

describe("joinStatements", () => {
  it("one UPDATE per row and table, found by that table's key", () => {
    const j: JoinEdit = {
      tables: [
        { schema: "crm", table: "voc", pk: ["idx"], pkAt: [0] },
        { schema: "public", table: "users", pk: ["idx"], pkAt: [2] },
      ],
      columns: [{ table: 0, column: "idx" }, { table: 0, column: "status" }, { table: 1, column: "idx" }, { table: 1, column: "role" }, { readOnly: "why" }],
    };
    const r = [
      [10, "open", 7, "admin", "x"],
      [11, "new", null, null, null],
    ];
    expect(joinStatements("postgres", j, r, { 0: { 1: "closed", 3: null }, 1: { 1: "done" } })).toEqual([
      "UPDATE crm.voc SET status = 'closed' WHERE idx = '10';",
      "UPDATE public.users SET role = NULL WHERE idx = '7';",
      "UPDATE crm.voc SET status = 'done' WHERE idx = '11';",
    ]);
  });
});

describe("submitFix", () => {
  const dup = (stmt: string) => `error returned from database: duplicate key value violates unique constraint "users_pkey"\nKey (id)=(1) already exists.\n→ ${stmt}`;
  const cols = [{ name: "id", dataType: "bigint", nullable: false, defaultValue: null, comment: null, isPk: true, autoIncrement: true }];
  it("a new row's key the database numbers already exists: the setval to copy (#167)", () => {
    expect(submitFix("postgres", dup("INSERT INTO public.users (name) VALUES ('a');"), users, () => cols)).toEqual({
      text: "The key's sequence is behind the table's highest id; new rows get numbers that already exist.",
      sql: "SELECT setval(pg_get_serial_sequence('public.users', 'id'), (SELECT max(id) FROM public.users));",
    });
  });
  it("reads the table's columns only for that error, and offers nothing when the key isn't numbered", () => {
    let asked = 0;
    const columns = () => (asked++, cols);
    expect(submitFix("postgres", "permission denied for table users\n→ INSERT INTO public.users (name) VALUES ('a');", users, columns)).toBeUndefined();
    expect(submitFix("postgres", dup("INSERT INTO public.users (id, name) VALUES ('1', 'a');"), users, columns)).toBeUndefined();
    expect(asked).toBe(0);
    expect(submitFix("postgres", dup("INSERT INTO public.users DEFAULT VALUES;"), users, () => [{ ...cols[0], autoIncrement: false }])).toBeUndefined();
    expect(submitFix("postgres", dup("INSERT INTO public.users DEFAULT VALUES;"), users, () => undefined)).toBeUndefined();
  });
});
