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
export const ScreenRedis ={ ...by("Redis"), name: "Screen · Redis" };
export const ScreenMysqlBackupDefiners = { ...by("MySQL backup: DEFINER= question"), name: "Screen · MySQL backup: DEFINER= question" };
export const ScreenTooSmall = { ...by("Terminal too small"), name: "Screen · Terminal too small" };
export const ScreenUpdateChannel = { ...by("Update channel"), name: "Screen · Update channel" };
// The explorer screen in each color mode the TUI picks from NO_COLOR · COLORTERM · TERM (crates/tui/src/theme.rs)
export const ColorsTruecolor = { ...by("Colors: truecolor"), name: "Colors · truecolor" };
export const Colors256 = { ...by("Colors: 256"), name: "Colors · 256" };
export const Colors16 = { ...by("Colors: 16"), name: "Colors · 16" };
export const ColorsNoColor = { ...by("Colors: NO_COLOR"), name: "Colors · NO_COLOR" };
export const ColorsNoColorGrid = { ...by("Colors: NO_COLOR grid"), name: "Colors · NO_COLOR grid" };
