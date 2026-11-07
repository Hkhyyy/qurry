/**
 * Console completion as Monaco gets it, over crates/shared/tests/corpus/suggest.json (`‸` = cursor), compared with
 * crates/shared/tests/goldens/completion/gui.json. `UPDATE_GOLDENS=1 npx vitest run src/lib/completion.test.ts` rewrites the golden.
 * The TUI side of the same corpus is checked in crates/shared (goldens/completion/tui.json).
 */
import { describe, expect, it, vi } from "vitest";
import corpus from "../../crates/shared/tests/corpus/suggest.json";
import assist from "../../crates/shared/tests/corpus/assist.json";
import golden from "../../crates/shared/tests/goldens/completion/gui.json";

const providers = vi.hoisted(() => new Map<string, any>());
const signatureProviders = vi.hoisted(() => new Map<string, any>());
const actionProviders = vi.hoisted(() => new Map<string, any>());
/** Models by URI (what monaco.editor.getModel finds) and the markers set on them */
const models = vi.hoisted(() => new Map<string, any>());
const markers = vi.hoisted(() => new Map<string, any[]>());
vi.mock("./monaco", () => {
  class Range {
    constructor(
      public startLineNumber: number,
      public startColumn: number,
      public endLineNumber: number,
      public endColumn: number,
    ) {}
  }
  return {
    monaco: {
      Range,
      Uri: { parse: (s: string) => s },
      MarkerSeverity: { Warning: 4 },
      editor: {
        onDidCreateModel: () => ({}),
        getModel: (uri: string) => models.get(uri) ?? null,
        setModelMarkers: (m: any, _owner: string, v: any[]) => markers.set(m.uri.toString(), v),
      },
      languages: {
        CompletionItemKind: Object.fromEntries(["Field", "Class", "Module", "Keyword", "Value", "Property", "Reference", "Function", "Snippet"].map((k) => [k, k])),
        CompletionItemInsertTextRule: { InsertAsSnippet: 4 },
        CompletionTriggerKind: { Invoke: 0, TriggerCharacter: 1 },
        registerCompletionItemProvider: (lang: string, p: unknown) => (providers.set(lang, p), {}),
        registerSignatureHelpProvider: (lang: string, p: unknown) => (signatureProviders.set(lang, p), {}),
        registerCodeActionProvider: (lang: string, p: unknown) => (actionProviders.set(lang, p), {}),
      },
    },
  };
});
type Case = (typeof corpus.cases)[number] & { mysql?: boolean; db?: string; trigger?: string; hints?: Record<string, [string, number][]> };
const asked: unknown[] = [];
let current: Case | undefined;
vi.mock("./api", () => ({
  meta: {
    completion: async (id: string) => (id === "assist" ? assist.tables : corpus.tables),
    hints: async (_id: string, req: { keys: boolean; pairs: boolean }) => {
      asked.push(req);
      const mode = req.pairs ? "pairs" : req.keys ? "keys" : "values";
      return (current?.hints?.[mode] ?? []).map(([value, count]) => ({ value, count }));
    },
  },
}));

