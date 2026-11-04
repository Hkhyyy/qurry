import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, userEvent, waitFor, within } from "storybook/test";
// Fake backend (same as the app's ?mock): returns fixed data for the commands screens call
import "../../lib/mock";
import "../../lib/monaco";
import App from "../../App";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { setFont } from "../../lib/prefs";
import { dialectOf, type ConnectionView, type QueryResult } from "../../lib/api";
import { ConnectionDialog } from "../../components/ConnectionDialog";
import { ConsolePane, SPLITTER_FAILED } from "../../components/ConsolePane";
import { CopyRowsDialog, GenerateDataDialog } from "../../components/DataTools";
import { DbaReport } from "../../components/DbaReport";
import { ErdView } from "../../components/ErdView";
import { FullTextSearchDialog } from "../../components/FullTextSearch";
import { HistoryDialog } from "../../components/HistoryDialog";
import { KeymapDialog } from "../../components/KeymapDialog";
import { MigrateView } from "../../components/MigrateView";
import { NewObjectDialog, TableDesigner } from "../../components/ObjectEditors";
import { ProductionLog } from "../../components/ProductionLog";
import { RagView } from "../../components/RagView";
import { RedisBrowser } from "../../components/RedisBrowser";
import { ResultChart } from "../../components/ResultChart";
import { ResultGrid } from "../../components/ResultGrid";
import { RunningBar } from "../../components/RunningBar";
import { SearchEverywhere } from "../../components/SearchEverywhere";
import { TableStructureView } from "../../components/TableStructure";
import { Switcher } from "../../components/Switcher";
import { BackupDialog, ExportDialog, ImportDialog } from "../../components/Transfer";
import { UpdateDialog } from "../../components/UpdateDialog";
import { WorkspaceBar } from "../../components/WorkspaceBar";
import { ConsoleIcon, ReportIcon, TabIcon, TableIcon } from "../icons";

