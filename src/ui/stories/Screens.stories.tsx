import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";
import { expect, fireEvent, userEvent, waitFor, within } from "storybook/test";
// Fake backend (same as the app's ?mock): returns fixed data for the commands screens call
import "../../lib/mock";
import { monaco } from "../../lib/monaco";
import App from "../../App";
import { ResultTabsField, SettingsDialog } from "../../features/settings/SettingsDialog";
import { setFont } from "../../lib/prefs";
import { saveCustomTheme, setThemeSync } from "../../lib/theme";
import { dialectOf, type ConnectionView, type QueryResult } from "../../lib/api";
import type { GridStatus, StatusItemId } from "../../lib/shared";
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
import { ConfirmHost, Menu, TabItem, TabStrip, Text, Toasts } from "..";
import { ReconnectToastView } from "../../components/ReconnectToasts";
import type { ReconnectToast } from "../../lib/reconnect";
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

/** Settings › Result tabs (#151): unpinned result tabs kept per console, 10 by default (shared with the TUI's ⌃k › Result Tabs…); what it
 *  does is in its ⓘ. A value that isn't 1 or more says so and isn't saved */
const ResultTabsForm = () => (
  <div className="settings-form" style={{ width: 520 }}>
    <ResultTabsField />
  </div>
);
export const SettingsResultTabs: S = { name: "Settings — Result tabs", render: () => <ResultTabsForm /> };
export const SettingsResultTabsInvalid: S = {
  name: "Settings — Result tabs: 0",
  render: () => <ResultTabsForm />,
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const field = await c.findByRole("spinbutton", { name: "Result tabs" });
    await userEvent.type(field, "{backspace}{backspace}0");
    await c.findByText("Enter 1 or more.");
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
/** A new data source's Schemas tab lists its databases with the form's settings before it is saved (as Test Connection connects) */
export const NewDataSourceSchemas: S = {
  name: "Data Source — new, Schemas tab",
  render: () => <ConnectionDialog onClose={noop} onSaved={noop} />,
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(body.getByRole("tab", { name: /^Schemas/ }));
    await userEvent.click((await body.findByText("shop_app_test")).closest("label")!);
  },
};
/** …while it connects through an AWS SSM instance that doesn't answer: the elapsed time and Cancel Listing (Esc) */
export const NewDataSourceSchemasConnecting: S = {
  name: "Data Source — new, Schemas tab connecting",
  render: () => <ConnectionDialog onClose={noop} onSaved={noop} />,
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(body.getByRole("tab", { name: /SSH\/SSL/ }));
    await userEvent.selectOptions(body.getByRole("combobox", { name: "Tunnel" }), "ssm");
    await userEvent.type(body.getByRole("textbox", { name: "Instance id" }), "i-00000000000000000");
    await userEvent.click(body.getByRole("tab", { name: /^Schemas/ }));
  },
};
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

/** Options › MCP: whether AI tools may read the admin reports (dba_report) of a Production source, off by default; elsewhere the
 * row is disabled and says why (not Production: checked, as they always may; MCP Off: AI tools don't see the source) */
const adminReports = (name: string, theme: "dark" | "light", c: ConnectionView): S => ({
  name: `Data Source — admin reports for AI tools, ${name} (${theme})`,
  globals: { theme },
  render: () => <ConnectionDialog edit={c} onClose={noop} onSaved={noop} />,
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement.ownerDocument.body).getByRole("tab", { name: "Options" }));
  },
});
const withOptions = (c: ConnectionView, options: ConnectionView["options"]) => ({ ...c, options: { ...c.options, ...options } }) as ConnectionView;
const REPORTS_OFF = withOptions(PROD, { mcp: "read" });
const REPORTS_ON = withOptions(PROD, { mcp: "read", mcpDbaReports: true });
const REPORTS_DEV = withOptions(DEV, { mcp: "read" });
const REPORTS_MCP_OFF = withOptions(PROD, { mcp: "off" });
export const AdminReportsOffDark = adminReports("Production, off", "dark", REPORTS_OFF);
export const AdminReportsOffLight = adminReports("Production, off", "light", REPORTS_OFF);
export const AdminReportsOnDark = adminReports("Production, on", "dark", REPORTS_ON);
export const AdminReportsOnLight = adminReports("Production, on", "light", REPORTS_ON);
export const AdminReportsNotProductionDark = adminReports("not Production", "dark", REPORTS_DEV);
export const AdminReportsNotProductionLight = adminReports("not Production", "light", REPORTS_DEV);
export const AdminReportsMcpOffDark = adminReports("MCP Off", "dark", REPORTS_MCP_OFF);
export const AdminReportsMcpOffLight = adminReports("MCP Off", "light", REPORTS_MCP_OFF);

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

