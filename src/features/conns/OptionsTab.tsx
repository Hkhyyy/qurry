import { ConnOptions, ConnectionConfig } from "../../lib/api";
import { Checkbox, FormGrid, FormSection, Named, Select, Text, TextArea, TextInput } from "../../ui";

/** The Data Source dialog's Options tab: read-only and Production, encryption, MCP, transactions and the session settings */
export function OptionsTab({ cfg, set, opt, setOpt, ids, mcpValue, noMcpWrite, reportsWhy }: {
  cfg: ConnectionConfig;
  set: <K extends keyof ConnectionConfig>(k: K, v: ConnectionConfig[K]) => void;
  opt: ConnOptions;
  setOpt: <K extends keyof ConnOptions>(k: K, v: ConnOptions[K]) => void;
  ids: string;
  mcpValue: "off" | "read" | "write";
  noMcpWrite: boolean;
  reportsWhy: string | null;
}) {
  return (
    <FormGrid labelWidth={140}>
      <FormSection>Connection</FormSection>
      <span />
      <Checkbox checked={!!cfg.readOnly} onChange={(v) => set("readOnly", v)}>
        Read-only
      </Checkbox>
      <span />
      <Named about="Production: confirm changes" info={["Before UPDATE, DELETE or INSERT runs, shows how many rows will change and asks."]}>
        <Checkbox checked={!!opt.confirmChanges} onChange={(v) => setOpt("confirmChanges", v)}>
          Production: show affected rows and confirm changes
        </Checkbox>
      </Named>
      {/* #106: on by default for Production (unset = on); off, consoles start in the Transaction control mode below */}
      <span />
      <Named
        about="Production: Manual transaction mode"
        info={["New consoles start in Tx: Manual: a change stays pending until you Commit or Roll back. Closing the tab, switching the database or quitting asks first.", "Off: they start in the Transaction control mode below."]}
      >
        <Checkbox checked={!!opt.confirmChanges && opt.productionManual !== false} disabled={!opt.confirmChanges} onChange={(v) => setOpt("productionManual", v ? null : false)}>
          Production: start consoles in Manual transaction mode
        </Checkbox>
      </Named>
      {(cfg.kind === "mysql" || cfg.kind === "postgres") && (
        <>
          <span />
          <div>
            <Named
              about="Allow unencrypted connection"
              info={["Without SSL settings Qurry requires TLS from remote servers.", "Tick this only for a server without TLS (or an old MySQL 5.7 without TLS 1.2) on a network you trust."]}
            >
              <Checkbox checked={!!opt.allowPlaintext} onChange={(v) => setOpt("allowPlaintext", v)}>
                Allow unencrypted connection (servers without TLS)
              </Checkbox>
            </Named>
            {/* What the choice does, while it is on */}
            {opt.allowPlaintext && (
              <Text as="div" size="sm" tone="warn" className="ui-check-note">
                Passwords and data can travel unencrypted to this server.
              </Text>
            )}
          </div>
        </>
      )}
      <label htmlFor={`${ids}-mcp`}>
        <Named info={["What qurry-mcp (Claude Code and other AI tools) may do with this data source.", "For Read, connect with a read-only database user."]}>MCP (AI tools)</Named>
      </label>
      <div className="ui-row">
        <Select id={`${ids}-mcp`} aria-label="MCP (AI tools)" value={mcpValue} onChange={(e) => setOpt("mcp", e.target.value as ConnOptions["mcp"])}>
          <option value="off">Off — not listed, no connection</option>
          <option value="read">Read</option>
          <option value="write" disabled={noMcpWrite} title={noMcpWrite ? `Not for a ${cfg.readOnly ? "Read-only" : "Production"} data source` : undefined}>
            Read &amp; write (when qurry-mcp runs with --allow-write)
          </option>
        </Select>
      </div>
      <span />
      <div>
        <Named
          about="Admin reports for AI tools"
          info={["Slow queries, sessions, locks. Query values are masked.", ...(reportsWhy ? [reportsWhy] : [])]}
        >
          {/* Not Production: shown checked, as AI tools read its reports anyway */}
          <Checkbox checked={reportsWhy ? mcpValue !== "off" : !!opt.mcpDbaReports} disabled={!!reportsWhy} onChange={(v) => setOpt("mcpDbaReports", v)}>
            AI tools can read admin reports
          </Checkbox>
        </Named>
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
      <TextInput value={opt.timeZone ?? ""} placeholder="UTC (e.g. Asia/Seoul, +09:00)" onChange={(e) => setOpt("timeZone", e.target.value || null)} />
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
  );
}
