import type { Meta, StoryObj } from "@storybook/react-vite";
import data from "./tui-snapshots.json";
import { Scene, TuiScreen } from "./TuiScreen";

/**
 * TUI. Drawn exactly as the real TUI code (crates/tui/src/kit — atoms · molecules · organisms) renders to a virtual terminal.
 * Redraw with `pnpm tui-snapshots` after changes. Scenes use terminal colors, so they are unaffected by the Theme above (the Light · and Theme · stories are drawn in the TUI themes)
 */
const scenes = (data as { scenes: Scene[] }).scenes;
const by = (name: string): StoryObj<{ scene: Scene }> => ({ name, args: { scene: scenes.find((s) => s.name === name)! } });

const meta: Meta<{ scene: Scene }> = { title: "TUI", component: TuiScreen, parameters: { layout: "padded" } };
export default meta;

export const Atoms = by("Atoms");
export const Molecules = by("Molecules");
export const Organisms = by("Organisms");
export const ScreenExplorer = { ...by("Explorer & table"), name: "Screen · Explorer & table" };
export const ScreenExplorerTemporary = { ...by("Explorer: temporary source"), name: "Screen · Explorer: temporary source" };
export const ScreenExplorerMarked = { ...by("Explorer: marked nodes"), name: "Screen · Explorer: marked nodes (m)" };
export const ScreenExplorerDropMarked = { ...by("Explorer: drop marked objects"), name: "Screen · Explorer: drop marked objects" };
export const ScreenProductionConfirm = { ...by("Production confirm"), name: "Screen · Production confirm" };
export const ScreenActionsPalette = { ...by("Actions palette"), name: "Screen · Actions palette" };
export const ScreenDataSourceForm = { ...by("Data source form"), name: "Screen · Data source form" };
// #131: Admin reports (MCP dba_report) for Production sources; disabled with why on other sources and with MCP Off
export const ScreenFormReportsOff = { ...by("Data source form: admin reports, off"), name: "Screen · Data source form: admin reports, Production off" };
export const ScreenFormReportsOn = { ...by("Data source form: admin reports, on"), name: "Screen · Data source form: admin reports, Production on" };
export const ScreenFormReportsNotProduction = { ...by("Data source form: admin reports, not Production"), name: "Screen · Data source form: admin reports, not Production" };
export const ScreenFormReportsMcpOff = { ...by("Data source form: admin reports, MCP Off"), name: "Screen · Data source form: admin reports, MCP Off" };
export const ScreenFormReportsOffLight = { ...by("Data source form: admin reports, off (light)"), name: "Light · Data source form: admin reports, Production off" };
export const ScreenFormReportsOnLight = { ...by("Data source form: admin reports, on (light)"), name: "Light · Data source form: admin reports, Production on" };
export const ScreenKeymap = { ...by("Keymap (F1)"), name: "Screen · Keymap (F1)" };
export const ScreenWorkspaces = { ...by("Workspaces"), name: "Screen · Workspaces" };
export const ScreenConsole = { ...by("Console & result"), name: "Screen · Console & result" };
export const ScreenConsoleRunning = { ...by("Console gutter marks"), name: "Screen · Console running: [■ Cancel] and gutter marks" };
export const ScreenConsoleEditable ={ ...by("Console result: editable"), name: "Screen · Console result: editable" };
export const ScreenConsoleReadOnly = { ...by("Console result: read-only"), name: "Screen · Console result: read-only (why)" };
// #135: a load waiting on a source that reconnects (the GUI's status bar and toast)
export const ScreenReconnectWaiting = { ...by("Reconnect: table tab waiting"), name: "Screen · Reconnect: table tab waiting" };
export const ScreenReconnectSlow = { ...by("Reconnect: slow"), name: "Screen · Reconnect: slow (over 10 s)" };
export const ScreenReconnectFailed = { ...by("Reconnect: failed"), name: "Screen · Reconnect: failed (Message popup)" };
export const ScreenReconnectCancelled = { ...by("Reconnect: cancelled"), name: "Screen · Reconnect: cancelled" };
export const ScreenReconnectExplorer = { ...by("Reconnect: explorer expand"), name: "Screen · Reconnect: explorer expand" };
export const ScreenConsoleJoin = { ...by("Console result: join"), name: "Screen · Console result: join (per table, outer side read-only)" };
export const ScreenConsoleProductionTx = { ...by("Console: Production transaction open"), name: "Screen · Console: Production transaction open (pending, reminder)" };
export const ScreenConsoleFkJoin = { ...by("Console: FK join"), name: "Screen · Console: FK join completion" };
export const ScreenConsoleSignature = { ...by("Console: signature hint"), name: "Screen · Console: signature hint line" };
export const ScreenConsoleUnknownName = { ...by("Console: unknown name"), name: "Screen · Console: unknown name (⌥↩)" };
export const ScreenConsoleUnclosed = { ...by("Console: unclosed bracket"), name: "Screen · Console: unclosed bracket" };
export const ScreenConsoleRunSubquery = { ...by("Console: run a subquery"), name: "Screen · Console: run a subquery (F5)" };
export const ScreenConsoleRunError = { ...by("Console: run error"), name: "Screen · Console: run error" };
export const ScreenGridMarked = { ...by("Grid: marked rows"), name: "Screen · Grid: marked rows (m)" };
// Status line items (#132): the GUI status bar's items after the key hints, Position in the corner
export const ScreenStatusSelection = { ...by("Status line: selection items"), name: "Screen · Status line: selection items (marked rows)" };
export const ScreenStatusSize = { ...by("Status line: Size"), name: "Screen · Status line: Size" };
export const ScreenStatusItems = { ...by("Status bar items checklist"), name: "Screen · Status bar items checklist (⌃k)" };
export const ScreenStatusNarrow = { ...by("Status line: narrow"), name: "Small · Status line: narrow (80 columns)" };
export const ScreenChart = { ...by("Chart"), name: "Screen · Chart" };
export const ScreenGridLoadingMore = { ...by("Grid: loading more"), name: "Screen · Grid: loading more" };
export const ScreenGridLoadingMoreNoColor = { ...by("Grid: loading more, NO_COLOR"), name: "Screen · Grid: loading more, NO_COLOR" };
// The grid's horizontal scrollbar on its bottom border (#107)
export const ScreenGridScrollbar = { ...by("Grid scrollbar: focused"), name: "Screen · Grid scrollbar: focused" };
export const ScreenGridScrollbarBlur = { ...by("Grid scrollbar: not focused"), name: "Screen · Grid scrollbar: not focused" };
export const ScreenGridScrollbarLight = { ...by("Grid scrollbar: light"), name: "Screen · Grid scrollbar: light" };
export const ScreenGridScrollbar16 = { ...by("Grid scrollbar: 16 colors"), name: "Screen · Grid scrollbar: 16 colors" };
export const ScreenGridScrollbarNoColor = { ...by("Grid scrollbar: NO_COLOR"), name: "Screen · Grid scrollbar: NO_COLOR" };
export const ScreenGridScrollbarError = { ...by("Grid scrollbar: beside a failed load-more"), name: "Screen · Grid scrollbar: beside a failed load-more" };
export const ScreenProductionLog = { ...by("Production log"), name: "Screen · Production log" };
export const ScreenProductionLogNote = { ...by("Production log · note"), name: "Screen · Production log · note" };
export const ScreenTableDesigner = { ...by("Table designer"), name: "Screen · Table designer (⌃F6)" };
export const ScreenGenerateData = { ...by("Generate test data"), name: "Screen · Generate test data" };
export const ScreenCopyRows = { ...by("Copy related rows"), name: "Screen · Copy related rows" };
export const ScreenImportPick = { ...by("Import Data Sources — pick"), name: "Screen · Import Data Sources: pick a location" };
export const ScreenImportConfirm = { ...by("Import Data Sources — confirm"), name: "Screen · Import Data Sources: confirm (Space leaves a row out)" };
export const ScreenDbaStale = { ...by("Database Admin: refresh failed"), name: "Screen · Database Admin: refresh failed" };
export const ScreenDbaError = { ...by("Database Admin: report error"), name: "Screen · Database Admin: report error" };
export const ScreenTableFilter = { ...by("Table filter completion"), name: "Screen · Table filter completion" };
export const ScreenTableFilterColors = { ...by("Table filter: SQL colors"), name: "Screen · Table filter: SQL colors" };
export const ScreenTableFilterColorsLight = { ...by("Table filter: SQL colors (light)"), name: "Screen · Table filter: SQL colors (light)" };
export const ScreenTableFilterOrderBy = { ...by("Table filter: ORDER BY colors"), name: "Screen · Table filter: ORDER BY colors" };
export const ScreenDbaDatabase = { ...by("Database Admin: database"), name: "Screen · Database Admin: database" };
export const ScreenRedis = { ...by("Redis"), name: "Screen · Redis" };
export const ScreenRedisDatabase = { ...by("Redis: database"), name: "Screen · Redis: database (d)" };
export const ScreenRedisCommandLine = { ...by("Redis: command line"), name: "Screen · Redis: command line (:)" };
export const ScreenRedisArgumentHints = { ...by("Redis: argument hints"), name: "Screen · Redis: argument hints" };
export const ScreenMysqlBackupDefiners = { ...by("MySQL backup: DEFINER= question"), name: "Screen · MySQL backup: DEFINER= question" };
export const ScreenTooSmall = { ...by("Terminal too small"), name: "Screen · Terminal too small" };
// Small windows: what stays when a dialog doesn't fit
export const ScreenTooSmallQuestion = { ...by("Terminal too small: question waiting"), name: "Small · Terminal too small: question waiting" };
export const ScreenProductionConfirmMin = { ...by("Production confirm: minimum size"), name: "Small · Production confirm (40×10)" };
export const ScreenGenerateDataMin = { ...by("Generate test data: minimum size"), name: "Small · Generate test data (40×10)" };
export const ScreenCopyRows80 = { ...by("Copy related rows: 80×24"), name: "Small · Copy related rows (80×24)" };
export const ScreenConfirmTooSmall = { ...by("Confirm: too small to answer"), name: "Small · Confirm too small to answer" };
export const ScreenDataSourceForm80 = { ...by("Data source form: 80×24"), name: "Small · Data source form (80×24)" };
export const ScreenUpdateChannel = { ...by("Update channel"), name: "Screen · Update channel" };
export const ScreenTheme = { ...by("Theme"), name: "Screen · Theme" };
export const ScreenThemeSyncLight = { ...by("Theme: Sync with OS on a light terminal"), name: "Screen · Theme: Sync with OS on a light terminal" };
export const ScreenGridAlign = { ...by("Grid alignment"), name: "Screen · Grid alignment" };
export const ScreenGridAlignChanged = { ...by("Grid alignment: changed"), name: "Screen · Grid alignment: changed" };
export const ScreenGridAlignPalette = { ...by("Grid alignment palette"), name: "Screen · Grid alignment palette" };
export const ScreenGridAlignPick = { ...by("Grid alignment: pick"), name: "Screen · Grid alignment: pick" };
// The explorer screen in each color mode the TUI picks from NO_COLOR · COLORTERM · TERM (crates/tui/src/theme.rs)
export const ColorsTruecolor = { ...by("Colors: truecolor"), name: "Colors · truecolor" };
export const Colors256 = { ...by("Colors: 256"), name: "Colors · 256" };
export const Colors16 = { ...by("Colors: 16"), name: "Colors · 16" };
export const ColorsNoColor = { ...by("Colors: NO_COLOR"), name: "Colors · NO_COLOR" };
export const ColorsNoColorGrid = { ...by("Colors: NO_COLOR grid"), name: "Colors · NO_COLOR grid" };
// Split panes with many tabs: the visible tab of the focused pane and of the other pane in each color mode
export const TabBarTruecolor = { ...by("Tab bar: truecolor"), name: "Tab bar · truecolor" };
export const TabBar16 = { ...by("Tab bar: 16 colors"), name: "Tab bar · 16 colors" };
export const TabBarNoColor = { ...by("Tab bar: NO_COLOR"), name: "Tab bar · NO_COLOR" };
// The light theme (⌃k › Theme… › Light, or Sync with OS when COLORFGBG says the terminal background is light), on a light terminal
export const LightTruecolor = { ...by("Light: truecolor"), name: "Light · truecolor" };
export const Light256 = { ...by("Light: 256"), name: "Light · 256" };
export const Light16 = { ...by("Light: 16"), name: "Light · 16" };
export const LightNoColor = { ...by("Light: NO_COLOR"), name: "Light · NO_COLOR" };
export const LightGridEdits = { ...by("Light: grid edits"), name: "Light · grid edits" };
export const LightGridJson = { ...by("Light: grid JSON"), name: "Light · grid JSON" };
export const LightProductionConfirm = { ...by("Light: production confirm"), name: "Light · Production confirm" };
export const LightTabBar = { ...by("Tab bar: light"), name: "Light · tab bar" };
export const LightReconnect = { ...by("Reconnect: table tab waiting (light)"), name: "Light · Reconnect: table tab waiting" };
// Each built-in theme (⌃k › Theme…, shared with the GUI): Dark, Light and High contrast on the terminal's background, the others on their own
export const ThemeDark = { ...by("Theme: Dark"), name: "Theme · Dark" };
export const ThemeLight = { ...by("Theme: Light"), name: "Theme · Light" };
export const ThemeHighContrast = { ...by("Theme: High contrast"), name: "Theme · High contrast" };
export const ThemeDracula = { ...by("Theme: Dracula"), name: "Theme · Dracula" };
export const ThemeOneDark = { ...by("Theme: One Dark"), name: "Theme · One Dark" };
export const ThemeSolarizedDark = { ...by("Theme: Solarized Dark"), name: "Theme · Solarized Dark" };
export const ThemeSolarizedLight = { ...by("Theme: Solarized Light"), name: "Theme · Solarized Light" };
