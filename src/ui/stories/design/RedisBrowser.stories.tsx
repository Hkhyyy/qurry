import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState, type ReactNode } from "react";
import { Badge, Tabs, Text, TreeRow } from "../../index";
import { TYPE_COLOR } from "../../../components/RedisBrowser";

// Agreed design for 0.3.2, #108: Redis browser cleanup. Chosen: 1 Proposed, 2 A2, 3 A (greyed out), 4 Proposed.
// Not product code: replace it with the real components when the issue is built, then delete this story.
const meta: Meta = { title: "Design/0.3.2/Redis browser (#108)", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const T = ({ t }: { t: string }) => (
  <Badge color={TYPE_COLOR[t] ?? "#777"} className="rd-badge">
    {t.toUpperCase()}
  </Badge>
);
const Count = ({ n }: { n: number }) => <span style={{ color: "var(--muted)", fontSize: 11, marginLeft: 6 }}>{n}</span>;
const Ttl = ({ v }: { v?: string }) => <span style={{ color: v ? "var(--text)" : "var(--muted)", fontSize: 11 }}>{v ?? "—"}</span>;
const Folder = () => <span style={{ color: "var(--muted)" }}>▭</span>;

function Frame({ title, note, children, h = 330 }: { title: string; note: string; children: ReactNode; h?: number }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0 }}>
      <Text size="sm">
        <b>{title}</b> — {note}
      </Text>
      <div style={{ display: "flex", height: h, border: "1px solid var(--border)", borderRadius: 6, overflow: "hidden", background: "var(--bg)" }}>{children}</div>
    </div>
  );
}

function Head({ ttl = "TTL" }: { ttl?: string }) {
  return (
    <div style={{ display: "flex", padding: "4px 10px", fontSize: 11, color: "var(--muted)", borderBottom: "1px solid var(--border)", background: "var(--panel)" }}>
      <span style={{ flex: 1 }}>Key</span>
      <span>{ttl}</span>
    </div>
  );
}

/** The key tree, today (counts in the TTL column) or proposed (counts beside the name, TTL only for keys) */
function KeyTree({ proposed }: { proposed: boolean }) {
  const f = (depth: number, name: string, n: number, open?: boolean) => (
    <TreeRow depth={depth} open={open} icon={<Folder />} label={name} badge={proposed ? <Count n={n} /> : undefined} trailing={proposed ? undefined : <Ttl v={String(n)} />} />
  );
  const k = (depth: number, type: string, name: string, ttl?: string, selected?: boolean) => (
    <TreeRow depth={depth} leaf selected={selected} label={<span style={{ display: "inline-flex", gap: 6, alignItems: "center" }}><T t={type} />{name}</span>} trailing={proposed ? <Ttl v={ttl} /> : ttl ? <Ttl v={ttl} /> : undefined} />
  );
  return (
    <div style={{ flex: 1, minWidth: 0, overflow: "auto" }}>
      <Head ttl={proposed ? "TTL" : "TTL"} />
      {f(0, "dev", 7, true)}
      {f(1, "admin_sessions", 4)}
      {f(1, "sessions", 3, true)}
      {k(2, "zset", "dev:sessions:{405}")}
      {k(2, "zset", "dev:sessions:{7633}")}
      {f(0, "prod", 493, true)}
      {f(1, "refresh", 62)}
      {f(1, "sessions", 419)}
      {f(1, "user", 2, true)}
      {f(2, "mkt-daily-signup", 2, true)}
      {k(3, "string", "prod:user:mkt-daily-signup:2026-05-05", "1d 4h", true)}
      {k(3, "string", "prod:user:mkt-daily-signup:2026-05-16", "1d 4h")}
    </div>
  );
}

function Value() {
  return (
    <div style={{ flex: 1.2, minWidth: 0, borderLeft: "1px solid var(--border)", display: "flex", flexDirection: "column" }}>
      <div style={{ padding: "6px 10px", borderBottom: "1px solid var(--border)", display: "flex", gap: 8, alignItems: "center", fontSize: 12 }}>
        <T t="string" />
        <b>prod:user:mkt-daily-signup:2026-05-05</b>
        <span style={{ marginLeft: "auto", color: "var(--muted)" }}>TTL 1d 4h · 42 B</span>
      </div>
      <pre style={{ margin: 0, padding: 10, fontSize: 12, color: "var(--text)" }}>{'{\n  "date": "2026-05-05",\n  "signups": 128\n}'}</pre>
    </div>
  );
}

export const KeyTreeTtl: S = {
  name: "1. Key tree — TTL column",
  render: () => (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18, padding: 18 }}>
      <Frame title="Today" note="folder key counts and real TTLs share the TTL column">
        <KeyTree proposed={false} />
      </Frame>
      <Frame title="Proposed" note="count beside the folder name (like tables 68); the TTL column only has key TTLs, — when none">
        <KeyTree proposed />
      </Frame>
    </div>
  ),
};

