// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// main.tsx starts loading it without waiting (the app renders even if it fails), Storybook awaits it in a loader,
// vitest initializes it in its setup file. Then call it synchronously through shared(); callers must handle it not being ready.
import init, * as wasm from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";
import type { CompletionTable, DbKind } from "./api";
import type { ExecPlan, ExecSettings } from "./execPlan";
import type { Segment } from "./sqlSplit";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

/** qurry_shared::sql::SplitOptions: editor rules where the GUI splits differently from the server (absent = the server's rule) */
export interface SplitOptions {
  mysqlDashComment?: boolean;
  dollarAfterName?: boolean;
}

/** A completion candidate (qurry_shared::complete::Item) */
export interface CompletionItem {
  label: string;
  detail: string;
  insert: string;
  kind: "column" | "table" | "schema" | "keyword" | "value";
}

/** Candidates and the range they replace on the caret's line: 1-based UTF-16 columns, as Monaco counts */
export interface Suggestions {
  startColumn: number;
  endColumn: number;
  items: CompletionItem[];
}

/** A value spot read from the text before the cursor (qurry_shared::complete::Ctx) */
export interface HintContext {
  /** alias or table (if absent, find the column among the statement's tables) */
  ref?: string;
  column: string;
  path: string[];
  mode: "values" | "keys" | "pairs";
  /** Typing inside quotes (don't add quotes when inserting) */
  quoted: boolean;
  /** Already typed text (range to replace) */
  typed: string;
  /** keys: from the operator to the cursor (`->> '` + typed) — for replacing with an @> condition */
  op?: string;
}

/** A value spot with its table and column, to ask meta.hints for (qurry_shared::complete::HintAt) */
export interface HintAt {
  schema: string;
  table: string;
  column: string;
  typeName: string;
  path: string[];
  mode: "values" | "keys" | "pairs";
  quoted: boolean;
  typed: string;
  op: string;
  /** MySQL: a backslash in a value is doubled when inserted */
  mysql: boolean;
}

/** One connection's completion metadata, indexed once (CompletionIndex in crates/shared-wasm). Carets are UTF-16 offsets into `text` */
export interface Completion {
  suggest(text: string, caret: number, db: string, mysql: boolean): Suggestions;
  hintAt(text: string, caret: number, db: string, mysql: boolean): HintAt | undefined;
  hintItems(text: string, caret: number, hint: HintAt, hints: { value: string; count: number }[], asPairs: boolean): Suggestions;
  /** Release the WASM memory now (not when GC finalizes it). Calls after this find no candidates */
  free(): void;
}

/** Rust reads a missing op as "", but not an op set to undefined */
const wire = (ctx: HintContext) => ({ ...ctx, op: ctx.op ?? "" });

// Offsets are UTF-16 code units (JS string indices); the wrapper converts them. Statement checks are UI decisions only: the backend enforces read-only
const api = {
  ping: (text: string): PingResponse => wasm.ping({ text }),
  isRead: (sql: string, kind: DbKind): boolean => wasm.isRead(sql, kind),
  isDdl: (sql: string, kind: DbKind): boolean => wasm.isDdl(sql, kind),
  splitStatements: (text: string, kind: DbKind, opts: SplitOptions): [number, number][] => wasm.splitStatements(text, kind, opts),
  splitScript: (text: string, kind: DbKind, opts: SplitOptions): [number, number][] => wasm.splitScript(text, kind, opts),
  planExecution: (text: string, offset: number, segs: Segment[], kind: DbKind, settings: ExecSettings): ExecPlan | undefined => wasm.planExecution(text, offset, segs, kind, settings),
  completionIndex: (tables: CompletionTable[]): Completion => {
    let idx: wasm.CompletionIndex | undefined = new wasm.CompletionIndex(JSON.stringify(tables));
    // A request can still hold this index after free() (it awaited the backend for hints)
    const none: Suggestions = { startColumn: 1, endColumn: 1, items: [] };
    return {
      suggest(text, caret, db, mysql) {
        if (!idx) return none;
        const s = idx.suggest(text, caret, db, mysql);
        return { startColumn: s.startColumn, endColumn: s.endColumn, items: s.lead.concat(s.rest) };
      },
      hintAt: (text, caret, db, mysql) => idx?.hintAt(text, caret, db, mysql),
      hintItems: (text, caret, hint, hints, asPairs) => idx?.hintItems(text, caret, hint, hints, asPairs) ?? none,
      free() {
        idx?.free();
        idx = undefined;
      },
    };
  },
  hintContext: (before: string, mysql: boolean): HintContext | undefined => wasm.hintContext(before, mysql),
  hintable: (ctx: HintContext, typeName: string): boolean => wasm.hintable(wire(ctx), typeName),
  hintInsert: (value: string, ctx: HintContext, typeName: string, mysql: boolean): string => wasm.hintInsert(value, wire(ctx), typeName, mysql),
  pairsInsert: wasm.pairsInsert,
};

let ready = false;
let loading: Promise<void> | undefined;

/** Load the module (fetches the .wasm). Safe to call more than once */
export function initShared(): Promise<void> {
  if (!loading && ready) loading = Promise.resolve();
  if (!loading) {
    performance.mark("qurry:shared-wasm-start");
    loading = init({ module_or_path: wasmUrl }).then(() => {
      ready = true;
      // Init time, also what the smoke test (scripts/perf/wasm-smoke.mjs) waits for
      performance.measure("qurry:shared-wasm-init", "qurry:shared-wasm-start");
    });
  }
  return loading;
}

/** Same, from the .wasm bytes already in hand (tests) */
export function initSharedSync(bytes: BufferSource) {
  wasm.initSync({ module: bytes });
  ready = true;
  loading = Promise.resolve();
}

/** Whether shared() can be called now */
export function sharedReady(): boolean {
  return ready;
}

export function shared(): typeof api {
  if (!ready) throw new Error("Shared WASM is not ready: initShared() has not finished or failed (see the console; run `pnpm wasm` if src/wasm/pkg is missing)");
  return api;
}
