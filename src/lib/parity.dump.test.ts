/**
 * Parity dump (`pnpm parity:dump`, skipped by plain `pnpm test`): runs today's TS functions over crates/shared/tests/corpus
 * and writes crates/shared/tests/goldens/ts/*.json. When goldens/rust exists (`cargo test -p qurry-tui parity_dump -- --ignored`),
 * it also writes crates/shared/tests/DIVERGENCES.md: every input where both sides have an output and they disagree.
 * Goldens map corpus id → function → output. Both dumps use the same keys and output shapes (segment text, not offsets).
 * A `‸` in a corpus text marks the cursor (removed before the call; UTF-16 offset here, byte offset in Rust).
 */
import { describe, expect, it, vi } from "vitest";
import { dialectOf, type Cell, type DbKind, type QueryResult } from "./api";
import { buildChart, COUNT, defaultAxes, niceTicks, numericColumns, shortNumber } from "./chart";
import { DEFAULT_EXEC, planExecution, type ExecPlan, type ExecSettings } from "./execPlan";
import { EXTRACTORS, extractRows, type Extractor } from "./extract";
import { fuzzyScore } from "./fuzzy";
import { inList } from "./inList";
import { keyTree, type KeyNode } from "./keyTree";
import { formatSpec, keyLabel } from "./shortcuts";
import { splitScript, splitStatements, type Segment } from "./sqlSplit";
import { isDdl, isRead } from "./stmtKind";
import { tableTabLabels, type TableTabKey } from "./tabLabels";
import { fracWidth, isTimeType, padFrac } from "./timeFrac";
import { autoMap } from "./transfer";
import { hintable, hintContext, hintInsert, pairsForKeys } from "./valueHint";
import sqlCorpus from "../../crates/shared/tests/corpus/sql.json";
import stmtCorpus from "../../crates/shared/tests/corpus/statements.json";
import hintCorpus from "../../crates/shared/tests/corpus/valuehints.json";
import resultCorpus from "../../crates/shared/tests/corpus/results.json";
import numberCorpus from "../../crates/shared/tests/corpus/numbers.json";
import tabCorpus from "../../crates/shared/tests/corpus/tablabels.json";
import keyCorpus from "../../crates/shared/tests/corpus/rediskeys.json";
import columnCorpus from "../../crates/shared/tests/corpus/columns.json";
import specCorpus from "../../crates/shared/tests/corpus/keyspecs.json";
import fuzzyCorpus from "../../crates/shared/tests/corpus/fuzzy.json";
import sqlTs from "../../crates/shared/tests/goldens/ts/sql.json";
import stmtRust from "../../crates/shared/tests/goldens/rust/statements.json";
import resultTs from "../../crates/shared/tests/goldens/ts/results.json";
import numberTs from "../../crates/shared/tests/goldens/ts/numbers.json";
import tabTs from "../../crates/shared/tests/goldens/ts/tablabels.json";
import keyTs from "../../crates/shared/tests/goldens/ts/rediskeys.json";
import columnTs from "../../crates/shared/tests/goldens/ts/columns.json";
import fuzzyTs from "../../crates/shared/tests/goldens/ts/fuzzy.json";

// shortcuts.ts reads navigator.platform on import: pin macOS so the goldens are the same on every host (parity_dump.rs uses disp_mac)
vi.hoisted(() => vi.stubGlobal("navigator", { platform: "MacIntel" }));

type Golden = Record<string, Record<string, unknown>>;
const CARET = "‸";

const caret = (text: string) => ({ text: text.replace(CARET, ""), at: text.indexOf(CARET) });
const slices = (text: string, segs: Segment[]) => segs.map((s) => text.slice(s.start, s.end));
/** The console's splitter (ConsolePane): the dialect's own (Redis lines) or splitScript */
const consoleSplit = (kind: DbKind) => (t: string) => dialectOf(kind).split?.(t) ?? splitScript(t, kind);
const plan = (p: ExecPlan) =>
  p === null ? null : "run" in p ? { run: p.run.map((s) => s.sql) } : { ask: p.ask.map((o) => ({ label: o.label, stmts: o.stmts.map((s) => s.sql) })) };

/** Settings combinations dumped for every input with a cursor (same list in parity_dump.rs) */
const PLANS: Record<string, ExecSettings> = {
  ask: DEFAULT_EXEC,
  smallest: { ...DEFAULT_EXEC, inside: "smallest" },
  whole: { ...DEFAULT_EXEC, inside: "whole" },
  fromCaret: { ...DEFAULT_EXEC, inside: "fromCaret" },
  "outside:whole": { ...DEFAULT_EXEC, outside: "whole" },
  "outside:below": { ...DEFAULT_EXEC, outside: "below" },
};

