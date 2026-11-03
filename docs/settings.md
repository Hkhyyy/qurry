# Settings

English | [한국어](ko/settings.md)

GUI **Settings** (`⌘,`) has three pages: **Appearance**, **Keymap** and **Passwords**. Settings that the GUI and the TUI share are kept in `settings.json` in the config folder ([where that is](getting-started.md#where-qurry-keeps-your-settings)). The TUI has no settings window: its options are in `⌃k` (Find Action), and its font is the terminal's (see [TUI in your terminal › Fonts](tui.md#fonts)).

## Appearance

| Setting | What it does | Shared with the TUI |
|---|---|---|
| Theme | Dark, Light or High contrast, or **Sync with OS**. Quick switch: `` ⌃` `` | No (the TUI follows the terminal's colors) |
| Editor font | Font family and size of the query console editor. Size 8–32 px (default 13); `⌃⇧.` / `⌃⇧,` change it too, and Settings shows the new size | No (the TUI uses the terminal's font) |
| Grid font | Font family and size of the result and table grids. Size 9–16 px (default 12): rows keep their height, so larger text would be cut. Columns get wider with the size | No (the TUI uses the terminal's font) |
| Alignment | Where cells sit in result grids, per kind of value (below) | Yes (TUI **⌃k › Grid Alignment…**) |
| Updates | Check for a new version when the app starts; **Check Now** | — |
| Channel | Stable or beta releases ([Updates](updates.md)) | Yes |

### Fonts

Font changes apply at once, to every open console and grid.

**Font family**: the list suggests the monospace fonts installed on this computer (JetBrains Mono, Menlo, Fira Code, D2Coding, Cascadia Code and other common ones); any other installed font can be typed in. Each field shows its text in the chosen font. Leave the field empty for the default (JetBrains Mono, or Menlo when it isn't installed).

A font that isn't installed (a typo, or a font removed since) never breaks the screen: Qurry falls back to the default monospace font, and Settings says `"<name>" isn't installed, so the default monospace font is used.` Pick a font from the list, or clear the field to go back to the default.

The theme and fonts are kept by the GUI on this computer (not in the config folder), so they don't affect the TUI.

### Alignment in result grids

Each cell is aligned by the kind of value it holds. Pick **Left**, **Center** or **Right** for each kind:

| Kind | Which values | Default |
|---|---|---|
| String | text, `uuid`, enums, arrays, ranges, and types Qurry doesn't know | Left |
| Number | integer, `numeric` / `decimal`, floating point (also Oracle `BINARY_FLOAT` / `BINARY_DOUBLE`), `money`, `oid` (also when the driver sends them as text) | Right |
| Date / time | `date`, `time`, `timestamp`, `interval`, `datetime` | Left |
| Boolean | `bool` / `boolean` | Left |
| NULL | `<null>` (and `<default>` in a row you are adding) | Right |
| JSON / binary | `json` / `jsonb`, `bytea`, `blob`, `binary` / `varbinary` | Left |

`NULL` is on the right by default so that a `NULL` in a numeric column lines up with the numbers above and below it. Before 0.3.0 it was on the left: to get that back, set **NULL** to **Left**.

The TUI result grid uses the same kinds and the same setting: change it there with **⌃k › Grid Alignment…** (pick a kind, then Left, Center or Right). The TUI reads `settings.json` when it starts and again when you open **⌃k › Grid Alignment…**, so after changing the alignment in the GUI, open that command (or restart the TUI) to see it. In `settings.json` only the kinds you changed are written, for example `"gridAlign": { "null": "left" }`; a kind you set back to its default is removed.

## Keymap

Every GUI shortcut, and the keys you changed: [GUI keyboard shortcuts](shortcuts-gui.md). Changed shortcuts are kept in `keymap.json`. The TUI's keys are changed in its `F1` window.

## Passwords

Where the key for saved passwords is kept (config folder or system keychain): [Getting started › Where Qurry keeps your settings](getting-started.md#where-qurry-keeps-your-settings).

---

[← User guide](README.md)
