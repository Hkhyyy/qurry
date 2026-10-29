import { useEffect, useState } from "react";
import { ColumnDef, ConnectionView, ddl, ForeignKeyDef, IndexDef, NewObject, TableDef } from "../lib/api";
import { ask } from "../lib/dialogs";
import { Button, Checkbox, CodeBlock, Field, Hint, IconButton, Modal, Notice, SectionTitle, Select, Table, Tabs, Td, Text, TextArea, TextInput, Th } from "../ui";

/** 타입 칸 자동완성 */
const TYPES: Record<string, string[]> = {
  postgres: ["integer", "bigint", "smallint", "numeric(10,2)", "real", "double precision", "boolean", "text", "varchar(255)", "char(1)", "date", "timestamp", "timestamptz", "time", "interval", "uuid", "json", "jsonb", "bytea", "inet", "text[]"],
  mysql: ["int", "bigint", "smallint", "tinyint(1)", "decimal(10,2)", "float", "double", "varchar(255)", "char(1)", "text", "longtext", "date", "datetime", "timestamp", "time", "json", "blob", "enum('a','b')"],
};
const ACTIONS = ["NO ACTION", "RESTRICT", "CASCADE", "SET NULL", "SET DEFAULT"];

const split = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);

/** 만든 DDL 미리보기 + 실행 (운영이면 묻는다). 실패하면 이유 */
async function runDdl(target: string, conn: ConnectionView, statements: string[], what: string): Promise<string | null> {
  if (conn.options?.confirmChanges && !(await ask(`Run ${statements.length} statement${statements.length > 1 ? "s" : ""} to ${what}?`, { title: `Production: ${conn.name}`, detail: statements.map((s) => ({ text: s.replace(/\s+/g, " ").slice(0, 300), value: "schema change", tone: "danger" as const })), okLabel: "Run", danger: true }))) return "cancelled";
  try {
    await ddl.run(target, statements);
    return null;
  } catch (e) {
    return String(e);
  }
}

/** 미리보기를 짧게 늦춰 부른다 (칸마다 치는 동안 매번 부르지 않게) */
function usePreview<T>(input: T, make: (v: T) => Promise<string[]>): { sql: string[]; err: string | null } {
  const [out, setOut] = useState<{ sql: string[]; err: string | null }>({ sql: [], err: null });
  const key = JSON.stringify(input);
  useEffect(() => {
    const t = setTimeout(() => make(input).then((sql) => setOut({ sql, err: null }), (e) => setOut({ sql: [], err: String(e) })), 150);
    return () => clearTimeout(t);
  }, [key]);
  return out;
}

/**
 * 테이블 만들기 · 고치기 (New ▸ Table… · Modify Table… ⌘F6): 컬럼 · 키와 인덱스 · 외래 키. 바꾸면 아래에 DDL 이 바로 보인다.
 * PG 는 한 트랜잭션으로 (실패하면 아무것도 안 바뀜), MySQL 은 문장마다 바로 반영된다
 */