/** Screens: real screen components with fake data (same data as the app's ?mock). Buttons are clickable */
const meta: Meta = { title: "GUI/5 Screens", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const conn = (id: string, name: string, kind: string, color: string | null, extra: Partial<ConnectionView> = {}) =>
  ({ id, name, kind, host: "db.example.com", port: kind === "mysql" ? 3306 : 5432, user: "app", database: "shop_app", color, hasPassword: true, connected: true, ...extra }) as ConnectionView;
const DEV = conn("dev", "[DEV] shop_app", "postgres", "#5f5b24");
const PROD = conn("prod", "[PROD] shop_app", "postgres", "#6e2a2a", { options: { confirmChanges: true } } as Partial<ConnectionView>);
const STAGE = conn("stage", "[STAGE] shop_app", "postgres", "#28406e");
const REDIS = conn("rd", "[Redis] cache", "redis", "#6e2a2a", { database: "0" });
const CONNS = [DEV, PROD, STAGE, REDIS];
const noop = () => {};

const sales: QueryResult = {
  columns: [
    { name: "region", typeName: "TEXT" },
    { name: "month", typeName: "DATE" },
    { name: "orders", typeName: "INT8" },
    { name: "revenue", typeName: "NUMERIC" },
  ],
  rows: ["Seoul", "Busan", "Incheon", "Daegu"].flatMap((r, i) =>
    ["2026-07-01", "2026-08-01", "2026-09-01", "2026-10-01"].map((m, j) => [r, m, 120 - i * 20 + j * 7, ((4 - i) * 12000 + j * 1500).toFixed(2)]),
  ),
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 12,
  hasResultSet: true,
};

const Frame = ({ children, h = 560 }: { children: React.ReactNode; h?: number }) => <div style={{ height: h, display: "flex", flexDirection: "column", border: "1px solid var(--ui-border)" }}>{children}</div>;

/** Whole app (explorer · tabs · console). Right-click the explorer, ⇧⌘A for Find Action, ⌘↩ in the console to run */
export const FullApp: S = { name: "App (whole window)", render: () => <div style={{ height: "100vh" }}><App /></div> };

/** Database Explorer, several nodes: ⌘-click two tables, then right-click one. Go to DDL, Edit Data, Copy Name / Reference, Drop… and
 *  Refresh run for both; the commands for one node are greyed out (their tooltip: click a node without ⌘ or ⇧). ⇧-click selects a range */
export const ExplorerSeveral: S = {
  name: "Database Explorer — several selected",
  render: () => <div style={{ height: "100vh" }}><App /></div>,
  play: async ({ canvasElement }) => {
    const row = (name: string) => canvasElement.querySelector<HTMLElement>(`.ws-explorer.on [data-node="dev|shop_app|public|${name}"]`)!;
    await waitFor(() => expect(row("users")).toBeTruthy(), { timeout: 5000 });
    await userEvent.click(row("contracts"));
    // ⌘-click (⌃-click off macOS)
    fireEvent.mouseDown(row("users"), navigator.platform.includes("Mac") ? { metaKey: true } : { ctrlKey: true });
    const r = row("users").getBoundingClientRect();
    fireEvent.contextMenu(row("users"), { clientX: r.left + 120, clientY: r.bottom });
  },
};

/** Settings › Appearance: theme, editor and grid fonts, grid alignment per value class, the automatic update check and the update channel (alignment and channel shared with the TUI) */
export const Settings: S = { name: "Settings — Appearance & updates", render: () => <SettingsDialog onClose={noop} onUpdate={noop} onChannel={noop} /> };
/** Editor / Grid font: each family is picked from a drop-down of the installed fonts (search, "Monospaced only" on by default, the default
 * entry first), sizes in px. A saved family that isn't installed stays on top of the list marked "(not installed)" and renders with the
 * default monospace. The choice is saved like in the app; preview.tsx resets it before every story. The drop-down is the ui kit's
 * PopupList in select mode (search box on top, fixed height), the same list as ComboBox's (GUI/3 Organisms › List popup) */
export const SettingsFonts: S = {
  name: "Settings — Fonts (font not installed)",
  render: () => <SettingsDialog onClose={noop} onUpdate={noop} onChannel={noop} />,
  play: async ({ canvasElement }) => {
    setFont("grid", { family: "No Such Mono" });
    await userEvent.click(await within(canvasElement.ownerDocument.body).findByRole("combobox", { name: "Grid font family" }));
  },
};

/** Settings › Keymap: every menu, toolbar and Find Action command by scope (Global, Database Explorer, Query Console, Data Grid), "none" where it has no key yet */
export const SettingsKeymap: S = { name: "Settings — Keymap", render: () => <SettingsDialog initialPage="keymap" onClose={noop} onUpdate={noop} onChannel={noop} /> };

export const NewDataSource: S = { name: "Data Source — new", render: () => <ConnectionDialog onClose={noop} onSaved={noop} /> };
export const EditDataSource: S = { name: "Data Source — edit (SSH)", render: () => <ConnectionDialog edit={{ ...PROD, ssh: { enabled: true, host: "bastion.example.com", port: 22, user: "ec2-user", auth: "key", keyPath: "~/.ssh/id_ed25519" } } as ConnectionView} onClose={noop} onSaved={noop} /> };
/** SSH/SSL tab with Tunnel = AWS SSM: profile · region · instance id. Test Connection answers "(via AWS SSM i-…)" */
export const EditDataSourceSsm: S = {
  name: "Data Source — edit (AWS SSM)",
  render: () => (
    <ConnectionDialog
      edit={{ ...PROD, host: "orders.cluster-abc.ap-northeast-2.rds.amazonaws.com", ssm: { enabled: true, profile: "prod", region: "ap-northeast-2", target: "i-0123456789abcdef0" } } as ConnectionView}
      onClose={noop}
      onSaved={noop}
    />
  ),
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement.ownerDocument.body).getByRole("tab", { name: /SSH\/SSL/ }));
  },
};
/** An SSM Test Connection that is still starting (the instance never answers): elapsed time and Cancel Test, which stops the session */
export const EditDataSourceSsmTesting: S = {
  name: "Data Source — AWS SSM test running",
  render: () => (
    <ConnectionDialog
      edit={{ ...PROD, host: "orders.cluster-abc.ap-northeast-2.rds.amazonaws.com", ssm: { enabled: true, profile: "prod", region: "ap-northeast-2", target: "i-00000000000000000" } } as ConnectionView}
      onClose={noop}
      onSaved={noop}
    />
  ),
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(body.getByRole("tab", { name: /SSH\/SSL/ }));
    await userEvent.click(body.getByRole("button", { name: "Test Connection" }));
  },
};
/** A failed SSM Test Connection with a long message (no Session Manager plugin): the body scrolls so its last line, what to do, shows */
export const EditDataSourceSsmError: S = {
  name: "Data Source — AWS SSM test error",
  render: () => (
    <ConnectionDialog
      edit={{ ...PROD, host: "orders.cluster-abc.ap-northeast-2.rds.amazonaws.com", ssm: { enabled: true, profile: "prod", region: "ap-northeast-2", target: "i-0000000000000dead" } } as ConnectionView}
      onClose={noop}
      onSaved={noop}
    />
  ),
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(body.getByRole("tab", { name: /SSH\/SSL/ }));
    await userEvent.click(body.getByRole("button", { name: "Test Connection" }));
  },
};

