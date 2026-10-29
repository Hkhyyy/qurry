/** Editor split tree (same rules as the TUI's panes.rs). Panes (groups) are leaves, splits are branches. At most MAX_PANES panes */
export type Pane = { t: "leaf"; g: number } | { t: "split"; dir: "right" | "down"; a: Pane; b: Pane };

export const MAX_PANES = 4;
export const leaf = (g: number): Pane => ({ t: "leaf", g });

/** Pane ids (from left/top) */
export const leaves = (p: Pane): number[] => (p.t === "leaf" ? [p.g] : [...leaves(p.a), ...leaves(p.b)]);

/** Split pane g in direction dir. The new pane id and the new tree (null if there are already MAX_PANES) */
export function splitPane(p: Pane, g: number, dir: "right" | "down"): { pane: Pane; g: number } | null {
  const ls = leaves(p);
  if (ls.length >= MAX_PANES || !ls.includes(g)) return null;
  const n = [0, 1, 2, 3].find((i) => !ls.includes(i))!;
  const walk = (x: Pane): Pane => (x.t === "leaf" ? (x.g === g ? { t: "split", dir, a: leaf(g), b: leaf(n) } : x) : { ...x, a: walk(x.a), b: walk(x.b) });
  return { pane: walk(p), g: n };
}

/** Remove panes with no tabs and let the neighbor fill the space (one pane if all are empty) */
export function prunePanes(p: Pane, used: Set<number>): Pane {
  const walk = (x: Pane): Pane | null => {
    if (x.t === "leaf") return used.has(x.g) ? x : null;
    const a = walk(x.a);
    const b = walk(x.b);
    return a && b ? { ...x, a, b } : a ?? b;
  };
  return walk(p) ?? leaf(leaves(p)[0]);
}
