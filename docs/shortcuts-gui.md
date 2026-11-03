# GUI keyboard shortcuts

English | [한국어](ko/shortcuts-gui.md)

Inside the app the same list is searchable via **Help › Keymap Reference**, the ⌨ toolbar button, or `⇧⌘/`. **Find Action** (`⇧⌘A`) lists every command together with its shortcut.
On Windows/Linux read `⌘` as `Ctrl` and `⌥` as `Alt`.

Command shortcuts can be changed in **Settings › Keymap** (or **Edit Keymap…** in the Keymap Reference): click *Change* and press the new keys. Every action you can reach from a menu, a context menu, a toolbar or Find Action is listed there — the ones without a default key too (the last table below), so any of them can get one. A key works where its section says: anywhere (Global), in the Database Explorer, in the query console or in a data grid. In the Database Explorer and the data grid a key can also be Delete, Insert or Backspace. The tables below show the defaults.

### Keys that stay fixed

- `⇧⇧` (Search Everywhere) is a double tap, and the Switcher (`⌃⇥`) opens on release of `⌃`: neither is a key combination.
- `Esc` always closes (a popup, a dialog, the find bar), so `⇧⎋` (hide the tool window) stays with it.
- Moving and opening: arrows, Home / End, Page Up / Down, Tab and `↩` in the tree, the grid, lists and popups. As in DataGrip, they are not commands.
- Typing: letters start speed search in the tree and edit the cell in the grid (`↩` / `F2` too), so a letter can't be a key there.
- The editor's and the system's own keys: `⌘c` / `⌘v` / `⌘a` in the grid, `⌘/`, `⌘f` (and the find bar's toggles) and `⌃Space` in the editor.
- Keys inside a dialog act on that dialog: query history, the Redis browser and terminal, the data source form. Mouse gestures (`⌥` click, drag) stay too.
- Menu entries made from your data are choices, not commands: a data source, workspace or console by name, a column's filter values, a data extractor, an auto-refresh interval, the Query Execution Settings options.

<!-- keymap:gui:start -->

## Search & Navigation

| macOS | Action |
|---|---|
| `⇧⇧` | Search Everywhere (All / Database / Files / Symbols / Actions, ⇥ switches tabs). Data sources you are not connected to are searched from the list saved the last time they were loaded (marked cached); opening a result connects |
| `⌘o` | Go to Database Object |
| `⌥⌘o` | Go to Symbol (columns) |
| `⇧⌘O` | Go to File (open and recent tabs) |
| `⇧⌘A` | Find Action |
| `⌥⇧⌘F` | Full-text Search in database |
| `⌘e` | Recent Files |
| `⌃⇥ / ⌃⇧⇥` | Switcher (hold ⌃, release to open) |
| `⇧⌘/` | Keymap Reference (this window) |

## In search results (Search Everywhere / Go to)

| macOS | Action |
|---|---|
| `↩` | Select in Database Explorer |
| `⌘↓ / F4` | Edit data (table) / open console (data source; Redis: key browser) |
| `⌘b` | Go to DDL |
| `F1` | Quick documentation |
| `⌥F1` | Select in Database Explorer (the open table; in search results the selected object) |

## Tool windows & appearance

| macOS | Action |
|---|---|
| `⌘1` | Workspace 1 |
| `⌘2` | Workspace 2 |
| `⌘3` | Workspace 3 |
| `⌘4` | Workspace 4 |
| `⌘5` | Workspace 5 |
| `⌘6` | Workspace 6 |
| `⌘7` | Workspace 7 |
| `⌘8` | Workspace 8 |
| `⌘9` | Workspace 9 |
| `⌘0` | Database Explorer (focus / hide) |
| `⇧⌘F` | Find tool window (Full-text Search results) |
| `⇧⎋` | Hide active tool window |
| `⇧⌘F12` | Maximize editor (also: double-click a tab) |
| `⌘,` | Settings (theme) |
| `` ⌃` `` | Quick switch theme (Dark / Light / High contrast / Sync with OS) |
| `⌃⇧.` | Editor font size bigger |
| `⌃⇧,` | Editor font size smaller |

## Editor tabs

| macOS | Action |
|---|---|
| `⌘w` | Close tab (middle-click also closes) |
| `⌥ + click ×` | Close other tabs |
| `⇧⌘T` | Reopen closed tab |
| `⇧⌘]` | Next tab |
| `⇧⌘[` | Previous tab |
| `⇧F6` | Rename console (same as Rename File… in the tab menu) |
| `⌥⇥ / ⌥⇧⇥` | Go to the next split (⇧ previous) |
| `right-click tab` | Pin, Split Right/Down (up to 4 panes), Move to Next Group, Close others… |
| `drag tab` | Reorder, or drop on a pane's right/bottom edge to split it (up to 4) |

## Database Explorer

| macOS | Action |
|---|---|
| `⌘↓ / F4` | Edit data (table) / open console (data source; Redis: key browser) |
| `⇧⌘L` | New query console |
| `⇧⌘F10` | Jump to query console |
| `⌘b` | Go to DDL |
| `⌥⌘g` | SQL Generator |
| `⌥⇧⌘G` | Copy DDL to clipboard |
| `⌥⇧⌘B` | Open DDL in console |
| `⌥⇧⌘C` | Copy reference |
| `F1` | Quick documentation |
| `⌘F6` | Modify table (columns, keys, indexes, foreign keys) |
| `⇧F6` | Rename table / view / sequence / schema / database |
| `⌘;` | Data source properties |
| `⌘n` | New data source |
| `⌘d` | Duplicate data source |
| `⌘r / ⌥⇧⌘R` | Refresh |
| `⌥⌘y` | Refresh (anywhere) |
| `⌘F2` | Disconnect |
| `⌦ / ⌘⌫` | Remove data source / drop object (asks first) |
| `type letters` | Speed search in the tree |

## Query console

| macOS | Action |
|---|---|
| `⌘↩` | Execute: asks what to run when there are several statements (⚙ Query Execution Settings), or the selection |
| `⌘F2` | Cancel running statement |
| `⌥⌘e` | Query history |
| `⌃Space` | Code completion |
| `⌥⌘l` | Reformat code |
| `⌘/` | Line comment |
| `⌥⌘/` | Block comment |
| `⌘d` | Duplicate line |
| `⌘⌫` | Delete line |
| `⌥⇧↑` | Move line up |
| `⌥⇧↓` | Move line down |
| `⌥↑` | Extend selection |
| `⌥↓` | Shrink selection |
| `⌃g` | Select next occurrence |
| `⌃⌘g` | Select all occurrences |
| `⇧⌘U` | Toggle case |
| `⌘l` | Go to line |
| `⌘f` | Find in editor (⌥⌘r regex · ⌥⌘c match case · ⌥⌘w words) |
| `⌘r` | Replace in editor ($1 groups with regex) |

## Data grid (results & table editor)

| macOS | Action |
|---|---|
| `↩ / F2` | Edit cell |
| `⇧↩` | Open in Value Editor |
| `⌥⌘n` | Set NULL |
| `⌥⌘d` | Set DEFAULT |
| `⌘n` | Add Row |
| `⌘d` | Clone Row |
| `⌘⌫` | Delete Row |
| `⌥⌘z` | Revert Selected |
| `⌘↩` | Submit Changes |
| `⌘c / ⌘v` | Copy (with chosen extractor; CSV / TSV copy values only — right-click › Copy with Headers adds the column names) / paste |
| `⌘a` | Select all |
| `⌘f` | Find on Current Page |
| `⌘r` | Reload Page |
| `⌘F2` | Cancel Running Statements |
| `⌘b` | Go to DDL |
| `click header · ⌥ click` | Sort · add to multi-column sort |

## Query history & Redis browser

| macOS | Action |
|---|---|
| `⌦ / ⌘⌫` | Delete the selected history entry |
| `⌘r` | Redis: scan keys again |
| `⌘s` | Redis: save the value being edited |
| `⌃l` | Redis terminal: clear |

## Other actions (menus, toolbars, Find Action — give them a key in Settings › Keymap)

| macOS | Action |
|---|---|
| `—` | Edit Keymap… |
| `—` | Export Data Sources… |
| `—` | Import Data Sources… |
| `—` | Theme: Dark |
| `—` | Theme: Light |
| `—` | Theme: High Contrast |
| `—` | Theme: Sync with OS |
| `—` | Reset Editor Font Size |
| `—` | Force Refresh |
| `—` | Collapse All |
| `—` | Show System Schemas |
| `—` | New Materialized View… |
| `—` | Generate SELECT Statement |
| `—` | Generate INSERT Statement |
| `—` | Generate UPDATE Statement |
| `—` | Generate DELETE Statement |
| `—` | Truncate… |
| `—` | Export Table Data… |
| `—` | Import Data into New Table… |
| `—` | Sessions & Locks |
| `—` | Slow Queries |
| `—` | Table Sizes & Indexes |
| `—` | Server Overview |
| `—` | Users & Privileges |
| `—` | Compare & Migrate… |
| `—` | Close Other Tabs |
| `—` | Close All but Pinned |
| `—` | Close Tabs to the Left |
| `—` | Close Tabs to the Right |
| `—` | Close All Tabs |
| `—` | Pin / Unpin Tab |
| `—` | Copy Tab Reference |
| `—` | Copy Data Source Name |
| `—` | Split and Move Right |
| `—` | Split and Move Down |
| `—` | Move Tab to Next Group |
| `—` | Open Tab in Next Group |
| `—` | Move Tab to New Workspace… |
| `—` | Duplicate Console |
| `—` | New Workspace… |
| `—` | Rename Workspace… |
| `—` | Split Right |
| `—` | Split Down |
| `—` | Explain Plan |
| `—` | Transaction Mode: Manual |
| `—` | Transaction Mode: Auto |
| `—` | Pin / Unpin Result Tab |
| `—` | Close Result Tab |
| `—` | Close All Unpinned Result Tabs |
| `—` | Clear Output |
| `—` | Generate UUID |
| `—` | Copy with Headers |
| `—` | Preview Pending Changes |
| `—` | View as Table |
| `—` | View as Transposed Table |
| `—` | View as Chart |
| `—` | Show Filter |
| `—` | Sort via ORDER BY |
| `—` | Reset View |
| `—` | View Query |
| `—` | Copy Query to Console |
| `—` | Copy Query to Clipboard |
| `—` | Commit |
| `—` | Roll Back |
| `—` | Explain Analyse |
| `—` | Export Data… |
| `—` | Copy as IN List |
| `—` | AI Search (RAG)… |
| `—` | AI Search (RAG) — Index This Table… |
| `—` | Delete Workspace… |
| `—` | Remove Data Source from This Workspace |
| `—` | Production Log… |
| `—` | Snippets… |
| `—` | Save Query as Snippet… |
| `—` | Show Diagram (ERD) |
| `—` | Back Up Database… |
| `—` | Restore Database… |
| `—` | Import Data from File… |
| `—` | Generate Test Data… |
| `—` | New Table… |
| `—` | New View… |
| `—` | New Schema… |
| `—` | New Database… |
| `—` | New Sequence… |
| `—` | Copy Related Rows To… |
| `—` | Manage Shown Schemas… |
| `—` | Unsplit All |

<!-- keymap:gui:end -->

---

[← User guide](README.md)
