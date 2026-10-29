import type { Meta, StoryObj } from "@storybook/react-vite";
import data from "./tui-snapshots.json";
import { Scene, TuiScreen } from "./TuiScreen";

/**
 * TUI. 실제 TUI 코드 (crates/tui/src/kit — atoms · molecules · organisms) 로 가상 터미널에 그린 그대로다.
 * 바뀌면 `pnpm tui-snapshots` 로 다시 그린다. 테마는 터미널 색이라 위 Theme 와 상관없다
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
export const ScreenTableDesigner = { ...by("Table designer"), name: "Screen · Table designer (⌃F6)" };
export const ScreenGenerateData = { ...by("Generate test data"), name: "Screen · Generate test data" };
export const ScreenCopyRows = { ...by("Copy related rows"), name: "Screen · Copy related rows" };
export const ScreenRedis = { ...by("Redis"), name: "Screen · Redis" };
