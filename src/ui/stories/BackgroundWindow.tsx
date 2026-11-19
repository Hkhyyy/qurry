import Editor from "@monaco-editor/react";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import "../../lib/monaco";
import { MONACO_THEME, connStripe, connTabVars, useThemeState } from "../../lib/theme";
import { backdropStyle, type Blend, type ThemeBackground } from "../../lib/themeBackground";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { BackgroundImageBar, BackgroundImageGroup, ThemeBackdrop } from "../../features/settings/ThemeBackground";
import { GridCell, GridHeaderRow, Menu, RowNumberCell, TreeRow } from "..";
import { DatabaseIcon, PgIcon, TableIcon } from "../icons";

// Theme background image stories (#202): made-up images drawn in code, and the main window with the image behind the SQL editor and
// the result grid (and nowhere else: the explorer, tab bar, grid header, status bar and the open menu keep their own surface)

const svg = (s: string) => `data:image/svg+xml;utf8,${encodeURIComponent(s)}`;
/** Ridges under a pale sky: from near white to near black, the worst case for contrast */
export const RIDGES = svg(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1600 1000" width="1600" height="1000">
<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#e9eef4"/><stop offset="1" stop-color="#93a5b9"/></linearGradient></defs>
<rect width="1600" height="1000" fill="url(#s)"/><circle cx="1180" cy="250" r="110" fill="#fbf8f1"/>
<path d="M0 560 L220 430 L420 520 L640 380 L880 500 L1100 400 L1340 480 L1600 410 V1000 H0Z" fill="#70839a"/>
<path d="M0 680 L260 560 L520 650 L760 540 L1020 640 L1300 560 L1600 640 V1000 H0Z" fill="#46566b"/>
<path d="M0 800 L300 700 L600 780 L900 690 L1200 790 L1600 720 V1000 H0Z" fill="#26303d"/>
<path d="M0 920 L400 850 L800 910 L1200 860 L1600 900 V1000 H0Z" fill="#0d1117"/></svg>`);
/** A small tile for Tile */
export const DOTS = svg(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="48" height="48">
<rect width="48" height="48" fill="#8a96a3"/><circle cx="12" cy="12" r="7" fill="#f2f4f7"/><circle cx="36" cy="36" r="7" fill="#141a21"/></svg>`);

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
export function BackgroundWindow({ image, src, blend, menu = true }: { image: ThemeBackground | null; src: string; blend: Blend; menu?: boolean }) {
  const { current } = useThemeState();
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
          <ThemeBackdrop image={image} src={src} kind={current.kind} blend={blend} style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
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

/** The made-up files and their images */
export const FILES: Record<string, string> = { "ridges.webp": RIDGES, "dots.png": DOTS };
export const SAMPLE: ThemeBackground = { file: "ridges.webp", opacity: 60, fill: "cover", anchor: "center" };

/**
 * The real theme editor (Settings › Theme) with an option's section put where it would go: A's rows at the end of the colors column,
 * B's bar over the preview. The image layer is put in the editor's own preview, which draws the colors being edited (a mockup: the
 * editor doesn't take the section yet, so they are placed into its DOM once it is drawn)
 */
export function InEditor({ option, initial }: { option: "A" | "B"; initial: ThemeBackground | null }) {
  const { current } = useThemeState();
  const [image, setImage] = useState(initial);
  const [slot, setSlot] = useState<{ section: HTMLElement; layer: HTMLElement } | null>(null);
  const blend: Blend = option === "A" ? "one-way" : "both";
  const src = FILES[image?.file ?? ""] ?? RIDGES;
  useEffect(() => {
    const t = setInterval(() => {
      const tokens = document.querySelector(".theme-tokens");
      const preview = document.querySelector<HTMLElement>(".theme-preview");
      if (!tokens || !preview) return;
      clearInterval(t);
      const section = document.createElement("div");
      if (option === "A") tokens.insertBefore(section, tokens.querySelector(".theme-advanced"));
      else preview.parentElement!.insertBefore(section, preview.parentElement!.firstChild);
      const layer = document.createElement("div");
      layer.className = "theme-backdrop";
      layer.setAttribute("aria-hidden", "true");
      preview.classList.add("theme-backdrop-host");
      preview.insertBefore(layer, preview.firstChild);
      setSlot({ section, layer });
    }, 50);
    return () => clearInterval(t);
  }, []);
  // A's rows come after Basic colors: scrolled to, as the user would
  useEffect(() => void (option === "A" && slot?.section.scrollIntoView({ block: "nearest" })), [slot]);
  useEffect(() => {
    if (!slot) return;
    slot.layer.removeAttribute("style");
    if (image) Object.assign(slot.layer.style, backdropStyle(image, src, current.kind, blend));
  }, [slot, image, src, current.kind]);
  const props = { image, src, p: current.colors, kind: current.kind, onPick: () => setImage(SAMPLE), onChange: setImage };
  return (
    <>
      <SettingsDialog initialPage="theme" onClose={noop} onUpdate={noop} onChannel={noop} />
      {slot && createPortal(option === "A" ? <BackgroundImageGroup {...props} /> : <BackgroundImageBar {...props} />, slot.section)}
    </>
  );
}
