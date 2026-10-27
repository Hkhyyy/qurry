# Qurry

A free DataGrip alternative for PostgreSQL, MySQL/MariaDB and Redis — query + ferry. Formerly **dbtool**.

![Qurry GUI](docs/screenshots/gui.png)

![qurry-tui](docs/screenshots/tui.png)

- **Free and open source.** DataGrip-style explorer, query console, data editor, explain plan, DDL, full-text search and data export (CSV, TSV, JSON, SQL INSERT, Markdown) and import (CSV, TSV, JSON), with DataGrip key bindings.
- **GUI, terminal and AI from one engine.** The desktop app, the terminal UI (great over SSH) and the MCP server share the same data sources and passwords.
- **Safe for AI.** The MCP server is read-only by default, and the engine enforces that: one reading statement, in a READ ONLY transaction that is rolled back.
- **Small and fast.** Rust + Tauri, no JVM. SSH tunnels, SSL/TLS, Redis Sentinel and Cluster built in.

| Binary | What it is |
|---|---|
| **Qurry** (GUI, Tauri) | Desktop app modeled on DataGrip |
| **qurry-tui** | Terminal UI — works on its own, no GUI needed |
| **qurry-mcp** | MCP server so AI tools such as Claude Code can explore and query your databases |

All three share the same data sources and passwords, so a connection created in one shows up in the others.

- Config folder: `~/Library/Application Support/com.hyunkyo.dbtool` (macOS), `~/.config/com.hyunkyo.dbtool` (Linux). The folder keeps its old name so data sources and passwords from dbtool carry over.
- Custom shortcuts are stored in `keymap.json` in that folder (`"gui"` and `"tui"` sections, only the ones you changed)
- Passwords are stored in `secrets.json` in that folder, encrypted with AES-GCM (key in `secret.key`). The folder is 0700 and its files 0600. The OS keychain is not used, so anyone who can read your files (or a copy of this folder) can decrypt the passwords.

## Download

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

### Updates

Both apps check GitHub for a newer release when they start and every hour while they run (one request to the GitHub API) and tell you when there is one. You can also check right away.

- **GUI** — a `⬆ vX.Y.Z available` button appears in the status bar. It lists what changed in each version since yours. **Update & Restart** downloads the new version, checks its signature, replaces the app and opens it again with your tabs. The release page is the manual fallback. Check now from **Qurry › Check for Updates…** or **Settings › Appearance › Updates**, where you can also turn the automatic check off. In-app updates start from 0.1.9 (Apple Silicon), and from the first Intel / Linux release.
- **TUI** — the status line shows `⬆ vX.Y.Z available`. Run **⌃k › Update Qurry & Restart** to download the release, verify it against the signed `SHA256SUMS`, replace `qurry-tui` and `qurry-mcp` in place and restart with your tabs. **⌃k › Check for Updates** checks now and lists what changed in each version since yours. From a shell, `qurry-tui --update` does the same. A TUI built from source (`target/release`) is not checked or replaced; update it with `git pull` and `cargo build`. A Homebrew install is updated with `brew upgrade qurry`.
- Turn the check off with `qurry-tui --no-update-check` or `QURRY_NO_UPDATE_CHECK=1`.

## Supported databases

| Database | Features |
|---|---|
| PostgreSQL | Explorer tree (databases, schemas, tables, views, sequences, routines), query console (server-side cursor paging, cancel, manual transactions), data editing, explain plan, DDL, full-text search |
| MySQL / MariaDB | Same as above. A MySQL database is its own schema, so tables appear directly under the database |
| Redis | Medis-style browser (key list with type and TTL, per-type value editor), redis-cli style terminal, **Standalone / Sentinel / Cluster** |

