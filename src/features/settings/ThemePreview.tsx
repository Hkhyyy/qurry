import Editor from "@monaco-editor/react";
import { useLayoutEffect, useRef } from "react";
import { MONACO_THEME, TOKEN_VARS, connStripe, connTabVars, themeVars, type Palette } from "../../lib/theme";
import { Button, cx, GridCell, GridHeaderRow, JsonText, Notice, RowNumberCell, TreeRow } from "../../ui";
import { DatabaseIcon, PgIcon, TableIcon } from "../../ui/icons";

// Settings › Appearance › Theme: the editor's preview, drawn by the app's own styles with the palette's variables (the explorer, a tab bar,
// a console, a result grid with each edit state, a JSON cell, notices). Monaco draws from its one theme, which the editor's live
// preview has set to the palette. Pointing at a color's name outlines what draws it (usedBy)

const SQL = "-- users to check this week\nselect upper(name) as who, 100 as n\nfrom users where name <> 'lee';";
const ROWS: [string, string, string | null][] = [
  ["1", "kim", "12.50"],
  ["2", "lee", null],
  ["3", "park", "7.00"],
  ["4", "choi", "3.25"],
  ["5", "new row", null],
];
const RED = "#6e2a2a";
const GREEN = "#2f5634";

/** Two colors: a token's variables are set to each for a moment, and what changes between them draws it */
const PROBES = ["#010203", "#fefdfc"];
const rgbOf = (h: string) => `rgb(${[1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ")})`;
const hasText = (el: Element) => [...el.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent!.trim());
/** What of el's look a token can be in: its own text, icon strokes, background, borders, inset stripes */
function look(el: Element) {
  const s = getComputedStyle(el);
  const text = hasText(el) || el instanceof SVGElement ? s.color + s.fill + s.stroke : "";
  const borders = (["Top", "Right", "Bottom", "Left"] as const).map((d) => (s[`border${d}Width`] === "0px" ? "" : s[`border${d}Color`])).join();
  return [text, s.backgroundColor, borders, s.boxShadow, s.outlineColor].join("|");
}
/** Monaco's spans in the token's color (Monaco doesn't read the variables) */
const MONACO_TOKENS = ["keyword", "string", "number", "comment", "function", "text"];

/** The elements in `wrap` that draw token `k` of palette `p` */
export function usedBy(wrap: HTMLElement, k: string, p: Palette): Element[] {
  const vars = TOKEN_VARS[k] ?? [];
  const els = [...wrap.querySelectorAll("*")].filter((el) => !el.closest(".monaco-editor") && (el as HTMLElement).offsetParent !== null);
  const before = vars.map((v) => wrap.style.getPropertyValue(v));
  const looks = PROBES.map((c) => {
    vars.forEach((v) => wrap.style.setProperty(v, c));
    return els.map(look);
  });
  vars.forEach((v, i) => wrap.style.setProperty(v, before[i]));
  const inCss = els.filter((_, i) => looks[0][i] !== looks[1][i]).map((el) => (el instanceof SVGElement && el.ownerSVGElement ? el.ownerSVGElement : el));
  const want = rgbOf(p[k]);
  const inMonaco = MONACO_TOKENS.includes(k) ? [...wrap.querySelectorAll(".monaco-editor .view-line span span")].filter((el) => getComputedStyle(el).color === want && el.textContent!.trim()) : [];
  if (k === "bg") inMonaco.push(...wrap.querySelectorAll(".monaco-editor-background"));
  if (k === "muted") inMonaco.push(...wrap.querySelectorAll(".monaco-editor .line-numbers"));
  return [...new Set([...inCss, ...inMonaco])];
}

