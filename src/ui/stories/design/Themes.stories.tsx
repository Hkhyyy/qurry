import type { Meta, StoryObj } from "@storybook/react-vite";
import { useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { Button, Select, Text, TextInput, normalizeHex } from "../../index";

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
    { key: "hover", label: "Hover", vars: ["--ui-hover", "--c-33353a"] },
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
    { key: "prodTint", label: "Production (red) tint", vars: ["--conn-bg"], check: ["text", "prodTint", 4.5] },
    { key: "prodStripe", label: "Production stripe", vars: ["--conn-stripe"], check: ["prodStripe", "panel", 3] },
    { key: "tint", label: "Other data source (green) tint", vars: [], check: ["text", "tint", 4.5] },
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

/** The inline CSS variables that make the mock UI draw a palette */
function themeVars(p: Palette): CSSProperties {
  const v: Record<string, string> = {};
  for (const t of TOKENS) for (const name of t.vars) v[name] = p[t.key];
  return { ...v, background: p.bg, color: p.text } as CSSProperties;
}

// ───────── Mock Qurry window ─────────
// Every element says which tokens color it (data-tok), so the editor can point at it. Each token colors at least one element here.
const mono: CSSProperties = { fontFamily: "var(--ui-font-mono)", fontSize: 12 };
/** Clips content but leaves room for the "where it's used" outline, drawn just outside an element */
const clipped: CSSProperties = { overflow: "clip", overflowClipMargin: 4 };
/** data-tok attribute: the tokens this element shows */
const tok = (...keys: Key[]) => ({ "data-tok": keys.join(" ") });

/** A thin colored edge inside its (position: relative) parent: the Production stripe */
const Stripe = ({ side }: { side: "left" | "top" }) => (
  <span {...tok("prodStripe")} style={{ position: "absolute", left: 0, top: 0, background: "var(--conn-stripe)", ...(side === "left" ? { bottom: 0, width: 3 } : { right: 0, height: 2 }) }} />
);

function Explorer({ p }: { p: Palette }) {
  const row = (label: ReactNode, depth: number, keys: Key[] = [], bg?: string, stripe?: boolean) => (
    <div {...tok(...keys)} style={{ position: "relative", display: "flex", alignItems: "center", gap: 5, height: 22, paddingLeft: 6 + depth * 14, background: bg, whiteSpace: "nowrap" }}>
      {stripe && <Stripe side="left" />}
      {label}
    </div>
  );
  const db = <span {...tok("link")} style={{ color: "var(--ui-icon-db)" }}>◆</span>;
  const tb = <span {...tok("icon")} style={{ color: "var(--ui-icon)" }}>▦</span>;
  return (
    <div {...tok("panel", "border")} style={{ width: 150, flex: "none", background: "var(--panel)", borderRight: "1px solid var(--border)", fontSize: 12, ...clipped }}>
      <div {...tok("muted")} style={{ padding: "4px 8px", color: "var(--muted)", fontSize: 11, borderBottom: "1px solid var(--border)" }}>Database Explorer</div>
      {row(<>{db}<span>prod-orders</span><span {...tok("warn")} style={{ color: "var(--ui-warn)", fontSize: 10, marginLeft: "auto", paddingRight: 6 }}>PROD</span></>, 0, ["prodTint"], p.prodTint, true)}
      {row(<><span style={{ color: "var(--muted)" }}>▸</span><span>public</span></>, 1, ["prodTint"], p.prodTint, true)}
      {row(<>{tb}<span>orders</span></>, 2, ["prodTint"], p.prodTint, true)}
      {row(<>{db}<span>analytics</span></>, 0, ["tint"], p.tint)}
      {row(<>{db}<span>dev-local</span></>, 0)}
      {row(<>{tb}<span>users</span><span style={{ color: "var(--muted)", fontSize: 10, marginLeft: "auto", paddingRight: 6 }}>hover</span></>, 1, ["hover"], "var(--ui-hover)")}
    </div>
  );
}

function Tabs() {
  return (
    <div {...tok("panel", "border")} style={{ display: "flex", background: "var(--panel)", borderBottom: "1px solid var(--border)", flex: "none" }}>
      <div {...tok("prodTint")} style={{ position: "relative", display: "flex", alignItems: "center", padding: "0 12px", height: 28, fontSize: 12, color: "var(--text)", background: "var(--conn-bg)", whiteSpace: "nowrap" }}>
        <Stripe side="top" />
        prod-orders
        <span {...tok("accent")} style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 2, background: "var(--accent)" }} />
      </div>
      <div {...tok("muted")} style={{ display: "flex", alignItems: "center", padding: "0 12px", height: 28, fontSize: 12, color: "var(--muted)", whiteSpace: "nowrap" }}>
        dev-local
      </div>
    </div>
  );
}

