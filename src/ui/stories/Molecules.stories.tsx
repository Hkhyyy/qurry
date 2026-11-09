import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Badge, Checkbox, ColorPicker, DetailRow, EmptyState, Field, FormGrid, FormSection, Hint, InfoTip, Kbd, KeyHints, ListItem, MenuItem, MenuLabel, MenuSeparator, Named, Notice, RadioGroup, RailItem, SearchField, SectionTitle, Select, StatusItem, TabItem, Tabs, Text, TextInput, TlsOptIn, TreeRow, TxPending } from "..";
import { connTabVars } from "../../lib/theme";
import { ConsoleIcon, DatabaseIcon, FolderIcon, PgIcon, ReportIcon, SchemaIcon, TabIcon, TableIcon } from "../icons";

/** Molecules: a row · cell made of a few atoms */
const meta: Meta = { title: "GUI/2 Molecules" };
export default meta;
type S = StoryObj;

const Frame = ({ children, width = 520 }: { children: React.ReactNode; width?: number }) => <div style={{ width, border: "1px solid var(--ui-border)", borderRadius: 6, padding: 4, background: "var(--ui-surface)" }}>{children}</div>;

export const Fields: S = {
  name: "Field · RadioGroup · ColorPicker",
  render: function Render() {
    const [scope, setScope] = useState("one");
    const [color, setColor] = useState<string | null>("#6e2a2a");
    return (
      <div style={{ width: 480 }}>
        <Field label="Name">
          <TextInput full defaultValue="[PROD] kr" />
        </Field>
        <Field label="Driver" hint="Drivers are installed when you pick them.">
          <Select full defaultValue="postgres">
            <option value="postgres">PostgreSQL</option>
          </Select>
        </Field>
        <Field label="Port" error="Must be a number">
          <TextInput className="w-port" defaultValue="54x2" />
        </Field>
        <Field label="Snippet">
          <RadioGroup value={scope} onChange={setScope} options={[{ value: "one", label: 'Only for "[PROD] kr"' }, { value: "all", label: "For all data sources", hint: "(everywhere)" }]} />
        </Field>
        <Field label="Color">
          <ColorPicker
            value={color}
            onChange={setColor}
            options={[
              { name: "none", value: null },
              { name: "red", value: "#6e2a2a" },
              { name: "green", value: "#2f5634" },
              { name: "blue", value: "#28406e" },
            ]}
          />
        </Field>
        <FormSection>SSL / TLS</FormSection>
        <Hint>Host keys are checked against ~/.ssh/known_hosts.</Hint>
      </div>
    );
  },
};

/** SearchField is a TextInput: no autocorrect or spell check (see GUI/1 Atoms › Inputs) */
/** One rule: the ⓘ right after the name it explains (a field's label, a checkbox beside its text, a hint line), the same gap and popover.
 *  Click, Space or Enter opens it; Esc, a click outside or a scroll closes it */
export const About: S = {
  name: "Named · InfoTip (ⓘ)",
  render: function Render() {
    const [prod, setProd] = useState(true);
    const [plain, setPlain] = useState(true);
    return (
      <div style={{ width: 520 }}>
        <Field label="Keyword case" info={["Applies as you type in a console and in Reformat Code.", "Never inside strings, comments or quoted names.", "Shared with the TUI."]}>
          <Select defaultValue="off">
            <option value="off">Off</option>
            <option value="upper">UPPER</option>
          </Select>
        </Field>
        <Field label="Theme" group hint="Following the system: Dark now." info={["The TUI uses the same themes; the terminal's background picks dark or light."]}>
          <Select aria-label="Theme" defaultValue="dark">
            <option value="dark">Dark</option>
          </Select>
        </Field>
        <FormGrid labelWidth={140}>
          <span />
          <Named about="Production: confirm changes" info={["Before UPDATE, DELETE or INSERT runs, shows how many rows will change and asks."]}>
            <Checkbox checked={prod} onChange={setProd}>
              Production: show affected rows and confirm changes
            </Checkbox>
          </Named>
          <span />
          <div>
            <Named about="Allow unencrypted connection" info={["Without SSL settings Qurry requires TLS from remote servers."]}>
              <Checkbox checked={plain} onChange={setPlain}>
                Allow unencrypted connection
              </Checkbox>
            </Named>
            {plain && (
              <Text as="div" size="sm" tone="warn" className="ui-check-note">
                Passwords and data can travel unencrypted to this server.
              </Text>
            )}
          </div>
          <label>
            <Named info={["What qurry-mcp (Claude Code and other AI tools) may do with this data source."]}>MCP (AI tools)</Named>
          </label>
          <Select aria-label="MCP (AI tools)" defaultValue="read">
            <option value="read">Read</option>
          </Select>
        </FormGrid>
        <Hint>
          <Named about="SSH tunnels" info={["Host keys are checked against ~/.ssh/known_hosts.", "With no private key or password, ssh-agent is used."]}>
            Host and port on General are reached from the SSH server.
          </Named>
        </Hint>
      </div>
    );
  },
};

