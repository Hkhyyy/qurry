// Color themes. The palettes and their tokens are crates/shared/src/themes.json, the table the TUI draws with too (qurry_shared::theme);
// the pick, Sync with OS and custom themes are in settings.json, shared with the TUI (theme_settings). Applying a theme sets each
// token's CSS variables on <html> (below), data-theme to its kind for the styles that differ by kind, and Monaco's theme (monaco.ts).
import { useSyncExternalStore, type CSSProperties } from "react";
import TABLE from "../../crates/shared/src/themes.json";
import { api } from "./api";

export type Kind = "dark" | "light" | "hc";
/** The kind of the theme drawn (what styles that differ by kind go by) */
export type Theme = Kind;

export interface ThemeToken {
  key: string;
  group: string;
  label: string;
  desc: string;
  /** Shown without "Advanced colors" */
  basic?: boolean;
  /** [text, background, minimum contrast] for each surface the color is drawn on: one of the two is this token */
  checks?: [string, string, number][];
}
export const TOKENS = TABLE.tokens as ThemeToken[];
/** The TUI's tokens (crates/tui/src/kit/atoms.rs) and the theme token each one draws with */
export const TUI_TOKENS = TABLE.tui as Record<string, string>;

export type Palette = Record<string, string>;
export interface ThemeDef {
  id: string;
  name: string;
  kind: Kind;
  custom: boolean;
  colors: Palette;
}
/** A built-in theme and the colors changed from it (qurry_shared::theme::CustomTheme) */
export interface CustomTheme {
  id: string;
  name: string;
  base: string;
  colors: Palette;
}
export interface ThemeSync {
  dark: string;
  light: string;
}
export interface ThemeSettings {
  /** A theme id or "system"; null when never picked (Dark) */
  theme: string | null;
  sync: ThemeSync;
  custom: CustomTheme[];
}
/** A contrast pair below its minimum (qurry_shared::theme::Warning) */
export interface ThemeWarning {
  token: string;
  fg: string;
  bg: string;
  ratio: number;
  min: number;
}

export const SYSTEM = "system";
export const BUILT_IN: ThemeDef[] = TABLE.themes.map((t) => ({ ...t, kind: t.kind as Kind, custom: false }));

/**
 * Each token and the CSS variables it sets: the theme variables of App.css (--bg …, and the --c-* names the app's styles use),
 * tokens.css --ui-*, and the grid states and Data Source tints. The --ui-* that tokens.css derives from the others are set too:
 * custom properties resolve where they are declared (:root), so the editor's preview, which sets them on a wrapper, needs them
 */
export const TOKEN_VARS: Record<string, string[]> = {
  bg: ["--bg", "--ui-bg", "--c-1e1f22"],
  panel: ["--panel", "--ui-surface", "--c-2b2d30"],
  panel2: ["--panel-2", "--ui-surface-2", "--c-26282b"],
  odd: ["--c-222326"],
  hover: ["--ui-hover", "--c-33353a"],
  border: ["--border", "--ui-border", "--c-393b40"],
  borderStrong: ["--ui-border-strong", "--c-4e5157"],
  text: ["--text", "--ui-text", "--c-dfe1e5"],
  muted: ["--muted", "--ui-text-muted", "--c-868a91", "--ui-json-punct"],
  nullText: ["--c-6f737a"],
  icon: ["--ui-icon"],
  accent: ["--accent", "--ui-accent", "--c-3574f0"],
  link: ["--c-548af7", "--ui-icon-db"],
  onAccent: ["--ui-text-on-accent"],
  sel: ["--sel", "--ui-selected", "--c-2e436e"],
  keyword: ["--c-cf8e6d", "--ui-json-keyword"],
  string: ["--ui-json-string"],
  number: [],
  comment: [],
  function: ["--c-56a8f5"],
  jsonKey: ["--ui-json-key", "--c-c77dbb"],
  gridNum: ["--c-2aacb8", "--ui-json-number"],
  changed: ["--grid-changed"],
  added: ["--grid-added"],
  deleted: ["--grid-deleted"],
  match: ["--grid-match"],
  prodTint: ["--theme-prod-tint"],
  prodStripe: ["--theme-prod-stripe"],
  tint: ["--theme-tint"],
  ok: ["--ok", "--ui-ok", "--c-5fb865"],
  warn: ["--ui-warn", "--c-e5a64b"],
  danger: ["--danger", "--ui-danger"],
};

/** A palette as inline CSS variables (the editor's preview) */
export function themeVars(p: Palette): CSSProperties {
  const v: Record<string, string> = {};
  for (const [k, names] of Object.entries(TOKEN_VARS)) for (const n of names) v[n] = p[k];
  return v as CSSProperties;
}

