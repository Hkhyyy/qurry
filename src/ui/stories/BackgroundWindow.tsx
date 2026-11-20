import Editor from "@monaco-editor/react";
import { useEffect, useState } from "react";
import "../../lib/monaco";
import { MOCK_IMAGES } from "../../lib/mock";
import { MONACO_THEME, connStripe, connTabVars, previewTheme, saveCustomTheme, setThemePick, useThemeState } from "../../lib/theme";
import { newBackgroundSlot, newCornerSlot, type ThemeImages } from "../../lib/themeImages";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { ThemeBackdrop, ThemeSticker } from "../../features/settings/ThemeImages";
import { GridCell, GridHeaderRow, Menu, RowNumberCell, TreeRow } from "..";
import { DatabaseIcon, PgIcon, TableIcon } from "../icons";

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

/** The main window: explorer, tabs, the console (editor over the result grid, the image behind both), status bar; a menu open over the grid */
export function BackgroundWindow({ images, still, menu = true }: { /** The theme in use's when not given */ images?: ThemeImages; still?: boolean; menu?: boolean }) {
  const current = useThemeState().current.images;
  const { background, corner } = images ?? current ?? {};
  return (
    <div className="bgwin" style={{ height: "100vh", display: "flex", flexDirection: "column", background: "var(--bg)", color: "var(--text)" }}>
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
            <div className="tab active" style={connTabVars("#2f5634")}>
              <span className="tab-label">console</span>
            </div>
            <div className="tab">
              <span className="tab-label">orders</span>
            </div>
          </div>
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
