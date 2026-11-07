// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// main.tsx starts loading it without waiting (the app renders even if it fails), Storybook awaits it in a loader,
// vitest initializes it in its setup file. Then call it synchronously through shared(); callers must handle it not being ready.
import { useSyncExternalStore } from "react";
import init, * as wasm from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";
import type { Cell, CompletionTable, DbKind, QueryResult } from "./api";
import type { ExecPlan, ExecSettings, Stmt } from "./execPlan";
import type { Extractor } from "./extract";
import type { Segment } from "./sqlSplit";
import type { TableTabKey } from "./tabLabels";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

/** qurry_shared::sql::SplitOptions: editor rules where the GUI splits differently from the server (absent = the server's rule) */
export interface SplitOptions {
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

/** Candidates in a table tab's WHERE / ORDER BY field, replacing the field's text from `from` (UTF-16) to the caret */
export interface FilterSuggestions {
  from: number;
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
  /** A table tab's WHERE / ORDER BY field (qurry_shared::complete::suggest_filter), `explicit` when asked for (⌃Space): the word from `from` to `caret` is replaced */
  suggestFilter(schema: string, table: string, orderBy: boolean, field: string, caret: number, db: string, mysql: boolean, explicit: boolean): FilterSuggestions;
  /** Release the WASM memory now (not when GC finalizes it). Calls after this find no candidates */
  free(): void;
}

/** Rust reads a missing op as "", but not an op set to undefined */
const wire = (ctx: HintContext) => ({ ...ctx, op: ctx.op ?? "" });

/** qurry_shared::extract::Options: where the GUI writes values differently from the TUI (absent = the TUI's rule) */
export interface ExtractOptions {
  strictNumbers?: boolean;
  jsNumbers?: boolean;
}

/** qurry_shared::format::ValueClass: what a grid cell holds, for its alignment */
export type ValueClass = "string" | "number" | "dateTime" | "boolean" | "null" | "jsonBinary";
/** qurry_shared::format::Align: "column" (NULL only) = where its column's type sits */
export type Align = "left" | "right" | "center" | "column";
/** Where a cell sits: "column" resolved to its column's */
export type CellAlign = Exclude<Align, "column">;
/** qurry_shared::format::Alignments: only the classes the user changed */
export type Alignments = Partial<Record<ValueClass, Align>>;

/** qurry_shared::keytree::Options */
export interface KeyTreeOptions {
  utf16Order?: boolean;
}

/** qurry_shared::extract::Rows */
export interface ExtractRowsInput {
  names: string[];
  types: string[];
  vals: Cell[][];
  keys: [string, Cell][][];
  tableRef: string;
  header: boolean;
}

/** qurry_shared::keytree::KeyNode: a key is its index in the names passed in */
export type SharedKeyNode = { kind: "folder"; name: string; path: string; count: number; children: SharedKeyNode[] } | { kind: "key"; item: number };

/** Chart data (crates/shared/src/chart.rs): one label per X value, one series per Y */
export interface ChartData {
  labels: string[];
  series: { name: string; values: number[] }[];
}

type Columns = QueryResult["columns"];

// Offsets are UTF-16 code units (JS string indices); the wrapper converts them. Statement checks are UI decisions only: the backend enforces read-only
const api = {
  ping: (text: string): PingResponse => wasm.ping({ text }),
  /** Title of the notice for a failed Commit's error: "Commit outcome unknown" when the connection was lost during it */
  commitFailedTitle: (msg: string): string => wasm.commitFailedTitle(msg),
  /** The result grid's line when loading more rows failed (qurry_shared::errors::more_rows_failed, the TUI's words) */
  moreRowsFailed: (msg: string): string => wasm.moreRowsFailed(msg),
  /** The result grid's shown columns ("cols 3–8 of 14", 1-based), or undefined when every column fits (qurry_shared::format::shown_cols, the TUI's words) */
  shownCols: (first: number, last: number, total: number): string | undefined => wasm.shownCols(first, last, total),
  /** The DBA report's database picker when the databases can't be listed (qurry_shared::errors::db_list_failed, the TUI's words) */
  dbListFailed: (msg: string): string => wasm.dbListFailed(msg),
  /** Result word of a Production log entry ("outcome unknown", "failed", "committed" …), or undefined when its rows say it */
  auditStatus: (started: boolean, sql: string, failed: boolean): string | undefined => wasm.auditStatus(started, sql, failed),
  /** The data source form's hint for a local Host with AWS SSM, or undefined (qurry_shared::errors::ssm_local_host, the TUI's words) */
  ssmLocalHost: (host: string): string | undefined => wasm.ssmLocalHost(host),
  /** The connect error is one where "Allow unencrypted connection" for the source is the way forward (qurry_shared::errors::tls_opt_in) */
  tlsOptIn: (msg: string): boolean => wasm.tlsOptIn(msg),
  isRead: (sql: string, kind: DbKind): boolean => wasm.isRead(sql, kind),
  isDdl: (sql: string, kind: DbKind): boolean => wasm.isDdl(sql, kind),
  explainAnalyzeAsk: (sql: string, kind: DbKind): string | undefined => wasm.explainAnalyzeAsk(sql, kind),
  /** [warn, text after "Explain Analyse "] */
  explainAnalyzeNote: (sql: string, kind: DbKind): [boolean, string] | undefined => wasm.explainAnalyzeNote(sql, kind),
  splitStatements: (text: string, kind: DbKind, opts: SplitOptions): [number, number][] => wasm.splitStatements(text, kind, opts),
  splitScript: (text: string, kind: DbKind, opts: SplitOptions): [number, number][] => wasm.splitScript(text, kind, opts),
  planExecution: (text: string, offset: number, segs: Segment[], kind: DbKind, settings: ExecSettings): ExecPlan | undefined => wasm.planExecution(text, offset, segs, kind, settings),
  planSelection: (sel: string, segs: Segment[], settings: ExecSettings): Stmt[] => wasm.planSelection(sel, segs, settings),
  statementAt: (text: string, offset: number, kind: DbKind): string | undefined => wasm.statementAt(text, offset, kind),
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
      suggestFilter: (schema, table, orderBy, field, caret, db, mysql, explicit) => idx?.suggestFilter(schema, table, orderBy, field, caret, db, mysql, explicit) ?? { from: caret, items: [] },
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
  isTimeType: (typeName: string): boolean => wasm.isTimeType(typeName),
  fracWidth: (values: string[]): number => wasm.fracWidth(values),
  padFrac: (s: string, width: number): string => wasm.padFrac(s, width),
  valueClass: (typeName: string, value: Cell): ValueClass => wasm.valueClass(typeName, value),
  /** The Value Editor panel's text for a cell: JSON (json / jsonb, or a JSON object or array) pretty-printed (qurry_shared::value) */
  panelText: (typeName: string, value: string): { json: boolean; text: string } => wasm.panelText(typeName, value),
  /** The panel's "{ }" toggle: valid JSON between pretty and one line, undefined for anything else */
  toggleJsonLayout: (text: string): string | undefined => wasm.toggleJsonLayout(text),
  /** What a panel edit writes to the cell, or why a json / jsonb column refuses it */
  valueToApply: (typeName: string, original: string, edited: string): { Ok: string } | { Err: string } => wasm.valueToApply(typeName, original, edited),
  alignOf: (cls: ValueClass, overrides: Alignments): Align => wasm.alignOf(cls, overrides),
  /** Where a cell of `cls` sits in a column of `typeName` whose first non-null value is `first` (an untyped column goes by it) */
  cellAlign: (typeName: string, first: Cell, cls: ValueClass, overrides: Alignments): CellAlign => wasm.cellAlign(typeName, first, cls, overrides),
  isJsonType: (typeName: string): boolean => wasm.isJsonType(typeName),
  /** [start, end, kind, …]: UTF-16 ranges, kind an index of qurry_shared::format::JsonKind */
  jsonSpans: (text: string, maxChars: number): Uint32Array => wasm.jsonSpans(text, maxChars),
  /** A label per tab, undefined where the table name is unique */
  tableTabLabels: (tabs: TableTabKey[]): (string | undefined)[] => wasm.tableTabLabels(tabs),
  isNumericType: (typeName: string): boolean => wasm.isNumericType(typeName),
  /** null → NULL */
  sqlLiteral: (kind: DbKind, value: string | null): string => wasm.sqlLiteral(kind, value),
  inList: (rows: Cell[][], types: string[], kind: DbKind, opts: ExtractOptions): string => wasm.inList(rows, types, kind, opts),
  extractRows: (fmt: Extractor, rows: ExtractRowsInput, kind: DbKind, opts: ExtractOptions): string => wasm.extractRows(fmt, rows, kind, opts),
  fuzzyScore: (query: string, text: string, wordStarts: boolean): number | undefined => wasm.fuzzyScore(query, text, wordStarts),
  keyTree: (names: string[], opts: KeyTreeOptions): SharedKeyNode[] => wasm.keyTree(names, opts),
  autoMap: (file: string[], table: string[] | null): string[] => wasm.autoMap(file, table),
  numericColumns: (columns: Columns, rows: Cell[][]): number[] => wasm.numericColumns(columns, rows),
  /** y undefined = the row count */
  defaultAxes: (columns: Columns, rows: Cell[][]): { x: number; y?: number } => wasm.defaultAxes(columns, rows),
  /** A negative Y is the row count */
  aggregate: (columns: Columns, rows: Cell[][], x: number, ys: number[]): ChartData => wasm.aggregate(columns, rows, x, ys),
  pie: (data: ChartData, maxSlices: number): ChartData => wasm.pie(data, maxSlices),
  niceTicks: (min: number, max: number, count: number): number[] => wasm.niceTicks(min, max, count),
  shortNumber: (n: number): string => wasm.shortNumber(n),
  keyLabel: (s: string): string => wasm.keyLabel(s),
  formatSpec: (spec: string, mac: boolean): string => wasm.formatSpec(spec, mac),
  // bindable, bindableIn and keyConflicts: shortcuts.ts has TS copies (the keymap editor works without the module); these check them in tests
  bindable: (spec: string): boolean => wasm.bindable(spec),
  bindableIn: (spec: string, scope: string): boolean => wasm.bindableIn(spec, scope),
  /** Ids of the other commands using `spec` in a scope overlapping `scope` */
  keyConflicts: (id: string, scope: string, spec: string, bound: { id: string; scope: string; keys: string[] }[]): string[] => wasm.keyConflicts(id, scope, spec, bound),
};

let ready = false;
let loading: Promise<void> | undefined;
const listeners = new Set<() => void>();

function markReady() {
  ready = true;
  listeners.forEach((l) => l());
}

/** Load the module (fetches the .wasm). Safe to call more than once */
export function initShared(): Promise<void> {
  if (!loading && ready) loading = Promise.resolve();
  if (!loading) {
    performance.mark("qurry:shared-wasm-start");
    loading = init({ module_or_path: wasmUrl }).then(() => {
      markReady();
      // Init time, also what the smoke test (scripts/perf/wasm-smoke.mjs) waits for
      performance.measure("qurry:shared-wasm-init", "qurry:shared-wasm-start");
    });
  }
  return loading;
}

/** Same, from the .wasm bytes already in hand (tests) */
export function initSharedSync(bytes: BufferSource) {
  wasm.initSync({ module: bytes });
  markReady();
  loading = Promise.resolve();
}

export function shared(): typeof api {
  if (!ready) throw new Error("Shared WASM is not ready: initShared() has not finished or failed (see the console; run `pnpm wasm` if src/wasm/pkg is missing)");
  return api;
}

/**
 * shared() when the module is loaded, otherwise `fallback`: for display helpers called while rendering, so a grid or tab bar
 * still draws (unformatted) while the module loads or after it failed. The failure itself is logged once at start (main.tsx).
 * A useMemo over these keeps the fallback until its deps change: put useSharedReady() in its deps
 */
export function sharedOr<T>(fallback: T, f: (s: typeof api) => T): T {
  return ready ? f(api) : fallback;
}

/** The module is loaded (shared() won't throw) */
export const sharedReady = () => ready;

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

/** sharedReady() as React state: the component renders again once the module has loaded */
export const useSharedReady = () => useSyncExternalStore(subscribe, sharedReady);
