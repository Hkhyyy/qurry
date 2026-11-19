// A theme's background image (#202, GUI only: a terminal draws characters, so the TUI ignores it): how it is drawn behind the SQL
// editor and the result grid, and what it costs the theme's contrast checks. Drawn at opacity α, the image moves each pixel of
// Background (and the grid's odd rows) toward its own pixel by α; the worst pixel is black or white, so the checks (qurry_shared::theme,
// through the WASM) are run with those surfaces mixed that far toward black and white.
import type { CSSProperties } from "react";
import type { Kind, Palette, ThemeWarning } from "./theme";

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

export interface ThemeBackground {
  /** The copy in the config folder */
  file: string;
  /** 0–100 */
  opacity: number;
  fill: Fill;
  anchor: Anchor;
}
/**
 * "one-way": the image only darkens a dark theme's Background (multiply; a light one's only lightens, screen), so text, drawn lighter
 * (darker), keeps the contrast it has, and the image's light and dark parts all show as shades; "both": the image is drawn as it is
 * (DataGrip's), and costs contrast above the cap
 */
export type Blend = "one-way" | "both";

/** The surfaces the image is behind */
const SURFACES = ["bg", "odd"];

const hex2 = (n: number) => Math.round(n).toString(16).padStart(2, "0");
/** a mixed toward b by t (0–1), in sRGB as CSS opacity mixes */
export function mixHex(a: string, b: string, t: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(1 + 2 * i, 3 + 2 * i), 16);
  return "#" + [0, 1, 2].map((i) => hex2(ch(a, i) * (1 - t) + ch(b, i) * t)).join("");
}

/** The colors the image's pixels can push Background toward */
const extremes = (kind: Kind, blend: Blend) => (blend === "both" ? ["#000000", "#ffffff"] : [kind === "light" ? "#ffffff" : "#000000"]);

/** Whether the image can show at all: one-way on a black (white) Background has no room */
export const canShow = (p: Palette, kind: Kind, blend: Blend) => extremes(kind, blend).some((x) => x !== p.bg.toLowerCase());

/** The checks that fail over the image's worst pixels at opacity (0–100) and pass without the image, one per token */
export function warningsOver(p: Palette, kind: Kind, blend: Blend, opacity: number, warnings: (p: Palette) => ThemeWarning[]): ThemeWarning[] {
  const before = new Set(warnings(p).map((w) => w.token));
  const found = new Map<string, ThemeWarning>();
  for (const x of extremes(kind, blend)) {
    const over = { ...p, ...Object.fromEntries(SURFACES.map((s) => [s, mixHex(p[s], x, opacity / 100)])) };
    for (const w of warnings(over)) if (!before.has(w.token) && !found.has(w.token)) found.set(w.token, w);
  }
  return [...found.values()];
}

/** The highest opacity (whole %) at which no check fails over the image that passes without it */
export function opacityCap(p: Palette, kind: Kind, blend: Blend, warnings: (p: Palette) => ThemeWarning[]): number {
  let [lo, hi] = [0, 100];
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (warningsOver(p, kind, blend, mid, warnings).length === 0) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

const SIZE: Record<Fill, string> = { cover: "cover", contain: "contain", tile: "auto", center: "auto" };
/** The image layer's style: src is the image's URL */
export const backdropStyle = (b: ThemeBackground, src: string, kind: Kind, blend: Blend): CSSProperties => ({
  backgroundImage: `url("${src}")`,
  backgroundSize: SIZE[b.fill],
  backgroundRepeat: b.fill === "tile" ? "repeat" : "no-repeat",
  backgroundPosition: b.anchor,
  opacity: b.opacity / 100,
  mixBlendMode: blend === "both" ? "normal" : kind === "light" ? "screen" : "multiply",
});
