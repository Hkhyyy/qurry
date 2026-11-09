import type { Meta, StoryObj } from "@storybook/react-vite";
import { useRef, useEffect, useState } from "react";
import { userEvent, within } from "storybook/test";
import {
  Badge,
  Button,
  Checkbox,
  ComboBox,
  ConfirmHost,
  confirm,
  ConfirmOptions,
  DetailList,
  FormGrid,
  FormSection,
  IconButton,
  Kbd,
  KeyHints,
  List,
  ListGroup,
  ListItem,
  Menu,
  Modal,
  Notice,
  Popover,
  PopupList,
  RadioGroup,
  SearchField,
  Select,
  SectionTitle,
  Spacer,
  TabItem,
  Table,
  Tabs,
  TabStrip,
  Td,
  Text,
  TextInput,
  Th,
  ToolButton,
  Toolbar,
  ToolbarSep,
} from "..";
import { connTabVars } from "../../lib/theme";
import { TabIcon } from "../icons";

/** Organisms: blocks made of molecules — toolbar · dialog · confirm dialog · menu · popover · list · table · form */
const meta: Meta = { title: "GUI/3 Organisms" };
export default meta;
type S = StoryObj;

export const ToolbarStory: S = {
  name: "Toolbar",
  render: () => (
    <div style={{ width: 760, border: "1px solid var(--ui-border)" }}>
      <Toolbar>
        <IconButton title="Execute (⌘↩)" tone="ok">
          ▶
        </IconButton>
        <ToolButton>Explain ▾</ToolButton>
        <IconButton title="History">⏱</IconButton>
        <ToolbarSep />
        <ToolButton active>Tx: Manual ▾</ToolButton>
        <ToolButton accent>✓ Commit</ToolButton>
        <ToolButton>↶ Roll back</ToolButton>
        <Spacer />
        <Text tone="muted">[PROD] kr</Text>
        <Badge tone="danger">PROD</Badge>
      </Toolbar>
    </div>
  ),
};

export const ListStory: S = {
  name: "List · DetailList",
  render: () => (
    <div style={{ display: "flex", gap: 24 }}>
      <List boxed style={{ width: 320, height: 180 }}>
        <ListGroup>Database</ListGroup>
        <ListItem icon="▦" title="users" hint="public" trailing="kr" selected />
        <ListItem icon="▦" title="orders" hint="public" trailing="kr" />
        <ListGroup>Actions</ListGroup>
        <ListItem icon="⚡" title="Back Up Database…" trailing={<Kbd>⌃k</Kbd>} />
      </List>
      <List boxed style={{ width: 240, height: 180 }} empty="Nothing found" />
      <div style={{ width: 520 }}>
        <DetailList
          items={[
            { text: "update users set name = 'x' where id <= 3", value: "3 rows", tone: "warn" },
            { text: "create table refunds(id int)", value: "schema change", tone: "danger" },
          ]}
        />
      </div>
    </div>
  ),
};

export const TableStory: S = {
  name: "Table",
  render: () => (
    <div style={{ width: 640, border: "1px solid var(--ui-border)", borderRadius: 6, overflow: "hidden" }}>
      <Table>
        <thead>
          <tr>
            <Th>Version</Th>
            <Th>Description</Th>
            <Th num>Rows</Th>
            <Th>Applied</Th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <Td mono>V1</Td>
            <Td>create</Td>
            <Td num>0</Td>
            <Td muted>2026-10-28 14:32</Td>
          </tr>
          <tr>
            <Td mono>V2</Td>
            <Td>add index</Td>
            <Td num>12,408</Td>
            <Td muted>—</Td>
          </tr>
        </tbody>
      </Table>
    </div>
  ),
};

export const MenuStory: S = {
  name: "Menu",
  render: () => (
    <div style={{ height: 300 }}>
      <Menu
        at={{ x: 24, y: 24 }}
        onClose={() => {}}
        items={[
          { label: "New Query Console", shortcut: "⇧⌘L", action: () => {} },
          { label: "Diagrams", submenu: [{ label: "Show Diagram (ERD)", action: () => {} }] },
          { separator: true },
          { heading: "Add to this workspace:" },
          { label: "[DEV] shop_app", action: () => {} },
          { separator: true },
          { label: "Remove Data Source", danger: true, action: () => {} },
          { label: "Disabled", disabled: true, action: () => {} },
        ]}
        footer={<Text tone="muted" size="sm">Color</Text>}
      />
    </div>
  ),
};

