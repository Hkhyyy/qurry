import { invoke } from "@tauri-apps/api/core";

/**
 * 바꿀 수 있는 GUI 명령 단축키. 기본값은 여기, 사용자 설정은 설정 폴더 keymap.json 의 "gui" (기본값과 다른 것만).
 * 키 표기: "Mod+Shift+L" — Mod = macOS ⌘ / 그 밖 Ctrl, Ctrl = macOS ⌃. 수식키 순서: Mod, Ctrl, Alt, Shift.
 * 키 이름은 물리 키(e.code) 기준이라 한글 입력 상태에서도 같다.
 * scope: global = 어디서나, tree = Database Explorer, editor = 쿼리 콘솔. 같은 범위(또는 global 과 다른 범위)끼리 겹치면 충돌
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
  { id: "explorer", title: "Database Explorer (focus / hide)", scope: "global", keys: ["Mod+1"] },
  { id: "find.window", title: "Find Tool Window", scope: "global", keys: ["Mod+3"] },
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

  { id: "tree.editData", title: "Edit Data / Open Console", scope: "tree", keys: ["Mod+Down", "F4"] },
  { id: "tree.jumpConsole", title: "Jump to Query Console", scope: "tree", keys: ["Mod+Shift+F10"] },
  { id: "tree.ddl", title: "Go to DDL", scope: "tree", keys: ["Mod+B"] },
  { id: "tree.sqlGen", title: "SQL Generator", scope: "tree", keys: ["Mod+Alt+G"] },
  { id: "tree.ddlCopy", title: "Copy DDL to Clipboard", scope: "tree", keys: ["Mod+Alt+Shift+G"] },
  { id: "tree.ddlConsole", title: "Open DDL in Console", scope: "tree", keys: ["Mod+Alt+Shift+B"] },
  { id: "tree.copyRef", title: "Copy Reference", scope: "tree", keys: ["Mod+Alt+Shift+C"] },
  { id: "tree.quickDoc", title: "Quick Documentation", scope: "tree", keys: ["F1"] },
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

// ───────── 키 표기 ─────────

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

/** 키 입력 → 표기. 수식키만 누른 경우는 null */
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

/** 키 표기의 글자: Shift 없는 ⌘/⌃/⌥(Ctrl+/Alt+) + 글자 하나는 소문자 ("⌘k"). 대문자는 Shift 를 눌러야 하는 키 ("⇧⌘A") */
export function keyLabel(s: string): string {
  return s
    .replace(/([⌃⌥⇧⌘]+)([A-Z])(?![A-Za-z0-9])/g, (m, mods: string, k: string) => (mods.includes("⇧") ? m : mods + k.toLowerCase()))
    .replace(/\b((?:(?:Ctrl|Alt|Shift|Win|Cmd)\+)+)([A-Z])(?![A-Za-z0-9])/g, (m, mods: string, k: string) => (mods.includes("Shift+") ? m : mods + k.toLowerCase()));
}

/** 화면 표시: macOS 는 ⌃⌥⇧⌘ 기호, 그 밖은 Ctrl+Alt+Shift+ (글자는 keyLabel 규칙) */
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

/** 새로 지정할 수 있는 키: 수식키 조합이나 F 키 (그 밖은 글자 입력·이동과 겹친다. tree 의 Delete 기본값은 Reset 으로 되돌린다) */
export function bindable(spec: string): boolean {
  return /^(Mod|Ctrl|Alt)\+/.test(spec) || /(^|\+)F\d{1,2}$/.test(spec);
}

// ───────── 사용자 설정 ─────────

let user: Record<string, string[]> = {};
const listeners = new Set<() => void>();

export function bindings(id: string): string[] {
  return user[id] ?? byId.get(id)?.keys ?? [];
}

/** 현재 키 표시 ("⇧⌘L"). 키가 없으면 "" */
export function shortcutLabel(id: string): string {
  return bindings(id).map(formatSpec).join(" / ");
}

export function isCustom(id: string): boolean {
  return id in user;
}

/** 이 키 입력이 명령의 단축키인지 */
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

/** 같은 키를 쓰는(범위가 겹치는) 다른 명령 */
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

/** 키를 지정한다 (add = 기존 키에 더하기). 겹치는 다른 명령에서는 그 키를 뺀다. 반환: 키를 뺀 명령 */
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
