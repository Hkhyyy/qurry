import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  Checkbox,
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
  RadioGroup,
  SearchField,
  Select,
  SectionTitle,
  Spacer,
  Table,
  Tabs,
  Td,
  Text,
  TextInput,
  Th,
  ToolButton,
  Toolbar,
  ToolbarSep,
} from "..";

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
export const ConfirmTerminate: S = {
  parameters: { layout: "fullscreen" },
  render: () => <OpenConfirm title="[PROD] kr" message="Terminate Session 1385?" detail={[{ text: "select pg_sleep(600) /* sleeper */", value: "pid 1385" }]} okLabel="Terminate Session" danger />,
};
export const ConfirmChoice: S = {
  parameters: { layout: "fullscreen" },
  render: () => <OpenConfirm title="Switch to Auto" message="This console has uncommitted changes." okLabel="Commit and Switch" cancelLabel="Stay in Manual" />,
};
