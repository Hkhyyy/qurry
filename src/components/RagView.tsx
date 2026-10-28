import { useEffect, useMemo, useState } from "react";
import { ask } from "../lib/dialogs";
import { ConnectionView, dialectOf, KIND_LABEL, meta, QueryResult, rag, RAG_DEFAULT_MODEL, RagIndex, targetId, VECTOR_KINDS } from "../lib/api";
import { ResultGrid } from "./ResultGrid";
import { Button, EmptyState, List, ListItem, Notice, Row, Select, Spacer, Text, TextInput } from "../ui";

/** 원본이 될 수 있는 SQL Data Source */
const isSqlSource = (c: ConnectionView) => c.kind !== "redis" && c.kind !== "mongodb" && !VECTOR_KINDS.includes(c.kind);
/** 벡터를 넣을 수 있는 곳 */
const isStore = (c: ConnectionView) => c.kind === "postgres" || VECTOR_KINDS.includes(c.kind);
const FLAT = ["mysql", "sqlite", "clickhouse"];
/** 접속 대상: 기본 DB 면 Data Source id 그대로 */
const tid = (c: ConnectionView, db?: string) => targetId(c.id, db && db !== (c.database || (c.kind === "postgres" ? "postgres" : "")) ? db : undefined);
const isText = (t: string) => /char|text|string|clob|json|xml/i.test(t);

const blank = (connId: string, db?: string, schema?: string, table?: string): Draft => ({
  id: "",
  name: table ? `${table} search` : "",
  source: connId,
  schema: schema ?? "",
  table: table ?? "",
  idColumn: "",
  textColumns: [],
  provider: "ollama",
  model: "",
  baseUrl: "",
  dest: "",
  collection: table ? `${table}_embeddings` : "",
  _db: db,
});

/** 고르는 중인 원본 DB (저장 형식에는 없다: source 의 conn::db 로 들어간다) */
type Draft = RagIndex & { _db?: string };

