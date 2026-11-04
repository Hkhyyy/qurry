import { useSyncExternalStore } from "react";
import type { FontFamily } from "./api";
import { monaco } from "./monaco";

/** Settings › Appearance fonts for the editor (Monaco; size also ⌃⇧. / ⌃⇧,) and the result grid. Works even if browser storage is blocked */
export type FontTarget = "editor" | "grid";
/** family "" = the default monospace stack */
export type Font = { family: string; size: number };

/** Same stack as --ui-font-mono (tokens.css): the fallback after any chosen family */
export const MONO = `"JetBrains Mono", Menlo, monospace`;
export const DEFAULT_FONT = 13;
export const DEFAULTS: Record<FontTarget, Font> = { editor: { family: "", size: DEFAULT_FONT }, grid: { family: "", size: 12 } };
/** Grid rows are 24px high (ResultGrid ROW_H): larger text would be cut */
export const SIZES: Record<FontTarget, [number, number]> = { editor: [8, 32], grid: [9, 16] };
// The editor size key predates the rest: kept so the size saved by earlier versions stays
const KEYS: Record<FontTarget, { family: string; size: string }> = {
  editor: { family: "dbtool.editorFontFamily", size: "dbtool.editorFontSize" },
  grid: { family: "dbtool.gridFontFamily", size: "dbtool.gridFontSize" },
};

/** CSS font-family for a chosen family: quoted (quotes, commas and other CSS syntax dropped, so a pasted "A, B" stays one family) before the monospace fallback */
export const fontStack = (family: string) => {
  const f = family.replace(/["\\;{},]/g, "").trim();
  return f ? `"${f}", ${MONO}` : MONO;
};

const clamp = (t: FontTarget, n: number) => Math.min(SIZES[t][1], Math.max(SIZES[t][0], Math.round(n)));
const read = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const loadFont = (t: FontTarget): Font => ({ family: read(KEYS[t].family) ?? "", size: clamp(t, Number(read(KEYS[t].size)) || DEFAULTS[t].size) });

let fonts: Record<FontTarget, Font> = { editor: loadFont("editor"), grid: loadFont("grid") };
const subs = new Set<() => void>();

function apply(t: FontTarget) {
  const { family, size } = fonts[t];
  if (t === "editor") for (const ed of monaco.editor.getEditors()) ed.updateOptions({ fontFamily: fontStack(family), fontSize: size });
  else {
    document.documentElement.style.setProperty("--grid-font-family", fontStack(family));
    document.documentElement.style.setProperty("--grid-font-size", `${size}px`);
  }
}
apply("grid");

export const font = (t: FontTarget) => fonts[t];
/** Change, save and apply live. Returns the font in effect (size clamped) */
export function setFont(t: FontTarget, f: Partial<Font>): Font {
  const next = { ...fonts[t], ...f };
  next.size = clamp(t, next.size);
  fonts = { ...fonts, [t]: next };
  try {
    localStorage.setItem(KEYS[t].family, next.family);
    localStorage.setItem(KEYS[t].size, String(next.size));
  } catch {
    /* Still applied for this run even if saving fails */
  }
  apply(t);
  subs.forEach((s) => s());
  return next;
}

/** Current fonts (re-renders on change) */
export function useFonts(): Record<FontTarget, Font> {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => fonts,
  );
}

/** One row of the font picker: family "" = the default stack; missing = saved but not among the installed families */
export type FontOption = { family: string; missing?: boolean };
export const DEFAULT_FONT_LABEL = "Default monospace";

/** Font picker rows: the default, the saved family if it isn't installed (so it still shows, marked), then the installed families
 * (monospaced only with monoOnly — the current one stays either way), all matching query. installed null or empty (still loading,
 * or the OS can't list them): nothing is marked missing */
export function fontOptions(installed: FontFamily[] | null, current: string, monoOnly: boolean, query: string): FontOption[] {
  const cur = current.trim().toLowerCase();
  const isCur = (name: string) => name.toLowerCase() === cur;
  const known = !!installed?.length;
  const rows: FontOption[] = [
    { family: "" },
    ...(cur && !installed?.some((f) => isCur(f.name)) ? [{ family: current.trim(), missing: known }] : []),
    ...(installed ?? []).filter((f) => !monoOnly || f.mono || isCur(f.name)).map((f) => ({ family: f.name })),
  ];
  const q = query.trim().toLowerCase();
  return q ? rows.filter((r) => (r.family || DEFAULT_FONT_LABEL).toLowerCase().includes(q)) : rows;
}
