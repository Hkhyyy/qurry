import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, ColorPicker, EmptyState, FormGrid, FormSection, Hint, Modal, Notice, Select, Spacer, Tabs, Text, TextArea, TextInput, TlsOptIn } from "../ui";
import { needsTlsOptIn } from "./TlsFix";
import { api, COLOR_PRESETS, ConnOptions, RedisMode, SslConfig, ConnectionConfig, ConnectionView, DbKind, DEFAULT_PORT, defaultDbOf, DRIVER_MISSING, isFileKind, isSystemDb, KIND_LABEL, meta, SCOPE_ALL, SchemaInfo, SshConfig, SsmConfig, targetId } from "../lib/api";
import { scopeChecked, toggleScoped } from "../lib/scopeToggle";
import { elapsedText } from "./RunningBar";
import { sharedOr, useSharedReady } from "../lib/shared";

interface Props {
  edit?: ConnectionView;
  onClose: () => void;
  onSaved: (saved: ConnectionConfig) => void;
}

const EMPTY_SSH: SshConfig = { enabled: false, host: "", port: 22, user: "", auth: "key", keyPath: "~/.ssh/id_ed25519" };
const EMPTY_SSM: SsmConfig = { enabled: false, profile: "", region: "", target: "" };

/** "Testing… 12.3 s" while a Test Connection runs (an AWS SSM session can take up to 30 s) */
function TestElapsed({ label = "Testing…" }: { label?: string }) {
  const [start] = useState(Date.now);
  const [now, setNow] = useState(start);
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, []);
  return (
    <span role="status">
      <Text size="sm">
        {label} <b className="running-time">{elapsedText(now - start)}</b>
      </Text>
    </span>
  );
}

