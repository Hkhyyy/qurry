// Smoke test for the shared WASM under the app's production CSP (`vite preview` sends it):
// 1. the module initializes (no CSP refusal) and the app renders;
// 2. when the .wasm cannot load, the app still renders (so the in-app updater can still deliver a fix).
// Exits non-zero on failure. Prints the init time and the .wasm transfer size as JSON.
//
// Server (free port):  scripts/perf/size.sh && npx vite preview --port 14251 --strictPort
// Run:  node scripts/perf/wasm-smoke.mjs [chromium|webkit] [preview url]
import { chromium, webkit } from "@playwright/test";

const [engine = "chromium", url = "http://localhost:14251"] = process.argv.slice(2);
const browser = await { chromium, webkit }[engine].launch();
const failures = [];

async function load({ blockWasm }) {
  const page = await browser.newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(String(e)));
  if (blockWasm) await page.route("**/*.wasm", (r) => r.abort());
  await page.goto(url);
  // React mounted: #root holds elements, not the boot error text
  const rendered = await page.waitForSelector("#root > *", { timeout: 10_000 }).then(() => true, () => false);
  const init = blockWasm
    ? null
    : await page
        .waitForFunction(() => performance.getEntriesByName("qurry:shared-wasm-init")[0]?.duration, null, { timeout: 10_000 })
        .then((h) => h.jsonValue(), () => null);
  const wasm = await page.evaluate(() => performance.getEntriesByType("resource").find((e) => e.name.endsWith(".wasm"))?.transferSize ?? null);
  await page.close();
  return { rendered, init, wasm, errors };
}

const ok = await load({ blockWasm: false });
if (!ok.rendered) failures.push("app did not render");
if (ok.init == null) failures.push("shared WASM did not initialize");
// Only WASM errors count: without the Tauri backend the app logs failed IPC calls
const wasmErrors = ok.errors.filter((e) => /WebAssembly|wasm/i.test(e));
if (wasmErrors.length) failures.push(`console errors: ${wasmErrors.join(" | ")}`);

const blocked = await load({ blockWasm: true });
if (!blocked.rendered) failures.push("app did not render when the .wasm failed to load");
if (!blocked.errors.some((e) => /WebAssembly/.test(e))) failures.push("no console error naming the WebAssembly failure");

await browser.close();
console.log(JSON.stringify({ engine, initMs: ok.init && +ok.init.toFixed(1), wasmTransferBytes: ok.wasm, failures }, null, 2));
process.exit(failures.length ? 1 : 0);
