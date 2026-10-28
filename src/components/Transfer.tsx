import { useEffect, useState } from "react";
import { api, DataPreview, meta } from "../lib/api";
import { autoMap, DataFormat, FORMATS, progressText } from "../lib/transfer";
import { Button, Hint, Modal, Notice, Select, TextInput } from "../ui";

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
      <div className="transfer-label">Format</div>
      {FORMATS.map((f) => (
        <label key={f.id} className="transfer-choice">
          <input type="radio" name="fmt" checked={c.format === f.id} onChange={() => setC({ ...c, format: f.id })} />
          <b>{f.label}</b> <span className="muted">{f.hint}</span>
        </label>
      ))}
      <div className="transfer-label">To</div>
      <label className="transfer-choice">
        <input type="radio" name="dest" checked={!c.clipboard} onChange={() => setC({ ...c, clipboard: false })} />
        <b>File…</b> <span className="muted">all rows, choose where to save</span>
      </label>
      <label className="transfer-choice">
        <input type="radio" name="dest" checked={c.clipboard} onChange={() => setC({ ...c, clipboard: true })} />
        <b>Clipboard</b> <span className="muted">all rows, paste anywhere (up to 64 MB)</span>
      </label>
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
        <span className="muted">{file ? `${fileName} · ${file.pv.format.toUpperCase()} · ${(file.pv.bytes / 1048576).toFixed(1)} MB` : "CSV, TSV or JSON (array of objects)"}</span>
      </div>
      {create && (
        <div className="transfer-file">
          <span className="transfer-label" style={{ margin: 0 }}>Table name</span>
          <TextInput value={name} onChange={(e) => setName(e.target.value)} placeholder="new_table" />
          <span className="muted">columns are created as text</span>
        </div>
      )}
      {file && (
        <div className="transfer-map">
          <table>
            <thead>
              <tr>
                <th>File column</th>
                <th>Sample</th>
                <th>{create ? "New column (empty = skip)" : "Table column"}</th>
              </tr>
            </thead>
            <tbody>
              {file.pv.columns.map((fc, i) => (
                <tr key={i}>
                  <td>{fc}</td>
                  <td className="muted">{file.pv.rows.slice(0, 3).map((r) => r[i] ?? "NULL").join(", ")}</td>
                  <td>
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
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
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
          <span key={j.id} className="sb-widget sb-transfer" title={j.label}>
            <span className="sb-transfer-label">{j.label}</span>
            <span className={"sb-bar" + (p.pct === null ? " busy" : "")}>
              <span style={p.pct === null ? undefined : { width: `${p.pct}%` }} />
            </span>
            <span>{p.text}</span>
            <button className="sb-cancel" title="Cancel" onClick={() => api.cancelJob(j.id)}>
              ✕
            </button>
          </span>
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
          <div className="transfer-label">Format</div>
          <label className="transfer-choice">
            <input type="radio" checked={format === "custom"} onChange={() => setFormat("custom")} />
            <b>Custom (.dump)</b> <span className="muted">compressed, restore with Restore… (pg_restore)</span>
          </label>
          <label className="transfer-choice">
            <input type="radio" checked={format === "plain"} onChange={() => setFormat("plain")} />
            <b>SQL (.sql)</b> <span className="muted">plain text, readable, runs in any console</span>
          </label>
        </>
      )}
      <div className="transfer-label">What</div>
      {(["all", "schema", "data"] as const).map((x) => (
        <label key={x} className="transfer-choice">
          <input type="radio" checked={part === x} onChange={() => setPart(x)} />
          <b>{x === "all" ? "Schema and data" : x === "schema" ? "Schema only" : "Data only"}</b>
        </label>
      ))}
      <Hint>Runs {pg ? "pg_dump" : "mysqldump"} on this computer (install the client tools if it's missing). The password is passed through the environment, not the command line.</Hint>
    </Modal>
  );
}