/** The ⓘ alone, opened (its popover stays inside the window) */
export const InfoTipOpen: S = {
  name: "InfoTip — open",
  render: () => (
    <div style={{ display: "flex", justifyContent: "space-between", width: "100%" }}>
      <Named info={["Stable: releases made from a version tag.", "Beta: also the pre-release built from every merge to main.", "Shared with the TUI."]}>Channel</Named>
      <InfoTip about="the right edge" info={["Near the window's edge the popover moves left to stay inside."]} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    canvasElement.querySelector<HTMLButtonElement>('button[aria-label="About Channel"]')?.click();
  },
};

export const SearchAndTabs: S = {
  name: "SearchField · Tabs · SectionTitle",
  render: function Render() {
    const [t, setT] = useState<"a" | "b" | "c">("a");
    return (
      <div style={{ width: 520, display: "flex", flexDirection: "column", gap: 16 }}>
        <SearchField placeholder="Filter tables, views, routines" />
        <SearchField placeholder="Loading…" busy defaultValue="user" />
        <Frame>
          <SearchField bare placeholder="Type / to see commands" />
        </Frame>
        <Tabs
          items={[
            { id: "a", label: "General" },
            { id: "b", label: "SSH/SSL ●" },
            { id: "c", label: "Options" },
          ]}
          value={t}
          onChange={setT}
        />
        <Tabs
          size="sm"
          items={[
            { id: "a", label: "All" },
            { id: "b", label: "Database" },
            { id: "c", label: "Actions" },
          ]}
          value={t}
          onChange={setT}
        />
        {/* A disabled tab is greyed out, can't be picked (click, ←/→ skip it) and says why on hover: Redis Config on managed Redis */}
        <Tabs
          size="sm"
          items={[
            { id: "a", label: "Content" },
            { id: "b", label: "Info" },
            { id: "c", label: "Config", disabled: true, title: "This server doesn't allow CONFIG — its settings are in the provider's console" },
          ]}
          value={t === "c" ? "a" : t}
          onChange={setT}
        />
        <Frame>
          <SectionTitle>Actions</SectionTitle>
        </Frame>
      </div>
    );
  },
};

export const Feedback: S = {
  name: "Notice · EmptyState · KeyHints",
  render: () => (
    <div style={{ width: 560, display: "flex", flexDirection: "column", gap: 8 }}>
      <Notice>Runs pg_dump on this computer.</Notice>
      <Notice tone="ok">✓ PostgreSQL 16.4 on aarch64</Notice>
      <Notice tone="warn">2 consoles have uncommitted changes.</Notice>
      <Notice tone="danger">ERROR: relation "no_such_table" does not exist</Notice>
      <Frame>
        <EmptyState title="No snippets yet">Save a query you use often to find it here.</EmptyState>
      </Frame>
      <KeyHints
        items={[
          ["↩", "Open"],
          ["⌘↓", "Edit Data"],
          ["⌘b", "Go to DDL"],
          ["⇥", "Next tab"],
        ]}
      />
    </div>
  ),
};

/** A console's open transaction (#106), beside Commit / Roll back: amber, red on a Production data source, "idle" once the reminder
 *  about a transaction left open came (its text is on hover) */
export const TxPendingBadges: S = {
  name: "TxPending",
  render: () => (
    <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
      <TxPending />
      <TxPending production />
      <TxPending production idle='A transaction on Production "[PROD] shop_app" has been open for 5 min without a statement: its changes aren&apos;t saved and the rows it changed stay locked. Commit or Roll back.' />
    </div>
  ),
};

