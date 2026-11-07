// Console autocompletion for Monaco. The rules are qurry_shared::complete (the TUI's too), run through the shared WASM:
// one CompletionIndex per connection, built when its metadata loads, then asked synchronously on every request.
// Only the value · json key lookups (meta.hints) wait on the backend. The same index gives signature help inside a call's
// parentheses and the unknown-name markers (with their quick fixes, ⌥↩).
import { monaco } from "./monaco";
import { CompletionTable, meta } from "./api";
import { initShared, shared, type Completion, type CompletionItem, type FilterSuggestions, type Suggestions } from "./shared";

/** Console (model URI) → its target (connection id, or "dataSource::db") and database name; target → its completion index */
const modelConn = new Map<string, { connId: string; db: string }>();
const cache = new Map<string, Promise<Completion>>();
let noConnection: Completion | undefined;
let warned = false;

/** Load metadata and index it. Metadata that fails to load is not cached (retried on the next request) */
function load(connId: string): Promise<Completion> {
  const p: Promise<Completion> = meta
    .completion(connId)
    .catch(() => {
      if (cache.get(connId) === p) cache.delete(connId);
      return [] as CompletionTable[];
    })
    .then(async (tables) => {
      await initShared();
      return shared().completionIndex(tables);
    });
  // The consoles of this target get their markers once the metadata is there
  p.then(() => {
    for (const [uri, b] of modelConn) if (b.connId === connId) scheduleMarks(uri, 0);
  }, () => {});
  cache.set(connId, p);
  return p;
}

/** db: the console's database by name, also when it is the connection's default (the target then has no "::db"), as the TUI passes it */
export function bindModel(uri: string, connId: string, db: string) {
  modelConn.set(uri, { connId, db });
  // A module that can't load is said when the console opens, not later as a dialog that takes the keys being typed
  if (!cache.has(connId)) load(connId).catch(warnOff);
  scheduleMarks(uri, 0);
}

/** Once, where a GUI user sees it (the devtools console is not) */
function warnOff(e: unknown) {
  if (warned) return;
  warned = true;
  const why = `Console autocomplete runs in Qurry's shared WebAssembly module, which did not load (${e}). Restart Qurry; if it keeps happening, the system WebView is too old: update macOS (Safari) or webkit2gtk.`;
  console.error(why);
  void import("./dialogs").then((d) => d.notice(why, "Autocomplete is off"));
}
/** When the editor closes */
export function unbindModel(uri: string) {
  modelConn.delete(uri);
  clearTimeout(markTimers.get(uri));
  markTimers.delete(uri);
}

const MARKER_OWNER = "qurry-names";
const markTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** Underline the console's unknown tables and columns (qurry_shared::complete::marks: the same scope as completion; none for CTEs,
 * aliases, functions, databases not loaded) after a pause in typing. Quick fixes (⌥↩) offer the closest names */
function scheduleMarks(uri: string, delay = 300) {
  clearTimeout(markTimers.get(uri));
  markTimers.set(
    uri,
    setTimeout(async () => {
      markTimers.delete(uri);
      const model = monaco.editor.getModel(monaco.Uri.parse(uri));
      const bound = modelConn.get(uri);
      const lang = model?.getLanguageId();
      if (!model || !bound || (lang !== "pgsql" && lang !== "mysql")) return;
      const version = model.getVersionId();
      const idx = await indexFor(bound.connId);
      if (!idx || model.isDisposed() || model.getVersionId() !== version) return;
      const marks = idx.marks(model.getValue(), bound.db, lang === "mysql");
      monaco.editor.setModelMarkers(
        model,
        MARKER_OWNER,
        marks.map((m) => {
          const a = model.getPositionAt(m.start);
          const b = model.getPositionAt(m.end);
          const fix = m.fixes.length ? ` — ⌥↩ for the closest names (${m.fixes.join(", ")})` : "";
          return {
            severity: monaco.MarkerSeverity.Warning,
            message: `${m.message}${fix}. If it was just created, refresh the data source: completion knows the names from when it loaded`,
            startLineNumber: a.lineNumber,
            startColumn: a.column,
            endLineNumber: b.lineNumber,
            endColumn: b.column,
            source: "Qurry",
            code: JSON.stringify(m.fixes),
          };
        }),
      );
    }, delay),
  );
}

