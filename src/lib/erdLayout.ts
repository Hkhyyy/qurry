/** ERD 배치: 가리켜지는 테이블(부모)이 위, 가리키는 테이블(자식)이 아래인 층. 같은 층은 부모 위치 평균으로 줄 세운다 */
export interface Box {
  key: string;
  width: number;
  height: number;
}

const GAP_X = 48;
const GAP_Y = 64;
/** 한 층에 이보다 많으면 줄을 나눈다 (가로로 끝없이 길어지지 않게) */
const MAX_PER_ROW = 6;

export function layout(boxes: Box[], edges: { from: string; to: string }[]): Map<string, { x: number; y: number }> {
  const keys = new Set(boxes.map((b) => b.key));
  const parents = new Map<string, string[]>();
  for (const e of edges) if (e.from !== e.to && keys.has(e.from) && keys.has(e.to)) parents.set(e.from, [...(parents.get(e.from) ?? []), e.to]);

  // 층: 부모 층 + 1 (순환은 지나는 중인 것을 0 으로 끊는다)
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

  // 층마다 줄로 (너무 많으면 여러 줄)
  const byRank = new Map<number, Box[]>();
  for (const b of boxes) byRank.set(rank.get(b.key)!, [...(byRank.get(rank.get(b.key)!) ?? []), b]);
  const order = new Map<string, number>();
  const pos = new Map<string, { x: number; y: number }>();
  let y = 0;
  for (const r of [...byRank.keys()].sort((a, b) => a - b)) {
    // 부모의 순서 평균으로 줄 세운다 (선이 덜 꼬이게)
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
