# TUI in your terminal

English | [한국어](ko/tui.md)

`qurry-tui` adapts to the terminal it runs in: how many colors it has, whether its background is light, how big the window is, and what tmux or the terminal keeps for itself. It all follows the environment, down to the font; the only setting is the theme (shared with the GUI).

## Connect from a URL

For on-call work, `qurry-tui` connects the way `psql`, `mysql` and `redis-cli` do, without saving a data source first:

```sh
qurry-tui postgres://app@db.internal:5432/shop
qurry-tui --read-only mysql://root@127.0.0.1:3306/shop
qurry-tui redis://cache.internal:6379/2
qurry-tui rediss://default@cache.example.com:6380/0          # TLS
qurry-tui --incident postgres://postgres@db.internal/postgres # straight into Database Admin › Sessions
qurry-tui --ssh bastion postgres://app@10.0.3.12/shop         # through an SSH tunnel (a ~/.ssh/config Host works)
qurry-tui --env                                               # the URL in DATABASE_URL
```

The URL opens a **temporary data source** at the top of the explorer (marked **TEMP** before its name, **TEMP · READ-ONLY** with `--read-only`), selects it and connects. It lives only in this `qurry-tui` process: it isn't written to `connections.json` or the password file, the Qurry app, other `qurry-tui` windows and the MCP server never see it, its tabs aren't restored next time, its statements aren't kept in the query history (a snippet saved from its console is usable with every data source), every workspace shows it (it can't be added to, moved to or removed from one), and it is gone when `qurry-tui` exits. Its form can't save it (the message says so); to keep it, add a data source with the same settings (`n` in the explorer). Everything else works as with a saved data source (consoles, tables, Database Admin, the Read-only rules). Your saved data sources are in the explorer below it as usual. Without a URL (or `--env`), `qurry-tui` starts as it always did.

What the URL leaves out comes from the environment, as `psql`, `mysql` and `redis-cli` read it (the URL wins, then the environment, then `~/.pgpass`):

| | Taken from |
|---|---|
| PostgreSQL (`postgres://`, `postgresql://`) | `PGHOST`, `PGPORT`, `PGUSER`, `PGDATABASE`, `PGPASSWORD`, `PGSSLMODE`, then `~/.pgpass` (or `PGPASSFILE`). Defaults as in libpq: `localhost`, `5432`, your login name, a database named after the user. `?host=`, `port`, `user`, `dbname`, `sslmode`, `sslrootcert`, `sslcert`, `sslkey` in the URL work too; `connect_timeout`, `application_name`, `pgbouncer` and `schema` are ignored with a note |
| MySQL (`mysql://`) | `MYSQL_PWD` for the password; `localhost`, `3306` and your login name otherwise. `?ssl-mode=`, `ssl-ca`, `ssl-cert`, `ssl-key`; `charset` and `connect_timeout` are ignored with a note |
| Redis (`redis://`, `rediss://`) | `REDISCLI_AUTH` for the password; `127.0.0.1`, `6379`, database `0` otherwise. The database is the path (`/2`) |