/** Like Search Everywhere: header (tabs · search field) + list + key hints below */
export const PopoverStory: S = {
  name: "Popover",
  parameters: { layout: "fullscreen" },
  render: function Render() {
    const [t, setT] = useState<"all" | "db">("all");
    return (
      <div style={{ height: 420, position: "relative" }}>
        <Popover
          width={640}
          head={
            <>
              <div style={{ display: "flex", padding: "4px 12px 0", borderBottom: "1px solid var(--ui-border)" }}>
                <Tabs
                  size="sm"
                  items={[
                    { id: "all", label: "All" },
                    { id: "db", label: "Database" },
                  ]}
                  value={t}
                  onChange={setT}
                />
              </div>
              <SearchField bare autoFocus defaultValue="user" />
            </>
          }
          foot={
            <KeyHints
              items={[
                ["↩", "Open"],
                ["⇥", "Next tab"],
              ]}
            />
          }
        >
          <List>
            <ListGroup>Database</ListGroup>
            <ListItem icon="▦" title="users" hint="public" trailing="[PROD] kr · kr" selected />
            <ListItem icon="·" title="user_id" hint="int8 · public.orders" trailing="[PROD] kr · kr" />
          </List>
        </Popover>
      </div>
    );
  },
};

export const FormGridStory: S = {
  name: "FormGrid",
  render: function Render() {
    const [ssh, setSsh] = useState(true);
    return (
      <div style={{ width: 620 }}>
        <FormGrid>
          <FormSection>SSH tunnel</FormSection>
          <span />
          <Checkbox checked={ssh} onChange={setSsh}>
            Use SSH tunnel
          </Checkbox>
          <label>Host</label>
          <div className="ui-row">
            <TextInput grow defaultValue="bastion.example.com" disabled={!ssh} />
            <Text tone="muted">Port</Text>
            <TextInput className="w-port" defaultValue="22" disabled={!ssh} />
          </div>
          <label>Auth type</label>
          <Select defaultValue="key" disabled={!ssh}>
            <option value="key">Key pair</option>
          </Select>
        </FormGrid>
      </div>
    );
  },
};

export const ModalStory: S = {
  name: "Modal",
  parameters: { layout: "fullscreen" },
  render: () => (
    <Modal
      title="Back Up kr"
      onClose={() => {}}
      footer={
        <>
          <Button>Cancel</Button>
          <Button variant="primary">Choose File & Back Up</Button>
        </>
      }
    >
      <SectionTitle>Format</SectionTitle>
      <RadioGroup value="custom" onChange={() => {}} options={[{ value: "custom", label: <b>Custom (.dump)</b>, hint: "compressed" }, { value: "sql", label: <b>SQL (.sql)</b>, hint: "plain text" }]} />
      <SectionTitle>What</SectionTitle>
      <RadioGroup value="all" onChange={() => {}} options={[{ value: "all", label: <b>Schema and data</b> }, { value: "schema", label: <b>Schema only</b> }]} />
      <Notice>Runs pg_dump on this computer.</Notice>
    </Modal>
  ),
};

function OpenConfirm(o: ConfirmOptions) {
  useEffect(() => {
    confirm(o);
  }, []);
  return <ConfirmHost />;
}

/** Confirm dialog (confirm()): red button when dangerous · default focus on Cancel */
export const ConfirmProduction: S = {
  parameters: { layout: "fullscreen" },
  render: () => (
    <OpenConfirm
      title="Production: [PROD] kr"
      message="These statements change data. Run them?"
      detail={[
        { text: "update users set name = 'x' where id <= 3", value: "3 rows", tone: "warn" },
        { text: "delete from orders where created_at < now() - interval '1 year'", value: "12,408 rows", tone: "danger" },
        { text: "create table refunds(id int)", value: "schema change", tone: "danger" },
      ]}
      okLabel="Run"
      danger
    />
  ),
};
/** Explain Analyse of several statements that need a look: the SQL in the code, its reason a plain line under it */
export const ConfirmExplainAnalyse: S = {
  parameters: { layout: "fullscreen" },
  render: () => (
    <OpenConfirm
      title="[PROD] kr: Explain Analyse"
      message="Explain Analyse really runs the statements, and 2 of them need a look. Run them anyway?"
      detail={[
        { text: "select nextval('orders_id_seq')", note: "a rollback can't undo what it does outside the transaction: sequence values, other servers, files, locks, or a COMMIT · MySQL DDL among several statements" },
        { text: "delete from users", note: "DELETE without WHERE changes every row, holding their locks until the rollback undoes it" },
      ]}
      okLabel="Run Anyway"
      danger
    />
  ),
};
export const ConfirmTerminate: S = {
  parameters: { layout: "fullscreen" },
  render: () => <OpenConfirm title="[PROD] kr" message="Terminate Session 1385?" detail={[{ text: "select pg_sleep(600) /* sleeper */", value: "pid 1385" }]} okLabel="Terminate Session" danger />,
};
export const ConfirmChoice: S = {
  parameters: { layout: "fullscreen" },
  render: () => <OpenConfirm title="Switch to Auto" message="This console has uncommitted changes." okLabel="Commit and Switch" cancelLabel="Stay in Manual" />,
};