function Sidebar({ keys }: { keys: boolean }) {
  return (
    <div style={{ width: 210, flex: "none", borderRight: "1px solid var(--border)", background: "var(--panel)", overflow: "hidden" }}>
      <div style={{ padding: "6px 10px", fontSize: 11, color: "var(--muted)" }}>Database Explorer</div>
      <TreeRow depth={0} open icon={<span style={{ color: "#d9534f" }}>◆</span>} label="[DEV] RentalpayRedis" />
      {keys ? (
        <>
          <TreeRow depth={1} open icon={<span style={{ color: "#5b9bd5" }}>▤</span>} label="db0" badge={<Count n={500} />} />
          <TreeRow depth={2} icon={<Folder />} label="dev" badge={<Count n={7} />} />
          <TreeRow depth={2} open icon={<Folder />} label="prod" badge={<Count n={493} />} />
          <TreeRow depth={3} icon={<Folder />} label="sessions" badge={<Count n={419} />} />
          <TreeRow depth={3} leaf selected label={<span style={{ display: "inline-flex", gap: 6, alignItems: "center" }}><T t="string" />prod:user:mkt-…05</span>} />
        </>
      ) : (
        <>
          <TreeRow depth={1} leaf selected icon={<span style={{ color: "#5b9bd5" }}>▤</span>} label="db0" badge={<Count n={500} />} />
          <TreeRow depth={1} leaf icon={<span style={{ color: "#5b9bd5" }}>▤</span>} label="db1" badge={<Count n={12} />} />
          <TreeRow depth={1} leaf icon={<span style={{ color: "#5b9bd5" }}>▤</span>} label="db2" badge={<Count n={0} />} />
        </>
      )}
    </div>
  );
}


/** One Redis tab per data source: the database is picked in the tab's toolbar */
function TabBar() {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "4px 8px", borderBottom: "1px solid var(--border)", background: "var(--panel)", fontSize: 12 }}>
      <select defaultValue="db0" style={{ fontSize: 12 }} title="Database (db0 … db15)">
        <option value="db0">db0 · 500 keys</option>
        <option value="db1">db1 · 12 keys</option>
        <option value="db2">db2 · 0 keys</option>
      </select>
      <input placeholder="Filter keys (pattern, e.g. prod:user:*)" style={{ flex: 1, fontSize: 12, padding: "2px 6px" }} />
      <span style={{ color: "var(--muted)" }}>Content · Terminal · Info · Config</span>
    </div>
  );
}

function OneTab() {
  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
      <TabBar />
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <KeyTree proposed />
        <Value />
      </div>
    </div>
  );
}

export const Layout: S = {
  name: "2. Explorer vs key tab",
  render: () => (
    <div style={{ display: "grid", gridTemplateColumns: "1fr", gap: 18, padding: 18 }}>
      <Frame title="Today" note="the same key tree in the explorer and again in the tab" h={260}>
        <Sidebar keys />
        <KeyTree proposed />
        <Value />
      </Frame>
      <Frame title="A (recommended). Explorer: data source and databases only; keys in the tab" note="like SQL: pick a table in the explorer, browse its data in a tab" h={260}>
        <Sidebar keys={false} />
        <KeyTree proposed />
        <Value />
      </Frame>
      <Frame title="A2 (chosen). Explorer: databases only; ONE Redis tab per data source with a database picker" note="clicking db1 in the explorer switches the open tab to db1 instead of opening another tab; key tree + value stay side by side" h={290}>
        <Sidebar keys={false} />
        <OneTab />
      </Frame>
      <Frame title="B. Explorer keeps the key tree; the tab shows only the value" note="no second tree; the tab is a value viewer/editor" h={260}>
        <Sidebar keys />
        <Value />
      </Frame>
    </div>
  ),
};

function RedisTabs({ config }: { config: "error" | "hidden" | "disabled" }) {
  const [tab, setTab] = useState<string>(config === "error" ? "config" : "info");
  const items = [
    { id: "content", label: "Content" },
    { id: "terminal", label: "Terminal" },
    { id: "info", label: "Info" },
    ...(config === "hidden" ? [] : [{ id: "config", label: config === "disabled" ? <span title="This server doesn't allow CONFIG (AWS ElastiCache / MemoryDB …). Settings are in the provider's console; server details are in Info." style={{ opacity: 0.45 }}>Config</span> : "Config" }]),
  ] as { id: string; label: ReactNode }[];
  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
      <div style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>
        <Tabs items={items} value={tab} onChange={(t) => (config === "disabled" && t === "config" ? null : setTab(t))} size="sm" />
      </div>
      <div style={{ padding: 10, fontSize: 12 }}>
        {tab === "config" ? (
          <span>This server doesn't allow CONFIG — managed Redis services (AWS ElastiCache / MemoryDB, Azure Cache, GCP Memorystore, Upstash …) disable or rename it. Its settings are in the provider's console (e.g. the ElastiCache parameter group). Server details are still in Info.</span>
        ) : (
          <pre style={{ margin: 0 }}>{"redis_version: 7.1.0\nrole: master\nconnected_clients: 42\nused_memory_human: 1.21G\nmaxmemory_policy: volatile-lru (from the parameter group)"}</pre>
        )}
      </div>
    </div>
  );
}