/** A built-in or custom theme by id: a custom one is its base with its colors over it */
export function findTheme(s: ThemeSettings, id: string): ThemeDef | undefined {
  const b = BUILT_IN.find((t) => t.id === id);
  if (b) return b;
  const c = s.custom.find((t) => t.id === id);
  const base = c && BUILT_IN.find((t) => t.id === c.base);
  if (!c || !base) return undefined;
  const colors = { ...base.colors };
  for (const [k, v] of Object.entries(c.colors)) if (k in colors) colors[k] = v;
  return { id: c.id, name: c.name, kind: base.kind, custom: true, colors };
}

/** The built-in themes, then the custom ones */
export const allThemes = (s: ThemeSettings): ThemeDef[] => [...BUILT_IN, ...s.custom.map((c) => findTheme(s, c.id)).filter((t): t is ThemeDef => !!t)];

/** The theme to draw: Dark when none was picked, Sync with OS by the system appearance; a theme that no longer exists is Dark */
export function resolveTheme(s: ThemeSettings, osDark: boolean): ThemeDef {
  const pick = s.theme ?? "dark";
  return findTheme(s, pick === SYSTEM ? (osDark ? s.sync.dark : s.sync.light) : pick) ?? BUILT_IN[0];
}

/** Before 0.3.2 the GUI kept its pick in the webview (localStorage); moved to settings.json once */
const OLD_KEY = "dbtool.theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");
let settings: ThemeSettings = { theme: null, sync: { dark: "dark", light: "light" }, custom: [] };
/** The editor's live preview: drawn instead of the pick until it ends */
let preview: ThemeDef | null = null;
let current: ThemeDef = resolveTheme(settings, media.matches);
let snapshot = { settings, current };
const subs = new Set<() => void>();

function apply() {
  current = preview ?? resolveTheme(settings, media.matches);
  snapshot = { settings, current };
  const root = document.documentElement;
  root.dataset.theme = current.kind;
  // Themes other than the three the app's styles were tuned for derive its secondary grays and blues from their tokens (App.css)
  if (["dark", "light", "hc"].includes(current.id)) delete root.dataset.derived;
  else root.dataset.derived = "";
  for (const [k, names] of Object.entries(TOKEN_VARS)) for (const n of names) root.style.setProperty(n, current.colors[k]);
  // The native title bar follows the window theme, which follows the OS unless set (white over a dark app)
  const native = settings.theme === SYSTEM && !preview ? null : current.kind === "light" ? "light" : "dark";
  import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(native))
    .catch(() => {});
  subs.forEach((f) => f());
}
media.addEventListener("change", () => settings.theme === SYSTEM && apply());
apply();

/**
 * Read the shared settings (before the app renders, so it starts in its theme). The first start after 0.3.2 moves the pick the webview
 * kept into them, unless settings.json has one (the TUI's, or the GUI's on another machine). When something goes wrong the status bar
 * says so at start, with how to recover (takeThemeNotice)
 */
export async function initTheme() {
  let old: string | null = null;
  try {
    old = localStorage.getItem(OLD_KEY);
  } catch {
    /* No webview storage: nothing to move */
  }
  const valid = old === SYSTEM || BUILT_IN.some((t) => t.id === old);
  try {
    settings = await api.themeSettings();
  } catch (e) {
    if (valid) settings = { ...settings, theme: old };
    notice = `Couldn't read the theme settings (${e}): ${valid ? "your theme" : "Dark"} for now. Make settings.json in the config folder readable (or delete it), then restart`;
    return apply();
  }
  if (settings.theme === null && old && valid) {
    settings = { ...settings, theme: old };
    try {
      await api.setTheme(old);
      localStorage.removeItem(OLD_KEY);
    } catch (e) {
      notice = `Couldn't move your theme to settings.json (${e}): it is used for now. Pick it again in Settings › Appearance to keep it`;
    }
  }
  apply();
}

let notice: string | null = null;
/** What initTheme has to say in the status bar (once) */
export function takeThemeNotice() {
  const n = notice;
  notice = null;
  return n;
}

const save = (p: Promise<unknown>): Promise<void> =>
  p.then(() => {}, (e) => {
    throw `Couldn't save the theme (${e}): it is used until Qurry quits. Pick it again in Settings › Appearance to keep it`;
  });

/** Pick a theme (an id, or "system") for both apps */
export function setThemePick(id: string): Promise<void> {
  settings = { ...settings, theme: id };
  apply();
  return save(api.setTheme(id));
}

