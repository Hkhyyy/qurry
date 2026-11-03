import { useSyncExternalStore } from "react";
import { monaco } from "./monaco";

/** Settings › Appearance fonts for the editor (Monaco; size also ⌃⇧. / ⌃⇧,) and the result grid. Works even if browser storage is blocked */
export type FontTarget = "editor" | "grid";
/** family "" = the default monospace stack */
export type Font = { family: string; size: number };

/** Same stack as --ui-font-mono (tokens.css): the fallback after any chosen family */
export const MONO = `"JetBrains Mono", Menlo, monospace`;
export const DEFAULT_FONT = 13;
const DEFAULTS: Record<FontTarget, Font> = { editor: { family: "", size: DEFAULT_FONT }, grid: { family: "", size: 12 } };
/** Grid rows are 24px high (ResultGrid ROW_H): larger text would be cut */
export const SIZES: Record<FontTarget, [number, number]> = { editor: [8, 32], grid: [9, 16] };
// The editor size key predates the rest: kept so the size saved by earlier versions stays
const KEYS: Record<FontTarget, { family: string; size: string }> = {
  editor: { family: "dbtool.editorFontFamily", size: "dbtool.editorFontSize" },
  grid: { family: "dbtool.gridFontFamily", size: "dbtool.gridFontSize" },
};

/** CSS font-family for a chosen family: quoted (quotes and other CSS syntax dropped) before the monospace fallback */
export const fontStack = (family: string) => {
  const f = family.replace(/["\\;{}]/g, "").trim();
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

/** Whether a font is installed: text in it measures differently from every generic fallback. Canvas missing: assume it is */
export function fontInstalled(family: string): boolean {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return true;
  const width = (f: string) => {
    ctx.font = `48px ${f}`;
    return ctx.measureText("mmmmmmmmmlli1WQ@#").width;
  };
  return ["monospace", "serif", "sans-serif"].some((g) => width(`${fontStack(family).split(",")[0]}, ${g}`) !== width(g));
}

/** Monospace fonts offered in Settings (only the installed ones are listed; any other family can be typed) */
export const KNOWN_MONO = [
  "Andale Mono", "Cascadia Code", "Cascadia Mono", "Consolas", "Courier New", "D2Coding", "DejaVu Sans Mono", "Fira Code", "Fira Mono",
  "Hack", "IBM Plex Mono", "Inconsolata", "Iosevka", "JetBrains Mono", "Liberation Mono", "Menlo", "Monaco", "Noto Sans Mono",
  "PT Mono", "Roboto Mono", "SF Mono", "Source Code Pro", "Ubuntu Mono", "Victor Mono",
];
