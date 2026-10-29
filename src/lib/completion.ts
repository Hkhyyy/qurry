import { monaco } from "./monaco";
import { CompletionTable, meta } from "./api";
import { hintable, hintContext, hintInsert, pairsForKeys } from "./valueHint";

/** 콘솔(모델 URI) → 접속 id, 접속 id → 메타 캐시 */
const modelConn = new Map<string, string>();
const cache = new Map<string, Promise<CompletionTable[]>>();

/** 메타를 불러온다. 실패하면 캐시하지 않는다 (다음 요청 때 다시 시도) */
function load(connId: string): Promise<CompletionTable[]> {
  const p: Promise<CompletionTable[]> = meta.completion(connId).catch(() => {
    if (cache.get(connId) === p) cache.delete(connId);
    return [];
  });
  cache.set(connId, p);
  return p;
}

export function bindModel(uri: string, connId: string) {
  modelConn.set(uri, connId);
  if (!cache.has(connId)) load(connId);
}
/** 에디터가 닫힐 때 */
export function unbindModel(uri: string) {
  modelConn.delete(uri);
}
/** Data Source(와 그 DB 들)의 메타를 버린다: 끊기·새로 고침 때 */
export function invalidateCompletion(connId: string) {
  for (const k of [...cache.keys()]) if (k === connId || k.startsWith(connId + "::")) cache.delete(k);
}

const KEYWORDS = (
  "select from where and or not in is null like ilike between exists group by order having limit offset join left right inner outer full cross on using as " +
  "distinct union all intersect except insert into values update set delete returning create table view index alter drop truncate with case when then else end " +
  "asc desc nulls first last count sum avg min max coalesce now current_date current_timestamp interval true false begin commit rollback explain analyze"
).split(" ");

