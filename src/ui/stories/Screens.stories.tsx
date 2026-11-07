import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";
import { expect, fireEvent, userEvent, waitFor, within } from "storybook/test";
// Fake backend (same as the app's ?mock): returns fixed data for the commands screens call
import "../../lib/mock";
import { monaco } from "../../lib/monaco";
import App from "../../App";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { setFont } from "../../lib/prefs";
import { saveCustomTheme, setThemeSync } from "../../lib/theme";
import { dialectOf, type ConnectionView, type QueryResult } from "../../lib/api";
import { ConnectionDialog } from "../../components/ConnectionDialog";
import { ConsolePane, SPLITTER_FAILED, type OutputLine } from "../../components/ConsolePane";
import type { StmtMark } from "../../lib/stmtMark";
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
import { SearchEverywhere } from "../../components/SearchEverywhere";
import { TableEditor } from "../../components/TableEditor";
import { TableStructureView } from "../../components/TableStructure";
import { Switcher } from "../../components/Switcher";
import { BackupDialog, ExportDialog, ImportDialog } from "../../components/Transfer";
import { UpdateDialog } from "../../components/UpdateDialog";
import { WorkspaceBar } from "../../components/WorkspaceBar";
import { StatusBar } from "../../features/settings/StatusBar";
import { useConnActions } from "../../features/conns/useConnActions";
import { ConfirmHost, Menu, Text } from "..";
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
/** "Add Font File…" under the list (not in DataGrip) copies a .ttf / .otf / .woff / .woff2 into Qurry's fonts folder and picks it (the
 * mock adds "Fira Code"). Added fonts come first, marked "custom", whatever "Monospaced only" says; − removes one (asks first; a picker
 * set to it goes back to the default). Here a font is added from the grid font picker, which is then opened again */
export const SettingsFontsCustom: S = {
  name: "Settings — Fonts (added font file)",
  render: () => <SettingsDialog onClose={noop} onUpdate={noop} onChannel={noop} />,
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    const field = await body.findByRole("combobox", { name: "Grid font family" });
    await userEvent.click(field);
    await userEvent.click(await body.findByRole("button", { name: "Add Font File…" }));
    await waitFor(() => expect(field.textContent).toMatch(/Fira Code/));
    await userEvent.click(field);
  },
};

/** Settings › Appearance › Theme with Sync with OS on: a dark and a light theme of the user's choice (here One Dark and Solarized Light),
 *  shared with the TUI, which picks between them by its terminal's background */
export const SettingsThemeSync: S = {
  name: "Settings — Theme: Sync with OS",
  render: () => <SettingsDialog onClose={noop} onUpdate={noop} onChannel={noop} />,
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await setThemeSync({ dark: "one-dark", light: "solarized-light" });
    await userEvent.click(await body.findByRole("checkbox", { name: "Sync with OS" }));
  },
};
/** Settings › Appearance › Theme › Edit Colors…: a custom theme from any built-in (or custom) one. Basic colors, the rest under "Advanced
 *  colors"; a picker and the hex per color, ↺ back to the base's. The whole window draws the colors being edited, the preview shows every
 *  state, and pointing at a color's name outlines what draws it (here Comment, set too dark: its contrast warning shows next to it and in
 *  the header, and it is saved with the warning if asked, never changed). Import… / Export… read and write a small JSON file */
