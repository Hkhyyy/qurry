# Settings

English | [한국어](ko/settings.md)

GUI **Settings** (`⌘,`, or the ⚙ button at the bottom of the workspace rail on the far left) has three pages: **Appearance**, **Keymap** and **Passwords**. Settings that the GUI and the TUI share are kept in `settings.json` in the config folder ([where that is](getting-started.md#where-qurry-keeps-your-settings)). The TUI has no settings window: its options are in `⌃k` (Find Action), and its font is the terminal's (see [TUI in your terminal › Fonts](tui.md#fonts)).

## Appearance

| Setting | What it does | Shared with the TUI |
|---|---|---|
| Theme | Dark, Light, High contrast, Dracula, One Dark, Solarized Dark, Solarized Light or a custom theme, or **Sync with OS** with a dark and a light theme of your choice ([below](#themes)). Quick switch: `` ⌃` `` | Yes (TUI **⌃k › Theme…**) |
| Editor font | Font family and size of the query console editor. Size 8–32 px (default 13); `⌃⇧.` / `⌃⇧,` change it too, and Settings shows the new size | No (the TUI uses the terminal's font) |
| Grid font | Font family and size of the result and table grids. Size 9–16 px (default 12): rows keep their height, so larger text would be cut. Columns get wider with the size | No (the TUI uses the terminal's font) |
| Keyword case | **Off** (default), **UPPER** or **lower** for SQL keywords as you type in a console and in **Reformat Code** ([below](#keyword-case-and-reformat-code)) | Yes (TUI **⌃k › Keyword Case…**) |
| Alignment | Where cells sit in result grids, per kind of value (below) | Yes (TUI **⌃k › Grid Alignment…**) |
| Status bar | Which items the status bar shows for the focused grid: Position, Shown columns, Not null, Sum, Avg, Min, Max, Size ([below](#status-bar-items)) | Yes (TUI **⌃k › Status Bar Items…**) |
| Updates | Check for a new version when the app starts; **Check Now** | — |
| Channel | Stable or beta releases ([Updates](updates.md)) | Yes |

### Fonts

Font changes apply at once, to every open console and grid.

**Font family**: pick it from the list of fonts installed on this computer. **Monospaced only** (on by default) hides proportional fonts; turn it off to pick any installed font. Type to search the list (typing on the closed field opens it), `↑` `↓` to move, `↩` to pick, `⎋` to close the list. The field shows the chosen font in itself. **Default monospace**, at the top of the list, is JetBrains Mono, or Menlo when it isn't installed. The list is read again each time it opens, so a font you install while Qurry is running shows up without a restart. On macOS and Linux (`fc-list`) it lists the installed fonts; elsewhere it offers only the default and the saved font.

A saved font that isn't installed (removed since, or chosen on another computer) never breaks the screen: Qurry falls back to the default monospace font, and the field and the top of the list show it as `<name> (not installed)`. Pick another font to replace it.

**Add Font File…** (at the bottom of the list; not in DataGrip) uses a font without installing it on the computer: pick a `.ttf`, `.otf`, `.woff` or `.woff2` file of up to 20 MB, and Qurry copies it into the `fonts` folder inside the config folder and picks it for that field. The copy is named after the file (letters, digits, `-` and `_` kept, anything else becomes a space); if a font of that name was already added, the new one gets ` 2`, ` 3`… instead of replacing it. A file that isn't really a font (Qurry checks its first bytes, not its extension), or one that can't be loaded as a font (damaged or incomplete), is refused with the reason. Added fonts are listed in both fields right after **Default monospace**, marked `custom`, whether or not **Monospaced only** is on; one with the name of an installed font replaces it in the list. The `−` button on its row removes it (after asking): the copy is deleted, the file you added it from is kept, and a field set to it goes back to **Default monospace** (unless a font of that name is installed: it then shows in the installed one). Added fonts are loaded each time Qurry starts. The TUI uses the terminal's font, so added fonts don't apply there.

The fonts are kept by the GUI on this computer (not in the config folder; only added font files are copied into its `fonts` folder), so they don't affect the TUI.

### Themes

Seven themes are built in: **Dark**, **Light** and **High contrast**, and **Dracula**, **One Dark**, **Solarized Dark** and **Solarized Light**. Each one colors everything: panels, the editor (SQL, and JSON in the Value Editor), result grids with their edited, added and deleted rows, find matches and NULLs, the Data Source tints and the Production stripe, and the TUI. Every built-in theme keeps text at least 4.5:1, and dim text, NULLs, icons and the accent at least 3:1 (WCAG contrast), on each surface they are drawn on: the editor and its current line, panels, odd grid rows, hover, the selection and the Production tint. Where a published palette doesn't (Solarized's text and dim text, Dracula's and One Dark's dim text, One Dark's comments, some status colors), the color is made a little lighter or darker in the same hue.

Pick one in **Settings › Appearance › Theme**, with **Quick Switch Theme** (`` ⌃` ``, which also lists custom themes) or in Find Action (`Theme: …`). With **Sync with OS** on, two more fields pick the theme for a dark system appearance and the one for a light appearance (any dark or high-contrast theme, and any light one, custom ones too); Qurry switches when the system does. Until a theme is picked in either app, the GUI draws Dark and the TUI Sync with OS (a terminal can be light or dark); a pick in either one is the pick of both.

The theme is saved in `settings.json` in the config folder (`"theme"`, `"themeSync"`, `"customThemes"`), and the TUI uses the same one (**⌃k › Theme…**; there Sync with OS goes by the terminal's background, see [TUI › Themes](tui.md#themes)). Up to 0.3.1 the GUI kept its theme inside the app: the first start of 0.3.2 moves it to `settings.json`, unless the TUI had saved a theme there already, which then wins. If the move fails, the status bar says so and the theme is used for that run; pick it again in Settings to keep it.

**Edit Colors…** makes a custom theme. Start from any theme in **Based on** (a built-in one, or a custom one to change it). The basic colors come first (background, panels, text, dim text, accent, selection, the syntax colors and the Production tint); **Advanced colors** has the rest (row numbers, borders, icons, the grid states, the Green tint, the Production stripe, status colors). The Production tint and stripe are what a Data Source colored **Red** gets, the Green tint one colored **Green**; the other Data Source colors are mixed into the theme's panels. Each color has a picker and its `#rrggbb` hex (a hex being typed applies once it is a color; one that isn't is marked and not applied), and ↺ goes back to the base's color. While you edit, the whole window draws the new colors, and the preview shows the explorer, tabs, a console, a result grid in every state, a JSON value and notices; point at a color's name to outline where it is used. A color below the contrast minimum on any surface it is drawn on is warned about next to it, naming the surface furthest below (`⚠ 2.1:1 on Background`), and in the header, and saved as you set it if you save: nothing is changed for you. **Save as new theme** adds it under the name you give and switches to it (with Sync with OS on, that turns it off, as the message says); **Save** replaces the custom theme you started from (Sync with OS stays on); **Delete** removes it (a pick of it, also in Sync with OS, goes back to its base). Leaving with unsaved colors asks first, and so do changing **Based on** and **Import…**, which replace them.

**Export…** writes the theme as a small JSON file, **Import…** reads one into the editor (check it, then save it):

```json
{ "qurryTheme": 1, "name": "Night shift", "base": "dracula", "colors": { "bg": "#1d1e26", "accent": "#9580ff" } }
```

`base` is a built-in theme and `colors` only the colors changed from it, by these names (in the editor's order: `bg`, `panel`, `panel2`, `odd`, `hover`, `border`, `borderStrong`, `text`, `muted`, `nullText`, `icon`, `accent`, `link`, `onAccent`, `sel`, `keyword`, `string`, `number`, `comment`, `function`, `jsonKey`, `gridNum`, `changed`, `added`, `deleted`, `match`, `prodTint`, `prodStripe`, `tint`, `ok`, `warn`, `danger`). A file is refused, with the reason, when it is larger than 16 KB, isn't this JSON, has another key, an unknown color name or base, or a color that isn't `#rrggbb`.

In 0.3.2 Dark, Light and High contrast changed slightly where they missed the contrast minimum: text on the accent (primary buttons; High contrast's is now black), the Production stripe, Dark's dim text and NULL gray, Light's accent and link blue, its NULL gray, its Production and Green tints, and Light's error red.

### Keyword case and Reformat Code

**Reformat Code** (GUI `⌘⌥l`, TUI `⌥⇧F` or **⌃k › Reformat Code**) lays out the selection, or without a selection the statement at the caret: one clause per line, its items indented by 4 spaces. Strings, comments and quoted names stay as written. One undo (`⌘z`, TUI `⌃u`) puts the text back as it was. The GUI and the TUI use the same formatter, so both lay out a statement the same way. Redis, MongoDB and vector database consoles aren't reformatted.

**Keyword case** sets the letter case of SQL keywords (`SELECT`, `FROM`, `WHERE`, `JOIN`, `AND`, `IN` …):

- **Off** (the default): what you type stays as you type it, and Reformat Code writes keywords in upper case, as it did before 0.3.2.
- **UPPER** or **lower**: a keyword you finish with a space, a new line or punctuation (`,` `(` `;` …) changes to that case as you type, and Reformat Code writes keywords in it. A keyword inside a string, a comment or a quoted name (`"order"`, `` `order` ``) is left alone, and so is a name after or before a dot (`t.order`, `order.id`). Words tables often use as column names (`user`, `name`, `type`, `key`, `value` …) and function names (`count`, `coalesce` …) are never changed. Typing changes only keywords that are rarely names; Reformat Code, which you run on a whole statement, also changes the rest, such as `FUNCTION`, `RETURNS`, `ROWS`, `UNBOUNDED` and `NATURAL`. In the TUI, `⌃u` first undoes the case change and then the character you typed.

The setting is saved in `settings.json` as `"keywordCase"` (`"off"`, `"upper"` or `"lower"`). The TUI reads it when it starts and again when you open **⌃k › Keyword Case…**, so after changing it in the GUI, open that command (or restart the TUI).

If your TUI `keymap.json` already gives `⌥⇧F` to another command, that command keeps it and Reformat Code has no key: the status line says so when the TUI starts. Give Reformat Code a key in the `F1` window (or run it from `⌃k`).

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

### Column widths in result grids

A column starts as wide as its name and its first values need (up to a limit), so a long value ends in `…`. To change that:

- **Drag** the right edge of a column header. **Double-click** the edge to fit the column to its content: its name and the longest value among the rows loaded so far. Double-click a fitted column's edge again to put back the width it had before (TUI: `=` again).
- **Right-click** a column header: **Fit Column**, **Fit All Columns**, **Reset Widths** (every column back to its computed width), **Narrow Column**, **Widen Column**.
- With a cell or a column header selected, `⌃⌥←` / `⌃⌥→` narrow / widen the current column by 4 characters (change the keys in **Settings › Keymap**, *Data grid*).

In the TUI, `<` / `>` narrow / widen the current column and `=` fits it (the key bar shows them; change them in the `F1` window, *Data grid keys*); **⌃k › Fit All Columns** and **⌃k › Reset Column Widths** do every column.

A table tab remembers its widths per table (data source · database · schema · table), in `settings.json` under `"columnWidths"`, so the GUI and the TUI show the same widths: in characters, which the GUI draws in its grid font and the TUI as that many cells. A wide character (Hangul, Chinese, Japanese, emoji) counts as 2, as a terminal draws it, so a fitted column shows the whole value in both apps. A table tab reads the saved widths each time it opens, so a change made in the other app shows when you open the table again. A column you never resized keeps its computed width, and **Reset Widths** forgets the table's widths. If the settings file can't be written, the new widths still apply until the tab closes. A console result keeps its widths until it runs again; columns with the same name (a join's two `id` columns) resize on their own. The transposed view (**View as › Transpose**) keeps its own layout.

### Status bar items

The status bar shows, on the right, items for the focused result or table grid, each as its own segment. **Position** and **Shown columns** are on by default; turn the others on where you need them:

| Item | What it shows |
|---|---|
| Position | The current cell (`Row 3, Col 4`), or the selected rows and columns: `Rows 5–26 (22) · Col 4`, `Rows 3–42 (40) · Cols 5–7`, `22 rows · Col 3` when the rows aren't one range (⌘-clicked) |
| Shown columns | The columns in view when not every one fits (`cols 3–8 of 14`) |
| Not null | The selected cells that have a value (NULL left out) |
| Sum, Avg, Min, Max | Over the numbers among the selected cells (2 or more cells) |
| Size | One cell: its size in bytes and characters (`Size: 11 bytes (7 chars)`; binary: its length) and, when the table's metadata gives it, its column's declared limit: `varchar(8): up to 8 chars (32 bytes in UTF-8)`, MySQL `up to 8 chars (32 bytes in utf8mb4)`, `char(8): always 8 chars (padded)`, `varbinary(16): up to 16 bytes`, `blob: up to 65,535 bytes`, `bytea: no limit (up to 1 GB)`. Not shown for numbers and dates / times |

Choose them in **Settings › Appearance › Status bar**, or **right-click the status bar** (or click one of its items) for the same checklist: it stays open while you toggle items (`↑` `↓` move, `Space` or `↩` toggles, `⎋` closes); **Restore Defaults** goes back to Position and Shown columns. The choice is kept in `settings.json` as `"statusBarItems"` (the items that are on) and shared with the TUI, where **⌃k › Status Bar Items…** changes it ([TUI status line](tui.md#status-line)). Each app reads it when it starts and when you open its checklist, so a change made in the other app shows there.

Size counts the bytes of the value as Qurry sends it, in UTF-8. A column in another character set (MySQL `latin1`) can store it in fewer bytes; the limit is worded in the column's own character set. A table tab knows its columns' limits; a console result only where its column is a table's column (a result of one table with its key, or a join's column of a table), otherwise Size shows the bytes only. A binary value longer than the 64 bytes a grid receives shows as `over 64 bytes`.

When the window is narrow, the status text gives way first (down to a short width), then UTF-8, the column limit, Max, Min, Avg, Sum, Not null, Size and Shown columns go, in that order; Position and Read-only always stay.

## Keymap

Every GUI shortcut, and the keys you changed: [GUI keyboard shortcuts](shortcuts-gui.md). Changed shortcuts are kept in `keymap.json`. The TUI's keys are changed in its `F1` window.

## Passwords

Where the key for saved passwords is kept (config folder or system keychain): [Getting started › Where Qurry keeps your settings](getting-started.md#where-qurry-keeps-your-settings).

---

[← User guide](README.md)