function Editor() {
  const k = (s: string) => <span {...tok("keyword")} style={{ color: "var(--monaco-keyword)" }}>{s}</span>;
  const lines: ReactNode[] = [
    <span {...tok("comment")} style={{ color: "var(--monaco-comment)", fontStyle: "italic" }}>-- paid orders this week</span>,
    <>{k("SELECT")} id, <span {...tok("function")} style={{ color: "var(--monaco-predefined)" }}>upper</span>(name), total, meta</>,
    <>{k("FROM")} orders {k("WHERE")} status = <span {...tok("string")} style={{ color: "var(--monaco-string)" }}>'paid'</span></>,
    <>{k("LIMIT")} <span {...tok("number")} style={{ color: "var(--monaco-number)" }}>100</span>;</>,
    <>{k("SELECT")} * {k("FROM")} ord</>,
    "",
  ];
  return (
    <div {...tok("bg")} style={{ ...mono, position: "relative", background: "var(--bg)", padding: "4px 0", flex: "none" }}>
      {lines.map((l, i) => (
        <div key={i} style={{ display: "flex", height: 18, alignItems: "center" }}>
          <span {...tok("muted")} style={{ width: 26, textAlign: "right", paddingRight: 8, color: "color-mix(in srgb, var(--muted) 70%, var(--bg))" }}>{i + 1}</span>
          <span {...tok("text")} style={{ whiteSpace: "nowrap" }}>{l}</span>
        </div>
      ))}
      {/* Completion popup: a popup's edge is a strong border */}
      <div {...tok("borderStrong", "panel")} style={{ position: "absolute", left: 128, top: 4 + 5 * 18, width: 140, background: "var(--panel)", border: "1px solid var(--ui-border-strong)", borderRadius: 4, boxShadow: "var(--ui-shadow-pop)", fontSize: 11, padding: 2, zIndex: 1 }}>
        <div {...tok("sel")} style={{ display: "flex", gap: 6, padding: "1px 4px", background: "var(--sel)", borderRadius: 2 }}>
          <span style={{ color: "var(--ui-icon)" }}>▦</span>orders<span style={{ marginLeft: "auto", color: "var(--muted)" }}>table</span>
        </div>
      </div>
    </div>
  );
}

function Json({ k, v }: { k: string; v: ReactNode }) {
  return (
    <>
      <span {...tok("muted")} style={{ color: "var(--ui-json-punct)" }}>{"{"}</span>
      <span {...tok("jsonKey")} style={{ color: "var(--ui-json-key)" }}>"{k}"</span>
      <span style={{ color: "var(--ui-json-punct)" }}>: </span>
      {v}
      <span style={{ color: "var(--ui-json-punct)" }}>{"}"}</span>
    </>
  );
}

