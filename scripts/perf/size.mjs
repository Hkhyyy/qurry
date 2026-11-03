// Sizes of what `pnpm build` ships (dist/assets): the shared WASM module against its budget, and the JS / CSS chunks.
// Run after a production build (scripts/perf/size.sh builds one with wasm-opt, as CI and release do):  node scripts/perf/size.mjs
import { readdirSync, readFileSync } from "node:fs";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const dir = new URL("../../dist/assets/", import.meta.url);
// Highest levels, as a static server would precompress them
const br = (b) => brotliCompressSync(b, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length;
const gz = (b) => gzipSync(b, { level: 9 }).length;
const rows = readdirSync(dir)
  .filter((name) => /\.(js|css|wasm)$/.test(name))
  .map((name) => {
    const b = readFileSync(new URL(name, dir));
    return { name, raw: b.length, gzip: gz(b), brotli: br(b) };
  })
  .sort((a, b) => b.raw - a.raw);
const total = (ext) => {
  const of = rows.filter((r) => r.name.endsWith(ext));
  const sum = (k) => of.reduce((a, r) => a + r[k], 0);
  return { name: `all ${ext} (${of.length} files)`, raw: sum("raw"), gzip: sum("gzip"), brotli: sum("brotli") };
};
const kb = (n) => (n / 1024).toFixed(1);
const shown = [...rows.slice(0, 8), total(".js"), total(".css"), total(".wasm")];
const w = Math.max(...shown.map((r) => r.name.length));
console.log(`${"file".padEnd(w)}  ${"raw KB".padStart(8)}  ${"gzip KB".padStart(8)}  ${"brotli KB".padStart(9)}`);
for (const r of shown) console.log(`${r.name.padEnd(w)}  ${kb(r.raw).padStart(8)}  ${kb(r.gzip).padStart(8)}  ${kb(r.brotli).padStart(9)}`);
const wasm = rows.find((r) => r.name.endsWith(".wasm"));
if (wasm) console.log(`WASM budget 1024 KB raw / 350 KB brotli: ${wasm.raw <= 1024 * 1024 && wasm.brotli <= 350 * 1024 ? "OK" : "OVER"}`);
