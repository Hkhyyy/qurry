/** ERD layout: layers with referenced tables (parents) above and referencing tables (children) below. Each layer is ordered by the average position of parents */
export interface Box {
  key: string;
  width: number;
  height: number;
}

const GAP_X = 48;
const GAP_Y = 64;
/** Wrap a layer into rows above this many (so it does not grow endlessly wide) */
const MAX_PER_ROW = 6;

export function layout(boxes: Box[], edges: { from: string; to: string }[]): Map<string, { x: number; y: number }> {
  const keys = new Set(boxes.map((b) => b.key));
  const parents = new Map<string, string[]>();
  for (const e of edges) if (e.from !== e.to && keys.has(e.from) && keys.has(e.to)) parents.set(e.from, [...(parents.get(e.from) ?? []), e.to]);

  // Layer: parent layer + 1 (cycles are broken by treating the in-progress node as 0)
  const rank = new Map<string, number>();
  const visiting = new Set<string>();
  const rankOf = (k: string): number => {
    if (rank.has(k)) return rank.get(k)!;
    if (visiting.has(k)) return 0;
    visiting.add(k);
    const r = Math.max(-1, ...(parents.get(k) ?? []).map(rankOf)) + 1;
    visiting.delete(k);
    rank.set(k, r);
    return r;
  };
  boxes.forEach((b) => rankOf(b.key));

  // Rows per layer (several rows if too many)
  const byRank = new Map<number, Box[]>();
  for (const b of boxes) byRank.set(rank.get(b.key)!, [...(byRank.get(rank.get(b.key)!) ?? []), b]);
  const order = new Map<string, number>();
  const pos = new Map<string, { x: number; y: number }>();
  let y = 0;
  for (const r of [...byRank.keys()].sort((a, b) => a - b)) {
    // Order by the average order of parents (fewer crossing lines)
    const row = byRank.get(r)!.map((b, i) => {
      const ps = (parents.get(b.key) ?? []).filter((p) => order.has(p)).map((p) => order.get(p)!);
      return { b, score: ps.length ? ps.reduce((s, x) => s + x, 0) / ps.length : 1e6 + i };
    });
    row.sort((a, b) => a.score - b.score);
    for (let i = 0; i < row.length; i += MAX_PER_ROW) {
      const line = row.slice(i, i + MAX_PER_ROW);
      let x = 0;
      for (const { b } of line) {
        order.set(b.key, order.size);
        pos.set(b.key, { x, y });
        x += b.width + GAP_X;
      }
      y += Math.max(...line.map((l) => l.b.height)) + GAP_Y;
    }
  }
  return pos;
}