function sqlGolden(): Golden {
  const out: Golden = {};
  for (const e of sqlCorpus) {
    const kind = e.dialect as DbKind;
    const { text, at } = caret(e.text);
    const o: Record<string, unknown> = {};
    if (kind !== "redis") o.splitStatements = slices(text, splitStatements(text, kind));
    o.splitScript = slices(text, consoleSplit(kind)(text));
    if (at >= 0) {
      for (const [name, s] of Object.entries(PLANS)) o[`planExecution(${name})`] = plan(planExecution(text, at, consoleSplit(kind)(text), s, kind));
    }
    out[e.id] = o;
  }
  return out;
}

function stmtGolden(): Golden {
  // Keys keep the names of the GUI's old checks (isReadStmt, DDL_RE) so the goldens and DIVERGENCES.md stay comparable
  return Object.fromEntries(stmtCorpus.map((e) => [e.id, { isReadStmt: isRead(e.dialect as DbKind, e.sql), DDL_RE: isDdl(e.dialect as DbKind, e.sql) }]));
}

function hintGolden(): Golden {
  const out: Golden = {};
  for (const e of hintCorpus) {
    const mysql = "mysql" in e && !!e.mysql;
    const ctx = hintContext(e.before, mysql);
    const o: Record<string, unknown> = {
      hintContext: ctx && { ref: ctx.ref ?? null, column: ctx.column, path: ctx.path, mode: ctx.mode, quoted: ctx.quoted, typed: ctx.typed, op: ctx.op ?? null },
    };
    if (ctx) {
      o.hintInsert = hintInsert(e.value, ctx, e.typeName, mysql);
      o.hintable = hintable(ctx, e.typeName);
      if (ctx.mode === "keys") o.pairsForKeys = pairsForKeys(e.value, ctx);
    }
    out[e.id] = o;
  }
  return out;
}

function resultGolden(): Golden {
  const out: Golden = {};
  for (const e of resultCorpus) {
    const r: QueryResult = { columns: e.columns, rows: e.rows as Cell[][], rowsAffected: 0, truncated: false, elapsedMs: 0, hasResultSet: true };
    const names = r.columns.map((c) => c.name);
    const types = r.columns.map((c) => c.typeName);
    const axes = defaultAxes(r);
    const y = axes.ys[0];
    const idIdx = names.indexOf("id");
    const keys = r.rows.map((row): [string, Cell][] => (idIdx >= 0 ? [["id", row[idIdx]]] : []));
    const base = { names, types, vals: r.rows, keys, tableRef: "MY_TABLE", header: true };
    const o: Record<string, unknown> = {
      numericColumns: numericColumns(r).map((i) => names[i]),
      defaultAxes: { x: names[axes.x] ?? null, y: y === COUNT ? null : names[y] },
      "buildChart(bar)": buildChart(r, axes.x, axes.ys, "bar"),
      "buildChart(pie)": buildChart(r, axes.x, axes.ys, "pie"),
      inList: inList(r.rows, types),
      "inList(mysql)": inList(r.rows, types, "mysql"),
      "inList(column)": Object.fromEntries(names.map((n, c) => [n, inList(r.rows.map((row) => [row[c]]), [types[c]])])),
      "extract(TSV, no header)": extractRows("TSV", { ...base, dialect: dialectOf("postgres"), header: false }),
      "extract(SQL Inserts, mysql)": extractRows("SQL Inserts", { ...base, dialect: dialectOf("mysql") }),
      // As ResultGrid uses it: only time columns get a width
      fracWidth: Object.fromEntries(names.map((n, c) => [n, isTimeType(types[c]) ? fracWidth(r.rows.map((row) => row[c])) : 0])),
      padFrac: Object.fromEntries(
        names.flatMap((n, c) => {
          if (!isTimeType(types[c])) return [];
          const w = fracWidth(r.rows.map((row) => row[c]));
          return [[n, r.rows.map((row) => (typeof row[c] === "string" ? padFrac(row[c] as string, w) : row[c]))]];
        }),
      ),
    };
    for (const fmt of EXTRACTORS as readonly Extractor[]) o[`extract(${fmt})`] = extractRows(fmt, { ...base, dialect: dialectOf("postgres") });
    out[e.id] = o;
  }
  return out;
}

/**
 * Chart outputs in goldens/ts are what the TS chart code (replaced by chart.rs, now called through WASM) wrote, frozen: the dump
 * keeps them instead of writing chart.rs's answers as the expected ones. A corpus row added since has none (see chart.rs tests)
 */
