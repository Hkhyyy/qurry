/**
 * 값 추천 자리 찾기 (콘솔 자동완성): 커서 앞 글이
 * - `status = '` · `status in ('a', '` · `status like '` · `o.status <> ` → 그 컬럼의 값 (values)
 * - `data->>'` · `data ->> ` · `data->'meta'->>'` (MySQL `data->>'$.meta.`) → 그 json 경로의 키 (keys)
 * - `reasons @> '` → 많이 나온 `키: 값` 의 @> 조건 (pairs, PostgreSQL). 키가 없는 json (객체의 배열) 은 keys 자리에서도 이것으로
 */
export interface HintContext {
  /** alias 또는 테이블 (없으면 문장의 테이블에서 컬럼으로 찾는다) */
  ref?: string;
  column: string;
  path: string[];
  mode: "values" | "keys" | "pairs";
  /** 따옴표 안에서 치는 중 (넣을 때 따옴표를 붙이지 않는다) */
  quoted: boolean;
  /** 이미 친 글자 (바꿀 범위) */
  typed: string;
  /** keys: 연산자부터 커서까지 (`->> '` + typed) — @> 조건으로 바꿔 넣을 때 */
  op?: string;
}

const ID = String.raw`(?:"[^"]+"|\w+)`;
/** 컬럼 + json 경로 (->'a' / ->>'a' / -> 0) */
const EXPR = String.raw`(?:(${ID})\.)?(${ID})((?:\s*->>?\s*(?:'[^']*'|\d+))*)`;
const unq = (s: string) => s.replace(/^"|"$/g, "");

function pathOf(seg: string): string[] {
  const out: string[] = [];
  const re = /->>?\s*(?:'([^']*)'|(\d+))/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(seg))) out.push(m[1] ?? m[2]);
  return out;
}

/** MySQL json 경로 `$.a."b c".d` → ["a", "b c", "d"] */
function myPath(p: string): string[] {
  return [...p.matchAll(/\.(?:"([^"]*)"|(\w+))/g)].map((m) => m[1] ?? m[2]);
}

/** 키 자리: `->>'ab` (quoted) · `->> ab` (따옴표 전) */
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
    // MySQL 의 '$.…' 는 위에서. 여기 오는 '$…' 는 다 친 경로가 아니다
    if (m[5]?.startsWith("$") || NOT_COLUMN.test(m[2])) return null;
    return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: pathOf(m[3]), mode: "keys", quoted: m[5] !== undefined, typed: m[5] ?? m[6] ?? "", op: m[4] };
  }
  if ((m = VALUE_RE.exec(before))) {
    // 키워드 뒤 (where = …) 같은 건 컬럼이 아니다
    if (NOT_COLUMN.test(m[2])) return null;
    const mode = m[4] === "@>" ? "pairs" : "values";
    if (mysql && mode === "pairs") return null;
    return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: pathOf(m[3]), mode, quoted: m[5] !== undefined, typed: m[5] ?? m[6] ?? "" };
  }
  return null;
}

/** 넣을 글: 따옴표 안이면 ' 만 두 번, 밖이면 숫자 · 불리언 컬럼 값은 그대로, 나머지는 '…' */
export function hintInsert(value: string, ctx: HintContext, typeName: string): string {
  const esc = value.replace(/'/g, "''");
  if (ctx.quoted) return esc;
  const bare = ctx.mode === "values" && !ctx.path.length && /int|numeric|decimal|float|double|real|serial|bool|bit|money/i.test(typeName);
  return bare ? value : `'${esc}'`;
}

/** keys 자리에서 키가 없을 때 (객체의 배열): 연산자째 `@> '[{…}]'` 로 바꿔 넣는 글 */
export function pairsForKeys(value: string, ctx: HintContext): string {
  const lead = /^\s/.test(ctx.op ?? "") ? " " : "";
  return `${lead}@> '${value.replace(/'/g, "''")}'`;
}

/** 추천할 만한 컬럼: 바이너리 · 공간 · 배열은 빼고, json 은 경로가 있을 때 (값) 나 키 · 조건을 볼 때만 */
export function hintable(ctx: HintContext, typeName: string): boolean {
  const t = typeName.toLowerCase();
  const json = /json/.test(t);
  if (ctx.mode !== "values") return json;
  if (/bytea|blob|binary|geometry|geography|tsvector|\[\]|^_/.test(t)) return false;
  return json ? ctx.path.length > 0 : true;
}
