import Editor from "@monaco-editor/react";
import { useEffect, useState } from "react";
import "../../lib/monaco";
import { MOCK_IMAGES } from "../../lib/mock";
import { MONACO_THEME, connStripe, connTabVars, previewTheme, saveCustomTheme, setThemePick, useThemeState } from "../../lib/theme";
import { newBackgroundSlot, newCornerSlot, type ThemeImages } from "../../lib/themeImages";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { ThemeBackdrop, ThemeSticker } from "../../features/settings/ThemeImages";
import { cx, GridCell, GridHeaderRow, IconButton, Menu, RowNumberCell, Spacer, TreeRow } from "..";
import { DatabaseIcon, IcExport, IcFilter, IcMinus, IcOrder, IcPlus, IcReload, IcRevert, IcSubmit, PgIcon, TableIcon } from "../icons";

// Theme image stories (#202): the main window with a Background image behind the SQL editor and the result grid (and nowhere else:
// the explorer, tab bar, grid header, status bar and the open menu keep their own surface) and a Corner image in front of the window,
// under the open menu. The images are the mock's made-up ones (src/lib/mock-images), read as the app reads them (theme_image)

const SQL = `-- orders this week, by customer
select c.name,
       count(*)      as orders,
       sum(o.total)  as spent
from shop.orders o
join shop.customers c on c.id = o.customer_id
where o.created_at >= now() - interval '7 days'
  and o.status <> 'cancelled'
group by c.name
order by spent desc
limit 20;`;
const ROWS: [string, string, string][] = [
  ["kim", "12", "1,240.50"],
  ["lee", "9", "980.00"],
  ["park", "7", "812.25"],
  ["choi", "6", "640.00"],
  ["jung", "5", "512.75"],
  ["kang", "4", "430.10"],
  ["cho", "4", "402.00"],
  ["yoon", "3", "318.40"],
  ["jang", "3", "290.00"],
  ["lim", "2", "188.90"],
];
const W = [44, 160, 90, 120];
const noop = () => {};

/** The grid as it was drawn over a Background image before #205: its colors opaque (only the odd rows a half tint), so the image showed
 *  little behind the data. Story-only, for comparing; the app has no such switch */
const BEFORE_205 = [
  [".grid-header, .cell.head.pinned", "var(--panel)"],
  [".cell.rownum", "var(--panel-2)"],
  [".cell.pinned", "var(--bg)"],
  [".grid-row.odd .cell.pinned", "var(--c-222326)"],
  [".cell.sel", "var(--c-2e436e)"],
  [".cell.changed", "var(--grid-changed)"],
  [".grid-row.added .cell:not(.rownum)", "var(--grid-added)"],
  [".grid-row.deleted .cell:not(.rownum)", "var(--grid-deleted)"],
  [".cell.match", "var(--grid-match)"],
  [".cell.head.col-sel, .cell.rownum.row-sel", "var(--c-33405a)"],
]
  .map(([sel, bg]) => `${sel.split(", ").map((x) => `.tints-205-off .theme-backdrop-host.has-image ${x}`).join(", ")} { background: ${bg}; }`)
  .join("\n");

const TCOLS: [string, string, number, "num" | undefined][] = [
  ["id", "int8", 70, "num"],
  ["user_name", "varchar", 150, undefined],
  ["amount", "numeric", 120, "num"],
  ["created_at", "timestamptz", 230, undefined],
  ["memo", "text", 110, undefined],
];
const TROWS = Array.from({ length: 18 }, (_, i) => [String(i + 1), `user_${i + 1}`, (i * 1234.5).toFixed(2), `2026-10-15 12:00:0${i % 3}.000 +09:00`, i % 3 ? "<null>" : "note"]);

/** A table tab: its toolbar, WHERE / ORDER BY, and the grid with a pinned key column, a selected cell, an edited cell, a new and a
 *  deleted row, the image behind the grid as TableEditor draws it (the real grid cells and CSS) */
