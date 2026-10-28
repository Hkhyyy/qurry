## Qurry {{VERSION}}

A free DataGrip alternative for PostgreSQL, MySQL/MariaDB and Redis (Standalone / Sentinel / Cluster), with SSH tunnels and SSL/TLS. Formerly **dbtool**.

### Downloads
| | GUI | TUI + MCP (`qurry-tui`, `qurry-mcp`) |
|---|---|---|
| macOS, Apple Silicon | `Qurry_{{VERSION}}_aarch64.dmg` | `qurry-cli_{{VERSION}}_aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `Qurry_{{VERSION}}_x64.dmg` | `qurry-cli_{{VERSION}}_x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `Qurry_{{VERSION}}_amd64.AppImage` / `.deb` | `qurry-cli_{{VERSION}}_x86_64-unknown-linux-gnu.tar.gz` |

Use any of them on its own or together. When used together they share data sources and passwords. **The TUI works without the GUI** — add and edit connections inside it with `n` / `e`.

### Install
**GUI (macOS)** — open the DMG and drag `Qurry.app` into Applications.
The app is not code-signed yet, so the first launch shows an "unidentified developer" / "damaged" warning. Either run
```bash
xattr -dr com.apple.quarantine /Applications/Qurry.app
```
or right-click the app in Finder → Open.

**GUI (Linux)** — `chmod +x Qurry_{{VERSION}}_amd64.AppImage` and run it, or `sudo apt install ./Qurry_{{VERSION}}_amd64.deb` (needs WebKitGTK 4.1).

**TUI / MCP** — `brew install hkhyyy/qurry/qurry`, or by hand:
```bash
tar -xzf qurry-cli_{{VERSION}}_<target>.tar.gz
xattr -d com.apple.quarantine qurry-tui qurry-mcp 2>/dev/null   # macOS
mkdir -p ~/.local/bin && mv qurry-tui qurry-mcp ~/.local/bin/
qurry-tui                             # F1 = keymap
claude mcp add qurry -- qurry-mcp     # register with Claude Code (read-only by default)
```
Already installed? In the TUI run ⌃k › **Update Qurry & Restart** (or `qurry-tui --update`). The GUI shows `⬆ available` in its status bar; **Update & Restart** installs the new version in place.

Coming from **dbtool**? Your data sources, passwords and settings are kept. Updating an old `dbtool-tui` replaces `dbtool-tui` / `dbtool-mcp` with Qurry under the old names (so existing MCP settings keep working); the next update also adds `qurry-tui` / `qurry-mcp`. The `dbtool-cli_…` file (Apple Silicon) in this release is the same archive, kept so older versions can update.

See the [user guide](https://github.com/Hkhyyy/qurry/tree/main/docs) for all features and shortcuts.
