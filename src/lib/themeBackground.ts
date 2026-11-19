// A theme's images (#202, GUI only: a terminal draws characters, so the TUI ignores them), two independent slots, each optional with its
// own image: Background, faint behind the SQL editor and the result grid; Corner, scaled down in front of the window (below menus,
// dialogs and popups), a sticker.
// The background costs contrast: drawn at opacity α, the image moves each pixel of Background (and the grid's odd rows) toward its own pixel
// by α; the worst pixel is black or white, so the checks (qurry_shared::theme, through the WASM) are run with those surfaces mixed
// that far toward black and white. GIF and animated WebP play, and stop on their first frame while motion is paused (stillFrame).
import type { CSSProperties } from "react";
import type { Palette, ThemeWarning } from "./theme";

export type Fill = "cover" | "contain" | "tile" | "center";
export const FILLS: { id: Fill; label: string }[] = [
  { id: "cover", label: "Cover" },
  { id: "contain", label: "Contain" },
  { id: "tile", label: "Tile" },
  { id: "center", label: "Center" },
];
/** Where the image sits (CSS background-position): DataGrip's nine anchors, row by row */
export const ANCHORS = ["left top", "center top", "right top", "left center", "center", "right center", "left bottom", "center bottom", "right bottom"] as const;
export type Anchor = (typeof ANCHORS)[number];
export const CORNERS = ["left top", "right top", "left bottom", "right bottom"] as const;
export type Corner = (typeof CORNERS)[number];
/** Corner's height, % of the window */
export const SIZE_MIN = 10;
export const SIZE_MAX = 40;

/** file: the copy in the config folder; opacity 0–100 */
export interface BackgroundSlot {
  file: string;
  opacity: number;
  fill: Fill;
  anchor: Anchor;
}
export interface CornerSlot {
  file: string;
  corner: Corner;
  size: number;
  opacity: number;
}
/** A theme's images: either slot, both or none */
export interface ThemeImages {
  background?: BackgroundSlot;
  corner?: CornerSlot;
}
/** A new image's settings: DataGrip's 15% in the background; a corner sticker bottom right, fully opaque */
export const newBackgroundSlot = (file: string): BackgroundSlot => ({ file, opacity: 15, fill: "cover", anchor: "center" });
export const newCornerSlot = (file: string): CornerSlot => ({ file, corner: "right bottom", size: 20, opacity: 100 });

/** The surfaces the image is behind */
const SURFACES = ["bg", "odd"];

const hex2 = (n: number) => Math.round(n).toString(16).padStart(2, "0");
/** a mixed toward b by t (0–1), in sRGB as CSS opacity mixes */
export function mixHex(a: string, b: string, t: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(1 + 2 * i, 3 + 2 * i), 16);
  return "#" + [0, 1, 2].map((i) => hex2(ch(a, i) * (1 - t) + ch(b, i) * t)).join("");
}

/** Background: the checks that fail over the image's worst pixels at opacity (0–100) and pass without the image, one per token */
export function warningsOver(p: Palette, opacity: number, warnings: (p: Palette) => ThemeWarning[]): ThemeWarning[] {
  const before = new Set(warnings(p).map((w) => w.token));
  const found = new Map<string, ThemeWarning>();
  for (const x of ["#000000", "#ffffff"]) {
    const over = { ...p, ...Object.fromEntries(SURFACES.map((s) => [s, mixHex(p[s], x, opacity / 100)])) };
    for (const w of warnings(over)) if (!before.has(w.token) && !found.has(w.token)) found.set(w.token, w);
  }
  return [...found.values()];
}

/** Background: the highest opacity (whole %) at which no check fails over the image that passes without it */
export function opacityCap(p: Palette, warnings: (p: Palette) => ThemeWarning[]): number {
  let [lo, hi] = [0, 100];
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (warningsOver(p, mid, warnings).length === 0) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

const SIZE: Record<Fill, string> = { cover: "cover", contain: "contain", tile: "auto", center: "auto" };
/** Background: the image layer's style (src is the image's URL) */
export const backdropStyle = (b: BackgroundSlot, src: string): CSSProperties => ({
  backgroundImage: `url("${src}")`,
  backgroundSize: SIZE[b.fill],
  backgroundRepeat: b.fill === "tile" ? "repeat" : "no-repeat",
  backgroundPosition: b.anchor,
  opacity: b.opacity / 100,
});

/** Corner: the sticker's place, height and opacity; margin from the edges in px. In the window its height is a share of the window's
 *  (vh); in the editor's preview, of the preview's (%) */
export function stickerStyle(c: CornerSlot, margin = 16, unit: "vh" | "%" = "vh"): CSSProperties {
  const [x, y] = c.corner.split(" ");
  return { [x]: margin, [y]: margin, height: `${c.size}${unit}`, maxWidth: `calc(100${unit === "vh" ? "vw" : "%"} - ${2 * margin}px)`, opacity: c.opacity / 100 };
}

/** An image's first frame as a still (a PNG data URL): a GIF or an animated WebP drawn into a canvas once loaded */
export function stillFrame(src: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas");
      [c.width, c.height] = [img.naturalWidth || 300, img.naturalHeight || 150];
      c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
      resolve(c.toDataURL());
    };
    img.onerror = () => reject(new Error("image didn't load"));
    img.src = src;
  });
}
