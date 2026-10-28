import { useEffect, useMemo, useState } from "react";
import { ask, askText } from "../lib/dialogs";
import { ConnectionView, DataDiff, meta, migrate, Migration, SchemaChange, targetId } from "../lib/api";
import { Button, Notice, Select, Tabs } from "../ui";

/** 비교·마이그레이션 대상: Data Source · DB · 스키마 (· 테이블) */
interface End {
  connId: string;
  db: string;
  schema: string;
  table: string;
}

const isSql = (c: ConnectionView) => c.kind === "postgres" || c.kind === "mysql";
/** 접속 대상 id: PG 는 conn::db, MySQL 은 DB 가 곧 스키마 */
const tid = (c: ConnectionView | undefined, e: End) => (c ? targetId(c.id, c.kind === "postgres" && e.db && e.db !== (c.database || "postgres") ? e.db : undefined) : "");
const schemaOf = (c: ConnectionView | undefined, e: End) => (c?.kind === "mysql" ? e.db : e.schema);

function EndPicker({ label, conns, value, onChange, table, noSchema }: { label: string; conns: ConnectionView[]; value: End; onChange: (e: End) => void; table?: boolean; noSchema?: boolean }) {
  const c = conns.find((x) => x.id === value.connId);
  const [dbs, setDbs] = useState<string[]>([]);
  const [schemas, setSchemas] = useState<string[]>([]);
  const [tables, setTables] = useState<string[]>([]);
  useEffect(() => {
    if (!c) return;
    meta.databases(c.id).then(setDbs, () => setDbs([]));
  }, [value.connId]);
  useEffect(() => {
    if (!c || c.kind !== "postgres" || !value.db) return setSchemas([]);
    meta.schemas(tid(c, value)).then((s) => setSchemas(s.filter((x) => !x.isSystem).map((x) => x.name)), () => setSchemas([]));
  }, [value.connId, value.db]);
  useEffect(() => {
    const sc = schemaOf(c, value);
    if (!table || !c || !sc) return setTables([]);
    meta.objects(tid(c, value), sc).then((o) => setTables(o.relations.filter((r) => r.kind === "table" || r.kind === "partitioned").map((r) => r.name)), () => setTables([]));
  }, [value.connId, value.db, value.schema, table]);
  return (
    <div className="mig-end">
      <span className="mig-label">{label}</span>
      <Select value={value.connId} onChange={(e) => onChange({ connId: e.target.value, db: "", schema: "", table: "" })}>
        <option value="">— data source —</option>
        {conns.filter(isSql).map((x) => (
          <option key={x.id} value={x.id}>
            {x.name}
          </option>
        ))}
      </Select>
      <Select value={value.db} onChange={(e) => onChange({ ...value, db: e.target.value, schema: c?.kind === "mysql" ? e.target.value : "", table: "" })}>
        <option value="">— database —</option>
        {dbs.map((d) => (
          <option key={d}>{d}</option>
        ))}
      </Select>
      {c?.kind === "postgres" && !noSchema && (
        <Select value={value.schema} onChange={(e) => onChange({ ...value, schema: e.target.value, table: "" })}>
          <option value="">— schema —</option>
          {schemas.map((d) => (
            <option key={d}>{d}</option>
          ))}
        </Select>
      )}
      {table && (
        <Select value={value.table} onChange={(e) => onChange({ ...value, table: e.target.value })}>
          <option value="">— table —</option>
          {tables.map((d) => (
            <option key={d}>{d}</option>
          ))}
        </Select>
      )}
    </div>
  );
}

const empty: End = { connId: "", db: "", schema: "", table: "" };
const ready = (c: ConnectionView | undefined, e: End, table = false) => !!c && !!schemaOf(c, e) && (!table || !!e.table);

