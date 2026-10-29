import { invoke } from "@tauri-apps/api/core";

/**
 * Rebindable GUI command shortcuts. Defaults live here; user settings go in "gui" of keymap.json in the config folder (only those differing from defaults).
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

export const SHORTCUTS: Shortcut[] = [
  { id: "goto.object", title: "Go to Database Object", scope: "global", keys: ["Mod+O"] },
  { id: "goto.symbol", title: "Go to Symbol (columns)", scope: "global", keys: ["Mod+Alt+O"] },
  { id: "goto.file", title: "Go to File", scope: "global", keys: ["Mod+Shift+O"] },
  { id: "find.action", title: "Find Action", scope: "global", keys: ["Mod+Shift+A"] },
  { id: "fts", title: "Full-text Search", scope: "global", keys: ["Mod+Alt+Shift+F"] },
  { id: "recent", title: "Recent Files", scope: "global", keys: ["Mod+E"] },
  { id: "keymap", title: "Keymap Reference", scope: "global", keys: ["Mod+Shift+/"] },
  { id: "explorer", title: "Database Explorer (focus / hide)", scope: "global", keys: ["Mod+0"] },
  { id: "find.window", title: "Find Tool Window", scope: "global", keys: ["Mod+Shift+F"] },
  { id: "maximize", title: "Maximize Editor", scope: "global", keys: ["Mod+Shift+F12"] },
  { id: "settings", title: "Settings", scope: "global", keys: ["Mod+,"] },
  { id: "theme", title: "Quick Switch Theme", scope: "global", keys: ["Ctrl+`"] },
  { id: "font.bigger", title: "Increase Editor Font Size", scope: "global", keys: ["Ctrl+Shift+."] },
  { id: "font.smaller", title: "Decrease Editor Font Size", scope: "global", keys: ["Ctrl+Shift+,"] },
  { id: "console.new", title: "New Query Console", scope: "global", keys: ["Mod+Shift+L"] },
  { id: "refresh", title: "Refresh", scope: "global", keys: ["Mod+Alt+Y"] },
  { id: "tab.close", title: "Close Tab", scope: "global", keys: ["Mod+W"] },
  { id: "tab.reopen", title: "Reopen Closed Tab", scope: "global", keys: ["Mod+Shift+T"] },
  { id: "tab.next", title: "Next Tab", scope: "global", keys: ["Mod+Shift+]"] },
  { id: "tab.prev", title: "Previous Tab", scope: "global", keys: ["Mod+Shift+["] },
  { id: "split.other", title: "Go to the Next Split (⇧: previous)", scope: "global", keys: ["Alt+Tab", "Alt+Shift+Tab"] },
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n): Shortcut => ({ id: `ws.${n}`, title: `Workspace ${n}`, scope: "global", keys: [`Mod+${n}`] })),

  { id: "tree.editData", title: "Edit Data / Open Console", scope: "tree", keys: ["Mod+Down", "F4"] },
  { id: "tree.jumpConsole", title: "Jump to Query Console", scope: "tree", keys: ["Mod+Shift+F10"] },
  { id: "tree.ddl", title: "Go to DDL", scope: "tree", keys: ["Mod+B"] },
  { id: "tree.sqlGen", title: "SQL Generator", scope: "tree", keys: ["Mod+Alt+G"] },
  { id: "tree.ddlCopy", title: "Copy DDL to Clipboard", scope: "tree", keys: ["Mod+Alt+Shift+G"] },
  { id: "tree.ddlConsole", title: "Open DDL in Console", scope: "tree", keys: ["Mod+Alt+Shift+B"] },
  { id: "tree.copyRef", title: "Copy Reference", scope: "tree", keys: ["Mod+Alt+Shift+C"] },
  { id: "tree.quickDoc", title: "Quick Documentation", scope: "tree", keys: ["F1"] },
  { id: "tree.modify", title: "Modify Table", scope: "tree", keys: ["Mod+F6"] },
  { id: "tree.rename", title: "Rename", scope: "tree", keys: ["Shift+F6"] },
  { id: "tree.properties", title: "Data Source Properties", scope: "tree", keys: ["Mod+;"] },
  { id: "tree.newDs", title: "New Data Source", scope: "tree", keys: ["Mod+N"] },
  { id: "tree.duplicate", title: "Duplicate Data Source", scope: "tree", keys: ["Mod+D"] },
  { id: "tree.refresh", title: "Refresh Node", scope: "tree", keys: ["Mod+R", "Mod+Alt+Shift+R"] },
  { id: "tree.disconnect", title: "Disconnect", scope: "tree", keys: ["Mod+F2"] },
  { id: "tree.remove", title: "Remove Data Source / Drop Object", scope: "tree", keys: ["Delete", "Mod+Backspace"] },

  { id: "editor.run", title: "Execute", scope: "editor", keys: ["Mod+Enter"] },
  { id: "editor.cancel", title: "Cancel Running Statement", scope: "editor", keys: ["Mod+F2"] },
  { id: "editor.history", title: "Browse Query History", scope: "editor", keys: ["Mod+Alt+E"] },
  { id: "editor.reformat", title: "Reformat Code", scope: "editor", keys: ["Mod+Alt+L"] },
  { id: "editor.blockComment", title: "Comment with Block Comment", scope: "editor", keys: ["Mod+Alt+/"] },
  { id: "editor.duplicate", title: "Duplicate Line or Selection", scope: "editor", keys: ["Mod+D"] },
  { id: "editor.deleteLine", title: "Delete Line", scope: "editor", keys: ["Mod+Backspace"] },
  { id: "editor.moveUp", title: "Move Line Up", scope: "editor", keys: ["Alt+Shift+Up"] },
  { id: "editor.moveDown", title: "Move Line Down", scope: "editor", keys: ["Alt+Shift+Down"] },
  { id: "editor.extend", title: "Extend Selection", scope: "editor", keys: ["Alt+Up"] },
  { id: "editor.shrink", title: "Shrink Selection", scope: "editor", keys: ["Alt+Down"] },
  { id: "editor.nextOcc", title: "Select Next Occurrence", scope: "editor", keys: ["Ctrl+G"] },
  { id: "editor.allOcc", title: "Select All Occurrences", scope: "editor", keys: ["Mod+Ctrl+G"] },
  { id: "editor.toggleCase", title: "Toggle Case", scope: "editor", keys: ["Mod+Shift+U"] },
  { id: "editor.gotoLine", title: "Go to Line", scope: "editor", keys: ["Mod+L"] },
  { id: "editor.replace", title: "Replace (regex .*, match case, words)", scope: "editor", keys: ["Mod+R"] },
];

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

const MAC_KEYS: Record<string, string> = { Enter: "↩", Backspace: "⌫", Delete: "⌦", Esc: "⎋", Tab: "⇥", Up: "↑", Down: "↓", Left: "←", Right: "→", Space: "Space", PageUp: "⇞", PageDown: "⇟" };

/** Character in key notation: ⌘/⌃/⌥ (Ctrl+/Alt+) without Shift + a single letter is lowercase ("⌘k"). Uppercase means a key that needs Shift ("⇧⌘A") */
export function keyLabel(s: string): string {
  return s
    .replace(/([⌃⌥⇧⌘]+)([A-Z])(?![A-Za-z0-9])/g, (m, mods: string, k: string) => (mods.includes("⇧") ? m : mods + k.toLowerCase()))
    .replace(/\b((?:(?:Ctrl|Alt|Shift|Win|Cmd)\+)+)([A-Z])(?![A-Za-z0-9])/g, (m, mods: string, k: string) => (mods.includes("Shift+") ? m : mods + k.toLowerCase()));
}