export const SettingsThemeEditor: S = {
  name: "Settings — Theme editor",
  render: () => <SettingsDialog initialPage="theme" onClose={noop} onUpdate={noop} onChannel={noop} />,
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.selectOptions(await body.findByRole("combobox", { name: "Based on" }), "dracula");
    const hex = await body.findByRole("textbox", { name: "Comment hex" });
    await userEvent.clear(hex);
    await userEvent.type(hex, "#3d4053");
    await userEvent.type(await body.findByRole("textbox", { name: "Theme name" }), "Dracula (mine)");
    await userEvent.hover(canvasElement.ownerDocument.querySelector('[data-token="comment"] .theme-token-label')!);
  },
};
/** Quick Switch Theme (⌃`, as DataGrip's): the built-in themes, custom ones (marked), Sync with OS with its pair, and Edit Colors… */
export const QuickSwitchTheme: S = {
  name: "Quick Switch Theme",
  render: () => <App />,
  play: async ({ canvasElement }) => {
    await saveCustomTheme({ id: "", name: "Night shift", base: "dracula", colors: { bg: "#1d1e26", accent: "#9580ff" } }).catch(() => {});
    const body = within(canvasElement.ownerDocument.body);
    await body.findAllByText("[DEV] shop_app");
    fireEvent.keyDown(canvasElement.ownerDocument.body, { key: "`", code: "Backquote", ctrlKey: true });
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

/** The table tab's WHERE field completes like the console (the field as `SELECT * FROM public.users WHERE …`): the table's columns
 * first, then tables and keywords; ↑↓ PgUp PgDn move, ↩ / ⇥ pick, ⎋ closes the list, then resets the field */
/** The mock's public.users columns (what the WHERE field completes against) */
const users: QueryResult = {
  columns: [
    { name: "id", typeName: "INT8" },
    { name: "user_name", typeName: "VARCHAR" },
  ],
  rows: Array.from({ length: 12 }, (_, i) => [i + 1, `user_${i + 1}`]),
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 8,
  hasResultSet: true,
};
export const TableFilterCompletion: S = {
  name: "Table data — WHERE completion",
  render: () => (
    <Frame h={360}>
      <TableEditor
        where=""
        orderBy=""
        running={false}
        error={null}
        result={users}
        query="SELECT * FROM public.users"
        loadingMore={false}
        completion={{ target: "dev", schema: "public", table: "users", db: "shop_app", mysql: false }}
        onApply={noop}
        onCancel={noop}
        onFetchMore={noop}
        onSubmit={async () => {}}
        onExportCsv={noop}
        onDdl={noop}
        onCopyQueryToConsole={noop}
      />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const field = await within(canvasElement).findByRole("combobox", { name: "WHERE" });
    await userEvent.type(field, "id > 1 and u");
  },
};

export const Chart: S = { name: "Result Chart", render: () => <Frame><ResultChart result={sales} /></Frame> };
export const Grid: S = {
  name: "Result Grid",
  render: () => (
    <Frame>
      <ResultGrid result={sales} running={false} loadingMore={false} query="select region, month, orders, revenue from sales" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </Frame>
  ),
};

/** Column widths (#109): drag a header's right edge, double-click it to fit, right-click a header for Fit Column / Fit All Columns /
 * Reset Widths, ⌃⌥← / ⌃⌥→ on the current column. memo is longer than its computed width (cut with …); Fit on receiver_name counts
 * each Hangul character as 2, as the TUI draws it, so the whole name shows */
const orders: QueryResult = {
  columns: [
    { name: "idx", typeName: "INT8" },
    { name: "order_no", typeName: "VARCHAR" },
    { name: "receiver_name", typeName: "VARCHAR" },
    { name: "memo", typeName: "TEXT" },
    { name: "created_at", typeName: "TIMESTAMPTZ" },
  ],
  rows: [
    [486, "O-20261101", "김현교", "first order after the promotion; the customer asked for a call before delivery, then changed the address twice and asked for gift wrap", "2026-11-01 10:10:00 +09:00"],
    [487, "O-20261102", "이수진", null, "2026-11-02 10:11:00 +09:00"],
    [488, "O-20261103", "박민수 (배송 전 전화 요청, 경비실 보관)", "refund requested", "2026-11-03 10:12:00 +09:00"],
    [489, "O-20261104", "최지우", "gift wrap", "2026-11-04 10:13:00 +09:00"],
  ],
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 3,
  hasResultSet: true,
};
export const GridColumnWidths: S = {
  name: "Result Grid — column widths (try it)",
  render: () => (
    <div style={{ position: "relative", height: 220, border: "1px solid var(--ui-border)" }}>
      <ResultGrid result={orders} widthsOf="story-orders" running={false} loadingMore={false} query="select * from orders" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </div>
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

/** Loading more rows (infinite scroll, #94): nothing on the grid (no row is covered); the toolbar's ↻ is ■ (cancels the fetch, the rows stay),
 * the status bar shows it as it shows a run (✕ on hover cancels too) */
export const GridLoadingMore: S = {
  name: "Result Grid — loading more rows",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", height: 284, border: "1px solid var(--ui-border)" }}>
      <div style={{ position: "relative", flex: 1 }}>
        <ResultGrid result={{ ...mixed, truncated: true }} running={false} loadingMore query="select * from payment_check" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
      </div>
      <StatusBar status="" busy={{ label: "Loading more rows", since: Date.now() - 1200, onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} widgets={{ pos: "Row 3, Col 5" }} />
    </div>
  ),
};
/** A load-more that failed (#80): the reason beside the row count (not over the rows), and Run again */
export const GridLoadMoreFailed: S = {
  name: "Result Grid — load more failed",
  render: () => (
    <div style={{ position: "relative", height: 260, border: "1px solid var(--ui-border)" }}>
      <ResultGrid result={{ ...mixed, truncated: true, moreError: "Couldn't load more rows: Session closed" }} running={false} loadingMore={false} query="select * from payment_check" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
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

/** Selection as a list of ranges (DataGrip): a 3×2 range, then ⌘-click row 5 and one cell of row 8; copy and the edits take every range */
export const GridMultiSelect: S = {
  name: "Result Grid — multi-range selection (⌘-click)",
  render: () => (
    <div style={{ position: "relative", height: 320, border: "1px solid var(--ui-border)" }}>
      <ResultGrid result={mixed} edit={{ schema: "public", table: "payment_check", pk: ["card_brand"] }} running={false} loadingMore={false} query="select * from payment_check" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    // One user session, so the held ⇧ / ⌘ reach the clicks
    const user = userEvent.setup();
    await user.click(c.getAllByRole("gridcell", { name: "VISA" })[0]);
    await user.keyboard("{Shift>}");
    await user.click(c.getByRole("gridcell", { name: "387000.00" }));
    await user.keyboard("{/Shift}{Meta>}");
    await user.click(c.getByRole("rowheader", { name: "5" }));
    await user.click(c.getByRole("gridcell", { name: "2026-10-28 09:07:00 +09:00" }));
    await user.keyboard("{/Meta}");
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
/** The toolbar's database picker: [PROD] shop_app's shown databases, preselected on its default */
const dbaDb = { database: "shop_app", loadDbs: async () => ["postgres", "shop_app", "shop_app_test"], fixedDb: false, onDatabase: noop };
/** Picking a database reruns the report there (the mock's Table Sizes names the database it ran on) */
function DbaPicker() {
  const [db, setDb] = useState("shop_app");
  return (
    <DbaReport target={db === "shop_app" ? "prod" : `prod::${db}`} connName="[PROD] shop_app" report="table_sizes" readOnly={false} dialect={dialectOf("postgres")} visible {...dbaDb} database={db} onDatabase={setDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
  );
}
export const Dba: S = {
  name: "Database Admin",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible {...dbaDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const DbaSlowQueries: S = {
  name: "Database Admin: Slow Queries",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="slow_queries" readOnly={false} dialect={dialectOf("postgres")} visible {...dbaDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const DbaUnusedIndexes: S = {
  name: "Database Admin: Unused Indexes",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="unused_indexes" readOnly={false} dialect={dialectOf("postgres")} visible {...dbaDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const DbaDatabase: S = {
  name: "Database Admin: database picker",
  render: () => (
    <Frame h={600}>
      <div style={{ display: "grid", gridTemplateRows: "1fr 1fr 1fr 1fr", height: "100%" }}>
        <DbaPicker />
        {/* MySQL without a default schema: all schemas, or one picked */}
        <DbaReport target="my" connName="[MySQL] shop" report="slow_queries" readOnly={false} dialect={dialectOf("mysql")} visible database="" loadDbs={async () => ["", "shop", "shop_archive"]} fixedDb={false} onDatabase={noop} onReport={noop} onOpenConsole={noop} onStatus={noop} />
        {/* The account can't connect to the default database, so the list can't be read: open the picker for the way out */}
        <DbaReport target="prod" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible database="shop_app" loadDbs={() => Promise.reject('permission denied for database "shop_app"')} fixedDb={false} onDatabase={noop} onReport={noop} onOpenConsole={noop} onStatus={noop} />
        {/* One fixed database: shown, read-only */}
        <DbaReport target="prod" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible database="shop_app" loadDbs={async () => []} fixedDb onDatabase={noop} onReport={noop} onOpenConsole={noop} onStatus={noop} />
      </div>
    </Frame>
  ),
};
export const DbaError: S = {
  name: "Database Admin: report error",
  render: () => (
    <Frame h={600}>
      <div style={{ display: "grid", gridTemplateRows: "1fr 1fr", height: "100%" }}>
        <DbaReport target="busy" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible {...dbaDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
        <DbaReport target="maria" connName="[MariaDB] shop" report="slow_queries" readOnly={false} dialect={dialectOf("mysql")} visible {...dbaDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
      </div>
    </Frame>
  ),
};
export const DbaStale: S = {
  name: "Database Admin: refresh failed",
  render: () => (
    <Frame h={600}>
      <DbaReport target="flaky" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible {...dbaDb} onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const Migrate: S = { name: "Compare & Migrate", render: () => <Frame h={600}><MigrateView conns={CONNS} initialConnId="dev" onOpenConsole={noop} onStatus={noop} /></Frame> };
export const Rag: S = { name: "AI Search (RAG)", render: () => <Frame h={600}><RagView conns={CONNS} runJob={async (_l, run) => run("job")} onStatus={noop} /></Frame> };
/** ⌘↩ at startup while the WebAssembly module loads: shown like a run in the status bar; the toolbar's ■, the ✕ on hover or ⌘F2 drops it */
export const ConsoleSplitterLoading: S = {
  name: "Console — loading the statement splitter",
  render: () => (
    <div style={{ width: 760, border: "1px solid var(--ui-border)" }}>
      <StatusBar status="" busy={{ label: "Loading the statement splitter", since: Date.now(), onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} />
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
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop} registerApi={noop} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  ),
};
/** ⌘↩ inside one of several statements: the Statements popup has the keys; the editor highlights what the highlighted option runs */
export const ConsoleStatementsPopup: S = {
  name: "Console — Statements popup (⌘↩)",
  render: () => (
    <Frame h={420}>
      <ConsolePane
        tabId="story-stmts" target="dev" initialSql={"select * from customers;\nselect 1;\nselect 2;"} connName="[DEV] shop_app" kind="postgres" dialect={dialectOf("postgres")} database="shop_app" schema="public"
        dbList={[]} schemaList={[]} running={false} loadingMore={false} results={[]} activeResult="output"
        output={[]} txManual={false} inTx={false}
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop} registerApi={noop} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    // ⌘↩ through the editor's own action (synthetic key events don't reach Monaco's keybindings)
    const ed = await waitFor(() => monaco.editor.getEditors().find((e) => canvasElement.contains(e.getContainerDomNode())) ?? Promise.reject(new Error("no editor yet")));
    ed.focus();
    ed.trigger("story", "dbtool.run", null);
    const chooser = await waitFor(() => canvasElement.querySelector<HTMLElement>(".stmt-chooser") ?? Promise.reject(new Error("no popup yet")), { timeout: 10000 });
    fireEvent.keyDown(chooser, { key: "ArrowDown" });
    await waitFor(() => expect(canvasElement.querySelector(".stmt-preview")).not.toBeNull(), { timeout: 10000 });
  },
};
/** The shared completion engine in the console (mock metadata: orders.user_id → users.id, the user function order_total): an
 * unknown column underlined (⌥↩ lists the closest names) and signature help inside a call's parentheses */
export const ConsoleAssist: S = {
  name: "Console — unknown name and signature help",
  render: () => (
    <Frame h={420}>
      <ConsolePane
        tabId="story-assist" target="dev" initialSql={"select nme, order_total(id, ) from orders o\njoin users u on o.user_id = u.id;"} connName="[DEV] shop_app" kind="postgres" dialect={dialectOf("postgres")} database="shop_app" schema="public"
        dbList={[]} schemaList={[]} running={false} loadingMore={false} results={[]} activeResult="output"
        output={[]} txManual={false} inTx={false}
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop} registerApi={noop} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const ed = await waitFor(() => monaco.editor.getEditors().find((e) => canvasElement.contains(e.getContainerDomNode())) ?? Promise.reject(new Error("no editor yet")));
    await waitFor(() => expect(canvasElement.querySelector(".squiggly-warning")).not.toBeNull(), { timeout: 10000 });
    ed.focus();
    // Inside order_total( id, ‸): its second parameter
    ed.setPosition({ lineNumber: 1, column: "select nme, order_total(id, ".length + 1 });
    ed.trigger("story", "editor.action.triggerParameterHints", null);
    await waitFor(() => expect(document.querySelector(".parameter-hints-widget .parameter.active")?.textContent).toBe("with_tax boolean"), { timeout: 10000 });
  },
};
/** A script's Output longer than the pane: it follows the newest line while scrolled to the bottom; scrolled up, it stays put */
export const ConsoleLongOutput: S = {
  name: "Console — long Output",
  render: () => (
    <Frame h={420}>
      <ConsolePane
        tabId="story" target="dev" initialSql={"update orders set status = 'paid' where id = 1;"} connName="[DEV] shop_app" kind="postgres" dialect={dialectOf("postgres")} database="shop_app" schema="public"
        dbList={[]} schemaList={[]} running={false} loadingMore={false} results={[]} activeResult="output"
        output={[
          ...Array.from({ length: 40 }, (_, i) => [
            { ts: "10:24:01", text: `[DEV] shop_app> update orders set status = 'paid' where id = ${i + 1}`, level: "sql" as const },
            { ts: "10:24:01", text: "1 row affected in 2 ms", level: "info" as const },
          ]).flat(),
          { ts: "10:24:02", text: `[DEV] shop_app> insert into order_audit (order_id, payload) values (41, '{${Array.from({ length: 12 }, (_, k) => `"field_${k + 1}": "value ${k + 1}"`).join(", ")}}')`, level: "sql" },
          { ts: "10:24:02", text: "ERROR: value too long for type character varying(255)", level: "error" },
        ]}
        txManual={false} inTx={false}
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop} registerApi={noop} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  ),
};
/** A console result from one table with its primary key (QueryResult.edit): editable as in a table tab, here with one pending edit */
const consoleUsers: QueryResult = {
  columns: [{ name: "id", typeName: "INT8" }, { name: "name", typeName: "VARCHAR" }, { name: "email", typeName: "TEXT" }],
  rows: [[1, "kim", "kim@example.com"], [2, "lee", null], [3, "park", "park@example.com"]],
  rowsAffected: 0, truncated: false, elapsedMs: 4, hasResultSet: true,
  edit: { schema: "public", table: "users", pk: ["id"], columns: ["id", "user_name", "email"] },
};
const consoleProps = {
  tabId: "story", target: "dev", connName: "[DEV] shop_app", kind: "postgres" as const, dialect: dialectOf("postgres"), database: "shop_app", schema: "public",
  dbList: [], schemaList: [], running: false, loadingMore: false, output: [], txManual: false, inTx: false,
  onChange: noop, onExecute: noop, onCancel: noop, onSetActiveResult: noop, onPin: noop, onCloseResult: noop, onFetchMore: noop, onSubmit: async () => {}, onExport: noop, onHistory: noop,
  onTxMode: noop, onCommit: noop, onRollback: noop, onLoadDbs: noop, onLoadSchemas: noop, onSwitchDb: noop, onSwitchSchema: noop, registerApi: noop, onClearOutput: noop, onError: noop, onLoadingChange: noop,
};
/** One toggling button per toolbar (#107): the console's ▶ is ■ in the same spot while it runs a statement (not for a result's
 * load-more); a grid's ↻ is ■ during each of its loads (a run, the load-more on scroll), and that ■ cancels only the fetch in flight */
function RunStopToolbars() {
  const label = (t: string) => <Text size="sm" tone="muted">{t}</Text>;
  // The grid fills a positioned box (as in a tab); a console needs room for its toolbar and a line of its editor
  const box = (h: number, child: React.ReactNode) => <div style={{ position: "relative", height: h, border: "1px solid var(--ui-border)" }}>{child}</div>;
  const grid = (running: boolean, loadingMore: boolean) =>
    box(110, <ResultGrid result={{ ...mixed, truncated: true }} running={running} loadingMore={loadingMore} query="select * from payment_check" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, padding: 12 }}>
      {label("Console — idle: ▶ Execute (⌘↩)")}
      {box(130, <ConsolePane {...consoleProps} tabId="story-idle" initialSql="select * from orders;" activeResult="output" results={[]} />)}
      {label("Console — running: ■ Cancel (⌘F2) in the same spot")}
      {box(130, <ConsolePane {...consoleProps} tabId="story-running" initialSql="select pg_sleep(5);" activeResult="output" results={[]} running />)}
      {label("Grid — idle: ↻ Reload Page (⌘r)")}
      {grid(false, false)}
      {label("Grid — loading (a run): ■ Cancel (⌘F2)")}
      {grid(true, false)}
      {label("Grid — loading more on scroll: ■ cancels the fetch, the rows stay")}
      {grid(false, true)}
    </div>
  );
}
export const ToolbarsRunStop: S = { name: "Toolbars — run and stop", render: () => <RunStopToolbars /> };
export const ToolbarsRunStopLight: S = { name: "Toolbars — run and stop, light", globals: { theme: "light" }, render: () => <RunStopToolbars /> };
export const ConsoleEditableResult: S = {
  name: "Console — editable result (pending edit)",
  render: () => (
    <Frame h={420}>
      <ConsolePane {...consoleProps} initialSql="select id, user_name as name, email from users;" activeResult="r1"
        results={[{ id: "r1", title: "Result 1", sql: "select id, user_name as name, email from users", kind: "result", result: consoleUsers, pinned: false }]} />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    // After the editor took focus on mount: edit in the cell's own input
    await waitFor(() => {
      if (!canvasElement.querySelector(".monaco-editor .view-lines")) throw new Error("editor not mounted yet");
    });
    await userEvent.dblClick(await c.findByText("lee"));
    const input = await c.findByDisplayValue("lee");
    await userEvent.clear(input);
    await userEvent.type(input, "Lee{Enter}");
  },
};
/** A join: read-only, the greyed-out edit buttons say why (QueryResult.readOnlyReason) */
export const ConsoleReadOnlyResult: S = {
  name: "Console — read-only result (reason)",
  render: () => {
    const { edit: _, ...joined } = consoleUsers;
    const sql = "select u.id, u.name, o.total from users u join orders o on o.user_id = u.id";
    return (
      <Frame h={420}>
        <ConsolePane {...consoleProps} initialSql={sql + ";"} activeResult="r1"
          results={[{ id: "r1", title: "Result 1", sql, kind: "result", result: { ...joined, readOnlyReason: "Read-only: the columns come from more than one table." }, pinned: false }]} />
      </Frame>
    );
  },
};
/** A console after its runs: each executed statement is marked on its first line (✓ done, ✗ failed, spinning while it runs; all three
 * shown together here). Hover a mark for when it ran and its result or error; click it for its line in Output */
function ConsoleMarks() {
  const [output, setOutput] = useState<OutputLine[]>([]);
  const [active, setActive] = useState("output");
  // The story runs the whole console once (the user's Query Execution setting would ask which statement); the setting is put back right away
  const [saved] = useState(() => {
    const v = localStorage.getItem("dbtool.execSettings");
    localStorage.setItem("dbtool.execSettings", JSON.stringify({ inside: "whole" }));
    return v;
  });
  useEffect(() => {
    if (saved === null) localStorage.removeItem("dbtool.execSettings");
    else localStorage.setItem("dbtool.execSettings", saved);
  }, [saved]);
  const runs: [StmtMark["state"], string][] = [["ok", "1 row retrieved starting from 1 in 4 ms"], ["error", 'ERROR: column "status" of relation "payment" does not exist'], ["running", ""]];
  return (
    <Frame h={420}>
      <ConsolePane
        tabId="story-marks" target="dev" initialSql={"select 1;\nupdate payment\n   set status = 'paid'\n where id = 42;\nselect pg_sleep(5);"} connName="[DEV] shop_app" kind="postgres" dialect={dialectOf("postgres")} database="shop_app" schema="public"
        dbList={[]} schemaList={[]} running loadingMore={false} results={[]} activeResult={active}
        output={output} txManual={false} inTx={false}
        onChange={noop}
        onExecute={(stmts, _explain, mark) => {
          const ts = "2026-11-04 10:24:01";
          setOutput(stmts.flatMap((s, i) => {
            const [state, text] = runs[i];
            mark?.(i, { state, out: `o${i}`, ts, text });
            return [{ ts, text: `[DEV] shop_app> ${s}`, level: "sql" as const, stmt: `o${i}` }, ...(text ? [{ ts, text, level: state === "ok" ? ("info" as const) : ("error" as const) }] : [])];
          }));
        }}
        onCancel={noop} onSetActiveResult={setActive} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop}
        registerApi={(_id, a) => setTimeout(() => a.run())} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  );
}
export const ConsoleGutterMarks: S = { name: "Console — gutter marks", render: () => <ConsoleMarks /> };
/** One Redis tab per data source (#108): the database picker (db0 · 7 keys) and key filter on top, folder counts beside the name, TTLs only for keys */
const RedisTab = () => (
  <Frame h={600}>
    <RedisBrowser target="rd" connId="rd" connName="[Redis] cache" database="db0" readOnly={false} production={false} onSwitchDb={noop} onStatus={noop} />
  </Frame>
);
export const Redis: S = { name: "Redis Browser", render: RedisTab };
/** Managed Redis (ElastiCache, Memorystore …) refuses CONFIG: the Config tab is greyed out; hover it for why and where the settings are */
export const RedisManaged: S = {
  name: "Redis Browser — managed Redis (Config greyed out)",
  beforeEach: () => {
    (globalThis as any).__mockRedis = { managed: true };
    return () => delete (globalThis as any).__mockRedis;
  },
  render: RedisTab,
};
/** Terminal: a hash as a field / value table, HELP from the bundled command table, completion with argument hints */
export const RedisTerminal: S = {
  name: "Redis Browser — Terminal",
  render: RedisTab,
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("tab", { name: "Terminal" }));
    const input = await c.findByRole("textbox", { name: "Redis command" });
    await userEvent.type(input, "HGETALL user:1{Escape}{Enter}");
    await userEvent.type(input, "HELP HGET{Enter}");
    await userEvent.type(input, "HG");
  },
};
/** Terminal key names from a bounded SCAN: the key typed out comes first, and ↩ runs the line unless ↑↓ moved to another key (⇥ picks) */
export const RedisTerminalKeys: S = {
  name: "Redis Browser — Terminal key names",
  render: RedisTab,
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("tab", { name: "Terminal" }));
    await userEvent.type(await c.findByRole("textbox", { name: "Redis command" }), "HGETALL user:1");
  },
};
const WS_NAMES = ["Default", "Reports", "Billing", "Analytics", "Ops", "Support", "Growth", "Search", "Mobile", "Payments"];
/** The rail at a fixed window height: n workspaces, the 2nd current. Keymap and Settings stay at the bottom while the list scrolls */
const Rail = ({ n, h = 420 }: { n: number; h?: number }) => {
  const list = Array.from({ length: n }, (_, i) => ({ id: `w${i}`, name: i < WS_NAMES.length ? WS_NAMES[i] : `Workspace ${i + 1}`, connIds: [] }));
  return (
    <div style={{ height: h, display: "flex", border: "1px solid var(--ui-border)", width: "fit-content" }}>
      <WorkspaceBar
        list={list}
        current="w1"
        counts={Object.fromEntries(list.map((w, i) => [w.id, i % 4]))}
        shortcut={(n) => `⌘${n}`}
        onSwitch={noop}
        onNew={noop}
        onRename={noop}
        onDelete={noop}
        onKeymap={noop}
        onSettings={noop}
      />
    </div>
  );
};
export const Workspaces: S = { name: "Workspace rail", render: () => <Rail n={2} /> };
export const WorkspacesMany: S = { name: "Workspace rail — 30 workspaces (list scrolls, footer stays)", render: () => <Rail n={30} /> };
export const WorkspacesLight: S = { name: "Workspace rail — light", globals: { theme: "light" }, render: () => <Rail n={30} /> };
export const WorkspacesHc: S = { name: "Workspace rail — high contrast", globals: { theme: "hc" }, render: () => <Rail n={30} /> };
/** Status bar, information only (#107): status text, transfers, update badge, read-only, the focused grid's shown columns, the focused
 * editor's (Ln, Col) or grid's (Row, Col) position, encoding. While the focused tab runs or loads more rows, the running indicator
 * (bar · what · elapsed) takes the status text's place; hovering it shows ✕, which cancels (shown hovered in the last two) */
export const StatusBarStory: S = {
  name: "Status bar",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <style>{".sb-hovered .rb-cancel { visibility: visible; }"}</style>
      <StatusBar status="1 statement executed in 12 ms" readOnly update="0.3.1" onUpdate={noop} jobs={[]} widgets={{ pos: "Ln 2, Col 11" }} />
      <StatusBar status="1 statement executed in 12 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ pos: "Ln 2, Col 11 (8 chars)" }} />
      <StatusBar status="3000+ rows retrieved" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ cols: "cols 3–8 of 14", pos: "Row 3, Col 5" }} />
      <StatusBar status="Table users opened" readOnly={false} onUpdate={noop} jobs={[]} />
      <div className="sb-hovered">
        <StatusBar status="" busy={{ label: "Executing", since: Date.now() - 3400, onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} widgets={{ pos: "Ln 2, Col 11" }} />
      </div>
      <div className="sb-hovered">
        <StatusBar status="" busy={{ label: "Loading more rows", since: Date.now() - 800, onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} widgets={{ cols: "cols 3–8 of 14", pos: "Row 3, Col 5" }} />
      </div>
    </div>
  ),
};
export const StatusBarLight: S = { ...StatusBarStory, name: "Status bar — light", globals: { theme: "light" } };

// ───────── Import Data Sources (#63): the real flow on the mock backend ─────────
/** Explorer ⋮ › Import Data Sources ▸ (opened by hovering it): the locations found, newest IDE version first, the count, the path as the
 * tooltip; then File… and Folder… (the native pickers pick one or the other) */
function ImportMenu() {
  const a = useConnActions({ conns: CONNS, refresh: async () => {}, setStatus: noop });
  return (
    <div style={{ position: "relative", height: 260, width: 640 }}>
      <span className="menu-anchor" style={{ position: "absolute", left: 220, top: 8 }}>
        <Menu items={[{ label: "Export Data Sources…", action: noop }, { label: "Import Data Sources", submenu: a.importItems() }]} onClose={noop} />
      </span>
    </div>
  );
}
export const ImportSubmenu: S = {
  name: "Import Data Sources ▸ (explorer ⋮)",
  render: () => <ImportMenu />,
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await waitFor(async () => {
      await userEvent.hover(c.getByText("Import Data Sources"));
      await expect(c.getByText("DataGrip 2025.2 · global  (7)")).toBeTruthy();
    });
  },
};

/** The import confirm: one row per source, a checkbox on each added row (unchecked = left out), skipped rows with the reason;
 * Import N counts the checked rows and is disabled at 0 */
function ImportFlow({ from }: { from: { path: string } | { folder: boolean } }) {
  const a = useConnActions({ conns: CONNS, refresh: async () => {}, setStatus: noop });
  useEffect(() => void a.importFrom(from), []);
  return <ConfirmHost />;
}
const DG = "/Users/me/Library/Application Support/JetBrains/DataGrip2025.2/options";
export const ImportPreview: S = { name: "Import Data Sources — preview with checkboxes", render: () => <ImportFlow from={{ path: DG }} /> };
export const ImportPreviewLight: S = { name: "Import Data Sources — preview (light)", globals: { theme: "light" }, render: () => <ImportFlow from={{ path: DG }} /> };
/** Every source skipped: a notice (OK only) with the rows */
export const ImportNothing: S = {
  name: "Import Data Sources — nothing importable (notice)",
  render: () => {
    (window as any).__mockImportAllSkipped = true;
    useEffect(() => () => void ((window as any).__mockImportAllSkipped = false), []);
    return <ImportFlow from={{ path: DG }} />;
  },
};
/** A file that can't be parsed: where, and that nothing was imported */
export const ImportError: S = {
  name: "Import Data Sources — a file that can't be read",
  render: () => {
    (window as any).__mockImportPick = "broken";
    useEffect(() => () => void ((window as any).__mockImportPick = undefined), []);
    return <ImportFlow from={{ folder: false }} />;
  },
};
