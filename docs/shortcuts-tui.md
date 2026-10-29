# TUI keyboard shortcuts

English | [한국어](ko/shortcuts-tui.md)

Keys are written as macOS shows them: `⌃` Control, `⌥` Option, `⇧` Shift. On Linux read `⌃` as `Ctrl` and `⌥` as `Alt` (the TUI shows them that way there).

Press **F1** inside `qurry-tui`, or run `qurry-tui --help`, to see the same list. In the F1 window the **Commands** at the top can be changed: select one, press Enter and then the new key (`a` adds a second key, `Del` resets it). The mouse works too: click to select, double-click to open or edit, scroll with the wheel.
⌃Enter is only distinguishable in terminals that support the kitty keyboard protocol (iTerm2, kitty, WezTerm, Ghostty). Elsewhere use F5 or ⌥Enter.

**New to it? `⌃p` alone is enough to start.** It searches tables, columns, open tabs and actions (data sources you are not connected to are searched from the list saved the last time they were loaded). The status bar always shows the keys for what you are looking at (explorer, console, table, results, Redis), followed by the most useful global keys. The F1 window starts with the same short list. Keys are shown lowercase when Shift is not needed (`⌃k`); an uppercase letter means Shift (`D`, `⌥⇧X`).

| Key | Action |
|---|---|
| `⌃p` | Search everything: tables, columns, tabs, actions (not sure? start here) |
| `⌃k` | All actions, each with its shortcut |
| `⌃l` | New query console for the selected data source |
| `F5 / ⌃Enter / ⌥Enter` | Run (asks what to run when there are several statements) |
| `⌃e` | Recent tabs |
| `⌃w` | Close tab |
| `Enter` (explorer) | Connect · open a table |
| `s` (explorer) | Choose the databases and schemas shown |
| `Esc` | Back (to the explorer, or from results to the editor) |
| `⌃q` | Quit |

<!-- keymap:tui:start -->

## Commands (default keys — change them in the F1 window)