Connection options:
- **SSH tunnel** with a key or password. Host keys are checked against `~/.ssh/known_hosts`: a changed key is refused, and an unknown server stops with its SHA256 fingerprint until you choose **Trust host key** (GUI: data source dialog; TUI: `Ctrl+y` in the form or `Ctrl+k › Trust SSH Host Key`).
- **SSL/TLS**: Require / Verify CA / Full Verification, with optional CA and client certificate. Works for PostgreSQL, MySQL and Redis.
- **Options tab**: read-only, transaction control and isolation level, time zone, keep-alive, auto-disconnect when idle, single database mode, startup script.
- **Production**: before a statement that changes data runs, Qurry counts the rows it will change (the same table and WHERE as a `SELECT count(*)`, nothing is changed) and asks. Statements it can't count (`UPDATE … FROM`, `DELETE … USING`, CTEs) still ask. GUI: Options tab · TUI: the `Production` toggle in the data source form.

> Redis Sentinel and Cluster nodes advertise their own addresses, so they can't be combined with an SSH tunnel.

## Database admin (PostgreSQL, MySQL)

GUI: right-click a data source › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `Ctrl+k` › **Database Admin…**

| Group | Reports |
|---|---|
| Activity | **Sessions** (who blocks whom, query / transaction age; refreshes every 3 s) · **Lock Waits** — right-click / `c` cancel the query, `K` terminate the session |
| Performance | **Slow Queries** — top 100 by total time (`pg_stat_statements` / `performance_schema`), open or EXPLAIN in a console |
| Storage | **Table Sizes** · **Unused Indexes** · **Duplicate Indexes** · **Foreign Keys Without an Index** — generate DROP / CREATE INDEX |
| Maintenance | **Vacuum & Dead Rows** · **Transaction ID Age** (wraparound) — generate VACUUM ANALYZE (PostgreSQL) |
| Server | **Server Overview** (uptime, connections, cache hit ratio) · **Replication** lag · **Settings** |
| Security | **Users & Roles** · **Table Privileges** — generate GRANT / REVOKE |

Generated SQL opens in a new console; nothing changes until you run it. Stopping sessions asks first and is blocked on read-only data sources.

**Diagram (ERD)**: GUI — right-click a schema or database › Diagrams › Show Diagram: tables with keys and foreign keys, auto-laid out (parents above children); drag tables, zoom, double-click to open a table, **Copy as Mermaid**, **Save SVG**. TUI — `Ctrl+k` › Show Diagram: each table's foreign keys in and out as text, plus the Mermaid source (`y` copies).

**Compare & migrate**: GUI — right-click a data source › Tools › Compare & Migrate; TUI — `Ctrl+k` › Compare Schemas… / Compare Table Data… / Run Migrations…
- *Schema compare*: the DDL that makes the target schema look like the source (tables, columns, keys, indexes, foreign keys, views); drops come last and are left unchecked. Open it in a console or save it as the next migration file.
- *Data compare*: rows matched by the primary key → INSERT / UPDATE / DELETE to sync the target.
- *Migrations*: a folder of `V1__create.sql`, `V2__add_index.sql` … applied in version order, each in a transaction; applied ones are recorded in `qurry_schema_history`, and a file changed after it was applied is refused.

**Back up / restore**: right-click a database › Tools › Back Up Database… / Restore Database… (TUI: `Ctrl+k`). Runs `pg_dump` / `pg_restore` / `psql` or `mysqldump` / `mysql` from this computer (Homebrew `libpq`, `mysql-client`, Postgres.app and the usual Linux paths are found too; for PostgreSQL the client closest to the server version is used). Passwords go through the environment, SSH tunnels are used, progress shows in the status bar and can be cancelled.

**Query history & snippets** are kept in the config folder and shared by the GUI and the TUI (passwords in `PASSWORD '…'` / `IDENTIFIED BY '…'` are masked). GUI: `⌥⌘e` (History / Snippets tabs, Save as Snippet) · TUI: `Ctrl+k` › Query History…, Snippets…, Save Query as Snippet….

**Explain plan** (PostgreSQL): each node shows its own share of the time (Explain Analyze) or cost, and flags what is worth a look — a Seq Scan over a big table, row estimates off by 10× or more, filters that throw away most rows, sorts and hashes spilled to disk. TUI: `Alt+e` (or `Ctrl+k` › Explain Analyze).

## Build & run

