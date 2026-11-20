import { open } from "@tauri-apps/plugin-dialog";
import { ColorSwatches } from "../../components/ConnectionDialog";
import { ConnOptions, ConnectionConfig, ConnectionView, DbKind, DEFAULT_PORT, KIND_LABEL, RedisMode } from "../../lib/api";
import { Button, FormGrid, Named, Select, Text, TextInput } from "../../ui";

const PASSWORD_COMMAND_EXAMPLE = "/opt/homebrew/bin/aws rds generate-db-auth-token --hostname %HOSTNAME% --port %PORT% --username %USERNAME% --region ap-northeast-2";

/** The Data Source dialog's General tab: name, driver (and its plugin), where the database is, the login and the color */
export function GeneralTab({
  cfg, setCfg, set, setOpt, opt, edit, ids, driver, installing, installDriver, fileDb, pickDbFile, ssmLocalHost, port, pwLabel, password, setPassword, rmode, setRedis,
}: {
  cfg: ConnectionConfig;
  setCfg: (f: (c: ConnectionConfig) => ConnectionConfig) => void;
  set: <K extends keyof ConnectionConfig>(k: K, v: ConnectionConfig[K]) => void;
  setOpt: <K extends keyof ConnOptions>(k: K, v: ConnOptions[K]) => void;
  opt: ConnOptions;
  edit?: ConnectionView;
  ids: string;
  driver: boolean | null;
  installing: boolean;
  installDriver: () => void;
  fileDb: boolean;
  pickDbFile: () => void;
  ssmLocalHost: string | undefined;
  port: (v: number, on: (n: number) => void, fallback: number, disabled?: boolean) => React.ReactNode;
  pwLabel: string;
  password: string | undefined;
  setPassword: (v: string) => void;
  rmode: RedisMode;
  setRedis: <K extends keyof RedisMode>(k: K, v: RedisMode[K]) => void;
}) {
  // The password comes from the command: the Password field is off (the saved one stays as it is)
  const fromCommand = !!cfg.passwordCommand?.trim();
  return (
    // Room for "Password command" and its ⓘ
    <FormGrid labelWidth={156}>
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
                <Text tone="muted">
                  <Named about="the driver plugin" info={["Install downloads it from this version's GitHub release and checks its signature."]}>
                    Not installed
                  </Named>
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
          <TextInput aria-label="User" value={cfg.user} placeholder={cfg.kind === "redis" ? "(optional, ACL user)" : cfg.kind === "milvus" ? "(optional — empty: the password is an API key)" : ""} onChange={(e) => set("user", e.target.value)} />
          {/* htmlFor and aria-label: a click on the name focuses the field, and the field's name leaves out the ⓘ */}
          <label htmlFor={`${ids}-pw`}>
            <Named info={["Saved passwords are encrypted in secrets.json; its key is in the config folder or in the system keychain (Settings › Passwords)."]}>{pwLabel}</Named>
          </label>
          <TextInput
            id={`${ids}-pw`}
            aria-label={pwLabel}
            type="password"
            disabled={fromCommand}
            value={fromCommand ? "" : (password ?? "")}
            placeholder={fromCommand ? "(from the command)" : edit?.hasPassword ? "<saved — leave empty to keep>" : ""}
            onChange={(e) => setPassword(e.target.value)}
          />
          <label htmlFor={`${ids}-pwcmd`}>
            <Named
              info={[
                `Runs at every login and uses its output as the password. Full program path (apps started from Finder don't get the shell's PATH). %HOSTNAME% %PORT% %USERNAME% %DATABASE% are replaced; arguments are passed as they are, never through a shell. Example: ${PASSWORD_COMMAND_EXAMPLE}`,
              ]}
            >
              Password command
            </Named>
          </label>
          <div className="ui-row">
            <TextInput grow id={`${ids}-pwcmd`} aria-label="Password command" value={cfg.passwordCommand ?? ""} placeholder={PASSWORD_COMMAND_EXAMPLE} onChange={(e) => set("passwordCommand", e.target.value)} />
            <Text tone="muted">Reuse for</Text>
            <TextInput
              className="w-port"
              aria-label="Reuse for seconds"
              inputMode="numeric"
              value={cfg.passwordReuseSecs ?? ""}
              placeholder="—"
              onChange={(e) => {
                const v = e.target.value.replace(/\D/g, "");
                set("passwordReuseSecs", v ? Number(v) : null);
              }}
            />
            <Text tone="muted">sec.</Text>
          </div>
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
  );
}