| Key | Action |
|---|---|
| `⌃q` | Quit |
| `F1` | Keymap |
| `⌃k` | Actions Palette |
| `⌃p` | Search Everywhere (objects · columns · tabs · actions) |
| `⌃o` | Go to Database Object |
| `⌃b` | Go to DDL |
| `⌃e` | Recent Tabs / Switcher |
| `⌃l` | New Query Console |
| `⌃w` | Close Tab |
| `⌥Right / ⌃PgDn` | Next Tab |
| `⌥Left / ⌃PgUp` | Previous Tab |
| `⌥0` | Database Explorer |
| `⌥n` | New Workspace… |
| `⌥⇧N` | Rename Workspace… |
| `F6` | Cycle Focus (explorer → editor → results) |
| `⌃F6` | Modify Table… (selected table) |
| `⇧F6` | Rename… (selected table, view, sequence, schema or database) |
| `⌥\` | Split Right (focused pane, up to 4) |
| `⌥-` | Split Down (focused pane, up to 4) |
| `⌥Tab` | Go to the Next Split |
| `⌥⇧Left` | Move Split Divider Left / Up |
| `⌥⇧Right` | Move Split Divider Right / Down |
| `⌥[` | Database Explorer Narrower |
| `⌥]` | Database Explorer Wider |
| `F5 / ⌃Enter / ⌥Enter` | Execute (asks what to run — see Query Execution Settings) |
| `⌥e` | Explain Plan |
| `⌃Space` | Code Completion |
| `⌃c` | Cancel Running Statement |
| `⌥t` | Toggle Transaction Mode (Auto/Manual) |
| `⌥c` | Commit |
| `⌥r` | Roll Back |
| `⌥o` | Toggle Output / Result |
| `⌃⌥Up` | Go to the Split Above |
| `⌃⌥Down` | Go to the Split Below |
| `⌃⌥Left` | Go to the Split on the Left |
| `⌃⌥Right` | Go to the Split on the Right |
| `⌥⇧Up` | Shrink Editor (console) |
| `⌥⇧Down` | Grow Editor (console) |
| `⌃s` | Submit Changes |
| `⌃f` | Find / Replace in Editor (regex) |
| `⌃F2` | Disconnect |

## Database Explorer

| Key | Action |
|---|---|
| `↑ ↓  (j k)` | Move |
| `→ / ←  (l h)` | Expand / collapse (← on a child jumps to its parent) |
| `Enter / double-click` | Expand / collapse (connects) · open table data · open a Redis key · show DDL of routines |
| `F4 / ⌃↓` | Open a query console for the data source / database (Redis: the key browser) |
| `/` | Filter loaded nodes (/ again edits it · empty or Esc clears) |
| `n / e / D` | New / edit / remove data source |
| `w` | Move the data source to another workspace (⌥1…9 switches · ⌥n new · ⌥⇧N rename) |
| `c / X` | Connect / disconnect |
| `r / F5` | Refresh |
| `s` | Manage shown databases and schemas (Space check · Enter apply · shared with the GUI) |
| `y  ⌘c` | Copy name (OSC 52 clipboard) |
| `PgUp / PgDn · Home / End` | Page · first / last node |
| `?` | Keymap (same as F1) |
| `Tab / Esc` | Go to the open tab |

## Query console

| Key | Action |
|---|---|
| `completion list` | Opens as you type (or ⌃Space) · ↑↓ choose · Enter/Tab insert · Esc close |
| `⇧Tab` | Go to the result grid (Esc / Tab returns) |
| `Esc` | Go to the explorer |
| `⌃u / ⌃r` | Undo / redo (editor) |
| `⌃f` | Find / replace bar: ⌥x regex · ⌥c match case · ⌥w words · Enter next (↑ previous) · Tab to Replace: Enter one, ⌥a all ($1 groups) · Esc close |
| `⌘a  ⌫` | Select all, then delete (or ⌃k › Clear Console) |
| `click / drag  ⌘c` | Move the cursor / select text (runs only the selection) · copy the selection |
| `mouse drag` | Drag the editor/result border to resize |

## Tabs & split (mouse)

| Key | Action |
|---|---|
| `drag a tab` | Onto a pane's right / bottom edge = split it (up to 4 panes) · onto another pane or tab bar = move · within a bar = reorder |
| `drag the border` | Resize the Database Explorer (its right border) or the panes (the border between them; in a top/bottom split also the empty part of the lower tab bar) |
| `click a pane` | Focus it |
| `⇧wheel / trackpad` | Scroll the grid and the explorer sideways (⇧← / ⇧→ in the explorer) |

## Data grid (results & table data)

| Key | Action |
|---|---|
| `arrows  (h j k l)` | Move |
| `PgUp / PgDn · g / G` | Page · first / last row (more rows load automatically) |
| `Home / End` | First / last column |
| `⇧↑↓←→ / drag` | Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or drag the bar on the bottom border |
| `Enter / F2` | Edit cell (table data) |
| `n` | Set NULL (table data) |
| `a / Insert` | Add row (table data) |
| `d / Delete` | Mark row deleted, again = undo (table data) |
| `⌃z` | Revert pending changes (table data) |
| `s` | Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows) |
| `w or / · o` | Edit WHERE · ORDER BY (table data) |
| `r` | Reload (table data) |
| `x` | Export data (CSV, TSV, JSON, SQL INSERT, Markdown → file or clipboard) |
| `y  ⌘c` | Copy cell value (a selected range: as TSV, pastes into spreadsheets as cells) |
| `Y` | Copy as an IN list: ('a', 'b', …) — the selected range, or the whole column |
| `⌘v` | Paste into cells from the selected one (tabs / lines fill several; extra lines add rows; table data) |
| `Esc` | Back (query results: to the editor) |

## DDL / routine view

| Key | Action |
|---|---|
| `↑ ↓  (j k) · PgUp / PgDn` | Scroll |
| `y  ⌘c` | Copy the text |
| `Esc` | Back to the explorer |

## Database Admin tab (⌃k › Database Admin…)

| Key | Action |
|---|---|
| `r · a` | Refresh · auto-refresh on/off (sessions and locks refresh every 3 s) |
| `c · K` | Cancel the query · terminate the session of the selected row (asks first) |
| `Enter · g` | Open the row's query in a console · generate SQL from the row (DROP INDEX, VACUUM, REVOKE…) |
| `p` | Pick another report |

## Redis browser (Medis style)

| Key | Action |
|---|---|
| `→ / Enter · ←` | Folder: expand · collapse (← on a key: go to its folder) |
| `Enter / → / Tab` | Key: go to the value |
| `/` | Key pattern (SCAN MATCH) |
| `m` | Scan more |
| `r` | Rescan |
| `a` | Add key (SET / HSET / RPUSH / SADD / ZADD …) |
| `D / Delete` | Delete key |
| `R` | Rename key |
| `t` | Set TTL (empty = persist) |
| `:` | Run any command |
| `e / Enter (value)` | Edit selected element / string |
| `a (value)` | Add element |
| `x / Delete (value)` | Delete element |
| `Esc / Tab / ← (value)` | Back to keys |

## Data source form

| Key | Action |
|---|---|
| `↑ ↓ / Tab` | Move between fields |
| `type / Backspace` | Edit text fields |
| `Space / ← →` | Toggle on/off · change choice (Type, Color, SSL mode, Redis mode…) |
| `⌃t` | Test connection |
| `⌃y` | Trust the SSH server's host key (after ⌃t showed its fingerprint) |
| `Enter / ⌃s` | Save |
| `Esc` | Cancel |

## Popups

| Key | Action |
|---|---|
| `⌃b` | Go to DDL (in Search Everywhere / Go to Database Object) |
| `type` | Filter (palettes) |
| `↑ ↓ / PgUp PgDn` | Move / scroll |
| `Enter` | Run / confirm |
| `y / n` | Answer confirmations |
| `Esc` | Close |

<!-- keymap:tui:end -->

---

[← User guide](README.md)