/** List row: search results · history · switcher · migration · RAG */
export const ListItems: S = {
  name: "ListItem",
  render: () => (
    <Frame>
      <ListItem icon={<TableIcon />} title="users" hint="public" trailing="[PROD] kr · kr" color="#6e2a2a" selected />
      <ListItem icon="⚡" title="New Query Console" hint="Tool Windows" trailing={<Kbd>⇧⌘L</Kbd>} />
      <ListItem icon={<ConsoleIcon />} title="console_2 [DEV] shop" hint="closed tab" dim color="#2f5634" />
      <ListItem mono title="select * from users where id = 42" trailing="2026. 10. 28. 오후 3:43" />
      <ListItem leading={<Checkbox checked onChange={() => {}} />} title={<><Text tone="accent">add column</Text> users.email</>} />
      <ListItem leading={<Checkbox checked={false} onChange={() => {}} />} title={<><Text tone="danger">drop table</Text> old_stuff</>} />
      <ListItem title={<b>tickets search</b>} subtitle="3,000 rows · 2026-10-27 14:50" />
    </Frame>
  ),
};

/** Detail row: confirm dialog statement + row count · pid */
export const DetailRows: S = {
  name: "DetailRow",
  render: () => (
    <div style={{ width: 640, border: "1px solid var(--ui-border)", borderRadius: 6, overflow: "hidden" }}>
      <DetailRow value="pid 1385">select pg_sleep(600) /* sleeper */</DetailRow>
      <DetailRow value="3 rows" tone="warn">
        update users set name = 'x' where id &lt;= 3
      </DetailRow>
      <DetailRow value="12,408 rows" tone="danger">
        delete from orders where created_at &lt; now() - interval '1 year'
      </DetailRow>
      <DetailRow value="schema change" tone="danger">
        create table refunds(id int)
      </DetailRow>
    </div>
  ),
};

/** Under a connect error that needs TLS (tree status, console Output, table tab, Test Connection). The click resolves after a second here */
const TLS_ERROR = "This server didn't set up an encrypted (TLS) connection, so Qurry didn't send the password. If it's an old server and the network is trusted, turn on \"Allow unencrypted connection\" for this data source (GUI: Options tab · TUI: data source form, \"Allow unencrypted\").";
const slow = () => new Promise((r) => setTimeout(r, 1000));
export const TlsOptInDev: S = {
  name: "TlsOptIn · dev",
  render: () => (
    <Frame width={420}>
      <div className="tree-status err" style={{ paddingLeft: 24 }}>
        {TLS_ERROR}
        <TlsOptIn host="10.0.0.12" onAllow={slow} />
      </div>
      <div className="tree-status err" style={{ paddingLeft: 24 }}>
        The allow step itself failed:
        <TlsOptIn host="10.0.0.12" onAllow={() => slow().then(() => Promise.reject("Data source not found"))} />
      </div>
    </Frame>
  ),
};
export const TlsOptInProduction: S = {
  name: "TlsOptIn · Production",
  render: () => (
    <Frame width={420}>
      <div className="tree-status err" style={{ paddingLeft: 24 }}>
        {TLS_ERROR}
        <TlsOptIn host="prod-db.internal" production onAllow={slow} />
      </div>
      <Notice tone="danger">
        In Data Source Properties (inDialog): the Options tab is right there
        <TlsOptIn host="prod-db.internal" production inDialog onAllow={slow} />
      </Notice>
    </Frame>
  ),
};

export const MenuItems: S = {
  name: "MenuItem",
  render: () => (
    <div className="ui-menu" style={{ position: "static", width: 260 }}>
      <MenuItem label="New Query Console" shortcut="⇧⌘L" />
      <MenuItem label="Diagrams" hasSubmenu />
      <MenuItem label="Disabled" disabled />
      <MenuItem label="Rename…" shortcut="⇧F6" disabled title="Works on one node: click a node without ⌘ or ⇧ to select only it" />
      <MenuSeparator />
      <MenuLabel>Add to this workspace:</MenuLabel>
      <MenuItem label="Remove Data Source" danger />
    </div>
  ),
};

