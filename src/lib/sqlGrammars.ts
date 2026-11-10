import { language as mysqlLanguage } from "monaco-editor/languages/definitions/mysql/mysql";
import { language as pgsqlLanguage } from "monaco-editor/languages/definitions/pgsql/pgsql";
import { language as sqlLanguage } from "monaco-editor/languages/definitions/sql/sql";

/**
 * MySQL reads `--` as a comment only when a space, a control character or the line end follows (`select 1--1` is 1 - -1), and so does
 * the console's split (qurry_shared::sql dash_comment). The built-in grammar greys out every `--…`, so `select 1--1; delete …` looked like
 * one statement with a comment while ⌘↩ ran the DELETE: colour it the way it runs
 */
const comments = [
  [/--(?:[\x00-\x20\x7f].*|$)/, "comment"],
  [/#+.*/, "comment"],
  [/\/\*/, { token: "comment.quote", next: "@comment" }],
];
/**
 * The built-in grammars know only part of the words a statement is made of: pgsql only PostgreSQL's reserved words (no UPDATE, SET,
 * INSERT, DELETE, VALUES, ALTER, DROP, BEGIN …), mysql no BEGIN, COMMIT, ROLLBACK … Those showed as plain names. Words that are
 * common column names (type, first, last, row, partition …) aren't added
 */
const COMMON_KEYWORDS = `SELECT INSERT UPDATE DELETE MERGE SET VALUES INTO FROM WHERE JOIN LEFT RIGHT INNER OUTER FULL CROSS NATURAL ON USING AS BY ORDER
  GROUP HAVING LIMIT OFFSET FETCH RETURNING WITH RECURSIVE UNION INTERSECT EXCEPT ALL DISTINCT CREATE ALTER DROP TRUNCATE RENAME TO ADD COLUMN
  TABLE VIEW MATERIALIZED INDEX UNIQUE SCHEMA DATABASE FUNCTION PROCEDURE TRIGGER SEQUENCE EXTENSION IF EXISTS NOT REPLACE CASCADE RESTRICT
  BEGIN TRANSACTION COMMIT ROLLBACK SAVEPOINT EXPLAIN ANALYZE VACUUM COPY SHOW CALL DO CONFLICT NOTHING GRANT REVOKE
  NULLS OVER WINDOW PRECEDING FOLLOWING UNBOUNDED INTERVAL ESCAPE LATERAL`.split(/\s+/);
/**
 * The grammars list these words as operators, drawn in the base theme's operator gray. They are keywords in the table tab's WHERE /
 * ORDER BY (qurry_shared::highlight, completion's keywords), so the console draws them as keywords too; symbol operators stay operators
 */
const WORD_OPERATORS = ["IN", "IS", "NOT", "NULL", "AND", "OR", "LIKE", "BETWEEN"];
type Grammar = { tokenizer: Record<string, unknown[]>; keywords: string[]; operators?: unknown } & Record<string, unknown>;
const withKeywords = (l: Grammar) => ({
  ...l,
  keywords: [...new Set([...l.keywords, ...COMMON_KEYWORDS, ...WORD_OPERATORS])],
  operators: (l.operators as string[]).filter((o) => !WORD_OPERATORS.includes(o)),
});

/**
 * The Monarch grammar of every SQL language a console uses (Dialect.monaco): pgsql, mysql, and the generic sql of the driver DBs
 * (SQLite, DuckDB, MSSQL, ClickHouse, Oracle). lib/monaco.ts registers each
 */
export const SQL_GRAMMARS: Record<string, ReturnType<typeof withKeywords>> = {
  pgsql: withKeywords(pgsqlLanguage),
  mysql: withKeywords({ ...mysqlLanguage, tokenizer: { ...mysqlLanguage.tokenizer, comments } }),
  sql: withKeywords(sqlLanguage),
};
