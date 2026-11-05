import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
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
const mono: CSSProperties = { fontFamily: "var(--ui-font-mono)", fontSize: 12 };

function Explorer({ p }: { p: Palette }) {
  const row = (label: ReactNode, depth: number, tint?: string, stripe?: string): ReactNode => (
    <div style={{ display: "flex", alignItems: "center", gap: 5, height: 22, paddingLeft: 6 + depth * 14, background: tint, boxShadow: stripe ? `inset 3px 0 0 ${stripe}` : undefined, whiteSpace: "nowrap" }}>{label}</div>
  );
  const db = <span style={{ color: "var(--ui-icon-db)" }}>◆</span>;
  const tb = <span style={{ color: "var(--ui-icon)" }}>▦</span>;
  return (
    <div style={{ width: 150, flex: "none", background: "var(--panel)", borderRight: "1px solid var(--border)", fontSize: 12, overflow: "hidden" }}>
      <div style={{ padding: "4px 8px", color: "var(--muted)", fontSize: 11, borderBottom: "1px solid var(--border)" }}>Database Explorer</div>
      {row(<>{db}<span>prod-orders</span><span style={{ color: "var(--ui-warn)", fontSize: 10, marginLeft: "auto", paddingRight: 6 }}>PROD</span></>, 0, p.prodTint, p.prodStripe)}
      {row(<><span style={{ color: "var(--muted)" }}>▸</span><span>public</span></>, 1, p.prodTint, p.prodStripe)}
      {row(<>{tb}<span>orders</span></>, 2, p.prodTint, p.prodStripe)}
      {row(<>{db}<span>dev-local</span></>, 0)}
      {row(<>{tb}<span>users</span></>, 1)}
    </div>
  );
}

function Tabs({ p }: { p: Palette }) {
  const tab = (label: string, active: boolean, tint?: string, stripe?: string) => (
    <div style={{ display: "flex", alignItems: "center", padding: "0 12px", height: 28, fontSize: 12, color: active ? "var(--text)" : "var(--muted)", background: active ? tint ?? "var(--bg)" : "transparent", borderBottom: `2px solid ${active ? "var(--accent)" : "transparent"}`, boxShadow: stripe ? `inset 0 2px 0 ${stripe}` : undefined, whiteSpace: "nowrap" }}>{label}</div>
  );
  return (
    <div style={{ display: "flex", background: "var(--panel)", borderBottom: "1px solid var(--border)", flex: "none" }}>
      {tab("prod-orders", true, p.prodTint, p.prodStripe)}
      {tab("dev-local", false)}
    </div>
  );
}

function Editor() {
  const k = (s: string) => <span style={{ color: "var(--monaco-keyword)" }}>{s}</span>;
  const lines: ReactNode[] = [
    <span style={{ color: "var(--monaco-comment)", fontStyle: "italic" }}>-- paid orders this week</span>,
    <>{k("SELECT")} id, <span style={{ color: "var(--monaco-predefined)" }}>upper</span>(name), total</>,
    <>{k("FROM")} orders {k("WHERE")} status = <span style={{ color: "var(--monaco-string)" }}>'paid'</span></>,
    <>{k("LIMIT")} <span style={{ color: "var(--monaco-number)" }}>100</span>;</>,
  ];
  return (
    <div style={{ ...mono, background: "var(--bg)", padding: "4px 0", flex: "none" }}>
      {lines.map((l, i) => (
        <div key={i} style={{ display: "flex", height: 18, alignItems: "center", background: i === 2 ? "color-mix(in srgb, var(--text) 4%, transparent)" : undefined }}>
          <span style={{ width: 26, textAlign: "right", paddingRight: 8, color: i === 2 ? "var(--muted)" : "color-mix(in srgb, var(--muted) 60%, var(--bg))" }}>{i + 1}</span>
          <span style={{ whiteSpace: "nowrap" }}>{l}</span>
        </div>
      ))}
    </div>
  );
}

