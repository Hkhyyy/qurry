// GUI performance budget in Chromium and WebKit, one browser at a time. Prints a table (and JSON with --json).
//   Production build (`vite preview`, the app's CSP, a stub backend: the ?mock backend is dev only):
//     cold load (first contentful paint, app shell rendered) and shared WASM init (duration, ready time from navigation)
//   Dev server with ?mock and a 2,000-table schema (10 schemas × 200 tables × 20 columns):
//     per-keystroke completion cost: the provider's synchronous steps (src/lib/completion.ts) replayed through the app's own
//     shared() module and a Monaco model, at every caret while typing a normal script, and at the end and middle of large scripts;
//     the calls Monaco makes while really typing in a console; ⌃Space to the first suggestion row; React commits in a 4-pane layout
//
// Needs dist/ (scripts/perf/size.sh builds it the way CI ships it). Starts and stops its own servers on free ports.
// Run:  node scripts/perf/web.mjs [--engines chromium,webkit] [--runs 10] [--json]
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { createServer } from "node:net";
import { chromium, webkit } from "@playwright/test";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i < 0 ? dflt : process.argv[i + 1];
};
const ENGINES = arg("engines", "chromium,webkit").split(",");
const RUNS = Number(arg("runs", 10));
const JSON_OUT = process.argv.includes("--json");
const root = new URL("../../", import.meta.url).pathname;
if (!existsSync(`${root}dist/index.html`)) {
  console.error("dist/ is missing: run scripts/perf/size.sh first (production build with wasm-opt)");
  process.exit(1);
}

// ---------- stats ----------
const sorted = (xs) => [...xs].sort((a, b) => a - b);
const pct = (xs, p) => sorted(xs)[Math.max(0, Math.ceil(xs.length * p) - 1)];
const stats = (xs) => ({ n: xs.length, p50: pct(xs, 0.5), p95: pct(xs, 0.95), min: Math.min(...xs), max: Math.max(...xs) });

// ---------- servers ----------
function freePort() {
  return new Promise((ok) => {
    const s = createServer().listen(0, "localhost", () => {
      const { port } = s.address();
      s.close(() => ok(port));
    });
  });
}
const servers = [];
async function serve(args) {
  const port = await freePort();
  const p = spawn(`${root}node_modules/.bin/vite`, [...args, "--port", String(port), "--strictPort"], { cwd: root, stdio: "ignore", detached: true });
  servers.push(p);
  const url = `http://localhost:${port}`;
  for (let i = 0; i < 200; i++) {
    if (await fetch(url).then((r) => r.ok, () => false)) return url;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`vite ${args.join(" ")} did not start on ${port}`);
}
const stopServers = () => servers.forEach((p) => { try { process.kill(-p.pid); } catch {} });
process.on("exit", stopServers);
process.on("SIGINT", () => process.exit(130));

// ---------- production: cold load and WASM init ----------
// The ?mock backend is not in the production bundle: every command answers [] / null / {} like an empty install
function stubBackend() {
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd) => (cmd === "load_keymap" ? {} : cmd.startsWith("plugin:") ? Promise.reject("no backend") : /^list_|_workspaces$/.test(cmd) ? [] : null),
    transformCallback: () => 0,
  };
}

async function coldLoad(browser, url) {
  const fcp = [], shell = [], wasmInit = [], wasmReady = [];
  for (let i = 0; i < RUNS + 2; i++) {
    const ctx = await browser.newContext(); // fresh context: empty HTTP cache
    await ctx.addInitScript(stubBackend);
    await ctx.addInitScript(stampShell);
    const page = await ctx.newPage();
    await page.goto(url);
    const t = await page
      .waitForFunction(() => {
        const m = performance.getEntriesByName("qurry:shared-wasm-init")[0];
        const p = performance.getEntriesByName("first-contentful-paint")[0];
        return m && p && window.__shell && { fcp: p.startTime, shell: window.__shell, init: m.duration, ready: m.startTime + m.duration };
      }, null, { polling: "raf", timeout: 20_000 })
      .then((h) => h.jsonValue(), () => null);
    if (i >= 2 && t) {
      fcp.push(t.fcp);
      shell.push(t.shell);
      wasmInit.push(t.init);
      wasmReady.push(t.ready);
    }
    await ctx.close();
  }
  return { fcp: stats(fcp), shell: stats(shell), wasmInit: stats(wasmInit), wasmReady: stats(wasmReady) };
}
// Stamps the first animation frame in which the app shell (.app) is in the DOM
function stampShell() {
  const tick = () => (document.querySelector(".app") ? (window.__shell = performance.now()) : requestAnimationFrame(tick));
  requestAnimationFrame(tick);
}