function Grid() {
  const cols: [string, number, boolean][] = [["id", 30, true], ["name", 58, false], ["total", 76, true], ["meta", 1, false]];
  type Row = { v: ReactNode[]; keys?: Key[]; bg?: string; deleted?: boolean };
  const gift = <Json k="gift" v={<span {...tok("keyword")} style={{ color: "var(--ui-json-keyword)" }}>true</span>} />;
  const rush = <Json k="tag" v={<span {...tok("string")} style={{ color: "var(--ui-json-string)" }}>"rush"</span>} />;
  const rows: Row[] = [
    { v: ["1", "Kim", "1,200.00", gift] },
    { v: ["2", "Lee", "35.50", null] },
    { v: ["3", "Park", "980.00", rush] },
    { v: ["4", "Choi", "12.00", null], keys: ["added"], bg: "var(--grid-added)" },
    { v: ["5", "Jung", "7.25", null], keys: ["deleted", "danger"], bg: "var(--grid-deleted)", deleted: true },
  ];
  const cell = (key: string | number, w: number, num: boolean, extra: CSSProperties = {}, children: ReactNode = null, keys: Key[] = []) => (
    <div key={key} {...tok(...keys)} style={{ width: w === 1 ? undefined : w, flex: w === 1 ? 1 : "none", minWidth: 0, padding: "0 6px", textAlign: num ? "right" : "left", overflow: "hidden", whiteSpace: "nowrap", borderRight: "1px solid var(--border)", lineHeight: "22px", ...extra }}>{children}</div>
  );
  return (
    <div style={{ ...mono, flex: 1, minHeight: 0, borderTop: "1px solid var(--border)", display: "flex", flexDirection: "column" }}>
      <div {...tok("panel")} style={{ display: "flex", alignItems: "center", gap: 4, height: 26, padding: "0 6px", background: "var(--panel)", borderBottom: "1px solid var(--border)", fontSize: 11, flex: "none" }}>
        <span {...tok("hover", "borderStrong")} style={{ padding: "1px 7px", border: "1px solid var(--ui-border-strong)", borderRadius: 4, background: "var(--ui-hover)" }} title="Shown in its hover state">↻</span>
        <span {...tok("borderStrong")} style={{ padding: "1px 7px", border: "1px solid var(--ui-border-strong)", borderRadius: 4 }}>+</span>
        <span {...tok("accent")} style={{ padding: "1px 8px", border: "1px solid var(--accent)", borderRadius: 4, background: "var(--accent)" }}>
          <span {...tok("onAccent")} style={{ color: "var(--ui-text-on-accent)" }}>Submit (3)</span>
        </span>
        <span style={{ marginLeft: "auto", color: "var(--muted)" }}>Find: Lee</span>
      </div>
      <div {...tok("panel")} style={{ display: "flex", height: 22, alignItems: "center", background: "var(--panel)", borderBottom: "1px solid var(--border)", flex: "none" }}>
        {cell("#", 24, true, { color: "var(--muted)", background: "var(--panel-2)" }, null, ["panel2"])}
        {cols.map(([c, w, n]) => (c === "total" ? cell(c, w, n, { color: "var(--c-9fbfff)", fontWeight: 600 }, "total ↓", ["link"]) : cell(c, w, n, { color: "var(--text)", fontWeight: 600 }, c)))}
      </div>
      {rows.map((r, i) => (
        <div key={i} {...tok(...(r.keys ?? [i % 2 ? "odd" : "bg"]))} style={{ display: "flex", height: 22, alignItems: "center", flex: "none", background: r.bg ?? (i % 2 ? "var(--c-222326)" : "var(--bg)") }}>
          {cell("#", 24, true, { color: "var(--muted)", background: "var(--panel-2)" }, i + 1, ["panel2"])}
          {r.v.map((v, j) => {
            const [, w, num] = cols[j];
            const st: CSSProperties = { color: v === null ? "var(--c-6f737a)" : num ? "var(--c-2aacb8)" : "var(--text)", fontStyle: v === null ? "italic" : undefined };
            const keys: Key[] = v === null ? ["nullText"] : num ? ["gridNum"] : [];
            if (r.deleted) Object.assign(st, { color: "var(--danger)", textDecoration: "line-through" });
            // a selected cell, a Find match, an edited (not yet submitted) cell
            if (i === 0 && j === 1) {
              Object.assign(st, { background: "var(--sel)", boxShadow: "inset 0 0 0 1px var(--accent)" });
              keys.push("sel", "accent");
            }
            if (i === 1 && j === 1) {
              st.background = "var(--grid-match)";
              keys.push("match");
            }
            if (i === 2 && j === 2) {
              st.background = "var(--grid-changed)";
              keys.push("changed");
            }
            return cell(j, w, num, st, v ?? "NULL", keys);
          })}
        </div>
      ))}
    </div>
  );
}

function StatusBar() {
  return (
    <div {...tok("panel")} style={{ display: "flex", alignItems: "center", gap: 10, height: 22, padding: "0 8px", background: "var(--panel)", borderTop: "1px solid var(--border)", color: "var(--muted)", fontSize: 11, flex: "none", whiteSpace: "nowrap", ...clipped }}>
      <span {...tok("accent")} style={{ color: "var(--accent)" }}>◐</span>
      <span {...tok("text")} style={{ color: "var(--text)" }}>Running 1.4 s</span>
      <span {...tok("ok")} style={{ color: "var(--ok)" }}>✓ 3 rows</span>
      <span {...tok("warn")} style={{ color: "var(--ui-warn)" }}>3 pending</span>
      <span {...tok("danger")} style={{ color: "var(--danger)" }}>✕ 1 error</span>
      <span {...tok("muted")} style={{ marginLeft: "auto" }}>UTF-8</span>
    </div>
  );
}

/**
 * A few lines of the TUI in this theme. It maps the same tokens: ACCENT = accent text (focused border), SEL, NUMBER = grid numbers,
 * DIM = dim text, CELL_EDITED = edited cell, OK / WARN / DANGER
 */