function Grid() {
  const cols: [string, number, boolean][] = [["id", 44, true], ["name", 88, false], ["total", 78, true], ["note", 1, false]];
  const rows = [
    ["1", "Kim", "1,200.00", "gift"],
    ["2", "Lee", "35.50", null],
    ["3", "Park", "980.00", "rush"],
  ];
  const cell = (key: string | number, w: number, num: boolean, extra: CSSProperties = {}, children: ReactNode = null) => (
    <div key={key} style={{ width: w === 1 ? undefined : w, flex: w === 1 ? 1 : "none", padding: "0 6px", textAlign: num ? "right" : "left", overflow: "hidden", whiteSpace: "nowrap", borderRight: "1px solid var(--border)", ...extra }}>{children}</div>
  );
  return (
    <div style={{ ...mono, flex: 1, minHeight: 0, borderTop: "1px solid var(--border)" }}>
      <div style={{ display: "flex", height: 22, alignItems: "center", background: "var(--panel)", borderBottom: "1px solid var(--border)" }}>
        {cell("#", 26, true, { color: "var(--muted)", background: "var(--panel-2)" })}
        {cols.map(([c, w, n]) => cell(c, w, n, { color: c === "total" ? "var(--c-9fbfff)" : "var(--text)", fontWeight: 600 }, c === "total" ? "total ↓" : c))}
      </div>
      {rows.map((r, i) => (
        <div key={i} style={{ display: "flex", height: 22, alignItems: "center", background: i % 2 ? "var(--c-222326)" : "var(--bg)" }}>
          {cell("#", 26, true, { color: "var(--muted)", background: "var(--panel-2)", lineHeight: "22px" }, i + 1)}
          {r.map((v, j) => {
            const [, w, num] = cols[j];
            const st: CSSProperties = { lineHeight: "22px", color: v === null ? "var(--c-6f737a)" : num ? "var(--c-2aacb8)" : "var(--text)", fontStyle: v === null ? "italic" : undefined };
            // a selected cell, an edited (not yet submitted) cell
            if (i === 0 && j === 1) Object.assign(st, { background: "var(--sel)", outline: "1px solid var(--accent)", outlineOffset: -1 });
            if (i === 2 && j === 2) st.background = "var(--grid-changed)";
            return cell(j, w, num, st, v ?? "NULL");
          })}
        </div>
      ))}
    </div>
  );
}

function StatusBar() {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, height: 22, padding: "0 8px", background: "var(--panel)", borderTop: "1px solid var(--border)", color: "var(--muted)", fontSize: 11, flex: "none", whiteSpace: "nowrap" }}>
      <span style={{ color: "var(--accent)" }}>◐</span>
      <span style={{ color: "var(--text)" }}>Running 1.4 s</span>
      <span style={{ color: "var(--ok)" }}>✓ 3 rows</span>
      <span style={{ color: "var(--ui-warn)" }}>1 pending change</span>
      <span style={{ marginLeft: "auto" }}>UTF-8</span>
    </div>
  );
}

/** A few lines of the TUI in this theme: the TUI maps the same tokens (ACCENT = link, SEL, NUMBER = grid numbers, DIM = dim text) */
function TuiStrip({ p }: { p: Palette }) {
  const W = 64;
  const pad = (s: string, w: number, right = false) => (right ? s.padStart(w) : s.padEnd(w)).slice(0, w);
  const B = (s: string) => <span style={{ color: p.link }}>{s}</span>;
  const title = " orders  3 rows ";
  const row = (id: string, name: string, total: string | null, sel = false) => (
    <div style={{ background: sel ? p.sel : undefined }}>
      {B("┃")}
      <span style={{ color: p.gridNum }}>{pad(id, 4, true)}</span>
      <span>{"  " + pad(name, 12)}</span>
      {total === null ? <span style={{ color: p.muted, fontStyle: "italic" }}>{pad("NULL", 10, true)}</span> : <span style={{ color: p.gridNum }}>{pad(total, 10, true)}</span>}
      <span>{" ".repeat(W - 2 - 28)}</span>
      {B("┃")}
    </div>
  );
  return (
    <div style={{ ...mono, fontSize: 11, lineHeight: "14px", background: p.bg, color: p.text, padding: "4px 6px", whiteSpace: "pre", borderTop: `1px solid ${p.border}`, overflow: "hidden" }}>
      <div style={{ display: "inline-block" }}>
        <div>
          {B("┏")}
          <b style={{ color: p.link }}>{title}</b>
          {B("━".repeat(W - 2 - title.length) + "┓")}
        </div>
        <div>
          {B("┃")}
          <b>{pad("  id  name             total", W - 2)}</b>
          {B("┃")}
        </div>
        {row("1", "Kim", "1,200.00", true)}
        {row("2", "Lee", null)}
        {B("┗" + "━".repeat(W - 2) + "┛")}
      </div>
      <div style={{ color: p.muted }}>
        <span style={{ color: p.accent }}>⠋</span> <span style={{ color: p.text }}>Running 1.4s</span> <span style={{ color: p.keyword }}>⌘↩</span> run <span style={{ color: p.keyword }}>⌃k</span> commands <span style={{ color: p.ok }}>●</span> prod
      </div>
    </div>
  );
}

function MiniWindow({ p, width = 460 }: { p: Palette; width?: number | string }) {
  return (
    <div style={{ ...themeVars(p), width, border: `1px solid ${p.border}`, borderRadius: 6, overflow: "hidden", display: "flex", flexDirection: "column", fontSize: 12 }}>
      <div style={{ display: "flex", height: 196 }}>
        <Explorer p={p} />
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <Tabs p={p} />
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
        (an icon / UI color) ≥ 3:1. Dark, Light and High contrast are today's values; the others are the official palettes as they are.
      </Text>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(462px, 1fr))", gap: 20 }}>
        {BUILT_IN.map((t) => (
          <div key={t.id} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <Text size="sm">
              <b>{t.name}</b> — {EXISTING.has(t.id) ? "built in today" : "candidate"}, {t.kind === "light" ? "light" : "dark"}
            </Text>
            <MiniWindow p={t.p} />
            <Measures p={t.p} />
          </div>
        ))}
      </div>
    </div>
  );
}
export const Candidates: S = { name: "Theme candidates", render: () => <Gallery /> };

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

