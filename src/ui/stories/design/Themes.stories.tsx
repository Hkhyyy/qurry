import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
// Fake backend (same as the app's ?mock): the preview is the real app on fixed data
import "../../../lib/mock";
import { monaco } from "../../../lib/monaco";
import App from "../../../App";
import { useTabsStore } from "../../../features/tabs/store";
import { TuiScreen, type Run, type Scene } from "../TuiScreen";
import tuiSnapshots from "../tui-snapshots.json";
import { Button, Notice, Select, Text, TextInput, normalizeHex } from "../../index";
import { ResultGrid } from "../../../components/ResultGrid";
import type { QueryResult } from "../../../lib/api";

// Design for 0.3.2, #103: more built-in themes (Dracula, One Dark, Solarized Dark / Light), custom themes, Sync with OS per theme.
// Not product code: replace it with the real components when the issue is built, then delete this story.
const meta: Meta = { title: "Design/0.3.2/Themes (#103)", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

// ───────── Tokens ─────────
// Each editable token and the real CSS variables it sets (App.css :root / [data-theme], tokens.css --ui-*, grid.css --c-*).
// --ui-* are set too: tokens.css resolves var(--bg) etc. on :root, so a wrapper has to set them itself.
// --monaco-* stand for the Monaco theme rules (lib/monaco.ts: keyword, string, number, comment, predefined).
// --grid-changed / --grid-added / --grid-deleted / --grid-match are new: grid.css has them as fixed rgba() today, which don't adapt.
type Key =
  | "bg" | "panel" | "panel2" | "odd" | "hover" | "border" | "borderStrong"
  | "text" | "muted" | "nullText" | "icon"
  | "accent" | "link" | "onAccent" | "sel"
  | "keyword" | "string" | "number" | "comment" | "function" | "jsonKey"
  | "gridNum" | "changed" | "added" | "deleted" | "match"
  | "prodTint" | "prodStripe" | "tint"
  | "ok" | "warn" | "danger";
type Palette = Record<Key, string>;

/** fg on bg must reach min (WCAG ratio); the token the row belongs to is one of the two */
interface Tok { key: Key; label: string; vars: string[]; check?: [Key, Key, number] }
const GROUPS: { name: string; tokens: Tok[] }[] = [
  { name: "Background & surfaces", tokens: [
    { key: "bg", label: "Background, editor", vars: ["--bg", "--ui-bg", "--c-1e1f22"] },
    { key: "panel", label: "Panels, grid header, status bar", vars: ["--panel", "--ui-surface", "--c-2b2d30"] },
    { key: "panel2", label: "Row numbers, gutters", vars: ["--panel-2", "--ui-surface-2", "--c-26282b"] },
    { key: "odd", label: "Odd grid rows", vars: ["--c-222326"] },
    { key: "hover", label: "Hover", vars: ["--ui-hover", "--c-33353a", "--hover-tint"] },
    { key: "border", label: "Borders", vars: ["--border", "--ui-border", "--c-393b40"] },
    { key: "borderStrong", label: "Strong borders", vars: ["--ui-border-strong", "--c-4e5157"] },
  ] },
  { name: "Text", tokens: [
    { key: "text", label: "Text", vars: ["--text", "--ui-text", "--c-dfe1e5"], check: ["text", "bg", 4.5] },
    { key: "muted", label: "Dim text", vars: ["--muted", "--ui-text-muted", "--c-868a91", "--ui-json-punct"], check: ["muted", "bg", 3] },
    { key: "nullText", label: "NULL", vars: ["--c-6f737a"], check: ["nullText", "bg", 3] },
    { key: "icon", label: "Icons", vars: ["--ui-icon"], check: ["icon", "panel", 3] },
  ] },
  { name: "Accent & selection", tokens: [
    { key: "accent", label: "Accent", vars: ["--accent", "--ui-accent", "--c-3574f0"], check: ["accent", "bg", 3] },
    { key: "link", label: "Accent text, DB icons, TUI focus", vars: ["--c-548af7", "--ui-icon-db", "--c-9fbfff"], check: ["link", "bg", 4.5] },
    { key: "onAccent", label: "Text on accent", vars: ["--ui-text-on-accent"], check: ["onAccent", "accent", 4.5] },
    { key: "sel", label: "Selection", vars: ["--sel", "--ui-selected", "--c-2e436e"], check: ["text", "sel", 4.5] },
  ] },
  { name: "Syntax", tokens: [
    { key: "keyword", label: "Keyword", vars: ["--monaco-keyword", "--c-cf8e6d", "--ui-json-keyword"], check: ["keyword", "bg", 4.5] },
    { key: "string", label: "String", vars: ["--monaco-string", "--ui-json-string"], check: ["string", "bg", 4.5] },
    { key: "number", label: "Number", vars: ["--monaco-number"], check: ["number", "bg", 4.5] },
    { key: "comment", label: "Comment", vars: ["--monaco-comment"], check: ["comment", "bg", 3] },
    { key: "function", label: "Function", vars: ["--monaco-predefined", "--c-56a8f5"], check: ["function", "bg", 4.5] },
    { key: "jsonKey", label: "JSON key", vars: ["--ui-json-key", "--c-c77dbb"], check: ["jsonKey", "bg", 4.5] },
  ] },
  { name: "Grid states", tokens: [
    { key: "gridNum", label: "Numbers", vars: ["--c-2aacb8", "--ui-json-number"], check: ["gridNum", "bg", 4.5] },
    { key: "changed", label: "Edited cell", vars: ["--grid-changed"], check: ["text", "changed", 4.5] },
    { key: "added", label: "Added row", vars: ["--grid-added"], check: ["text", "added", 4.5] },
    { key: "deleted", label: "Deleted row", vars: ["--grid-deleted"], check: ["danger", "deleted", 4.5] },
    { key: "match", label: "Find match", vars: ["--grid-match"], check: ["text", "match", 4.5] },
  ] },
  { name: "Data source tints / Production", tokens: [
    { key: "prodTint", label: "Production (red) tint", vars: ["--theme-prod-tint"], check: ["text", "prodTint", 4.5] },
    { key: "prodStripe", label: "Production stripe", vars: ["--theme-prod-stripe"], check: ["prodStripe", "panel", 3] },
    { key: "tint", label: "Other data source (green) tint", vars: ["--theme-tint"], check: ["text", "tint", 4.5] },
  ] },
  { name: "Status colors", tokens: [
    { key: "ok", label: "OK", vars: ["--ok", "--ui-ok", "--c-5fb865"], check: ["ok", "panel", 3] },
    { key: "warn", label: "Warning", vars: ["--ui-warn", "--c-e5a64b"], check: ["warn", "panel", 3] },
    { key: "danger", label: "Danger, errors", vars: ["--danger", "--ui-danger"], check: ["danger", "panel", 3] },
  ] },
];
const TOKENS = GROUPS.flatMap((g) => g.tokens);
const LABEL = Object.fromEntries(TOKENS.map((t) => [t.key, t.label])) as Record<Key, string>;
/** What each token colors, in plain words (shown under the token in the editor) */
const DESC: Record<Key, string> = {
  bg: "Editor and result grid background",
  panel: "Side panels, tab bar, grid header and status bar",
  panel2: "Row-number column of the result grid",
  odd: "Background of every other result row",
  hover: "A button or tree row under the mouse pointer",
  border: "Lines between panels, tabs and grid cells",
  borderStrong: "Edges of buttons, inputs, popups and dialogs",
  text: "Ordinary text: code, cell values, names",
  muted: "Secondary text: headers, hints, line numbers, inactive tabs",
  nullText: "NULL in result cells",
  icon: "Tree icons of tables and other objects",
  accent: "Active tab underline, focus outline, primary button, running indicator",
  link: "Data source icons, the sorted column's name, the TUI's focused border",
  onAccent: "Text on primary buttons",
  sel: "Selected cell, row or list item",
  keyword: "SQL keywords like SELECT, and true / false / null in JSON",
  string: "Quoted text in SQL and JSON",
  number: "Numbers in SQL code",
  comment: "Comments in SQL code",
  function: "Function names like upper()",
  jsonKey: "Keys in JSON values",
  gridNum: "Numbers in the result grid and the TUI",
  changed: "A cell you edited but haven't submitted yet",
  added: "A new row not submitted yet",
  deleted: "A row marked for deletion (its text uses the error color)",
  match: "Cells that match the grid's Find",
  prodTint: "Background of a Production data source in the tree and its tabs",
  prodStripe: "Thin red edge marking Production rows and tabs",
  tint: "Background of a data source given another color (here green)",
  ok: "Success messages and checks",
  warn: "Warnings, pending changes, the PROD label",
  danger: "Errors and rows marked for deletion",
};
/** Shown by default; the rest is behind "Advanced colors" */
const BASIC: Key[] = ["bg", "panel", "text", "muted", "accent", "sel", "keyword", "string", "number", "comment", "prodTint"];

// ───────── Color math ─────────
const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const hex = (c: number[]) => "#" + c.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
/** = CSS color-mix(in srgb, a t, b) */
const mix = (a: string, b: string, t: number) => hex(rgb(a).map((v, i) => v * t + rgb(b)[i] * (1 - t)));
const lum = (h: string) => {
  const [r, g, b] = rgb(h).map((v) => ((v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
/** WCAG 2 contrast ratio */
const ratio = (a: string, b: string) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};

// ───────── Themes ─────────
type Kind = "dark" | "light" | "hc";
interface ThemeDef { id: string; name: string; kind: Kind; custom?: boolean; p: Palette }
type Core = Pick<Palette, "bg" | "panel" | "panel2" | "border" | "text" | "muted" | "accent" | "sel" | "ok" | "warn" | "danger" | "keyword" | "string" | "number" | "comment" | "function" | "jsonKey"> & Partial<Palette>;

// The Data Source color presets the tints are made of (lib/api.ts COLOR_PRESETS Red / Green), mixed like lib/theme.ts connBg / connStripe
const RED = "#6e2a2a";
const GREEN = "#2f5634";
const tintOf = (c: string, kind: Kind, panel: string) => (kind === "light" ? mix(c, "#ffffff", 0.22) : kind === "hc" ? c : mix(c, panel, 0.35));

/** Fill the tokens a theme doesn't set from the ones it does */
function mk(id: string, name: string, kind: Kind, c: Core): ThemeDef {
  const p: Palette = {
    odd: mix(c.text, c.bg, 0.025),
    hover: mix(c.text, c.panel, 0.07),
    borderStrong: mix(c.text, c.border, 0.15),
    nullText: c.muted,
    icon: mix(c.text, c.muted, 0.5),
    link: c.accent,
    onAccent: ratio("#ffffff", c.accent) >= ratio(c.bg, c.accent) ? "#ffffff" : c.bg,
    gridNum: c.number,
    changed: mix(c.accent, c.bg, 0.22),
    added: mix(c.ok, c.bg, 0.12),
    deleted: mix(c.danger, c.bg, 0.08),
    match: mix(c.warn, c.bg, 0.28),
    prodTint: tintOf(RED, kind, c.panel),
    prodStripe: mix(RED, "#ffffff", 0.75),
    tint: tintOf(GREEN, kind, c.panel),
    ...c,
  };
  return { id, name, kind, p };
}

const BUILT_IN: ThemeDef[] = [
  // Today's values (App.css, tokens.css, grid.css, lib/monaco.ts)
  mk("dark", "Dark", "dark", {
    bg: "#1e1f22", panel: "#2b2d30", panel2: "#26282b", odd: "#222326", hover: "#33353a", border: "#393b40", borderStrong: "#4e5157",
    text: "#dfe1e5", muted: "#868a91", nullText: "#6f737a", icon: "#ced0d6",
    accent: "#3574f0", link: "#548af7", onAccent: "#ffffff", sel: "#2e436e",
    keyword: "#cf8e6d", string: "#6aab73", number: "#2aacb8", comment: "#7a7e85", function: "#56a8f5", jsonKey: "#c77dbb", gridNum: "#2aacb8",
    ok: "#5fb865", warn: "#e5a64b", danger: "#f75464",
  }),
  mk("light", "Light", "light", {
    bg: "#ffffff", panel: "#f7f8fa", panel2: "#f2f3f5", odd: "#fafafa", hover: "#ebecee", border: "#ebecf0", borderStrong: "#c9ccd6",
    text: "#1f2023", muted: "#6c707e", nullText: "#818594", icon: "#6c707e",
    accent: "#3574f0", link: "#3574f0", onAccent: "#ffffff", sel: "#d5e1ff",
    keyword: "#0033b3", string: "#067d17", number: "#1750eb", comment: "#8c8c8c", function: "#00627a", jsonKey: "#871094", gridNum: "#1750eb",
    ok: "#208a3c", warn: "#b06f00", danger: "#e55765",
  }),
  mk("hc", "High contrast", "hc", {
    bg: "#000000", panel: "#0a0a0a", panel2: "#000000", odd: "#000000", hover: "#1a1a1a", border: "#8a8a8a", borderStrong: "#9a9a9a",
    text: "#ffffff", muted: "#c8c8c8", nullText: "#b0b0b0", icon: "#ced0d6",
    accent: "#5fa0ff", link: "#5fa0ff", onAccent: "#ffffff", sel: "#1f4fbf",
    keyword: "#ffb86c", string: "#8df08d", number: "#6fe8f5", comment: "#b0b0b0", function: "#ffffff", jsonKey: "#f5a8f0", gridNum: "#2aacb8",
    ok: "#6fe07a", warn: "#e5a64b", danger: "#ff6b78",
  }),
  // draculatheme.com/contribute: Background #282a36, Current Line / Selection #44475a, Foreground #f8f8f2, Comment #6272a4,
  // Cyan #8be9fd, Green #50fa7b, Orange #ffb86c, Pink #ff79c6, Purple #bd93f9, Red #ff5555, Yellow #f1fa8c; panels = its #21222c / #191a21
  mk("dracula", "Dracula", "dark", {
    bg: "#282a36", panel: "#21222c", panel2: "#191a21", border: "#44475a",
    text: "#f8f8f2", muted: "#6272a4", accent: "#bd93f9", link: "#bd93f9", sel: "#44475a",
    keyword: "#ff79c6", string: "#f1fa8c", number: "#bd93f9", comment: "#6272a4", function: "#50fa7b", jsonKey: "#8be9fd", gridNum: "#bd93f9",
    ok: "#50fa7b", warn: "#ffb86c", danger: "#ff5555",
  }),
  // Atom One Dark (atom/one-dark-ui, one-dark-syntax): editor #282c34, panels #21252b, border #181a1f, text #abb2bf, dim #7f848e,
  // selection #3e4451, comment #5c6370, red #e06c75, green #98c379, yellow #e5c07b, orange #d19a66, blue #61afef, purple #c678dd
  mk("onedark", "One Dark", "dark", {
    bg: "#282c34", panel: "#21252b", panel2: "#1e2227", border: "#181a1f",
    text: "#abb2bf", muted: "#7f848e", accent: "#4d78cc", link: "#61afef", sel: "#3e4451",
    keyword: "#c678dd", string: "#98c379", number: "#d19a66", comment: "#5c6370", function: "#61afef", jsonKey: "#e06c75", gridNum: "#d19a66",
    ok: "#98c379", warn: "#e5c07b", danger: "#e06c75",
  }),
  // Solarized (ethanschoonover.com/solarized): base03 #002b36, base02 #073642, base01 #586e75, base00 #657b83, base0 #839496,
  // base1 #93a1a1, base2 #eee8d5, base3 #fdf6e3; yellow #b58900, orange #cb4b16, red #dc322f, magenta #d33682, violet #6c71c4,
  // blue #268bd2, cyan #2aa198, green #859900. Dark: text base0, dim base01; light: text base00, dim base1 (the official roles)
  mk("solarized-dark", "Solarized Dark", "dark", {
    bg: "#002b36", panel: "#073642", panel2: "#00252f", border: "#0d4654",
    text: "#839496", muted: "#586e75", accent: "#268bd2", link: "#268bd2", sel: "#274642",
    keyword: "#859900", string: "#2aa198", number: "#d33682", comment: "#586e75", function: "#268bd2", jsonKey: "#b58900", gridNum: "#d33682",
    ok: "#859900", warn: "#b58900", danger: "#dc322f",
  }),
  mk("solarized-light", "Solarized Light", "light", {
    bg: "#fdf6e3", panel: "#eee8d5", panel2: "#f5efdc", border: "#e0d9c4",
    text: "#657b83", muted: "#93a1a1", accent: "#268bd2", link: "#268bd2", sel: "#e3dcc5",
    keyword: "#859900", string: "#2aa198", number: "#d33682", comment: "#93a1a1", function: "#268bd2", jsonKey: "#b58900", gridNum: "#d33682",
    ok: "#859900", warn: "#b58900", danger: "#dc322f",
  }),
];
const EXISTING = new Set(["dark", "light", "hc"]);
/** Saved custom themes in the mocks: a base + the overrides */
const CUSTOM: ThemeDef[] = [
  { id: "c-night", name: "Night shift", kind: "dark", custom: true, p: { ...BUILT_IN[3].p, bg: "#1d1e26", accent: "#9580ff", link: "#9580ff" } },
  { id: "c-paper", name: "Paper", kind: "light", custom: true, p: { ...BUILT_IN[6].p, text: "#3d4f55", muted: "#6f7f80" } },
];
const ALL = [...BUILT_IN, ...CUSTOM];

/** The inline CSS variables that make the app under an element draw a palette */
function themeVars(p: Palette): CSSProperties {
  const v: Record<string, string> = {};
  for (const t of TOKENS) for (const name of t.vars) v[name] = p[t.key];
  return { ...v, background: p.bg, color: p.text } as CSSProperties;
}

// ───────── Preview: the real app ─────────
// The real App (with the ?mock data) and a real TUI snapshot, drawn with the palette. Nothing here is drawn by hand.
const mono: CSSProperties = { fontFamily: "var(--ui-font-mono)", fontSize: 12 };

/** The Monaco theme for a palette (lib/monaco.ts dbtool-dark with the palette's colors). Monaco has one theme for every editor */
function setMonacoTheme(p: Palette) {
  const fg = (c: string) => c.slice(1);
  monaco.editor.defineTheme("qurry-preview", {
    base: lum(p.bg) > 0.4 ? "vs" : "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: fg(p.keyword) },
      { token: "string", foreground: fg(p.string) },
      { token: "number", foreground: fg(p.number) },
      { token: "comment", foreground: fg(p.comment), fontStyle: "italic" },
      { token: "predefined", foreground: fg(p.function) },
      { token: "string.key.json", foreground: fg(p.jsonKey) },
      // The base themes color pgsql strings and functions #FF0000 / #FF00FF, over the rules above (the app has that today; #103 fixes it)
      { token: "string.sql", foreground: fg(p.string) },
      { token: "predefined.sql", foreground: fg(p.function) },
    ],
    colors: {
      "editor.background": p.bg,
      "editor.foreground": p.text,
      "editor.lineHighlightBackground": p.panel2,
      "editorLineNumber.foreground": p.muted,
      "editorLineNumber.activeForeground": p.text,
      "editor.selectionBackground": p.sel,
    },
  });
  monaco.editor.setTheme("qurry-preview");
}

/** Waits for f to return something (the mock app renders in steps) */
async function until<T>(f: () => T | null | undefined): Promise<T> {
  for (let i = 0; i < 100; i++) {
    const v = f();
    if (v) return v;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("the preview app didn't get ready");
}

// Three statements, run as a whole: a function, a string and a number; the editable users result; one that fails (✓ and ✗ marks)
const PREVIEW_SQL = "-- users to check this week\nselect upper(user_name) as name, 100 as n from users where user_name <> 'lee';\nselect * from users limit 4;\nselect * from no_such_table;";

const noop = () => {};
// The zoomed app trips Chrome's harmless "ResizeObserver loop" error, which the app would show in its status bar
window.addEventListener("error", (e) => e.message?.includes("ResizeObserver loop") && e.stopImmediatePropagation(), true);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** Mouse down + up + click, as a click on the element */
const press = (el: HTMLElement, init: MouseEventInit = {}) => {
  for (const type of ["mousedown", "mouseup", "click"]) el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, ...init }));
};
/** Types into a React input (the value setter, then an input event) */
const typeInto = (input: HTMLInputElement, text: string) => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
};