function tablesInStatement(text: string) {
  const out: { ref: string; alias?: string }[] = [];
  const re = /\b(?:from|join|update|into)\s+((?:"[^"]+"|\w+)(?:\.(?:"[^"]+"|\w+))?)(?:\s+(?:as\s+)?(\w+))?/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    const alias = m[2] && !KEYWORDS.includes(m[2].toLowerCase()) ? m[2] : undefined;
    out.push({ ref: m[1].replace(/"/g, ""), alias });
  }
  return out;
}

function findTable(all: CompletionTable[], ref: string) {
  const [a, b] = ref.toLowerCase().split(".");
  return b ? all.find((t) => t.schema.toLowerCase() === a && t.name.toLowerCase() === b) : all.find((t) => t.name.toLowerCase() === a && t.schema === "public") ?? all.find((t) => t.name.toLowerCase() === a);
}

for (const lang of ["pgsql", "mysql"]) {
  monaco.languages.registerCompletionItemProvider(lang, {
    // 값 · json 키 추천: 따옴표 · '->>' 뒤 · '=' 뒤 빈칸에서 바로 (그 밖의 빈칸 · '>' 에서는 열지 않는다)
    triggerCharacters: [".", "'", " ", ">"],
    async provideCompletionItems(model, position, context) {
      const connId = modelConn.get(model.uri.toString());
      const all = connId ? await (cache.get(connId) ?? load(connId)) : [];
      const K = monaco.languages.CompletionItemKind;
      const quiet = context.triggerKind === monaco.languages.CompletionTriggerKind.TriggerCharacter && (context.triggerCharacter === " " || context.triggerCharacter === ">");

      // 값 · json 키: where status = '… / data->>'… / reasons @> '…
      const lineText = model.getLineContent(position.lineNumber);
      const ctx = hintContext(lineText.slice(0, position.column - 1), lang === "mysql");
      if (ctx && connId) {
        const refs = tablesInStatement(model.getValue());
        const named = ctx.ref ? refs.find((r) => r.alias?.toLowerCase() === ctx.ref!.toLowerCase()) ?? refs.find((r) => r.ref.split(".").pop()!.toLowerCase() === ctx.ref!.toLowerCase()) : undefined;
        const cands = (named ? [named] : ctx.ref ? [] : refs).map((r) => findTable(all, r.ref)).filter((t): t is CompletionTable => !!t);
        const t = cands.find((x) => x.columns.some(([c]) => c.toLowerCase() === ctx.column.toLowerCase()));
        const col = t?.columns.find(([c]) => c.toLowerCase() === ctx.column.toLowerCase());
        if (t && col && hintable(ctx, col[1])) {
          const ask = (mode: "values" | "keys" | "pairs") => meta.hints(connId, { schema: t.schema, table: t.name, column: col[0], path: ctx.path, keys: mode === "keys", pairs: mode === "pairs" }).catch(() => []);
          let hints = await ask(ctx.mode);
          // 키가 없는 json (객체의 배열 등): 키 대신 많이 나온 키: 값을 @> 조건으로 (연산자째 바꾼다)
          const asPairs = ctx.mode === "keys" && !hints.length && lang !== "mysql";
          if (asPairs) hints = await ask("pairs");
          const rows = (n: number) => `${n.toLocaleString("en-US")} row${n === 1 ? "" : "s"}`;
          const range = new monaco.Range(position.lineNumber, position.column - ctx.typed.length, position.lineNumber, position.column);
          // 따옴표가 저절로 닫혀 있으면 그것까지 바꾼다
          const closing = lineText[position.column - 1] === "'" ? 1 : 0;
          const opRange = new monaco.Range(position.lineNumber, position.column - (ctx.op?.length ?? 0), position.lineNumber, position.column + closing);
          return {
            suggestions: hints.map((h, i) => ({
              label: { label: asPairs ? `@> '${h.value}'` : h.value, description: `${rows(h.count)}${ctx.mode === "keys" && !asPairs ? " · key" : ""}` },
              kind: ctx.mode === "keys" && !asPairs ? K.Property : K.Value,
              insertText: asPairs ? pairsForKeys(h.value, ctx) : hintInsert(h.value, ctx, col[1]),
              filterText: asPairs ? (ctx.op ?? "") + h.value : h.value,
              sortText: String(i).padStart(4, "0"),
              range: asPairs ? opRange : range,
            })),
          };
        }
        // 따옴표 안: 키워드 · 테이블은 맞지 않는다
        if (ctx.quoted) return { suggestions: [] };
      }
      if (quiet) return { suggestions: [] };
      const word = model.getWordUntilPosition(position);
      const range = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
      const line = model.getLineContent(position.lineNumber).slice(0, word.startColumn - 1);
      const text = model.getValue();
      const dot = /((?:"[^"]+"|\w+))\.$/.exec(line);

      if (dot) {
        const q = dot[1].replace(/"/g, "");
        // alias. 또는 table. → 컬럼
        const refs = tablesInStatement(text);
        const hit = refs.find((r) => r.alias?.toLowerCase() === q.toLowerCase()) ?? refs.find((r) => r.ref.split(".").pop()!.toLowerCase() === q.toLowerCase());
        const t = hit ? findTable(all, hit.ref) : undefined;
        if (t) {
          return { suggestions: t.columns.map(([c, ty], i) => ({ label: { label: c, description: ty }, kind: K.Field, insertText: c, range, sortText: String(i).padStart(4, "0") })) };
        }
        // schema. → 테이블
        const inSchema = all.filter((x) => x.schema.toLowerCase() === q.toLowerCase());
        return { suggestions: inSchema.map((x) => ({ label: { label: x.name, description: `${x.columns.length} cols` }, kind: K.Class, insertText: x.name, range })) };
      }

      const referenced = tablesInStatement(text).map((r) => findTable(all, r.ref)).filter(Boolean) as CompletionTable[];
      const cols = new Map<string, string>();
      for (const t of referenced) for (const [c, ty] of t.columns) if (!cols.has(c)) cols.set(c, `${ty} · ${t.name}`);
      const schemas = [...new Set(all.map((t) => t.schema))];
      const suggestions: monaco.languages.CompletionItem[] = [
        ...[...cols].map(([c, d]) => ({ label: { label: c, description: d }, kind: K.Field, insertText: c, range, sortText: "0" + c })),
        ...all.map((t) => ({
          label: { label: t.name, description: t.schema },
          kind: K.Class,
          insertText: t.schema === "public" ? t.name : `${t.schema}.${t.name}`,
          filterText: t.name,
          range,
          sortText: "1" + t.name,
        })),
        ...schemas.map((s) => ({ label: s, kind: K.Module, insertText: s, range, sortText: "2" + s })),
        ...KEYWORDS.map((k) => ({ label: k, kind: K.Keyword, insertText: k, range, sortText: "3" + k })),
      ];
      return { suggestions };
    },
  });
}