/** Sync with OS: the theme for a dark and for a light system appearance */
export function setThemeSync(sync: ThemeSync): Promise<void> {
  settings = { ...settings, sync };
  apply();
  return save(api.setThemeSync(sync));
}

/** Add (empty id) or replace a custom theme; the saved one (with its id) */
export async function saveCustomTheme(t: CustomTheme): Promise<CustomTheme> {
  const saved = await api.saveCustomTheme(t);
  settings = await api.themeSettings();
  apply();
  return saved;
}

export async function deleteCustomTheme(id: string): Promise<void> {
  await api.deleteCustomTheme(id);
  settings = await api.themeSettings();
  apply();
}

/** Draw a theme for now (the editor's live preview), or null to go back to the pick */
export function previewTheme(t: ThemeDef | null) {
  preview = t;
  apply();
}

export const getThemeSettings = () => settings;
export const currentTheme = () => current;

/** The settings and the theme drawn (re-renders on change) */
export function useThemeState(): { settings: ThemeSettings; current: ThemeDef } {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => snapshot,
  );
}
/** The kind of the theme drawn (re-renders on change) */
export const useTheme = (): Theme => useThemeState().current.kind;
/** Subscribe to every change of the theme drawn (monaco.ts redefines its theme) */
export const onTheme = (f: (t: ThemeDef) => void) => {
  const g = () => f(current);
  subs.add(g);
  g();
  return () => subs.delete(g);
};

/** Monaco has one theme for every editor: monaco.ts defines this one from the theme drawn */
export const MONACO_THEME = "qurry";

/**
 * Monaco's theme for a palette (SQL consoles, the Value Editor's JSON). The base themes have their own rules for some exact tokens,
 * more specific than "string" or "predefined", which would win over ours: the pgsql / mysql tokenizers' string.sql and predefined.sql
 * (#FF0000 / #FF00FF in vs and vs-dark) and JSON's values and keywords — each gets the theme's color too
 */
export function monacoThemeData(t: ThemeDef) {
  const p = t.colors;
  const fg = (k: string) => p[k].slice(1);
  return {
    base: (t.kind === "light" ? "vs" : t.kind === "hc" ? "hc-black" : "vs-dark") as "vs" | "vs-dark" | "hc-black",
    inherit: true,
    rules: [
      { token: "keyword", foreground: fg("keyword") },
      { token: "string", foreground: fg("string") },
      { token: "string.sql", foreground: fg("string") },
      { token: "number", foreground: fg("number") },
      { token: "comment", foreground: fg("comment"), fontStyle: "italic" },
      { token: "predefined", foreground: fg("function") },
      { token: "predefined.sql", foreground: fg("function") },
      { token: "string.key.json", foreground: fg("jsonKey") },
      { token: "string.value.json", foreground: fg("string") },
      { token: "keyword.json", foreground: fg("keyword") },
    ],
    colors: {
      "editor.background": p.bg,
      "editor.foreground": p.text,
      "editor.lineHighlightBackground": p.panel2,
      "editorLineNumber.foreground": p.muted,
      "editorLineNumber.activeForeground": p.text,
      "editor.selectionBackground": p.sel,
    },
  };
}

// The Data Source color presets the theme has tokens for (lib/api.ts COLOR_PRESETS Red, the Production color, and Green)
const RED = "#6e2a2a";
const GREEN = "#2f5634";
const is = (a: string, b: string) => a.toLowerCase() === b;
/** Connection color (tree · tab background): Red and Green take the theme's tints, the others are mixed into the panels as the
 *  theme's kind says (App.css --conn-tint-*) — text stays readable, the strong color goes in the left stripe (connStripe) */
export const connBg = (color: string | null | undefined) =>
  !color ? undefined : is(color, RED) ? "var(--theme-prod-tint)" : is(color, GREEN) ? "var(--theme-tint)" : `color-mix(in srgb, ${color} var(--conn-tint-amount), var(--conn-tint-base))`;
/** Connection color stripe (tree left · tab top) — spot production DBs at a glance */
export const connStripe = (color: string | null | undefined) => (!color ? undefined : is(color, RED) ? "var(--theme-prod-stripe)" : `color-mix(in srgb, ${color} 75%, #ffffff)`);
/** Editor tab colors as CSS variables: App.css draws the stripe on every tab, the full tint on the active one and a fainter one on the rest */
export const connTabVars = (color: string | null | undefined) => (!color ? undefined : ({ "--conn-bg": connBg(color), "--conn-stripe": connStripe(color) } as CSSProperties));