const CHART_KEYS = ["numericColumns", "defaultAxes", "buildChart(bar)", "buildChart(pie)"];
function frozenCharts(g: Golden): Golden {
  for (const [id, o] of Object.entries(g))
    for (const k of CHART_KEYS) {
      const old = (resultTs as Golden)[id]?.[k];
      if (old === undefined) delete o[k];
      else o[k] = old;
    }
  return g;
}

function numberGolden(): Golden {
  const out: Golden = {};
  for (const t of numberCorpus.ticks) out[`ticks:${JSON.stringify(t)}`] = { niceTicks: niceTicks(t[0], t[1], t[2]) };
  for (const n of numberCorpus.shortNumbers) out[`short:${n}`] = { shortNumber: shortNumber(n) };
  return out;
}

function tabGolden(): Golden {
  return Object.fromEntries(
    tabCorpus.map((e) => {
      const m = tableTabLabels(e.tabs as TableTabKey[]);
      return [e.id, { tableTabLabels: e.tabs.map((t) => m.get(t.id) ?? t.table) }];
    }),
  );
}

type Tree = { folder: string; path: string; count: number; children: Tree[] } | { key: string };
const tree = (nodes: KeyNode<string>[]): Tree[] =>
  nodes.map((n) => (n.kind === "key" ? { key: n.item } : { folder: n.name, path: n.path, count: n.count, children: tree(n.children) }));

function keyGolden(): Golden {
  return Object.fromEntries(keyCorpus.map((e) => [e.id, { keyTree: tree(keyTree(e.keys as string[], (k) => k)) }]));
}

function columnGolden(): Golden {
  return Object.fromEntries(columnCorpus.map((e) => [e.id, { autoMap: autoMap(e.file, e.table) }]));
}

/** Storage specs get keyLabel and formatSpec (macOS notation, pinned above), display strings keyLabel only */
function specGolden(): Golden {
  return Object.fromEntries([
    ...specCorpus.specs.map((s) => [s, { keyLabel: keyLabel(s), formatSpec: formatSpec(s) }]),
    ...specCorpus.labels.map((s) => [s, { keyLabel: keyLabel(s) }]),
  ]);
}

function fuzzyGolden(): Golden {
  return Object.fromEntries(fuzzyCorpus.map((e) => [e.id, { fuzzyScore: fuzzyScore(e.query, e.text) }]));
}

const GOLDENS: Record<string, () => Golden> = {
  sql: sqlGolden,
  statements: stmtGolden,
  // completion: the GUI runs qurry_shared::complete through WASM (src/lib/completion.test.ts and goldens/completion/ check both front ends)
  valuehints: hintGolden,
  results: () => frozenCharts(resultGolden()),
  // Chart only, so all frozen (numberGolden is the same through WASM: chart.rs checks it)
  numbers: () => numberTs as Golden,
  tablabels: tabGolden,
  rediskeys: keyGolden,
  columns: columnGolden,
  keyspecs: specGolden,
  fuzzy: fuzzyGolden,
};

/** Key-order independent JSON (serde_json sorts object keys) */
const canon = (v: unknown): string =>
  Array.isArray(v)
    ? `[${v.map(canon).join(",")}]`
    : v && typeof v === "object"
      ? `{${Object.keys(v)
          .sort()
          .map((k) => `${JSON.stringify(k)}:${canon((v as Record<string, unknown>)[k])}`)
          .join(",")}}`
      : JSON.stringify(v);
