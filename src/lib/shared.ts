// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// main.tsx starts loading it without waiting (the app renders even if it fails), Storybook awaits it in a loader,
// vitest initializes it in its setup file. Then call it synchronously through shared(); callers must handle it not being ready.
import init, * as wasm from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";
import type { DbKind } from "./api";
import type { ExecPlan, ExecSettings } from "./execPlan";
import type { Segment } from "./sqlSplit";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

/** qurry_shared::sql::SplitOptions: editor rules where the GUI splits differently from the server (absent = the server's rule) */
export interface SplitOptions {
  flatComments?: boolean;
  mysqlDashComment?: boolean;
  dollarAfterName?: boolean;
}

// Offsets are UTF-16 code units (JS string indices); the wrapper converts them. Statement checks are UI decisions only: the backend enforces read-only
const api = {
  ping: (text: string): PingResponse => wasm.ping({ text }),
  isRead: (sql: string, kind: DbKind): boolean => wasm.isRead(sql, kind),
  isDdl: (sql: string, kind: DbKind): boolean => wasm.isDdl(sql, kind),
  splitStatements: (text: string, kind: DbKind, opts: SplitOptions): [number, number][] => wasm.splitStatements(text, kind, opts),
  splitScript: (text: string, kind: DbKind, opts: SplitOptions): [number, number][] => wasm.splitScript(text, kind, opts),
  planExecution: (text: string, offset: number, segs: Segment[], kind: DbKind, settings: ExecSettings): ExecPlan | undefined => wasm.planExecution(text, offset, segs, kind, settings),
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