/**
 * Brings the app to a state where every token shows, as a user would (e2e/app.ts): a console on [DEV] shop_app runs PREVIEW_SQL,
 * then in the users result (4 rows) one cell is edited, a row deleted, one added, "user_4" found and a cell selected
 */
async function stageApp(root: HTMLElement, live: () => boolean) {
  const node = (text: string) => [...root.querySelectorAll<HTMLElement>(".ws-explorer.on .tree-node")].find((n) => n.textContent?.includes(text));
  const dev = await until(() => node("[DEV] shop_app"));
  // React's StrictMode mounts the preview twice in development: only the second one stages
  if (!live()) return;
  press(dev);
  // The shortcut handler sees the selection after the app re-renders; it reads the event target, so the key comes from the row
  await sleep(300);
  dev.dispatchEvent(new KeyboardEvent("keydown", { key: "L", code: "KeyL", metaKey: true, shiftKey: true, bubbles: true }));
  const ed = await until(() => monaco.editor.getEditors().find((e) => root.contains(e.getContainerDomNode())));
  ed.setValue(PREVIEW_SQL);
  // Select everything and run (e2e/app.ts runAll): each statement runs
  ed.setSelection(ed.getModel()!.getFullModelRange());
  ed.trigger("preview", "dbtool.run", null);
  await until(() => root.querySelector(".stmt-mark.error"));
  // The users result (the second)
  const tabs = await until(() => { const t = root.querySelectorAll<HTMLElement>(".result-tabs [role=tab]"); return t.length >= 3 ? t : null; });
  press([...tabs].find((t) => t.textContent?.includes("select * from users")) ?? tabs[2]);
  const cell = (text: string) => until(() => [...root.querySelectorAll<HTMLElement>(".work-layer .grid-row .cell")].find((c) => c.textContent === text));
  const button = (title: string) => until(() => [...root.querySelectorAll<HTMLButtonElement>(".work-layer button[title]")].find((b) => b.title.startsWith(title) && b.offsetParent));
  // Edit user_2 → kim
  (await cell("user_2")).dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  const input = await until(() => root.querySelector<HTMLInputElement>(".cell-input"));
  typeInto(input, "kim");
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await sleep(100);
  // Delete the user_3 row, add one
  press(await cell("user_3"));
  await sleep(100);
  press(await button("Delete Row"));
  press(await button("Add Row"));
  await sleep(200);
  // Find user_4
  press(await button("Find on Current Page"));
  const find = await until(() => root.querySelector<HTMLInputElement>('input[placeholder="Find on current page"]'));
  typeInto(find, "user_4");
  await sleep(100);
  // A selected cell
  press(await cell("user_1"));
  // The pointer over a table in the explorer
  node("users")?.classList.add("preview-hover");
}

