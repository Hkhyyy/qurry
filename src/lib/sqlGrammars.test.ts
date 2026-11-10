import { describe, expect, it } from "vitest";
import { dialectOf, PLUGIN_KINDS, type DbKind } from "./api";
import { SQL_GRAMMARS } from "./sqlGrammars";

const WORDS = ["IN", "IS", "NOT", "NULL", "AND", "OR", "LIKE", "BETWEEN"];
const NOT_SQL = ["redis", "javascript"];

describe("console SQL grammars", () => {
  // Driver DBs (SQLite, DuckDB, MSSQL, ClickHouse, Oracle) use Monaco's generic "sql": their table filter draws these as keywords too
  it("every SQL language a console uses draws word operators as keywords, symbol operators stay operators", () => {
    const langs = new Set((["postgres", "mysql", ...PLUGIN_KINDS] as DbKind[]).map((k) => dialectOf(k).monaco).filter((m) => !NOT_SQL.includes(m)));
    expect([...langs].sort()).toEqual(["mysql", "pgsql", "sql"]);
    for (const id of langs) {
      const g = SQL_GRAMMARS[id];
      expect(g, id).toBeDefined();
      for (const w of WORDS) {
        expect(g.keywords, `${id} ${w}`).toContain(w);
        expect(g.operators, `${id} ${w}`).not.toContain(w);
      }
      expect(g.keywords, id).toContain("UPDATE");
    }
  });
});
