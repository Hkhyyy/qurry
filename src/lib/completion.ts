// Console autocompletion for Monaco. The rules are qurry_shared::complete (the TUI's too), run through the shared WASM:
// one CompletionIndex per connection, built when its metadata loads, then asked synchronously on every request.
// Only the value · json key lookups (meta.hints) wait on the backend.
import { monaco } from "./monaco";
import { CompletionTable, meta } from "./api";
import { initShared, shared, type Completion, type CompletionItem, type Suggestions } from "./shared";

/** Console (model URI) → connection id, connection id → its completion index */
const modelConn = new Map<string, string>();
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
  cache.set(connId, p);
  return p;
}

export function bindModel(uri: string, connId: string) {
  modelConn.set(uri, connId);
  if (!cache.has(connId)) load(connId).catch(() => {});
}
/** When the editor closes */
export function unbindModel(uri: string) {
  modelConn.delete(uri);
}
/** Drop metadata of a Data Source (and its DBs): on disconnect and refresh */
export function invalidateCompletion(connId: string) {
  for (const k of [...cache.keys()]) if (k === connId || k.startsWith(connId + "::")) cache.delete(k);
}

async function indexFor(connId: string | undefined): Promise<Completion | undefined> {
  try {
    if (connId) return await (cache.get(connId) ?? load(connId));
    await initShared();
    return (noConnection ??= shared().completionIndex([]));
  } catch (e) {
    if (!warned) {
      warned = true;
      console.error(`Console autocomplete is off: it runs in Qurry's shared WebAssembly module, which did not load (${e}). Restart Qurry; if it keeps happening, the system WebView is too old: update macOS (Safari) or webkit2gtk.`);
    }
    return undefined;
  }
}

/** Order as the shared rules return it (Monaco sorts by sortText when nothing is typed) */
const sortText = (i: number) => String(i).padStart(5, "0");

for (const lang of ["pgsql", "mysql"]) {
  const mysql = lang === "mysql";
  monaco.languages.registerCompletionItemProvider(lang, {
    // Value · json key suggestions: right away after a quote · after '->>' · in the space after '=' (not opened in other spaces or after '>')
    triggerCharacters: [".", "'", " ", ">"],
    async provideCompletionItems(model, position, context) {
      const connId = modelConn.get(model.uri.toString());
      const idx = await indexFor(connId);
      if (!idx) return { suggestions: [] };
      const K = monaco.languages.CompletionItemKind;
      const quiet = context.triggerKind === monaco.languages.CompletionTriggerKind.TriggerCharacter && (context.triggerCharacter === " " || context.triggerCharacter === ">");
      const text = model.getValue();
      const caret = model.getOffsetAt(position);
      const range = (s: Suggestions) => new monaco.Range(position.lineNumber, s.startColumn, position.lineNumber, s.endColumn);
      // db "": bare names only for tables in public (other schemas, MySQL DBs included, are inserted as schema.table)

      // Value · json key: where status = '… / data->>'… / reasons @> '…
      const hint = connId ? idx.hintAt(text, caret, "", mysql) : undefined;
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
      const s = idx.suggest(text, caret, "", mysql);
      const r = range(s);
      const kind: Record<CompletionItem["kind"], monaco.languages.CompletionItemKind> = { column: K.Field, table: K.Class, schema: K.Module, keyword: K.Keyword, value: K.Value };
      return {
        suggestions: s.items.map((it, i) => ({
          label: it.kind === "column" || it.kind === "table" ? { label: it.label, description: it.detail } : it.label,
          kind: kind[it.kind],
          insertText: it.insert,
          sortText: sortText(i),
          range: r,
        })),
      };
    },
  });
}
