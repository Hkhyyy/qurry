import * as monaco from "monaco-editor/editor/editor.api";
// editor.api has only the core and lacks features like autocomplete (suggest)·find·bracket matching → register everything via editor.main
import "monaco-editor/editor/editor.main";
import "monaco-editor/languages/definitions/pgsql/register";
import "monaco-editor/languages/definitions/mysql/register";
import { language as mysqlLanguage } from "monaco-editor/languages/definitions/mysql/mysql";
import { language as pgsqlLanguage } from "monaco-editor/languages/definitions/pgsql/pgsql";
import "monaco-editor/languages/definitions/redis/register";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import { loader } from "@monaco-editor/react";
import { MONACO_THEME, monacoThemeData, onTheme } from "./theme";
import type { DbKind } from "./api";
import { initShared, sharedOr } from "./shared";

// JSON (the grid's Value Editor panel) validates and folds in its own worker
self.MonacoEnvironment = { getWorker: (_, label) => (label === "json" ? new JsonWorker() : new EditorWorker()) };
loader.config({ monaco });

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
 * INSERT, DELETE, VALUES, ALTER, DROP, BEGIN …), mysql no BEGIN, COMMIT, ROLLBACK, TRUNCATE … Those showed as plain names
 */
const COMMON_KEYWORDS = `SELECT INSERT UPDATE DELETE MERGE SET VALUES INTO FROM WHERE JOIN LEFT RIGHT INNER OUTER FULL CROSS NATURAL ON USING AS BY ORDER
  GROUP HAVING LIMIT OFFSET FETCH RETURNING WITH RECURSIVE UNION INTERSECT EXCEPT ALL DISTINCT CREATE ALTER DROP TRUNCATE RENAME TO ADD COLUMN
  TABLE VIEW MATERIALIZED INDEX UNIQUE SCHEMA DATABASE FUNCTION PROCEDURE TRIGGER SEQUENCE TYPE EXTENSION IF EXISTS NOT REPLACE CASCADE RESTRICT
  BEGIN START TRANSACTION COMMIT ROLLBACK SAVEPOINT RELEASE EXPLAIN ANALYZE VACUUM COPY SHOW CALL DO CONFLICT NOTHING GRANT REVOKE
  NULLS FIRST LAST OVER PARTITION FILTER WINDOW ROWS RANGE PRECEDING FOLLOWING UNBOUNDED CURRENT ROW INTERVAL ESCAPE LATERAL`.split(/\s+/);
const withKeywords = (words: string[]) => [...new Set([...words, ...COMMON_KEYWORDS])];
monaco.languages.setMonarchTokensProvider("pgsql", { ...pgsqlLanguage, keywords: withKeywords(pgsqlLanguage.keywords) } as monaco.languages.IMonarchLanguage);
monaco.languages.setMonarchTokensProvider("mysql", { ...mysqlLanguage, keywords: withKeywords(mysqlLanguage.keywords), tokenizer: { ...mysqlLanguage.tokenizer, comments } } as monaco.languages.IMonarchLanguage);

// One Monaco theme, redefined from the theme drawn whenever it changes (lib/theme.ts monacoThemeData)
onTheme((t) => {
  monaco.editor.defineTheme(MONACO_THEME, monacoThemeData(t));
  monaco.editor.setTheme(MONACO_THEME);
});

export { monaco };

/** Owner of a console's run error marker, apart from every other marker so setting or clearing it leaves theirs alone */
const RUN_ERROR = "qurry.runError";

/**
 * Mark a failed run's error in the console (DataGrip): at the server's spot (`at`, an offset into the model; the word there, else one
 * character) or, when the error doesn't say, over the whole statement (`start`–`end`). null clears it (an edit, the next run)
 */
export function setRunError(m: monaco.editor.ITextModel, err: { start: number; end: number; at?: number; message: string } | null) {
  if (!err) return monaco.editor.setModelMarkers(m, RUN_ERROR, []);
  let range: monaco.IRange = monaco.Range.fromPositions(m.getPositionAt(err.start), m.getPositionAt(err.end));
  if (err.at !== undefined) {
    const p = m.getPositionAt(err.at);
    // The word from the spot on (not one that only ends there: `foo(` with the spot on the parenthesis)
    const w = [m.getWordAtPosition(p)].find((x) => x && p.column < x.endColumn);
    const max = m.getLineMaxColumn(p.lineNumber);
    // Past the line's last character (MySQL's near '' is the statement's end): the character before it
    const col = w ? w.startColumn : Math.max(Math.min(p.column, max - 1), 1);
    range = new monaco.Range(p.lineNumber, col, p.lineNumber, w ? w.endColumn : Math.min(col + 1, max));
  }
  monaco.editor.setModelMarkers(m, RUN_ERROR, [{ ...range, severity: monaco.MarkerSeverity.Error, message: err.message }]);
}

/**
 * When closing a console tab: discard that console's undo history.
 * Monaco keeps a closed model's undo (up to 20MB) in case the same URI is reopened, but console URIs (console-<tab id>.sql) are never reused.
 * For a tab not on screen (another workspace), briefly create a model with the same URI and different content so the leftover history is discarded
 */
export function forgetConsoleModel(tabId: string) {
  const uri = monaco.Uri.parse(`console-${tabId}.sql`);
  const open = monaco.editor.getModel(uri);
  if (open) return open.setValue("");
  const tmp = monaco.editor.createModel("\u0000forget", undefined, uri);
  tmp.setValue("");
  tmp.dispose();
}

/**
 * Quotes and brackets left open, and closing brackets without an opener (qurry_shared::pairs::unbalanced, the TUI editor's marks), as
 * error markers of their own owner: from the opening character to the end of the line it opens on. Read again shortly after each edit
 */
export function markUnbalanced(ed: monaco.editor.IStandaloneCodeEditor, kind: () => DbKind): monaco.IDisposable {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const update = () => {
    const m = ed.getModel();
    if (!m || m.isDisposed()) return;
    const marks = sharedOr([], (s) => s.unbalanced(m.getValue(), kind()));
    monaco.editor.setModelMarkers(
      m,
      "qurry.unbalanced",
      marks.map((u) => {
        const at = m.getPositionAt(u.start);
        const end = Math.max(m.getPositionAt(u.end).column, m.getLineMaxColumn(at.lineNumber));
        return { severity: monaco.MarkerSeverity.Error, message: u.message, startLineNumber: at.lineNumber, startColumn: at.column, endLineNumber: at.lineNumber, endColumn: end };
      }),
    );
  };
  void initShared().then(update, () => {});
  const sub = ed.onDidChangeModelContent(() => {
    clearTimeout(timer);
    timer = setTimeout(update, 250);
  });
  return {
    dispose() {
      clearTimeout(timer);
      sub.dispose();
    },
  };
}
