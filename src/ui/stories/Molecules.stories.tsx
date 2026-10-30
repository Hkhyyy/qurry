import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Badge, Checkbox, ColorPicker, DetailRow, EmptyState, Field, FormSection, Hint, Kbd, KeyHints, ListItem, MenuItem, MenuLabel, MenuSeparator, Notice, RadioGroup, RailItem, SearchField, SectionTitle, Select, StatusItem, TabItem, Tabs, Text, TextInput, TreeRow } from "..";
import { DatabaseIcon, FolderIcon, PgIcon, SchemaIcon, TableIcon } from "../icons";

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

/** List row: search results · history · switcher · migration · RAG */
export const ListItems: S = {
  name: "ListItem",
  render: () => (
    <Frame>
      <ListItem icon="▦" title="users" hint="public" trailing="[PROD] kr · kr" color="#6e2a2a" selected />
      <ListItem icon="⚡" title="New Query Console" hint="Tool Windows" trailing={<Kbd>⇧⌘L</Kbd>} />
      <ListItem icon="⌘" title="console_2 [DEV] shop" hint="closed tab" dim color="#2f5634" />
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

export const MenuItems: S = {
  name: "MenuItem",
  render: () => (
    <div className="ui-menu" style={{ position: "static", width: 260 }}>
      <MenuItem label="New Query Console" shortcut="⇧⌘L" />
      <MenuItem label="Diagrams" hasSubmenu />
      <MenuItem label="Disabled" disabled />
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
      <TreeRow depth={0} leaf leading={<Checkbox checked onChange={() => {}} />} label="All schemas" />
      <TreeRow depth={1} leaf icon={<Badge color="#5f9ea0">STRING</Badge>} iconClass="rd-type" label="session:abc" trailing="59m 0s" />
    </Frame>
  ),
};

/** One tab: editor tab · result tab */
export const TabItems: S = {
  name: "TabItem · RailItem · StatusItem",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <div style={{ display: "flex", height: 36, background: "var(--ui-surface)" }}>
        <TabItem icon="⌘" label="console [PROD] kr" active onClose={() => {}} style={{ background: "color-mix(in srgb, #6e2a2a 35%, var(--panel))", boxShadow: "inset 0 2px 0 #c46a6a" }} />
        <TabItem icon="▦" iconClass="table" label="users" onClose={() => {}} />
        <TabItem icon="◉" label="Sessions" pinned onPin={() => {}} />
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