/**
 * What #103 changes in the app CSS, so the preview follows the palette where the app has fixed colors today:
 * the grid states get variables, the data source tints come from the theme. preview-hover: the hover look without a pointer
 */
const PREVIEW_CSS = `
.theme-preview .grid-row.added .cell:not(.rownum) { background: var(--grid-added); }
.theme-preview .grid-row.deleted .cell:not(.rownum) { background: var(--grid-deleted); }
.theme-preview .cell.changed { background: var(--grid-changed); }
.theme-preview .cell.match { background: var(--grid-match); }
.theme-preview .conn-tree:has([data-node="prod"]) { background: var(--theme-prod-tint) !important; box-shadow: inset 3px 0 0 var(--theme-prod-stripe) !important; }
.theme-preview .conn-tree:has([data-node="my"]) { background: var(--theme-tint) !important; }
.theme-preview .tree-node.preview-hover { background: var(--hover-tint); }
.theme-preview .tok-hot { outline: 1px dashed var(--text); outline-offset: -1px; }
`;

/** A jsonb result (the app's ?mock payments rows): JSON keys, strings, numbers and null in the cells */
const JSON_RESULT: QueryResult = {
  columns: [{ name: "id", typeName: "INT8" }, { name: "status", typeName: "VARCHAR" }, { name: "data", typeName: "JSONB" }],
  rows: [1, 2, 3].map((i) => [i, i % 2 ? "paid" : "ready", `{"kind": "card", "live": ${i % 2 === 0}, "meta": {"src": "web", "coupon": null}, "items": [${i}, ${i + 1}]}`]),
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 3,
  hasResultSet: true,
};