export function TableDesigner({ target, conn, schema, table, onDone, onOpenConsole, onClose }: { target: string; conn: ConnectionView; schema: string; table?: string; onDone: (msg: string) => void; onOpenConsole: (sql: string) => void; onClose: () => void }) {
  const [before, setBefore] = useState<TableDef | null>(null);
  const [def, setDef] = useState<TableDef | null>(
    table ? null : { schema, name: "", columns: [{ name: "id", dataType: conn.kind === "mysql" ? "bigint" : "bigint", nullable: false, autoIncrement: true }], primaryKey: ["id"], indexes: [], foreignKeys: [] },
  );
  const [tab, setTab] = useState<"columns" | "keys" | "fks">("columns");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!table) return;
    ddl.tableDef(target, schema, table).then(
      (d) => (setBefore(d), setDef(d)),
      (e) => setErr(String(e)),
    );
  }, [target, schema, table]);
  const preview = usePreview(def, (d) => (d ? ddl.table(target, before, d) : Promise.resolve([])));
  const types = TYPES[conn.kind] ?? TYPES.postgres;

  if (!def) return <Modal title={`Modify Table: ${schema}.${table}`} onClose={onClose}>{err ? <Notice tone="danger">{err}</Notice> : <Text tone="muted">Reading the table…</Text>}</Modal>;
  const set = (patch: Partial<TableDef>) => setDef({ ...def, ...patch });
  const setCol = (i: number, patch: Partial<ColumnDef>) => {
    const old = def.columns[i];
    const columns = def.columns.map((c, j) => (j === i ? { ...c, ...patch } : c));
    // 이름을 바꾸면 PK · 인덱스 · FK 의 컬럼 이름도 따라간다
    const ren = (xs: string[]) => (patch.name !== undefined ? xs.map((x) => (x === old.name ? patch.name! : x)) : xs);
    set({ columns, primaryKey: ren(def.primaryKey), indexes: def.indexes.map((x) => ({ ...x, columns: ren(x.columns) })), foreignKeys: def.foreignKeys.map((f) => ({ ...f, columns: ren(f.columns) })) });
  };
  const togglePk = (name: string, on: boolean) => set({ primaryKey: on ? [...def.primaryKey, name] : def.primaryKey.filter((x) => x !== name) });
  const setIdx = (i: number, patch: Partial<IndexDef>) => set({ indexes: def.indexes.map((x, j) => (j === i ? { ...x, ...patch } : x)) });
  const setFk = (i: number, patch: Partial<ForeignKeyDef>) => set({ foreignKeys: def.foreignKeys.map((x, j) => (j === i ? { ...x, ...patch } : x)) });
  const tname = def.name || "new_table";
  const run = async () => {
    setBusy(true);
    setErr(null);
    const e = await runDdl(target, conn, preview.sql, table ? `modify ${schema}.${table}` : `create ${schema ? `${schema}.` : ""}${def.name}`);
    setBusy(false);
    if (e === "cancelled") return;
    if (e) return setErr(e);
    onDone(table ? `Table ${def.name} modified (${preview.sql.length} statement${preview.sql.length > 1 ? "s" : ""})` : `Table ${def.name} created`);
  };

  return (
    <Modal
      title={table ? `Modify Table: ${schema ? `${schema}.` : ""}${table}` : `New Table in ${schema || conn.name}`}
      size="xl"
      className="designer"
      onClose={onClose}
      footer={
        <>
          <span className="ui-modal-foot-hint">{conn.kind === "mysql" ? "MySQL applies each statement right away" : "Runs in one transaction — if a statement fails, nothing changes"}</span>
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={!preview.sql.length} onClick={() => onOpenConsole(preview.sql.map((s) => `${s};`).join("\n\n"))}>
            Open in Console
          </Button>
          <Button variant="primary" disabled={!preview.sql.length || busy} onClick={run}>
            {busy ? "Running…" : table ? "Apply" : "Create"}
          </Button>
        </>
      }
    >
      <datalist id="ddl-types">
        {types.map((t) => (
          <option key={t} value={t} />
        ))}
      </datalist>
      <div className="designer-head">
        <Field label="Name">
          <TextInput autoFocus={!table} value={def.name} placeholder="new_table" onChange={(e) => set({ name: e.target.value })} />
        </Field>
        <Field label="Comment">
          <TextInput full value={def.comment ?? ""} onChange={(e) => set({ comment: e.target.value })} />
        </Field>
      </div>
      <Tabs
        items={[
          { id: "columns", label: `Columns (${def.columns.length})` },
          { id: "keys", label: `Keys & Indexes (${def.indexes.length + (def.primaryKey.length ? 1 : 0)})` },
          { id: "fks", label: `Foreign Keys (${def.foreignKeys.length})` },
        ]}
        value={tab}
        onChange={setTab}
      />
      <div className="designer-body">
        {tab === "columns" && (
          <Table>
            <thead>
              <tr>
                <Th>Name</Th>
                <Th>Type</Th>
                <Th title="NOT NULL">Not null</Th>
                <Th>Default</Th>
                <Th title={conn.kind === "mysql" ? "AUTO_INCREMENT" : "GENERATED BY DEFAULT AS IDENTITY"}>Auto</Th>
                <Th>PK</Th>
                <Th>Comment</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {def.columns.map((c, i) => (
                <tr key={i}>
                  <Td>
                    <TextInput value={c.name} placeholder="column" onChange={(e) => setCol(i, { name: e.target.value })} />
                  </Td>
                  <Td>
                    <TextInput list="ddl-types" value={c.dataType} placeholder="type" onChange={(e) => setCol(i, { dataType: e.target.value })} />
                  </Td>
                  <Td>
                    <Checkbox checked={!c.nullable} onChange={(v) => setCol(i, { nullable: !v })} />
                  </Td>
                  <Td>
                    <TextInput value={c.default ?? ""} placeholder="—" onChange={(e) => setCol(i, { default: e.target.value || null })} />
                  </Td>
                  <Td>
                    <Checkbox checked={c.autoIncrement} onChange={(v) => setCol(i, { autoIncrement: v })} />
                  </Td>
                  <Td>
                    <Checkbox checked={def.primaryKey.includes(c.name)} onChange={(v) => togglePk(c.name, v)} />
                  </Td>
                  <Td>
                    <TextInput value={c.comment ?? ""} onChange={(e) => setCol(i, { comment: e.target.value || null })} />
                  </Td>
                  <Td>
                    <IconButton size="sm" tone="danger" title="Remove column" onClick={() => set({ columns: def.columns.filter((_, j) => j !== i), primaryKey: def.primaryKey.filter((x) => x !== c.name) })}>
                      ✕
                    </IconButton>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
        {tab === "keys" && (
          <>
            <Text tone="muted" size="sm">
              Primary key: {def.primaryKey.length ? def.primaryKey.join(", ") : "none"} (tick PK on the Columns tab)
            </Text>
            <Table>
              <thead>
                <tr>
                  <Th>Index</Th>
                  <Th>Columns (comma separated)</Th>
                  <Th>Unique</Th>
                  <Th />
                </tr>
              </thead>
              <tbody>
                {def.indexes.map((x, i) => (
                  <tr key={i}>
                    <Td>
                      <TextInput value={x.name} disabled={!!x.definition} onChange={(e) => setIdx(i, { name: e.target.value })} />
                    </Td>
                    <Td>{x.definition ? <Text tone="muted" mono size="sm">{x.definition}</Text> : <TextInput full value={x.columns.join(", ")} onChange={(e) => setIdx(i, { columns: split(e.target.value) })} />}</Td>
                    <Td>
                      <Checkbox checked={x.unique} disabled={!!x.definition} onChange={(v) => setIdx(i, { unique: v })} />
                    </Td>
                    <Td>
                      <IconButton size="sm" tone="danger" title="Remove index" onClick={() => set({ indexes: def.indexes.filter((_, j) => j !== i) })}>
                        ✕
                      </IconButton>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </>
        )}
        {tab === "fks" && (
          <Table>
            <thead>
              <tr>
                <Th>Name</Th>
                <Th>Columns</Th>
                <Th>References table</Th>
                <Th>Columns</Th>
                <Th>On delete</Th>
                <Th>On update</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {def.foreignKeys.map((f, i) => (
                <tr key={i}>
                  <Td>
                    <TextInput value={f.name} onChange={(e) => setFk(i, { name: e.target.value })} />
                  </Td>
                  <Td>
                    <TextInput value={f.columns.join(", ")} onChange={(e) => setFk(i, { columns: split(e.target.value) })} />
                  </Td>
                  <Td>
                    <TextInput
                      value={f.refSchema && f.refSchema !== def.schema ? `${f.refSchema}.${f.refTable}` : f.refTable}
                      onChange={(e) => {
                        const [a, b] = e.target.value.includes(".") ? e.target.value.split(".", 2) : [def.schema, e.target.value];
                        setFk(i, { refSchema: a, refTable: b });
                      }}
                    />
                  </Td>
                  <Td>
                    <TextInput value={f.refColumns.join(", ")} onChange={(e) => setFk(i, { refColumns: split(e.target.value) })} />
                  </Td>
                  {(["onDelete", "onUpdate"] as const).map((k) => (
                    <Td key={k}>
                      <Select value={f[k] || "NO ACTION"} onChange={(e) => setFk(i, { [k]: e.target.value })}>
                        {ACTIONS.map((a) => (
                          <option key={a}>{a}</option>
                        ))}
                      </Select>
                    </Td>
                  ))}
                  <Td>
                    <IconButton size="sm" tone="danger" title="Remove foreign key" onClick={() => set({ foreignKeys: def.foreignKeys.filter((_, j) => j !== i) })}>
                      ✕
                    </IconButton>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
        <div className="designer-add">
          {tab === "columns" && (
            <Button size="sm" onClick={() => set({ columns: [...def.columns, { name: "", dataType: "", nullable: true, autoIncrement: false }] })}>
              + Column
            </Button>
          )}
          {tab === "keys" && (
            <Button size="sm" onClick={() => set({ indexes: [...def.indexes, { name: `${tname}_idx${def.indexes.length + 1}`, columns: [], unique: false }] })}>
              + Index
            </Button>
          )}
          {tab === "fks" && (
            <Button size="sm" onClick={() => set({ foreignKeys: [...def.foreignKeys, { name: `${tname}_fk${def.foreignKeys.length + 1}`, columns: [], refSchema: def.schema, refTable: "", refColumns: ["id"], onDelete: "NO ACTION", onUpdate: "NO ACTION" }] })}>
              + Foreign Key
            </Button>
          )}
        </div>
      </div>
      <SectionTitle>SQL</SectionTitle>
      {preview.err ? <Hint>{preview.err}</Hint> : <CodeBlock className="designer-sql">{preview.sql.length ? preview.sql.map((s) => `${s};`).join("\n\n") : "-- no changes"}</CodeBlock>}
      {err && <Notice tone="danger">{err}</Notice>}
    </Modal>
  );
}

export type NewKind = "database" | "schema" | "view" | "matview" | "sequence";

/** New ▸ Database · Schema · View · Materialized View · Sequence: 몇 칸 + DDL 미리보기 */
export function NewObjectDialog({ kind, target, conn, schema, onDone, onOpenConsole, onClose }: { kind: NewKind; target: string; conn: ConnectionView; schema: string; onDone: (msg: string) => void; onOpenConsole: (sql: string) => void; onClose: () => void }) {
  const [name, setName] = useState("");
  const [charset, setCharset] = useState(conn.kind === "mysql" ? "utf8mb4" : "");
  const [query, setQuery] = useState("SELECT ");
  const [start, setStart] = useState("");
  const [inc, setInc] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const num = (s: string) => (s.trim() && Number.isFinite(Number(s)) ? Number(s) : null);
  const object: NewObject =
    kind === "database" ? { kind, name, charset } : kind === "schema" ? { kind, name } : kind === "sequence" ? { kind, schema, name, start: num(start), increment: num(inc) } : { kind: "view", schema, name, query, materialized: kind === "matview" };
  const preview = usePreview(object, (o) => ddl.newObject(target, o));
  const label = { database: "Database", schema: "Schema", view: "View", matview: "Materialized View", sequence: "Sequence" }[kind];
  const run = async () => {
    setBusy(true);
    setErr(null);
    const e = await runDdl(target, conn, preview.sql, `create ${label.toLowerCase()} ${name}`);
    setBusy(false);
    if (e === "cancelled") return;
    if (e) return setErr(e);
    onDone(`${label} ${name} created`);
  };
  return (
    <Modal
      title={`New ${label}${kind === "database" || kind === "schema" ? ` on ${conn.name}` : ` in ${schema}`}`}
      size={kind === "view" || kind === "matview" ? "lg" : undefined}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={!preview.sql.length} onClick={() => onOpenConsole(preview.sql.map((s) => `${s};`).join("\n\n"))}>
            Open in Console
          </Button>
          <Button variant="primary" disabled={!preview.sql.length || busy} onClick={run}>
            {busy ? "Running…" : "Create"}
          </Button>
        </>
      }
    >
      <Field label="Name">
        <TextInput full autoFocus value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      {kind === "database" && (
        <Field label={conn.kind === "mysql" ? "Character set" : "Encoding"} hint="Empty = the server default">
          <TextInput value={charset} placeholder={conn.kind === "mysql" ? "utf8mb4" : "UTF8"} onChange={(e) => setCharset(e.target.value)} />
        </Field>
      )}
      {(kind === "view" || kind === "matview") && (
        <Field label="Query">
          <TextArea rows={8} style={{ width: "100%" }} value={query} onChange={(e) => setQuery(e.target.value)} />
        </Field>
      )}
      {kind === "sequence" && (
        <>
          <Field label="Start with">
            <TextInput className="w-port" value={start} placeholder="1" onChange={(e) => setStart(e.target.value)} />
          </Field>
          <Field label="Increment by">
            <TextInput className="w-port" value={inc} placeholder="1" onChange={(e) => setInc(e.target.value)} />
          </Field>
        </>
      )}
      <SectionTitle>SQL</SectionTitle>
      {preview.err ? <Hint>{preview.err}</Hint> : <CodeBlock>{preview.sql.map((s) => `${s};`).join("\n\n")}</CodeBlock>}
      {err && <Notice tone="danger">{err}</Notice>}
    </Modal>
  );
}