export function ThemePreview({ p, hot }: { p: Palette; hot: string | null }) {
  const wrap = useRef<HTMLDivElement>(null);
  const colors = JSON.stringify(p);
  useLayoutEffect(() => {
    if (!hot || !wrap.current) return;
    const els = usedBy(wrap.current, hot, p);
    els.forEach((el) => el.classList.add("tok-hot"));
    return () => els.forEach((el) => el.classList.remove("tok-hot"));
  }, [hot, colors]);
  const w = [44, 110, 90];
  return (
    <div ref={wrap} className="theme-preview" style={{ ...themeVars(p), background: p.bg, color: p.text }} aria-label="Theme preview">
      <div className="theme-preview-top">
        <div className="theme-preview-tree">
          <div className="conn-tree" style={{ background: "var(--theme-prod-tint)", boxShadow: `inset 3px 0 0 ${connStripe(RED)}` }}>
            <TreeRow depth={0} open icon={<PgIcon />} label="[PROD] shop" />
            <TreeRow depth={1} open icon={<DatabaseIcon />} label="shop" />
            <TreeRow depth={2} leaf icon={<TableIcon />} label="orders" className="preview-hover" />
            <TreeRow depth={2} leaf icon={<TableIcon />} label="users" selected />
          </div>
          <div className="conn-tree" style={{ background: "var(--theme-tint)", boxShadow: `inset 3px 0 0 ${connStripe(GREEN)}` }}>
            <TreeRow depth={0} icon={<PgIcon />} label="[DEV] shop_app" />
          </div>
          <TreeRow depth={0} icon={<PgIcon />} label="local" sub="localhost" />
        </div>
        <div className="theme-preview-main">
          <div className="tabs ui-row">
            <div className="tab active" style={connTabVars(RED)}>
              <span className="tab-label">console</span>
            </div>
            <div className="tab" style={connTabVars(GREEN)}>
              <span className="tab-label">users</span>
            </div>
          </div>
          <div className="theme-preview-editor">
            <Editor value={SQL} language="pgsql" theme={MONACO_THEME} options={{ readOnly: true, domReadOnly: true, minimap: { enabled: false }, scrollBeyondLastLine: false, fontSize: 12, lineNumbersMinChars: 2, folding: false, renderLineHighlight: "none", scrollbar: { vertical: "hidden", horizontal: "hidden" }, overviewRulerLanes: 0 }} />
          </div>
        </div>
      </div>
      <div className="theme-preview-grid">
        <GridHeaderRow width={w[0] + w[1] + w[2] + 8} height={26}>
          <div className="cell head rownum" style={{ width: w[0] }} />
          <div className="cell head sorted" style={{ width: w[1] }}>
            <span className="col-name">name</span>
            <span className="sort-btn on">↑</span>
          </div>
          <div className="cell head" style={{ width: w[2] }}>
            <span className="col-name">amount</span>
            <span className="col-type">numeric</span>
          </div>
        </GridHeaderRow>
        {ROWS.map(([n, name, amount], i) => (
          <div key={n} className={cx("grid-row", i % 2 === 1 && "odd", i === 2 && "deleted", i === 4 && "added")} role="row">
            <RowNumberCell width={w[0]} selected={i === 0}>
              {n}
            </RowNumberCell>
            <GridCell width={w[1]} changed={i === 1} match={i === 3} selected={i === 0} active={i === 0}>
              {i === 1 ? "lee2" : name}
            </GridCell>
            <GridCell width={w[2]} kind={amount === null ? "null" : "num"} align={amount === null ? "left" : "right"}>
              {amount ?? "<null>"}
            </GridCell>
          </div>
        ))}
        <div className="grid-row" role="row">
          <RowNumberCell width={w[0]}>6</RowNumberCell>
          <div className="cell" style={{ width: w[1] + w[2] }}>
            <JsonText text={'{"kind": "card", "live": true, "n": 2}'} spans={[[0, 1, "punct"], [1, 7, "key"], [7, 9, "punct"], [9, 15, "string"], [15, 17, "punct"], [17, 23, "key"], [23, 25, "punct"], [25, 29, "keyword"], [29, 31, "punct"], [31, 34, "key"], [34, 36, "punct"], [36, 37, "number"], [37, 38, "punct"]]} />
          </div>
        </div>
      </div>
      <div className="theme-preview-notes">
        <Notice tone="warn">Refresh failed, showing data from 10:24</Notice>
        <Notice tone="danger">relation "no_such_table" does not exist</Notice>
        <Notice tone="ok">3 rows updated</Notice>
        <div className="ui-row">
          <Button size="sm" variant="primary">
            Submit
          </Button>
          <Button size="sm">Cancel</Button>
          <span className="muted">dim text</span>
          <a>link</a>
        </div>
      </div>
    </div>
  );
}
