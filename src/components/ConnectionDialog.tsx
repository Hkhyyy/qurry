import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, ColorPicker, EmptyState, FormGrid, FormSection, Hint, Modal, Notice, Select, Spacer, Tabs, Text, TextArea, TextInput } from "../ui";
import { api, COLOR_PRESETS, ConnOptions, RedisMode, SslConfig, ConnectionConfig, ConnectionView, DbKind, DEFAULT_PORT, DRIVER_MISSING, isFileKind, KIND_LABEL, meta, SchemaInfo, SshConfig, targetId } from "../lib/api";

interface Props {
  edit?: ConnectionView;
  onClose: () => void;
  onSaved: (saved: ConnectionConfig) => void;
}

const EMPTY_SSH: SshConfig = { enabled: false, host: "", port: 22, user: "", auth: "key", keyPath: "~/.ssh/id_ed25519" };

export function ConnectionDialog({ edit, onClose, onSaved }: Props) {
  const [cfg, setCfg] = useState<ConnectionConfig>(
    edit
      ? {
          id: edit.id, name: edit.name, kind: edit.kind, host: edit.host, port: edit.port, user: edit.user,
          database: edit.database, color: edit.color ?? null, ssh: edit.ssh ?? null,
          scope: edit.scope, readOnly: edit.readOnly ?? false, options: edit.options ?? {}, ssl: edit.ssl ?? null, redis: edit.redis ?? null,
        }
      : { id: "", name: "", kind: "postgres", host: "localhost", port: 5432, user: "", database: "", color: null, ssh: null },
  );
  const [tab, setTab] = useState<"general" | "ssh" | "schemas" | "options">("general");
  const [password, setPassword] = useState<string | undefined>(undefined);
  const [sshSecret, setSshSecret] = useState<string | undefined>(undefined);
  const [test, setTest] = useState<{ ok: boolean; msg: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // 드라이버 플러그인: 깔려 있는지 (null = 기본으로 들어 있는 DB)
  const [driver, setDriver] = useState<boolean | null>(null);
  const [installing, setInstalling] = useState(false);
  useEffect(() => {
    api.driverStatus(cfg.kind).then(setDriver, () => setDriver(null));
  }, [cfg.kind]);
  const installDriver = async () => {
    setInstalling(true);
    setTest(null);
    try {
      await api.installDriver(cfg.kind);
      setDriver(true);
      setTest({ ok: true, msg: `${KIND_LABEL[cfg.kind]} driver installed` });
    } catch (e) {
      setTest({ ok: false, msg: String(e) });
    } finally {
      setInstalling(false);
    }
  };
  const fileDb = isFileKind(cfg.kind);
  const pickDbFile = async () => {
    const p = await open({ multiple: false });
    if (typeof p === "string") setCfg((c) => ({ ...c, database: p, name: c.name || p.split(/[\\/]/).pop() || "" }));
  };

  const ssh = cfg.ssh ?? EMPTY_SSH;
  const opt: ConnOptions = cfg.options ?? {};
  const ssl: SslConfig = cfg.ssl ?? { enabled: false, mode: "require", caFile: "", certFile: "", keyFile: "" };
  const setSsl = <K extends keyof SslConfig>(k: K, v: SslConfig[K]) => setCfg((c) => ({ ...c, ssl: { ...ssl, ...(c.ssl ?? {}), [k]: v } }));
  const rmode: RedisMode = cfg.redis ?? { mode: "standalone", sentinelMaster: "", nodes: "" };
  const setRedis = <K extends keyof RedisMode>(k: K, v: RedisMode[K]) => setCfg((c) => ({ ...c, redis: { ...rmode, ...(c.redis ?? {}), [k]: v } }));
  const pickFile = async (k: "caFile" | "certFile" | "keyFile") => {
    const p = await open({ multiple: false });
    if (typeof p === "string") setSsl(k, p);
  };
  const setOpt = <K extends keyof ConnOptions>(k: K, v: ConnOptions[K]) => setCfg((c) => ({ ...c, options: { ...(c.options ?? {}), [k]: v } }));
  const set = <K extends keyof ConnectionConfig>(k: K, v: ConnectionConfig[K]) => setCfg((c) => ({ ...c, [k]: v }));
  const setSsh = <K extends keyof SshConfig>(k: K, v: SshConfig[K]) => setCfg((c) => ({ ...c, ssh: { ...(c.ssh ?? EMPTY_SSH), [k]: v } }));

  const runTest = async () => {
    setBusy(true);
    setTest(null);
    try {
      // SSH 탭에서는 SSH 로그인만, General 탭에서는 (SSH 경유) DB 접속까지 확인
      const v = tab === "ssh" ? await api.testSsh(cfg, sshSecret) : await api.testConnection(cfg, password, sshSecret);
      setTest({ ok: true, msg: v });
    } catch (e) {
      setTest({ ok: false, msg: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    const name = cfg.name.trim() || `${cfg.user}@${cfg.host}`;
    try {
      const saved = await api.saveConnection({ ...cfg, name }, password, sshSecret);
      onSaved(saved);
    } catch (e) {
      setTest({ ok: false, msg: `Save failed: ${String(e)}` });
    }
  };

  const browseKey = async () => {
    const home = "~/.ssh";
    const p = await open({ multiple: false, defaultPath: home });
    if (typeof p === "string") setSsh("keyPath", p);
  };

  const tabHint =
    tab === "options"
      ? "Changes apply on the next connection (the data source is reconnected on OK)."
      : tab === "schemas"
      ? "Only checked databases and schemas are shown in the Database Explorer. Nothing checked = show all."
      : tab === "ssh"
      ? "The database host/port on the General tab are resolved from the SSH server. Host keys are checked against ~/.ssh/known_hosts."
      : "Passwords are stored encrypted in the app data folder (no Keychain prompts).";
  const port = (v: number, on: (n: number) => void, fallback: number, disabled?: boolean) => <TextInput className="w-port" disabled={disabled} value={v} onChange={(e) => on(Number(e.target.value) || fallback)} />;

  return (
    <Modal
      title={edit ? "Data Source Properties" : "New Data Source"}
      size="lg"
      stickyBackdrop
      onClose={onClose}
      footer={
        <>
          <Button onClick={runTest} disabled={busy}>
            {busy ? "Testing…" : tab === "ssh" ? "Test SSH Connection" : "Test Connection"}
          </Button>
          <Spacer />
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={save}>
            OK
          </Button>
        </>
      }
    >
      <Tabs
        items={[
          { id: "general", label: "General" },
          { id: "ssh", label: `SSH/SSL${ssh.enabled || ssl.enabled ? " ●" : ""}` },
          { id: "schemas", label: `Schemas${cfg.scope?.databases?.length ? " ●" : ""}` },
          { id: "options", label: "Options" },
        ]}
        value={tab}
        onChange={(t) => (setTab(t), setTest(null))}
      />

      {tab === "general" && (
        <FormGrid>
          <label>Name</label>
          <TextInput value={cfg.name} placeholder={`${cfg.user || "user"}@${cfg.host}`} onChange={(e) => set("name", e.target.value)} />
          <label>Driver</label>
          <Select
            value={cfg.kind}
            onChange={(e) => {
              const k = e.target.value as DbKind;
              setCfg((c) => ({ ...c, kind: k, port: DEFAULT_PORT[k] }));
            }}
          >
            {(Object.keys(KIND_LABEL) as DbKind[]).map((k) => (
              <option key={k} value={k}>
                {KIND_LABEL[k]}
              </option>
            ))}
          </Select>
          {driver !== null && (
            <>
              <label>Plugin</label>
              <div className="ui-row">
                {driver ? (
                  <Text tone="muted">✓ {KIND_LABEL[cfg.kind]} driver installed</Text>
                ) : (
                  <>
                    <Text tone="muted" truncate title="Downloaded from this version's GitHub release (signature checked)">
                      Not installed — downloaded from this version's GitHub release (signature checked)
                    </Text>
                    <Button size="sm" variant="primary" disabled={installing} onClick={installDriver}>
                      {installing ? "Installing…" : "Install"}
                    </Button>
                  </>
                )}
              </div>
            </>
          )}
          {!fileDb && (
            <>
              <label>Host</label>
              <div className="ui-row">
                <TextInput grow value={cfg.host} placeholder={cfg.kind === "mongodb" ? "host, or a mongodb:// / mongodb+srv:// URI" : ""} onChange={(e) => set("host", e.target.value)} />
                <Text tone="muted">Port</Text>
                {port(cfg.port, (n) => set("port", n), 0)}
              </div>
              <label>User</label>
              <TextInput value={cfg.user} placeholder={cfg.kind === "redis" ? "(optional, ACL user)" : cfg.kind === "milvus" ? "(optional — empty: the password is an API key)" : ""} onChange={(e) => set("user", e.target.value)} />
              <label>{cfg.kind === "qdrant" || cfg.kind === "chroma" ? "API key" : "Password"}</label>
              <TextInput type="password" value={password ?? ""} placeholder={edit?.hasPassword ? "<saved — leave empty to keep>" : ""} onChange={(e) => setPassword(e.target.value)} />
            </>
          )}
          {cfg.kind === "redis" && (
            <>
              <label>Connection</label>
              <Select value={rmode.mode} onChange={(e) => setRedis("mode", e.target.value as RedisMode["mode"])}>
                <option value="standalone">Standalone</option>
                <option value="sentinel">Sentinel</option>
                <option value="cluster">Cluster</option>
              </Select>
              {rmode.mode === "sentinel" && (
                <>
                  <label>Master name</label>
                  <TextInput value={rmode.sentinelMaster} placeholder="mymaster" onChange={(e) => setRedis("sentinelMaster", e.target.value)} />
                  <label>Sentinels</label>
                  <TextInput value={rmode.nodes} placeholder="host1:26379, host2:26379 (empty = Host/Port above)" onChange={(e) => setRedis("nodes", e.target.value)} />
                </>
              )}
              {rmode.mode === "cluster" && (
                <>
                  <label>Seed nodes</label>
                  <TextInput value={rmode.nodes} placeholder="extra nodes: host2:6379, host3:6379 (optional)" onChange={(e) => setRedis("nodes", e.target.value)} />
                </>
              )}
            </>
          )}
          <label>{fileDb ? "File" : cfg.kind === "oracle" ? "Service name" : "Database"}</label>
          {fileDb ? (
            <div className="ui-row">
              <TextInput grow value={cfg.database} placeholder={cfg.kind === "duckdb" ? "/path/to/data.duckdb (empty = in memory)" : "/path/to/app.db (created if missing)"} onChange={(e) => set("database", e.target.value)} />
              <Button size="sm" onClick={pickDbFile}>
                Browse…
              </Button>
            </div>
          ) : (
            <TextInput
              value={cfg.database}
              placeholder={cfg.kind === "postgres" ? "postgres" : cfg.kind === "redis" ? "0 (database index)" : cfg.kind === "oracle" ? "FREEPDB1, ORCLPDB1 …" : cfg.kind === "mssql" ? "master" : cfg.kind === "clickhouse" ? "default" : cfg.kind === "chroma" ? "default_database" : cfg.kind === "milvus" ? "default" : ""}
              onChange={(e) => set("database", e.target.value)}
            />
          )}
          {cfg.kind === "oracle" && (
            <>
              <label>Oracle client</label>
              <div className="ui-row">
                <TextInput grow value={opt.oracleClient ?? ""} placeholder="Instant Client folder (empty = ORACLE_CLIENT_LIB_DIR / system default)" onChange={(e) => setOpt("oracleClient", e.target.value || null)} />
                <Button
                  size="sm"
                  onClick={async () => {
                    const p = await open({ directory: true });
                    if (typeof p === "string") setOpt("oracleClient", p);
                  }}
                >
                  Browse…
                </Button>
              </div>
            </>
          )}
          <label>Color</label>
          <ColorSwatches value={cfg.color ?? null} onChange={(v) => set("color", v)} />
        </FormGrid>
      )}

      {tab === "ssh" && (
        <FormGrid>
          <FormSection>SSH tunnel</FormSection>
          <span />
          <Checkbox checked={ssh.enabled} onChange={(v) => setSsh("enabled", v)}>
            Use SSH tunnel
          </Checkbox>
          <label>Host</label>
          <div className="ui-row">
            <TextInput grow disabled={!ssh.enabled} value={ssh.host} placeholder="bastion.example.com" onChange={(e) => setSsh("host", e.target.value)} />
            <Text tone="muted">Port</Text>
            {port(ssh.port, (n) => setSsh("port", n), 22, !ssh.enabled)}
          </div>
          <label>User</label>
          <TextInput disabled={!ssh.enabled} value={ssh.user} onChange={(e) => setSsh("user", e.target.value)} />
          <label>Auth type</label>
          <Select disabled={!ssh.enabled} value={ssh.auth} onChange={(e) => setSsh("auth", e.target.value as SshConfig["auth"])}>
            <option value="key">Key pair</option>
            <option value="password">Password</option>
          </Select>
          {ssh.auth === "key" && (
            <>
              <label>Private key</label>
              <div className="ui-row">
                <TextInput grow disabled={!ssh.enabled} value={ssh.keyPath} onChange={(e) => setSsh("keyPath", e.target.value)} />
                <Button size="sm" disabled={!ssh.enabled} onClick={browseKey}>
                  Browse…
                </Button>
              </div>
            </>
          )}
          <label>{ssh.auth === "key" ? "Passphrase" : "Password"}</label>
          <TextInput type="password" disabled={!ssh.enabled} value={sshSecret ?? ""} placeholder={edit?.ssh?.enabled ? "<saved — leave empty to keep>" : ssh.auth === "key" ? "(none)" : ""} onChange={(e) => setSshSecret(e.target.value)} />
          <FormSection>SSL / TLS</FormSection>
          <span />
          <Checkbox checked={ssl.enabled} onChange={(v) => setSsl("enabled", v)}>
            Use SSL{cfg.kind === "redis" ? " (rediss://)" : ""}
          </Checkbox>
          <label>Mode</label>
          <Select disabled={!ssl.enabled} value={ssl.mode || "require"} onChange={(e) => setSsl("mode", e.target.value)}>
            <option value="require">Require (encrypt, don't verify)</option>
            <option value="verify-ca">Verify CA</option>
            <option value="verify-full">Full Verification (CA + host name)</option>
          </Select>
          {(["caFile", "certFile", "keyFile"] as const).map((k) => (
            <span key={k} style={{ display: "contents" }}>
              <label>{k === "caFile" ? "CA file" : k === "certFile" ? "Client cert" : "Client key"}</label>
              <div className="ui-row">
                <TextInput grow disabled={!ssl.enabled} value={ssl[k]} placeholder={k === "caFile" ? "(system trust store)" : "(none)"} onChange={(e) => setSsl(k, e.target.value)} />
                <Button size="sm" disabled={!ssl.enabled} onClick={() => pickFile(k)}>
                  Browse…
                </Button>
              </div>
            </span>
          ))}
        </FormGrid>
      )}

      {tab === "schemas" && <SchemasTab connId={edit?.id} scope={cfg.scope ?? {}} onChange={(scope) => setCfg((c) => ({ ...c, scope }))} />}

      {tab === "options" && (
        <FormGrid labelWidth={140}>
          <FormSection>Connection</FormSection>
          <span />
          <Checkbox checked={!!cfg.readOnly} onChange={(v) => set("readOnly", v)}>
            Read-only
          </Checkbox>
          <span />
          <Checkbox checked={!!opt.confirmChanges} onChange={(v) => setOpt("confirmChanges", v)} title="Before UPDATE / DELETE / INSERT … run, show how many rows will change and ask">
            Production: show affected rows and confirm changes
          </Checkbox>
          <label>Transaction control</label>
          <div className="ui-row">
            <Select value={opt.txManual ? "manual" : "auto"} onChange={(e) => setOpt("txManual", e.target.value === "manual")}>
              <option value="auto">Auto</option>
              <option value="manual">Manual</option>
            </Select>
            <Select value={opt.isolation ?? ""} onChange={(e) => setOpt("isolation", e.target.value || null)} title="Isolation level">
              <option value="">Database Default</option>
              <option value="read uncommitted">Read Uncommitted</option>
              <option value="read committed">Read Committed</option>
              <option value="repeatable read">Repeatable Read</option>
              <option value="serializable">Serializable</option>
            </Select>
          </div>
          <label>Switch schema</label>
          <Select className="w-auto" value={opt.switchSchema ?? "auto"} onChange={(e) => setOpt("switchSchema", e.target.value as ConnOptions["switchSchema"])}>
            <option value="auto">Automatic</option>
            <option value="manual">Manual</option>
            <option value="disable">Disable</option>
          </Select>
          <label>Time zone</label>
          <TextInput value={opt.timeZone ?? ""} placeholder="Database default (e.g. Asia/Seoul, UTC)" onChange={(e) => setOpt("timeZone", e.target.value || null)} />
          <span />
          <div className="ui-row">
            <Checkbox checked={!!opt.keepAliveSecs} onChange={(v) => setOpt("keepAliveSecs", v ? 60 : null)}>
              Run keep-alive query each
            </Checkbox>
            <TextInput className="w-port" type="number" min={1} disabled={!opt.keepAliveSecs} value={opt.keepAliveSecs ?? 60} onChange={(e) => setOpt("keepAliveSecs", Math.max(1, Number(e.target.value) || 1))} />
            <Text tone="muted">sec.</Text>
          </div>
          <span />
          <div className="ui-row">
            <Checkbox checked={!!opt.autoDisconnectSecs} onChange={(v) => setOpt("autoDisconnectSecs", v ? 300 : null)}>
              Auto-disconnect after
            </Checkbox>
            <TextInput className="w-port" type="number" min={1} disabled={!opt.autoDisconnectSecs} value={opt.autoDisconnectSecs ?? 300} onChange={(e) => setOpt("autoDisconnectSecs", Math.max(1, Number(e.target.value) || 1))} />
            <Text tone="muted">sec.</Text>
          </div>
          <span />
          <Checkbox checked={!!opt.singleDb} onChange={(v) => setOpt("singleDb", v)}>
            Single database mode
          </Checkbox>
          <label className="top">Startup script</label>
          <TextArea
            className="ui-textarea"
            spellCheck={false}
            rows={4}
            placeholder="SQL executed on every new connection, e.g. SET search_path TO app, public;"
            value={opt.startupScript ?? ""}
            onChange={(e) => setOpt("startupScript", e.target.value)}
          />
        </FormGrid>
      )}

      <Hint>{tabHint}</Hint>
      {test && <Notice tone={test.ok ? "ok" : "danger"}>{(test.ok ? "✓ " : "✕ ") + test.msg}</Notice>}
      {test && !test.ok && test.msg.includes(DRIVER_MISSING) && (
        <div className="ui-row conn-test-actions">
          <Button variant="primary" disabled={installing} onClick={installDriver}>
            {installing ? "Installing…" : `Install the ${KIND_LABEL[cfg.kind]} driver`}
          </Button>
        </div>
      )}
      {/* 처음 보는 SSH 서버: 지문을 확인한 뒤 신뢰 → 다시 시험 */}
      {test && !test.ok && test.msg.includes("Unknown SSH host key") && cfg.ssh && (
        <div className="ui-row conn-test-actions">
          <Button
            onClick={async () => {
              try {
                await api.trustSshHost(cfg.ssh!.host, cfg.ssh!.port);
                await runTest();
              } catch (e) {
                setTest({ ok: false, msg: String(e) });
              }
            }}
          >
            Trust Host Key and Test Again
          </Button>
        </div>
      )}
    </Modal>
  );
}

export function ColorSwatches({ value, onChange }: { value: string | null; onChange: (v: string | null) => void }) {
  return <ColorPicker value={value} options={COLOR_PRESETS} onChange={onChange} />;
}

type Scope = NonNullable<ConnectionConfig["scope"]>;

/** DataGrip Data Source 속성의 Schemas 탭: 트리에 보일 DB·스키마를 고른다 */
function SchemasTab({ connId, scope, onChange }: { connId?: string; scope: Scope; onChange: (s: Scope) => void }) {
  const [dbs, setDbs] = useState<string[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [schemas, setSchemas] = useState<Record<string, SchemaInfo[] | "loading">>({});
  const [openDb, setOpenDb] = useState<string | null>(null);

  useEffect(() => {
    if (!connId) return;
    meta.databases(connId).then(setDbs, (e) => setErr(String(e)));
  }, [connId]);

  if (!connId) return <EmptyState>Save the data source first, then choose the databases and schemas to show.</EmptyState>;
  if (err) return <Notice tone="danger">{err}</Notice>;
  if (!dbs) return <EmptyState>Loading databases…</EmptyState>;

  const selDbs = scope.databases ?? [];
  const allDbs = selDbs.length === 0;
  const toggleDb = (db: string) => {
    const next = selDbs.includes(db) ? selDbs.filter((d) => d !== db) : [...selDbs, db];
    onChange({ ...scope, databases: next.length ? next : null });
  };
  const loadSchemas = (db: string) => {
    setOpenDb(openDb === db ? null : db);
    if (schemas[db]) return;
    setSchemas((s) => ({ ...s, [db]: "loading" }));
    meta.schemas(targetId(connId, db)).then(
      (list) => setSchemas((s) => ({ ...s, [db]: list.filter((x) => !x.isSystem) })),
      () => setSchemas((s) => ({ ...s, [db]: [] })),
    );
  };
  const toggleSchema = (db: string, name: string) => {
    const cur = scope.schemas?.[db] ?? [];
    const next = cur.includes(name) ? cur.filter((x) => x !== name) : [...cur, name];
    const all = { ...(scope.schemas ?? {}) };
    if (next.length) all[db] = next;
    else delete all[db];
    onChange({ ...scope, schemas: all });
  };

  return (
    <div className="scope-list">
      <label className="scope-row all">
        <input type="checkbox" checked={allDbs} onChange={() => onChange({ ...scope, databases: null })} /> All databases
      </label>
      {dbs.map((db) => {
        const sch = schemas[db];
        const selSch = scope.schemas?.[db] ?? [];
        return (
          <div key={db}>
            <div className="scope-row">
              <span className="twisty" onClick={() => loadSchemas(db)}>{openDb === db ? "▾" : "▸"}</span>
              <label>
                <input type="checkbox" checked={selDbs.includes(db)} onChange={() => toggleDb(db)} /> {db}
              </label>
              <Text tone="muted">{selSch.length ? `${selSch.length} schemas` : "all schemas"}</Text>
            </div>
            {openDb === db && (
              <div className="scope-children">
                {sch === "loading" && <Text as="div" tone="muted">Loading…</Text>}
                {Array.isArray(sch) &&
                  sch.map((x) => (
                    <label key={x.name} className="scope-row">
                      <input type="checkbox" checked={selSch.includes(x.name)} onChange={() => toggleSchema(db, x.name)} /> {x.name}
                    </label>
                  ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