/** Compare & Migrate 탭: 스키마 비교 (DDL) · 데이터 비교 (동기화 SQL) · 마이그레이션 파일 적용 */
export function MigrateView(p: { conns: ConnectionView[]; initialConnId: string; initialDb?: string; onOpenConsole: (connId: string, db: string | undefined, sql: string) => void; onStatus: (s: string) => void }) {
  const [tab, setTab] = useState<"schema" | "data" | "files">("schema");
  const start: End = { ...empty, connId: p.initialConnId, db: p.initialDb ?? "" };
  const [src, setSrc] = useState<End>(start);
  const [dst, setDst] = useState<End>(start);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [changes, setChanges] = useState<SchemaChange[] | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [data, setData] = useState<DataDiff | null>(null);
  const [dir, setDir] = useState<string | null>(null);
  const [migs, setMigs] = useState<Migration[] | null>(null);
  const sc = p.conns.find((x) => x.id === src.connId);
  const dc = p.conns.find((x) => x.id === dst.connId);
  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    setErr(null);
    try {
      await f();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };
  const compare = () =>
    run(async () => {
      const r = await migrate.compareSchemas(tid(sc, src), schemaOf(sc, src), tid(dc, dst), schemaOf(dc, dst));
      setChanges(r);
      // 지우는 것은 기본으로 고르지 않는다
      setChecked(new Set(r.map((c, i) => (c.destructive ? -1 : i)).filter((i) => i >= 0)));
    });
  const pickedSql = useMemo(() => (changes ?? []).filter((_, i) => checked.has(i)).map((c) => `-- ${c.kind}: ${c.object}\n${c.sql}`).join("\n\n"), [changes, checked]);
  const chooseDir = async () => {
    const d = await migrate.pickFolder();
    if (d) setDir(d);
    return d;
  };
  const loadMigs = (d = dir) => d && dc && dst.db && run(async () => setMigs(await migrate.status(tid(dc, dst), d)));
  useEffect(() => {
    if (tab === "files") loadMigs();
  }, [tab, dir, dst.connId, dst.db]);

  return (
    <div className="mig">
      <Tabs
        items={[
          { id: "schema", label: "Schema Compare" },
          { id: "data", label: "Data Compare" },
          { id: "files", label: "Migrations" },
        ]}
        value={tab}
        onChange={setTab}
      />
      {tab !== "files" && <EndPicker label="Source" conns={p.conns} value={src} onChange={setSrc} table={tab === "data"} />}
      <EndPicker label={tab === "files" ? "Database" : "Target"} conns={p.conns} value={dst} onChange={setDst} table={tab === "data"} noSchema={tab === "files"} />
      {err && <Notice tone="danger">{err}</Notice>}

      {tab === "schema" && (
        <>
          <div className="mig-actions">
            <Button variant="primary" disabled={busy || !ready(sc, src) || !ready(dc, dst)} onClick={compare}>
              {busy ? "Comparing…" : "Compare"}
            </Button>
            <span className="muted">Makes the target look like the source. Drops are left unchecked.</span>
            <span className="grow" />
            <Button disabled={!checked.size} onClick={() => dc && p.onOpenConsole(dc.id, dc.kind === "postgres" ? dst.db : undefined, pickedSql)}>
              Open in Console
            </Button>
            <Button
              disabled={!checked.size}
              onClick={async () => {
                const d = dir ?? (await chooseDir());
                if (!d) return;
                const name = (await askText("Migration name", `sync ${schemaOf(sc, src)} to ${schemaOf(dc, dst)}`));
                if (name) p.onStatus(`Saved ${await migrate.save(d, name, pickedSql + "\n")}`);
              }}
            >
              Save as Migration…
            </Button>
          </div>
          {changes && (
            <div className="mig-body">
              <div className="mig-list">
                {changes.length === 0 && <div className="muted pad">No differences — the schemas match.</div>}
                {changes.map((c, i) => (
                  <label key={i} className={"mig-item" + (c.destructive ? " danger" : "")}>
                    <input type="checkbox" checked={checked.has(i)} onChange={(e) => setChecked((s) => { const n = new Set(s); if (e.target.checked) n.add(i); else n.delete(i); return n; })} />
                    <span className="mig-kind">{c.kind.replace(/_/g, " ")}</span>
                    <span>{c.object}</span>
                  </label>
                ))}
              </div>
              <pre className="mig-sql">{pickedSql || "-- check changes to see their SQL"}</pre>
            </div>
          )}
        </>
      )}

      {tab === "data" && (
        <>
          <div className="mig-actions">
            <Button variant="primary" disabled={busy || !ready(sc, src, true) || !ready(dc, dst, true)} onClick={() => run(async () => setData(await migrate.compareData(tid(sc, src), schemaOf(sc, src), src.table, tid(dc, dst), schemaOf(dc, dst), dst.table)))}>
              {busy ? "Comparing…" : "Compare Rows"}
            </Button>
            <span className="muted">Rows are matched by the primary key; only columns both tables have are compared.</span>
            <span className="grow" />
            <Button disabled={!data?.sql} onClick={() => dc && data && p.onOpenConsole(dc.id, dc.kind === "postgres" ? dst.db : undefined, data.sql)}>
              Open Sync SQL in Console
            </Button>
          </div>
          {data && (
            <>
              <div className="mig-counts">
                <b>{data.same.toLocaleString()}</b> same · <b>{data.different.toLocaleString()}</b> different · <b>{data.onlySource.toLocaleString()}</b> only in source · <b>{data.onlyTarget.toLocaleString()}</b> only in target
                {data.truncated && <span className="danger"> · compared the first 200,000 rows only</span>}
              </div>
              <pre className="mig-sql full">{data.sql || "-- the tables hold the same data"}</pre>
            </>
          )}
        </>
      )}

      {tab === "files" && (
        <>
          <div className="mig-actions">
            <Button onClick={chooseDir}>{dir ? "Change Folder…" : "Choose Folder…"}</Button>
            <span className="muted">{dir ?? "Files named V1__create_tables.sql, V2__add_index.sql … applied in order; applied ones are recorded in qurry_schema_history."}</span>
            <span className="grow" />
            <Button disabled={!dir || busy} onClick={() => loadMigs()}>
              Refresh
            </Button>
            <Button
              className="primary"
              disabled={!dir || busy || !migs?.some((m) => m.status === "pending")}
              onClick={async () => {
                const pending = migs!.filter((m) => m.status === "pending");
                if (!dc || !(await ask(`Apply ${pending.length} migration(s) to "${dc.name}"?`, { detail: pending.map((m) => ({ text: m.description, value: `V${m.version}` })), okLabel: "Apply" }))) return;
                run(async () => {
                  const done = await migrate.apply(tid(dc, dst), dir!);
                  p.onStatus(`Applied ${done.length} migration(s)`);
                  setMigs(await migrate.status(tid(dc, dst), dir!));
                });
              }}
            >
              Apply Pending
            </Button>
          </div>
          {migs && (
            <div className="transfer-map mig-table">
              <table>
                <thead>
                  <tr>
                    <th>Version</th>
                    <th>Description</th>
                    <th>Status</th>
                    <th>Applied</th>
                    <th>File</th>
                  </tr>
                </thead>
                <tbody>
                  {migs.length === 0 && (
                    <tr>
                      <td colSpan={5} className="muted">No V*__*.sql files in this folder</td>
                    </tr>
                  )}
                  {migs.map((m) => (
                    <tr key={m.version + m.file}>
                      <td>V{m.version}</td>
                      <td>{m.description}</td>
                      <td className={"mig-st " + m.status}>{m.status}</td>
                      <td className="muted">{m.appliedAt ?? ""}</td>
                      <td className="muted">{m.file}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}
