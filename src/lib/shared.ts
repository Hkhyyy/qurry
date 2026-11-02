// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// main.tsx starts loading it without waiting (the app renders even if it fails), Storybook awaits it in a loader,
// vitest initializes it in its setup file. Then call it synchronously through shared(); callers must handle it not being ready.
import init, * as wasm from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

const api = {
  ping: (text: string): PingResponse => wasm.ping({ text }),
  keyLabel: (s: string): string => wasm.keyLabel(s),
  formatSpec: (spec: string, mac: boolean): string => wasm.formatSpec(spec, mac),
  bindable: (spec: string): boolean => wasm.bindable(spec),
  /** Ids of the other commands using `spec` in a scope overlapping `scope` */
  keyConflicts: (id: string, scope: string, spec: string, bound: { id: string; scope: string; keys: string[] }[]): string[] => wasm.keyConflicts(id, scope, spec, bound),
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
