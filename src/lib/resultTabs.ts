// A console's result tabs follow its statements (#151): one tab per statement, kept while the statement is in the editor, at most
// `cap` unpinned per console (Settings › Result tabs). The TUI keeps the same rules in crates/tui/src/app/result_tabs.rs
import type { ResultEntry } from "../components/ConsolePane";

/** What the model works on: a console tab's results and the one shown ("output" or an entry id) */
export interface Tabs {
  results: ResultEntry[];
  activeResult: string;
  /** The run in progress: its statements, their places followed through edits (edited) until their results come */
  pending?: RunStmt[];
}

/** A statement to run and its place in the script (null: not one of its statements, e.g. part of one selected). `into`: the tab it
 *  runs again for (↻, submitted edits): its result goes there in place */
export interface RunStmt {
  sql: string;
  stmt: number | null;
  into?: string;
}

/** What a run puts in a tab */
export type Content = Pick<ResultEntry, "title" | "sql" | "kind" | "result" | "plan" | "planJson">;

/** Newer than any tab's last run or show (the least recently shown closes first) */
const tick = (s: Tabs) => s.results.reduce((n, r) => Math.max(n, r.used), 0) + 1;

/** The statement's unpinned tab, else its pinned one */
export const tabOf = (s: Tabs, stmt: number) => s.results.find((r) => r.stmt === stmt && !r.pinned) ?? s.results.find((r) => r.stmt === stmt);

/**
 * Statement `stmt` (its place in the script; null for a run that isn't one of its statements, e.g. part of one selected) returned
 * `c`: tab `into` (a rerun of it, pinned or not) or else the statement's unpinned tab gets it and keeps its id; none (or only a pinned
 * one): a new tab `id`, bound to the statement. Shown either way
 */
export function ran<S extends Tabs>(s: S, cap: number, stmt: number | null, id: string, c: Content, into?: string): S {
  const used = tick(s);
  const own = s.results.find((r) => r.id === into) ?? (stmt === null ? undefined : s.results.find((r) => r.stmt === stmt && !r.pinned));
  if (own) return { ...s, activeResult: own.id, results: s.results.map((r) => (r.id === own.id ? { ...r, plan: undefined, planJson: undefined, ...c, used } : r)) };
  // A pinned tab of this statement stays as it is and no longer follows the statement: the new tab does
  const results = s.results.map((r) => (stmt !== null && r.stmt === stmt ? { ...r, stmt: null } : r));
  const num = stmt === null ? s.results.reduce((n, r) => Math.max(n, r.num), 0) + 1 : stmt + 1;
  return prune({ ...s, activeResult: id, results: [...results, { id, stmt, num, ...c, pinned: false, used }] }, cap);
}

/** Over the cap, close the least recently shown unpinned tabs (never the shown one); pinned tabs don't count */
export function prune<S extends Tabs>(s: S, cap: number): S {
  let results = s.results;
  while (results.filter((r) => !r.pinned).length > Math.max(cap, 1)) {
    const lru = results.filter((r) => !r.pinned && r.id !== s.activeResult).reduce((a, b) => (b.used < a.used ? b : a));
    results = results.filter((r) => r.id !== lru.id);
  }
  return results === s.results ? s : { ...s, results };
}

/** Show a tab (or "output") */
export function show<S extends Tabs>(s: S, id: string): S {
  if (id === "output") return s.activeResult === id ? s : { ...s, activeResult: id };
  const used = tick(s);
  return { ...s, activeResult: id, results: s.results.map((r) => (r.id === id ? { ...r, used } : r)) };
}

/** The caret moved into statement `stmt`: its tab is shown; a statement without one (or null) leaves things as they are */
export function caret<S extends Tabs>(s: S, stmt: number | null): S {
  const t = stmt === null ? undefined : tabOf(s, stmt);
  return t && t.id !== s.activeResult ? show(s, t.id) : s;
}

