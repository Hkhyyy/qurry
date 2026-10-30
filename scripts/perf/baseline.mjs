// GUI performance baseline: completion latency (2k tables × 20 columns), React commits while typing and running
// in a 4-pane layout, JS bundle size, cold start. Prints JSON; results are not kept in the repo.
//
// Playwright is not a project dependency:  npm i --no-save --prefix scripts/perf playwright && npx --prefix scripts/perf playwright install chromium webkit
// Servers (free ports):  pnpm dev --port 14250 --strictPort   and   pnpm build && pnpm preview --port 14251 --strictPort
// Run:  node scripts/perf/baseline.mjs [chromium|webkit] [dev url] [preview url]
import { readdirSync, readFileSync } from "node:fs";
import { brotliCompressSync } from "node:zlib";
import { chromium, webkit } from "playwright";

const [engine = "chromium", devUrl = "http://localhost:14250", previewUrl = "http://localhost:14251"] = process.argv.slice(2);
const browserType = { chromium, webkit }[engine];
const RUNS = 20;
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const p95 = (xs) => [...xs].sort((a, b) => a - b)[Math.ceil(xs.length * 0.95) - 1];
const stats = (xs) => ({ median: +median(xs).toFixed(1), p95: +p95(xs).toFixed(1), n: xs.length });

// Counts React commits through the DevTools hook (React calls onCommitFiberRoot once per commit)
function countCommits() {
  window.__commits = 0;
  window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    isDisabled: false, supportsFiber: true, renderers: new Map(),
    inject: () => 1, onCommitFiberRoot: () => window.__commits++, onCommitFiberUnmount() {}, onPostCommitFiberRoot() {}, checkDCE() {},
  };
}

// 2,000 tables in 10 schemas, 20 columns each, served as completion metadata on top of the ?mock backend
function bigCompletion() {
  const types = ["bigint", "text", "varchar", "numeric", "timestamptz", "jsonb", "boolean", "integer", "date", "uuid"];
  const tables = Array.from({ length: 2000 }, (_, i) => ({
    schema: i < 200 ? "public" : `s${Math.floor(i / 200)}`,
    name: `t_${String(i).padStart(4, "0")}`,
    columns: Array.from({ length: 20 }, (_, c) => [`col_${String(c).padStart(2, "0")}`, types[c % types.length]]),
  }));
  const inner = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (cmd, args) => (cmd === "completion_meta" ? Promise.resolve(tables) : inner(cmd, args));
}

async function setText(page, text) {
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text);
  await page.keyboard.press("Escape");
}

// From the Ctrl+Space keydown to the first suggestion row in the DOM, measured in the page. A MutationObserver
// stamps the time right after the mutating task (polling per animation frame would round to ~16.7 ms steps)
async function suggestLatency(page) {
  const out = [];
  for (let i = 0; i < RUNS + 2; i++) {
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !document.querySelector(".suggest-widget.visible"));
    await page.evaluate(() => {
      window.__t0 = 0;
      window.__t1 = 0;
      document.addEventListener("keydown", () => (window.__t0 = performance.now()), { capture: true, once: true });
      const mo = new MutationObserver(() => {
        if (!document.querySelector(".suggest-widget.visible .monaco-list-row")) return;
        window.__t1 = performance.now();
        mo.disconnect();
      });
      mo.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["class"] });
    });
    await page.keyboard.press("Control+Space");
    await page.waitForFunction(() => window.__t1);
    const ms = await page.evaluate(() => window.__t1 - window.__t0);
    if (i >= 2) out.push(ms); // first two are warm-up
  }
  await page.keyboard.press("Escape");
  return stats(out);
}

async function openConsole(page) {
  await page.goto(`${devUrl}/?mock`);
  await page.getByText("Database Explorer").first().waitFor();
  await page.evaluate(bigCompletion);
  await page.getByText("[DEV] shop_app").first().click();
  await page.keyboard.press("Meta+Shift+L");
  await page.locator(".monaco-editor").first().waitFor();
  await page.locator(".monaco-editor").first().click();
}

async function split(page, groupIndex, label) {
  await page.locator(".editor-group").nth(groupIndex).locator(".tab.active").click({ button: "right" });
  await page.getByText(label, { exact: true }).click();
  await page.waitForTimeout(300);
}

async function devMetrics(browser) {
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  await ctx.addInitScript(countCommits);
  const page = await ctx.newPage();
  await openConsole(page);

  await setText(page, "select * from t_1");
  const tables = await suggestLatency(page);
  await setText(page, "select * from s3.t_0601 a where a.");
  const columns = await suggestLatency(page);

  // 4 panes: split right, then split each half down
  await split(page, 0, "Split Right");
  await split(page, 0, "Split Down");
  await split(page, 2, "Split Down");
  const panes = await page.locator(".editor-group").count();
  const editor = page.locator(".editor-group").nth(0).locator(".monaco-editor");
  await editor.click();
  await setText(page, "");
  await page.waitForTimeout(500);

  const statement = "select col_00, col_01 from t_0001 where col_00 = 1";
  await page.evaluate(() => (window.__commits = 0));
  const t0 = Date.now();
  await page.keyboard.type(statement, { delay: 30 });
  await page.keyboard.press("Escape");
  await page.waitForTimeout(500);
  const typing = { chars: statement.length, commits: await page.evaluate(() => window.__commits), ms: Date.now() - t0 };

  await page.evaluate(() => (window.__commits = 0));
  await page.keyboard.press("Meta+Enter");
  await page.locator(".editor-group").nth(0).getByText("user_1", { exact: true }).first().waitFor();
  await page.waitForTimeout(1000);
  const running = { commits: await page.evaluate(() => window.__commits) };
  await ctx.close();
  return { completion: { tables2k: tables, columns20: columns }, fourPane: { panes, typing, running } };
}

// Production build (vite preview), first render of the app shell. No backend: every command answers [] or null
async function coldStart(browser) {
  const out = [];
  for (let i = 0; i < 7; i++) {
    const ctx = await browser.newContext();
    await ctx.addInitScript(() => {
      window.__TAURI_INTERNALS__ = { invoke: async (cmd) => (cmd === "load_keymap" ? {} : cmd.startsWith("plugin:") ? Promise.reject("no backend") : /^list_|_workspaces$/.test(cmd) ? [] : null), transformCallback: () => 0 };
    });
    const page = await ctx.newPage();
    await page.goto(previewUrl);
    const h = await page.waitForFunction(() => document.querySelector(".app") && performance.now(), null, { polling: "raf" });
    if (i >= 2) out.push(await h.jsonValue());
    await ctx.close();
  }
  return stats(out);
}

function bundle() {
  const dir = new URL("../../dist/assets/", import.meta.url);
  const files = readdirSync(dir).filter((f) => /\.(js|wasm)$/.test(f));
  const sum = (ext) => {
    const bytes = files.filter((f) => f.endsWith(ext)).map((f) => readFileSync(new URL(f, dir)));
    return { files: bytes.length, raw: bytes.reduce((a, b) => a + b.length, 0), brotli: bytes.reduce((a, b) => a + brotliCompressSync(b).length, 0) };
  };
  return { js: sum(".js"), wasm: sum(".wasm") };
}

const browser = await browserType.launch();
const result = { engine, version: browser.version(), bundle: bundle(), coldStartMs: await coldStart(browser), ...(await devMetrics(browser)) };
await browser.close();
console.log(JSON.stringify(result, null, 2));
