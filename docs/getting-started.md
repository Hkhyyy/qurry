# Getting started

English | [한국어](ko/getting-started.md)

How to install Qurry, keep it up to date and get from an empty window to your first query.

## Install

**TUI + MCP with Homebrew** (macOS, Linux):

```bash
brew install hkhyyy/qurry/qurry      # update: brew upgrade qurry
```

Or get the latest build from [Releases](https://github.com/Hkhyyy/qurry/releases):

| | GUI | TUI + MCP |
|---|---|---|
| macOS, Apple Silicon | `Qurry_<version>_aarch64.dmg` | `qurry-cli_<version>_aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `Qurry_<version>_x64.dmg` | `qurry-cli_<version>_x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `Qurry_<version>_amd64.AppImage` or `.deb` | `qurry-cli_<version>_x86_64-unknown-linux-gnu.tar.gz` |

- **macOS**: the GUI needs macOS 13 Ventura or later (on macOS 12 or older keep Qurry 0.2.x, see [Updates](updates.md#macos-12-and-older); the TUI and MCP server still run). Drag `Qurry.app` to Applications. The app is not code-signed yet, so macOS shows an "unidentified developer" / "damaged" warning on first launch. Run `xattr -dr com.apple.quarantine /Applications/Qurry.app` (or right-click the app → Open).
- **Linux**: `chmod +x Qurry_*.AppImage` and run it, or `sudo apt install ./Qurry_<version>_amd64.deb`. The GUI needs WebKitGTK 4.1 (Ubuntu 22.04+, Debian 12+, Fedora 38+).
- **TUI / MCP**: `tar -xzf qurry-cli_….tar.gz && mkdir -p ~/.local/bin && mv qurry-tui qurry-mcp ~/.local/bin/` (macOS: `xattr -d com.apple.quarantine qurry-tui qurry-mcp` first).

Stable releases come from a version tag; every change to `main` also publishes a beta pre-release.

## Updates

Both apps tell you when a new version is out and install it in place: GUI `⬆ vX.Y.Z available` › **Update & Restart**, TUI **⌃k › Update Qurry & Restart** (or `qurry-tui --update`). You get stable releases unless you switch to the beta channel (GUI **Settings › Updates**, TUI **⌃k › Update Channel…**). Channels, turning the check off and macOS 12: [Updates](updates.md).

## First steps

### GUI

1. **Add a data source.** Click `+` at the top of the Database Explorer (or press `⌘n`), choose the database type and fill in host, port, user and password. **Test Connection** shows the server version. Types such as SQLite or SQL Server need a small driver first — the dialog offers to install it ([Drivers](data-sources.md#drivers-added-when-you-need-them)).
2. **Connect and browse.** Expand the data source in the explorer. Right-click a data source (or a database) › **Manage Shown Schemas…** to choose which databases and schemas appear.
3. **Run a query.** Select the data source and press `⇧⌘L` for a new query console. Write SQL and press `⌘↩`. When the editor holds several statements Qurry asks which one to run; a selection runs as is. With the caret inside a subquery (`(select …)`, a CTE body, `in (select …)`) it also offers that subquery, after the whole statement and innermost first, as DataGrip does: the editor highlights exactly what each choice runs, and the choice runs only that text, through the same checks as any run (read-only, the Production confirmation, the dangerous-statement ask). A parenthesised list such as `in (1, 2)` is not offered, nor a column whose name starts with a query word (`(table_name = …)`). While it runs, the toolbar's ▶ is ■ in the same spot: click it or press `⌘F2` to cancel. A result grid's ↻ is ■ the same way while it loads, also while it loads more rows as you scroll; that ■ stops only the rows on their way, the ones shown stay. The status bar shows what the focused tab is doing and for how long (hover it for a ✕ that cancels too), the caret position (`Ln 2, Col 11`) while the editor has focus, and for a focused result grid its current cell (`Row 3, Col 5`) or the selected rows and columns (`Rows 5–26 (22) · Col 4`) and, when not every column fits, the ones in view (`cols 3–8 of 14`); right-click it to add Not null, Sum, Avg, Min, Max or the cell's Size ([Status bar items](settings.md#status-bar-items)). Each statement that ran is marked in the gutter: ✓ done, ✗ failed, a spinner while it runs (a run stops at the first statement that fails, so the ones after it get no mark). Hover a mark to see when it ran and how long it took with the rows, or the error; click it to see its line in **Output**. Marks show the latest run only: a new run clears the marks of earlier ones once it starts, and editing a statement drops its mark. When a statement fails, its error is also marked in the editor (a red squiggle; hover it for the message): where the server points (PostgreSQL's position, MySQL's line and `near '…'`), or over the whole statement when the server doesn't say. Fix the statement and run it again: any edit or the next run clears the mark, and **Output** keeps the error (with PostgreSQL's `(position N)`, which the mark leaves out). Under each statement **Output** also lists what PostgreSQL said while it ran (`NOTICE: …`, `WARNING: …` in the warning color, `INFO: …`: a `RAISE NOTICE` in a `DO` block, "already exists, skipping" from `IF NOT EXISTS`), up to 100 per statement and then `… N more`; MySQL's warnings aren't shown yet. In the TUI the spot is underlined and the status line says where (`Error at line 3, col 8: …`). Each statement's result gets a tab of its own, numbered by the statement's place in the script (`2: orders`): moving the caret into a statement shows its tab, running the statement again replaces that tab's rows (its column widths stay), and deleting the statement closes its tab. A pinned tab (right-click › **Pin Tab**) is never replaced and no longer follows its statement: running the statement again opens a new tab. A tab's ↻ refreshes that tab, pinned or not. Each console keeps up to 10 unpinned result tabs ([Settings › Appearance › Console › Result tabs](settings.md#result-tabs)); past that, the least recently shown one closes.
4. **Open and edit a table.** Double-click a table (or `⌘↓`). Edit cells in place (`↩`), add rows with `⌘n`, delete with `⌘⌫`, and press `⌘↩` to save all changes in one transaction. `⌘v` pastes TSV or CSV (what Qurry's copy or a spreadsheet writes) from the current cell and adds rows as needed: an empty field is NULL, `""` an empty string, and one line without a tab is one value (a row the grid just copied as CSV still splits into its cells). `⇧↩` opens the **Value Editor** beside the grid: it shows the current cell's whole value (JSON pretty-printed and highlighted) and follows the selection while the grid keeps the arrow keys. Click into it or press `⇧↩` again to edit, `⌘↩` applies (moving to another cell applies too), `⎋` goes back to the grid, `⌘w` or `⇧⎋` closes it. For JSON, **{ }** switches between pretty-printed and one line, and text that isn't valid JSON is refused in a `json` / `jsonb` column, with the reason. A console result can be edited the same way when it is a plain `SELECT` from one table and includes the table's whole primary key (aliases and a subset of the columns are fine). A join is edited per table: the columns of each table whose whole primary key is in the result, by that key ([Safety › Editing a join's result](safety.md)); rows can't be added or deleted there. `GROUP BY`, `DISTINCT`, `UNION`, computed columns, views and results without the key stay read-only, and the greyed-out edit buttons (for a join, the dimmed 🔒 column header) say why (e.g. "Read-only: the result doesn't include the primary key of public.users (id)."). In Manual mode or while a transaction is open, Submit runs the edits in it, and they stay pending until Commit / Roll back. Running the console again with unsaved edits asks first.
5. **Find anything.** Press `⇧` twice (Search Everywhere) to find tables, columns, open tabs and commands. `⇧⌘A` lists every command with its shortcut, and `⇧⌘/` shows the whole keymap.

### TUI

Start `qurry-tui` in a terminal. (Keys below are written as on macOS; on Linux `⌃` is `Ctrl` and `⌥` is `Alt`.)

1. **Add a data source.** In the explorer press `n`, fill in the form (`Tab` moves, `Space` / `←→` changes a choice, `⌃t` tests) and save with `⌃s`. `e` edits the selected data source.
2. **Connect and browse.** `Enter` on a data source connects and expands it; `s` chooses the databases and schemas shown.
3. **Run a query.** `⌃l` opens a console for the selected data source. Run with `F5` (or `⌃Enter` / `⌥Enter`), or click **[▶ Run]** in the console's title; while it runs (or its result loads more rows) that is **[■ Cancel]**, and `⌃c` cancels too. A result wider than the window shows which columns are in view on its bottom border: the heavier part of the line is the part shown (click or drag it), and `◀ cols 3–8 of 14 ▶` at its right end. The line-number column marks the first line of each statement that ran: `✓` done, `✗` failed, `…` running (a run stops at the first failure). Clicking a mark shows **Output**. Output lists PostgreSQL's notices under their statement, as the GUI does. Only the latest run is marked (a new run clears the earlier marks), and editing a line of the statement drops its mark. The result panel's title lists **Output** and one tab per statement (`Output │ 1: customers │ 2: orders`), the shown one underlined: the caret in a statement shows its tab, `⌥.` / `⌥,` step through them (Output included) and a click picks one; when they don't fit, the leftmost fold into `‹n`. Running a statement again replaces its tab and deleting the statement closes it. **⌃k › Pin / Unpin Result Tab** keeps the shown tab as it is (📌; running its statement again opens a new tab), **⌃k › Close Result Tab** closes it, and **⌃k › Result Tabs…** sets how many unpinned tabs a console keeps (10, shared with the GUI).
4. **Open and edit a table.** `Enter` on a table opens its rows. `Enter` / `F2` edits a cell, `a` adds a row, `d` marks a row deleted, and `⌃s` saves. `v` shows the current cell's whole value in a panel beside the grid (JSON pretty-printed) that follows the cursor; `v` or `⌃w` closes it. A console result from one table with its primary key is edited the same way, a join per table (`a` / `d` don't add or delete rows there); for any other result or column, an edit key says in the status line why it can't be edited.
5. **Not sure what to press?** `⌃p` searches tables, columns, tabs and actions; `⌃k` lists every action with its key; `F1` shows the keymap. The status bar always shows the keys for what you are looking at.

### Data sources are shared

The GUI, the TUI and the MCP server read the same data sources and passwords, so a connection made in one shows up in the others.

## Where Qurry keeps your settings

- Config folder: `~/Library/Application Support/com.hyunkyo.dbtool` (macOS), `~/.config/com.hyunkyo.dbtool` (Linux). The folder keeps its old name so data sources and passwords from dbtool carry over.
- App log: `~/Library/Logs/Qurry/qurry.log` (macOS), `~/.local/state/qurry/qurry.log` (Linux), `%LOCALAPPDATA%\Qurry\logs\qurry.log` (Windows), written by the GUI, the TUI and the MCP server together (each line says which, with its time). It holds what helps with a bug report: connects, SSH and AWS SSM tunnels opening and closing, reconnects, pools closing and housekeeping (keep-alive, auto-disconnect), each with how long it took. It never holds passwords, keys, tokens, query text, values or rows: secrets show as `***` and SQL literals as `'…'`, server errors from catalog reads and lost connections are logged by their SQLSTATE only, and connect and tunnel errors in full (with the same redaction) so the log shows why a connect failed. A file grows to 5 MB, then moves to `qurry.1.log` (and that one to `qurry.2.log`), so it takes at most 15 MB. The folder is 0700 and its files 0600. You can delete it any time; Qurry starts a new one. Run with `--config-dir DIR` (TUI, MCP server), the log is `DIR/logs/qurry.log` instead.
- Custom shortcuts are stored in `keymap.json` in that folder (`"gui"` and `"tui"` sections, only the ones you changed)
- Settings the GUI and the TUI share (theme and custom themes, update channel, keyword case, grid alignment, table column widths) are stored in `settings.json` in that folder ([Settings](settings.md))
- Passwords are stored in `secrets.json` in that folder, encrypted with AES-GCM. The folder is 0700 and its files 0600. By default the key is in the same folder (`secret.key`), so anyone who can read your files (or a copy of this folder, e.g. a backup) can decrypt the passwords. To keep the key in the system keychain instead (macOS Keychain, Secret Service on Linux, Windows Credential Manager): GUI **Settings › Passwords** · TUI `⌃k` › **Password Key…**. Only that one key goes into the keychain; each app (GUI, TUI, MCP server) asks for access once, and on macOS again after each update while Qurry isn't code-signed. Over SSH or on a headless Linux without a keychain, keep the key in the folder. Update every Qurry app on the machine before moving the key: Qurry 0.1.0 and 0.1.1 replace a `secret.key` they can't use with a new key, so passwords saved by those versions afterwards can't be read by the others.

---

[← User guide](README.md)
