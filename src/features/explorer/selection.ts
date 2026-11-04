import type { MenuEntry } from "../../ui";

/** Database Explorer selection (DataGrip): click = one node, ⌘-click = add / remove, ⇧-click = the visible nodes from the anchor */
export interface TreeSel {
  ids: string[];
  /** Where a ⇧-click range starts (the last plain or ⌘-click) */
  anchor: string | null;
  /** The last clicked node: arrow keys move from it */
  lead: string | null;
}
export const NO_SEL: TreeSel = { ids: [], anchor: null, lead: null };
export type ClickHow = "one" | "toggle" | "range";

/** The selection after a click on `id`. visible = the selectable nodes on screen, top to bottom (folders and columns aren't) */
export function clickSelect(s: TreeSel, id: string, how: ClickHow, visible: string[]): TreeSel {
  if (how === "toggle" && visible.includes(id)) {
    const kept = s.ids.filter((x) => visible.includes(x));
    return { ids: kept.includes(id) ? kept.filter((x) => x !== id) : [...kept, id], anchor: id, lead: id };
  }
  if (how === "range") {
    const a = visible.indexOf(s.anchor ?? id);
    const b = visible.indexOf(id);
    if (a >= 0 && b >= 0) return { ids: visible.slice(Math.min(a, b), Math.max(a, b) + 1), anchor: s.anchor, lead: id };
  }
  return { ids: [id], anchor: id, lead: id };
}

/** Tooltip of a command that works on one node while several are selected: also how to get back to one */
export const ONE_NODE = "Works on one node: click a node without ⌘ or ⇧ to select only it";
/** Tooltip of a several-node command that doesn't fit every selected node */
export const NOT_ALL = "Not available for every selected node (Drop: objects of one database)";

/** The context menu for several selected nodes: commands in `multi` run for all of them (undefined = not for these nodes), every other
 *  command is greyed out with a tooltip. The menu keeps its shape, so it doesn't jump between one and several nodes */
export function forSelection(items: MenuEntry[], multi: Record<string, (() => void) | undefined>): MenuEntry[] {
  return items.map((it) => {
    if ("submenu" in it) return { ...it, submenu: forSelection(it.submenu, multi) };
    if (!("action" in it)) return it;
    if (it.id && it.id in multi) {
      const run = multi[it.id];
      return run ? { ...it, action: run } : { ...it, disabled: true, title: NOT_ALL };
    }
    return { ...it, disabled: true, title: ONE_NODE };
  });
}
