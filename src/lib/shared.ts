// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// main.tsx starts loading it without waiting (the app renders even if it fails), Storybook awaits it in a loader,
// vitest initializes it in its setup file. Then call it synchronously through shared(); callers must handle it not being ready.
import init, * as wasm from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";
import type { Cell, DbKind } from "./api";
import type { Extractor } from "./extract";
import type { TableTabKey } from "./tabLabels";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

/** qurry_shared::extract::Options: where the GUI writes values differently from the TUI (absent = the TUI's rule) */
export interface ExtractOptions {
  strictNumbers?: boolean;
  jsNumbers?: boolean;
}

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

const api = {
  ping: (text: string): PingResponse => wasm.ping({ text }),
  isTimeType: (typeName: string): boolean => wasm.isTimeType(typeName),
  fracWidth: (values: string[]): number => wasm.fracWidth(values),
  padFrac: (s: string, width: number): string => wasm.padFrac(s, width),
  /** A label per tab, undefined where the table name is unique */
  tableTabLabels: (tabs: TableTabKey[]): (string | undefined)[] => wasm.tableTabLabels(tabs),
  isNumericType: (typeName: string): boolean => wasm.isNumericType(typeName),
  inList: (rows: Cell[][], types: string[], kind: DbKind, opts: ExtractOptions): string => wasm.inList(rows, types, kind, opts),
  extractRows: (fmt: Extractor, rows: ExtractRowsInput, kind: DbKind, opts: ExtractOptions): string => wasm.extractRows(fmt, rows, kind, opts),
  fuzzyScore: (query: string, text: string, wordStarts: boolean): number | undefined => wasm.fuzzyScore(query, text, wordStarts),
  keyTree: (names: string[], opts: KeyTreeOptions): SharedKeyNode[] => wasm.keyTree(names, opts),
  autoMap: (file: string[], table: string[] | null): string[] => wasm.autoMap(file, table),
};

let ready = false;
let loading: Promise<void> | undefined;

/** Load the module (fetches the .wasm). Safe to call more than once */
export function initShared(): Promise<void> {
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
}

export function shared(): typeof api {
  if (!ready) throw new Error("Shared WASM is not ready: initShared() has not finished or failed (see the console; run `pnpm wasm` if src/wasm/pkg is missing)");
  return api;
}

/**
 * shared() when the module is loaded, otherwise `fallback`: for display helpers called while rendering, so a grid or tab bar
 * still draws (unformatted) while the module loads or after it failed. The failure itself is logged once at start (main.tsx)
 */
export function sharedOr<T>(fallback: T, f: (s: typeof api) => T): T {
  return ready ? f(api) : fallback;
}

/** The module is loaded (shared() won't throw) */
export const sharedReady = () => ready;