export const TableNew: S = { name: "New Table", render: () => <TableDesigner target="dev" conn={DEV} schema="public" onDone={noop} onOpenConsole={noop} onClose={noop} /> };
export const TableModify: S = { name: "Modify Table (⌘F6)", render: () => <TableDesigner target="dev" conn={DEV} schema="public" table="customers" onDone={noop} onOpenConsole={noop} onClose={noop} /> };
/** A column's type is a ComboBox: the suggestions for the data source's database open right under the field (typing "int" here
 * filters them, prefix matches first); any other type can be typed. ⎋ closes the list, not the dialog */
export const TableModifyTypes: S = {
  name: "Modify Table — column type suggestions",
  render: () => <TableDesigner target="dev" conn={DEV} schema="public" table="customers" onDone={noop} onOpenConsole={noop} onClose={noop} />,
  play: async ({ canvasElement }) => {
    const field = await within(canvasElement.ownerDocument.body).findByRole("combobox", { name: "Type of org_id" });
    await userEvent.clear(field);
    await userEvent.type(field, "int");
  },
};
export const NewView: S = { name: "New View", render: () => <NewObjectDialog kind="view" target="dev" conn={DEV} schema="public" onDone={noop} onOpenConsole={noop} onClose={noop} /> };
export const NewDatabase: S = { name: "New Database", render: () => <NewObjectDialog kind="database" target="dev" conn={DEV} schema="public" onDone={noop} onOpenConsole={noop} onClose={noop} /> };

export const ProdLog: S = { name: "Production Log", render: () => <ProductionLog connId={null} onOpenUndo={noop} onClose={noop} /> };
export const GenerateData: S = { name: "Generate Test Data", render: () => <GenerateDataDialog target="dev" schema="public" table="orders" onDone={noop} onClose={noop} /> };
export const CopyRows: S = { name: "Copy Related Rows", render: () => <CopyRowsDialog source="prod" schema="public" table="users" conn={PROD} conns={CONNS} onDone={noop} onClose={noop} /> };

export const Chart: S = { name: "Result Chart", render: () => <Frame><ResultChart result={sales} /></Frame> };
export const Grid: S = {
  name: "Result Grid",
  render: () => (
    <Frame>
      <ResultGrid result={sales} running={false} loadingMore={false} query="select region, month, orders, revenue from sales" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </Frame>
  ),
};

/** Every value class with NULLs, in a panel two rows high: the header stays under the toolbar while scrolling (#82), NULL sits on the right (#75) */
const mixed: QueryResult = {
  columns: [
    { name: "card_brand", typeName: "TEXT" },
    { name: "amount", typeName: "NUMERIC" },
    { name: "refunded", typeName: "BOOL" },
    { name: "paid_at", typeName: "TIMESTAMPTZ" },
    { name: "meta", typeName: "JSONB" },
    { name: "receipt", typeName: "BYTEA" },
    ...Array.from({ length: 8 }, (_, i) => ({ name: `extra_${i + 1}`, typeName: "INT4" })),
  ],
  rows: Array.from({ length: 40 }, (_, i) => [
    i % 4 === 3 ? null : ["VISA", "MASTER", "AMEX"][i % 3],
    i % 3 === 1 ? null : (129000 * (i + 1)).toFixed(2),
    i % 5 === 4 ? null : i % 2 === 0,
    i % 6 === 5 ? null : `2026-10-28 09:${String(i).padStart(2, "0")}:00 +09:00`,
    i % 2 ? null : `{"pg":"toss","n":${i}}`,
    i % 3 ? null : "0xDEADBEEF",
    ...Array.from({ length: 8 }, (_, j) => ((i + j) % 4 === 0 ? null : i * 10 + j)),
  ]),
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 4,
  hasResultSet: true,
};
export const GridShort: S = {
  name: "Result Grid — short panel, every value class",
  render: () => (
    <div style={{ position: "relative", height: 140, border: "1px solid var(--ui-border)" }}>
      <ResultGrid result={mixed} running={false} loadingMore={false} query="select * from payment_check" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </div>
  ),
};