/** The TUI snapshot's dark colors (crates/tui/src/kit/atoms.rs, the terminal's own fg / bg) → the token that draws them */
const TUI_TOKEN: Record<string, Key> = {
  "#abb2bf": "text", "#e6e6e6": "text", "#1e1f22": "bg", "#2b2d30": "panel", "#26282e": "panel2", "#323438": "hover",
  "#6b7079": "muted", "#868a91": "muted", "#60636a": "borderStrong", "#548af7": "link", "#6fb3d2": "link", "#2e436e": "sel",
  "#2aacb8": "gridNum", "#243454": "changed", "#5fb865": "ok", "#c9a26d": "warn", "#f75464": "danger", "#b54f4f": "danger",
  "#c77dbb": "jsonKey", "#6aab73": "string", "#cf8e6d": "keyword", "#cc7832": "keyword", "#56a8f5": "function", "#56b6c2": "function",
  "#6e2a2a": "prodTint", "#2f5634": "tint",
};
const TUI_SCENE = (tuiSnapshots as { scenes: Scene[] }).scenes.find((s) => s.name === "Console & result")!;
/** The token a TUI snapshot color stands for (the Production color as text is the stripe, as a background the tint) */
const tokenOfTui = (c: string, bg: boolean): Key | undefined => {
  const k = TUI_TOKEN[c.toLowerCase()];
  return k === "prodTint" && !bg ? "prodStripe" : k;
};
/** The TUI scene with each color swapped for its token's color in the palette */
function tuiIn(p: Palette): Scene {
  const map = (c: string | null, bg: boolean) => {
    const k = c && tokenOfTui(c, bg);
    return k ? p[k] : c;
  };
  return { ...TUI_SCENE, desc: "", rows: TUI_SCENE.rows.map((runs) => runs.map(([t, f, b, ...rest]) => [t, map(f, false), map(b, true), ...rest] as Run)) };
}