function ThemeEditor() {
  const [baseId, setBaseId] = useState("dracula");
  const base = ALL.find((t) => t.id === baseId)!;
  // Starts with two edits: the text a little warmer, and a comment color too dark to read (shows the warning)
  const [over, setOver] = useState<Partial<Palette>>({ text: "#e8e8e0", comment: "#4a4f6a" });
  const [name, setName] = useState("Dracula (mine)");
  const [dark, setDark] = useState("c-night");
  const [light, setLight] = useState("solarized-light");
  const p: Palette = { ...base.p, ...over };
  const set = (k: Key, v: string) => setOver((o) => ({ ...o, [k]: v }));
  const changed = Object.keys(over).filter((k) => over[k as Key] !== base.p[k as Key]).length;
  const warnings = TOKENS.filter((t) => t.check && ratio(p[t.check[0]], p[t.check[1]]) < t.check[2]).length;
  const option = (t: ThemeDef) => (
    <option key={t.id} value={t.id}>
      {t.name}
      {t.custom ? " (custom)" : ""}
    </option>
  );
  return (
    <div style={{ padding: 16 }}>
      <div style={{ display: "flex", width: 1180, height: 760, border: "1px solid var(--border)", borderRadius: 8, overflow: "hidden", background: "var(--bg)", boxShadow: "var(--ui-shadow-modal)" }}>
        <div style={{ width: 170, flex: "none", background: "var(--panel)", borderRight: "1px solid var(--border)", padding: "8px 0", fontSize: 13 }}>
          {NAV.map((n) => (
            <div key={n} style={{ padding: "4px 14px", background: n === "Appearance" ? "var(--sel)" : undefined }}>{n}</div>
          ))}
        </div>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--border)", display: "flex", alignItems: "center", gap: 8 }}>
            <b style={{ fontSize: 13 }}>Appearance › Theme</b>
            <span style={{ color: "var(--muted)", fontSize: 12 }}>
              {changed} {changed === 1 ? "token" : "tokens"} changed from {base.name}
              {warnings > 0 && <span style={{ color: "var(--ui-warn)" }}> · ⚠ {warnings} below the contrast minimum</span>}
            </span>
          </div>
          <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
            <div style={{ width: 430, flex: "none", display: "flex", flexDirection: "column", borderRight: "1px solid var(--border)" }}>
              <div style={{ padding: "10px 16px", display: "flex", alignItems: "center", gap: 8, fontSize: 12 }}>
                <span style={{ color: "var(--muted)", width: 70 }}>Based on</span>
                <Select value={baseId} onChange={(e) => (setBaseId(e.target.value), setOver({}))}>
                  {BUILT_IN.map(option)}
                </Select>
              </div>
              <div className="theme-tokens" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "0 16px 10px" }}>
                {GROUPS.map((g) => (
                  <div key={g.name} style={{ marginBottom: 10 }}>
                    <div style={{ fontSize: 11, fontWeight: 600, color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.4, padding: "6px 0 3px", borderBottom: "1px solid var(--border)", marginBottom: 3 }}>{g.name}</div>
                    {g.tokens.map((t) => {
                      const r = t.check && ratio(p[t.check[0]], p[t.check[1]]);
                      const low = t.check && r! < t.check[2];
                      const edited = over[t.key] !== undefined && over[t.key] !== base.p[t.key];
                      const other = t.check && (t.check[0] === t.key ? t.check[1] : t.check[0]);
                      return (
                        <div key={t.key} style={{ display: "flex", alignItems: "center", gap: 6, height: 26, fontSize: 12 }}>
                          <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: edited ? "var(--c-548af7)" : undefined }} title={t.vars.join(" ")}>
                            {t.label}
                          </span>
                          {low && (
                            <span style={{ color: "var(--ui-warn)", whiteSpace: "nowrap", fontSize: 11 }} title={`${LABEL[t.check![0]]} on ${LABEL[t.check![1]]}: needs ${t.check![2]}:1`}>
                              ⚠ {r!.toFixed(1)}:1 on {LABEL[other!].split(",")[0]}
                            </span>
                          )}
                          <HexField value={p[t.key]} onChange={(v) => set(t.key, v)} />
                          <button className="ui-icon-btn ui-icon-btn-sm" title="Reset to base" aria-label="Reset to base" style={{ visibility: edited ? "visible" : "hidden" }} onClick={() => setOver(({ [t.key]: _, ...o }) => o)}>
                            ↺
                          </button>
                        </div>
                      );
                    })}
                  </div>
                ))}
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
                Preview
              </Text>
              <MiniWindow p={p} width="100%" />
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
            <span style={{ marginRight: "auto", color: "var(--muted)", fontSize: 11, alignSelf: "center" }}>Values below the minimum are saved with the warning, never changed silently; an invalid hex is not applied</span>
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
