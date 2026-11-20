// A custom theme's images (#202, GUI only: a terminal draws characters, so the TUI keeps them in settings.json and never draws them),
// two independent slots, each optional with its own image: Background, faint behind the SQL editor and the result grid; Corner, scaled
// down in front of the window (below menus, dialogs and popups), a sticker. The files are copies in the config folder
// (crates/core/src/theme_images.rs, which checks them); the GUI reads their bytes and draws them from blob URLs (holdImage), freed when nothing draws them.
// The background costs contrast: drawn at opacity α, the image moves each pixel of Background (and the grid's odd rows) toward its own pixel
// by α; the worst pixel is black or white, so the checks (qurry_shared::theme, through the WASM) are run with those surfaces mixed
// that far toward black and white. GIF and animated WebP play, and stop on their first frame while motion is paused (stillFrame).
import type { CSSProperties } from "react";
import { api } from "./api";
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
/** Two themes' images are the same, whatever order their fields come in (settings.json writes them in its own) */
export function sameImages(a: ThemeImages | undefined, b: ThemeImages | undefined): boolean {
  const key = (i: ThemeImages | undefined) => JSON.stringify(i ?? {}, (_, v) => (v && typeof v === "object" ? Object.fromEntries(Object.entries(v).sort()) : v));
  return key(a) === key(b);
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

const MIME: Record<string, string> = { png: "image/png", jpg: "image/jpeg", gif: "image/gif", webp: "image/webp" };
/** A GIF or WebP may move (they are frozen while motion is paused); a PNG or JPEG is drawn as it is */
export const mayMove = (file: string) => /\.(gif|webp)$/.test(file);

/** Each image in use: its blob URL, its still once made, and how many drawings use it. The last one to let go frees both */
const images = new Map<string, { url: Promise<string>; still?: Promise<string>; users: number }>();
const revoke = (u?: Promise<string>) => u?.then((x) => URL.revokeObjectURL(x), () => {});

/** A theme image's blob URL, read once while it is used (the bytes command: no asset protocol or CSP change needed); each call is
 *  one use, ended by releaseImage. A file that can't be read is read again by the next use */
export function holdImage(file: string): Promise<string> {
  let e = images.get(file);
  if (!e) {
    const url = api.themeImage(file).then((b) => URL.createObjectURL(new Blob([b], { type: MIME[file.split(".").pop() ?? ""] ?? "" })));
    e = { url, users: 0 };
    const entry = e;
    url.catch(() => images.get(file) === entry && images.delete(file));
    images.set(file, e);
  }
  e.users++;
  return e.url;
}

/** One use of a file ends; with none left its blob URL and still are freed */
export function releaseImage(file: string) {
  const e = images.get(file);
  if (!e || --e.users > 0) return;
  images.delete(file);
  revoke(e.url);
  revoke(e.still);
}

/** A GIF's or WebP's first frame as a still (a blob URL), made once while the file is used: drawn into a canvas from a fresh load,
 *  which is its first frame. Only for a file in use (holdImage) */
export function stillFrame(file: string): Promise<string> {
  const e = images.get(file);
  if (!e) return Promise.reject(new Error("not in use"));
  e.still ??= e.url.then(
    (src) =>
      new Promise<string>((resolve, reject) => {
        const img = new Image();
        img.onload = () => {
          const c = document.createElement("canvas");
          [c.width, c.height] = [img.naturalWidth || 300, img.naturalHeight || 150];
          c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
          c.toBlob((b) => (b ? resolve(URL.createObjectURL(b)) : reject(new Error("no still"))));
        };
        img.onerror = () => reject(new Error("image didn't load"));
        img.src = src;
      }),
  );
  const still = e.still;
  still.catch(() => e.still === still && (e.still = undefined));
  return still;
}
