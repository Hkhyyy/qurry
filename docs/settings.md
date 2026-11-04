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

**Font family**: pick it from the list of fonts installed on this computer. **Monospaced only** (on by default) hides proportional fonts; turn it off to pick any installed font. Type to search the list (typing on the closed field opens it), `↑` `↓` to move, `↩` to pick, `⎋` to close the list. The field shows the chosen font in itself. **Default monospace**, at the top of the list, is JetBrains Mono, or Menlo when it isn't installed. The list is read again each time it opens, so a font you install while Qurry is running shows up without a restart. On macOS and Linux (`fc-list`) it lists the installed fonts; elsewhere it offers only the default and the saved font.

A saved font that isn't installed (removed since, or chosen on another computer) never breaks the screen: Qurry falls back to the default monospace font, and the field and the top of the list show it as `<name> (not installed)`. Pick another font to replace it.

**Add Font File…** (at the bottom of the list; not in DataGrip) uses a font without installing it on the computer: pick a `.ttf`, `.otf`, `.woff` or `.woff2` file of up to 20 MB, and Qurry copies it into the `fonts` folder inside the config folder and picks it for that field. The copy is named after the file (letters, digits, `-` and `_` kept, anything else becomes a space); if a font of that name was already added, the new one gets ` 2`, ` 3`… instead of replacing it. A file that isn't really a font (Qurry checks its first bytes, not its extension), or one that can't be loaded as a font (damaged or incomplete), is refused with the reason. Added fonts are listed in both fields right after **Default monospace**, marked `custom`, whether or not **Monospaced only** is on; one with the name of an installed font replaces it in the list. The `−` button on its row removes it (after asking): the copy is deleted, the file you added it from is kept, and a field set to it goes back to **Default monospace** (unless a font of that name is installed: it then shows in the installed one). Added fonts are loaded each time Qurry starts. The TUI uses the terminal's font, so added fonts don't apply there.

The theme and fonts are kept by the GUI on this computer (not in the config folder; only added font files are copied into its `fonts` folder), so they don't affect the TUI.

### Alignment in result grids

Each cell is aligned by the kind of value it holds. Pick **Left**, **Center** or **Right** for each kind (for NULL also **Column type**):

| Kind | Which values | Default |
|---|---|---|
| String | text, `uuid`, enums, arrays, ranges, and types Qurry doesn't know | Left |
| Number | integer, `numeric` / `decimal`, floating point (also Oracle `BINARY_FLOAT` / `BINARY_DOUBLE`), `money`, `oid` (also when the driver sends them as text) | Right |
| Date / time | `date`, `time`, `timestamp`, `interval`, `datetime` | Left |
| Boolean | `bool` / `boolean` | Left |
| NULL | `<null>` (and `<default>` in a row you are adding) | Column type |
| JSON / binary | `json` / `jsonb`, `bytea`, `blob`, `binary` / `varbinary` | Left |

With **Column type** (the default) a `NULL` sits where its column's values do: on the left in a text, date / time, boolean or JSON column, on the right in a numeric column, so it lines up with the values above and below it. A column without a type (a SQLite expression such as `sum(x)`, MongoDB) goes by its first non-`NULL` value. If you changed that kind's alignment, the `NULL` follows it too. **Left** or **Right** puts every `NULL` there whatever the column. In 0.3.0 every `NULL` was on the right: to keep that, set **NULL** to **Right**; before 0.3.0 it was on the left (**Left**).

JSON cells (`json`, `jsonb` and the other drivers' JSON types) are colored like DataGrip's: keys, strings, numbers, `true` / `false` / `null` and punctuation, still on one line. Binary cells stay plain. The TUI colors them the same way in the terminal's colors, and not at all with `NO_COLOR`.

The TUI result grid uses the same kinds and the same setting: change it there with **⌃k › Grid Alignment…** (pick a kind, then Left, Center or Right; NULL also offers Column type). The TUI reads `settings.json` when it starts and again when you open **⌃k › Grid Alignment…**, so after changing the alignment in the GUI, open that command (or restart the TUI) to see it. In `settings.json` only the kinds you changed are written, for example `"gridAlign": { "null": "right" }`; a kind you set back to its default is removed. A `"null": "left"` (or `"center"`) saved by 0.3.0 keeps working. Right was the 0.3.0 default, so it was never saved: if you had Right, pick **Right** again.

## Keymap

Every GUI shortcut, and the keys you changed: [GUI keyboard shortcuts](shortcuts-gui.md). Changed shortcuts are kept in `keymap.json`. The TUI's keys are changed in its `F1` window.

## Passwords

Where the key for saved passwords is kept (config folder or system keychain): [Getting started › Where Qurry keeps your settings](getting-started.md#where-qurry-keeps-your-settings).

---

[← User guide](README.md)
