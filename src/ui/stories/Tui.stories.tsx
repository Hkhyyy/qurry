import type { Meta, StoryObj } from "@storybook/react-vite";
import data from "./tui-snapshots.json";
import { Scene, TuiScreen } from "./TuiScreen";

/**
 * TUI 화면. 실제 TUI 코드 (crates/tui/src/kit.rs 의 색 · 조각) 로 그린 그대로다.
 * 바뀌면 `pnpm tui-snapshots` 로 다시 그린다. 테마는 터미널 색이라 위 Theme 와 상관없다
 */
const meta: Meta<{ scene: Scene }> = { title: "TUI/Screens", component: TuiScreen, parameters: { layout: "padded" } };
export default meta;
type S = StoryObj<{ scene: Scene }>;

const scenes = (data as { scenes: Scene[] }).scenes;
const by = (name: string): S => ({ name, args: { scene: scenes.find((s) => s.name === name)! } });

export const Components = by("Components");
export const Explorer = by("Explorer & table");
export const ProductionConfirm = by("Production confirm");
export const ActionsPalette = by("Actions palette");
export const DataSourceForm = by("Data source form");
export const Keymap = by("Keymap (F1)");
export const Workspaces = by("Workspaces");
