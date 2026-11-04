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

Both apps tell you when a new version is out and install it in place: GUI `⬆ vX.Y.Z available` › **Update & Restart**, TUI **⌃k › Update Qurry & Restart** (or `qurry-tui --update`). You get stable releases unless you switch to the beta channel (GUI **Settings › Appearance › Updates**, TUI **⌃k › Update Channel…**). Channels, turning the check off and macOS 12: [Updates](updates.md).

## First steps

### GUI

1. **Add a data source.** Click `+` at the top of the Database Explorer (or press `⌘n`), choose the database type and fill in host, port, user and password. **Test Connection** shows the server version. Types such as SQLite or SQL Server need a small driver first — the dialog offers to install it ([Drivers](data-sources.md#drivers-added-when-you-need-them)).
2. **Connect and browse.** Expand the data source in the explorer. Right-click a data source (or a database) › **Manage Shown Schemas…** to choose which databases and schemas appear.
3. **Run a query.** Select the data source and press `⇧⌘L` for a new query console. Write SQL and press `⌘↩`. When the editor holds several statements Qurry asks which one to run; a selection runs as is. `⌘F2` cancels a running statement. Each statement that ran is marked in the gutter: ✓ done, ✗ failed, a spinner while it runs (a run stops at the first statement that fails, so the ones after it get no mark). Hover a mark to see when it ran and how long it took with the rows, or the error; click it to see its line in **Output**. Editing a statement drops its mark, and running it again replaces it.
4. **Open and edit a table.** Double-click a table (or `⌘↓`). Edit cells in place (`↩`), add rows with `⌘n`, delete with `⌘⌫`, and press `⌘↩` to save all changes in one transaction. `⇧↩` opens the **Value Editor** beside the grid: it shows the current cell's whole value (JSON pretty-printed and highlighted) and follows the selection while the grid keeps the arrow keys. Click into it or press `⇧↩` again to edit, `⌘↩` applies (moving to another cell applies too), `⎋` goes back to the grid, `⌘w` or `⇧⎋` closes it. For JSON, **{ }** switches between pretty-printed and one line, and text that isn't valid JSON is refused in a `json` / `jsonb` column, with the reason. A console result can be edited the same way when it is a plain `SELECT` from one table and includes the table's whole primary key (aliases and a subset of the columns are fine). Joins, `GROUP BY`, `DISTINCT`, `UNION`, computed columns, views and results without the key stay read-only, and the greyed-out edit buttons say why (e.g. "Read-only: the columns come from more than one table."). Console results are read-only in Manual mode or while a transaction is open, because the edits are saved in a transaction of their own. Running the console again with unsaved edits asks first.
5. **Find anything.** Press `⇧` twice (Search Everywhere) to find tables, columns, open tabs and commands. `⇧⌘A` lists every command with its shortcut, and `⇧⌘/` shows the whole keymap.

### TUI

Start `qurry-tui` in a terminal. (Keys below are written as on macOS; on Linux `⌃` is `Ctrl` and `⌥` is `Alt`.)

1. **Add a data source.** In the explorer press `n`, fill in the form (`Tab` moves, `Space` / `←→` changes a choice, `⌃t` tests) and save with `⌃s`. `e` edits the selected data source.
2. **Connect and browse.** `Enter` on a data source connects and expands it; `s` chooses the databases and schemas shown.
3. **Run a query.** `⌃l` opens a console for the selected data source. Run with `F5` (or `⌃Enter` / `⌥Enter`). The line-number column marks the first line of each statement that ran: `✓` done, `✗` failed, `…` running (a run stops at the first failure). Clicking a mark shows **Output**; editing a line of the statement drops its mark.
4. **Open and edit a table.** `Enter` on a table opens its rows. `Enter` / `F2` edits a cell, `a` adds a row, `d` marks a row deleted, and `⌃s` saves. `v` shows the current cell's whole value in a panel beside the grid (JSON pretty-printed) that follows the cursor; `v` or `⌃w` closes it. A console result from one table with its primary key is edited the same way; for any other result, an edit key says in the status line why it can't be edited.
5. **Not sure what to press?** `⌃p` searches tables, columns, tabs and actions; `⌃k` lists every action with its key; `F1` shows the keymap. The status bar always shows the keys for what you are looking at.

### Data sources are shared

The GUI, the TUI and the MCP server read the same data sources and passwords, so a connection made in one shows up in the others.

## Where Qurry keeps your settings

- Config folder: `~/Library/Application Support/com.hyunkyo.dbtool` (macOS), `~/.config/com.hyunkyo.dbtool` (Linux). The folder keeps its old name so data sources and passwords from dbtool carry over.
- Custom shortcuts are stored in `keymap.json` in that folder (`"gui"` and `"tui"` sections, only the ones you changed)
- Settings the GUI and the TUI share (update channel, grid alignment) are stored in `settings.json` in that folder ([Settings](settings.md))
- Passwords are stored in `secrets.json` in that folder, encrypted with AES-GCM. The folder is 0700 and its files 0600. By default the key is in the same folder (`secret.key`), so anyone who can read your files (or a copy of this folder, e.g. a backup) can decrypt the passwords. To keep the key in the system keychain instead (macOS Keychain, Secret Service on Linux, Windows Credential Manager): GUI **Settings › Passwords** · TUI `⌃k` › **Password Key…**. Only that one key goes into the keychain; each app (GUI, TUI, MCP server) asks for access once, and on macOS again after each update while Qurry isn't code-signed. Over SSH or on a headless Linux without a keychain, keep the key in the folder. Update every Qurry app on the machine before moving the key: Qurry 0.1.0 and 0.1.1 replace a `secret.key` they can't use with a new key, so passwords saved by those versions afterwards can't be read by the others.

---

[← User guide](README.md)