function TableView({ slot, still, seeThrough }: { slot?: ThemeImages["background"]; still?: boolean; seeThrough?: boolean }) {
  const RN = 44;
  return (
    <div className="table-editor" style={{ position: "relative", flex: 1, minHeight: 0 }}>
      <ThemeBackdrop slot={slot} still={still} seeThrough={seeThrough} className="table-body">
        <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column" }}>
          <div className="grid-toolbar">
            <IconButton title="Reload" size="sm">
              <IcReload />
            </IconButton>
            <IconButton title="Add Row" size="sm">
              <IcPlus />
            </IconButton>
            <IconButton title="Delete Row" size="sm">
              <IcMinus />
            </IconButton>
            <IconButton title="Revert" size="sm">
              <IcRevert />
            </IconButton>
            <IconButton title="Submit" size="sm">
              <IcSubmit />
            </IconButton>
            <span className="muted">Tx: Auto · 500+ rows · 1 changed</span>
            <Spacer />
            <span className="muted">CSV</span>
            <IconButton title="Export" size="sm">
              <IcExport />
            </IconButton>
          </div>
          <div className="dg-filter">
            <div className="dg-filter-field">
              <IcFilter />
              <span className="kw">WHERE</span>
              <input aria-label="WHERE" readOnly />
            </div>
            <div className="dg-filter-field">
              <IcOrder />
              <span className="kw">ORDER BY</span>
              <input aria-label="ORDER BY" readOnly />
            </div>
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: "hidden", fontFamily: "var(--grid-font-family)", fontSize: "var(--grid-font-size)" }}>
            <GridHeaderRow width={RN + TCOLS.reduce((a, c) => a + c[2], 0)} height={26}>
              <div className="cell head rownum" style={{ width: RN }} />
              {TCOLS.map(([name, type, w], c) => (
                <div key={name} className={c === 0 ? "cell head pinned pin-edge" : c === 1 ? "cell head col-sel" : "cell head"} style={{ width: w }}>
                  <span className="col-name">{name}</span>
                  <span className="col-type">{type}</span>
                </div>
              ))}
            </GridHeaderRow>
            {TROWS.map((r, i) => (
              <div key={i} className={["grid-row", i % 2 ? "odd" : "", i === 4 ? "added" : "", i === 6 ? "deleted" : ""].join(" ")} role="row" style={{ position: "relative" }}>
                <RowNumberCell width={RN} selected={i === 2}>
                  {i === 4 ? "+" : r[0]}
                </RowNumberCell>
                {TCOLS.map(([name, , w, kind], c) => (
                  <GridCell
                    key={name}
                    width={w}
                    pin={c === 0 ? { left: RN, edge: true } : undefined}
                    kind={r[c] === "<null>" ? "null" : kind}
                    align={kind === "num" ? "right" : "left"}
                    selected={i === 2 && c === 1}
                    active={i === 2 && c === 1}
                    changed={i === 1 && c === 2}
                  >
                    {i === 4 && c > 0 ? (c === 1 ? "user_new" : "<null>") : i === 1 && c === 2 ? "99.00" : r[c]}
                  </GridCell>
                ))}
              </div>
            ))}
          </div>
        </div>
      </ThemeBackdrop>
    </div>
  );
}

/** The main window: explorer, tabs, the console (editor over the result grid, the image behind both), status bar; a menu open over the grid */
export function BackgroundWindow({
  images,
  still,
  menu = true,
  view = "console",
  tints = true,
}: {
  /** The theme in use's when not given */
  images?: ThemeImages;
  still?: boolean;
  menu?: boolean;
  /** A console (editor over its result) or a table tab */
  view?: "console" | "table";
  /** #205's tints of the grid's colors over the image; off draws the grid as before it (a story-only switch, for comparing) */
  tints?: boolean;
}) {
  const current = useThemeState().current.images;
  const { background, corner } = images ?? current ?? {};
  return (
    <div className={tints ? "bgwin" : "bgwin tints-205-off"} style={{ height: "100vh", display: "flex", flexDirection: "column", background: "var(--bg)", color: "var(--text)" }}>
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <div style={{ width: 210, flex: "none", background: "var(--panel)", borderRight: "1px solid var(--border)", paddingTop: 4 }}>
          <div className="conn-tree" style={{ boxShadow: `inset 3px 0 0 ${connStripe("#2f5634")}` }}>
            <TreeRow depth={0} open icon={<PgIcon />} label="[DEV] shop_app" sub="db.example.com" />
            <TreeRow depth={1} open icon={<DatabaseIcon />} label="shop" />
            <TreeRow depth={2} leaf icon={<TableIcon />} label="customers" />
            <TreeRow depth={2} leaf icon={<TableIcon />} label="orders" selected />
            <TreeRow depth={2} leaf icon={<TableIcon />} label="products" />
          </div>
        </div>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <div className="tabs ui-row" style={{ background: "var(--panel)", gap: 0 }}>
            <div className={view === "console" ? "tab active" : "tab"} style={connTabVars("#2f5634")}>
              <span className="tab-label">console</span>
            </div>
            <div className={view === "table" ? "tab active" : "tab"}>
              <span className="tab-label">users</span>
            </div>
          </div>
          {!tints && <style>{BEFORE_205}</style>}
          {view === "table" ? <TableView slot={background} still={still} /> : (
          <ThemeBackdrop slot={background} still={still} style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
            <div style={{ flex: "1 1 55%", minHeight: 0 }}>
              <Editor value={SQL} language="pgsql" theme={MONACO_THEME} options={{ readOnly: true, minimap: { enabled: false }, scrollBeyondLastLine: false, fontSize: 13, renderLineHighlight: "none", overviewRulerLanes: 0 }} />
            </div>
            <div style={{ flex: "1 1 45%", minHeight: 0, borderTop: "1px solid var(--border)", display: "flex", flexDirection: "column" }}>
              <div style={{ height: 28, flex: "none", background: "var(--panel)", borderBottom: "1px solid var(--border)", display: "flex", alignItems: "center", padding: "0 10px", fontSize: 12, color: "var(--muted)" }}>Result 1 · 10 rows · 42 ms</div>
              <div className="bgwin-grid" style={{ fontFamily: "var(--grid-font-family)", fontSize: "var(--grid-font-size)" }}>
                <GridHeaderRow width={W.reduce((a, b) => a + b, 0)} height={26}>
                  <div className="cell head rownum" style={{ width: W[0] }} />
                  <div className="cell head" style={{ width: W[1] }}>
                    <span className="col-name">name</span>
                  </div>
                  <div className="cell head" style={{ width: W[2] }}>
                    <span className="col-name">orders</span>
                  </div>
                  <div className="cell head" style={{ width: W[3] }}>
                    <span className="col-name">spent</span>
                  </div>
                </GridHeaderRow>
                {ROWS.map(([name, n, spent], i) => (
                  <div key={name} className={i % 2 ? "grid-row odd" : "grid-row"} role="row" style={{ position: "relative" }}>
                    <RowNumberCell width={W[0]}>{i + 1}</RowNumberCell>
                    <GridCell width={W[1]}>{name}</GridCell>
                    <GridCell width={W[2]} kind="num" align="right">
                      {n}
                    </GridCell>
                    <GridCell width={W[3]} kind="num" align="right">
                      {spent}
                    </GridCell>
                  </div>
                ))}
              </div>
            </div>
          </ThemeBackdrop>
          )}
        </div>
      </div>
      <div className="statusbar">
        <span>[DEV] shop_app · shop</span>
      </div>
      <ThemeSticker slot={corner} still={still} />
      {menu && (
        <Menu
          at={{ x: 560, y: 470 }}
          onClose={noop}
          items={[{ label: "Copy", shortcut: "⌘c", action: noop }, { label: "Edit Cell", shortcut: "↩", action: noop }, { separator: true }, { label: "Delete Row", shortcut: "⌘⌫", danger: true, action: noop }]}
        />
      )}
    </div>
  );
}