/** Two colors: a token's CSS variables are set to each for a moment, and what changes between them draws it (also through color-mix) */
const PROBES = ["#010203", "#fefdfc"];
const hasText = (el: Element) => [...el.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent!.trim());
/** What of el's look a token can be in: text (only where it has text of its own, not inherited by a box), icon strokes, background, borders, stripe, ::before mark */
function look(el: Element) {
  const s = getComputedStyle(el);
  const text = hasText(el) || el instanceof SVGElement ? s.color + s.fill + s.stroke : "";
  const borders = (["Top", "Right", "Bottom", "Left"] as const).map((d) => (s[`border${d}Width`] === "0px" ? "" : s[`border${d}Color`])).join();
  return [text, s.backgroundColor, borders, s.boxShadow, getComputedStyle(el, "::before").backgroundColor].join("|");
}
/** Monaco draws from its theme, not the CSS variables: the tokens it shows, by what they color */
const MONACO: Partial<Record<Key, string>> = {
  keyword: ".view-line span span", string: ".view-line span span", number: ".view-line span span", comment: ".view-line span span", function: ".view-line span span",
  text: ".view-line span span", muted: ".line-numbers", bg: ".monaco-editor-background",
};
/**
 * The elements that draw token k. The app's CSS: its variables are set to each probe and back in one go, so nothing is painted in between
 * and other tokens with the same value don't count. Monaco: the spans in k's color. The TUI snapshot: the runs whose color maps to k
 */
function usedBy(wrap: HTMLElement, tui: HTMLElement, k: Key, p: Palette): Element[] {
  const vars = TOKENS.find((t) => t.key === k)!.vars;
  const before = vars.map((v) => wrap.style.getPropertyValue(v));
  const els = [...wrap.querySelectorAll("*")].filter((el) => !tui.contains(el) && !el.closest(".monaco-editor") && el.checkVisibility());
  const looks = PROBES.map((c) => {
    vars.forEach((v) => wrap.style.setProperty(v, c));
    return els.map(look);
  });
  vars.forEach((v, i) => wrap.style.setProperty(v, before[i]));
  const want = `rgb(${rgb(p[k]).join(", ")})`;
  const inMonaco = MONACO[k] ? [...wrap.querySelectorAll<HTMLElement>(MONACO[k]!)].filter((el) => { const s = getComputedStyle(el); return (k === "bg" ? s.backgroundColor : s.color) === want && (k === "bg" || el.textContent!.trim()); }) : [];
  const rows = tui.querySelectorAll("pre > div");
  const inTui = TUI_SCENE.rows.flatMap((runs, y) =>
    runs.flatMap(([, f, b], i) => (([[f, false], [b, true]] as const).some(([c, bg]) => c && tokenOfTui(c, bg) === k) ? [rows[y]?.children[i]] : [])),
  );
  // An icon's path can't take an outline: its <svg> does
  const inCss = els.filter((_, i) => looks[0][i] !== looks[1][i]).map((el) => (el instanceof SVGElement && el.ownerSVGElement ? el.ownerSVGElement : el));
  return [...new Set([...inCss, ...inMonaco, ...inTui])].filter(Boolean) as Element[];
}

/**
 * The real app in the palette (staged so every token shows), a jsonb result, a warning notice; under them the TUI.
 * hot: the token being pointed at; what draws it gets a thin dashed outline (inside the element, nothing blinks or is covered).
 * zoom: the app is laid out at 1080 × 720 and scaled.
 */
function RealPreview({ p, hot, zoom = 0.7 }: { p: Palette; hot?: Key | null; zoom?: number }) {
  const wrap = useRef<HTMLDivElement>(null);
  const app = useRef<HTMLDivElement>(null);
  const tui = useRef<HTMLDivElement>(null);
  // The tabs are kept outside the app (zustand): a preview shown again starts without the last one's consoles
  useState(() => useTabsStore.setState(useTabsStore.getInitialState(), true));
  useEffect(() => {
    let live = true;
    stageApp(app.current!, () => live).catch(() => {});
    // The console editor sets the app's own Monaco theme when it mounts: set this one again after it
    const id = window.setTimeout(() => setMonacoTheme(p), 1500);
    return () => {
      live = false;
      clearTimeout(id);
    };
  }, []);
  const colors = JSON.stringify(p);
  useEffect(() => setMonacoTheme(p), [colors]);
  useLayoutEffect(() => {
    if (!hot) return;
    const els = usedBy(wrap.current!, tui.current!, hot, p);
    els.forEach((el) => el.classList.add("tok-hot"));
    return () => els.forEach((el) => el.classList.remove("tok-hot"));
  }, [hot, colors]);
  return (
    <div ref={wrap} className="theme-preview" style={{ ...themeVars(p), display: "flex", flexDirection: "column", gap: 10, background: "transparent" }}>
      <style>{PREVIEW_CSS}</style>
      <div ref={app} style={{ width: 1080 * zoom, height: 720 * zoom, border: `1px solid ${p.border}`, borderRadius: 6, overflow: "hidden", flex: "none" }}>
        <div style={{ width: 1080, height: 720, zoom }}>
          <App />
        </div>
      </div>
      <div style={{ width: 1080 * zoom, border: `1px solid ${p.border}`, borderRadius: 6, overflow: "hidden", background: p.bg }}>
        <div style={{ width: 1080, zoom, display: "flex", flexDirection: "column" }}>
          <div style={{ position: "relative", height: 132 }}>
            <ResultGrid result={JSON_RESULT} running={false} loadingMore={false} query="select id, status, data from payments" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
          </div>
          {/* Database Admin when a refresh fails (DbaReport) */}
          <Notice tone="warn">Refresh failed, showing data from 10:24:01: Session closed</Notice>
        </div>
      </div>
      <div ref={tui} style={{ zoom: zoom * 0.95 }}>
        <TuiScreen scene={tuiIn(p)} colors={{ bg: p.bg, fg: p.text, border: p.border }} />
      </div>
    </div>
  );
}