/** Value Editor panel (⇧↩): beside the grid, not over it; follows the current cell (↑↓ keep moving the grid), jsonb pretty-printed and highlighted */
export const GridValuePanel: S = {
  name: "Result Grid — Value Editor panel (⇧↩)",
  render: () => (
    <div style={{ position: "relative", height: 420, border: "1px solid var(--ui-border)" }}>
      <ResultGrid result={mixed} edit={{ schema: "public", table: "payment_check", pk: ["card_brand"] }} running={false} loadingMore={false} query="select * from payment_check" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getAllByRole("gridcell", { name: '{"pg":"toss","n":0}' })[0]);
    await userEvent.keyboard("{Shift>}{Enter}{/Shift}");
  },
};

export const History: S = { name: "Query History", render: () => <HistoryDialog connId="dev" title="[DEV] shop_app Query History" onPick={noop} onClose={noop} /> };
export const Search: S = {
  name: "Search Everywhere (⇧⇧)",
  render: () => (
    <div style={{ height: 520, position: "relative" }}>
      <SearchEverywhere
        initialTab="all"
        conns={CONNS}
        contextConnId="dev"
        files={[{ key: "f1", icon: <ConsoleIcon />, title: "console [DEV] shop_app", hint: "[DEV] shop_app", open: true }]}
        actions={[{ name: "Production Log…", group: "Run", run: noop }, { name: "Generate Test Data…", group: "Data", run: noop }]}
        onClose={noop}
        onSelectInTree={noop}
        onOpenData={noop}
        onDdl={noop}
        onQuickDoc={noop}
        onOpenFile={noop}
      />
    </div>
  ),
};
export const FindInData: S = { name: "Full-text Search", render: () => <FullTextSearchDialog scope={{ connId: "dev", connName: "[DEV] shop_app", db: "shop_app", schemas: ["public"], tables: [] }} onClose={noop} onSearch={noop} /> };
export const SwitcherStory: S = {
  name: "Switcher (⌃⇥)",
  render: () => (
    <Switcher
      items={[
        { key: "a", icon: <ConsoleIcon />, title: "console [DEV] shop_app", hint: "[DEV] shop_app", open: true, color: "#5f5b24" },
        { key: "b", icon: <TableIcon />, title: "users", hint: "[PROD] shop_app · public", open: true, color: "#6e2a2a" },
        { key: "d", icon: <TabIcon kind="ddl" obj="view" />, title: "active_users [DDL]", hint: "[PROD] shop_app", open: true, color: "#6e2a2a" },
        { key: "c", icon: <ReportIcon />, title: "Sessions & Locks", hint: "[PROD] shop_app", open: true },
      ]}
      start={1}
      onPick={noop}
      onClose={noop}
    />
  ),
};
export const Keymap: S = { name: "Keymap Reference (⇧⌘/)", render: () => <KeymapDialog onClose={noop} onEdit={noop} /> };

export const Export: S = { name: "Export Data", render: () => <ExportDialog what="public.users" onExport={noop} onClose={noop} /> };
export const Import: S = { name: "Import Data", render: () => <ImportDialog target="dev" schema="public" table="users" onImport={noop} onClose={noop} /> };
export const Backup: S = { name: "Back Up Database", render: () => <BackupDialog what="shop_app" pg onRun={noop} onClose={noop} /> };
export const BackupMysql: S = { name: "Back Up Database (MySQL)", render: () => <BackupDialog what="shop_app" pg={false} onRun={noop} onClose={noop} /> };
export const Update: S = {
  name: "Update available",
  render: () => (
    <UpdateDialog
      info={{ current: "0.2.5", latest: { version: "0.2.6", url: "https://github.com/Hkhyyy/qurry/releases/tag/v0.2.6", notes: "", assets: [] }, changes: [{ version: "0.2.6", changes: ["[feat] 데이터 작업 · 결과 차트 · 운영 안전", "[fix] 탐색기 가로 스크롤"] }] } as never}
      onClose={noop}
    />
  ),
};

