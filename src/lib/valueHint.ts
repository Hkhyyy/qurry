/**
 * Value-suggestion spots (console autocomplete): when the text before the cursor is
 * - `status = '` · `status in ('a', '` · `status like '` · `o.status <> ` → values of that column (values)
 * - `data->>'` · `data ->> ` · `data->'meta'->>'` (MySQL `data->>'$.meta.`) → keys at that json path (keys)
 * - `reasons @> '` → @> conditions of frequent `key: value` (pairs, PostgreSQL). Keyless json (arrays of objects) uses this in keys spots too
 *
 * The rules are qurry_shared::complete, through the shared WASM (completion.ts uses its CompletionIndex directly); these are its single-spot helpers.
 */
import { shared, type HintContext } from "./shared";

export type { HintContext };

export function hintContext(before: string, mysql = false): HintContext | null {
  const c = shared().hintContext(before, mysql);
  // op only at keys spots, as before
  return c ? { ...c, op: c.op || undefined } : null;
}

/** Text to insert: inside quotes, only double ' (and \ on MySQL); outside, numeric · boolean column values as is, the rest '…' */
export function hintInsert(value: string, ctx: HintContext, typeName: string, mysql = false): string {
  return shared().hintInsert(value, ctx, typeName, mysql);
}

/** For keys spots with no keys (arrays of objects): text replacing it, operator included, with `@> '[{…}]'` */
export function pairsForKeys(value: string, ctx: HintContext): string {
  return shared().pairsInsert(value, ctx.op ?? "");
}

/** Columns worth suggesting: excluding binary · spatial · arrays; json only with a path (values) or when looking at keys · conditions */
export function hintable(ctx: HintContext, typeName: string): boolean {
  return shared().hintable(ctx, typeName);
}
