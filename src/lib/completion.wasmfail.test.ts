/** Console completion when the shared WASM module does not load: no candidates, and the user is told once (not only in devtools) */
import { expect, it, vi } from "vitest";

const providers = vi.hoisted(() => new Map<string, any>());
const notice = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("./monaco", () => ({
  monaco: {
    languages: {
      CompletionItemKind: {},
      CompletionTriggerKind: { Invoke: 0, TriggerCharacter: 1 },
      registerCompletionItemProvider: (lang: string, p: unknown) => (providers.set(lang, p), {}),
    },
  },
}));
vi.mock("./api", () => ({ meta: { completion: async () => [], hints: async () => [] } }));
vi.mock("./shared", () => ({
  initShared: () => Promise.reject(new Error("CompileError: wasm-unsafe-eval")),
  shared: () => {
    throw new Error("not ready");
  },
}));
vi.mock("./dialogs", () => ({ notice }));

it("tells the user once that autocomplete is off", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const { bindModel } = await import("./completion");
  const model = { uri: { toString: () => "inmemory://x" }, getValue: () => "sel", getLineContent: () => "sel", getOffsetAt: () => 3 };
  bindModel("inmemory://x", "c", "postgres");
  for (let i = 0; i < 3; i++) {
    const r = await providers.get("pgsql").provideCompletionItems(model, { lineNumber: 1, column: 4 }, { triggerKind: 0 });
    expect(r.suggestions).toEqual([]);
  }
  await vi.waitFor(() => expect(notice).toHaveBeenCalledTimes(1));
  expect(notice.mock.calls[0]).toEqual([expect.stringContaining("Restart Qurry"), "Autocomplete is off"]);
});