// ───────── Contrast labels ─────────
function Ratio({ fg, bg, min, label }: { fg: string; bg: string; min: number; label: string }) {
  const r = ratio(fg, bg);
  const ok = r >= min;
  return (
    <span style={{ whiteSpace: "nowrap", color: ok ? "var(--text)" : "var(--ui-warn)" }} title={`${fg} on ${bg}, needs ${min}:1`}>
      {label} {r.toFixed(2)}:1 {ok ? "✓" : "⚠"} <span style={{ color: "var(--muted)" }}>≥ {min}</span>
    </span>
  );
}
const Measures = ({ p }: { p: Palette }) => (
  <div style={{ display: "flex", gap: 14, fontSize: 12, flexWrap: "wrap" }}>
    <Ratio label="Text" fg={p.text} bg={p.bg} min={4.5} />
    <Ratio label="Dim" fg={p.muted} bg={p.bg} min={3} />
    <Ratio label="Accent" fg={p.accent} bg={p.bg} min={3} />
  </div>
);

// ───────── 1. Gallery ─────────
function Gallery() {
  const [id, setId] = useState(BUILT_IN[0].id);
  const cur = BUILT_IN.find((t) => t.id === id)!;
  return (
    <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
      <Text size="sm">
        All seven ship in 0.3.2. Pick one: below is the real app in it, staged so every color shows (a console run with ✓ and ✗, a result with an edited cell, a deleted and an added row, a find match and a selected cell, a jsonb result, a warning), then the real TUI. Under each name: WCAG contrast on the background, text ≥ 4.5:1,
        dim text and the accent (an icon / UI color) ≥ 3:1. Dark, Light and High contrast are today's values; the four new ones are shown with their official palettes, and the ones
        marked ⚠ (Solarized, a few pairs elsewhere) get adjusted colors that pass when built (#103).
      </Text>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(300px, 1fr))", gap: 8 }}>
        {BUILT_IN.map((t) => (
          <button
            key={t.id}
            onClick={() => setId(t.id)}
            aria-pressed={t.id === id}
            style={{ display: "flex", flexDirection: "column", gap: 4, padding: "8px 10px", textAlign: "left", cursor: "pointer", color: "var(--text)", background: t.id === id ? "var(--sel)" : "var(--panel)", border: `1px solid ${t.id === id ? "var(--accent)" : "var(--border)"}`, borderRadius: 6 }}
          >
            <span style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
              <span style={{ display: "inline-flex", gap: 2 }}>
                {[t.p.bg, t.p.accent, t.p.keyword].map((c, j) => (
                  <span key={j} style={{ width: 10, height: 10, borderRadius: 2, background: c, border: "1px solid var(--border)" }} />
                ))}
              </span>
              <b>{t.name}</b>
              <span style={{ color: "var(--muted)", fontSize: 11 }}>
                {EXISTING.has(t.id) ? "built in today" : "new in 0.3.2"}, {t.kind === "light" ? "light" : "dark"}
              </span>
            </span>
            <Measures p={t.p} />
          </button>
        ))}
      </div>
      <RealPreview p={cur.p} zoom={1} />
    </div>
  );
}
export const Candidates: S = {
  name: "Themes in 0.3.2",
  // The app CSS clips the page at the window height: scroll inside the story
  render: () => (
    <div style={{ height: "100vh", overflow: "auto" }}>
      <Gallery />
    </div>
  ),
};

