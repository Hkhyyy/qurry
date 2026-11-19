import Editor from "@monaco-editor/react";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import "../../lib/monaco";
import { MONACO_THEME, connStripe, connTabVars, useThemeState } from "../../lib/theme";
import { newBackground, type ThemeBackground } from "../../lib/themeBackground";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { BackgroundImageSection, ThemeBackdrop, ThemeSticker } from "../../features/settings/ThemeBackground";
import { GridCell, GridHeaderRow, Menu, RowNumberCell, TreeRow } from "..";
import { DatabaseIcon, PgIcon, TableIcon } from "../icons";

// Theme background image stories (#202): made-up images drawn in code, and the main window with the image Behind the SQL editor and
// the result grid (and nowhere else: the explorer, tab bar, grid header, status bar and the open menu keep their own surface) or in a
// Corner, in front of the window and under the open menu

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
/** An animated badge (stands in for a GIF): a ring that pulses around a dot */
export const BEACON = svg(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
<rect x="10" y="10" width="180" height="180" rx="36" fill="#2c6e63"/><circle cx="100" cy="100" r="22" fill="#f4d35e"/>
<circle cx="100" cy="100" r="30" fill="none" stroke="#f4d35e" stroke-width="8"><animate attributeName="r" values="30;70;30" dur="1.6s" repeatCount="indefinite"/>
<animate attributeName="opacity" values="1;0;1" dur="1.6s" repeatCount="indefinite"/></circle></svg>`);

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
export function BackgroundWindow({ image, src, still, menu = true }: { image: ThemeBackground | null; src: string; still?: boolean; menu?: boolean }) {
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
          <ThemeBackdrop image={image} src={src} still={still} style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
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
      <ThemeSticker image={image} src={src} still={still} />
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
export const FILES: Record<string, string> = { "ridges.webp": RIDGES, "dots.png": DOTS, "beacon.gif": BEACON };
export const SAMPLE = newBackground("ridges.webp");

/**
 * The real Settings dialog over the main window, as in the app: the theme editor with the section over its preview, the image applied
 * live to the window behind (Peek fades Settings to show it) and to the preview (behind its editor and grid only, or in its corner).
 * A mockup: the editor doesn't take the section yet, so it and the preview's image are put into the editor's DOM once it is drawn.
 * src: a file's image other than FILES' (the local preview's)
 */
export function InEditor({ initial, src: own }: { initial: ThemeBackground | null; src?: string }) {
  const { current } = useThemeState();
  const [image, setImage] = useState(initial);
  const [slot, setSlot] = useState<{ section: HTMLElement; layer: HTMLElement; sticker: HTMLElement } | null>(null);
  const src = own ?? FILES[image?.file ?? ""] ?? RIDGES;
  useEffect(() => {
    const t = setInterval(() => {
      const preview = document.querySelector<HTMLElement>(".theme-preview");
      const editor = preview?.querySelector<HTMLElement>(".theme-preview-editor");
      const grid = preview?.querySelector<HTMLElement>(".theme-preview-grid");
      if (!preview || !editor || !grid) return;
      clearInterval(t);
      const section = document.createElement("div");
      preview.parentElement!.insertBefore(section, preview.parentElement!.firstChild);
      // The preview's image covers its editor and grid only (not its explorer, tabs or notes), as one picture
      const [P, E, G] = [preview, editor, grid].map((el) => el.getBoundingClientRect());
      const [x, gy, w, h] = [E.left - P.left, G.top - E.top, P.width, G.bottom - E.top];
      const layer = document.createElement("div");
      Object.assign(layer.style, { position: "absolute", left: "0", top: `${E.top - P.top}px`, width: `${w}px`, height: `${h}px`, clipPath: `polygon(${x}px 0, ${w}px 0, ${w}px ${h}px, 0 ${h}px, 0 ${gy}px, ${x}px ${gy}px)` });
      const sticker = document.createElement("div");
      Object.assign(sticker.style, { position: "absolute", inset: "0", pointerEvents: "none", zIndex: "1" });
      preview.style.position = "relative";
      // Over the layer: the preview's parts (positioned, so they paint above it), the editor and grid with clear surfaces
      [...preview.children].forEach((el) => ((el as HTMLElement).style.position = "relative"));
      preview.insertBefore(layer, preview.firstChild);
      preview.appendChild(sticker);
      [editor, grid].forEach((el) => (el.classList.add("theme-backdrop-host"), (el.style.background = "transparent")));
      setSlot({ section, layer, sticker });
    }, 50);
    return () => clearInterval(t);
  }, []);
  return (
    <>
      <BackgroundWindow image={image} src={src} menu={false} />
      <SettingsDialog initialPage="theme" onClose={noop} onUpdate={noop} onChannel={noop} />
      {slot && createPortal(<BackgroundImageSection image={image} src={src} p={current.colors} onPick={() => setImage(SAMPLE)} onChange={setImage} />, slot.section)}
      {slot && createPortal(<ThemeBackdrop image={image} src={src} style={{ position: "absolute", inset: 0 }} />, slot.layer)}
      {slot && createPortal(<ThemeSticker image={image} src={src} inPreview />, slot.sticker)}
    </>
  );
}
