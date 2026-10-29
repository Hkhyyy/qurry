/**
 * Find value-suggestion spots (console autocomplete): when the text before the cursor is
 * - `status = '` · `status in ('a', '` · `status like '` · `o.status <> ` → values of that column (values)
 * - `data->>'` · `data ->> ` · `data->'meta'->>'` (MySQL `data->>'$.meta.`) → keys at that json path (keys)
 * - `reasons @> '` → @> conditions of frequent `key: value` (pairs, PostgreSQL). Keyless json (arrays of objects) uses this in keys spots too
 */
export interface HintContext {
  /** alias or table (if absent, find the column among the statement's tables) */
  ref?: string;
  column: string;
  path: string[];
  mode: "values" | "keys" | "pairs";
  /** Typing inside quotes (don't add quotes when inserting) */
  quoted: boolean;
  /** Already typed text (range to replace) */
  typed: string;
  /** keys: from the operator to the cursor (`->> '` + typed) — for replacing with an @> condition */
  op?: string;
}

const ID = String.raw`(?:"[^"]+"|\w+)`;
/** Column + json path (->'a' / ->>'a' / -> 0) */
const EXPR = String.raw`(?:(${ID})\.)?(${ID})((?:\s*->>?\s*(?:'[^']*'|\d+))*)`;
const unq = (s: string) => s.replace(/^"|"$/g, "");

function pathOf(seg: string): string[] {
  const out: string[] = [];
  const re = /->>?\s*(?:'([^']*)'|(\d+))/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(seg))) out.push(m[1] ?? m[2]);
  return out;
}

/** MySQL json path `$.a."b c".d` → ["a", "b c", "d"] */
function myPath(p: string): string[] {
  return [...p.matchAll(/\.(?:"([^"]*)"|(\w+))/g)].map((m) => m[1] ?? m[2]);
}

/** Key spot: `->>'ab` (quoted) · `->> ab` (before the quote) */
const KEYS_RE = new RegExp(String.raw`${EXPR}(\s*->>?\s*(?:'([^']*)|(\w*)))$`);
const MY_KEYS_RE = new RegExp(String.raw`(?:(${ID})\.)?(${ID})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)\.(\w*)$`);
const MY_VALUE_EXPR = String.raw`(?:(${ID})\.)?(${ID})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)'`;
const OP = String.raw`\s*(=|<>|!=|@>|(?:not\s+)?i?like|(?:not\s+)?in\s*\((?:\s*(?:'[^']*'|[\w.-]+)\s*,)*)\s*`;
const VALUE_RE = new RegExp(String.raw`${EXPR}${OP}(?:'([^']*)|([\w.-]*))$`, "i");
const MY_VALUE_RE = new RegExp(String.raw`${MY_VALUE_EXPR}${OP}(?:'([^']*)|([\w.-]*))$`, "i");
const NOT_COLUMN = /^(where|and|or|not|set|on|select|by|when|then|else|case)$/i;

export function hintContext(before: string, mysql = false): HintContext | null {
  let m: RegExpExecArray | null;
  if (mysql && (m = MY_KEYS_RE.exec(before))) return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: myPath(m[3]), mode: "keys", quoted: true, typed: m[4] };
  if (mysql && (m = MY_VALUE_RE.exec(before)))
    return m[4] === "@>" ? null : { ref: m[1] && unq(m[1]), column: unq(m[2]), path: myPath(m[3]), mode: "values", quoted: m[5] !== undefined, typed: m[5] ?? m[6] ?? "" };
  if ((m = KEYS_RE.exec(before))) {
    // MySQL '$.…' is handled above. A '$…' reaching here is not a complete path
    if (m[5]?.startsWith("$") || NOT_COLUMN.test(m[2])) return null;
    return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: pathOf(m[3]), mode: "keys", quoted: m[5] !== undefined, typed: m[5] ?? m[6] ?? "", op: m[4] };
  }
  if ((m = VALUE_RE.exec(before))) {
    // Things after keywords (where = …) aren't columns
    if (NOT_COLUMN.test(m[2])) return null;
    const mode = m[4] === "@>" ? "pairs" : "values";
    if (mysql && mode === "pairs") return null;
    return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: pathOf(m[3]), mode, quoted: m[5] !== undefined, typed: m[5] ?? m[6] ?? "" };
  }
  return null;
}

/** Text to insert: inside quotes, only double '; outside, numeric · boolean column values as is, the rest '…' */
export function hintInsert(value: string, ctx: HintContext, typeName: string): string {
  const esc = value.replace(/'/g, "''");
  if (ctx.quoted) return esc;
  const bare = ctx.mode === "values" && !ctx.path.length && /int|numeric|decimal|float|double|real|serial|bool|bit|money/i.test(typeName);
  return bare ? value : `'${esc}'`;
}

/** For keys spots with no keys (arrays of objects): text replacing it, operator included, with `@> '[{…}]'` */
export function pairsForKeys(value: string, ctx: HintContext): string {
  const lead = /^\s/.test(ctx.op ?? "") ? " " : "";
  return `${lead}@> '${value.replace(/'/g, "''")}'`;
}

/** Columns worth suggesting: excluding binary · spatial · arrays; json only with a path (values) or when looking at keys · conditions */
export function hintable(ctx: HintContext, typeName: string): boolean {
  const t = typeName.toLowerCase();
  const json = /json/.test(t);
  if (ctx.mode !== "values") return json;
  if (/bytea|blob|binary|geometry|geography|tsvector|\[\]|^_/.test(t)) return false;
  return json ? ctx.path.length > 0 : true;
}
