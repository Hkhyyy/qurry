/** GUI shortcut list (source for Help › Keymap Reference and docs/shortcuts-gui.md). [macOS key, description] for fixed keys (other OS notation via toOther);
 *  [null, description, command id] for rebindable shortcuts (lib/shortcuts.ts): the screen shows the current keys, the docs the defaults from the command table */
export type KeyRow = [mac: string, desc: string] | [mac: null, desc: string, id: string];
export interface KeySection {
  title: string;
  rows: KeyRow[];
}

export const KEYMAP: KeySection[] = [
  {
    title: "Search & Navigation",
    rows: [
      ["⇧⇧", "Search Everywhere (All / Database / Files / Symbols / Actions, ⇥ switches tabs). Data sources you are not connected to are searched from the list saved the last time they were loaded (marked cached); opening a result connects"],
      [null, "Go to Database Object", "goto.object"],
      [null, "Go to Symbol (columns)", "goto.symbol"],
      [null, "Go to File (open and recent tabs)", "goto.file"],
      [null, "Find Action", "find.action"],
      [null, "Full-text Search in database", "fts"],
      [null, "Recent Files", "recent"],
      ["⌃⇥ / ⌃⇧⇥", "Switcher (hold ⌃, release to open)"],
      [null, "Keymap Reference (this window)", "keymap"],
    ],
  },
  {
    title: "In search results (Search Everywhere / Go to)",
    rows: [
      ["↩", "Select in Database Explorer"],
      ["⌘↓", "Edit data (open table)"],
      ["⌘b", "Go to DDL"],
      ["F1", "Quick documentation"],
      ["⌥F1", "Select in Database Explorer"],
    ],
  },
  {
    title: "Tool windows & appearance",
    rows: [
      ["⌘1…⌘9", "Switch to workspace 1…9"],
      [null, "Database Explorer (focus / hide)", "explorer"],
      [null, "Find tool window (Full-text Search results)", "find.window"],
      ["⇧⎋", "Hide active tool window"],
      [null, "Maximize editor (also: double-click a tab)", "maximize"],
      [null, "Settings (theme)", "settings"],
      [null, "Quick switch theme (Dark / Light / High contrast / Sync with OS)", "theme"],
      [null, "Editor font size bigger", "font.bigger"],
      [null, "Editor font size smaller", "font.smaller"],
    ],
  },
  {
    title: "Editor tabs",
    rows: [
      [null, "Close tab (middle-click also closes)", "tab.close"],
      ["⌥ + click ×", "Close other tabs"],
      [null, "Reopen closed tab", "tab.reopen"],
      [null, "Next tab", "tab.next"],
      [null, "Previous tab", "tab.prev"],
      ["⇧F6", "Rename console"],
      [null, "Go to the next split (⇧ previous)", "split.other"],
      ["right-click tab", "Pin, Split Right/Down (up to 4 panes), Move to Next Group, Close others…"],
      ["drag tab", "Reorder, or drop on a pane's right/bottom edge to split it (up to 4)"],
    ],
  },
  {
    title: "Database Explorer",
    rows: [
      [null, "Edit data (table) / open console (data source; Redis: key browser)", "tree.editData"],
      [null, "New query console", "console.new"],
      [null, "Jump to query console", "tree.jumpConsole"],
      [null, "Go to DDL", "tree.ddl"],
      [null, "SQL Generator", "tree.sqlGen"],
      [null, "Copy DDL to clipboard", "tree.ddlCopy"],
      [null, "Open DDL in console", "tree.ddlConsole"],
      [null, "Copy reference", "tree.copyRef"],
      [null, "Quick documentation", "tree.quickDoc"],
      [null, "Modify table (columns, keys, indexes, foreign keys)", "tree.modify"],
      [null, "Rename table / view / sequence / schema / database", "tree.rename"],
      [null, "Data source properties", "tree.properties"],
      [null, "New data source", "tree.newDs"],
      [null, "Duplicate data source", "tree.duplicate"],
      [null, "Refresh", "tree.refresh"],
      [null, "Refresh (anywhere)", "refresh"],
      [null, "Disconnect", "tree.disconnect"],
      [null, "Remove data source / drop object (asks first)", "tree.remove"],
      ["type letters", "Speed search in the tree"],
    ],
  },
  {
    title: "Query console",
    rows: [
      [null, "Execute: asks what to run when there are several statements (⚙ Query Execution Settings), or the selection", "editor.run"],
      [null, "Cancel running statement", "editor.cancel"],
      [null, "Query history", "editor.history"],
      ["⌃Space", "Code completion"],
      [null, "Reformat code", "editor.reformat"],
      ["⌘/", "Line comment"],
      [null, "Block comment", "editor.blockComment"],
      [null, "Duplicate line", "editor.duplicate"],
      [null, "Delete line", "editor.deleteLine"],
      [null, "Move line up", "editor.moveUp"],
      [null, "Move line down", "editor.moveDown"],
      [null, "Extend selection", "editor.extend"],
      [null, "Shrink selection", "editor.shrink"],
      [null, "Select next occurrence", "editor.nextOcc"],
      [null, "Select all occurrences", "editor.allOcc"],
      [null, "Toggle case", "editor.toggleCase"],
      [null, "Go to line", "editor.gotoLine"],
      ["⌘f", "Find in editor (⌥⌘r regex · ⌥⌘c match case · ⌥⌘w words)"],
      [null, "Replace in editor ($1 groups with regex)", "editor.replace"],
    ],
  },
  {
    title: "Data grid (results & table editor)",
    rows: [
      ["↩ / F2", "Edit cell"],
      ["⇧↩", "Open value editor"],
      ["⌥⌘n", "Set NULL"],
      ["⌥⌘d", "Set DEFAULT"],
      ["⌘n", "Add row"],
      ["⌘d", "Clone row"],
      ["⌘⌫", "Delete rows"],
      ["⌥⌘z", "Revert selected changes"],
      ["⌘↩", "Submit changes"],
      ["⌘c / ⌘v", "Copy (with chosen extractor) / paste"],
      ["⌘a", "Select all"],
      ["⌘f", "Find in grid"],
      ["⌘r", "Reload"],
      ["⌘F2", "Cancel"],
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