export function ConnectionDialog({ edit, onClose, onSaved }: Props) {
  const [cfg, setCfg] = useState<ConnectionConfig>(
    edit
      ? {
          id: edit.id, name: edit.name, kind: edit.kind, host: edit.host, port: edit.port, user: edit.user,
          database: edit.database, color: edit.color ?? null, ssh: edit.ssh ?? null, ssm: edit.ssm ?? null,
          scope: edit.scope, readOnly: edit.readOnly ?? false, options: edit.options ?? {}, ssl: edit.ssl ?? null, redis: edit.redis ?? null,
        }
      : // New sources start hidden from MCP (turn it on in Options › MCP)
        { id: "", name: "", kind: "postgres", host: "localhost", port: 5432, user: "", database: "", color: null, ssh: null, options: { mcp: "off" } },
  );
  const [tab, setTab] = useState<"general" | "ssh" | "schemas" | "options">("general");
  const [password, setPassword] = useState<string | undefined>(undefined);
  const [sshSecret, setSshSecret] = useState<string | undefined>(undefined);
  const [test, setTest] = useState<{ ok: boolean; msg: string } | null>(null);
  // The running test's id (cancelTest stops it, and with it its SSH / SSM tunnel); closing the dialog stops it too
  const [testing, setTesting] = useState<string | null>(null);
  const testingRef = useRef<string | null>(null);
  useEffect(() => () => void (testingRef.current && api.cancelTest(testingRef.current)), []);
  // A new data source's Schemas tab while it connects to list the databases: Esc stops that first, as it stops a test
  const cancelListing = useRef<(() => void) | null>(null);
  // The Allow button under a failed test goes away when it tests again: focus moves to Test Connection first (the same button
  // becomes Cancel Test and back), so Escape cancels the test, then closes the dialog
  const testButton = useRef<HTMLButtonElement>(null);
  // The dialog keeps its size, so a long result (an SSM error ends with what to run or install) can end below the body: show its end
  const testEnd = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (test) testEnd.current?.scrollIntoView({ block: "nearest" });
  }, [test]);
  // Driver plugin: whether it is installed (null = built-in DB)
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
  const ssm = cfg.ssm ?? EMPTY_SSM;
  const tunnel: "none" | "ssh" | "ssm" = ssm.enabled ? "ssm" : ssh.enabled ? "ssh" : "none";
  const opt: ConnOptions = cfg.options ?? {};
  // MCP for older sources that never set it: behavior at the time the dialog opened (hidden if Production). Editing doesn't change this value
  const [legacyMcp] = useState<"off" | "write">(() => (edit?.options?.confirmChanges ? "off" : edit ? "write" : "off"));
  // A value core doesn't know ("On", a typo) means off there, so show and save it as off
  const mcpSet: "off" | "read" | "write" = opt.mcp == null ? legacyMcp : ["off", "read", "write"].includes(opt.mcp) ? opt.mcp : "off";
  // Read-only and Production sources have no MCP write (core: ConnectionConfig::mcp_access): ticking either shows and saves Read
  const noMcpWrite = !!cfg.readOnly || !!opt.confirmChanges;
  const mcpValue = mcpSet === "write" && noMcpWrite ? "read" : mcpSet;
  // Newly enabled SSL is require: verifying needs a CA the built-in list knows (AWS RDS, a company CA, a server behind an SSH tunnel need a CA file)
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
  const setSsm = <K extends keyof SsmConfig>(k: K, v: SsmConfig[K]) => setCfg((c) => ({ ...c, ssm: { ...(c.ssm ?? EMPTY_SSM), [k]: v } }));
  // One tunnel at a time; the other keeps its fields (a source that never used SSM gets no ssm settings).
  // A running test and the last result were for the other tunnel: stop it and drop both (no result for the old tunnel shows up later)
  const setTunnel = (t: typeof tunnel) => {
    if (testingRef.current) void api.cancelTest(testingRef.current);
    testingRef.current = null;
    setTesting(null);
    setTest(null);
    setCfg((c) => ({
      ...c,
      ssh: c.ssh || t === "ssh" ? { ...(c.ssh ?? EMPTY_SSH), enabled: t === "ssh" } : c.ssh,
      ssm: c.ssm || t === "ssm" ? { ...(c.ssm ?? EMPTY_SSM), enabled: t === "ssm" } : c.ssm,
    }));
  };
  const sshTest = tab === "ssh" && tunnel === "ssh";

  const runTest = async (c = cfg) => {
    const id = crypto.randomUUID();
    testingRef.current = id;
    setTesting(id);
    setTest(null);
    let result: { ok: boolean; msg: string };
    try {
      // On the SSH tab, test only the SSH login; otherwise the DB connection through the tunnel (AWS SSM starts a session for it)
      result = { ok: true, msg: sshTest ? await api.testSsh(c, sshSecret, id) : await api.testConnection(c, password, sshSecret, id) };
    } catch (e) {
      result = { ok: false, msg: String(e) };
    }
    // Dropped meanwhile (the Tunnel changed): its result is for settings no longer shown
    if (testingRef.current !== id) return;
    testingRef.current = null;
    setTesting(null);
    setTest(result);
  };
  const cancelTest = () => void (testing && api.cancelTest(testing));

  const save = async () => {
    const name = cfg.name.trim() || `${cfg.user}@${cfg.host}`;
    try {
      // Save the shown MCP value as is (if left unset, it would turn on by itself after Production is turned off)
      const saved = await api.saveConnection({ ...cfg, name, options: { ...opt, mcp: mcpValue } }, password, sshSecret);
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

  useSharedReady();
  // With AWS SSM a local Host is the instance itself: said, not refused (a database on the instance is fine)
  const ssmLocalHost = tunnel === "ssm" ? sharedOr(undefined, (s) => s.ssmLocalHost(cfg.host)) : undefined;
  const tabHint =
    tab === "options"
      ? "Changes apply on the next connection (the data source is reconnected on OK)."
      : tab === "schemas"
      ? `${edit ? "" : "Listed with this dialog's settings; nothing is saved until OK. "}Only checked databases and schemas are shown in the Database Explorer. Nothing checked = show all.`
      : tab === "ssh" && tunnel === "ssm"
      ? "The database host/port on the General tab are reached from the instance (e.g. an RDS endpoint). Test Connection starts a session and connects through it."
      : tab === "ssh"
      ? "The database host/port on the General tab are resolved from the SSH server. Host keys are checked against ~/.ssh/known_hosts. The SSH host may be a ~/.ssh/config Host (HostName, User, Port, IdentityFile, ProxyJump); with no private key or password, ssh-agent is used."
      : "Passwords are stored encrypted in the app data folder (no Keychain prompts).";
  const port = (v: number, on: (n: number) => void, fallback: number, disabled?: boolean) => <TextInput className="w-port" disabled={disabled} value={v} onChange={(e) => on(Number(e.target.value) || fallback)} />;

  return (
    <Modal
      title={edit ? "Data Source Properties" : "New Data Source"}
      size="lg"
      fixed
      stickyBackdrop
      onClose={onClose}
      // Esc while testing cancels the test and keeps the dialog (and its edits); the next Esc closes it
      onKeyDown={(e) => {
        if (e.key === "Escape" && testing) {
          e.preventDefault();
          cancelTest();
        } else if (e.key === "Escape" && cancelListing.current) {
          e.preventDefault();
          cancelListing.current();
        }
      }}
      footer={
        <>
          {testing ? (
            <Button onClick={cancelTest}>Cancel Test</Button>
          ) : (
            <Button ref={testButton} onClick={() => runTest()}>
              {sshTest ? "Test SSH Connection" : "Test Connection"}
            </Button>
          )}
          {testing && <TestElapsed />}
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
          { id: "ssh", label: `SSH/SSL${tunnel !== "none" || ssl.enabled ? " ●" : ""}` },
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
              {ssmLocalHost && (
                <>
                  <span />
                  <Text tone="warn">{ssmLocalHost}</Text>
                </>
              )}
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
          <ColorSwatches custom value={cfg.color ?? null} onChange={(v) => set("color", v)} />
        </FormGrid>
      )}

      {tab === "ssh" && (
        <FormGrid>
          <FormSection>Tunnel</FormSection>
          <label>Tunnel</label>
          <Select className="w-auto" aria-label="Tunnel" value={tunnel} onChange={(e) => setTunnel(e.target.value as typeof tunnel)}>
            <option value="none">None</option>
            <option value="ssh">SSH</option>
            <option value="ssm">AWS SSM (Session Manager)</option>
          </Select>
          {tunnel === "ssm" ? (
            <>
              <label>AWS profile</label>
              <TextInput aria-label="AWS profile" value={ssm.profile} placeholder="(default profile)" onChange={(e) => setSsm("profile", e.target.value)} />
              <label>Region</label>
              <TextInput aria-label="Region" value={ssm.region} placeholder="(the profile's region)" onChange={(e) => setSsm("region", e.target.value)} />
              <label>Instance id</label>
              <TextInput aria-label="Instance id" value={ssm.target} placeholder="i-0123456789abcdef0" onChange={(e) => setSsm("target", e.target.value)} />
              <span />
              <Text tone="muted">Runs the AWS CLI v2 with the Session Manager plugin, signed in with your AWS config or SSO. Qurry stores no AWS secrets.</Text>
            </>
          ) : (
            <>
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
                    <TextInput grow aria-label="Private key" disabled={!ssh.enabled} value={ssh.keyPath} placeholder="empty: ssh-agent, then ~/.ssh/config IdentityFile" onChange={(e) => setSsh("keyPath", e.target.value)} />
                    <Button size="sm" disabled={!ssh.enabled} onClick={browseKey}>
                      Browse…
                    </Button>
                  </div>
                </>
              )}
              <label>{ssh.auth === "key" ? "Passphrase" : "Password"}</label>
              <TextInput type="password" disabled={!ssh.enabled} value={sshSecret ?? ""} placeholder={edit?.ssh?.enabled ? "<saved — leave empty to keep>" : ssh.auth === "key" ? "(none)" : "(empty: ssh-agent)"} onChange={(e) => setSshSecret(e.target.value)} />
            </>
          )}
          <FormSection>SSL / TLS</FormSection>
          <span />
          <Checkbox checked={ssl.enabled} onChange={(v) => setSsl("enabled", v)}>
            Use SSL{cfg.kind === "redis" ? " (rediss://)" : ""}
          </Checkbox>
          <label>Mode</label>
          <Select disabled={!ssl.enabled} value={ssl.mode || "require"} onChange={(e) => setSsl("mode", e.target.value)}>
            <option value="require">Require — encrypt, no certificate check</option>
            <option value="verify-ca">Verify CA</option>
            <option value="verify-full">Full Verification (CA + host name)</option>
          </Select>
          {(["caFile", "certFile", "keyFile"] as const).map((k) => (
            <span key={k} style={{ display: "contents" }}>
              <label>{k === "caFile" ? "CA file" : k === "certFile" ? "Client cert" : "Client key"}</label>
              <div className="ui-row">
                <TextInput grow disabled={!ssl.enabled} value={ssl[k]} placeholder={k !== "caFile" ? "(none)" : cfg.kind === "redis" ? "(system trust store)" : "(built-in public CA list)"} onChange={(e) => setSsl(k, e.target.value)} />
                <Button size="sm" disabled={!ssl.enabled} onClick={() => pickFile(k)}>
                  Browse…
                </Button>
              </div>
            </span>
          ))}
        </FormGrid>
      )}

      {tab === "schemas" && (
        <SchemasTab
          connId={edit?.id}
          form={{ config: cfg, password, sshSecret }}
          cancelRef={cancelListing}
          onAllowPlaintext={() => setOpt("allowPlaintext", true)}
          kind={cfg.kind}
          defaultDb={defaultDbOf(cfg)}
          scope={cfg.scope ?? {}}
          onChange={(scope) => setCfg((c) => ({ ...c, scope }))}
        />
      )}

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
          {/* #106: on by default for Production (unset = on); off, consoles start in the Transaction control mode below */}
          <span />
          <Checkbox
            checked={!!opt.confirmChanges && opt.productionManual !== false}
            disabled={!opt.confirmChanges}
            onChange={(v) => setOpt("productionManual", v ? null : false)}
            title="New consoles start in Tx: Manual, so a change stays pending until you Commit or Roll back (closing the tab, switching the database or quitting asks first). Off: they start in the Transaction control mode below."
          >
            Production: start consoles in Manual transaction mode
          </Checkbox>
          {(cfg.kind === "mysql" || cfg.kind === "postgres") && (
            <>
              <span />
              <Checkbox checked={!!opt.allowPlaintext} onChange={(v) => setOpt("allowPlaintext", v)} title="Without SSL settings Qurry requires TLS from remote servers. Tick this to connect to a server without TLS (or an old MySQL 5.7 without TLS 1.2) on a network you trust.">
                Allow unencrypted connection (servers without TLS)
              </Checkbox>
            </>
          )}
          <label>MCP (AI tools)</label>
          <div className="ui-row">
            <Select value={mcpValue} onChange={(e) => setOpt("mcp", e.target.value as ConnOptions["mcp"])} title="What qurry-mcp (Claude Code and other AI tools) may do with this data source. For Read, connect with a read-only database user.">
              <option value="off">Off — not listed, no connection</option>
              <option value="read">Read</option>
              <option value="write" disabled={noMcpWrite} title={noMcpWrite ? `Not for a ${cfg.readOnly ? "Read-only" : "Production"} data source` : undefined}>
                Read &amp; write (when qurry-mcp runs with --allow-write)
              </option>
            </Select>
          </div>
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
      {/* Server without TLS: tick Allow unencrypted in this form (saved only with OK) → test again */}
      {test && !test.ok && needsTlsOptIn(test.msg) && (
        <TlsOptIn
          host={cfg.host}
          production={!!opt.confirmChanges}
          inDialog
          onAllow={async () => {
            const next = { ...cfg, options: { ...opt, allowPlaintext: true } };
            setCfg(next);
            testButton.current?.focus();
            await runTest(next);
          }}
        />
      )}
      {/* Unknown SSH server: trust after checking the fingerprint → test again */}
      {test && !test.ok && test.msg.includes("Unknown SSH host key") && tunnel === "ssh" && cfg.ssh && (
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
      <div ref={testEnd} />
    </Modal>
  );
}