/** Display: ⌃⌥⇧⌘ symbols on macOS, Ctrl+Alt+Shift+ elsewhere (letters follow keyLabel rules) */
export function formatSpec(spec: string): string {
  return keyLabel(formatSpecRaw(spec));
}

function formatSpecRaw(spec: string): string {
  const parts = spec.split("+");
  const key = parts.pop()!;
  const has = (m: string) => parts.includes(m);
  if (isMac) {
    return (has("Ctrl") ? "⌃" : "") + (has("Alt") ? "⌥" : "") + (has("Shift") ? "⇧" : "") + (has("Mod") ? "⌘" : "") + (MAC_KEYS[key] ?? key);
  }
  const mods = [has("Mod") || has("Ctrl") ? "Ctrl" : "", has("Alt") ? "Alt" : "", has("Shift") ? "Shift" : ""].filter(Boolean);
  return [...mods, key].join("+");
}

/** Keys that can be newly bound: modifier combos or F keys (others clash with typing·navigation. tree's default Delete is restored with Reset) */
export function bindable(spec: string): boolean {
  return /^(Mod|Ctrl|Alt)\+/.test(spec) || /(^|\+)F\d{1,2}$/.test(spec);
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
}

const overlaps = (a: Scope, b: Scope) => a === b || a === "global" || b === "global";

/** Other commands using the same key (with overlapping scope) */
export function conflicts(id: string, spec: string): Shortcut[] {
  const me = byId.get(id);
  if (!me) return [];
  return SHORTCUTS.filter((s) => s.id !== id && overlaps(s.scope, me.scope) && bindings(s.id).includes(spec));
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
