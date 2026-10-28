import { useEffect, useState } from "react";
import { api, DataPreview, meta } from "../lib/api";
import { autoMap, DataFormat, FORMATS, progressText } from "../lib/transfer";
import { Button, Hint, IconButton, Modal, Notice, ProgressBar, RadioGroup, SectionTitle, Select, StatusItem, Table, Td, Text, TextInput, Th } from "../ui";

const LAST = "qurry.export";
function lastChoice(): { format: DataFormat; clipboard: boolean } {
  try {
    const v = JSON.parse(localStorage.getItem(LAST) ?? "");
    if (FORMATS.some((f) => f.id === v.format)) return { format: v.format, clipboard: !!v.clipboard };
  } catch {
    /* 처음이면 CSV 파일 */
  }
  return { format: "csv", clipboard: false };
}

/** 내보내기 전에 형식과 대상(파일/클립보드)을 고른다. 고른 것은 다음에도 */
export function ExportDialog({ what, onExport, onClose }: { what: string; onExport: (format: DataFormat, clipboard: boolean) => void; onClose: () => void }) {
  const [c, setC] = useState(lastChoice);
  const go = () => {
    try {
      localStorage.setItem(LAST, JSON.stringify(c));
    } catch {
      /* 저장 못 해도 내보내기는 한다 */
    }
    onExport(c.format, c.clipboard);
  };
  return (
    <Modal
      title={<>Export {what}</>}
      onClose={onClose}
      onKeyDown={(e) => e.key === "Enter" && go()}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" autoFocus onClick={go}>
            Export
          </Button>
        </>
      }
    >
      <SectionTitle>Format</SectionTitle>
      <RadioGroup value={c.format} onChange={(format) => setC({ ...c, format })} options={FORMATS.map((f) => ({ value: f.id, label: <b>{f.label}</b>, hint: f.hint }))} />
      <SectionTitle>To</SectionTitle>
      <RadioGroup
        value={c.clipboard ? "clip" : "file"}
        onChange={(v) => setC({ ...c, clipboard: v === "clip" })}
        options={[
          { value: "file", label: <b>File…</b>, hint: "all rows, choose where to save" },
          { value: "clip", label: <b>Clipboard</b>, hint: "all rows, paste anywhere (up to 64 MB)" },
        ]}
      />
    </Modal>
  );
}

