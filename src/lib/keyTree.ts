// Redis key folders: qurry_shared::keytree through WASM, as the TUI groups them. Folder names sort as JavaScript sorts strings
// (UTF-16 order, D2 of the 0.3.0 architecture batch: the keyTree row of crates/shared/tests/DIVERGENCES.md)
import { sharedOr, type SharedKeyNode } from "./shared";

/** Folders from Redis keys split on ":" (like Medis). path = "dev:sessions", count = keys under it */
export type KeyNode<T> = { kind: "folder"; name: string; path: string; count: number; children: KeyNode<T>[] } | { kind: "key"; item: T };

/** Folders first (by name), then keys (in received order). Keys keep their full names. Until the module is loaded: the keys without folders */
export function keyTree<T>(items: T[], name: (t: T) => string): KeyNode<T>[] {
  const nodes = (v: SharedKeyNode[]): KeyNode<T>[] => v.map((n) => (n.kind === "key" ? { kind: "key", item: items[n.item] } : { ...n, children: nodes(n.children) }));
  const tree = sharedOr<SharedKeyNode[] | null>(null, (s) => s.keyTree(items.map(name), { utf16Order: true }));
  return tree ? nodes(tree) : items.map((item) => ({ kind: "key", item }));
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