function TuiStrip({ p }: { p: Palette }) {
  const W = 64;
  const pad = (s: string, w: number, right = false) => (right ? s.padStart(w) : s.padEnd(w)).slice(0, w);
  const B = (s: string) => <span {...tok("link")} style={{ color: p.link }}>{s}</span>;
  const title = " orders  3 rows ";
  const row = (id: string, name: string, total: string | null, opt: { sel?: boolean; edited?: boolean } = {}) => (
    <div {...(opt.sel ? tok("sel") : {})} style={{ background: opt.sel ? p.sel : undefined }}>
      {B("┃")}
      <span {...tok("gridNum")} style={{ color: p.gridNum }}>{pad(id, 4, true)}</span>
      <span>{"  " + pad(name, 12)}</span>
      {total === null ? (
        <span {...tok("muted")} style={{ color: p.muted, fontStyle: "italic" }}>{pad("NULL", 10, true)}</span>
      ) : (
        <span {...tok("gridNum", ...(opt.edited ? (["changed"] as Key[]) : []))} style={{ color: p.gridNum, background: opt.edited ? p.changed : undefined }}>{pad(total, 10, true)}</span>
      )}
      <span>{" ".repeat(W - 2 - 28)}</span>
      {B("┃")}
    </div>
  );
  return (
    <div {...tok("bg")} style={{ ...mono, fontSize: 11, lineHeight: "14px", background: p.bg, color: p.text, padding: "4px 6px", whiteSpace: "pre", borderTop: `1px solid ${p.border}`, ...clipped }}>
      <div style={{ display: "inline-block" }}>
        <div>
          {B("┏")}
          <b {...tok("link")} style={{ color: p.link }}>{title}</b>
          {B("━".repeat(W - 2 - title.length) + "┓")}
        </div>
        <div>
          {B("┃")}
          <b {...tok("text")}>{pad("  id  name             total", W - 2)}</b>
          {B("┃")}
        </div>
        {row("1", "Kim", "1,200.00", { sel: true })}
        {row("2", "Lee", null)}
        {row("3", "Park", "980.00", { edited: true })}
        {B("┗" + "━".repeat(W - 2) + "┛")}
      </div>
      <div {...tok("muted")} style={{ color: p.muted }}>
        <span {...tok("accent")} style={{ color: p.accent }}>⠋</span> <span style={{ color: p.text }}>Running 1.4s</span> <span {...tok("ok")} style={{ color: p.ok }}>✓ 3 rows</span> <span {...tok("warn")} style={{ color: p.warn }}>1 pending</span>{" "}
        <span {...tok("danger")} style={{ color: p.danger }}>✕ 1 error</span> <b style={{ color: p.text }}>⌘↩</b> run <b style={{ color: p.text }}>⌃k</b> commands
      </div>
    </div>
  );
}

/** hot: the token being pointed at. Its elements get a thin dashed outline just outside them (in the preview's text color), never a fill */
function MiniWindow({ p, width = 460, hot }: { p: Palette; width?: number | string; hot?: Key | null }) {
  const css = hot ? `.tok-scope [data-tok~="${hot}"] { outline: 1px dashed var(--text); outline-offset: 2px; }` : "";
  return (
    <div className="tok-scope" style={{ ...themeVars(p), width, border: `1px solid ${p.border}`, borderRadius: 6, ...clipped, display: "flex", flexDirection: "column", fontSize: 12 }}>
      <style>{css}</style>
      <div style={{ display: "flex", height: 336 }}>
        <Explorer p={p} />
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <Tabs />
          <Editor />
          <Grid />
        </div>
      </div>
      <StatusBar />
      <TuiStrip p={p} />
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
  return (
    <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
      <Text size="sm">
        Each card is the same mock drawn with that theme's tokens (inline CSS variables). Under it: WCAG contrast on the background, text ≥ 4.5:1, dim text and the accent
        (an icon / UI color) ≥ 3:1. All seven ship in 0.3.2. Dark, Light and High contrast are today's values; the four new ones are shown with their official palettes, and the ones marked ⚠ (Solarized, a few pairs elsewhere) get adjusted colors that pass when built (#103).
      </Text>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(462px, 1fr))", gap: 20 }}>
        {BUILT_IN.map((t) => (
          <div key={t.id} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <Text size="sm">
              <b>{t.name}</b> — {EXISTING.has(t.id) ? "built in today" : "new in 0.3.2"}, {t.kind === "light" ? "light" : "dark"}
            </Text>
            <MiniWindow p={t.p} />
            <Measures p={t.p} />
          </div>
        ))}
      </div>
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

const NAV = ["Appearance", "Editor", "Keymap", "Data Editor", "AI", "Plugins"];

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
      <div style={{ display: "flex", width: 1240, height: 800, border: "1px solid var(--border)", borderRadius: 8, overflow: "hidden", background: "var(--bg)", boxShadow: "var(--ui-shadow-modal)" }}>
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
              <MiniWindow p={p} width="100%" hot={hot} />
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
        <MiniWindow p={cur.p} width={560} />
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