// ---------- dev: completion ----------
// 2,000 tables in 10 schemas, 20 columns each, served as completion metadata on top of the ?mock backend
function bigCompletion() {
  const types = ["bigint", "text", "varchar", "numeric", "timestamptz", "jsonb", "boolean", "integer", "date", "uuid"];
  window.__bigTables = Array.from({ length: 2000 }, (_, i) => ({
    schema: i < 200 ? "public" : `s${Math.floor(i / 200)}`,
    name: `t_${String(i).padStart(4, "0")}`,
    columns: Array.from({ length: 20 }, (_, c) => [`col_${String(c).padStart(2, "0")}`, types[c % types.length]]),
  }));
  const inner = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (cmd, args) => (cmd === "completion_meta" ? Promise.resolve(window.__bigTables) : inner(cmd, args));
}

// A normal console script against the big schema (≈1 KB, 12 statements)
const SCRIPT = `-- daily checks
select a.col_00, a.col_01, b.col_02 from t_0001 a join s3.t_0601 b on b.col_00 = a.col_00 where a.col_03 > 10 order by a.col_01;
select count(*) from t_0002 where col_05 is not null;
update t_0003 set col_01 = 'x' where col_00 = 42;
insert into s1.t_0201 (col_00, col_01, col_02) values (1, 'a', 'b');
with recent as (select col_00, col_04 from s2.t_0450 where col_04 > now() - interval '1 day')
select r.col_00, t.col_02 from recent r left join t_0100 t on t.col_00 = r.col_00;
select t.col_10, sum(t.col_03) from s9.t_1999 t group by t.col_10 having sum(t.col_03) > 0;
delete from t_0150 where col_08 < date '2024-01-01';
select * from s5.t_1000 x where x.col_02 like 'abc%' and x.col_06 = true limit 100;
select distinct u.col_01 from t_0007 u where u.col_00 in (select col_00 from t_0008);
explain select * from s4.t_0800 where col_00 = 1;
select col_00, col_01 from t_0199 order by col_00 desc;
`;

// Runs in the page: the provider's synchronous steps for a request at `caret` (completion.ts provideCompletionItems, suggest
// path), through the app's own modules. A keystroke is timed over `reps` indexes, each fed the same keystrokes in order (as in
// real typing: an index reuses its last answer), and divided by `reps`: WebKit's clock has 1 ms steps, Chromium's 0.1 ms
async function replay(cases) {
  const { shared } = await import("/src/lib/shared.ts");
  const { monaco } = await import("/src/lib/monaco.ts");
  const s = shared();
  const t0 = performance.now();
  s.completionIndex(window.__bigTables).free();
  const indexMs = performance.now() - t0;
  const K = monaco.languages.CompletionItemKind;
  const kind = { column: K.Field, table: K.Class, schema: K.Module, keyword: K.Keyword, value: K.Value };
  const request = (idx, model, position) => {
    const text = model.getValue();
    const at = model.getOffsetAt(position);
    const before = model.getLineContent(position.lineNumber).slice(0, position.column - 1);
    if (s.hintContext(before, false)) return false; // value spot: waits on the backend, another path
    const r = idx.suggest(text, at, "", false);
    const range = new monaco.Range(position.lineNumber, r.startColumn, position.lineNumber, r.endColumn);
    window.__sink = r.items.map((it, i) => ({
      label: it.kind === "column" || it.kind === "table" ? { label: it.label, description: it.detail } : it.label,
      kind: kind[it.kind], insertText: it.insert, sortText: String(i).padStart(5, "0"), range,
    }));
    return true;
  };
  const out = { indexMs };
  for (const c of cases) {
    // typing: the text grows by one character per keystroke (prefix + typed so far + suffix), the caret after it
    const model = monaco.editor.createModel("", "pgsql");
    const ms = [];
    for (let pass = 0; pass < 2; pass++) {
      ms.length = 0; // the first pass warms the JIT
      const idxs = Array.from({ length: c.reps }, () => s.completionIndex(window.__bigTables));
      for (const k of c.carets) {
        model.setValue(c.prefix + c.typed.slice(0, k) + c.suffix);
        const position = model.getPositionAt(c.prefix.length + k);
        const start = performance.now();
        let asked = false;
        for (const idx of idxs) asked = request(idx, model, position);
        if (asked) ms.push((performance.now() - start) / c.reps);
      }
      idxs.forEach((i) => i.free());
    }
    model.dispose();
    out[c.name] = { ms, bytes: c.prefix.length + c.typed.length + c.suffix.length };
  }
  return out;
}

