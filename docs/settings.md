# Settings

English | [한국어](ko/settings.md)

GUI **Settings** (`⌘,`) has three pages: **Appearance**, **Keymap** and **Passwords**. Settings that the GUI and the TUI share are kept in `settings.json` in the config folder ([where that is](getting-started.md#where-qurry-keeps-your-settings)).

## Appearance

| Setting | What it does | Shared with the TUI |
|---|---|---|
| Theme | Dark, Light or High contrast, or **Sync with OS**. Quick switch: `` ⌃` `` | No (the TUI follows the terminal's colors) |
| Alignment | Where cells sit in result grids, per kind of value (below) | Yes |
| Updates | Check for a new version when the app starts; **Check Now** | — |
| Channel | Stable or beta releases ([Updates](updates.md)) | Yes |

### Alignment in result grids

Each cell is aligned by the kind of value it holds. Pick **Left**, **Center** or **Right** for each kind:

| Kind | Which values | Default |
|---|---|---|
| String | text, `uuid`, enums, arrays, ranges, and types Qurry doesn't know | Left |
| Number | integer, `numeric` / `decimal`, floating point, `money`, `oid` (also when the driver sends them as text) | Right |
| Date / time | `date`, `time`, `timestamp`, `interval`, `datetime` | Left |
| Boolean | `bool` / `boolean` | Left |
| NULL | `<null>` (and `<default>` in a row you are adding) | Right |
| JSON / binary | `json` / `jsonb`, `bytea`, `blob`, `binary` / `varbinary` | Left |

`NULL` is on the right by default so that a `NULL` in a numeric column lines up with the numbers above and below it. Before 0.3.0 it was on the left: to get that back, set **NULL** to **Left**.

The TUI result grid uses the same kinds and the same setting. It reads `settings.json` when it starts, so restart the TUI after changing the alignment in the GUI. In `settings.json` only the kinds you changed are written, for example `"gridAlign": { "null": "left" }`; a kind you set back to its default is removed.

## Keymap

Change any shortcut; see [GUI keyboard shortcuts](shortcuts-gui.md). Changed shortcuts are kept in `keymap.json`.

## Passwords

Keep the key of the password file in the config folder or in the system keychain; see [Where Qurry keeps your settings](getting-started.md#where-qurry-keeps-your-settings).

---

[← User guide](README.md)
