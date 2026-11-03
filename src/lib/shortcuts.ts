import { invoke } from "@tauri-apps/api/core";
import TABLE from "../../crates/shared/src/keymap-table.json";
import { initShared, shared, sharedReady } from "./shared";

/**
 * Rebindable GUI command shortcuts. Defaults and titles come from the command table shared with the TUI (crates/shared/src/keymap-table.json,
 * rows with a "gui" id); user settings go in "gui" of keymap.json in the config folder (only those differing from defaults).
 * Key notation: "Mod+Shift+L" — Mod = ⌘ on macOS / Ctrl elsewhere, Ctrl = ⌃ on macOS. Modifier order: Mod, Ctrl, Alt, Shift.
 * Key names follow the physical key (e.code), so they're the same with Korean input active.
 * scope: global = anywhere, tree = Database Explorer, editor = query console, grid = data grid. Overlaps within the same scope (or between global and
 * another scope) are conflicts. Every action a menu, context menu, toolbar or Find Action runs has a command here, with or without a default key
 */
export type Scope = "global" | "tree" | "editor" | "grid";
export interface Shortcut {
  id: string;
  title: string;
  /** Its line in Help › Keymap Reference */
  desc: string;
  scope: Scope;
  keys: string[];
}

interface Row {
  gui?: string;
  title: string;
  guiDesc?: string;
  scope?: Scope;
  guiKeys?: string[];
}

// Imported as JSON, not through WASM: keys work while the module loads (or if the WebView refuses it)
export const SHORTCUTS: Shortcut[] = (TABLE as Row[]).flatMap((r) => (r.gui ? [{ id: r.gui, title: r.title, desc: r.guiDesc ?? r.title, scope: r.scope ?? "global", keys: r.guiKeys ?? [] }] : []));

export const SCOPE_LABEL: Record<Scope, string> = { global: "Global", tree: "Database Explorer", editor: "Query Console", grid: "Data Grid" };

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

// Notation is drawn by the shared WASM module (same rules as the TUI). Until it has loaded (or if the WebView refuses it), formatSpec
// draws specs with a small copy of its rule, so labels never show the stored "Mod+Shift+L"; on-screen text is already written in it

/** Character in key notation: ⌘/⌃/⌥ (Ctrl+/Alt+) without Shift + a single letter is lowercase ("⌘k"). Uppercase means a key that needs Shift ("⇧⌘A") */
export function keyLabel(s: string): string {
  return sharedReady() ? shared().keyLabel(s) : s;
}

/** Display: ⌃⌥⇧⌘ symbols on macOS, Ctrl+Alt+Shift+ elsewhere (letters follow keyLabel rules) */
export function formatSpec(spec: string): string {
  return sharedReady() ? shared().formatSpec(spec, isMac) : formatSpecTs(spec);
}

const MAC_KEYS: Record<string, string> = { Enter: "↩", Backspace: "⌫", Delete: "⌦", Esc: "⎋", Tab: "⇥", Up: "↑", Down: "↓", Left: "←", Right: "→", PageUp: "⇞", PageDown: "⇟" };

/** formatSpec without the module: qurry_shared::keymap::format_spec for the specs commands use (shortcuts.test.ts compares them) */
function formatSpecTs(spec: string): string {
  const parts = spec.split("+");
  let key = parts.pop()!;
  const has = (m: string) => parts.includes(m);
  if (/^[A-Z]$/.test(key) && parts.length && !has("Shift")) key = key.toLowerCase();
  if (isMac) return (has("Ctrl") ? "⌃" : "") + (has("Alt") ? "⌥" : "") + (has("Shift") ? "⇧" : "") + (has("Mod") ? "⌘" : "") + (MAC_KEYS[key] ?? key);
  return [has("Mod") || has("Ctrl") ? "Ctrl" : "", has("Alt") ? "Alt" : "", has("Shift") ? "Shift" : "", key].filter(Boolean).join("+");
}

