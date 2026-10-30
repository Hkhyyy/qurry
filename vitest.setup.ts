// Shared WASM for every test file: instantiated synchronously from the .wasm bytes, so tests can call shared() directly
import { existsSync, readFileSync } from "node:fs";

const wasm = new URL("./src/wasm/pkg/qurry_shared_wasm_bg.wasm", import.meta.url);
if (!existsSync(wasm)) throw new Error("src/wasm/pkg is missing: run `pnpm wasm` first (`pnpm test` does it for you)");
const { initSharedSync } = await import("./src/lib/shared");
initSharedSync(readFileSync(wasm));
