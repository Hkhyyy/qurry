/** Logic of the list popup under a field (PopupList · ComboBox) and of a dialog being moved (Modal), kept pure for the tests */

/** Suggestions for what's typed: case-insensitive, an exact match first, then the ones starting with it, then the ones containing it */
export function matchOptions(options: string[], query: string): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return options;
  const rank = (o: string) => {
    const l = o.toLowerCase();
    return l === q ? 0 : l.startsWith(q) ? 1 : l.includes(q) ? 2 : 3;
  };
  return options
    .map((o) => [rank(o), o] as const)
    .filter(([r]) => r < 3)
    .sort((a, b) => a[0] - b[0])
    .map(([, o]) => o);
}

/** A key in a list popup of `count` rows: the new highlighted row (↑↓ PgUp PgDn), "pick" (↩), "close" (⎋), or null when it isn't the list's */
export function listKey(key: string, active: number, count: number): number | "pick" | "close" | null {
  const step = ({ ArrowDown: 1, ArrowUp: -1, PageDown: 10, PageUp: -10 } as Record<string, number>)[key];
  if (step) return Math.max(0, Math.min(count - 1, active + step));
  if (key === "Enter") return count ? "pick" : null;
  if (key === "Escape") return "close";
  return null;
}

/**
 * A key in a menu (DataGrip, macOS): the stop to focus next, or null when the key doesn't move. `stops` are the labels of the
 * items the keys reach (disabled ones and separators left out; null for a stop without a name, the color swatch row);
 * `cur` is the focused one, -1 for none (a menu opened by the pointer). ↑↓ go round, Home End, a letter jumps to the next
 * item whose name starts with it (a ✓ or blank before the name doesn't count)
 */
export function menuKey(key: string, cur: number, stops: (string | null)[]): number | null {
  const n = stops.length;
  if (!n) return null;
  if (key === "ArrowDown") return cur < 0 ? 0 : (cur + 1) % n;
  if (key === "ArrowUp") return cur < 0 ? n - 1 : (cur - 1 + n) % n;
  if (key === "Home") return 0;
  if (key === "End") return n - 1;
  if (key.length !== 1 || !/\S/.test(key)) return null;
  const k = key.toLowerCase();
  for (let i = 1; i <= n; i++) {
    const j = (cur + i + n) % n;
    if (stops[j]?.replace(/^[^\p{L}\p{N}]+/u, "").toLowerCase().startsWith(k)) return j;
  }
  return null;
}

const GAP = 2;
const MARGIN = 8;

/** Where a popup of `size` goes: right under the field, left-aligned to it; above when it doesn't fit below and there's more room
 * above; always inside the window */
export function placePopup(field: { left: number; top: number; bottom: number }, size: { width: number; height: number }, view: { width: number; height: number }): { left: number; top: number } {
  const below = field.bottom + GAP;
  const fitsBelow = below + size.height <= view.height - MARGIN;
  const top = fitsBelow || view.height - below >= field.top ? below : Math.max(MARGIN, field.top - GAP - size.height);
  return { left: Math.max(MARGIN, Math.min(field.left, view.width - size.width - MARGIN)), top };
}

/** A dialog moved by its title bar (Modal): the offset `off` from where it opens, held so the whole dialog stays inside the window.
 * `base` is where it is at offset 0; one larger than the window keeps its top left corner in */
export function clampDrag(off: { x: number; y: number }, base: { left: number; top: number; width: number; height: number }, view: { width: number; height: number }): { x: number; y: number } {
  return {
    x: Math.max(0, Math.min(base.left + off.x, view.width - base.width)) - base.left,
    y: Math.max(0, Math.min(base.top + off.y, view.height - base.height)) - base.top,
  };
}

/** The window on top: the one in the highest layer (its z-index: a confirm over a dialog), the last opened among equals (DOM order) */
export function topOf<T>(windows: T[], layer: (w: T) => number): T | undefined {
  let top: T | undefined;
  for (const w of windows) if (top === undefined || layer(w) >= layer(top)) top = w;
  return top;
}
