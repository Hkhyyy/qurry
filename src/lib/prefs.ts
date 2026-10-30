import { monaco } from "./monaco";

/** Editor font size (⌃⇧. / ⌃⇧,). Works even if browser storage is blocked */
const KEY = "dbtool.editorFontSize";
export const DEFAULT_FONT = 13;
let size = (() => {
  try {
    return Number(localStorage.getItem(KEY)) || DEFAULT_FONT;
  } catch {
    return DEFAULT_FONT;
  }
})();
export const editorFontSize = () => size;
export function setEditorFontSize(n: number) {
  size = Math.min(32, Math.max(8, n));
  try {
    localStorage.setItem(KEY, String(size));
  } catch {
    /* Ignore */
  }
  for (const ed of monaco.editor.getEditors()) ed.updateOptions({ fontSize: size });
  document.documentElement.style.setProperty("--grid-font", `${size}px`);
  return size;
}