function cases() {
  const every = (s) => Array.from({ length: s.length }, (_, i) => i + 1);
  // Large scripts: SCRIPT repeated; one statement typed at the end, and in the middle (caret mid-text)
  const big = (kb) => SCRIPT.repeat(Math.ceil((kb * 1024) / SCRIPT.length));
  const stmt = "select a.col_00, b.col_01 from t_0001 a join s3.t_0601 b on b.col_00 = a.col_00 where a.";
  const large = (kb, reps) => {
    const t = big(kb);
    const mid = t.lastIndexOf("\n", t.length / 2) + 1;
    return [
      { name: `${kb} KB script, typing at the end`, prefix: t, typed: stmt, suffix: "", carets: every(stmt), reps },
      { name: `${kb} KB script, typing in the middle`, prefix: t.slice(0, mid), typed: stmt + ";\n", suffix: t.slice(mid), carets: every(stmt), reps },
    ];
  };
  return [{ name: "normal script (1 KB), every keystroke", prefix: "", typed: SCRIPT, suffix: "", carets: every(SCRIPT), reps: 20 }, ...large(100, 5), ...large(1024, 1)];
}

// Counts React commits through the DevTools hook (React calls onCommitFiberRoot once per commit)
function countCommits() {
  window.__commits = 0;
  window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    isDisabled: false, supportsFiber: true, renderers: new Map(),
    inject: () => 1, onCommitFiberRoot: () => window.__commits++, onCommitFiberUnmount() {}, onPostCommitFiberRoot() {}, checkDCE() {},
  };
}

// As the e2e fixture (e2e/app.ts): ⌘ shortcuts on any OS and engine
function asMac() {
  const ua = navigator.userAgent.replace(/\([^)]*\)/, "(Macintosh; Intel Mac OS X 10_15_7)");
  Object.defineProperty(navigator, "platform", { get: () => "MacIntel" });
  Object.defineProperty(navigator, "userAgent", { get: () => ua });
}

async function setText(page, text) {
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text);
  await page.keyboard.press("Escape");
}

// From the ⌃Space keydown to the first suggestion row in the DOM, measured in the page. A MutationObserver stamps the time
// right after the mutating task (polling per animation frame would round to ~16.7 ms steps)
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
    if (i >= 2) out.push(await page.evaluate(() => window.__t1 - window.__t0)); // first two warm up
  }
  await page.keyboard.press("Escape");
  return stats(out);
}

async function split(page, groupIndex, label) {
  await page.locator(".editor-group").nth(groupIndex).locator(".tab.active").click({ button: "right" });
  await page.getByText(label, { exact: true }).click();
  await page.waitForTimeout(300);
}