// ───────── Editor tab bar: the active tab in each theme, split panes, many tabs ─────────
/** kind / obj: tab kind and the object kind it shows (the icon comes from TabIcon, as in the app) */
type DemoTab = { label: string; kind: string; obj?: string; color?: string };
const PROD = "#6e2a2a";
const DEV = "#5f5b24";
const MYSQL = "#22524f";
const LEFT: DemoTab[] = [
  { label: "users", kind: "table", obj: "table", color: DEV },
  { label: "active_users", kind: "table", obj: "view", color: DEV },
  { label: "console [PROD] shop_app", kind: "console", color: PROD },
  { label: "active_users [DDL]", kind: "ddl", obj: "view", color: PROD },
  { label: "Sessions", kind: "dba" },
  { label: "public.payments", kind: "table", obj: "table", color: MYSQL },
];
const RIGHT: DemoTab[] = [
  { label: "console [DEV] shop_app", kind: "console", color: DEV },
  { label: "refresh_stats [DDL]", kind: "ddl", obj: "routine", color: DEV },
  { label: "customers", kind: "table", obj: "table" },
];
const MANY: DemoTab[] = Array.from({ length: 16 }, (_, i) => (i % 3 === 0 ? { label: `console ${i + 1}`, kind: "console", color: [PROD, DEV, MYSQL][(i / 3) % 3] } : { label: `table_${i + 1}`, kind: "table", obj: "table" }));

/** One editor group as the app draws it (useEditorGroups): tab strip over the work area. Click a tab or a pane to activate / focus it */
function DemoGroup({ tabs, state, onFocus }: { tabs: DemoTab[]; state?: "focused" | "unfocused"; onFocus?: () => void }) {
  const [active, setActive] = useState(tabs.length >> 1);
  return (
    <div className={"panel editor-group" + (state ? " " + state : "")} style={{ flex: 1, minWidth: 0 }} onMouseDownCapture={onFocus}>
      <TabStrip className="tabs" activeKey={String(active)} overflow={tabs.map((t, i) => ({ key: String(i), label: t.label, active: i === active, onSelect: () => setActive(i) }))}>
        {tabs.map((t, i) => (
          <TabItem key={i} icon={<TabIcon kind={t.kind} obj={t.obj} />} label={t.label} active={i === active} onClose={() => {}} onMouseDown={() => setActive(i)} style={connTabVars(t.color)} />
        ))}
      </TabStrip>
      <div className="work-area" style={{ height: 56, boxShadow: tabs[active].color ? `inset 0 2px 0 ${tabs[active].color}` : undefined }} />
    </div>
  );
}

function TabBars() {
  const [focused, setFocused] = useState(0);
  const state = (g: number) => (focused === g ? "focused" : "unfocused");
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16, width: 900 }}>
      <Text tone="muted">One pane</Text>
      <div style={{ display: "flex", border: "1px solid var(--ui-border)" }}>
        <DemoGroup tabs={LEFT} />
      </div>
      <Text tone="muted">Split: the focused pane's active tab has the accent bar, the other pane's a gray one</Text>
      <div style={{ display: "flex", gap: 1, background: "var(--ui-border)", border: "1px solid var(--ui-border)" }}>
        <DemoGroup tabs={LEFT} state={state(0)} onFocus={() => setFocused(0)} />
        <DemoGroup tabs={RIGHT} state={state(1)} onFocus={() => setFocused(1)} />
      </div>
      <Text tone="muted">Many tabs: the strip scrolls to the active tab; ▾ lists them all</Text>
      <div style={{ display: "flex", width: 640, border: "1px solid var(--ui-border)" }}>
        <DemoGroup tabs={MANY} />
      </div>
    </div>
  );
}

export const TabBarDark: S = { name: "Tab bar — dark", globals: { theme: "dark" }, render: () => <TabBars /> };
export const TabBarLight: S = { name: "Tab bar — light", globals: { theme: "light" }, render: () => <TabBars /> };
export const TabBarHc: S = { name: "Tab bar — high contrast", globals: { theme: "hc" }, render: () => <TabBars /> };