export const ConfigTab: S = {
  name: "3. Config tab on managed Redis",
  render: () => (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 18, padding: 18 }}>
      <Frame title="Today" note="a tab that only says it can't work" h={200}>
        <RedisTabs config="error" />
      </Frame>
      <Frame title="A. Greyed out" note="checked once on connect; hover says why and where to look" h={200}>
        <RedisTabs config="disabled" />
      </Frame>
      <Frame title="B. Hidden" note="no Config tab where CONFIG is refused" h={200}>
        <RedisTabs config="hidden" />
      </Frame>
    </div>
  ),
};

const mono: React.CSSProperties = { fontFamily: "JetBrains Mono, Menlo, monospace", fontSize: 12 };

function TerminalToday() {
  return (
    <div style={{ flex: 1, padding: 10, ...mono }}>
      <div style={{ color: "var(--muted)" }}>db0&gt; HGETALL prod:user:1</div>
      <div>1) "name"</div>
      <div>2) "kim"</div>
      <div>3) "tier"</div>
      <div>4) "gold"</div>
      <div style={{ marginTop: 6 }}>
        db0&gt; HG<span style={{ background: "var(--text)", width: 7, display: "inline-block" }}>&nbsp;</span>
      </div>
    </div>
  );
}

function TerminalProposed() {
  return (
    <div style={{ flex: 1, padding: 10, position: "relative", ...mono }}>
      <div style={{ color: "var(--muted)" }}>db0&gt; HGETALL prod:user:1</div>
      <table style={{ borderCollapse: "collapse", margin: "4px 0 8px" }}>
        <thead>
          <tr>
            <th style={{ textAlign: "left", padding: "1px 10px 1px 0", color: "var(--muted)" }}>field</th>
            <th style={{ textAlign: "left", color: "var(--muted)" }}>value</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td style={{ paddingRight: 10 }}>name</td>
            <td>kim</td>
          </tr>
          <tr>
            <td style={{ paddingRight: 10 }}>tier</td>
            <td>gold</td>
          </tr>
        </tbody>
      </table>
      <div style={{ color: "var(--muted)" }}>db0&gt; HELP HGET</div>
      <div style={{ color: "var(--muted)", marginBottom: 8 }}>HGET key field — Returns the value of a field in a hash. (since 2.0.0, @read @hash @fast)</div>
      <div>
        db0&gt; HG<span style={{ background: "var(--text)", width: 7, display: "inline-block" }}>&nbsp;</span>
        <span style={{ color: "var(--muted)" }}> key field</span>
      </div>
      <div style={{ position: "absolute", left: 60, top: 150, background: "var(--panel)", border: "1px solid var(--border)", borderRadius: 6, boxShadow: "0 6px 18px rgba(0,0,0,.35)", minWidth: 300 }}>
        {[
          ["HGET", "key field", "Get a hash field"],
          ["HGETALL", "key", "All fields and values"],
          ["HGETDEL", "key FIELDS n field…", "Get and delete fields"],
        ].map(([c, a, d], i) => (
          <div key={c} style={{ display: "flex", gap: 8, padding: "3px 8px", background: i === 0 ? "var(--sel)" : undefined }}>
            <b>{c}</b>
            <span style={{ color: "var(--muted)" }}>{a}</span>
            <span style={{ marginLeft: "auto", color: "var(--muted)", fontSize: 11 }}>{d}</span>
          </div>
        ))}
      </div>
      <div style={{ position: "absolute", right: 10, bottom: 8, color: "var(--muted)", fontSize: 11 }}>↑↓ history · ⇥ complete · HELP cmd · keys complete from SCAN</div>
    </div>
  );
}

export const Terminal: S = {
  name: "4. Terminal tab",
  render: () => (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18, padding: 18 }}>
      <Frame title="Today" note="plain prompt and raw replies" h={280}>
        <TerminalToday />
      </Frame>
      <Frame title="Proposed" note="command and key completion with argument hints, ↑↓ history, HELP, replies shown by type (hash as a table)" h={280}>
        <TerminalProposed />
      </Frame>
    </div>
  ),
};
