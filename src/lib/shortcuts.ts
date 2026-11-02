import { invoke } from "@tauri-apps/api/core";
import TABLE from "../../crates/shared/src/keymap-table.json";
import { initShared, shared, sharedReady } from "./shared";

/**
 * Rebindable GUI command shortcuts. Defaults and titles come from the command table shared with the TUI (crates/shared/src/keymap-table.json,
 * rows with a "gui" id); user settings go in "gui" of keymap.json in the config folder (only those differing from defaults).
 * Key notation: "Mod+Shift+L" — Mod = ⌘ on macOS / Ctrl elsewhere, Ctrl = ⌃ on macOS. Modifier order: Mod, Ctrl, Alt, Shift.
 * Key names follow the physical key (e.code), so they're the same with Korean input active.
 * scope: global = anywhere, tree = Database Explorer, editor = query console. Overlaps within the same scope (or between global and another scope) are conflicts
 */
export type Scope = "global" | "tree" | "editor";
export interface Shortcut {
  id: string;
  title: string;
  scope: Scope;
  keys: string[];
}

interface Row {
  gui?: string;
  title: string;
  scope?: Scope;
  guiKeys?: string[];
}

// Imported as JSON, not through WASM: keys work while the module loads (or if the WebView refuses it)
export const SHORTCUTS: Shortcut[] = (TABLE as Row[]).flatMap((r) => (r.gui ? [{ id: r.gui, title: r.title, scope: r.scope ?? "global", keys: r.guiKeys ?? [] }] : []));

export const SCOPE_LABEL: Record<Scope, string> = { global: "Global", tree: "Database Explorer", editor: "Query Console" };

const isMac = navigator.platform.toLowerCase().includes("mac");
const byId = new Map(SHORTCUTS.map((s) => [s.id, s]));

// ───────── Key notation ─────────

const CODE_NAMES: Record<string, string> = {
  Slash: "/", Backslash: "\\", Semicolon: ";", Quote: "'", Comma: ",", Period: ".", BracketLeft: "[", BracketRight: "]", Backquote: "`", Minus: "-", Equal: "=",
  Enter: "Enter", NumpadEnter: "Enter", Backspace: "Backspace", Delete: "Delete", Escape: "Esc", Tab: "Tab", Space: "Space",
  ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown", Insert: "Insert",
};

function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  return CODE_NAMES[code] ?? null;
}

/** Key event → notation. null when only modifiers are pressed */
export function eventSpec(e: { code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }): string | null {
  const k = keyName(e.code);
  if (!k) return null;
  const mods: string[] = [];
  if (isMac ? e.metaKey : e.ctrlKey) mods.push("Mod");
  if (isMac && e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  return [...mods, k].join("+");
}

// Notation is drawn by the shared WASM module (same rules as the TUI); until it has loaded, specs show as stored ("Mod+Shift+L")

/** Character in key notation: ⌘/⌃/⌥ (Ctrl+/Alt+) without Shift + a single letter is lowercase ("⌘k"). Uppercase means a key that needs Shift ("⇧⌘A") */
export function keyLabel(s: string): string {
  return sharedReady() ? shared().keyLabel(s) : s;
}

/** Display: ⌃⌥⇧⌘ symbols on macOS, Ctrl+Alt+Shift+ elsewhere (letters follow keyLabel rules) */
export function formatSpec(spec: string): string {
  return sharedReady() ? shared().formatSpec(spec, isMac) : spec;
}

/** Keys that can be newly bound: modifier combos or F keys (others clash with typing·navigation. tree's default Delete is restored with Reset) */
export function bindable(spec: string): boolean {
  return sharedReady() && shared().bindable(spec);
}

// ───────── User settings ─────────

let user: Record<string, string[]> = {};
const listeners = new Set<() => void>();

export function bindings(id: string): string[] {
  return user[id] ?? byId.get(id)?.keys ?? [];
}

/** Current key label ("⇧⌘L"). "" if no key */
export function shortcutLabel(id: string): string {
  return bindings(id).map(formatSpec).join(" / ");
}

export function isCustom(id: string): boolean {
  return id in user;
}

/** Whether this key event is the command's shortcut */
export function hit(spec: string | null, id: string): boolean {
  return spec !== null && bindings(id).includes(spec);
}

export function onKeymapChange(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function changed() {
  for (const fn of listeners) fn();
  invoke("save_keymap", { map: user }).catch((e) => console.error("save keymap:", e));
}

export async function loadKeymap() {
  try {
    user = await invoke<Record<string, string[]>>("load_keymap");
  } catch (e) {
    console.error("load keymap:", e);
    user = {};
  }
  for (const fn of listeners) fn();
  // Labels again once the WASM module that draws them has loaded
  if (!sharedReady()) initShared().then(() => listeners.forEach((fn) => fn()), () => {});
}

/** Other commands using the same key (with overlapping scope) */
export function conflicts(id: string, spec: string): Shortcut[] {
  const me = byId.get(id);
  if (!me) return [];
  const ids = shared().keyConflicts(id, me.scope, spec, SHORTCUTS.map((s) => ({ id: s.id, scope: s.scope, keys: bindings(s.id) })));
  return SHORTCUTS.filter((s) => ids.includes(s.id));
}

function setKeys(id: string, keys: string[]) {
  const def = byId.get(id)?.keys ?? [];
  if (keys.length === def.length && keys.every((k, i) => k === def[i])) delete user[id];
  else user[id] = keys;
}

/** Bind a key (add = add to existing keys). The key is removed from other overlapping commands. Returns: commands the key was removed from */
export function bind(id: string, spec: string, add: boolean): Shortcut[] {
  const taken = conflicts(id, spec);
  for (const s of taken) setKeys(s.id, bindings(s.id).filter((k) => k !== spec));
  const cur = add ? bindings(id).filter((k) => k !== spec) : [];
  setKeys(id, [...cur, spec]);
  changed();
  return taken;
}

export function removeKey(id: string, spec: string) {
  setKeys(id, bindings(id).filter((k) => k !== spec));
  changed();
}

export function resetShortcut(id: string) {
  delete user[id];
  changed();
}

export function resetAllShortcuts() {
  user = {};
  changed();
}