function IndexForm({ conns, init, hasKey, onKeySaved, onSaved, onCancel }: { conns: ConnectionView[]; init: Draft; hasKey: boolean; onKeySaved: () => void; onSaved: (x: RagIndex) => void; onCancel: () => void }) {
  const [d, setD] = useState<Draft>(init);
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setD((x) => ({ ...x, [k]: v }));
  const srcConnId = d.source.split("::")[0];
  const sc = conns.find((x) => x.id === srcConnId);
  const db = d._db ?? d.source.split("::")[1] ?? "";
  const [dbs, setDbs] = useState<string[]>([]);
  const [schemas, setSchemas] = useState<string[]>([]);
  const [tables, setTables] = useState<string[]>([]);
  const [cols, setCols] = useState<{ name: string; dataType: string; isPk: boolean }[]>([]);
  const [key, setKey] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const flat = !!sc && FLAT.includes(sc.kind);
  useEffect(() => {
    if (sc) meta.databases(sc.id).then(setDbs, () => setDbs([]));
  }, [srcConnId]);
  useEffect(() => {
    if (!sc || flat || !db) return setSchemas([]);
    meta.schemas(tid(sc, db)).then((s) => setSchemas(s.filter((x) => !x.isSystem).map((x) => x.name)), () => setSchemas([]));
  }, [srcConnId, db]);
  const schema = flat ? db : d.schema;
  useEffect(() => {
    if (!sc || !schema) return setTables([]);
    meta.objects(tid(sc, db), schema).then((o) => setTables(o.relations.filter((r) => r.kind !== "sequence").map((r) => r.name)), () => setTables([]));
  }, [srcConnId, db, schema]);
  useEffect(() => {
    if (!sc || !schema || !d.table) return setCols([]);
    meta.table(tid(sc, db), schema, d.table).then(
      (t) => {
        setCols(t.columns);
        // 처음 고른 테이블: PK 와 글 열을 미리 고른다
        setD((x) => (x.idColumn && x.textColumns.length ? x : { ...x, idColumn: x.idColumn || t.columns.find((c) => c.isPk)?.name || "", textColumns: x.textColumns.length ? x.textColumns : t.columns.filter((c) => !c.isPk && isText(c.dataType)).map((c) => c.name) }));
      },
      () => setCols([]),
    );
  }, [srcConnId, db, schema, d.table]);
  const dc = conns.find((x) => x.id === d.dest.split("::")[0]);
  const save = async () => {
    setErr(null);
    try {
      const { _db, ...x } = d;
      onSaved(await rag.save({ ...x, source: sc ? tid(sc, db) : x.source, schema, model: x.model || RAG_DEFAULT_MODEL[x.provider] }));
    } catch (e) {
      setErr(String(e));
    }
  };
  return (
    <div className="rag-form">
      <label>Name</label>
      <TextInput value={d.name} placeholder="e.g. support tickets" onChange={(e) => set("name", e.target.value)} />
      <label>Source</label>
      <Row gap={3} className="mig-end">
        <Select value={srcConnId} onChange={(e) => setD((x) => ({ ...x, source: e.target.value, _db: "", schema: "", table: "", idColumn: "", textColumns: [] }))}>
          <option value="">— data source —</option>
          {conns.filter(isSqlSource).map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </Select>
        <Select value={db} onChange={(e) => setD((x) => ({ ...x, _db: e.target.value, schema: "", table: "", idColumn: "", textColumns: [] }))}>
          <option value="">— database —</option>
          {dbs.map((x) => (
            <option key={x}>{x}</option>
          ))}
        </Select>
        {!flat && (
          <Select value={d.schema} onChange={(e) => setD((x) => ({ ...x, schema: e.target.value, table: "", idColumn: "", textColumns: [] }))}>
            <option value="">— schema —</option>
            {schemas.map((x) => (
              <option key={x}>{x}</option>
            ))}
          </Select>
        )}
        <Select value={d.table} onChange={(e) => setD((x) => ({ ...x, table: e.target.value, idColumn: "", textColumns: [], collection: x.collection || `${e.target.value}_embeddings`, name: x.name || `${e.target.value} search` }))}>
          <option value="">— table —</option>
          {tables.map((x) => (
            <option key={x}>{x}</option>
          ))}
        </Select>
      </Row>
      <label>Row ID</label>
      <Select value={d.idColumn} onChange={(e) => set("idColumn", e.target.value)}>
        <option value="">— column that identifies a row —</option>
        {cols.map((c) => (
          <option key={c.name} value={c.name}>
            {c.name}
            {c.isPk ? " (PK)" : ""}
          </option>
        ))}
      </Select>
      <label>Text to embed</label>
      <div className="rag-cols">
        {cols.length === 0 && <Text tone="muted">Pick a table</Text>}
        {cols.map((c) => (
          <label key={c.name} className="rag-col">
            <input type="checkbox" checked={d.textColumns.includes(c.name)} onChange={(e) => set("textColumns", e.target.checked ? [...d.textColumns, c.name] : d.textColumns.filter((x) => x !== c.name))} />
            {c.name} <Text tone="muted">{c.dataType}</Text>
          </label>
        ))}
      </div>
      <label>Embeddings</label>
      <Row gap={3} className="mig-end">
        <Select value={d.provider} onChange={(e) => set("provider", e.target.value as RagIndex["provider"])}>
          <option value="ollama">Ollama (runs on your computer)</option>
          <option value="openai">OpenAI</option>
        </Select>
        <TextInput grow value={d.model} placeholder={RAG_DEFAULT_MODEL[d.provider]} onChange={(e) => set("model", e.target.value)} />
        <TextInput grow value={d.baseUrl} placeholder={d.provider === "ollama" ? "http://localhost:11434" : "https://api.openai.com/v1"} onChange={(e) => set("baseUrl", e.target.value)} />
      </Row>
      {d.provider === "openai" && (
        <>
          <label>API key</label>
          <Row gap={3} className="mig-end">
            <TextInput type="password" grow value={key} placeholder={hasKey ? "<saved — type to replace>" : "sk-…"} onChange={(e) => setKey(e.target.value)} />
            <Button
              disabled={!key}
              onClick={async () => {
                await rag.setKey(key);
                setKey("");
                onKeySaved();
              }}
            >
              Save Key
            </Button>
          </Row>
          <span />
          <Text tone="muted" className="rag-note">The text of every row is sent to OpenAI to make its embedding. Use Ollama to keep it on this computer.</Text>
        </>
      )}
      <label>Store in</label>
      <Row gap={3} className="mig-end">
        <Select value={d.dest.split("::")[0]} onChange={(e) => set("dest", e.target.value)}>
          <option value="">— where to keep the vectors —</option>
          {conns.filter(isStore).map((c) => (
            <option key={c.id} value={c.id}>
              {c.name} · {KIND_LABEL[c.kind]}
            </option>
          ))}
        </Select>
        <TextInput grow value={d.collection} placeholder={dc?.kind === "postgres" ? "table, e.g. public.docs_embeddings" : "collection"} onChange={(e) => set("collection", e.target.value)} />
      </Row>
      {err && (
        <>
          <span />
          <Notice tone="danger">{err}</Notice>
        </>
      )}
      <span />
      <Row className="mig-actions">
        <Button variant="primary" disabled={!d.name || !d.table || !d.idColumn || !d.textColumns.length || !d.dest || !d.collection} onClick={save}>
          Save
        </Button>
        <Button onClick={onCancel}>Cancel</Button>
        <Text tone="muted">Saving doesn't embed anything yet — press Build.</Text>
      </Row>
    </div>
  );
}

/** AI Search (RAG) 탭: 인덱스 목록 · 만들기/고치기 · Build (임베딩) · 의미 검색 */
export function RagView(p: {
  conns: ConnectionView[];
  initial?: { connId: string; db?: string; schema?: string; table?: string };
  runJob: <T>(label: string, run: (job: string) => Promise<T>) => Promise<T | undefined>;
  onStatus: (s: string) => void;
}) {
  const [list, setList] = useState<RagIndex[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [editing, setEditing] = useState<Draft | null>(p.initial?.table ? blank(p.initial.connId, p.initial.db, p.initial.schema, p.initial.table) : null);
  const [hasKey, setHasKey] = useState(false);
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<QueryResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const reload = () =>
    rag.list().then((l) => {
      setList(l);
      setSel((s) => s ?? l[0]?.id ?? null);
    }, (e) => setErr(String(e)));
  useEffect(() => {
    reload();
    rag.hasKey().then(setHasKey, () => {});
  }, []);
  const cur = list.find((x) => x.id === sel);
  const srcConn = cur && p.conns.find((c) => c.id === cur.source.split("::")[0]);
  const dialect = useMemo(() => dialectOf(srcConn?.kind), [srcConn?.kind]);
  const search = async () => {
    if (!cur || !query.trim()) return;
    setBusy(true);
    setErr(null);
    try {
      setResult(await rag.search(cur.id, query.trim()));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };
  const build = async () => {
    if (!cur) return;
    const n = await p.runJob(`Embed ${cur.name}`, (job) => rag.build(cur.id, job));
    if (n !== undefined) p.onStatus(`Embedded ${n.toLocaleString()} rows into ${cur.collection}`);
    reload();
  };
  const empty: QueryResult = { columns: [], rows: [], rowsAffected: 0, truncated: false, elapsedMs: 0, hasResultSet: true };
  return (
    <div className="mig rag">
      <div className="rag-body">
        <div className="rag-list">
          <List>
            {list.map((x) => (
              <ListItem
                key={x.id}
                title={<b>{x.name}</b>}
                title2={x.name}
                subtitle={x.rows ? `${x.rows.toLocaleString()} rows · ${x.builtAt}` : "not built yet"}
                selected={x.id === sel && !editing}
                onClick={() => {
                  setSel(x.id);
                  setEditing(null);
                  setResult(null);
                  setErr(null);
                }}
              />
            ))}
          </List>
          <Button className="rag-new" onClick={() => setEditing(blank(p.initial?.connId ?? "", p.initial?.db, p.initial?.schema))}>
            + New Index…
          </Button>
        </div>
        <div className="rag-main">
          {editing ? (
            <IndexForm
              key={editing.id || "new"}
              conns={p.conns}
              init={editing}
              hasKey={hasKey}
              onKeySaved={() => { setHasKey(true); p.onStatus("OpenAI API key saved (encrypted)"); }}
              onSaved={(x) => { setEditing(null); setSel(x.id); reload(); p.onStatus(`Saved "${x.name}" — press Build to embed the rows`); }}
              onCancel={() => setEditing(null)}
            />
          ) : cur ? (
            <>
              <Row className="mig-actions">
                <Text tone="muted">
                  {cur.schema}.{cur.table} ({cur.textColumns.join(", ")}) → {cur.collection} · {cur.provider} {cur.model}
                  {cur.dim ? ` · ${cur.dim} dims` : ""}
                </Text>
                <Spacer />
                <Button variant="primary" onClick={build}>{cur.rows ? "Rebuild" : "Build"}</Button>
                <Button onClick={() => setEditing({ ...cur, _db: cur.source.split("::")[1] })}>Edit…</Button>
                <Button
                  onClick={async () => {
                    if (!(await ask(`Delete the index "${cur.name}"? The stored vectors in ${cur.collection} are kept.`, { okLabel: "Delete", danger: true }))) return;
                    await rag.remove(cur.id);
                    setSel(null);
                    reload();
                  }}
                >
                  Delete
                </Button>
              </Row>
              <Row className="mig-actions">
                <TextInput
                  grow
                  className="rag-q"
                  value={query}
                  placeholder={cur.rows ? "Ask in plain words, e.g. customers unhappy about late delivery" : "Build the index first"}
                  disabled={!cur.rows}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") search(); e.stopPropagation(); }}
                />
                <Button variant="primary" disabled={busy || !query.trim() || !cur.rows} onClick={search}>
                  {busy ? "Searching…" : "Search"}
                </Button>
              </Row>
              {err && <Notice tone="danger">{err}</Notice>}
              <div className="dba-grid">
                <ResultGrid result={result ?? empty} readOnly dialect={dialect} running={busy && !result} loadingMore={false} query={null} onFetchMore={() => {}} onSubmit={async () => {}} onReload={search} onCancel={() => {}} onExport={() => {}} />
              </div>
            </>
          ) : (
            <EmptyState>
              Make an index from a table: its text columns are turned into embeddings (with Ollama on this computer, or OpenAI) and stored in pgvector or a vector DB. Then search it in plain words — and from Claude with the
              qurry MCP server's semantic_search tool.
            </EmptyState>
          )}
        </div>
      </div>
    </div>
  );
}