```bash
# GUI (dev mode)
pnpm install
pnpm tauri dev

# GUI installer (.dmg on macOS, .AppImage / .deb on Linux)
pnpm tauri build

# TUI / MCP
cargo build --release -p qurry-tui -p qurry-mcp
ln -sf "$PWD/target/release/qurry-tui" ~/.local/bin/qurry-tui
ln -sf "$PWD/target/release/qurry-mcp" ~/.local/bin/qurry-mcp

qurry-tui            # press F1 for the keymap
qurry-tui --help     # print the keymap
```

## Layout

```
crates/core   qurry-core: drivers (PostgreSQL, MySQL, Redis), sessions, SSH tunnels, storage, AppState (every feature)
src-tauri     GUI backend: Tauri commands call AppState methods directly
src           GUI frontend (React + Monaco)
crates/tui    TUI (ratatui)
crates/mcp    MCP server (stdio JSON-RPC)
```

## GUI keyboard shortcuts

Inside the app the same list is searchable via **Help › Keymap Reference**, the ⌨ toolbar button, or `⇧⌘/`. **Find Action** (`⇧⌘A`) lists every command together with its shortcut.
On Windows/Linux read `⌘` as `Ctrl` and `⌥` as `Alt`.

Command shortcuts can be changed in **Settings › Keymap** (or **Edit Keymap…** in the Keymap Reference): click *Change* and press the new keys. The tables below show the defaults. Navigation keys inside the tree, grid and popups are fixed.

<!-- keymap:gui:start -->

#### Search & Navigation

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

#### In search results (Search Everywhere / Go to)

| macOS | Action |
|---|---|
| `↩` | Select in Database Explorer |
| `⌘↓` | Edit data (open table) |
| `⌘b` | Go to DDL |
| `F1` | Quick documentation |
| `⌥F1` | Select in Database Explorer |

#### Tool windows & appearance

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

#### Editor tabs

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

#### Database Explorer

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

#### Query console

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
| `⌘f` | Find in editor |

#### Data grid (results & table editor)

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

#### Query history & Redis browser

| macOS | Action |
|---|---|
| `⌦ / ⌘⌫` | Delete the selected history entry |
| `⌘r` | Redis: scan keys again |
| `⌘s` | Redis: save the value being edited |
| `⌃l` | Redis terminal: clear |

<!-- keymap:gui:end -->

## TUI keyboard shortcuts

Press **F1** inside `qurry-tui`, or run `qurry-tui --help`, to see the same list. In the F1 window the **Commands** at the top can be changed: select one, press Enter and then the new key (`a` adds a second key, `Del` resets it). The mouse works too: click to select, double-click to open or edit, scroll with the wheel.
Ctrl+Enter is only distinguishable in terminals that support the kitty keyboard protocol (iTerm2, kitty, WezTerm, Ghostty). Elsewhere use F5 or Alt+Enter.

**New to it? `Ctrl+p` alone is enough to start.** It searches tables, columns, open tabs and actions (data sources you are not connected to are searched from the list saved the last time they were loaded). The status bar always shows the keys for what you are looking at (explorer, console, table, results, Redis), followed by the most useful global keys. The F1 window starts with the same short list. Keys are shown lowercase when Shift is not needed (`Ctrl+k`); an uppercase letter means Shift (`D`, `Alt+Shift+X`).

| Key | Action |
|---|---|
| `Ctrl+p` | Search everything: tables, columns, tabs, actions (not sure? start here) |
| `Ctrl+k` | All actions, each with its shortcut |
| `Ctrl+l` | New query console for the selected data source |
| `F5 / Ctrl+Enter / Alt+Enter` | Run (asks what to run when there are several statements) |
| `Ctrl+e` | Recent tabs |
| `Ctrl+w` | Close tab |
| `Enter` (explorer) | Connect · open a table |
| `s` (explorer) | Choose the databases and schemas shown |
| `Esc` | Back (to the explorer, or from results to the editor) |
| `Ctrl+q` | Quit |

<!-- keymap:tui:start -->

#### Commands (default keys — change them in the F1 window)