// bindable and conflicts stay in TS (not through WASM) so the keymap editor works while the module loads or if it fails to load;
// shortcuts.test.ts checks they answer the same as qurry_shared::keymap::bindable / conflicts

/** Keys that can be newly bound: modifier combos or F keys (others clash with typing·navigation. tree's default Delete is restored with Reset) */
export function bindable(spec: string): boolean {
  return /^(Mod|Ctrl|Alt)\+/.test(spec) || /(^|\+)F\d{1,2}$/.test(spec);
}

/** Keys that can be bound in a scope: in the tree and the grid also Delete / Insert / Backspace (any modifiers) and ⇧↩ — not letters, which
 *  type into speed search or the cell there (qurry_shared::keymap::bindable_in) */
export function bindableIn(spec: string, scope: Scope): boolean {
  if (scope === "global" || scope === "editor") return bindable(spec);
  return bindable(spec) || /(^|\+)(Delete|Insert|Backspace)$/.test(spec) || spec === "Shift+Enter";
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

/** A menu entry's / Find Action item's command: spread into it ({ label, ...cmd("tab.pin"), action }) for its id and current (first) shortcut */
export function cmd(id: string): { id: string; shortcut?: string } {
  const k = bindings(id)[0];
  return { id, shortcut: k === undefined ? undefined : formatSpec(k) };
}

/** A toolbar button's tooltip: "Execute (⌘↩)", or the title alone while the command has no key */
export function keyTitle(title: string, id: string): string {
  const k = cmd(id).shortcut;
  return k ? `${title} (${k})` : title;
}

/** The commands of a scope this key event is bound to (for key handlers that run a menu's or Find Action's item by its id) */
export function commandsFor(spec: string | null, scope: Scope): string[] {
  return spec === null ? [] : SHORTCUTS.filter((s) => s.scope === scope && bindings(s.id).includes(spec)).map((s) => s.id);
}

type Runnable = { id?: string; disabled?: boolean; action?: () => void; run?: () => void; submenu?: Runnable[] };

/** Runs the enabled menu entry or Find Action item with this command id (submenus too): how a key runs what a menu shows.
 *  false when the list has no such enabled entry */
export function runById(items: readonly object[], id: string): boolean {
  for (const it of items as Runnable[]) {
    if (it.submenu && runById(it.submenu, id)) return true;
    if (it.id === id && !it.disabled) {
      (it.action ?? it.run)?.();
      return true;
    }
  }
  return false;
}

/** Runs the first command of `scope` bound to this key whose entry in `items()` is enabled (the list is built only when the key is
 *  a command key there). Whether one ran */
export function runKey(spec: string | null, scope: Scope, items: () => readonly object[]): boolean {
  const ids = commandsFor(spec, scope);
  if (!ids.length) return false;
  const list = items();
  return ids.some((id) => runById(list, id));
}

export function onKeymapChange(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function changed() {
  for (const fn of listeners) fn();
  invoke("save_keymap", { map: user }).catch((e) => console.error("save keymap:", e));
}

/** Loads the "gui" section of keymap.json. If the file doesn't load, the defaults apply and the file stays as it is (saving refuses to
 *  overwrite it): returns what to tell the user, null when it loaded */
export async function loadKeymap(): Promise<string | null> {
  let err: string | null = null;
  try {
    user = await invoke<Record<string, string[]>>("load_keymap");
  } catch (e) {
    user = {};
    err = `keymap.json ignored (${e}) — default shortcuts in use; fix or delete keymap.json in the config folder and restart`;
  }
  for (const fn of listeners) fn();
  // Labels again once the WASM module that draws them has loaded
  if (!sharedReady()) initShared().then(() => listeners.forEach((fn) => fn()), () => {});
  return err;
}

/** Other commands using the same key (with overlapping scope) */
export function conflicts(id: string, spec: string): Shortcut[] {
  const me = byId.get(id);
  if (!me) return [];
  const overlap = (s: Scope) => s === me.scope || s === "global" || me.scope === "global";
  return SHORTCUTS.filter((s) => s.id !== id && overlap(s.scope) && bindings(s.id).includes(spec));
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
