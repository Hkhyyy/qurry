/**
 * Console completion as Monaco gets it, over crates/shared/tests/corpus/suggest.json (`‸` = cursor), compared with
 * crates/shared/tests/goldens/completion/gui.json. `UPDATE_GOLDENS=1 npx vitest run src/lib/completion.test.ts` rewrites the golden.
 * The TUI side of the same corpus is checked in crates/shared (goldens/completion/tui.json).
 */
import { describe, expect, it, vi } from "vitest";
import corpus from "../../crates/shared/tests/corpus/suggest.json";
import golden from "../../crates/shared/tests/goldens/completion/gui.json";

const providers = vi.hoisted(() => new Map<string, any>());
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
      languages: {
        CompletionItemKind: Object.fromEntries(["Field", "Class", "Module", "Keyword", "Value", "Property"].map((k) => [k, k])),
        CompletionTriggerKind: { Invoke: 0, TriggerCharacter: 1 },
        registerCompletionItemProvider: (lang: string, p: unknown) => (providers.set(lang, p), {}),
      },
    },
  };
});
type Case = (typeof corpus.cases)[number] & { mysql?: boolean; db?: string; trigger?: string; hints?: Record<string, [string, number][]> };
const asked: unknown[] = [];
let current: Case | undefined;
vi.mock("./api", () => ({
  meta: {
    completion: async () => corpus.tables,
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
  bindModel(uri, c.db ? `c::${c.db}` : "c");
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
