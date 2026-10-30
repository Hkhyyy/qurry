// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// Initialize once before use (main.tsx before the first render, Storybook in a loader, vitest in its setup file),
// then call it synchronously through shared().
import init, { initSync, ping } from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

const api = {
  ping: (text: string): PingResponse => ping({ text }),
};

let ready = false;
let loading: Promise<void> | undefined;

/** Load the module (fetches the .wasm). Safe to call more than once */
export function initShared(): Promise<void> {
  loading ??= init({ module_or_path: wasmUrl }).then(() => {
    ready = true;
  });
  return loading;
}

/** Same, from the .wasm bytes already in hand (tests) */
export function initSharedSync(bytes: BufferSource) {
  initSync({ module: bytes });
  ready = true;
}

export function shared(): typeof api {
  if (!ready) throw new Error("Shared WASM is not initialized: await initShared() before the first call (run `pnpm wasm` if src/wasm/pkg is missing)");
  return api;
}
