/** Redis 키를 ":" 로 끊은 폴더 (Medis 처럼). path = "dev:sessions", count = 그 아래 키 개수 */
export type KeyNode<T> = { kind: "folder"; name: string; path: string; count: number; children: KeyNode<T>[] } | { kind: "key"; item: T };

/** 폴더 먼저 (이름순), 그다음 키 (받은 순서). 키는 전체 이름 그대로 둔다 */
export function keyTree<T>(items: T[], name: (t: T) => string): KeyNode<T>[] {
  const level = (items: T[], depth: number, prefix: string): KeyNode<T>[] => {
    const folders = new Map<string, T[]>();
    const keys: KeyNode<T>[] = [];
    for (const it of items) {
      const parts = name(it).split(":");
      if (parts.length > depth + 1) {
        const seg = parts[depth];
        if (!folders.has(seg)) folders.set(seg, []);
        folders.get(seg)!.push(it);
      } else keys.push({ kind: "key", item: it });
    }
    const out: KeyNode<T>[] = [...folders.keys()].sort().map((seg) => {
      const path = prefix ? `${prefix}:${seg}` : seg;
      const kids = folders.get(seg)!;
      return { kind: "folder", name: seg, path, count: kids.length, children: level(kids, depth + 1, path) };
    });
    return out.concat(keys);
  };
  return level(items, 0, "");
}
