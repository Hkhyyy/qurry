import { useEffect, useId, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, ColorPicker, Hint, Modal, Named, Notice, Spacer, Tabs, Text, TextInput, TlsOptIn } from "../ui";
import { needsTlsOptIn } from "./TlsFix";
import { api, COLOR_PRESETS, ConnOptions, RedisMode, SslConfig, ConnectionConfig, ConnectionView, defaultDbOf, DRIVER_MISSING, isFileKind, KIND_LABEL, SshConfig, SsmConfig } from "../lib/api";
import { elapsedText } from "./RunningBar";
import { sharedOr, useSharedReady } from "../lib/shared";
import { GeneralTab } from "../features/conns/GeneralTab";
import { OptionsTab } from "../features/conns/OptionsTab";
import { SchemasTab } from "../features/conns/SchemasTab";
import { SshSslTab } from "../features/conns/SshSslTab";

interface Props {
  edit?: ConnectionView;
  onClose: () => void;
  onSaved: (saved: ConnectionConfig) => void;
}

const EMPTY_SSH: SshConfig = { enabled: false, host: "", port: 22, user: "", auth: "key", keyPath: "~/.ssh/id_ed25519" };
const EMPTY_SSM: SsmConfig = { enabled: false, profile: "", region: "", target: "" };

/** "Testing… 12.3 s" while a Test Connection runs (an AWS SSM session can take up to 30 s) */
export function TestElapsed({ label = "Testing…" }: { label?: string }) {
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
  // The running test's id (cancelCall stops it, and with it its SSH / SSM tunnel); closing the dialog stops it too
  const [testing, setTesting] = useState<string | null>(null);
  const testingRef = useRef<string | null>(null);
  useEffect(() => () => void (testingRef.current && api.cancelCall(testingRef.current)), []);
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
  // #131: MCP's dba_report asks only on Production (core ConnectionConfig::mcp_dba_report_allowed): elsewhere the row is disabled
  // and its info says why; saving keeps the chosen value
  const reportsWhy = mcpValue === "off" ? "MCP is Off for this data source." : !opt.confirmChanges ? "Always on for non-Production sources." : null;
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
    if (testingRef.current) void api.cancelCall(testingRef.current);
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
  const cancelTest = () => void (testing && api.cancelCall(testing));

  const save = async () => {
    const name = cfg.name.trim() || `${cfg.user}@${cfg.host}`;
    try {
      // Save the shown MCP value as is (if left unset, it would turn on by itself after Production is turned off).
      // opt.mcpDbaReports is the chosen value even while its row is disabled: only the shown check is derived
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
  // The tab's one must-read line under it, the rest behind its ⓘ (General has none: its note is the Password's ⓘ)
  const tabHint: { text: string; about?: string; info?: string[] } | null =
    tab === "options"
      ? { text: "Applied on OK: the data source reconnects." }
      : tab === "schemas"
      ? { text: "Only checked items show in the Database Explorer; none checked shows all.", about: "the Schemas tab", info: edit ? undefined : ["Listed with this dialog's settings, as Test Connection connects; nothing is saved until OK."] }
      : tab === "ssh" && tunnel === "ssm"
      ? {
          text: "Host and port on General are reached from the instance (e.g. an RDS endpoint).",
          about: "AWS SSM",
          info: ["Runs the AWS CLI v2 with the Session Manager plugin, signed in with your AWS config or SSO. Qurry stores no AWS secrets.", "Test Connection starts a session and connects through it."],
        }
      : tab === "ssh"
      ? {
          text: "Host and port on General are reached from the SSH server.",
          about: "SSH tunnels",
          info: ["Host keys are checked against ~/.ssh/known_hosts.", "The SSH host may be a ~/.ssh/config Host (HostName, User, Port, IdentityFile, ProxyJump).", "With no private key or password, ssh-agent is used."],
        }
      : null;
  const ids = useId();
  const pwLabel = cfg.kind === "qdrant" || cfg.kind === "chroma" ? "API key" : "Password";
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
        <GeneralTab
          cfg={cfg} setCfg={setCfg} set={set} setOpt={setOpt} opt={opt} edit={edit} ids={ids}
          driver={driver} installing={installing} installDriver={installDriver} fileDb={fileDb} pickDbFile={pickDbFile} ssmLocalHost={ssmLocalHost}
          port={port} pwLabel={pwLabel} password={password} setPassword={setPassword} rmode={rmode} setRedis={setRedis}
        />
      )}

      {tab === "ssh" && (
        <SshSslTab
          cfg={cfg} edit={edit} tunnel={tunnel} setTunnel={setTunnel} ssh={ssh} setSsh={setSsh} ssm={ssm} setSsm={setSsm}
          sshSecret={sshSecret} setSshSecret={setSshSecret} browseKey={browseKey} ssl={ssl} setSsl={setSsl} pickFile={pickFile} port={port}
        />
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
        <OptionsTab cfg={cfg} set={set} opt={opt} setOpt={setOpt} ids={ids} mcpValue={mcpValue} noMcpWrite={noMcpWrite} reportsWhy={reportsWhy} />
      )}

      {tabHint && (
        <Hint>
          <Named about={tabHint.about} info={tabHint.info}>
            {tabHint.text}
          </Named>
        </Hint>
      )}
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

/**
 * What a new source's form still lacks before its databases can be listed (tab · field), in order. A half-filled form isn't
 * connected: the server would only answer with an error for the empty field
 */
export function missingForListing(c: ConnectionConfig): [string, string][] {
  const out: [string, string][] = [];
  if (isFileKind(c.kind)) {
    if (!c.database.trim()) out.push(["General", "File"]);
    return out;
  }
  if (!c.host.trim()) out.push(["General", "Host"]);
  if (["postgres", "mysql", "mssql", "oracle"].includes(c.kind) && !c.user.trim()) out.push(["General", "User"]);
  if (c.ssh?.enabled) {
    if (!c.ssh.host.trim()) out.push(["SSH/SSL", "SSH host"]);
    if (!c.ssh.user.trim()) out.push(["SSH/SSL", "SSH user"]);
  }
  if (c.ssm?.enabled && !c.ssm.target.trim()) out.push(["SSH/SSL", "Instance"]);
  return out;
}