- **Passwords.** Put them in the environment (`PGPASSWORD`, `MYSQL_PWD`, `REDISCLI_AUTH`, or the whole URL in `DATABASE_URL` with `--env`) or `~/.pgpass`, not in a URL on the command line: other users of this computer can see a command line with `ps` for as long as `qurry-tui` runs, so a password there makes the status line (and the terminal, before the screen opens) warn. The environment variables go only to the URL's server: saved data sources never get them (a saved PostgreSQL source without a saved password uses its own `~/.pgpass` line). `~/.pgpass` is read only when no one else can read it (`chmod 600 ~/.pgpass`), as `psql` does; otherwise the status line says it was ignored and how to fix it. A password never appears in the data source's name, the status line, errors, history or the Production log (`postgres://app:***@…`). A special character in a user or password is %-encoded in the URL (`@` is `%40`, `:` is `%3A`); an IPv6 host goes in brackets (`postgres://app@[::1]/shop`).
- **TLS** follows the same rules as a saved data source. `sslmode=require`, `verify-ca`, `verify-full` (`require` with `sslrootcert` checks the certificate against it, as libpq does: `verify-ca`) (MySQL `ssl-mode=REQUIRED`, `VERIFY_CA`, `VERIFY_IDENTITY`) turn SSL on with that mode. Certificate files are never skipped: `sslrootcert` (MySQL `ssl-ca`) without a mode checks the server's certificate against it (`verify-ca`, as `mysql --ssl-ca` does; MySQL `REQUIRED` with `ssl-ca` too), and `sslcert` / `sslkey` alone turn TLS on (`require`); with `disable`, `allow` or `prefer` (MySQL `DISABLED`, `PREFERRED`), which wouldn't use them, the URL is refused with the mode to use. Without a mode or files (or with `prefer`), a server on this computer or behind `--ssh` connects as it is, and a remote PostgreSQL or MySQL must still set up TLS: Qurry doesn't fall back to plain text the way `psql`'s default `prefer` does. `sslmode=disable` (MySQL `ssl-mode=DISABLED`) is your explicit "Allow unencrypted connection" for this one connection; the status line then says `Not encrypted`. `rediss://` always uses TLS and verifies the certificate (`?sslmode=require` only encrypts); `redis://` with TLS settings is refused with a pointer to `rediss://`.
- **`--read-only`** refuses statements that change data exactly as a Read-only data source does (PostgreSQL sessions are read-only on the server too). The status bar shows `READ-ONLY`.
- **`--incident`** opens Database Admin › Sessions on the data source (PostgreSQL, MySQL), reloading every 3 seconds: `c` cancels a query, `K` ends a session.
- **`--ssh DEST`** goes through an SSH tunnel: `DEST` is `[user@]host[:port]` or a `Host` from `~/.ssh/config`, whose `HostName`, `User`, `Port`, `IdentityFile`, `IdentityAgent` and `ProxyJump` apply. It logs in with ssh-agent, then the `IdentityFile` keys (or `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`); the database host in the URL is as seen from the SSH server. An unknown host key stops with its fingerprint, as for a saved data source: check it, then `⌃k › Trust SSH Host Key` and expand the data source again. See [SSH tunnel](data-sources.md#supported-databases) for what is read from `~/.ssh/config`.
- **Mistakes** stop before the screen opens, with exit code 2 and what is wrong: an unknown scheme (`mongodb://` isn't one of them), an unknown URL parameter, `--read-only` without a URL, something that looks like a mistyped URL (`postgres:/…`, `user:pw@host`: not echoed, it may hold a password), or a misspelled option (`--readonly` is refused, so it never opens a writable connection by mistake). `qurry-tui --help` lists the forms.

## Status line

The bottom line shows the status text on the left (the Production and Read-only badges first), then the keys of the screen and the common keys, and for a focused result or table grid the GUI status bar's items: `… ⌃k actions   Not null: 2 │ Sum: 1,329,000 │ cols 1–4 of 9 │ 2 rows · Col 3`, Position in the corner. **⌃k › Status Bar Items…** chooses them (Position and Shown columns by default; Not null, Sum, Avg, Min, Max, Size), the same setting as the GUI's ([Settings › Status bar items](settings.md#status-bar-items)): `↑` `↓` move, `Space` or `Enter` toggles and the list stays open, **Restore defaults** goes back.

The selection the items read is the marked rows (`m`) in the cursor's column, or a range selected with `⇧↑↓←→` (or dragged); with neither, Position is the cursor's cell and Size is about that cell. A table tab knows its columns' declared limits; a console result only where its column is a table's column.

When the line is too narrow, the screen's keys go first, then the common keys but `⌃k actions`, then the items (the column limit, Max, Min, Avg, Sum, Not null, Size, Shown columns), then `⌃k actions`; Position stays. The status text keeps up to half the line, as without items; only when an item would otherwise go does it give way down to a third (cut with `…`). A run's or a transfer's line is never cut to make room for items.

## Colors

The TUI picks one color mode when it starts, from these environment variables (the first that applies wins):

| Environment | What you get |
|---|---|
| `NO_COLOR` set to anything (see [no-color.org](https://no-color.org)) | No colors at all. Selections (also of text in the editor and inputs), badges (`PROD`, `READ-ONLY`) and the cell cursor are shown in reverse video, muted text is dim, the focused panel keeps its thick border |
| `COLORTERM=truecolor` or `24bit`, a `TERM` ending in `-direct`, or the `TERM` of kitty, Alacritty, WezTerm, foot or Ghostty | The full 24-bit theme, the same colors as the GUI |
| `TERM` containing `256color` (e.g. `xterm-256color`, `tmux-256color`) | Each color mapped to the nearest one in the xterm 256-color palette. Tints keep their hue: a production data source stays red |
| Anything else (e.g. `TERM=xterm`, `screen`) | The 16 terminal colors. Selections are in reverse video and text on a colored row is black, or white on red, blue and magenta, so it reads with any terminal theme |

When colors are off or limited to 16, the status line says so at startup, with what to change:

- `Colors off: unset NO_COLOR` — run `unset NO_COLOR` (or remove it from your shell profile) and start `qurry-tui` again.
- `16 colors: try COLORTERM=truecolor` — if your terminal does show 24-bit colors, `export COLORTERM=truecolor` before starting. Most modern terminals (iTerm2, kitty, WezTerm, Ghostty, Alacritty, Windows Terminal) set it themselves; it often gets lost over `ssh`, `sudo` or inside `tmux` / `screen`.

Over `ssh` or `sudo` (and in `tmux`, below) `COLORTERM` usually doesn't arrive, so a 24-bit terminal gets the 256 or 16 colors: `export COLORTERM=truecolor` on the other side, or pass it along with `SendEnv COLORTERM` in `~/.ssh/config` and `AcceptEnv COLORTERM` in the server's `sshd_config` (`sudo --preserve-env=COLORTERM` for sudo).

### Themes

The TUI draws the GUI's themes: the seven built-in ones (Dark, Light, High contrast, Dracula, One Dark, Solarized Dark, Solarized Light) and the custom ones made in the GUI's **Settings › Appearance › Theme › Edit Colors…** ([Settings › Themes](settings.md#themes)). Pick one with **⌃k › Theme…**; the pick is saved in `settings.json` in the [config folder](getting-started.md#where-qurry-keeps-your-settings) for both apps, and the list reads the file again when it opens, so a theme saved in the GUI shows up there.

| Theme | Colors |
|---|---|
| **Sync with OS** (default) | The dark or the light theme picked for Sync with OS in the GUI (Dark and Light unless changed): the light one when `COLORFGBG` says the background is white (its last number is `7` or `15`, e.g. `COLORFGBG=0;15`), the dark one otherwise, also when `COLORFGBG` isn't set |
| **Dark**, **Light**, **High contrast** | The theme's colors on the terminal's own background and text, as the TUI's dark and light palettes did before |
| **Dracula**, **One Dark**, **Solarized Dark**, **Solarized Light**, custom themes | The theme's colors on its own background and text (in truecolor and 256 colors) |

In truecolor a Data Source colored **Red** (Production) or **Green** takes the theme's Production or Green tint, as in the GUI (custom themes' too; the active tab a stronger one), NULL the theme's NULL color, and a range in the grid the selection's color halfway to the background; 256 and 16 colors keep the Data Source color's own hue, which their palettes can show. A light theme takes the GUI's light colors: the other Data Source colors become pale tints of the same hue, so icons and dim text on them stay readable (the active tab is the stronger one; 256 and 16 colors, which have no pale hues, keep a stronger tint so the hue survives), selections, edited cells and badges stay apart from each other, JSON cells use the theme's JSON colors, and production keeps its red. Every theme works in every color mode above: 256 colors map it to the nearest palette colors (a status line or header never lands on the background's color), 16 colors keep the terminal's own background and use the normal (not the bright) terminal colors in a light theme, and `NO_COLOR` stays monochrome with the same rules in every theme. The tree's object icons and Redis key types keep the TUI's own colors.

Terminals such as iTerm2, rxvt and Konsole set `COLORFGBG` from their background color; many others (kitty, WezTerm, Ghostty, Alacritty) don't, so on a light background there pick a light theme yourself. Up to 0.3.1 the TUI saved its pick as `"tuiTheme"` (Auto, Dark or Light): it is used until a theme is picked in either app (Auto is Sync with OS).

When Sync with OS picked the light theme, the status line says so at startup: `Light palette — ⌃k › Theme… › Dark to undo`. To keep the dark colors, pick **⌃k › Theme… › Dark**. It takes the place of the `16 colors` notice; with `NO_COLOR` the theme makes no difference, so the `Colors off` notice shows instead.

Some things never depend on color:

- **Unsaved grid changes**: next to the row number `+` marks an added row, `-` a deleted one (also struck through), `~` a row with edited cells; edited cells are bold.
- **The cell cursor inside a range**: with reverse video (16 colors, `NO_COLOR`) the range is reversed and the cursor is the one cell in it left plain, as in a spreadsheet.
- **The active tab**: bold. With 16 colors or `NO_COLOR` the focused pane's tab is in reverse video (its Data Source color inverted, so a production tab stays red) and, in split panes, the other panes' visible tab is underlined.
- **Statements that ran in a console**: `✓` done, `✗` failed, `…` running, in the line-number column on the statement's first line.
- **A failed statement's error**: underlined where the server points (the whole statement when it doesn't say); the status line says where. An edit or the next run clears it.

`qurry-tui --help` lists the same rules.

## Fonts

The terminal draws the TUI's text, so its font and size are the terminal's: Qurry has no font setting there (the GUI's are in [Settings › Appearance](settings.md#appearance)). Pick a monospace font; the TUI lays out text in cells and draws borders with box-drawing characters. In most terminals `⌘+` / `⌘-` change the size of the window you are in.

| Terminal | Where to set the font |
|---|---|
| Terminal.app | *Settings › Profiles › Text › Font › Change…* (per profile) |
| iTerm2 | *Settings › Profiles › Text › Font* (per profile) |
| Ghostty | `font-family = "JetBrains Mono"` and `font-size = 13` in `~/.config/ghostty/config`; reload with `⇧⌘,` |
| WezTerm | `config.font = wezterm.font("JetBrains Mono")` and `config.font_size = 13.0` in `~/.wezterm.lua`; applied when you save |
| Alacritty | `[font]` with `normal = { family = "JetBrains Mono" }` and `size = 13.0` in `~/.config/alacritty/alacritty.toml`; applied when you save |

## Small windows

The layout is drawn down to **40 × 10** cells. 80 × 24 and larger is comfortable; in a small window, hide the explorer with `⌥0` or narrow it with `⌥[`.
The value panel (`v`) needs a result area at least 60 cells wide; in a narrower one the grid keeps the whole width until the window is larger.
Dialogs stay usable down to that size: when the Data Source form doesn't fit, its fields scroll to the one you are on (the bottom border shows which, e.g. `12/28`) and the key hints stay at the bottom (a long test message above them is cut and ends in `⋯`); a confirm takes the whole height and keeps its first line, its question, every `⚠` warning (such as `⚠ Production data source`) and `y / Enter` · `n / Esc`; the details in between (statements, columns, tables) give way to a line that says how many are hidden and how many rows those statements would change (`⋯ 4 lines hidden · 12,409 rows`), so make the window larger to read them before you answer. If even the question and the warnings don't fit, the keys line says `Enlarge the window to answer` and `y` / `Enter` do nothing until they fit (`n` / `Esc` still cancel).

Below 40 × 10 the TUI shows `Terminal too small` with the current size and the size it needs, instead of a broken screen. Nothing is lost: make the window larger and everything is back where it was. Meanwhile keys and clicks do nothing, so nothing unseen is run or answered; only `⌃q` quits, and only when nothing waits for an answer. With a dialog open the message says `A dialog is open: make the window larger to answer it`, and with uncommitted changes in a console `N console(s) have uncommitted changes: make the window larger to quit`; make the window larger, then answer the dialog, or press `⌃q` and choose whether to roll the changes back.

## tmux and other multiplexers

- **`⌃b` (Go to DDL) is tmux's default prefix.** Qurry keeps the key, like DataGrip. In tmux, press `⌃b` twice: tmux sends one `⌃b` through to Qurry. Or use `⌃k` › **Go to DDL**, or give it another key in the F1 window (select **Go to DDL**, press Enter, then the new key).
- **`⌥` keys** (`⌥1…9` workspaces, `⌥0` explorer, `⌥Enter` run, split keys) need the terminal to send Option as Meta: in macOS Terminal turn on *Settings › Profiles › Keyboard › Use Option as Meta key*; in iTerm2 set *Profiles › Keys › Left Option key* to *Esc+*. In tmux, `set -s escape-time 10` in `~/.tmux.conf` keeps `Esc` and `⌥` keys from lagging or being read as two keys.
- **`⌃Enter`** needs the kitty keyboard protocol, which tmux and screen don't pass on: use `F5` or `⌥Enter` there.
- **24-bit colors inside tmux**: tmux must know the outer terminal has them (`set -as terminal-features ",xterm-256color:RGB"` for tmux 3.2 and later) and `COLORTERM=truecolor` must reach the shell inside tmux. Without it Qurry uses the 256-color palette, which looks almost the same.

See [TUI keyboard shortcuts](shortcuts-tui.md) for every key.