/** Tree row: explorer · schema picker · Redis keys */
export const TreeRows: S = {
  name: "TreeRow",
  render: () => (
    <Frame width={360}>
      <TreeRow depth={0} open icon={<PgIcon />} label={<b>[PROD] kr</b>} badge={<Badge onClick={() => {}}>3</Badge>} />
      <TreeRow depth={1} open icon={<DatabaseIcon />} label="kr" badge={<Badge>1 of 3</Badge>} />
      <TreeRow depth={2} open icon={<SchemaIcon />} label="public" />
      <TreeRow depth={3} open icon={<FolderIcon />} label="tables" sub="9" />
      <TreeRow depth={4} icon={<TableIcon />} label="users" selected />
      <TreeRow depth={4} icon={<TableIcon />} label="orders" sub="주문" />
      {/* Several selected (⌘-click / ⇧-click): each selected row has the selection color */}
      <TreeRow depth={4} icon={<TableIcon />} label="payments" selected />
      <TreeRow depth={0} leaf leading={<Checkbox checked onChange={() => {}} />} label="All schemas" />
      {/* Redis: a database in the explorer (key count beside it, no keys under it); in the Redis tab a folder's count beside its name, a key's TTL on the right (— without expiry) */}
      <TreeRow depth={1} leaf icon={<DatabaseIcon />} label="db0" sub="500" />
      <TreeRow depth={0} icon={<FolderIcon />} label="sessions" sub="419" />
      <TreeRow depth={1} leaf icon={<Badge color="#5f9ea0">STRING</Badge>} iconClass="rd-type" label="session:abc" trailing="1d 4h" />
      <TreeRow depth={1} leaf icon={<Badge color="#5f9ea0">STRING</Badge>} iconClass="rd-type" label="session:def" trailing={<span className="rd-ttl none">—</span>} />
    </Frame>
  ),
};

/** One tab: editor tab · result tab */
export const TabItems: S = {
  name: "TabItem · RailItem · StatusItem",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <div style={{ display: "flex", height: 36, background: "var(--ui-surface)" }}>
        <TabItem icon={<ConsoleIcon />} label="console [PROD] kr" active onClose={() => {}} style={connTabVars("#6e2a2a")} />
        <TabItem icon={<TableIcon />} label="users" onClose={() => {}} />
        <TabItem icon={<ReportIcon />} label="Sessions" pinned onPin={() => {}} />
      </div>
      {/* Every tab kind, and a DDL tab per object kind (TabIcon: the explorer tree's icons, a DDL mark on DDL tabs) */}
      <div style={{ display: "flex", flexWrap: "wrap", background: "var(--ui-surface)" }}>
        {(
          [
            ["console", undefined, "console"],
            ["table", "table", "users"],
            ["table", "view", "active_users"],
            ["ddl", "table", "users [DDL]"],
            ["ddl", "view", "active_users [DDL]"],
            ["ddl", "matview", "daily_sales [DDL]"],
            ["ddl", "routine", "refresh_stats [DDL]"],
            ["ddl", "sequence", "users_id_seq [DDL]"],
            ["ddl", undefined, "SQL (no object kind)"],
            ["redis", undefined, "cache"],
            ["dba", undefined, "Sessions"],
            ["erd", undefined, "Diagram: public"],
            ["migrate", undefined, "Compare"],
            ["rag", undefined, "Ask"],
          ] as const
        ).map(([kind, obj, label]) => (
          <TabItem key={label} icon={<TabIcon kind={kind} obj={obj} />} label={label} style={{ height: 32 }} />
        ))}
      </div>
      <div style={{ display: "flex", height: 30, background: "var(--ui-surface)" }}>
        <TabItem variant="result" label="Output" />
        <TabItem variant="result" label="users" active onClose={() => {}} />
        <TabItem variant="result" label="Plan: Result 1" pinned onPin={() => {}} />
      </div>
      <div style={{ display: "flex", gap: 6, width: 56, flexDirection: "column", alignItems: "center", background: "var(--ui-surface-2)", padding: 8 }}>
        <RailItem initials="DE" label="Default" active />
        <RailItem initials="RE" label="Reports" />
      </div>
      <div className="statusbar" style={{ display: "flex", height: 24 }}>
        <Text truncate>1 statement executed</Text>
        <span style={{ flex: 1 }} />
        <StatusItem accent onClick={() => {}}>
          ⬆ v0.2.5 available
        </StatusItem>
        <StatusItem>🔒 Read-only</StatusItem>
        <StatusItem>1:20</StatusItem>
        <StatusItem>UTF-8</StatusItem>
      </div>
    </div>
  ),
};
