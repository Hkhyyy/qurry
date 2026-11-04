import type { Meta, StoryObj } from "@storybook/react-vite";
import data from "./tui-snapshots.json";
import { Scene, TuiScreen } from "./TuiScreen";

/**
 * TUI. Drawn exactly as the real TUI code (crates/tui/src/kit — atoms · molecules · organisms) renders to a virtual terminal.
 * Redraw with `pnpm tui-snapshots` after changes. The theme uses terminal colors, so it's unaffected by the Theme above
 */
const scenes = (data as { scenes: Scene[] }).scenes;
const by = (name: string): StoryObj<{ scene: Scene }> => ({ name, args: { scene: scenes.find((s) => s.name === name)! } });

const meta: Meta<{ scene: Scene }> = { title: "TUI", component: TuiScreen, parameters: { layout: "padded" } };
export default meta;

export const Atoms = by("Atoms");
export const Molecules = by("Molecules");
export const Organisms = by("Organisms");
export const ScreenExplorer = { ...by("Explorer & table"), name: "Screen · Explorer & table" };
export const ScreenProductionConfirm = { ...by("Production confirm"), name: "Screen · Production confirm" };
export const ScreenActionsPalette = { ...by("Actions palette"), name: "Screen · Actions palette" };
export const ScreenDataSourceForm = { ...by("Data source form"), name: "Screen · Data source form" };
export const ScreenKeymap = { ...by("Keymap (F1)"), name: "Screen · Keymap (F1)" };
export const ScreenWorkspaces = { ...by("Workspaces"), name: "Screen · Workspaces" };
export const ScreenConsole = { ...by("Console & result"), name: "Screen · Console & result" };
export const ScreenChart = { ...by("Chart"), name: "Screen · Chart" };
export const ScreenProductionLog = { ...by("Production log"), name: "Screen · Production log" };
export const ScreenProductionLogNote = { ...by("Production log · note"), name: "Screen · Production log · note" };
export const ScreenTableDesigner = { ...by("Table designer"), name: "Screen · Table designer (⌃F6)" };
export const ScreenGenerateData = { ...by("Generate test data"), name: "Screen · Generate test data" };
export const ScreenCopyRows = { ...by("Copy related rows"), name: "Screen · Copy related rows" };
export const ScreenDbaStale = { ...by("Database Admin: refresh failed"), name: "Screen · Database Admin: refresh failed" };
export const ScreenDbaError = { ...by("Database Admin: report error"), name: "Screen · Database Admin: report error" };
export const ScreenTableFilter = { ...by("Table filter completion"), name: "Screen · Table filter completion" };
export const ScreenRedis ={ ...by("Redis"), name: "Screen · Redis" };
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