/** The bits of a Monaco text model the provider reads. Words as Monaco's default word definition (no separators, no spaces) */
function model(uri: string, text: string) {
  const lines = text.split("\n");
  const offsetAt = (p: { lineNumber: number; column: number }) => lines.slice(0, p.lineNumber - 1).reduce((n, l) => n + l.length + 1, 0) + p.column - 1;
  return {
    uri: { toString: () => uri },
    getValue: () => text,
    getLineContent: (n: number) => lines[n - 1],
    getOffsetAt: offsetAt,
    getPositionAt: (off: number) => {
      const head = text.slice(0, off).split("\n");
      return { lineNumber: head.length, column: head[head.length - 1].length + 1 };
    },
    getLanguageId: () => "pgsql",
    getVersionId: () => 1,
    isDisposed: () => false,
    getWordUntilPosition: (p: { lineNumber: number; column: number }) => {
      const w = /[^`~!@#$%^&*()\-=+[{\]}\\|;:'",.<>/?\s]*$/.exec(lines[p.lineNumber - 1].slice(0, p.column - 1))![0];
      return { word: w, startColumn: p.column - w.length, endColumn: p.column };
    },
  };
}

async function complete(c: Case, i: number) {
  const { bindModel } = await import("./completion");
  const at = c.text.indexOf("‸");
  const text = c.text.replace("‸", "");
  const head = text.slice(0, at).split("\n");
  const position = { lineNumber: head.length, column: head[head.length - 1].length + 1 };
  const uri = `inmemory://case/${i}`;
  // A console on the connection's default database: the target has no "::db", the name comes on its own
  bindModel(uri, "c", c.db ?? "");
  current = c;
  asked.length = 0;
  const context = c.trigger ? { triggerKind: 1, triggerCharacter: c.trigger } : { triggerKind: 0 };
  const r = await providers.get(c.mysql ? "mysql" : "pgsql").provideCompletionItems(model(uri, text), position, context);
  // Monaco's order with nothing typed yet: sortText, then label
  const key = (s: any) => s.sortText ?? (typeof s.label === "string" ? s.label : s.label.label);
  const items = [...r.suggestions].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
  return {
    hintRequests: [...asked],
    suggestions: items.map((s) => ({
      label: typeof s.label === "string" ? s.label : s.label.label,
      description: typeof s.label === "string" ? undefined : s.label.description,
      kind: s.kind,
      insertText: s.insertText,
      ...(s.filterText !== undefined && s.filterText !== (typeof s.label === "string" ? s.label : s.label.label) ? { filterText: s.filterText } : {}),
      range: [s.range.startLineNumber, s.range.startColumn, s.range.endLineNumber, s.range.endColumn],
    })),
  };
}

/** One suggestion per line, so a diff of the golden shows which ones changed */
type Out = Record<string, { hintRequests: unknown[]; suggestions: unknown[] }>;
const format = (out: Out) =>
  "{\n" +
  Object.entries(out)
    .map(([id, o]) => `  ${JSON.stringify(id)}: {\n    "hintRequests": ${JSON.stringify(o.hintRequests)},\n    "suggestions": [${o.suggestions.map((s) => "\n      " + JSON.stringify(s)).join(",")}${o.suggestions.length ? "\n    " : ""}]\n  }`)
    .join(",\n") +
  "\n}\n";

describe("console completion (GUI)", () => {
  it("matches the golden for every corpus case", async () => {
    const out: Out = {};
    for (const [i, c] of (corpus.cases as Case[]).entries()) out[c.id] = await complete(c, i);
    if (import.meta.env.UPDATE_GOLDENS) {
      const fs: { writeFileSync(p: URL, s: string): void } = await import("node:" + "fs");
      return fs.writeFileSync(new URL("../../crates/shared/tests/goldens/completion/gui.json", import.meta.url), format(out));
    }
    expect(out).toEqual(golden);
  });
});

/** The engine's other answers through Monaco's providers, on corpus/assist.json's metadata (foreign keys, a user function) */
describe("console assist (GUI)", () => {
  const at = (text: string) => {
    const head = text.slice(0, text.indexOf("‸")).split("\n");
    return { text: text.replace("‸", ""), position: { lineNumber: head.length, column: head[head.length - 1].length + 1 } };
  };

  it("offers a join's ON condition from the foreign keys and writes templates as snippets", async () => {
    const { bindModel } = await import("./completion");
    bindModel("inmemory://assist/join", "assist", "");
    const { text, position } = at("select * from orders o join users u ‸");
    const r = await providers.get("pgsql").provideCompletionItems(model("inmemory://assist/join", text), position, { triggerKind: 0 });
    expect(r.suggestions[0]).toMatchObject({ label: { label: "on o.user_id = u.id", description: "fk orders → users" }, kind: "Reference", insertText: "on o.user_id = u.id" });
    const t = at("upd‸");
    const s = await providers.get("pgsql").provideCompletionItems(model("inmemory://assist/join", t.text), t.position, { triggerKind: 0 });
    expect(s.suggestions.find((x: any) => x.label.label === "upd")).toMatchObject({ kind: "Snippet", insertText: "update ${1:table} set ${2:column} = ${3:value} where ${4:condition}", insertTextRules: 4 });
  });

  it("gives signature help inside a call, the parameter at the caret active", async () => {
    const { bindModel } = await import("./completion");
    bindModel("inmemory://assist/sig", "assist", "");
    const { text, position } = at("select order_total(id, ‸) from orders");
    const r = await signatureProviders.get("pgsql").provideSignatureHelp(model("inmemory://assist/sig", text), position);
    expect(r.value).toEqual({
      signatures: [
        { label: "public.order_total(order_id bigint) → numeric", parameters: [{ label: "order_id bigint" }] },
        { label: "public.order_total(order_id bigint, with_tax boolean) → numeric", parameters: [{ label: "order_id bigint" }, { label: "with_tax boolean" }] },
      ],
      activeSignature: 1,
      activeParameter: 1,
    });
    expect(await signatureProviders.get("pgsql").provideSignatureHelp(model("inmemory://assist/sig", "select 1"), { lineNumber: 1, column: 9 })).toBeNull();
  });

  it("marks unknown names and offers the closest ones as quick fixes", async () => {
    const { bindModel } = await import("./completion");
    const uri = "inmemory://assist/marks";
    const m = model(uri, "select nme from users;\nwith r as (select id k from users) select r.k from r");
    models.set(uri, m);
    bindModel(uri, "assist", "");
    await vi.waitFor(() => expect(markers.get(uri)?.length).toBe(1));
    const [mk] = markers.get(uri)!;
    expect(mk).toMatchObject({ startLineNumber: 1, startColumn: 8, endLineNumber: 1, endColumn: 11, source: "Qurry", code: '["user_name"]' });
    expect(mk.message).toMatch(/^Unknown column "nme" — ⌥↩ for the closest names \(user_name\)\. If it was just created, refresh the data source/);
    const r = actionProviders.get("pgsql").provideCodeActions(m, null, { markers: [mk] });
    expect(r.actions.map((a: any) => [a.title, a.edit.edits[0].textEdit.text])).toEqual([["Change to user_name", "user_name"]]);
  });
});
