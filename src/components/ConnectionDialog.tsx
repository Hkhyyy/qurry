import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
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

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">{edit ? "Data Source Properties" : "New Data Source"}</div>
        <div className="dlg-tabs">
          <span className={tab === "general" ? "on" : ""} onClick={() => { setTab("general"); setTest(null); }}>General</span>
          <span className={tab === "ssh" ? "on" : ""} onClick={() => { setTab("ssh"); setTest(null); }}>
            SSH/SSL{ssh.enabled || ssl.enabled ? " ●" : ""}
          </span>
          <span className={tab === "schemas" ? "on" : ""} onClick={() => { setTab("schemas"); setTest(null); }}>
            Schemas{cfg.scope?.databases?.length ? " ●" : ""}
          </span>
          <span className={tab === "options" ? "on" : ""} onClick={() => { setTab("options"); setTest(null); }}>Options</span>
        </div>

        {tab === "general" && (
          <div className="form">
            <label>Name</label>
            <input value={cfg.name} placeholder={`${cfg.user || "user"}@${cfg.host}`} onChange={(e) => set("name", e.target.value)} />
            <label>Driver</label>
            <select
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
            </select>
            {driver !== null && (
              <>
                <label>Plugin</label>
                <div className="row">
                  {driver ? (
                    <span className="lbl">✓ {KIND_LABEL[cfg.kind]} driver installed</span>
                  ) : (
                    <>
                      <span className="lbl">Not installed — downloaded from this version's GitHub release (signature checked)</span>
                      <button className="small" disabled={installing} onClick={installDriver}>
                        {installing ? "Installing…" : "Install"}
                      </button>
                    </>
                  )}
                </div>
              </>
            )}
            {!fileDb && (
              <>
                <label>Host</label>
                <div className="row">
                  <input className="grow" value={cfg.host} placeholder={cfg.kind === "mongodb" ? "host, or a mongodb:// / mongodb+srv:// URI" : ""} onChange={(e) => set("host", e.target.value)} />
                  <span className="lbl">Port</span>
                  <input className="port" value={cfg.port} onChange={(e) => set("port", Number(e.target.value) || 0)} />
                </div>
                <label>User</label>
                <input value={cfg.user} placeholder={cfg.kind === "redis" ? "(optional, ACL user)" : ""} onChange={(e) => set("user", e.target.value)} />
                <label>Password</label>
                <input
                  type="password"
                  value={password ?? ""}
                  placeholder={edit?.hasPassword ? "<saved — leave empty to keep>" : ""}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </>
            )}
            {cfg.kind === "redis" && (
              <>
                <label>Connection</label>
                <select value={rmode.mode} onChange={(e) => setRedis("mode", e.target.value as RedisMode["mode"])}>
                  <option value="standalone">Standalone</option>
                  <option value="sentinel">Sentinel</option>
                  <option value="cluster">Cluster</option>
                </select>
                {rmode.mode === "sentinel" && (
                  <>
                    <label>Master name</label>
                    <input value={rmode.sentinelMaster} placeholder="mymaster" onChange={(e) => setRedis("sentinelMaster", e.target.value)} />
                    <label>Sentinels</label>
                    <input value={rmode.nodes} placeholder="host1:26379, host2:26379 (empty = Host/Port above)" onChange={(e) => setRedis("nodes", e.target.value)} />
                  </>
                )}
                {rmode.mode === "cluster" && (
                  <>
                    <label>Seed nodes</label>
                    <input value={rmode.nodes} placeholder="extra nodes: host2:6379, host3:6379 (optional)" onChange={(e) => setRedis("nodes", e.target.value)} />
                  </>
                )}
              </>
            )}
            <label>{fileDb ? "File" : cfg.kind === "oracle" ? "Service name" : "Database"}</label>
            {fileDb ? (
              <div className="row">
                <input className="grow" value={cfg.database} placeholder={cfg.kind === "duckdb" ? "/path/to/data.duckdb (empty = in memory)" : "/path/to/app.db (created if missing)"} onChange={(e) => set("database", e.target.value)} />
                <button className="small" onClick={pickDbFile}>
                  Browse…
                </button>
              </div>
            ) : (
              <input value={cfg.database} placeholder={cfg.kind === "postgres" ? "postgres" : cfg.kind === "redis" ? "0 (database index)" : cfg.kind === "oracle" ? "FREEPDB1, ORCLPDB1 …" : cfg.kind === "mssql" ? "master" : cfg.kind === "clickhouse" ? "default" : ""} onChange={(e) => set("database", e.target.value)} />
            )}
            {cfg.kind === "oracle" && (
              <>
                <label>Oracle client</label>
                <div className="row">
                  <input className="grow" value={opt.oracleClient ?? ""} placeholder="Instant Client folder (empty = ORACLE_CLIENT_LIB_DIR / system default)" onChange={(e) => setOpt("oracleClient", e.target.value || null)} />
                  <button className="small" onClick={async () => { const p = await open({ directory: true }); if (typeof p === "string") setOpt("oracleClient", p); }}>
                    Browse…
                  </button>
                </div>
              </>
            )}
            <label>Color</label>
            <ColorSwatches value={cfg.color ?? null} onChange={(v) => set("color", v)} />
          </div>
        )}

        {tab === "ssh" && (
          <div className="form">
            <label />
            <label className="check">
              <input type="checkbox" checked={ssh.enabled} onChange={(e) => setSsh("enabled", e.target.checked)} /> Use SSH tunnel
            </label>
            <label>Host</label>
            <div className="row">
              <input className="grow" disabled={!ssh.enabled} value={ssh.host} placeholder="bastion.example.com" onChange={(e) => setSsh("host", e.target.value)} />
              <span className="lbl">Port</span>
              <input className="port" disabled={!ssh.enabled} value={ssh.port} onChange={(e) => setSsh("port", Number(e.target.value) || 22)} />
            </div>
            <label>User</label>
            <input disabled={!ssh.enabled} value={ssh.user} onChange={(e) => setSsh("user", e.target.value)} />
            <label>Auth type</label>
            <select disabled={!ssh.enabled} value={ssh.auth} onChange={(e) => setSsh("auth", e.target.value as SshConfig["auth"])}>
              <option value="key">Key pair</option>
              <option value="password">Password</option>
            </select>
            {ssh.auth === "key" && (
              <>
                <label>Private key</label>
                <div className="row">
                  <input className="grow" disabled={!ssh.enabled} value={ssh.keyPath} onChange={(e) => setSsh("keyPath", e.target.value)} />
                  <button disabled={!ssh.enabled} onClick={browseKey}>…</button>
                </div>
              </>
            )}
            <label>{ssh.auth === "key" ? "Passphrase" : "Password"}</label>
            <input
              type="password"
              disabled={!ssh.enabled}
              value={sshSecret ?? ""}
              placeholder={edit?.ssh?.enabled ? "<saved — leave empty to keep>" : ssh.auth === "key" ? "(none)" : ""}
              onChange={(e) => setSshSecret(e.target.value)}
            />
            <span className="form-section">SSL / TLS</span>
            <label />
            <label className="check">
              <input type="checkbox" checked={ssl.enabled} onChange={(e) => setSsl("enabled", e.target.checked)} /> Use SSL{cfg.kind === "redis" ? " (rediss://)" : ""}
            </label>
            <label>Mode</label>
            <select disabled={!ssl.enabled} value={ssl.mode || "require"} onChange={(e) => setSsl("mode", e.target.value)}>
              <option value="require">Require (encrypt, don't verify)</option>
              <option value="verify-ca">Verify CA</option>
              <option value="verify-full">Full Verification (CA + host name)</option>
            </select>
            {(["caFile", "certFile", "keyFile"] as const).map((k) => (
              <span key={k} style={{ display: "contents" }}>
                <label>{k === "caFile" ? "CA file" : k === "certFile" ? "Client cert" : "Client key"}</label>
                <div className="row">
                  <input className="grow" disabled={!ssl.enabled} value={ssl[k]} placeholder={k === "caFile" ? "(system trust store)" : "(none)"} onChange={(e) => setSsl(k, e.target.value)} />
                  <button disabled={!ssl.enabled} onClick={() => pickFile(k)}>…</button>
                </div>
              </span>
            ))}
          </div>
        )}

        {tab === "schemas" && (
          <SchemasTab
            connId={edit?.id}
            scope={cfg.scope ?? {}}
            onChange={(scope) => setCfg((c) => ({ ...c, scope }))}
          />
        )}

        {tab === "options" && (
          <div className="form options-form">
            <span className="form-section">Connection</span>
            <span />
            <label className="check">
              <input type="checkbox" checked={!!cfg.readOnly} onChange={(e) => set("readOnly", e.target.checked)} />
              Read-only
            </label>
            <label>Transaction control</label>
            <div className="row">
              <select value={opt.txManual ? "manual" : "auto"} onChange={(e) => setOpt("txManual", e.target.value === "manual")}>
                <option value="auto">Auto</option>
                <option value="manual">Manual</option>
              </select>
              <select value={opt.isolation ?? ""} onChange={(e) => setOpt("isolation", e.target.value || null)} title="Isolation level">
                <option value="">Database Default</option>
                <option value="read uncommitted">Read Uncommitted</option>
                <option value="read committed">Read Committed</option>
                <option value="repeatable read">Repeatable Read</option>
                <option value="serializable">Serializable</option>
              </select>
            </div>
            <label>Switch schema</label>
            <select value={opt.switchSchema ?? "auto"} onChange={(e) => setOpt("switchSchema", e.target.value as ConnOptions["switchSchema"])}>
              <option value="auto">Automatic</option>
              <option value="manual">Manual</option>
              <option value="disable">Disable</option>
            </select>
            <label>Time zone</label>
            <input value={opt.timeZone ?? ""} placeholder="Database default (e.g. Asia/Seoul, UTC)" onChange={(e) => setOpt("timeZone", e.target.value || null)} />
            <span />
            <div className="row">
              <label className="check">
                <input type="checkbox" checked={!!opt.keepAliveSecs} onChange={(e) => setOpt("keepAliveSecs", e.target.checked ? 60 : null)} />
                Run keep-alive query each
              </label>
              <input className="port" type="number" min={1} disabled={!opt.keepAliveSecs} value={opt.keepAliveSecs ?? 60} onChange={(e) => setOpt("keepAliveSecs", Math.max(1, Number(e.target.value) || 1))} />
              <span className="lbl">sec.</span>
            </div>
            <span />
            <div className="row">
              <label className="check">
                <input type="checkbox" checked={!!opt.autoDisconnectSecs} onChange={(e) => setOpt("autoDisconnectSecs", e.target.checked ? 300 : null)} />
                Auto-disconnect after
              </label>
              <input className="port" type="number" min={1} disabled={!opt.autoDisconnectSecs} value={opt.autoDisconnectSecs ?? 300} onChange={(e) => setOpt("autoDisconnectSecs", Math.max(1, Number(e.target.value) || 1))} />
              <span className="lbl">sec.</span>
            </div>
            <span />
            <label className="check">
              <input type="checkbox" checked={!!opt.singleDb} onChange={(e) => setOpt("singleDb", e.target.checked)} />
              Single database mode
            </label>
            <span />
            <label className="check" title="Before UPDATE / DELETE / INSERT … run, show how many rows will change and ask">
              <input type="checkbox" checked={!!opt.confirmChanges} onChange={(e) => setOpt("confirmChanges", e.target.checked)} />
              Production: show affected rows and confirm changes
            </label>
            <label className="top">Startup script</label>
            <textarea
              className="startup-script"
              spellCheck={false}
              rows={4}
              placeholder="SQL executed on every new connection, e.g. SET search_path TO app, public;"
              value={opt.startupScript ?? ""}
              onChange={(e) => setOpt("startupScript", e.target.value)}
            />
          </div>
        )}

        <div className="hint">
          {tab === "options"
            ? "Changes apply on the next connection (the data source is reconnected on OK)."
            : tab === "schemas"
            ? "Only checked databases and schemas are shown in the Database Explorer. Nothing checked = show all."
            : tab === "ssh"
            ? "The database host/port on the General tab are resolved from the SSH server. Host keys are checked against ~/.ssh/known_hosts."
            : "Passwords are stored encrypted in the app data folder (no Keychain prompts)."}
        </div>
        {test && <div className={test.ok ? "test-ok" : "test-err"}>{test.ok ? "✓ " : "✕ "}{test.msg}</div>}
        {test && !test.ok && test.msg.includes(DRIVER_MISSING) && (
          <div className="modal-actions" style={{ marginTop: 6 }}>
            <button className="primary" disabled={installing} onClick={installDriver}>
              {installing ? "Installing…" : `Install the ${KIND_LABEL[cfg.kind]} driver`}
            </button>
          </div>
        )}
        {/* 처음 보는 SSH 서버: 지문을 확인한 뒤 신뢰 → 다시 시험 */}
        {test && !test.ok && test.msg.includes("Unknown SSH host key") && cfg.ssh && (
          <div className="row" style={{ marginTop: 6 }}>
            <button
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
            </button>
          </div>
        )}
        <div className="modal-actions">
          <button onClick={runTest} disabled={busy}>{busy ? "Testing…" : tab === "ssh" ? "Test SSH Connection" : "Test Connection"}</button>
          <span className="grow" />
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={save}>OK</button>
        </div>
      </div>
    </div>
  );
}

export function ColorSwatches({ value, onChange }: { value: string | null; onChange: (v: string | null) => void }) {
  return (
    <div className="swatches">
      {COLOR_PRESETS.map((c) => (
        <span
          key={c.name}
          title={c.name}
          className={"swatch" + (c.value === value ? " on" : "") + (c.value ? "" : " none")}
          style={c.value ? { background: c.value } : undefined}
          onClick={() => onChange(c.value)}
        />
      ))}
    </div>
  );
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

  if (!connId) return <div className="empty">Save the data source first, then choose the databases and schemas to show.</div>;
  if (err) return <div className="test-err">{err}</div>;
  if (!dbs) return <div className="empty">Loading databases…</div>;

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
              <span className="muted">{selSch.length ? `${selSch.length} schemas` : "all schemas"}</span>
            </div>
            {openDb === db && (
              <div className="scope-children">
                {sch === "loading" && <div className="muted">Loading…</div>}
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