/** The WHERE field pairs quotes and brackets as the console does (and the TUI's w / o inputs): typed `name in ('a` reads `name in ('a')` */
export const TableFilterPairs: S = {
  name: "Table data — WHERE quotes and brackets",
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
        dialect={dialectOf("postgres")}
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
    const field = await within(canvasElement).findByRole<HTMLInputElement>("combobox", { name: "WHERE" });
    await userEvent.type(field, "user_name in ('a', 'b");
    await waitFor(() => expect(field.value).toBe("user_name in ('a', 'b')"));
  },
};

/** #136: the WHERE / ORDER BY fields drawn in the console's colors (qurry_shared::highlight): keywords — word operators IN, IS, NOT,
 * NULL, AND, OR, LIKE, BETWEEN too — functions, numbers, strings and comments; `=`, `<` and names in the text color */
const ColoredFilter = (p: { where: string; orderBy: string }) => (
  <Frame h={300}>
    <TableEditor
      where={p.where}
      orderBy={p.orderBy}
      running={false}
      error={null}
      result={users}
      query="SELECT * FROM public.users"
      loadingMore={false}
      dialect={dialectOf("postgres")}
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
);
const filterField = async (c: HTMLElement, kw: string) => within(c).findByRole<HTMLInputElement>("combobox", { name: kw });
/** [name, WHERE, ORDER BY] */
type FilterCase = readonly [string, string, string];
const filterStory = (theme: "dark" | "light", [name, where, orderBy]: FilterCase, play?: S["play"]): S => ({
  name: `Table data — colors: ${name} (${theme})`,
  globals: { theme },
  render: () => <ColoredFilter where={where} orderBy={orderBy} />,
  play,
});
const inList: FilterCase = ["IN with numbers", "id IN (366, 3934)", ""];
const strings: FilterCase = ["strings and a comment", "user_name = 'paid' AND user_name LIKE '%@example.com' -- note", ""];
const isNotNull: FilterCase = ["IS NOT NULL", "user_name IS NOT NULL AND id > 100", ""];
const functions: FilterCase = ["functions", "lower(user_name) = 'a@b.com' AND id BETWEEN 1 AND 10", "coalesce(user_name, 'x')"];
const orderBy: FilterCase = ["ORDER BY … DESC NULLS LAST", "", "user_name DESC NULLS LAST, id"];
const completion: FilterCase = ["completion list open", "", ""];
const typeInWhere: S["play"] = async ({ canvasElement }) => userEvent.type(await filterField(canvasElement, "WHERE"), "id IN (1) AND u");
const caretText: FilterCase = ["caret mid-text, a selection", "id IN (366, 3934) AND user_name = 'paid'", "id DESC"];
const caretMid: S["play"] = async ({ canvasElement }) => {
  const w = await filterField(canvasElement, "WHERE");
  w.focus();
  w.setSelectionRange(3, 5);
};
export const FilterColorsInDark = filterStory("dark", inList);
export const FilterColorsInLight = filterStory("light", inList);
export const FilterColorsStringsDark = filterStory("dark", strings);
export const FilterColorsStringsLight = filterStory("light", strings);
export const FilterColorsIsNotNullDark = filterStory("dark", isNotNull);
export const FilterColorsIsNotNullLight = filterStory("light", isNotNull);
export const FilterColorsFunctionsDark = filterStory("dark", functions);
export const FilterColorsFunctionsLight = filterStory("light", functions);
export const FilterColorsOrderByDark = filterStory("dark", orderBy);
export const FilterColorsOrderByLight = filterStory("light", orderBy);
export const FilterColorsCompletionDark = filterStory("dark", completion, typeInWhere);
export const FilterColorsCompletionLight = filterStory("light", completion, typeInWhere);
export const FilterColorsCaretDark = filterStory("dark", caretText, caretMid);
export const FilterColorsCaretLight = filterStory("light", caretText, caretMid);

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
      <StatusBar status="" busy={{ label: "Loading more rows", since: Date.now() - 1200, onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: CELL_3_5 }} />
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
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onCaret={noop} onStatements={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
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
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onCaret={noop} onStatements={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
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
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onCaret={noop} onStatements={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
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
/** A bracket left open: an error marker from it to the end of its line (hover: "Unclosed '('"), as the TUI underlines it */
export const ConsoleUnclosedBracket: S = {
  name: "Console — unclosed bracket",
  render: () => (
    <Frame h={420}>
      <ConsolePane
        tabId="story-unclosed" target="dev" initialSql={"select user_id, count(*\n  from payment\n where card_brand in ('VISA', 'MASTER')\n group by 1;"} connName="[DEV] shop_app" kind="postgres" dialect={dialectOf("postgres")} database="shop_app" schema="public"
        dbList={[]} schemaList={[]} running={false} loadingMore={false} results={[]} activeResult="output"
        output={[]} txManual={false} inTx={false}
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onCaret={noop} onStatements={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
        onTxMode={noop} onCommit={noop} onRollback={noop} onLoadDbs={noop} onLoadSchemas={noop} onSwitchDb={noop} onSwitchSchema={noop} registerApi={noop} onClearOutput={noop} onError={noop} onLoadingChange={noop}
      />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector(".squiggly-error")).not.toBeNull(), { timeout: 10000 });
  },
};
/** The story's console editor (Monaco), once it is there */
const storyEditor = (canvasElement: HTMLElement) => waitFor(() => monaco.editor.getEditors().find((e) => canvasElement.contains(e.getContainerDomNode())) ?? Promise.reject(new Error("no editor yet")));
/** ⌘↩ inside a subquery (DataGrip): the popup offers it after the whole statement, innermost first; the editor highlights exactly it */
export const ConsoleRunSubquery: S = {
  name: "Console — run a subquery (⌘↩)",
  render: () => (
    <Frame h={420}>
      <ConsolePane {...consoleProps} tabId="story-subquery" results={[]} activeResult="output" initialSql={"select 1;\nselect * from orders\nwhere user_id in (select id from users where n > (select avg(n) from users));"} />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const ed = await storyEditor(canvasElement);
    ed.focus();
    ed.setPosition({ lineNumber: 3, column: 60 });
    ed.trigger("story", "dbtool.run", null);
    const chooser = await waitFor(() => canvasElement.querySelector<HTMLElement>(".stmt-chooser") ?? Promise.reject(new Error("no popup yet")), { timeout: 10000 });
    fireEvent.keyDown(chooser, { key: "ArrowDown" });
    await waitFor(() => expect(canvasElement.querySelector(".stmt-preview")).not.toBeNull(), { timeout: 10000 });
  },
};
/** A failed statement after the first: its error marked where the server points (its own marker, the message on hover with the
 * way out); Output keeps it, an edit or the next run clears the mark */
export const ConsoleRunError: S = {
  name: "Console — run error marked",
  render: () => (
    <Frame h={420}>
      <ConsolePane
        {...consoleProps} tabId="story-run-error" results={[]} activeResult="output" initialSql={"select 1;\nselect id, n\n  from no_such_table\n where n > 1;"}
        output={[
          { ts: "10:24:01", text: "[DEV] shop_app> select id, n\n  from no_such_table\n where n > 1", level: "sql" },
          { ts: "10:24:01", text: 'ERROR: relation "no_such_table" does not exist (position 21)', level: "error" },
        ]}
        onExecute={(_stmts, _explain, mark) => mark?.(0, { state: "error", out: "story", ts: "2026-11-07 10:24:01", text: 'ERROR: relation "no_such_table" does not exist', at: 20 })}
      />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    // Run the second statement (selected, so it runs without asking); the story's onExecute fails it at PostgreSQL's position 21
    const ed = await storyEditor(canvasElement);
    ed.focus();
    ed.setSelection(new monaco.Range(2, 1, 4, 13));
    ed.trigger("story", "dbtool.run", null);
    await waitFor(() => expect(canvasElement.querySelector(".squiggly-error")).not.toBeNull(), { timeout: 10000 });
    ed.setPosition({ lineNumber: 3, column: 8 });
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
        onChange={noop} onExecute={noop} onCancel={noop} onSetActiveResult={noop} onCaret={noop} onStatements={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
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
  onChange: noop, onExecute: noop, onCancel: noop, onSetActiveResult: noop, onCaret: noop, onStatements: noop, onPin: noop, onCloseResult: noop, onFetchMore: noop, onSubmit: async () => {}, onExport: noop, onHistory: noop,
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
        results={[{ id: "r1", stmt: 0, num: 1, used: 1, title: "users", sql: "select id, user_name as name, email from users", kind: "result", result: consoleUsers, pinned: false }]} />
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
/** #151: one result tab per statement, numbered by its place in the script ("2: payments"); the caret entering a statement shows its
 *  tab, a rerun replaces it, deleting the statement closes it. Statement 1's first result was pinned (📌): it no longer follows, and
 *  running statement 1 again opened the tab after it. Settings › Result tabs caps the unpinned ones */
const resultTabsPayments: QueryResult = {
  columns: [{ name: "id", typeName: "INT8" }, { name: "order_id", typeName: "INT8" }, { name: "method", typeName: "VARCHAR" }, { name: "amount", typeName: "NUMERIC" }],
  rows: Array.from({ length: 5 }, (_, i) => [i + 1, 1000 + i, ["card", "bank", "point"][i % 3], ((i * 53) % 400) + 10]),
  rowsAffected: 0, truncated: false, elapsedMs: 3, hasResultSet: true,
};
const resultTabsSql = "select id, user_name as name, email from users";
const ConsoleResultTabsScreen = () => (
  <Frame h={420}>
    <ConsolePane {...consoleProps} tabId="story-result-tabs" initialSql={`${resultTabsSql};\nselect * from payments limit 5;`} activeResult="r3"
      output={[
        { ts: "10:24:01", level: "sql", text: `[DEV] shop_app> ${resultTabsSql}` },
        { ts: "10:24:01", level: "info", text: "3 rows retrieved starting from 1 in 4 ms" },
        { ts: "10:24:09", level: "sql", text: "[DEV] shop_app> select * from payments limit 5" },
        { ts: "10:24:09", level: "info", text: "5 rows retrieved starting from 1 in 3 ms" },
      ]}
      results={[
        { id: "r1", stmt: null, num: 1, used: 1, title: "users", sql: resultTabsSql, kind: "result", result: consoleUsers, pinned: true },
        { id: "r2", stmt: 0, num: 1, used: 2, title: "users", sql: resultTabsSql, kind: "result", result: { ...consoleUsers, rows: consoleUsers.rows.slice(0, 2) }, pinned: false },
        { id: "r3", stmt: 1, num: 2, used: 3, title: "payments", sql: "select * from payments limit 5", kind: "result", result: resultTabsPayments, pinned: false },
      ]} />
  </Frame>
);
export const ConsoleResultTabs: S = { name: "Console — result tabs", render: () => <ConsoleResultTabsScreen /> };
export const ConsoleResultTabsLight: S = { name: "Console — result tabs, light", globals: { theme: "light" }, render: () => <ConsoleResultTabsScreen /> };
/** Read-only: the greyed-out edit buttons say why (QueryResult.readOnlyReason; here the key isn't in the result) */
export const ConsoleReadOnlyResult: S = {
  name: "Console — read-only result (reason)",
  render: () => {
    const { edit: _, ...rest } = consoleUsers;
    const sql = "select name, email from users";
    return (
      <Frame h={420}>
        <ConsolePane {...consoleProps} initialSql={sql + ";"} activeResult="r1"
          results={[{ id: "r1", stmt: 0, num: 1, used: 1, title: "users", sql, kind: "result", result: { ...rest, readOnlyReason: "Read-only: the result doesn't include the primary key of public.users (id)." }, pinned: false }]} />
      </Frame>
    );
  },
};
/** #102's join (QueryResult.editJoin): each table's cells by its own key, one users edit pending. idx and status come twice, each
 *  from its table; the computed column is dimmed with 🔒 and says why on hover; voc 2 has no user, so that side's cells say why on
 *  hover; Add / Delete Row are greyed out (rows aren't one table's) */
const vocJoin: QueryResult = {
  columns: ["idx", "user_idx", "status", "idx", "role", "status", "shout"].map((name, i) => ({ name, typeName: [0, 1, 3].includes(i) ? "INT8" : "VARCHAR" })),
  rows: [[1, 10, "open", 10, "member", "active", "MEMBER"], [2, 99, "new", null, null, null, null], [3, 11, "closed", 11, "admin", "active", "ADMIN"]],
  rowsAffected: 0, truncated: false, elapsedMs: 6, hasResultSet: true,
  editJoin: {
    tables: [{ schema: "crm", table: "voc", pk: ["idx"], pkAt: [0] }, { schema: "public", table: "users", pk: ["idx"], pkAt: [3] }],
    columns: [{ table: 0, column: "idx" }, { table: 0, column: "user_idx" }, { table: 0, column: "status" }, { table: 1, column: "idx" }, { table: 1, column: "role" }, { table: 1, column: "status" }, { readOnly: "Read-only: shout is computed, not a column of the table." }],
  },
};
const vocSql = "select vo.*, pu.idx, pu.role, pu.status, upper(pu.role) as shout from crm.voc vo left join users pu on vo.user_idx = pu.idx";
export const ConsoleJoinResult: S = {
  name: "Console — join result, edited per table",
  render: () => (
    <Frame h={420}>
      <ConsolePane {...consoleProps} initialSql={vocSql + ";"} activeResult="r1" results={[{ id: "r1", stmt: 0, num: 1, used: 1, title: "crm.voc", sql: vocSql, kind: "result", result: vocJoin, pinned: false }]} />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await waitFor(() => {
      if (!canvasElement.querySelector(".monaco-editor .view-lines")) throw new Error("editor not mounted yet");
    });
    await userEvent.dblClick(await c.findByText("member"));
    const input = await c.findByDisplayValue("member");
    await userEvent.clear(input);
    await userEvent.type(input, "admin{Enter}");
  },
};
/** A Production console (#106): it starts in Tx: Manual and says so once in Output; a change ran, so the transaction is pending (the
 *  red badge beside Commit / Roll back); left unused, the backend's reminder came (Output, the badge's "idle" and its hover) */
const prodReminder = 'A transaction on Production "[PROD] shop_app" has been open for 5 min without a statement: its changes aren\'t saved and the rows it changed stay locked. Commit or Roll back. The server ends it after 10 min idle (idle_in_transaction_session_timeout), and rolls it back.';
export const ConsoleProductionTransaction: S = {
  name: "Console — Production transaction pending, idle reminder",
  render: () => (
    <Frame h={420}>
      <ConsolePane {...consoleProps} connName="[PROD] shop_app" production txManual inTx txIdle={prodReminder} initialSql="update users set role = 'admin' where idx = 10;" activeResult="output" results={[]}
        output={[
          { ts: "2026-11-07 10:00:00", level: "info", text: "Production: this console starts in Tx: Manual, so changes stay pending until you Commit or Roll back. To start them in Auto, turn off \"Production: start consoles in Manual transaction mode\" for this data source (GUI: Data Source Properties › Options · TUI: the data source form, \"Production Manual\")." },
          { ts: "2026-11-07 10:00:12", level: "sql", text: "[PROD] shop_app> update users set role = 'admin' where idx = 10;" },
          { ts: "2026-11-07 10:00:12", level: "info", text: "1 row affected in 4 ms" },
          { ts: "2026-11-07 10:05:15", level: "warn", text: prodReminder },
        ]} />
    </Frame>
  ),
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
            return [{ ts, text: `[DEV] shop_app> ${s.sql}`, level: "sql" as const, stmt: `o${i}` }, ...(text ? [{ ts, text, level: state === "ok" ? ("info" as const) : ("error" as const) }] : [])];
          }));
        }}
        onCancel={noop} onSetActiveResult={setActive} onCaret={noop} onStatements={noop} onPin={noop} onCloseResult={noop} onFetchMore={noop} onSubmit={async () => {}} onExport={noop} onHistory={noop}
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
/** What focused grids tell the status bar (as ResultGrid does): the cell or the selection (1-based rows and columns), the selected
 *  cells' stats, the current cell for Size (its table column's declared type, when known), the shown columns */
const sp = (first: number, last = first, count = last - first + 1) => ({ first, last, count });
const CELL_3_5: GridStatus = { pos: [sp(3), sp(5)], shown: [3, 8, 14] };
const SB_NULLS: GridStatus = { pos: [sp(30, 51), sp(6)], stats: { notNull: 4, nums: 0, sum: 0, min: 0, max: 0 }, shown: [1, 9, 14] };
const SB_NUMBERS: GridStatus = { pos: [sp(3, 42), sp(5, 7)], stats: { notNull: 120, nums: 120, sum: 1284930.5, min: 1, max: 98400 }, shown: [4, 11, 14] };
const SB_SCATTERED: GridStatus = { pos: [sp(1, 30, 22), sp(3)], stats: { notNull: 22, nums: 0, sum: 0, min: 0, max: 0 } };
const SB_CELL_PG: GridStatus = { kind: "postgres", pos: [sp(3), sp(4)], cell: { typeName: "VARCHAR", value: "Grüße ☕", column: { dataType: "character varying(8)" } } };
const SB_CELL_MY: GridStatus = { kind: "mysql", pos: [sp(3), sp(4)], cell: { typeName: "VARCHAR", value: "Grüße ☕", column: { dataType: "varchar(8)", charset: "utf8mb4" } } };
const SB_CELL_CONSOLE: GridStatus = { kind: "postgres", pos: [sp(2), sp(1)], cell: { typeName: "VARCHAR", value: "Grüße ☕" } };
const SB_ALL: StatusItemId[] = ["notNull", "sum", "avg", "min", "max", "size", "shownColumns", "position"];
const SbCaption = ({ children }: { children: string }) => (
  <Text tone="muted" size="sm">
    {children}
  </Text>
);
/** Status bar, information only (#107, #132): status text, transfers, update badge, then the focused grid's items, each its own segment
 * (muted name, value, a thin divider, no hover title): Position (the cell, or the selected rows and columns) and Shown columns by
 * default, Not null / Sum / Avg / Min / Max / Size when turned on; the focused editor's Ln, Col; Read-only; UTF-8. While the focused tab
 * runs or loads more rows, the running indicator (bar · what · elapsed) takes the status text's place; hovering it shows ✕, which
 * cancels (shown hovered in the last two) */
export const StatusBarStory: S = {
  name: "Status bar",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <style>{".sb-hovered .rb-cancel { visibility: visible; }"}</style>
      <SbCaption>Defaults (Position, Shown columns): one cell of a grid</SbCaption>
      <StatusBar status="3000+ rows retrieved" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: CELL_3_5 }} />
      <SbCaption>22 cells in one column, 18 of them NULL, Not null turned on</SbCaption>
      <StatusBar status="500 rows retrieved in 38 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: SB_NULLS }} items={["notNull", "shownColumns", "position"]} />
      <SbCaption>40 rows × 3 numeric columns, Sum / Avg / Min / Max turned on</SbCaption>
      <StatusBar status="500 rows retrieved in 38 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: SB_NUMBERS }} items={["sum", "avg", "min", "max", "shownColumns", "position"]} />
      <SbCaption>⌘-clicked rows that aren't one range</SbCaption>
      <StatusBar status="500 rows retrieved in 38 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: SB_SCATTERED }} items={["notNull", "shownColumns", "position"]} />
      <SbCaption>Size: one cell in a PostgreSQL table tab (users.nickname varchar(8)): its bytes and the column's limit</SbCaption>
      <StatusBar status="Table users opened" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: SB_CELL_PG }} items={["size", "shownColumns", "position"]} />
      <SbCaption>The same cell in a MySQL table tab (utf8mb4), Read-only data source</SbCaption>
      <StatusBar status="Table users opened" readOnly onUpdate={noop} jobs={[]} widgets={{ grid: SB_CELL_MY }} items={["size", "shownColumns", "position"]} />
      <SbCaption>A console result's column not mapped to a table column: the bytes only</SbCaption>
      <StatusBar status="1 statement executed in 12 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: SB_CELL_CONSOLE }} items={["size", "shownColumns", "position"]} />
      <SbCaption>The console editor focused (its own position), an update offered</SbCaption>
      <StatusBar status="1 statement executed in 12 ms" readOnly update="0.3.1" onUpdate={noop} jobs={[]} widgets={{ pos: "Ln 2, Col 11" }} />
      <StatusBar status="1 statement executed in 12 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ pos: "Ln 2, Col 11 (8 chars)" }} />
      <StatusBar status="Table users opened" readOnly={false} onUpdate={noop} jobs={[]} />
      <div className="sb-hovered">
        <StatusBar status="" busy={{ label: "Executing", since: Date.now() - 3400, onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} widgets={{ pos: "Ln 2, Col 11" }} />
      </div>
      <div className="sb-hovered">
        <StatusBar status="" busy={{ label: "Loading more rows", since: Date.now() - 800, onCancel: noop }} readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: CELL_3_5 }} />
      </div>
    </div>
  ),
};
/** Right-click the status bar (or click an item): the checklist of items by group, Restore Defaults; it stays open while items are
 *  toggled (↑↓ move, Space or ↩ toggles, Esc closes). The same setting as Settings › Appearance › Status bar and the TUI's ⌃k › Status
 *  Bar Items… (settings.json, here the mock's) */