// ───────── List popup under a field (PopupList) and its two uses: ComboBox (free text) · FontPicker (select, in Screens) ─────────
const PG_TYPES = ["integer", "bigint", "smallint", "numeric(10,2)", "real", "double precision", "boolean", "text", "varchar(255)", "char(1)", "date", "timestamp", "timestamptz", "time", "interval", "uuid", "json", "jsonb", "bytea", "inet", "text[]"];

/** A field with its PopupList always open: `rows` listed, row `active` highlighted */
function PopupDemo({ label, value = "", rows, active = 0, height, empty }: { label: string; value?: string; rows: string[]; active?: number; height?: number; empty?: string }) {
  const field = useRef<HTMLInputElement>(null);
  return (
    <div style={{ width: 170 }}>
      <Text tone="muted" size="sm">
        {label}
      </Text>
      <TextInput ref={field} full readOnly value={value} placeholder="type" />
      <PopupList anchor={field} active={active} height={height} empty={empty} onClose={() => {}}>
        {rows.map((r, i) => (
          <ListItem key={r} title={r} selected={i === active} />
        ))}
      </PopupList>
    </div>
  );
}
const PopupStates = () => (
  <div style={{ display: "flex", gap: 48, height: 320 }}>
    <PopupDemo label="Default" rows={PG_TYPES.slice(0, 6)} />
    <PopupDemo label="Highlighted row" rows={PG_TYPES.slice(0, 6)} active={3} />
    <PopupDemo label="Filtered" value="int" rows={["integer", "interval", "bigint", "smallint"]} />
    <PopupDemo label="Empty (fixed height)" rows={[]} height={120} empty="No matching font" />
    <PopupDemo label="Long list (scrolls)" rows={Array.from({ length: 60 }, (_, i) => `column_type_${i + 1}`)} active={30} />
  </div>
);
/**
 * PopupList: the one drop-down list under a field. Right under the field and left-aligned to it, at least as wide, inside the
 * window; the highlighted row stays in view. Rows are ListItems, keys are the owner's (listKey: ↑↓ PgUp PgDn, ↩ pick, ⎋ close).
 * An owner that has nothing to show (ComboBox with no match) doesn't render it; "Empty" is the select mode with a search box
 */
export const PopupListStory: S = { name: "List popup — states (dark)", globals: { theme: "dark" }, render: () => <PopupStates /> };
export const PopupListLight: S = { name: "List popup — states (light)", globals: { theme: "light" }, render: () => <PopupStates /> };
export const PopupListHc: S = { name: "List popup — states (high contrast)", globals: { theme: "hc" }, render: () => <PopupStates /> };
/** No room below the field: it opens above it, its bottom on the field's top */
export const PopupListFlipped: S = {
  name: "List popup — flipped above",
  render: () => (
    <div style={{ height: "calc(100vh - 32px)", display: "flex", alignItems: "flex-end" }}>
      <PopupDemo label="At the bottom of the window" rows={PG_TYPES.slice(0, 8)} active={1} />
    </div>
  ),
};

function TypeField({ initial = "" }: { initial?: string }) {
  const [v, setV] = useState(initial);
  return <ComboBox value={v} options={PG_TYPES} placeholder="type" aria-label="Type" onChange={setV} />;
}
const typeField = (canvas: HTMLElement) => within(canvas).getByRole("combobox", { name: "Type" });
/**
 * ComboBox: free text with suggestions (the table designer's column type). Typing filters (exact, prefix, then contains; no popup
 * when nothing matches, so varchar(100) is just kept), ↓ or a click lists them all with the current one highlighted; ↑↓ PgUp PgDn
 * move, ↩ or ⇥ takes the highlighted one, ⎋ closes only the list. Like every ui-kit text field: no autocorrect / spell check
 */
export const ComboBoxOpen: S = {
  name: "ComboBox — open",
  render: () => <TypeField initial="text" />,
  play: async ({ canvasElement }) => userEvent.click(typeField(canvasElement)),
};
export const ComboBoxFiltered: S = {
  name: "ComboBox — filtered (\"int\")",
  render: () => <TypeField />,
  play: async ({ canvasElement }) => userEvent.type(typeField(canvasElement), "int"),
};
export const ComboBoxFlipped: S = {
  name: "ComboBox — flipped above",
  render: () => (
    <div style={{ height: "calc(100vh - 32px)", display: "flex", alignItems: "flex-end" }}>
      <TypeField initial="bigint" />
    </div>
  ),
  play: async ({ canvasElement }) => userEvent.click(typeField(canvasElement)),
};
