import { SHORTCUTS } from "./shortcuts";

/** GUI shortcut list (source for Help › Keymap Reference and docs/shortcuts-gui.md). [macOS key, description] for fixed keys (other OS notation via toOther);
 *  [null, command id] for rebindable shortcuts (lib/shortcuts.ts): the description is the command table's (guiDesc, else title), the screen
 *  shows the current keys, the docs the defaults from the table. The last section (OTHER_ACTIONS) lists every command not listed here,
 *  mostly menu and Find Action commands without a default key (scripts/gen-keymap-docs.py builds the same list) */
export type KeyRow = [mac: string, desc: string] | [mac: null, id: string];
export interface KeySection {
  title: string;
  rows: KeyRow[];
}

export const KEYMAP: KeySection[] = [
  {
    title: "Search & Navigation",
    rows: [
      ["⇧⇧", "Search Everywhere (All / Database / Files / Symbols / Actions, ⇥ switches tabs). Data sources you are not connected to are searched from the list saved the last time they were loaded (marked cached); opening a result connects"],
      [null, "goto.object"],
      [null, "goto.symbol"],
      [null, "goto.file"],
      [null, "find.action"],
      [null, "fts"],
      [null, "recent"],
      ["⌃⇥ / ⌃⇧⇥", "Switcher (hold ⌃, release to open)"],
      [null, "keymap"],
    ],
  },
  {
    title: "In search results (Search Everywhere / Go to)",
    rows: [
      ["↩", "Select in Database Explorer"],
      [null, "tree.editData"],
      [null, "tree.ddl"],
      [null, "tree.quickDoc"],
      [null, "selectIn"],
    ],
  },
  {
    title: "Tool windows & appearance",
    rows: [
      [null, "ws.1"],
      [null, "ws.2"],
      [null, "ws.3"],
      [null, "ws.4"],
      [null, "ws.5"],
      [null, "ws.6"],
      [null, "ws.7"],
      [null, "ws.8"],
      [null, "ws.9"],
      [null, "explorer"],
      [null, "find.window"],
      ["⇧⎋", "Hide active tool window (in a grid: its Value Editor panel)"],
      [null, "maximize"],
      [null, "settings"],
      [null, "theme"],
      [null, "font.bigger"],
      [null, "font.smaller"],
    ],
  },
  {
    title: "Editor tabs",
    rows: [
      [null, "tab.close"],
      ["⌥ + click ×", "Close other tabs"],
      [null, "tab.reopen"],
      [null, "tab.next"],
      [null, "tab.prev"],
      [null, "console.rename"],
      [null, "split.other"],
      ["right-click tab", "Pin, Split Right/Down (up to 4 panes), Move to Next Group, Close others…"],
      ["drag tab", "Reorder, or drop on a pane's right/bottom edge to split it (up to 4)"],
    ],
  },
  {
    title: "Database Explorer",
    rows: [
      [null, "tree.editData"],
      [null, "console.new"],
      [null, "tree.jumpConsole"],
      [null, "tree.ddl"],
      [null, "tree.sqlGen"],
      [null, "tree.ddlCopy"],
      [null, "tree.ddlConsole"],
      [null, "tree.copyName"],
      [null, "tree.copyRef"],
      [null, "tree.quickDoc"],
      [null, "tree.modify"],
      [null, "tree.rename"],
      [null, "tree.properties"],
      [null, "tree.newDs"],
      [null, "tree.duplicate"],
      [null, "tree.refresh"],
      [null, "refresh"],
      [null, "tree.disconnect"],
      [null, "tree.remove"],
      ["type letters", "Speed search in the tree"],
      ["⌘-click", "Add a node to the selection or remove it (Go to DDL, Edit Data, copy, Drop…, Refresh run for every selected node)"],
      ["⇧-click", "Select the visible nodes from the last clicked one; a plain click or an arrow key selects one node again"],
    ],
  },
  {
    title: "Query console",
    rows: [
      [null, "editor.run"],
      [null, "editor.cancel"],
      [null, "editor.history"],
      ["⌃Space", "Code completion"],
      [null, "editor.reformat"],
      ["⌘/", "Line comment"],
      [null, "editor.blockComment"],
      [null, "editor.duplicate"],
      [null, "editor.deleteLine"],
      [null, "editor.moveUp"],
      [null, "editor.moveDown"],
      [null, "editor.extend"],
      [null, "editor.shrink"],
      [null, "editor.nextOcc"],
      [null, "editor.allOcc"],
      [null, "editor.toggleCase"],
      [null, "editor.gotoLine"],
      ["⌘f", "Find in editor (⌥⌘r regex · ⌥⌘c match case · ⌥⌘w words)"],
      [null, "editor.replace"],
    ],
  },
  {
    title: "Data grid (results & table editor)",
    rows: [
      ["↩ / F2", "Edit cell"],
      [null, "grid.valueEditor"],
      ["⌘↩ · ⎋", "In the Value Editor panel: apply the value · back to the grid (it follows the current cell)"],
      [null, "grid.setNull"],
      [null, "grid.setDefault"],
      [null, "grid.addRow"],
      [null, "grid.cloneRow"],
      [null, "grid.deleteRows"],
      [null, "grid.revert"],
      [null, "grid.submit"],
      ["⌘c / ⌘v", "Copy (with chosen extractor; CSV / TSV copy values only — right-click › Copy with Headers adds the column names) / paste"],
      ["⌘a", "Select all"],
      [null, "grid.find"],
      [null, "grid.reload"],
      [null, "grid.cancel"],
      [null, "grid.ddl"],
      ["click header · ⌥ click", "Sort · add to multi-column sort"],
    ],
  },
  {
    title: "Query history & Redis browser",
    rows: [
      ["⌦ / ⌘⌫", "Delete the selected history entry"],
      ["⌘r", "Redis: scan keys again"],
      ["⌘s", "Redis: save the value being edited"],
      ["⌃l", "Redis terminal: clear"],
    ],
  },
];

/** Title of the last section: the commands no section above lists */
export const OTHER_ACTIONS = "Other actions (menus, toolbars, Find Action — give them a key in Settings › Keymap)";
const listed = new Set(KEYMAP.flatMap((s) => s.rows.flatMap((r) => (r[0] === null ? [r[1]] : []))));
KEYMAP.push({ title: OTHER_ACTIONS, rows: SHORTCUTS.filter((s) => !listed.has(s.id)).map((s): KeyRow => [null, s.id]) });

const descs = new Map(SHORTCUTS.map((s) => [s.id, s.desc]));
/** The row's description */
export const rowDesc = (r: KeyRow) => (r[0] === null ? (descs.get(r[1]) ?? r[1]) : r[1]);

/** macOS symbols → Windows/Linux notation (the app takes ⌘ as Ctrl, ⌥ as Alt) */
export function toOther(k: string): string {
  return k
    .replace(/⌃/g, "Ctrl+")
    .replace(/⌘/g, "Ctrl+")
    .replace(/⌥/g, "Alt+")
    .replace(/⇧/g, "Shift+")
    .replace(/↩/g, "Enter")
    .replace(/⌫/g, "Backspace")
    .replace(/⌦/g, "Delete")
    .replace(/⎋/g, "Esc")
    .replace(/⇥/g, "Tab")
    .replace(/Ctrl\+Ctrl\+/g, "Ctrl+")
    .replace(/\+\s/g, " ");
}
