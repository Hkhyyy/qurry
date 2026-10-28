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

- **macOS**: drag `Qurry.app` to Applications. The app is not code-signed yet, so macOS shows an "unidentified developer" / "damaged" warning on first launch. Run `xattr -dr com.apple.quarantine /Applications/Qurry.app` (or right-click the app → Open).
- **Linux**: `chmod +x Qurry_*.AppImage` and run it, or `sudo apt install ./Qurry_<version>_amd64.deb`. The GUI needs WebKitGTK 4.1 (Ubuntu 22.04+, Debian 12+, Fedora 38+).
- **TUI / MCP**: `tar -xzf qurry-cli_….tar.gz && mkdir -p ~/.local/bin && mv qurry-tui qurry-mcp ~/.local/bin/` (macOS: `xattr -d com.apple.quarantine qurry-tui qurry-mcp` first).

A new release is published automatically every time `main` changes.

## Updates

Both apps check GitHub for a newer release when they start and every hour while they run (one request to the GitHub API) and tell you when there is one. You can also check right away.

- **GUI** — a `⬆ vX.Y.Z available` button appears in the status bar. It lists what changed in each version since yours. **Update & Restart** downloads the new version, checks its signature, replaces the app and opens it again with your tabs. The release page is the manual fallback. Check now from **Qurry › Check for Updates…** or **Settings › Appearance › Updates**, where you can also turn the automatic check off. In-app updates start from 0.1.9 (Apple Silicon), and from the first Intel / Linux release.
- **TUI** — the status line shows `⬆ vX.Y.Z available`. Run **⌃k › Update Qurry & Restart** to download the release, verify it against the signed `SHA256SUMS`, replace `qurry-tui` and `qurry-mcp` in place and restart with your tabs. **⌃k › Check for Updates** checks now and lists what changed in each version since yours. From a shell, `qurry-tui --update` does the same. A TUI built from source (`target/release`) is not checked or replaced; update it with `git pull` and `cargo build`. A Homebrew install is updated with `brew upgrade qurry`.
- Turn the check off with `qurry-tui --no-update-check` or `QURRY_NO_UPDATE_CHECK=1`.

## First steps

### GUI

1. **Add a data source.** Click `+` at the top of the Database Explorer (or press `⌘n`), choose the database type and fill in host, port, user and password. **Test Connection** shows the server version. Types such as SQLite or SQL Server need a small driver first — the dialog offers to install it ([Drivers](data-sources.md#drivers-added-when-you-need-them)).
2. **Connect and browse.** Expand the data source in the explorer. Right-click a data source (or a database) › **Manage Shown Schemas…** to choose which databases and schemas appear.
3. **Run a query.** Select the data source and press `⇧⌘L` for a new query console. Write SQL and press `⌘↩`. When the editor holds several statements Qurry asks which one to run; a selection runs as is. `⌘F2` cancels a running statement.
4. **Open and edit a table.** Double-click a table (or `⌘↓`). Edit cells in place (`↩`), add rows with `⌘n`, delete with `⌘⌫`, and press `⌘↩` to save all changes in one transaction.
5. **Find anything.** Press `⇧` twice (Search Everywhere) to find tables, columns, open tabs and commands. `⇧⌘A` lists every command with its shortcut, and `⇧⌘/` shows the whole keymap.

### TUI

Start `qurry-tui` in a terminal. (Keys below are written as on macOS; on Linux `⌃` is `Ctrl` and `⌥` is `Alt`.)

1. **Add a data source.** In the explorer press `n`, fill in the form (`Tab` moves, `Space` / `←→` changes a choice, `⌃t` tests) and save with `⌃s`. `e` edits the selected data source.
2. **Connect and browse.** `Enter` on a data source connects and expands it; `s` chooses the databases and schemas shown.
3. **Run a query.** `⌃l` opens a console for the selected data source. Run with `F5` (or `⌃Enter` / `⌥Enter`).
4. **Open and edit a table.** `Enter` on a table opens its rows. `Enter` / `F2` edits a cell, `a` adds a row, `d` marks a row deleted, and `⌃s` saves.
5. **Not sure what to press?** `⌃p` searches tables, columns, tabs and actions; `⌃k` lists every action with its key; `F1` shows the keymap. The status bar always shows the keys for what you are looking at.

### Data sources are shared

The GUI, the TUI and the MCP server read the same data sources and passwords, so a connection made in one shows up in the others.

## Where Qurry keeps your settings

- Config folder: `~/Library/Application Support/com.hyunkyo.dbtool` (macOS), `~/.config/com.hyunkyo.dbtool` (Linux). The folder keeps its old name so data sources and passwords from dbtool carry over.
- Custom shortcuts are stored in `keymap.json` in that folder (`"gui"` and `"tui"` sections, only the ones you changed)
- Passwords are stored in `secrets.json` in that folder, encrypted with AES-GCM (key in `secret.key`). The folder is 0700 and its files 0600. The OS keychain is not used, so anyone who can read your files (or a copy of this folder) can decrypt the passwords.

---

[← User guide](README.md)
