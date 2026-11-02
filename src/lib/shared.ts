// Rust shared with the backend and the TUI (crates/shared), compiled to WebAssembly by `pnpm wasm`.
// main.tsx starts loading it without waiting (the app renders even if it fails), Storybook awaits it in a loader,
// vitest initializes it in its setup file. Then call it synchronously through shared(); callers must handle it not being ready.
import init, { aggregate, defaultAxes, initSync, niceTicks, numericColumns, pie, ping, shortNumber } from "../wasm/pkg/qurry_shared_wasm";
import wasmUrl from "../wasm/pkg/qurry_shared_wasm_bg.wasm?url";
import type { Cell, QueryResult } from "./api";

export interface PingResponse {
  text: string;
  /** Characters, not bytes */
  chars: number;
}

/** Chart data (crates/shared/src/chart.rs): one label per X value, one series per Y */
export interface ChartData {
  labels: string[];
  series: { name: string; values: number[] }[];
}

type Columns = QueryResult["columns"];

const api = {
  ping: (text: string): PingResponse => ping({ text }),
  numericColumns: (columns: Columns, rows: Cell[][]): number[] => numericColumns(columns, rows),
  /** y undefined = the row count */
  defaultAxes: (columns: Columns, rows: Cell[][]): { x: number; y?: number } => defaultAxes(columns, rows),
  /** A negative Y is the row count */
  aggregate: (columns: Columns, rows: Cell[][], x: number, ys: number[]): ChartData => aggregate(columns, rows, x, ys),
  pie: (data: ChartData, maxSlices: number): ChartData => pie(data, maxSlices),
  niceTicks: (min: number, max: number, count: number): number[] => niceTicks(min, max, count),
  shortNumber: (n: number): string => shortNumber(n),
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
  initSync({ module: bytes });
  ready = true;
}

/** initShared() (or initSharedSync) has finished */
export const sharedReady = () => ready;

export function shared(): typeof api {
  if (!ready) throw new Error("Shared WASM is not ready: initShared() has not finished or failed (see the console; run `pnpm wasm` if src/wasm/pkg is missing)");
  return api;
}