/** Markdown table cell: a code span fenced longer than any backtick run inside */
const cell = (s: string) => {
  const fence = "`".repeat(Math.max(0, ...(s.match(/`+/g) ?? []).map((m) => m.length)) + 1);
  return `${fence} ${s.replace(/\|/g, "\\|")} ${fence}`;
};

function divergences(ts: Record<string, Golden>, rust: Record<string, Golden>): string {
  const rows: string[] = [];
  const stats = new Map<string, { compared: number; differ: number; tsOnly: number; rustOnly: number }>();
  for (const file of Object.keys(GOLDENS)) {
    const a = ts[file] ?? {};
    const b = rust[file] ?? {};
    for (const id of [...new Set([...Object.keys(a), ...Object.keys(b)])]) {
      const fa = a[id] ?? {};
      const fb = b[id] ?? {};
      for (const fn of [...new Set([...Object.keys(fa), ...Object.keys(fb)])]) {
        const key = `${file} › ${fn}`;
        const st = stats.get(key) ?? { compared: 0, differ: 0, tsOnly: 0, rustOnly: 0 };
        stats.set(key, st);
        if (!(fn in fb)) st.tsOnly++;
        else if (!(fn in fa)) st.rustOnly++;
        else {
          st.compared++;
          if (canon(fa[fn]) !== canon(fb[fn])) {
            st.differ++;
            rows.push(`| ${file} | ${cell(id)} | ${fn} | ${cell(canon(fa[fn]))} | ${cell(canon(fb[fn]))} |`);
          }
        }
      }
    }
  }
  const summary = [...stats].map(([k, s]) => `| ${k} | ${s.compared} | ${s.differ} | ${s.tsOnly} | ${s.rustOnly} |`);
  return [
    "# TS ↔ Rust divergences",
    "",
    "Generated by `pnpm parity:dump` from `goldens/ts` and `goldens/rust` (run `cargo test -p qurry-tui parity_dump -- --ignored` first). Do not edit by hand.",
    "Inputs are in `corpus/` (ids as listed); `‸` in a corpus text is the cursor. Outputs are compact JSON.",
    "`valuehints` is one implementation on both sides: valueHint.ts calls qurry_shared::complete through WASM, so it only guards the WASM wiring.",
    "Rules the TUI and the MCP server took with no option when the grid helpers moved to qurry-shared are not listed here (both sides now agree): see `rules_the_tui_and_mcp_changed_to` in grid_goldens.rs.",
    "",
    "## Summary",
    "",
    "| Corpus › function | Compared | Differ | TS only | Rust only |",
    "| --- | --- | --- | --- | --- |",
    ...summary,
    "",
    "TS only / Rust only: inputs where only one side has that function (no counterpart, or the dialect isn't handled there).",
    "",
    "## Divergences",
    "",
    "| Corpus | Input | Function | TS | Rust |",
    "| --- | --- | --- | --- | --- |",
    ...rows,
    "",
  ].join("\n");
}

interface Fs {
  existsSync(p: URL): boolean;
  mkdirSync(p: URL, o: { recursive: boolean }): void;
  readFileSync(p: URL, enc: "utf8"): string;
  writeFileSync(p: URL, s: string): void;
}

// Runs with plain `pnpm test`: the WASM facades keep what the GUI did before they replaced its TypeScript copies (D2 options)
describe("WASM facades reproduce the goldens", () => {
  it("splitting and execution plans: goldens/ts (the GUI's editor rules are kept, D2)", () => {
    expect(sqlGolden()).toEqual(sqlTs);
  });
  it("read / DDL checks: goldens/rust (D1: the GUI decides like the TUI, e.g. no Production confirmation for select 'delete')", () => {
    expect(stmtGolden()).toEqual(stmtRust);
  });
  it("results: IN lists, Data Extractors, fractional seconds, charts", () => {
    const got = resultGolden();
    // A corpus row added after the chart moved to Rust has no TS chart output to compare with
    for (const [id, o] of Object.entries(got)) for (const k of CHART_KEYS) if (!(k in ((resultTs as Golden)[id] ?? {}))) delete o[k];
    expect(got).toEqual(resultTs);
  });
  it("chart axis numbers", () => expect(numberGolden()).toEqual(numberTs));
  it("table tab labels", () => expect(tabGolden()).toEqual(tabTs));
  it("Redis key folders", () => expect(keyGolden()).toEqual(keyTs));
  it("import auto-map", () => expect(columnGolden()).toEqual(columnTs));
  it("fuzzy scores", () => expect(fuzzyGolden()).toEqual(fuzzyTs));
});

describe.skipIf(!import.meta.env.PARITY_DUMP)("parity dump", () => {
  it("writes TS goldens (and DIVERGENCES.md when Rust goldens exist)", async () => {
    const fs: Fs = await import("node:" + "fs");
    const root = new URL("../../crates/shared/tests/", import.meta.url);
    const tsDir = new URL("goldens/ts/", root);
    const rustDir = new URL("goldens/rust/", root);
    fs.mkdirSync(tsDir, { recursive: true });
    const ts: Record<string, Golden> = {};
    for (const [name, make] of Object.entries(GOLDENS)) {
      ts[name] = make();
      fs.writeFileSync(new URL(`${name}.json`, tsDir), JSON.stringify(ts[name], null, 2) + "\n");
    }
    if (!fs.existsSync(rustDir)) return;
    const rust: Record<string, Golden> = {};
    for (const name of Object.keys(GOLDENS)) {
      const f = new URL(`${name}.json`, rustDir);
      if (fs.existsSync(f)) rust[name] = JSON.parse(fs.readFileSync(f, "utf8"));
    }
    fs.writeFileSync(new URL("DIVERGENCES.md", root), divergences(ts, rust));
  });
});
