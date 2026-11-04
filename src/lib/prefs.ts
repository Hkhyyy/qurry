import { useSyncExternalStore } from "react";
import { api, type CustomFont, type FontFamily } from "./api";
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

const subscribe = (f: () => void) => (subs.add(f), () => void subs.delete(f));
/** Current fonts (re-renders on change) */
export function useFonts(): Record<FontTarget, Font> {
  return useSyncExternalStore(subscribe, () => fonts);
}

/** Font files added in Settings, and the FontFace registered for each (by file) */
let custom: CustomFont[] = [];
const faces = new Map<string, FontFace>();
async function registerCustomFonts() {
  const list = await api.customFonts();
  for (const [file, face] of faces)
    if (!list.some((f) => f.file === file)) {
      document.fonts.delete(face);
      faces.delete(file);
    }
  await Promise.all(
    list
      .filter((f) => !faces.has(f.file))
      .map(async (f) => {
        try {
          const face = await new FontFace(f.name, await api.fontFile(f.file)).load();
          document.fonts.add(face);
          faces.set(f.file, face);
        } catch (e) {
          console.warn(`The font file ${f.file} could not be loaded (${e}); it renders with the default font`);
        }
      }),
  );
  custom = list;
  monaco.editor.remeasureFonts();
  subs.forEach((s) => s());
}
let loading: Promise<void> = Promise.resolve();
/** List the added font files and register each new one with FontFace; a removed one is unregistered, so a font still set to it falls
 *  back to the default like one that isn't installed. At startup and after adding or removing one; one run at a time */
export const loadCustomFonts = () => (loading = loading.then(registerCustomFonts, registerCustomFonts));
/** "Add Font File…" (the backend opens the file dialog and copies the font): the added font, registered; null if cancelled.
 *  A file the WebView can't load as a font (truncated, or a format it doesn't take) is removed again and throws the reason */
export async function addCustomFont(): Promise<CustomFont | null> {
  const f = await api.addFontFile();
  if (!f) return null;
  await loadCustomFonts();
  if (!faces.has(f.file)) {
    await api.removeFontFile(f.file);
    await loadCustomFonts();
    throw `${f.name} couldn't be loaded as a font (the file may be damaged or incomplete)`;
  }
  return f;
}
/** Deletes an added font file; an editor or grid font set to it goes back to the default, unless the OS lists a font of that name
 *  (installed), which renders again once the added one is gone */
export async function removeCustomFont(f: CustomFont, installed: FontFamily[] | null) {
  await api.removeFontFile(f.file);
  const name = f.name.toLowerCase();
  if (!installed?.some((i) => i.name.toLowerCase() === name))
    for (const t of ["editor", "grid"] as const) if (fonts[t].family.trim().toLowerCase() === name) setFont(t, { family: "" });
  await loadCustomFonts();
}
/** The added font files (re-renders when they change) */
export function useCustomFonts(): CustomFont[] {
  return useSyncExternalStore(subscribe, () => custom);
}

/** One row of the font picker: family "" = the default stack; missing = saved but neither installed nor added; custom = an added font file */
export type FontOption = { family: string; missing?: boolean; custom?: boolean };
export const DEFAULT_FONT_LABEL = "Default monospace";

/** Font picker rows: the default, the saved family if it is neither installed nor added (so it still shows, marked), the added font
 * files (whatever monoOnly says: they were added on purpose; an installed family of the same name is left out, since the added one is
 * what renders), then the installed families (monospaced only with monoOnly — the current one stays either way), all matching query.
 * installed null or empty (still loading, or the OS can't list them): nothing is marked missing */
export function fontOptions(installed: FontFamily[] | null, custom: CustomFont[], current: string, monoOnly: boolean, query: string): FontOption[] {
  const cur = current.trim().toLowerCase();
  const isCur = (name: string) => name.toLowerCase() === cur;
  const isCustom = (name: string) => custom.some((c) => c.name.toLowerCase() === name.toLowerCase());
  const rows: FontOption[] = [
    { family: "" },
    ...(cur && !installed?.some((f) => isCur(f.name)) && !isCustom(cur) ? [{ family: current.trim(), missing: !!installed?.length }] : []),
    ...custom.map((c) => ({ family: c.name, custom: true })),
    ...(installed ?? []).filter((f) => !isCustom(f.name) && (!monoOnly || f.mono || isCur(f.name))).map((f) => ({ family: f.name })),
  ];
  const q = query.trim().toLowerCase();
  return q ? rows.filter((r) => (r.family || DEFAULT_FONT_LABEL).toLowerCase().includes(q)) : rows;
}
