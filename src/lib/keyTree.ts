/** Folders from Redis keys split on ":" (like Medis). path = "dev:sessions", count = keys under it */
export type KeyNode<T> = { kind: "folder"; name: string; path: string; count: number; children: KeyNode<T>[] } | { kind: "key"; item: T };

/** Folders first (by name), then keys (in received order). Keys keep their full names */
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

/** One row on screen (key browser: only visible rows are drawn) */
export type KeyRowView<T> = { kind: "folder"; name: string; path: string; count: number; depth: number } | { kind: "key"; item: T; depth: number };

/** Only expanded folders (open: path) show their contents */
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

/** Folders containing the key ("dev:sessions:{1}" → dev, dev:sessions) */
export function parentPaths(key: string): string[] {
  const parts = key.split(":");
  return parts.slice(1).map((_, i) => parts.slice(0, i + 1).join(":"));
}