export const StatusBarMenu: S = {
  name: "Status bar — items checklist",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "flex-end", height: 520 }}>
      <StatusBar status="500 rows retrieved in 38 ms" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: SB_NUMBERS }} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const bar = canvasElement.querySelector(".statusbar")!;
    const r = bar.getBoundingClientRect();
    fireEvent.contextMenu(bar, { clientX: r.right - 420, clientY: r.top });
    await waitFor(() => expect(document.querySelector(".sb-menu")).toBeTruthy());
  },
};
/** Narrow windows, every item on: the status text shrinks to 140 px, then UTF-8, the column limit, Max, Min, Avg, Sum, Not null, Size,
 *  Shown columns go; Position and Read-only stay (the text gives way) */
export const StatusBarNarrow: S = {
  name: "Status bar — narrow widths",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {[1260, 1000, 800, 640, 480, 360].map((w) => (
        <div key={w} style={{ width: w, display: "flex", flexDirection: "column", gap: 4 }}>
          <SbCaption>{`${w} px`}</SbCaption>
          <StatusBar status="500 rows retrieved in 38 ms" readOnly onUpdate={noop} jobs={[]} widgets={{ grid: SB_NUMBERS }} items={SB_ALL} />
          <StatusBar status="Table users opened" readOnly={false} onUpdate={noop} jobs={[]} widgets={{ grid: { ...SB_CELL_PG, shown: [1, 8, 12] } }} items={SB_ALL} />
        </div>
      ))}
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

/** #135 Reconnect: a table tab opened at once (Loading… in it) while its source reconnects through AWS SSM. The status bar's running
 *  indicator says so with the time; the toast (bottom right, above the status bar) has Cancel, past 10 s why, and on failure the reason
 *  and Retry. `waited`: seconds already waited (the clock runs on) */
function ReconnectScreen({ phase, waited = 3.2 }: { phase: ReconnectToast["phase"]; waited?: number }) {
  const [since] = useState(() => Date.now() - waited * 1000);
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, []);
  const name = "[PROD] orders";
  const t: ReconnectToast =
    phase === "waiting"
      ? { phase, id: "prod", message: `Reconnecting ${name} through AWS SSM`, slow: "Taking longer than usual: after the Mac sleeps, the AWS SSM session can take a while to come back.", since }
      : phase === "failed"
        ? { phase, id: "prod", title: `Couldn't reconnect ${name} through AWS SSM`, reason: "aws ssm start-session: TargetNotConnected — i-0a1b2c3d4e5f is not connected. Check that the instance is running and its SSM agent is online." }
        : { phase, id: "prod", text: `Cancelled while reconnecting ${name} through AWS SSM.` };
  const status = t.phase === "failed" ? `${t.title}: ${t.reason}` : t.phase === "cancelled" ? t.text : "";
  return (
    <div style={{ height: "100vh", display: "flex", flexDirection: "column", background: "var(--bg)" }}>
      <TabStrip className="tabs" activeKey="t">
        <TabItem label="payment" icon={<TableIcon />} active onClose={noop} />
      </TabStrip>
      <div style={{ position: "relative", flex: 1 }}>
        <TableEditor where="" orderBy="" running={phase === "waiting"} error={null} result={null} query="SELECT * FROM public.payment;" loadingMore={false}
          onApply={noop} onCancel={noop} onFetchMore={noop} onSubmit={async () => {}} onExportCsv={noop} onDdl={noop} onCopyQueryToConsole={noop} />
      </div>
      <Toasts>
        <ReconnectToastView t={t} now={now} onCancel={noop} onRetry={noop} onClose={noop} />
      </Toasts>
      <StatusBar status={status} busy={t.phase === "waiting" ? { label: t.message, since, onCancel: noop } : undefined} readOnly={false} onUpdate={noop} jobs={[]} />
    </div>
  );
}
const reconnectStory = (name: string, phase: ReconnectToast["phase"], light: boolean, waited?: number): S => ({
  name: `Reconnect — ${name} (${light ? "light" : "dark"})`,
  globals: { theme: light ? "light" : "dark" },
  render: () => <ReconnectScreen phase={phase} waited={waited} />,
});
export const ReconnectWaiting = reconnectStory("table tab waiting", "waiting", false);
export const ReconnectWaitingLight = reconnectStory("table tab waiting", "waiting", true);
export const ReconnectSlow = reconnectStory("slow (over 10 s)", "waiting", false, 14);
export const ReconnectSlowLight = reconnectStory("slow (over 10 s)", "waiting", true, 14);
export const ReconnectFailed = reconnectStory("failed, reason and Retry", "failed", false);
export const ReconnectFailedLight = reconnectStory("failed, reason and Retry", "failed", true);
export const ReconnectCancelled = reconnectStory("cancelled", "cancelled", false);
export const ReconnectCancelledLight = reconnectStory("cancelled", "cancelled", true);
