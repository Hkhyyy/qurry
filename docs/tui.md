# TUI in your terminal

English | [한국어](ko/tui.md)

`qurry-tui` adapts to the terminal it runs in: how many colors it has, how big the window is, and what tmux or the terminal keeps for itself. Nothing here needs a setting in Qurry; it all follows the environment, down to the font.

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

Some things never depend on color:

- **Unsaved grid changes**: next to the row number `+` marks an added row, `-` a deleted one (also struck through), `~` a row with edited cells; edited cells are bold.
- **The cell cursor inside a range**: with reverse video (16 colors, `NO_COLOR`) the range is reversed and the cursor is the one cell in it left plain, as in a spreadsheet.
- **The active tab**: bold. With 16 colors or `NO_COLOR` the focused pane's tab is in reverse video (its Data Source color inverted, so a production tab stays red) and, in split panes, the other panes' visible tab is underlined.

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