/** 가져오기: 파일 고르기 → 미리보기 → 파일 열마다 넣을 컬럼 (같은 이름은 자동). table 이 없으면 새 테이블 */
export function ImportDialog({ target, schema, table, onImport, onClose }: {
  target: string;
  schema: string;
  table?: string;
  onImport: (table: string, create: boolean, columns: [number, string][], label: string) => void;
  onClose: () => void;
}) {
  const create = !table;
  const [file, setFile] = useState<{ path: string; pv: DataPreview } | null>(null);
  const [cols, setCols] = useState<string[] | null>(create ? null : []);
  const [map, setMap] = useState<string[]>([]);
  const [name, setName] = useState(table ?? "");
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!table) return;
    meta.table(target, schema, table).then(
      (d) => setCols(d.columns.map((c) => c.name)),
      (e) => setErr(String(e)),
    );
  }, [target, schema, table]);

  const pick = async () => {
    setErr(null);
    try {
      const r = await api.pickDataFile();
      if (!r) return;
      setFile({ path: r[0], pv: r[1] });
      if (create && !name) setName(r[0].split(/[\\/]/).pop()!.replace(/\.[^.]+$/, "").replace(/\W+/g, "_").toLowerCase());
    } catch (e) {
      setErr(String(e));
    }
  };
  useEffect(() => {
    if (file) setMap(autoMap(file.pv.columns, create ? null : cols));
  }, [file, cols]);

  const chosen = map.map((c, i) => [i, c.trim()] as [number, string]).filter(([, c]) => c);
  const ok = !!file && chosen.length > 0 && !!name.trim();
  const fileName = file?.path.split(/[\\/]/).pop();
  return (
    <Modal
      title={create ? `Import Data into a New Table in ${schema}` : `Import Data into ${schema}.${table}`}
      size="lg"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!ok} onClick={() => onImport(name.trim(), create, chosen, `${fileName} → ${schema}.${name.trim()}`)}>
            Import {chosen.length ? `${chosen.length} column${chosen.length > 1 ? "s" : ""}` : ""}
          </Button>
        </>
      }
    >
      <div className="transfer-file">
        <Button size="sm" onClick={pick} autoFocus={!file}>
          {file ? "Change File…" : "Choose File…"}
        </Button>
        <Text tone="muted">{file ? `${fileName} · ${file.pv.format.toUpperCase()} · ${(file.pv.bytes / 1048576).toFixed(1)} MB` : "CSV, TSV or JSON (array of objects)"}</Text>
      </div>
      {create && (
        <div className="transfer-file">
          <Text tone="muted">Table name</Text>
          <TextInput value={name} onChange={(e) => setName(e.target.value)} placeholder="new_table" />
          <Text tone="muted">columns are created as text</Text>
        </div>
      )}
      {file && (
        <div className="transfer-map">
          <Table>
            <thead>
              <tr>
                <Th>File column</Th>
                <Th>Sample</Th>
                <Th>{create ? "New column (empty = skip)" : "Table column"}</Th>
              </tr>
            </thead>
            <tbody>
              {file.pv.columns.map((fc, i) => (
                <tr key={i}>
                  <Td>{fc}</Td>
                  <Td muted>{file.pv.rows.slice(0, 3).map((r) => r[i] ?? "NULL").join(", ")}</Td>
                  <Td>
                    {create ? (
                      <TextInput value={map[i] ?? ""} onChange={(e) => setMap(map.map((m, j) => (j === i ? e.target.value : m)))} />
                    ) : (
                      <Select value={map[i] ?? ""} onChange={(e) => setMap(map.map((m, j) => (j === i ? e.target.value : m)))}>
                        <option value="">— skip —</option>
                        {(cols ?? []).map((c) => (
                          <option key={c} value={c}>
                            {c}
                          </option>
                        ))}
                      </Select>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </div>
      )}
      {file && <Hint>Empty cells are imported as NULL. Everything goes in one transaction — if a row fails, nothing is imported.</Hint>}
      {err && <Notice tone="danger">{err}</Notice>}
    </Modal>
  );
}

export interface TransferJob { id: string; label: string; rows: number; done: number; total: number }

/** 하단 진행 막대: 도는 내보내기·가져오기마다 (취소 가능) */
export function TransferBar({ jobs }: { jobs: TransferJob[] }) {
  return (
    <>
      {jobs.map((j) => {
        const p = progressText(j);
        return (
          <StatusItem key={j.id} title={j.label}>
            <Text truncate style={{ maxWidth: 260 }}>
              {j.label}
            </Text>
            <ProgressBar pct={p.pct} />
            <span>{p.text}</span>
            <IconButton size="sm" tone="danger" title="Cancel" onClick={() => api.cancelJob(j.id)}>
              ✕
            </IconButton>
          </StatusItem>
        );
      })}
    </>
  );
}

/** 백업 (pg_dump / mysqldump): 형식·범위를 고른다 */
export function BackupDialog({ what, pg, onRun, onClose }: { what: string; pg: boolean; onRun: (o: { format: "custom" | "plain"; schemaOnly: boolean; dataOnly: boolean }) => void; onClose: () => void }) {
  const [format, setFormat] = useState<"custom" | "plain">(pg ? "custom" : "plain");
  const [part, setPart] = useState<"all" | "schema" | "data">("all");
  return (
    <Modal
      title={<>Back Up {what}</>}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" autoFocus onClick={() => onRun({ format, schemaOnly: part === "schema", dataOnly: part === "data" })}>
            Choose File & Back Up
          </Button>
        </>
      }
    >
      {pg && (
        <>
          <SectionTitle>Format</SectionTitle>
          <RadioGroup
            value={format}
            onChange={setFormat}
            options={[
              { value: "custom", label: <b>Custom (.dump)</b>, hint: "compressed, restore with Restore… (pg_restore)" },
              { value: "plain", label: <b>SQL (.sql)</b>, hint: "plain text, readable, runs in any console" },
            ]}
          />
        </>
      )}
      <SectionTitle>What</SectionTitle>
      <RadioGroup
        value={part}
        onChange={setPart}
        options={[
          { value: "all", label: <b>Schema and data</b> },
          { value: "schema", label: <b>Schema only</b> },
          { value: "data", label: <b>Data only</b> },
        ]}
      />
      <Hint>Runs {pg ? "pg_dump" : "mysqldump"} on this computer (install the client tools if it's missing). The password is passed through the environment, not the command line.</Hint>
    </Modal>
  );
}
