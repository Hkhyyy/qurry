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

/** 화면의 한 줄 (키 브라우저: 보이는 줄만 그린다) */
export type KeyRowView<T> = { kind: "folder"; name: string; path: string; count: number; depth: number } | { kind: "key"; item: T; depth: number };

/** 펼친 폴더(open: path)만 안을 보인다 */
export function visibleRows<T>(nodes: KeyNode<T>[], open: Set<string>, depth = 0, out: KeyRowView<T>[] = []): KeyRowView<T>[] {
  for (const n of nodes) {
    if (n.kind === "key") out.push({ kind: "key", item: n.item, depth });
    else {
      out.push({ kind: "folder", name: n.name, path: n.path, count: n.count, depth });
      if (open.has(n.path)) visibleRows(n.children, open, depth + 1, out);
    }
  }
  return out;
}

/** 키가 들어 있는 폴더들 ("dev:sessions:{1}" → dev, dev:sessions) */
export function parentPaths(key: string): string[] {
  const parts = key.split(":");
  return parts.slice(1).map((_, i) => parts.slice(0, i + 1).join(":"));
}