monaco.editor.onDidCreateModel((m) => {
  m.onDidChangeContent(() => {
    const uri = m.uri.toString();
    if (modelConn.has(uri)) scheduleMarks(uri);
  });
});
/** Drop metadata of a Data Source (and its DBs): on disconnect and refresh */
export function invalidateCompletion(connId: string) {
  for (const [k, p] of [...cache]) {
    if (k !== connId && !k.startsWith(connId + "::")) continue;
    cache.delete(k);
    // Free its WASM memory now: linear memory never shrinks, so waiting for GC raises the peak for good
    p.then((c) => c.free(), () => {});
  }
}

async function indexFor(connId: string | undefined): Promise<Completion | undefined> {
  try {
    if (connId) return await (cache.get(connId) ?? load(connId));
    await initShared();
    return (noConnection ??= shared().completionIndex([]));
  } catch (e) {
    warnOff(e);
    return undefined;
  }
}

/** A table tab's WHERE / ORDER BY field: the console's candidates for its text as `SELECT * FROM schema.table WHERE <text>`
 * (`ORDER BY`), from the same index as the console of `target` (no connection, no index: nothing); `explicit` when asked for (⌃Space) */
export async function filterSuggestions(target: string, f: { schema: string; table: string; orderBy: boolean; text: string; caret: number; db: string; mysql: boolean; explicit: boolean }): Promise<FilterSuggestions> {
  const idx = await indexFor(target);
  return idx?.suggestFilter(f.schema, f.table, f.orderBy, f.text, f.caret, f.db, f.mysql, f.explicit) ?? { from: f.caret, items: [] };
}

/** Order as the shared rules return it (Monaco sorts by sortText when nothing is typed) */
const sortText = (i: number) => String(i).padStart(5, "0");

