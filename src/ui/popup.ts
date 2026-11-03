/** Logic of the list popup under a field (PopupList · ComboBox), kept pure for the tests */

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