/**
 * Four tables split 2×2, as the editor area draws them with a Background image: with span "area" one picture behind all four (the
 * editor area's host draws it, the panes let it through), with "pane" one in each table. Tab strips, toolbars, the WHERE / ORDER BY
 * bars, headers and the split lines keep their own surface
 */
export function SplitWindow({ images, still }: { images: ThemeImages; still?: boolean }) {
  const bg = images.background;
  const area = !!bg && bg.span !== "pane";
  const group = (name: string, focused: boolean) => (
    <div className={cx("panel editor-group", focused ? "focused" : "unfocused")} style={{ minWidth: 0, minHeight: 0 }}>
      <div className="tabs ui-row" style={{ gap: 0 }}>
        <div className="tab active">
          <span className="tab-label">{name}</span>
        </div>
      </div>
      <div className="work-area" style={{ display: "flex", flexDirection: "column" }}>
        <TableView slot={area ? undefined : bg} seeThrough={area} still={still} />
      </div>
    </div>
  );
  return (
    <div style={{ height: "100vh", display: "flex", flexDirection: "column", background: "var(--bg)", color: "var(--text)" }}>
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <div style={{ width: 180, flex: "none", background: "var(--panel)", borderRight: "1px solid var(--border)", paddingTop: 4 }}>
          <TreeRow depth={0} open icon={<PgIcon />} label="[DEV] shop_app" />
          {["customers", "orders", "products", "users"].map((t) => (
            <TreeRow key={t} depth={1} leaf icon={<TableIcon />} label={t} />
          ))}
        </div>
        <ThemeBackdrop slot={area ? bg : undefined} still={still} className={cx("theme-editor-area", area && "span-area")} style={{ flex: 1, minWidth: 0 }}>
          <div style={{ height: "100%", display: "grid", gridTemplateColumns: "1fr 1px 1fr", gridTemplateRows: "1fr 1px 1fr" }}>
            {group("users", true)}
            <div className="resize-h" />
            {group("orders", false)}
            <div className="resize-v" />
            <div className="resize-v" />
            <div className="resize-v" />
            {group("customers", false)}
            <div className="resize-h" />
            {group("products", false)}
          </div>
        </ThemeBackdrop>
      </div>
      <div className="statusbar">
        <span>[DEV] shop_app · 4 tables</span>
      </div>
      <ThemeSticker slot={images.corner} still={still} />
    </div>
  );
}

export const SAMPLE_BACKGROUND = newBackgroundSlot(MOCK_IMAGES.ridges);
export const SAMPLE_CORNER = newCornerSlot(MOCK_IMAGES.beacon);

/**
 * The real Settings dialog over the main window, as in the app: a custom theme with these images is saved (in the mock) and picked,
 * and the theme editor opens on it. The editor draws its images live in the window behind (hold Peek to see it) and in its preview
 */
export function InEditor({ initial }: { initial: ThemeImages }) {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    saveCustomTheme({ id: "custom-pictures", name: "Pictures", base: "dark", colors: {}, images: initial })
      .then((t) => setThemePick(t.id))
      // Drawn instead of the toolbar's theme, so the editor starts from it
      .then(() => (previewTheme(null), setReady(true)));
  }, []);
  return (
    <>
      <BackgroundWindow menu={false} />
      {ready && <SettingsDialog initialPage="theme" onClose={noop} onUpdate={noop} onChannel={noop} />}
    </>
  );
}