async function devMetrics(browser, url) {
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  await ctx.addInitScript(countCommits);
  await ctx.addInitScript(asMac);
  const page = await ctx.newPage();
  await page.goto(`${url}/?mock`);
  const conn = page.locator(".ws-explorer.on .tree-node", { hasText: "[DEV] shop_app" }).first();
  await conn.waitFor();
  await page.evaluate(bigCompletion);
  await conn.click({ position: { x: 60, y: 8 } }); // the name, not the database-count badge
  await page.keyboard.press("Meta+Shift+L");
  await page.locator(".work-layer:visible .monaco-editor").first().waitFor();
  await page.locator(".work-layer:visible .monaco-editor").first().click();

  const r = await page.evaluate(replay, cases());
  const replayStats = Object.fromEntries(Object.entries(r).filter(([k]) => k !== "indexMs").map(([k, v]) => [k, { ...stats(v.ms), bytes: v.bytes }]));

  // Real typing: the suggest calls Monaco makes (CompletionIndex.suggest, glue + WASM), wrapped on the class the app uses
  await page.evaluate(async () => {
    const { CompletionIndex } = await import("/src/wasm/pkg/qurry_shared_wasm.js");
    const orig = CompletionIndex.prototype.suggest;
    window.__suggestMs = [];
    CompletionIndex.prototype.suggest = function (...a) {
      const t = performance.now();
      try { return orig.apply(this, a); } finally { window.__suggestMs.push(performance.now() - t); }
    };
  });
  await setText(page, "");
  await page.keyboard.type(SCRIPT.split("\n").slice(1, 5).join("\n"), { delay: 15 });
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
  const typed = await page.evaluate(() => window.__suggestMs);

  await setText(page, "select * from t_1");
  const tables = await suggestLatency(page);
  await setText(page, "select * from s3.t_0601 a where a.");
  const columns = await suggestLatency(page);

  // 4 panes: split right, then split each half down
  await split(page, 0, "Split Right");
  await split(page, 0, "Split Down");
  await split(page, 2, "Split Down");
  const editor = page.locator(".editor-group").nth(0).locator(".monaco-editor");
  await editor.click();
  await setText(page, "");
  await page.waitForTimeout(500);
  const statement = "select col_00, col_01 from t_0001 where col_00 = 1";
  await page.evaluate(() => (window.__commits = 0));
  await page.keyboard.type(statement, { delay: 30 });
  await page.keyboard.press("Escape");
  await page.waitForTimeout(500);
  const typingCommits = await page.evaluate(() => window.__commits);
  await page.evaluate(() => (window.__commits = 0));
  await page.keyboard.press("Meta+Enter");
  await page.locator(".editor-group").nth(0).getByText("user_1", { exact: true }).first().waitFor();
  await page.waitForTimeout(1000);
  const runCommits = await page.evaluate(() => window.__commits);
  await ctx.close();
  return { indexMs: r.indexMs, replay: replayStats, typed: stats(typed), tables, columns, statementChars: statement.length, typingCommits, runCommits };
}

// ---------- main ----------
const results = {};
try {
  const preview = await serve(["preview"]);
  const dev = await serve([]);
  for (const engine of ENGINES) {
    const browser = await { chromium, webkit }[engine].launch();
    results[engine] = { version: browser.version(), cold: await coldLoad(browser, preview), dev: await devMetrics(browser, dev) };
    await browser.close();
  }
} finally {
  stopServers();
}

if (JSON_OUT) console.log(JSON.stringify(results, null, 2));
const f = (x) => (x == null ? "-" : x >= 100 ? x.toFixed(0) : x >= 10 ? x.toFixed(1) : x.toFixed(2));
const rows = [];
const row = (metric, engine, s, budget = "") => rows.push([metric, engine, f(s.p50), f(s.p95), `${f(s.min)}–${f(s.max)}`, String(s.n), budget]);
for (const [e, r] of Object.entries(results)) {
  row("cold: first contentful paint (ms from nav)", e, r.cold.fcp);
  row("cold: app shell rendered (ms from nav)", e, r.cold.shell);
  row("WASM init duration (ms)", e, r.cold.wasmInit);
  row("WASM ready (ms from nav)", e, r.cold.wasmReady);
  rows.push(["completion index build, 2k tables (ms)", e, f(r.dev.indexMs), "", "", "1", ""]);
  for (const [name, s] of Object.entries(r.dev.replay)) row(`suggest/keystroke: ${name}`, e, s, name.startsWith("normal") ? "p95 ≤ 2" : "");
  row("suggest calls while typing in a console (ms)", e, r.dev.typed);
  row("⌃Space → first row, 2k tables (ms)", e, r.dev.tables);
  row("⌃Space → first row, alias. columns (ms)", e, r.dev.columns);
  rows.push([`React commits typing ${r.dev.statementChars} chars, 4 panes`, e, String(r.dev.typingCommits), "", "", "1", ""]);
  rows.push(["React commits running one query, 4 panes", e, String(r.dev.runCommits), "", "", "1", ""]);
}
const head = ["metric", "engine", "p50", "p95", "min–max", "n", "budget"];
const w = head.map((h, i) => Math.max(h.length, ...rows.map((r) => r[i].length)));
const line = (r) => r.map((c, i) => (i < 2 ? c.padEnd(w[i]) : c.padStart(w[i]))).join("  ");
console.log([line(head), w.map((n) => "-".repeat(n)).join("  "), ...rows.map(line)].join("\n"));
console.log(Object.entries(results).map(([e, r]) => `${e} ${r.version}`).join(", "));