export function ColorSwatches({ value, onChange, custom }: { value: string | null; onChange: (v: string | null) => void; custom?: boolean }) {
  return <ColorPicker value={value} options={COLOR_PRESETS} onChange={onChange} custom={custom} />;
}

type Scope = NonNullable<ConnectionConfig["scope"]>;

/** The form's settings, for listing a data source that isn't saved yet */
interface FormSettings {
  config: ConnectionConfig;
  password?: string;
  sshSecret?: string;
}

/**
 * Schemas tab of DataGrip Data Source properties: choose which DBs and schemas show in the tree. A saved source lists through its
 * saved settings; a new one connects with the form's settings, as Test Connection does, and keeps nothing (DataGrip does the same)
 */
function SchemasTab({ connId, form, cancelRef, onAllowPlaintext, kind, defaultDb, scope, onChange }: { connId?: string; form: FormSettings; cancelRef: { current: (() => void) | null }; onAllowPlaintext: () => void; kind: DbKind; defaultDb: string; scope: Scope; onChange: (s: Scope) => void }) {
  const [dbs, setDbs] = useState<string[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [schemas, setSchemas] = useState<Record<string, SchemaInfo[] | "loading" | { error: string }>>({});
  const [openDb, setOpenDb] = useState<string | null>(null);
  // The dialog's settings now: Try Again and expanding a database use them as they are then (Allow unencrypted, a fix made after an error)
  const formNow = useRef(form);
  formNow.current = form;
  const [attempt, setAttempt] = useState(0);
  const retry = () => (setErr(null), setAttempt((n) => n + 1));
  const wrap = useRef<HTMLDivElement>(null);
  // Schemas being listed for an expanded database (a new source: each connects on its own), stopped when the tab or the dialog closes
  const expanding = useRef(new Set<string>());
  useEffect(() => () => expanding.current.forEach((id) => void api.cancelTest(id)), []);

  useEffect(() => {
    if (connId) {
      meta.databases(connId).then(setDbs, (e) => setErr(String(e)));
      return;
    }
    // Cancel, Esc, leaving the tab and closing the dialog stop the connect, and with it its SSH / AWS SSM tunnel
    const id = crypto.randomUUID();
    let running = true;
    const done = () => {
      running = false;
      cancelRef.current = null;
    };
    const stop = () => (done(), void api.cancelTest(id));
    cancelRef.current = () => (stop(), setErr("Cancelled."));
    const f = formNow.current;
    api.formDatabases(f.config, f.password, f.sshSecret, id).then(
      (v) => running && (done(), setDbs(v)),
      (e) => running && (done(), setErr(String(e))),
    );
    return () => void (running && stop());
  }, [connId, attempt, cancelRef]);

  // Try Again, Cancel and the list replace the button that had focus: keep focus in the dialog, so Esc still stops or closes
  useEffect(() => {
    if (document.activeElement === document.body) wrap.current?.querySelector<HTMLElement>("button, input")?.focus();
  }, [dbs, err]);

  return (
    <div ref={wrap} style={{ display: "contents" }}>
      {body()}
    </div>
  );

  function body() {
    if (err && !connId) {
      // Fixed where Test Connection fixes them: say so (the TLS opt-in is offered right here, as under a failed test)
      const fixedByTest = err.includes("Unknown SSH host key") ? "trust the SSH host key" : err.includes(DRIVER_MISSING) ? "install the driver" : null;
      const cfg = formNow.current.config;
      return (
        <>
          <Notice tone="danger">{"✕ " + err}</Notice>
          {fixedByTest && <Text as="div" tone="muted">Test Connection offers to {fixedByTest}; then Try Again.</Text>}
          {needsTlsOptIn(err) && <TlsOptIn host={cfg.host} production={!!cfg.options?.confirmChanges} inDialog onAllow={async () => (onAllowPlaintext(), retry())} />}
          <div className="ui-row conn-test-actions">
            <Button onClick={retry}>Try Again</Button>
          </div>
        </>
      );
    }
    if (err) return <Notice tone="danger">{err}</Notice>;
    if (!dbs && !connId)
      return (
        <EmptyState action={<Button onClick={() => cancelRef.current?.()}>Cancel Listing</Button>}>
          <TestElapsed label="Connecting with this dialog's settings…" />
        </EmptyState>
      );
    if (!dbs) return <EmptyState>Loading databases…</EmptyState>;

    // Same rules as Manage Shown Schemas (ScopePopup): no filter = All, stored as no filter
    const isAll = (v: string[]) => !v.length || (v.length === 1 && v[0] === SCOPE_ALL);
    const selDbs = scope.databases?.length ? scope.databases : [SCOPE_ALL];
    const allDbs = selDbs.includes(SCOPE_ALL);
    const toggleDb = (db: string) => {
      // System databases stay out of the list that replaces All, as in Manage Shown Schemas
      const next = toggleScoped(selDbs, db, dbs.filter((d) => d === defaultDb || !isSystemDb(kind, d)), defaultDb);
      onChange({ ...scope, databases: isAll(next) ? null : next });
    };
    const loadSchemas = (db: string) => {
      setOpenDb(openDb === db ? null : db);
      // Listed or listing: kept. Failed: expanding again tries again
      const cur = schemas[db];
      if (cur === "loading" || Array.isArray(cur)) return;
      setSchemas((s) => ({ ...s, [db]: "loading" }));
      const id = crypto.randomUUID();
      const f = formNow.current;
      if (!connId) expanding.current.add(id);
      (connId ? meta.schemas(targetId(connId, db)) : api.formSchemas(f.config, db, f.password, f.sshSecret, id))
        .then(
          (list) => setSchemas((s) => ({ ...s, [db]: list.filter((x) => !x.isSystem) })),
          (e) => setSchemas((s) => ({ ...s, [db]: { error: String(e) } })),
        )
        .finally(() => expanding.current.delete(id));
    };
    const schList = (db: string) => (scope.schemas?.[db]?.length ? scope.schemas[db] : [SCOPE_ALL]);
    const toggleSchema = (db: string, name: string) => {
      const sch = schemas[db];
      const next = toggleScoped(schList(db), name, Array.isArray(sch) ? sch.map((x) => x.name) : [], "public");
      const all = { ...(scope.schemas ?? {}) };
      if (isAll(next)) delete all[db];
      else all[db] = next;
      onChange({ ...scope, schemas: all });
    };

    return (
      <div className="scope-list">
        <label className="scope-row all">
          <input type="checkbox" checked={allDbs} onChange={() => toggleDb(SCOPE_ALL)} /> All databases
        </label>
        {dbs.map((db) => {
          const sch = schemas[db];
          const selSch = schList(db);
          return (
            <div key={db}>
              <div className="scope-row">
                <span className="twisty" onClick={() => loadSchemas(db)}>{openDb === db ? "▾" : "▸"}</span>
                <label>
                  <input type="checkbox" checked={scopeChecked(selDbs, db, defaultDb)} onChange={() => toggleDb(db)} /> {db}
                </label>
                <Text tone="muted">{selSch.includes(SCOPE_ALL) ? "all schemas" : `${selSch.length} schemas`}</Text>
              </div>
              {openDb === db && (
                <div className="scope-children">
                  {sch === "loading" && <Text as="div" tone="muted">Loading…</Text>}
                  {sch && typeof sch === "object" && "error" in sch && <Text as="div" tone="danger">{sch.error}</Text>}
                  {Array.isArray(sch) &&
                    sch.map((x) => (
                      <label key={x.name} className="scope-row">
                        <input type="checkbox" checked={scopeChecked(selSch, x.name, "public")} onChange={() => toggleSchema(db, x.name)} /> {x.name}
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
}