| Key | Action |
|---|---|
| `Ctrl+q` | Quit |
| `F1` | Keymap |
| `Ctrl+k` | Actions Palette |
| `Ctrl+p` | Search Everywhere (objects · columns · tabs · actions) |
| `Ctrl+o` | Go to Database Object |
| `Ctrl+b` | Go to DDL |
| `Ctrl+e` | Recent Tabs / Switcher |
| `Ctrl+l` | New Query Console |
| `Ctrl+w` | Close Tab |
| `Alt+Right / Ctrl+PgDn` | Next Tab |
| `Alt+Left / Ctrl+PgUp` | Previous Tab |
| `Alt+1` | Database Explorer |
| `F6` | Cycle Focus (explorer → editor → results) |
| `Alt+\` | Split Right (focused pane, up to 4) |
| `Alt+-` | Split Down (focused pane, up to 4) |
| `Alt+Tab` | Go to the Next Split |
| `Alt+Shift+Left` | Move Split Divider Left / Up |
| `Alt+Shift+Right` | Move Split Divider Right / Down |
| `Alt+[` | Database Explorer Narrower |
| `Alt+]` | Database Explorer Wider |
| `F5 / Ctrl+Enter / Alt+Enter` | Execute (asks what to run — see Query Execution Settings) |
| `Alt+e` | Explain Plan |
| `Ctrl+Space` | Code Completion |
| `Ctrl+c` | Cancel Running Statement |
| `Alt+t` | Toggle Transaction Mode (Auto/Manual) |
| `Alt+c` | Commit |
| `Alt+r` | Roll Back |
| `Alt+o` | Toggle Output / Result |
| `Ctrl+Alt+Up` | Go to the Split Above |
| `Ctrl+Alt+Down` | Go to the Split Below |
| `Ctrl+Alt+Left` | Go to the Split on the Left |
| `Ctrl+Alt+Right` | Go to the Split on the Right |
| `Alt+Shift+Up` | Shrink Editor (console) |
| `Alt+Shift+Down` | Grow Editor (console) |
| `Ctrl+s` | Submit Changes |
| `Ctrl+F2` | Disconnect |

#### Database Explorer

| Key | Action |
|---|---|
| `↑ ↓  (j k)` | Move |
| `→ / ←  (l h)` | Expand / collapse (← on a child jumps to its parent) |
| `Enter / double-click` | Expand / collapse (connects) · open table data · open a Redis key · show DDL of routines |
| `F4 / Ctrl+↓` | Open a query console for the data source / database (Redis: the key browser) |
| `/` | Filter loaded nodes (/ again edits it · empty or Esc clears) |
| `n / e / D` | New / edit / remove data source |
| `c / X` | Connect / disconnect |
| `r / F5` | Refresh |
| `s` | Manage shown databases and schemas (Space check · Enter apply · shared with the GUI) |
| `y  ⌘c` | Copy name (OSC 52 clipboard) |
| `PgUp / PgDn · Home / End` | Page · first / last node |
| `?` | Keymap (same as F1) |
| `Tab / Esc` | Go to the open tab |

#### Query console

| Key | Action |
|---|---|
| `completion list` | Opens as you type (or Ctrl+Space) · ↑↓ choose · Enter/Tab insert · Esc close |
| `Shift+Tab` | Go to the result grid (Esc / Tab returns) |
| `Esc` | Go to the explorer |
| `Ctrl+u / Ctrl+r` | Undo / redo (editor) |
| `Ctrl+f` | Find / replace in the editor: `Alt+x` regex · `Alt+c` match case · `Alt+w` words · `Enter` next · `Tab` to Replace: `Enter` one, `Alt+a` all (`$1` groups) |
| `⌘a  ⌫` | Select all, then delete (or Ctrl+k › Clear Console) |
| `click / drag  ⌘c` | Move the cursor / select text (runs only the selection) · copy the selection |
| `mouse drag` | Drag the editor/result border to resize |

#### Tabs & split (mouse)

| Key | Action |
|---|---|
| `drag a tab` | Onto a pane's right / bottom edge = split it (up to 4 panes) · onto another pane or tab bar = move · within a bar = reorder |
| `drag the border` | Resize the Database Explorer (its right border) or the panes (the border between them; in a top/bottom split also the empty part of the lower tab bar) |
| `click a pane` | Focus it |
| `Shift+wheel / trackpad` | Scroll the grid and the explorer sideways (Shift+← / Shift+→ in the explorer) |

#### Data grid (results & table data)

| Key | Action |
|---|---|
| `arrows  (h j k l)` | Move |
| `PgUp / PgDn · g / G` | Page · first / last row (more rows load automatically) |
| `Home / End` | First / last column |
| `Shift+↑↓←→ / drag` | Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or drag the bar on the bottom border |
| `Enter / F2` | Edit cell (table data) |
| `n` | Set NULL (table data) |
| `a / Insert` | Add row (table data) |
| `d / Delete` | Mark row deleted, again = undo (table data) |
| `Ctrl+z` | Revert pending changes (table data) |
| `s` | Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows) |
| `w or / · o` | Edit WHERE · ORDER BY (table data) |
| `r` | Reload (table data) |
| `x` | Export data (CSV, TSV, JSON, SQL INSERT, Markdown → file or clipboard). Import: ⌃K › Import Data from File… |
| `y  ⌘c` | Copy cell value (a selected range: as TSV) |
| `Y` | Copy as an IN list: `('a', 'b', …)` — the selected range, or the whole column. Paste after `WHERE x IN` |
| `⌘v` | Paste into cells from the selected one (tabs / lines fill several; extra lines add rows; table data) |
| `Esc` | Back (query results: to the editor) |

#### DDL / routine view

| Key | Action |
|---|---|
| `↑ ↓  (j k) · PgUp / PgDn` | Scroll |
| `y  ⌘c` | Copy the text |
| `Esc` | Back to the explorer |

#### Redis browser (Medis style)

| Key | Action |
|---|---|
| `↑ ↓` | Select key |
| `Enter / → / Tab` | Go to the value |
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

#### Data source form

| Key | Action |
|---|---|
| `↑ ↓ / Tab` | Move between fields |
| `type / Backspace` | Edit text fields |
| `Space / ← →` | Toggle on/off · change choice (Type, Color, SSL mode, Redis mode…) |
| `Ctrl+t` | Test connection |
| `Ctrl+y` | Trust the SSH server's host key (after Ctrl+t showed its fingerprint) |
| `Enter / Ctrl+s` | Save |
| `Esc` | Cancel |

#### Popups

| Key | Action |
|---|---|
| `Ctrl+b` | Go to DDL (in Search Everywhere / Go to Database Object) |
| `type` | Filter (palettes) |
| `↑ ↓ / PgUp PgDn` | Move / scroll |
| `Enter` | Run / confirm |
| `y / n` | Answer confirmations |
| `Esc` | Close |

<!-- keymap:tui:end -->

## MCP server

```bash
claude mcp add qurry -- qurry-mcp                   # read-only (default)
claude mcp add qurry-rw -- qurry-mcp --allow-write  # adds the execute tool
```

| Tool | Description |
|---|---|
| `list_data_sources` | Configured connections (kind, host, default database, read-only, SSH) |
| `list_databases` / `list_schemas` / `list_objects` | Browse the tree (Redis: keys and their types) |
| `describe_table` | Columns, indexes, constraints |
| `get_ddl` | DDL (Redis: commands that recreate the key) |
| `search_objects` | Fuzzy search by object name (Go to Database Object), across the databases visible in each data source |
| `query` | Read-only query (100 rows by default, max 1000) |
| `explain` | Execution plan (optionally ANALYZE) |
| `full_text_search` | Find a value in every table |
| `redis_scan` / `redis_get` | Redis key list / key details |
| `export_csv` | Save a read-only query result as CSV in `~/Downloads/qurry-exports` (file name only, never overwrites) |
| `execute` | Only with `--allow-write`. Runs statements in one transaction; refused for read-only data sources |

**How read-only is enforced**
- PostgreSQL / MySQL: exactly one statement, starting with SELECT / WITH / VALUES / TABLE / SHOW / EXPLAIN / DESCRIBE, executed inside a READ ONLY transaction with a 60 s time limit that is rolled back afterwards. Statements are split with the server's own comment rules (MySQL `--x` and `/*! … */`, MariaDB `/*M! … */`, backquoted names, PostgreSQL nested comments), and MySQL runs over the prepared-statement protocol, which accepts a single statement. `COMMIT; DELETE …`, `WITH d AS (DELETE …) SELECT …`, `EXPLAIN ANALYZE DELETE`, `set_config(…read_only…)`, `SELECT … INTO OUTFILE` and functions with side effects outside the transaction (`pg_terminate_backend`, `pg_read_file`, `dblink`, `load_file`, `sleep`, …) are rejected.
- Redis: only read commands, and not `CONFIG` (it can reveal passwords) or `KEYS` (use `redis_scan`).
- Still, give the MCP server a **least-privilege database user** (read-only role, no superuser/FILE privilege): the checks above are a second line of defence, not a replacement for database permissions.

Options: `--config-dir DIR` (use another config folder), `--verbose` (engine logs to stderr).

## Safety

- **Read-only data sources** are enforced by the engine, not only the UI: in every console, the table editor, Redis and exports, only reading statements run (plus transaction control and `SET`, but not switching read-only off).
- **Transactions**: a transaction you open (Manual mode, or `BEGIN` in Auto mode) is never committed or rolled back behind your back by result paging. Switching Manual → Auto with uncommitted changes asks first, and closing a tab, disconnecting, switching database or quitting with uncommitted changes asks before rolling them back. Data sources with open transactions are not auto-disconnected.
- **Confirmations** before `UPDATE`/`DELETE` without `WHERE`, `DROP`, `TRUNCATE` and Redis `FLUSHALL`/`FLUSHDB`/`DEL` (GUI and TUI).
- **Scripts in Auto mode** run statement by statement: if one fails, the output says how many earlier statements were already committed.
- **Table editor** saves all edits in one transaction, and each `UPDATE`/`DELETE` must change exactly one row, otherwise nothing is saved.
- **Import** of data source files shows what changes first (e.g. "turns Read-only OFF", new startup script).
- **Query history** (GUI) hides passwords in `CREATE/ALTER USER … PASSWORD '…'`, `IDENTIFIED BY '…'` and Redis `AUTH`.
- **Releases** are built only from `main` by GitHub Actions pinned to commit SHAs, with `SHA256SUMS` and build provenance attestations. The update signing key is used only in a separate signing step (never during the build). The GUI checks the update bundle's signature, and `qurry-tui --update` checks the signature of `SHA256SUMS` (same key) and then the checksum before installing.

## Tests

Write the failing test first: a bug fix starts with a test that reproduces it, a feature with a test for the new behavior (screens and terminal keys that can't be tested are checked by hand). Every pull request runs the CI workflow: Rust unit tests, integration tests against PostgreSQL / MySQL / Redis containers, vitest and the web build.

```bash
cargo test -p qurry-core -p qurry-tui -p qurry-mcp   # unit tests (no database)
pnpm test                                               # GUI logic (src/lib/*.test.ts, vitest)
cargo test --workspace -- --include-ignored --skip tunnel   # needs a local PostgreSQL and the containers below
QURRY_TEST_PG=127.0.0.1:15499:postgres:shop cargo test -p qurry-core session -- --ignored   # PostgreSQL elsewhere (host:port:user:db)
```

Containers for the integration tests:

```bash
docker run -d --name dbtool-mysql-test -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop -p 13306:3306 mysql:8.4
docker run -d --name dbtool-redis-test -p 16379:6379 redis:7-alpine
# TLS (16391), Sentinel (26392 → 16392) and Cluster (17001-17003) servers: see the test comments in crates/core/src/redisdb.rs
# (TLS certificate folder: QURRY_TEST_TLS_DIR, default /tmp/dbtool-tls)
```

TUI tests can drive the app without a database: `crates/tui/src/testkit.rs` opens table tabs with fixed rows, sends keys (`h.key("Ctrl+A")`, `h.type_str("ban")`) and renders the screen to text (`h.screen(120, 30)`). See the tests at the end of `crates/tui/src/input.rs`.

After changing a shortcut, regenerate the tables in this README with `python3 scripts/gen-readme-keymap.py`.

## License

[MIT](LICENSE)
