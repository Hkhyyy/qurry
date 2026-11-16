import { ConnectionConfig, ConnectionView, SshConfig, SslConfig, SsmConfig } from "../../lib/api";
import { Button, Checkbox, FormGrid, FormSection, Select, Text, TextInput } from "../../ui";

/** The Data Source dialog's SSH/SSL tab: the tunnel (SSH or AWS SSM) and SSL / TLS */
export function SshSslTab({
  cfg, edit, tunnel, setTunnel, ssh, setSsh, ssm, setSsm, sshSecret, setSshSecret, browseKey, ssl, setSsl, pickFile, port,
}: {
  cfg: ConnectionConfig;
  edit?: ConnectionView;
  tunnel: "none" | "ssh" | "ssm";
  setTunnel: (t: "none" | "ssh" | "ssm") => void;
  ssh: SshConfig;
  setSsh: <K extends keyof SshConfig>(k: K, v: SshConfig[K]) => void;
  ssm: SsmConfig;
  setSsm: <K extends keyof SsmConfig>(k: K, v: SsmConfig[K]) => void;
  sshSecret: string | undefined;
  setSshSecret: (v: string) => void;
  browseKey: () => void;
  ssl: SslConfig;
  setSsl: <K extends keyof SslConfig>(k: K, v: SslConfig[K]) => void;
  pickFile: (k: "caFile" | "certFile" | "keyFile") => void;
  port: (v: number, on: (n: number) => void, fallback: number, disabled?: boolean) => React.ReactNode;
}) {
  return (
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
  );
}
