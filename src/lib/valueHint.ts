/**
 * 값 추천 자리 찾기 (콘솔 자동완성): 커서 앞 글이
 * - `status = '` · `status in ('a', '` · `status like '` · `o.status <> ` → 그 컬럼의 값
 * - `data->>'` · `data->'meta'->>'` (MySQL `data->>'$.meta.`) → 그 json 경로의 키
 */
export interface HintContext {
  /** alias 또는 테이블 (없으면 문장의 테이블에서 컬럼으로 찾는다) */
  ref?: string;
  column: string;
  path: string[];
  keys: boolean;
  /** 따옴표 안에서 치는 중 (넣을 때 따옴표를 붙이지 않는다) */
  quoted: boolean;
  /** 이미 친 글자 (바꿀 범위) */
  typed: string;
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

const KEYS_RE = new RegExp(String.raw`${EXPR}\s*->>?\s*'([^']*)$`);
const MY_KEYS_RE = new RegExp(String.raw`(?:(${ID})\.)?(${ID})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)\.(\w*)$`);
const MY_VALUE_EXPR = String.raw`(?:(${ID})\.)?(${ID})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)'`;
const OP = String.raw`\s*(?:=|<>|!=|(?:not\s+)?i?like|(?:not\s+)?in\s*\((?:\s*(?:'[^']*'|[\w.-]+)\s*,)*)\s*`;
const VALUE_RE = new RegExp(String.raw`${EXPR}${OP}(?:'([^']*)|([\w.-]*))$`, "i");
const MY_VALUE_RE = new RegExp(String.raw`${MY_VALUE_EXPR}${OP}(?:'([^']*)|([\w.-]*))$`, "i");

export function hintContext(before: string, mysql = false): HintContext | null {
  let m: RegExpExecArray | null;
  if (mysql && (m = MY_KEYS_RE.exec(before))) return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: myPath(m[3]), keys: true, quoted: true, typed: m[4] };
  if (mysql && (m = MY_VALUE_RE.exec(before))) return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: myPath(m[3]), keys: false, quoted: m[4] !== undefined, typed: m[4] ?? m[5] ?? "" };
  if ((m = KEYS_RE.exec(before))) {
    // MySQL 의 '$.…' 는 위에서. 여기 오는 '$…' 는 다 친 경로가 아니다
    if (m[4].startsWith("$")) return null;
    return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: pathOf(m[3]), keys: true, quoted: true, typed: m[4] };
  }
  if ((m = VALUE_RE.exec(before))) {
    // 키워드 뒤 (where = …) 같은 건 컬럼이 아니다
    if (/^(where|and|or|not|set|on|select|by)$/i.test(m[2])) return null;
    return { ref: m[1] && unq(m[1]), column: unq(m[2]), path: pathOf(m[3]), keys: false, quoted: m[4] !== undefined, typed: m[4] ?? m[5] ?? "" };
  }
  return null;
}

/** 넣을 글: 따옴표 안이면 ' 만 두 번, 밖이면 숫자 · 불리언 컬럼은 그대로, 나머지는 '…' */
export function hintInsert(value: string, ctx: HintContext, typeName: string): string {
  const esc = value.replace(/'/g, "''");
  if (ctx.quoted) return esc;
  const bare = !ctx.path.length && /int|numeric|decimal|float|double|real|serial|bool|bit|money/i.test(typeName);
  return bare ? value : `'${esc}'`;
}

/** 추천할 만한 컬럼: 바이너리 · 공간 · 배열은 빼고, json 은 경로가 있을 때 (값) 나 키를 볼 때만 */
export function hintable(ctx: HintContext, typeName: string): boolean {
  const t = typeName.toLowerCase();
  const json = /json/.test(t);
  if (ctx.keys) return json;
  if (/bytea|blob|binary|geometry|geography|tsvector|\[\]|^_/.test(t)) return false;
  return json ? ctx.path.length > 0 : true;
}
