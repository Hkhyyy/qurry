// The result grid's Value Editor panel (DataGrip's value editor side panel): it shows the grid's current cell and follows the
// selection; an edit typed into it is applied when the selection leaves that cell (as DataGrip does) or on ⌘↩.
// How values are shown and written back (JSON layout, invalid JSON) is qurry_shared::value, shared with the TUI's value panel
import { sharedOr } from "./shared";

/** Text typed in the panel (or re-laid out with "{ }") for the cell at data row r, column c */
export interface PanelDraft {
  r: number;
  c: number;
  text: string;
  /** What the panel showed for the cell before: a draft still equal to it changes nothing */
  base: string;
  /** The cell can be written (an editable result, the row not deleted, a value shown as it is) */
  editable: boolean;
}

type Pos = { r: number; c: number };

/** The draft is for the cell at `at` */
export const draftAt = (d: PanelDraft | null, at: Pos | null): d is PanelDraft => !!d && !!at && d.r === at.r && d.c === at.c;

/** The selection is at `at` now: the draft of the cell it left, or null when there is none or the selection is still on it */
export const leftDraft = (d: PanelDraft | null, at: Pos | null): PanelDraft | null => (d && !draftAt(d, at) ? d : null);

/** What the panel shows for a cell: json = highlight it as JSON (pretty-printed when valid) */
export const panelText = (typeName: string, value: string) => sharedOr({ json: false, text: value }, (s) => s.panelText(typeName, value));

/** The "{ }" toggle: valid JSON between pretty and one line, unchanged otherwise */
export const toggleJsonLayout = (text: string) => sharedOr(undefined, (s) => s.toggleJsonLayout(text)) ?? text;

/** The value a draft writes to its cell (`original` = the cell's value, `typeName` = its column's type): null when it writes nothing
 *  (read-only, or unchanged), error when a json / jsonb column refuses it */
export function draftToApply(d: PanelDraft, typeName: string, original: string): { text: string } | { error: string } | null {
  if (!d.editable || d.text === d.base) return null;
  const r = sharedOr<{ Ok: string } | { Err: string }>({ Ok: d.text }, (s) => s.valueToApply(typeName, original, d.text));
  return "Ok" in r ? { text: r.Ok } : { error: r.Err };
}

// ─── Closing from the keyboard ───
// ⌘w (Close Tab's key) and ⇧⎋ close the panel of the grid that has the focus (in the grid or in the panel) before anything else

const closers = new WeakMap<Element, () => void>();

/** While a grid's panel is open: how to close it (`wrap` = the grid's .grid-wrap). Returns the unregister function */
export function registerValuePanel(wrap: Element, close: () => void): () => void {
  closers.set(wrap, close);
  return () => closers.delete(wrap);
}

/** Close the Value Editor panel of the focused grid; false when the focus is not in a grid with an open panel */
export function closeFocusedValuePanel(): boolean {
  const wrap = (document.activeElement as HTMLElement | null)?.closest?.(".grid-wrap");
  const close = wrap && closers.get(wrap);
  if (!close) return false;
  close();
  return true;
}
