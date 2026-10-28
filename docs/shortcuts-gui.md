# GUI keyboard shortcuts

Inside the app the same list is searchable via **Help › Keymap Reference**, the ⌨ toolbar button, or `⇧⌘/`. **Find Action** (`⇧⌘A`) lists every command together with its shortcut.
On Windows/Linux read `⌘` as `Ctrl` and `⌥` as `Alt`.

Command shortcuts can be changed in **Settings › Keymap** (or **Edit Keymap…** in the Keymap Reference): click *Change* and press the new keys. The tables below show the defaults. Navigation keys inside the tree, grid and popups are fixed.

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
| `⌘↓` | Edit data (open table) |
| `⌘b` | Go to DDL |
| `F1` | Quick documentation |
| `⌥F1` | Select in Database Explorer |

## Tool windows & appearance

| macOS | Action |
|---|---|
| `⌘1` | Database Explorer (focus / hide) |
| `⌘3` | Find tool window (Full-text Search results) |
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
| `⇧F6` | Rename console |
| `⌥⇥` | Go to the next split (⇧ previous) |
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
| `⌘;` | Data source properties |
| `⌘n` | New data source |
| `⌘d` | Duplicate data source |
| `⌘r` | Refresh |
| `⌥⌘y` | Refresh (anywhere) |
| `⌘F2` | Disconnect |
| `⌦` | Remove data source / drop object (asks first) |
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
| `⇧↩` | Open value editor |
| `⌥⌘n` | Set NULL |
| `⌥⌘d` | Set DEFAULT |
| `⌘n` | Add row |
| `⌘d` | Clone row |
| `⌘⌫` | Delete rows |
| `⌥⌘z` | Revert selected changes |
| `⌘↩` | Submit changes |
| `⌘c / ⌘v` | Copy (with chosen extractor) / paste |
| `⌘a` | Select all |
| `⌘f` | Find in grid |
| `⌘r` | Reload |
| `⌘F2` | Cancel |
| `click header · ⌥ click` | Sort · add to multi-column sort |

## Query history & Redis browser

| macOS | Action |
|---|---|
| `⌦ / ⌘⌫` | Delete the selected history entry |
| `⌘r` | Redis: scan keys again |
| `⌘s` | Redis: save the value being edited |
| `⌃l` | Redis terminal: clear |

<!-- keymap:gui:end -->

---

[← User guide](README.md)