// ───────── 2. Custom theme editor ─────────
function HexField({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  const text = draft ?? value;
  const bad = draft !== null && !normalizeHex(draft);
  return (
    <>
      <label className="ui-swatch clickable" style={{ width: 18, height: 18, background: value, flex: "none" }} title="Pick a color">
        <input type="color" value={value} onChange={(e) => (setDraft(null), onChange(e.target.value))} style={{ opacity: 0, width: 0, height: 0 }} />
      </label>
      <TextInput
        value={text}
        spellCheck={false}
        style={{ width: 78, ...mono, borderColor: bad ? "var(--danger)" : undefined }}
        title={bad ? "Not a color (#rgb or #rrggbb): not applied" : undefined}
        onChange={(e) => {
          setDraft(e.target.value);
          const h = normalizeHex(e.target.value);
          if (h) onChange(h);
        }}
        onBlur={() => setDraft(null)}
      />
    </>
  );
}

// The settings pages the app has today (Settings dialog sidebar)
const NAV = ["Appearance", "Keymap", "Passwords"];

/** Whether a token's contrast pair is below its minimum: [ratio, the other token] */
const lowContrast = (t: Tok, p: Palette): [number, Key] | null => {
  if (!t.check) return null;
  const [fg, bg, min] = t.check;
  const r = ratio(p[fg], p[bg]);
  return r < min ? [r, fg === t.key ? bg : fg] : null;
};

/** hover / leave: the pointer on the label or description; focus / blur: the row's fields; edit: the color is being changed */
type Point = "hover" | "leave" | "focus" | "blur" | "edit";

/** One token: label, a "changed" dot, the contrast warning and the plain description. Pointing at the label shows where it's used in the preview */
function TokenRow({ t, p, edited, onPoint, onChange, onReset }: { t: Tok; p: Palette; edited: boolean; onPoint: (how: Point) => void; onChange: (v: string) => void; onReset: () => void }) {
  const low = lowContrast(t, p);
  return (
    <div
      data-row={t.key}
      // Opening the color picker is editing: the color has to be seen without the outline
      onFocus={(e) => onPoint((e.target as HTMLInputElement).type === "color" ? "edit" : "focus")}
      onBlur={() => onPoint("blur")}
      onMouseDown={(e) => (e.target as HTMLElement).closest(".ui-swatch") && onPoint("edit")}
      style={{ display: "flex", alignItems: "center", gap: 6, padding: "3px 4px", margin: "0 -4px", borderRadius: 4, fontSize: 12 }}
      className="theme-token-row"
    >
      <div className="theme-token-label" onMouseEnter={() => onPoint("hover")} onMouseLeave={() => onPoint("leave")} style={{ flex: 1, minWidth: 0 }} title={`CSS: ${t.vars.join(" ") || "(Data Source color, mixed)"}`}>
        <div style={{ display: "flex", alignItems: "center", gap: 5, whiteSpace: "nowrap" }}>
          <span>{t.label}</span>
          {edited && <span title="Changed from the base theme" style={{ width: 6, height: 6, borderRadius: 3, background: "var(--accent)", flex: "none" }} />}
          {low && (
            <span style={{ color: "var(--ui-warn)", fontSize: 11, marginLeft: "auto", overflow: "hidden", textOverflow: "ellipsis" }} title={`${LABEL[t.check![0]]} on ${LABEL[t.check![1]]}: needs ${t.check![2]}:1`}>
              ⚠ {low[0].toFixed(1)}:1 on {LABEL[low[1]].split(",")[0]}
            </span>
          )}
        </div>
        <div style={{ color: "var(--muted)", fontSize: 11 }}>{DESC[t.key]}</div>
      </div>
      <HexField value={p[t.key]} onChange={onChange} />
      <button className="ui-icon-btn ui-icon-btn-sm" title="Reset to base" aria-label="Reset to base" style={{ visibility: edited ? "visible" : "hidden" }} onClick={onReset}>
        ↺
      </button>
    </div>
  );
}

const groupHead: CSSProperties = { fontSize: 11, fontWeight: 600, color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.4, padding: "8px 0 3px", borderBottom: "1px solid var(--border)", marginBottom: 3 };

function ThemeEditor() {
  const [baseId, setBaseId] = useState("dracula");
  const base = ALL.find((t) => t.id === baseId)!;
  // Starts with two edits: the text a little warmer, and a comment color too dark to read (shows the warning)
  const [over, setOver] = useState<Partial<Palette>>({ text: "#e8e8e0", comment: "#4a4f6a" });
  const [name, setName] = useState("Dracula (mine)");
  const [dark, setDark] = useState("c-night");
  const [light, setLight] = useState("solarized-light");
  const [advanced, setAdvanced] = useState(false);
  // hot: outlined in the preview; about: named in the preview title (stays while its row has focus)
  const [hot, setHot] = useState<Key | null>(null);
  const [about, setAbout] = useState<Key | null>(null);
  const focused = useRef<Key | null>(null);
  const timer = useRef<number>(undefined);
  const point = (k: Key, how: Point) => {
    clearTimeout(timer.current);
    if (how === "focus") focused.current = k;
    if (how === "blur" && focused.current === k) focused.current = null;
    if (how === "hover" || how === "focus") setAbout(k);
    if (how === "edit") setAbout(k);
    if (how === "leave" || how === "blur") setAbout(focused.current);
    // shown while pointing at the label, or for 1.5 s after focusing a row; never while the color is changed
    setHot(how === "hover" || how === "focus" ? k : null);
    if (how === "focus") timer.current = window.setTimeout(() => setHot(null), 1500);
  };
  const p: Palette = { ...base.p, ...over };
  const isEdited = (k: Key) => over[k] !== undefined && over[k] !== base.p[k];
  const set = (k: Key, v: string) => {
    setOver((o) => ({ ...o, [k]: v }));
    point(k, "edit");
  };
  const changed = TOKENS.filter((t) => isEdited(t.key)).length;
  const warnings = TOKENS.filter((t) => lowContrast(t, p)).length;
  const advancedTokens = TOKENS.filter((t) => !BASIC.includes(t.key));
  const advancedWarnings = advancedTokens.filter((t) => lowContrast(t, p)).length;
  const row = (t: Tok) => (
    <TokenRow key={t.key} t={t} p={p} edited={isEdited(t.key)} onPoint={(how) => point(t.key, how)} onChange={(v) => set(t.key, v)} onReset={() => setOver(({ [t.key]: _, ...o }) => o)} />
  );
  const option = (t: ThemeDef) => (
    <option key={t.id} value={t.id}>
      {t.name}
      {t.custom ? " (custom)" : ""}
    </option>
  );
  return (
    <div style={{ padding: 16 }}>
      <style>{`.theme-token-row:hover, .theme-token-row:focus-within { background: var(--ui-hover); }`}</style>
      <div style={{ display: "flex", width: 1440, height: 860, border: "1px solid var(--border)", borderRadius: 8, overflow: "hidden", background: "var(--bg)", boxShadow: "var(--ui-shadow-modal)" }}>
        <div style={{ width: 170, flex: "none", background: "var(--panel)", borderRight: "1px solid var(--border)", padding: "8px 0", fontSize: 13 }}>
          {NAV.map((n) => (
            <div key={n} style={{ padding: "4px 14px", background: n === "Appearance" ? "var(--sel)" : undefined }}>{n}</div>
          ))}
        </div>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--border)", display: "flex", alignItems: "center", gap: 8 }}>
            <b style={{ fontSize: 13 }}>Appearance › Theme</b>
            <span style={{ color: "var(--muted)", fontSize: 12 }}>
              {changed} {changed === 1 ? "color" : "colors"} changed from {base.name}
              {warnings > 0 && <span style={{ color: "var(--ui-warn)" }}> · ⚠ {warnings} below the contrast minimum</span>}
            </span>
          </div>
          <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
            <div style={{ width: 450, flex: "none", display: "flex", flexDirection: "column", borderRight: "1px solid var(--border)" }}>
              <div style={{ padding: "10px 16px 4px", display: "flex", alignItems: "center", gap: 8, fontSize: 12 }}>
                <span style={{ color: "var(--muted)", width: 70 }}>Based on</span>
                <Select value={baseId} onChange={(e) => (setBaseId(e.target.value), setOver({}))}>
                  {BUILT_IN.map(option)}
                </Select>
              </div>
              <div style={{ padding: "0 16px 4px", color: "var(--muted)", fontSize: 11 }}>Point at a color's name to see where it's used in the preview.</div>
              <div className="theme-tokens" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "0 16px 10px" }}>
                <div style={groupHead}>Basic colors</div>
                {TOKENS.filter((t) => BASIC.includes(t.key)).map(row)}
                <button
                  className="theme-advanced"
                  aria-expanded={advanced}
                  onClick={() => setAdvanced((a) => !a)}
                  style={{ display: "flex", alignItems: "center", gap: 6, width: "100%", marginTop: 10, padding: "6px 0", background: "none", border: "none", borderTop: "1px solid var(--border)", borderRadius: 0, color: "var(--text)", fontSize: 12, cursor: "pointer", textAlign: "left" }}
                >
                  <span style={{ width: 10, color: "var(--muted)" }}>{advanced ? "▾" : "▸"}</span>
                  <b style={{ whiteSpace: "nowrap" }}>Advanced colors</b>
                  <span style={{ color: "var(--muted)", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{advancedTokens.length} more: surfaces, icons, grid states, tints, status</span>
                  {!advanced && advancedWarnings > 0 && <span style={{ marginLeft: "auto", color: "var(--ui-warn)", whiteSpace: "nowrap" }}>⚠ {advancedWarnings}</span>}
                </button>
                {advanced &&
                  GROUPS.map((g) => {
                    const ts = g.tokens.filter((t) => !BASIC.includes(t.key));
                    return ts.length === 0 ? null : (
                      <div key={g.name}>
                        <div style={groupHead}>{g.name}</div>
                        {ts.map(row)}
                      </div>
                    );
                  })}
              </div>
              <div style={{ padding: "10px 16px", borderTop: "1px solid var(--border)", display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ display: "flex", gap: 6 }}>
                  <Button size="sm">Import…</Button>
                  <Button size="sm">Export…</Button>
                  <Button size="sm" disabled={changed === 0} onClick={() => setOver({})}>
                    Reset to base
                  </Button>
                </div>
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <TextInput value={name} onChange={(e) => setName(e.target.value)} placeholder="Theme name" style={{ flex: 1 }} />
                  <Button size="sm" variant="primary" disabled={!name.trim()}>
                    Save as new theme
                  </Button>
                </div>
              </div>
            </div>
            <div style={{ flex: 1, minWidth: 0, padding: 16, display: "flex", flexDirection: "column", gap: 10, overflow: "auto" }}>
              <Text size="sm" tone="muted">
                Preview{about ? ` — ${LABEL[about]}: ${DESC[about]}` : ""}
              </Text>
              <RealPreview p={p} hot={hot} zoom={0.72} />
              <Measures p={p} />
              <div style={{ marginTop: "auto", borderTop: "1px solid var(--border)", paddingTop: 12, display: "flex", alignItems: "center", gap: 8, fontSize: 12, flexWrap: "wrap" }}>
                <b>Sync with OS</b>
                <span style={{ color: "var(--muted)" }}>dark theme</span>
                <Select value={dark} onChange={(e) => setDark(e.target.value)}>
                  {ALL.filter((t) => t.kind !== "light").map(option)}
                </Select>
                <span style={{ color: "var(--muted)" }}>light theme</span>
                <Select value={light} onChange={(e) => setLight(e.target.value)}>
                  {ALL.filter((t) => t.kind === "light").map(option)}
                </Select>
              </div>
            </div>
          </div>
          <div style={{ padding: "8px 16px", borderTop: "1px solid var(--border)", display: "flex", justifyContent: "flex-end", gap: 6, background: "var(--panel)" }}>
            <span style={{ marginRight: "auto", color: "var(--muted)", fontSize: 11, alignSelf: "center" }}>Colors below the contrast minimum are saved with the warning, never changed silently; an invalid hex is not applied</span>
            <Button>Cancel</Button>
            <Button variant="primary">OK</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
export const CustomEditor: S = { name: "Custom theme editor", render: () => <ThemeEditor /> };

// ───────── 3. Quick switch ─────────
function QuickSwitch() {
  const [current, setCurrent] = useState("dracula");
  const [hi, setHi] = useState(ALL.findIndex((t) => t.id === "dracula"));
  const key = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setHi((i) => (i + (e.key === "ArrowDown" ? 1 : ALL.length - 1)) % ALL.length);
    } else if (e.key === "Enter") setCurrent(ALL[hi].id);
  };
  const cur = ALL.find((t) => t.id === current)!;
  const item = (t: ThemeDef, i: number) => (
    <div
      key={t.id}
      role="option"
      aria-selected={i === hi}
      onMouseEnter={() => setHi(i)}
      onClick={() => setCurrent(t.id)}
      style={{ display: "flex", alignItems: "center", gap: 8, height: 24, padding: "0 10px", fontSize: 13, background: i === hi ? "var(--sel)" : undefined, cursor: "default" }}
    >
      <span style={{ width: 12, color: "var(--accent)" }}>{t.id === current ? "✓" : ""}</span>
      <span style={{ display: "inline-flex", gap: 2 }}>
        {[t.p.bg, t.p.accent, t.p.keyword].map((c, j) => (
          <span key={j} style={{ width: 10, height: 10, borderRadius: 2, background: c, border: "1px solid var(--border)" }} />
        ))}
      </span>
      <span>{t.name}</span>
      {t.custom && <span style={{ fontSize: 10, color: "var(--muted)", border: "1px solid var(--border)", borderRadius: 3, padding: "0 4px" }}>custom</span>}
    </div>
  );
  return (
    <div style={{ padding: 16, display: "flex", gap: 24, alignItems: "flex-start" }}>
      <div style={{ position: "relative", flex: "none" }}>
        <RealPreview p={cur.p} zoom={0.62} />
        <div
          tabIndex={0}
          role="listbox"
          aria-label="Theme"
          onKeyDown={key}
          ref={(el) => el?.focus({ preventScroll: true })}
          style={{ ...themeVars(cur.p), position: "absolute", left: 140, top: 30, width: 290, background: "var(--panel)", border: "1px solid var(--ui-border-strong)", borderRadius: 6, boxShadow: "var(--ui-shadow-pop)", outline: "none", overflow: "hidden" }}
        >
          <div style={{ padding: "6px 10px", fontSize: 12, fontWeight: 600, borderBottom: "1px solid var(--border)" }}>Theme</div>
          <div style={{ padding: "4px 0" }}>
            {BUILT_IN.map((t, i) => item(t, i))}
            <div style={{ fontSize: 11, color: "var(--muted)", padding: "6px 10px 2px", borderTop: "1px solid var(--border)", marginTop: 4 }}>Custom</div>
            {CUSTOM.map((t, i) => item(t, BUILT_IN.length + i))}
            <div style={{ borderTop: "1px solid var(--border)", marginTop: 4, padding: "4px 10px 0", fontSize: 13, height: 24, display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ width: 12 }} />
              Sync with OS
              <span style={{ marginLeft: "auto", color: "var(--muted)", fontSize: 11 }}>Night shift / Solarized Light</span>
            </div>
            <div style={{ padding: "0 10px 2px", fontSize: 13, height: 24, display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ width: 12 }} />
              Customize…
            </div>
          </div>
          <div style={{ padding: "4px 10px", fontSize: 11, color: "var(--muted)", borderTop: "1px solid var(--border)" }}>↑↓ move · ↩ apply · Esc close</div>
        </div>
      </div>
      <Text size="sm" tone="muted">
        Quick Switch Theme (⌃` › Theme, as in DataGrip): built-in themes, then custom ones (marked), then Sync with OS. ✓ is the current theme, the highlight follows ↑↓ or the mouse;
        ↩ or a click applies it, and the window (this popup too) redraws in it. The swatches are background, accent and keyword.
      </Text>
    </div>
  );
}
export const Quick: S = { name: "Quick switch", render: () => <QuickSwitch /> };
