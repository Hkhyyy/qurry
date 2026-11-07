import * as monaco from "monaco-editor/editor/editor.api";
// editor.api has only the core and lacks features like autocomplete (suggest)·find·bracket matching → register everything via editor.main
import "monaco-editor/editor/editor.main";
import "monaco-editor/languages/definitions/pgsql/register";
import "monaco-editor/languages/definitions/mysql/register";
import { language as mysqlLanguage } from "monaco-editor/languages/definitions/mysql/mysql";
import "monaco-editor/languages/definitions/redis/register";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import { loader } from "@monaco-editor/react";
import { MONACO_THEME, monacoThemeData, onTheme } from "./theme";

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
monaco.languages.setMonarchTokensProvider("mysql", { ...mysqlLanguage, tokenizer: { ...mysqlLanguage.tokenizer, comments } } as monaco.languages.IMonarchLanguage);

// One Monaco theme, redefined from the theme drawn whenever it changes (lib/theme.ts monacoThemeData)
onTheme((t) => {
  monaco.editor.defineTheme(MONACO_THEME, monacoThemeData(t));
  monaco.editor.setTheme(MONACO_THEME);
});

export { monaco };

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
