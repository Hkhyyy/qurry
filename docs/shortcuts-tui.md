# TUI keyboard shortcuts

English | [한국어](ko/shortcuts-tui.md)

Keys are written as macOS shows them: `⌃` Control, `⌥` Option, `⇧` Shift. On Linux read `⌃` as `Ctrl` and `⌥` as `Alt` (the TUI shows them that way there).

Press **F1** inside `qurry-tui`, or run `qurry-tui --help`, to see the same list. In the F1 window the **Commands** at the top can be changed: select one, press Enter and then the new key (`a` adds a second key, `Del` resets it). Every command of the `⌃k` palette is there, the ones without a default key too, followed by the Database Explorer's and the data grid's single-letter commands (`n` New Data Source, `a` Add Row …), which can be changed the same way: they work only in their pane and can be a letter. The mouse works too: click to select, double-click to open or edit, scroll with the wheel.
⌃Enter is only distinguishable in terminals that support the kitty keyboard protocol (iTerm2, kitty, WezTerm, Ghostty). Elsewhere use F5 or ⌥Enter.
In tmux, `⌃b` (Go to DDL) is the tmux prefix: press it twice, or see [TUI in your terminal](tui.md#tmux-and-other-multiplexers) for that and for ⌥ keys.

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

### Keys that stay fixed

The per-screen keys in the tables after the commands stay fixed: they move, open, edit, copy, filter or sort and run no command, so an explorer or grid key can't take them either.

- Moving: arrows, `j k h l`, `g G`, Page Up / Down, Home / End; `Enter` / `F2` open or edit, `Esc` / `Tab` go back.
- In the explorer `/` filters, `y` copies the name, `F4` opens a console; in the grid `y` copies, `s` sorts, `w` / `/` / `o` edit WHERE · ORDER BY, `r` reloads.
- `⌥1…9` switch workspaces (unless a command has that key), `⌃u` / `⌃r` undo and redo in the editor, the find bar's own keys.
- Keys inside a popup, the data source form, the Redis browser and the Database Admin tab act on that screen; the F1 window's own keys too.

### Several nodes in the explorer

`m` marks the node under the cursor or unmarks it (the GUI's `⌘`-click); moving keeps the marks, `Esc` clears them. Go to DDL (`⌃b`) and `F4` (a tab each), `y` (one `schema.name` per line, the GUI's Copy Reference), `Delete` (Drop…) and refresh then act on every marked node on screen; marks in collapsed or filtered-out folders show as "(N hidden)" in the explorer title. Drop… asks once, listing every statement (red on a Production data source, as for one object), and needs tables, views or sequences of one database. The statements come in the GUI's order (what depends on an object goes first; one statement per kind on PostgreSQL, in one transaction; one per object on MySQL, run one by one), and a failure names what was applied and what didn't run. After a failure the explorer reloads too and the marks are cleared. A command that doesn't fit every marked node says so in the status line, with how to unmark.

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
| `⌃F6` | Modify Table… (change columns, keys, indexes, foreign keys → ALTER) |
| `⇧F6` | Rename… (table, view, sequence, schema, database) |
| `⌥\` | Split Right (focused pane, up to 4) |
| `⌥-` | Split Down (focused pane, up to 4) |
| `⌥Tab` | Go to the Next Split |
| `⌥⇧Left` | Move Split Divider Left / Up |
| `⌥⇧Right` | Move Split Divider Right / Down |
| `⌥[` | Database Explorer Narrower |
| `⌥]` | Database Explorer Wider |
| `F5 / ⌃Enter` | Execute (asks what to run — see Query Execution Settings) |
| `⌥Enter` | Quick Fixes (an underlined unknown name's closest names; elsewhere Execute) |
| `⌥e` | Explain Plan |
| `⌃Space` | Code Completion |
| `⌥⇧F` | Reformat Code (the selection or the statement at the cursor) |
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

## Commands without a default key (give them one in the F1 window)

| Key | Action |
|---|---|
| — | Show Log |
| — | Import Data Sources… (DataGrip, IntelliJ, DBeaver, a Qurry export) |
| — | Data Source Properties… |
| — | New Data Source… |
| — | Refresh |
| — | Query History… (shared with the GUI) |
| — | Value Panel (the current cell's whole value, beside the grid) |
| — | Narrow Column (the current one) |
| — | Widen Column (the current one) |
| — | Fit Column to its content (the rows loaded so far; again: back to the width before) |
| — | Fit All Columns |
| — | Reset Column Widths (back to the computed widths) |
| — | Explain Analyze (runs the statement: real times and row counts) |
| — | Query Execution Settings (what ⌃Enter runs) |
| — | Clear Console (delete all text) |
| — | Edit Data (open table) |
| — | Revert Changes |
| — | Add Row |
| — | Delete Row |
| — | Set NULL |
| — | Export Data… (CSV, TSV, JSON, SQL INSERT, Markdown) |
| — | Copy Column as IN List  ('a', 'b', …) |
| — | Mark Row (add the row to the selection or take it out: y copies, d deletes the marked rows) |
| — | Find Similar Rows (pgvector, by the vector column) |
| — | AI Search (RAG)… — ask a table in plain words |
| — | Build RAG Index… (embed the rows) |
| — | New RAG Index from the Selected Table… |
| — | Set OpenAI API Key (RAG)… |
| — | Switch Workspace… |
| — | Delete Workspace… |
| — | Move Tab to Workspace… |
| — | Add Data Source to This Workspace… |
| — | Move Data Source to Another Workspace… |
| — | Remove Data Source from This Workspace |
| — | Production Log… (changes run on Production data sources, undo SQL) |
| — | Snippets… |
| — | Save Query as Snippet… |
| — | Show Diagram (ERD): foreign keys of a schema, as text + Mermaid |
| — | Compare Schemas… (DDL to make one schema look like another) |
| — | Compare Table Data… (sync SQL by primary key) |
| — | Run Migrations… (folder of V1__name.sql files) |
| — | Back Up Database… (pg_dump / mysqldump) |
| — | Restore Database… (from a .dump / .sql backup) |
| — | Database Admin… (sessions & locks, slow queries, sizes, indexes, vacuum, server, privileges) |
| — | Import Data from File… (CSV, TSV, JSON, Excel) |
| — | Generate Test Data… (fill the selected table with realistic rows) |
| — | New Table… (columns, keys, indexes, foreign keys — SQL shown before it runs) |
| — | New View… |
| — | New Schema… (PostgreSQL) |
| — | New Database… |
| — | New Sequence… (PostgreSQL) |
| — | Chart Result (bars: sum of a number column by the first text column) |
| — | Copy Related Rows To… (rows + foreign-key related rows into another data source, personal data masked) |
| — | Cancel Export/Import |
| — | Remove Data Source |
| — | Connect |
| — | Mark / Unmark Node (Go to DDL, open, copy, drop and refresh act on every marked node; Esc clears) |
| — | Drop… (the selected object, or the marked objects of one database: one confirmation) |
| — | Manage Shown Schemas… (databases · schemas in the explorer) |
| — | Unsplit All |
| — | Trust SSH Host Key (after checking its fingerprint) |
| — | Password Key… (keep it in the config folder or the system keychain) |
| — | Keyword Case… (UPPER or lower SQL keywords as you type and in Reformat Code; shared with the GUI) |
| — | Grid Alignment… (where cells sit per kind of value: NULL, numbers, text …; shared with the GUI) |
| — | Status Bar Items… (what the status line shows for a grid: position, shown columns, Sum, Size …; shared with the GUI) |
| — | Theme… (the GUI's built-in and custom themes, or Sync with OS by the terminal background) |
| — | Check for Updates |
| — | Update Channel… (stable releases only, or beta: also every merge to main) |
| — | Update Qurry & Restart (download the latest release) |

## Database Explorer keys (when the explorer has focus; change them in the F1 window)

| Key | Action |
|---|---|
| `?` | Keymap |
| `e` | Data Source Properties… |
| `n` | New Data Source… |
| `r` | Refresh |
| `X` | Disconnect |
| `w` | Move Data Source to Another Workspace… |
| `D` | Remove Data Source |
| `c` | Connect |
| `m` | Mark / Unmark Node (Go to DDL, open, copy, drop and refresh act on every marked node; Esc clears) |
| `Delete` | Drop… (the selected object, or the marked objects of one database: one confirmation) |
| `s` | Manage Shown Schemas… (databases · schemas in the explorer) |

## Data grid keys (when a grid has focus; change them in the F1 window)

| Key | Action |
|---|---|
| `v` | Value Panel (the current cell's whole value, beside the grid) |
| `<` | Narrow Column (the current one) |
| `>` | Widen Column (the current one) |
| `=` | Fit Column to its content (the rows loaded so far; again: back to the width before) |
| `⌃z` | Revert Changes |
| `a / Insert` | Add Row |
| `d / Delete` | Delete Row |
| `n` | Set NULL |
| `x` | Export Data… (CSV, TSV, JSON, SQL INSERT, Markdown) |
| `Y` | Copy Column as IN List  ('a', 'b', …) |
| `m` | Mark Row (add the row to the selection or take it out: y copies, d deletes the marked rows) |

## Database Explorer

| Key | Action |
|---|---|
| `↑ ↓  (j k)` | Move |
| `→ / ←  (l h)` | Expand / collapse (← on a child jumps to its parent) |
| `Enter / double-click` | Expand / collapse (connects) · open table data · show DDL of routines · Redis database: the source's Redis tab on it |
| `click (Redis database)` | Switch the source's open Redis tab to that database |
| `F4 / ⌃↓` | Open a query console for the data source / database (Redis: the source's Redis tab) |
| `/` | Filter loaded nodes (/ again edits it · empty or Esc clears) |
| `F5` | Refresh (the run key refreshes in the explorer) |
| `y  ⌘c` | Copy reference: schema.name, as the GUI's Copy Reference (OSC 52 clipboard; marked nodes: one per line) |
| `PgUp / PgDn · Home / End` | Page · first / last node |
| `Tab / Esc` | Go to the open tab (Esc clears the marks first) |

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
| `⇧↑↓←→ / drag` | Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or click / drag the bar on the bottom border (its heavier part is the columns shown; ◀ cols 3–8 of 14 ▶ at its right end) |
| `Enter / F2` | Edit cell (table data) |
| `s` | Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows) |
| `w or / · o` | Edit WHERE · ORDER BY (table data) |
| `r` | Reload (table data) |
| `y  ⌘c` | Copy cell value (a selected range or marked rows: as TSV, pastes into spreadsheets as cells) |
| `⌘v` | Paste into cells from the selected one (tabs / lines fill several; extra lines add rows; table data) |
| `Esc` | Back (query results: to the editor) |

## DDL / routine view

| Key | Action |
|---|---|
| `↑ ↓  (j k) · PgUp / PgDn` | Scroll |
| `v` | Table DDL: switch between SQL and Structure (columns, keys, indexes, foreign keys, triggers, options); kept for new DDL tabs |
| `r` | Reload what the tab shows (the DDL, or the Structure view) |
| `y  ⌘c` | Copy the text (a table's DDL: the SQL, in either view) |
| `Esc` | Back to the explorer |

## Database Admin tab (⌃k › Database Admin…)

| Key | Action |
|---|---|
| `r · a` | Refresh · auto-refresh on/off (sessions and locks refresh every 3 s) |
| `c · K` | Cancel the query · terminate the session of the selected row (asks first) |
| `Enter · g` | Open the row's query in a console · generate SQL from the row (DROP INDEX, VACUUM, REVOKE…) |
| `p` | Pick another report |
| `d` | Run the report on another database (MySQL: schema); the grid's title names it |

## Redis browser (Medis style)

| Key | Action |
|---|---|
| `→ / Enter · ←` | Folder: expand · collapse (← on a key: go to its folder) |
| `Enter / → / Tab` | Key: go to the value |
| `d` | Show another database (with key counts; one Redis tab per data source) |
| `/` | Filter keys (SCAN MATCH pattern, e.g. prod:user:*) |
| `m` | Scan more |
| `r` | Rescan |
| `a` | Add key (SET / HSET / RPUSH / SADD / ZADD …) |
| `D / Delete` | Delete key |
| `R` | Rename key |
| `t` | Set TTL (empty = persist) |
| `:` | Command line: Tab completes commands (with arguments) and key names, ↑↓ history, HELP command; read-only runs reads only, Production asks before writes, FLUSHDB / DEL always ask |
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
| `Go to DDL key` | DDL of the selected object (in Search Everywhere / Go to Database Object) |
| `type` | Filter (palettes) |
| `↑ ↓ / PgUp PgDn` | Move / scroll |
| `Enter` | Run / confirm |
| `y / n` | Answer confirmations |
| `Esc` | Close |

<!-- keymap:tui:end -->

---

[← User guide](README.md)