/** The changed middle of an edit: how many statements at the start (p) and at the end (q) have the same texts before and after */
function unchanged(before: string[], after: string[]) {
  let p = 0;
  while (p < before.length && p < after.length && before[p] === after[p]) p++;
  let q = 0;
  while (q < before.length - p && q < after.length - p && before[before.length - 1 - q] === after[after.length - 1 - q]) q++;
  return { p, q };
}

/**
 * Map old statement places to new ones after an edit, by their texts: the same leading and trailing statements keep their places; in
 * the changed middle, statements are paired in order (an edited statement keeps its tab) and the old ones left over are gone (null),
 * unless one of the middle's statements now holds its text (two merged while a statement is typed above another without its `;` yet)
 */
export function remap(before: string[], after: string[]): (number | null)[] {
  const { p, q } = unchanged(before, after);
  const mid = after.length - p - q;
  const into = (i: number) => {
    const t = before[i].trim();
    for (let k = after.length - q - 1; t && k >= p; k--) if (after[k].includes(t)) return k;
    return null;
  };
  return before.map((_, i) => (i < p ? i : i >= before.length - q ? i - before.length + after.length : i - p < mid ? i : into(i)));
}

/** A statement's text as it is compared with what a tab ran (without the closing semicolon) */
export const bare = (sql: string) => sql.trim().replace(/;$/, "").trimEnd();

/**
 * The script was edited: tabs follow their statements (remap); a statement that is gone closes its unpinned tab (a pinned one stays,
 * its number frozen, and so does tab `keep`: the shown grid with pending edits). A tab whose statement now reads differently goes to
 * the statement that reads exactly as it ran, if one has no tab yet: in the changed middle (a statement typed above it, split off on
 * its `;`), anywhere once its own is gone (the first of two identical statements deleted). A tab left on a statement that another tab
 * ran exactly as it reads now closes (its own was merged into that one, then deleted). The run in progress follows the same way. The
 * same object when nothing moved
 */
export function edited<S extends Tabs>(s: S, before: string[], after: string[], keep?: string): S {
  const map = remap(before, after);
  const { p, q } = unchanged(before, after);
  const target = (stmt: number | null) => (stmt === null ? null : (map[stmt] ?? null));
  const taken = new Set(s.results.map((r) => target(r.stmt)));
  /** Where what ran `sql` as statement `stmt` goes; `tab`: one per statement (a statement still running takes none) */
  const follow = (stmt: number | null, sql: string, tab: boolean) => {
    let to = target(stmt);
    if (stmt !== null && (to === null || bare(after[to]) !== bare(sql))) {
      const gone = to === null;
      const j = after.findIndex((t, k) => (gone || (k >= p && k < after.length - q)) && !(tab && taken.has(k)) && bare(t) === bare(sql));
      if (j >= 0) {
        if (tab) taken.add(j);
        to = j;
      }
    }
    return to;
  };
  const next = s.results.map((r) => ({ r, to: follow(r.stmt, r.sql, true) }));
  const exact = new Set(next.filter((x) => x.to !== null && bare(after[x.to]) === bare(x.r.sql)).map((x) => x.to));
  for (const x of next) if (x.to !== null && exact.has(x.to) && bare(after[x.to]) !== bare(x.r.sql)) x.to = null;
  let moved = false;
  const results: ResultEntry[] = [];
  for (const { r, to } of next) {
    if (to === r.stmt) {
      results.push(r);
      continue;
    }
    moved = true;
    if (to === null && !r.pinned && r.id !== keep) continue;
    results.push({ ...r, stmt: to, num: to === null ? r.num : to + 1 });
  }
  const pending = s.pending?.map((x) => {
    const to = follow(x.stmt, x.sql, false);
    return to === x.stmt ? x : { ...x, stmt: to };
  });
  if (!moved && !pending?.some((x, i) => x !== s.pending![i])) return s;
  const activeResult = s.activeResult === "output" || results.some((r) => r.id === s.activeResult) ? s.activeResult : "output";
  return { ...s, results: moved ? results : s.results, activeResult, pending };
}
