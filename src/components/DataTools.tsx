import { useEffect, useState } from "react";
import { ConnectionView, fake, FakePreview, subset, SubsetPlan, SubsetSpec } from "../lib/api";
import { ask } from "../lib/dialogs";
import { Badge, Button, Checkbox, Field, Hint, Modal, Notice, SectionTitle, Select, Table, Td, Text, TextInput, Th } from "../ui";

const MAX_ROWS = 100_000;

const cell = (v: unknown) => (v === null ? "NULL" : typeof v === "object" ? JSON.stringify(v) : String(v));

/**
 * 테스트 데이터 만들기: 테이블 구조(타입·길이·NOT NULL·PK·UNIQUE·FK·enum·CHECK)와 컬럼 이름으로 그럴듯한 행을 채운다.
 * 넣기 전에 컬럼마다 채우는 법과 예시를 보여 준다. 한 트랜잭션 — 하나라도 실패하면 아무것도 안 들어간다
 */
export function GenerateDataDialog({ target, schema, table, production, onDone, onClose }: { target: string; schema: string; table: string; production?: string; onDone: (rows: number) => void; onClose: () => void }) {
  const [pv, setPv] = useState<FakePreview | null>(null);
  const [rows, setRows] = useState("100");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const where = schema ? `${schema}.${table}` : table;
  const load = () => {
    setErr(null);
    fake.preview(target, schema, table).then(setPv, (e) => setErr(String(e)));
  };
  useEffect(load, [target, schema, table]);

  const n = Number(rows.replace(/[,_\s]/g, ""));
  const valid = Number.isInteger(n) && n >= 1 && n <= MAX_ROWS;
  const run = async () => {
    if (production && !(await ask(`Insert ${n.toLocaleString("en-US")} generated rows into ${where}?`, { title: `Production: ${production}`, okLabel: "Insert", danger: true }))) return;
    setBusy(true);
    setErr(null);
    try {
      onDone(await fake.fill(target, schema, table, n));
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      title={`Generate Test Data: ${where}`}
      size="xl"
      onClose={onClose}
      footer={
        <>
          <span className="ui-modal-foot-hint">Inserted in one transaction — if a row fails, nothing is inserted</span>
          <Button onClick={load} disabled={busy}>
            New Sample
          </Button>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!pv || !valid || busy} onClick={run}>
            {busy ? "Inserting…" : `Insert ${valid ? n.toLocaleString("en-US") : ""} Rows`}
          </Button>
        </>
      }
    >
      <div className="transfer-file">
        <Text tone="muted">Rows</Text>
        <TextInput className="w-port" autoFocus value={rows} onChange={(e) => setRows(e.target.value)} onKeyDown={(e) => e.key === "Enter" && valid && pv && !busy && run()} />
        <Text tone="muted">up to {MAX_ROWS.toLocaleString("en-US")}</Text>
      </div>
      {err && <Notice tone="danger">{err}</Notice>}
      {pv?.warnings.map((w) => (
        <Notice key={w} tone="warn">
          {w}
        </Notice>
      ))}
      {pv && (
        <>
          <SectionTitle>Columns</SectionTitle>
          <div className="gen-plan">
            <Table>
              <thead>
                <tr>
                  <Th>Column</Th>
                  <Th>Type</Th>
                  <Th>Filled with</Th>
                </tr>
              </thead>
              <tbody>
                {pv.plan.map((c) => (
                  <tr key={c.name}>
                    <Td mono>{c.name}</Td>
                    <Td muted>{c.dataType}</Td>
                    <Td muted={c.how.startsWith("left to")}>{c.how}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </div>
          <SectionTitle>Sample</SectionTitle>
          <div className="gen-sample">
            <Table>
              <thead>
                <tr>
                  {pv.result.columns.map((c) => (
                    <Th key={c.name}>{c.name}</Th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {pv.result.rows.map((r, i) => (
                  <tr key={i}>
                    {r.map((v, j) => (
                      <Td key={j} mono muted={v === null}>
                        {cell(v)}
                      </Td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </Table>
          </div>
          <Hint>Foreign keys use rows that already exist in the referenced table. Auto-numbered and generated columns are left to the database.</Hint>
        </>
      )}
    </Modal>
  );
}

/**
 * 운영 데이터 일부 복사: 시작 행 (WHERE) 과 FK 로 이어진 행 (부모는 끝까지, 자식은 고른 단계까지) 을 다른 Data Source 로.
 * 개인정보로 보이는 컬럼은 읽을 때 가린다 (md5 — 같은 값은 늘 같게). 대상에는 한 트랜잭션, 이미 있는 행은 건너뛴다
 */
export function CopyRowsDialog({ source, schema, table, conn, conns, onDone, onClose }: { source: string; schema: string; table: string; conn: ConnectionView; conns: ConnectionView[]; onDone: (msg: string) => void; onClose: () => void }) {
  const [filter, setFilter] = useState("");
  const [depth, setDepth] = useState(1);
  const [mask, setMask] = useState(true);
  // 운영이 아닌 곳을 먼저 (기본값이 운영이 되지 않게)
  const targets = conns.filter((c) => c.kind === conn.kind && c.id !== conn.id).sort((a, b) => Number(!!a.options?.confirmChanges) - Number(!!b.options?.confirmChanges));
  const [target, setTarget] = useState(targets[0]?.id ?? "");
  const [plan, setPlan] = useState<SubsetPlan | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<"preview" | "copy" | null>(null);
  const where = schema ? `${schema}.${table}` : table;
  const spec: SubsetSpec = { schema, table, filter, depth, mask };
  // 조건을 바꾸면 미리보기를 다시
  useEffect(() => setPlan(null), [filter, depth, mask]);

  const preview = async () => {
    setBusy("preview");
    setErr(null);
    try {
      setPlan(await subset.preview(source, spec));
    } catch (e) {
      setErr(String(e));
    }
    setBusy(null);
  };
  const copy = async () => {
    const t = conns.find((c) => c.id === target);
    if (!t || !plan) return;
    if (t.options?.confirmChanges && !(await ask(`Copy ${plan.total.toLocaleString("en-US")} rows into ${t.name}?`, { title: `Production: ${t.name}`, okLabel: "Copy", danger: true }))) return;
    setBusy("copy");
    setErr(null);
    try {
      const r = await subset.copy(source, spec, target);
      onDone(`${r.total.toLocaleString("en-US")} related row${r.total === 1 ? "" : "s"} copied from ${conn.name} into ${t.name} (rows already there were skipped)`);
    } catch (e) {
      setErr(String(e));
      setBusy(null);
    }
  };

  return (
    <Modal
      title={`Copy Related Rows: ${where}`}
      size="xl"
      onClose={onClose}
      footer={
        <>
          <span className="ui-modal-foot-hint">Reads {conn.name} · writes the target in one transaction</span>
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={!!busy} onClick={preview}>
            {busy === "preview" ? "Reading…" : "Preview"}
          </Button>
          <Button variant="primary" disabled={!plan || !target || !!busy} onClick={copy}>
            {busy === "copy" ? "Copying…" : `Copy ${plan ? plan.total.toLocaleString("en-US") : ""} Rows`}
          </Button>
        </>
      }
    >
      <Field label="Rows" hint="WHERE condition for the starting rows (empty = all rows)">
        <TextInput full autoFocus className="mono" placeholder="id = 42" value={filter} onChange={(e) => setFilter(e.target.value)} onKeyDown={(e) => e.key === "Enter" && !busy && preview()} />
      </Field>
      <Field label="Child rows" hint="Rows these point to (parents) always come along, so foreign keys hold">
        <Select value={depth} onChange={(e) => setDepth(Number(e.target.value))}>
          <option value={0}>None — only the rows and their parents</option>
          <option value={1}>1 level (e.g. a customer's orders)</option>
          <option value={2}>2 levels (e.g. … and the order items)</option>
          <option value={3}>3 levels</option>
        </Select>
      </Field>
      <Field label="Into">
        {targets.length ? (
          <Select value={target} onChange={(e) => setTarget(e.target.value)}>
            {targets.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
                {c.options?.confirmChanges ? " (Production)" : ""}
              </option>
            ))}
          </Select>
        ) : (
          <Text tone="muted">No other {conn.kind === "mysql" ? "MySQL" : "PostgreSQL"} data source — add one to copy into</Text>
        )}
      </Field>
      <Checkbox checked={mask} onChange={setMask}>
        Mask personal data (email, phone, people's names, address, ID / card numbers, passwords, birth dates, IP)
      </Checkbox>
      {err && <Notice tone="danger">{err}</Notice>}
      {plan && (
        <>
          <SectionTitle>{plan.total.toLocaleString("en-US")} rows from {plan.tables.length} tables (in insert order)</SectionTitle>
          <div className="gen-plan">
            <Table>
              <thead>
                <tr>
                  <Th>Table</Th>
                  <Th num>Rows</Th>
                  <Th>Why</Th>
                  <Th>Masked</Th>
                </tr>
              </thead>
              <tbody>
                {plan.tables.map((t) => (
                  <tr key={`${t.schema}.${t.table}`}>
                    <Td mono>{t.schema && t.schema !== schema ? `${t.schema}.${t.table}` : t.table}</Td>
                    <Td num>{t.rows.toLocaleString("en-US")}</Td>
                    <Td muted>{t.reason}</Td>
                    <Td>
                      <span className="gen-badges">
                        {t.masked.map((m) => (
                          <Badge key={m.column} tone="warn" title={m.mask}>
                            {m.column}
                          </Badge>
                        ))}
                      </span>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </div>
          <Hint>The tables must already exist in the target (Compare Schemas makes the DDL). Masked values are md5-based, so the same value always masks the same way.</Hint>
        </>
      )}
    </Modal>
  );
}