for (const lang of ["pgsql", "mysql"]) {
  const mysql = lang === "mysql";
  monaco.languages.registerCompletionItemProvider(lang, {
    // Value · json key suggestions: right away after a quote · after '->>' · in the space after '=' (not opened in other spaces or after '>')
    triggerCharacters: [".", "'", " ", ">"],
    async provideCompletionItems(model, position, context) {
      const bound = modelConn.get(model.uri.toString());
      const connId = bound?.connId;
      const idx = await indexFor(connId);
      if (!idx) return { suggestions: [] };
      const K = monaco.languages.CompletionItemKind;
      const quiet = context.triggerKind === monaco.languages.CompletionTriggerKind.TriggerCharacter && (context.triggerCharacter === " " || context.triggerCharacter === ">");
      const text = model.getValue();
      const caret = model.getOffsetAt(position);
      const range = (s: Suggestions) => new monaco.Range(position.lineNumber, s.startColumn, position.lineNumber, s.endColumn);
      // The tab's DB: in MySQL the schema is the DB, so its tables are inserted bare (complete.rs ignores it for PG)
      const db = bound?.db ?? "";

      // Value · json key: where status = '… / data->>'… / reasons @> '…
      // The line alone tells whether this is a value spot, so the whole script crosses into WASM only there (and for suggest)
      const before = model.getLineContent(position.lineNumber).slice(0, position.column - 1);
      const hint = connId && shared().hintContext(before, mysql) ? idx.hintAt(text, caret, db, mysql) : undefined;
      if (hint && connId) {
        const ask = (mode: "values" | "keys" | "pairs") => meta.hints(connId, { schema: hint.schema, table: hint.table, column: hint.column, path: hint.path, keys: mode === "keys", pairs: mode === "pairs" }).catch(() => []);
        let hints = await ask(hint.mode);
        // json without keys (arrays of objects, etc.): instead of keys, frequent key: value pairs as @> conditions (replacing the operator too)
        const asPairs = hint.mode === "keys" && !hints.length && !mysql;
        if (asPairs) hints = await ask("pairs");
        const s = idx.hintItems(text, caret, hint, hints, asPairs);
        const r = range(s);
        return {
          suggestions: s.items.map((it, i) => ({
            label: { label: it.label, description: it.detail },
            kind: hint.mode === "keys" && !asPairs ? K.Property : K.Value,
            insertText: it.insert,
            filterText: asPairs ? hint.op + hints[i].value : it.label,
            sortText: sortText(i),
            range: r,
          })),
        };
      }
      if (quiet) return { suggestions: [] };
      const s = idx.suggest(text, caret, db, mysql);
      const r = range(s);
      const kind: Record<CompletionItem["kind"], monaco.languages.CompletionItemKind> = { column: K.Field, table: K.Class, schema: K.Module, keyword: K.Keyword, value: K.Value, join: K.Reference, function: K.Function, snippet: K.Snippet };
      const described = new Set<CompletionItem["kind"]>(["column", "table", "join", "function", "snippet"]);
      return {
        suggestions: s.items.map((it, i) => {
          const snippet = it.kind === "function" || it.kind === "snippet";
          return {
            label: described.has(it.kind) ? { label: it.label, description: it.detail } : it.label,
            kind: kind[it.kind],
            insertText: it.insert,
            insertTextRules: snippet ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet : undefined,
            // `expand *` replaces the `*` it stands for, so it is found by that, not by its label
            filterText: it.label.startsWith("expand ") ? model.getLineContent(position.lineNumber).slice(s.startColumn - 1, position.column - 1) : undefined,
            // A function opens its signature help inside the parentheses it writes
            command: it.kind === "function" ? { id: "editor.action.triggerParameterHints", title: "Parameter hints" } : undefined,
            sortText: sortText(i),
            range: r,
          };
        }),
      };
    },
  });

  // Signature help inside a call's parentheses: built-in and user functions, the parameter at the caret
  monaco.languages.registerSignatureHelpProvider(lang, {
    signatureHelpTriggerCharacters: ["(", ","],
    signatureHelpRetriggerCharacters: [")"],
    async provideSignatureHelp(model, position) {
      const idx = await indexFor(modelConn.get(model.uri.toString())?.connId);
      const s = idx?.signatureAt(model.getValue(), model.getOffsetAt(position), mysql);
      if (!s) return null;
      const which = Math.max(0, s.signatures.findIndex((x) => x.params.length > s.active));
      const params = s.signatures[which].params;
      // A `…` list keeps its last parameter active past its end
      const active = s.active < params.length ? s.active : params[params.length - 1]?.includes("…") ? params.length - 1 : s.active;
      return {
        value: { signatures: s.signatures.map((x) => ({ label: x.label, parameters: x.params.map((label) => ({ label })) })), activeSignature: which, activeParameter: active },
        dispose() {},
      };
    },
  });

  // Quick fixes for an unknown name: its closest names (⌥↩, editor.quickFix)
  monaco.languages.registerCodeActionProvider(lang, {
    provideCodeActions(model, _range, context) {
      const actions = context.markers
        .filter((m) => m.source === "Qurry")
        .flatMap((m) =>
          (JSON.parse(typeof m.code === "string" ? m.code : "[]") as string[]).map((name, i) => ({
            title: `Change to ${name}`,
            kind: "quickfix",
            diagnostics: [m],
            isPreferred: i === 0,
            edit: { edits: [{ resource: model.uri, versionId: model.getVersionId(), textEdit: { range: { startLineNumber: m.startLineNumber, startColumn: m.startColumn, endLineNumber: m.endLineNumber, endColumn: m.endColumn }, text: name } }] },
          })),
        );
      return { actions, dispose() {} };
    },
  });
}
