# TUI in your terminal

English | [한국어](ko/tui.md)

`qurry-tui` adapts to the terminal it runs in: how many colors it has, how big the window is, and what tmux or the terminal keeps for itself. Nothing here needs a setting in Qurry; it all follows the environment.

## Colors

The TUI picks one color mode when it starts, from these environment variables (the first that applies wins):

| Environment | What you get |
|---|---|
| `NO_COLOR` set to anything (see [no-color.org](https://no-color.org)) | No colors at all. Selections, badges (`PROD`, `READ-ONLY`) and the cell cursor are shown in reverse video, muted text is dim, the focused panel keeps its thick border |
| `COLORTERM=truecolor` or `24bit`, or a `TERM` ending in `-direct` | The full 24-bit theme, the same colors as the GUI |
| `TERM` containing `256color` (e.g. `xterm-256color`, `tmux-256color`) | Each color mapped to the nearest one in the xterm 256-color palette. Tints keep their hue: a production data source stays red |
| Anything else (e.g. `TERM=xterm`, `screen`) | The 16 terminal colors. Selections are in reverse video and text on a colored row is dark, so it reads with any terminal theme |

When colors are off or limited to 16, the status line says so at startup, with what to change:

- `NO_COLOR is set: colors off (unset NO_COLOR for colors)` — run `unset NO_COLOR` (or remove it from your shell profile) and start `qurry-tui` again.
- `16 colors: set COLORTERM=truecolor for more if supported` — if your terminal does show 24-bit colors, `export COLORTERM=truecolor` before starting. Most modern terminals (iTerm2, kitty, WezTerm, Ghostty, Alacritty, Windows Terminal) set it themselves; it often gets lost over `ssh`, `sudo` or inside `tmux` / `screen`.

`qurry-tui --help` lists the same rules.

## Small windows

The layout is drawn down to **40 × 10** cells. 80 × 24 and larger is comfortable; in a small window, hide the explorer with `⌥0` or narrow it with `⌥[`.
Below 40 × 10 the TUI shows `Terminal too small` with the current size and the size it needs, instead of a broken screen. Nothing is lost: make the window larger and everything is back where it was. `⌃q` still quits.

## tmux and other multiplexers

- **`⌃b` (Go to DDL) is tmux's default prefix.** Qurry keeps the key, like DataGrip. In tmux, press `⌃b` twice: tmux sends one `⌃b` through to Qurry. Or use `⌃k` › **Go to DDL**, or give it another key in the F1 window (select **Go to DDL**, press Enter, then the new key).
- **`⌥` keys** (`⌥1…9` workspaces, `⌥0` explorer, `⌥Enter` run, split keys) need the terminal to send Option as Meta: in macOS Terminal turn on *Settings › Profiles › Keyboard › Use Option as Meta key*; in iTerm2 set *Profiles › Keys › Left Option key* to *Esc+*. In tmux, `set -s escape-time 10` in `~/.tmux.conf` keeps `Esc` and `⌥` keys from lagging or being read as two keys.
- **`⌃Enter`** needs the kitty keyboard protocol, which tmux and screen don't pass on: use `F5` or `⌥Enter` there.
- **24-bit colors inside tmux**: tmux must know the outer terminal has them (`set -as terminal-features ",xterm-256color:RGB"` for tmux 3.2 and later) and `COLORTERM=truecolor` must reach the shell inside tmux. Without it Qurry uses the 256-color palette, which looks almost the same.

See [TUI keyboard shortcuts](shortcuts-tui.md) for every key.