/** DDL tab → Structure (the toolbar's SQL | Structure switch picks it). Read-only sections from the table's metadata */
export const DdlStructure: S = { name: "DDL — Structure", render: () => <Frame h={720}><TableStructureView target="dev" schema="public" table="users" visible onShowSql={noop} /></Frame> };
export const DdlStructureBasics: S = {
  name: "DDL — Structure (source reports only the basics)",
  render: () => <Frame h={720}><TableStructureView target="dev" schema="billing" table="invoices" visible onShowSql={noop} /></Frame>,
};
export const DdlStructureError: S = {
  name: "DDL — Structure (load failed)",
  beforeEach: () => {
    (globalThis as any).__mockStructureFail = "connection reset by peer";
    return () => delete (globalThis as any).__mockStructureFail;
  },
  render: () => <Frame h={240}><TableStructureView target="dev" schema="public" table="users" visible onShowSql={noop} /></Frame>,
};
export const Diagram: S = { name: "Diagram (ERD)", render: () => <Frame h={600}><ErdView target="dev" schemas={["public"]} visible onOpenTable={noop} onStatus={noop} /></Frame> };
export const Dba: S = {
  name: "Database Admin",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const DbaSlowQueries: S = {
  name: "Database Admin: Slow Queries",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="slow_queries" readOnly={false} dialect={dialectOf("postgres")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const DbaUnusedIndexes: S = {
  name: "Database Admin: Unused Indexes",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="unused_indexes" readOnly={false} dialect={dialectOf("postgres")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const DbaError: S = {
  name: "Database Admin: report error",
  render: () => (
    <Frame h={600}>
      <div style={{ display: "grid", gridTemplateRows: "1fr 1fr", height: "100%" }}>
        <DbaReport target="busy" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
        <DbaReport target="maria" connName="[MariaDB] shop" report="slow_queries" readOnly={false} dialect={dialectOf("mysql")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
      </div>
    </Frame>
  ),
};
export const DbaStale: S = {
  name: "Database Admin: refresh failed",
  render: () => (
    <Frame h={600}>
      <DbaReport target="flaky" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const Migrate: S = { name: "Compare & Migrate", render: () => <Frame h={600}><MigrateView conns={CONNS} initialConnId="dev" onOpenConsole={noop} onStatus={noop} /></Frame> };
export const Rag: S = { name: "AI Search (RAG)", render: () => <Frame h={600}><RagView conns={CONNS} runJob={async (_l, run) => run("job")} onStatus={noop} /></Frame> };
/** ⌘↩ at startup while the WebAssembly module loads: shown like a run, Cancel (or ⌘F2) drops it */
export const ConsoleSplitterLoading: S = {
  name: "Console — loading the statement splitter",
  render: () => (
    <div style={{ width: 760, border: "1px solid var(--ui-border)" }}>
      <RunningBar running label="Loading the statement splitter" onCancel={noop} />
    </div>
  ),
};
/** The module failed to load: the run didn't start and Output says what to do (status bar: "Not run: …") */
export const ConsoleSplitterFailed: S = {
  name: "Console — statement splitter couldn't load",
  render: () => (
    <Frame h={420}>
      <ConsolePane
        tabId="story" target="dev" initialSql={"select * from customers;\nselect 1;"} connName="[DEV] shop_app" kind="postgres" dialect={dialectOf("postgres")} database="shop_app" schema="public"
        dbList={[]} schemaList={[]} running={false} loadingMore={false} results={[]} activeResult="output"
        output={[{ ts: "10:24:01", text: SPLITTER_FAILED, level: "error" }]} txManual={false} inTx={false}
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop} registerApi={noop} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  ),
};
export const Redis: S = {
  name: "Redis Browser",
  render: () => (
    <Frame h={600}>
      <RedisBrowser target="rd" connName="[Redis] cache" database="0" dbList={["db0", "db3"]} readOnly={false} onSwitchDb={noop} onLoadDbs={noop} onStatus={noop} />
    </Frame>
  ),
};
export const Workspaces: S = {
  name: "Workspace rail",
  render: () => (
    <div style={{ width: 64, height: 300, background: "var(--ui-surface-2)" }}>
      <WorkspaceBar
        list={[
          { id: "a", name: "Default", connIds: [] },
          { id: "b", name: "Reports", connIds: [] },
        ]}
        current="a"
        counts={{ a: 3, b: 1 }}
        shortcut={(n) => `⌘${n}`}
        onSwitch={noop}
        onNew={noop}
        onRename={noop}
        onDelete={noop}
      />
    </div>
  ),
};
