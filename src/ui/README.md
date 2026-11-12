# UI rules

Screens are built only from the pieces in this folder (GUI) and in `crates/tui/src/kit` (TUI).
Screen code doesn't use bare HTML (`<button>`, `<input>`, `<span className="muted">` …), numeric px or literal colors.
To change how something looks, change the piece, not the screen code.

Preview: `pnpm storybook` → http://localhost:6006 — the GUI by level (1 Atoms · 2 Molecules · 3 Organisms · 4 Grid), with the theme switcher at the top. The TUI: screens that `pnpm tui-snapshots` draws with the real TUI code.

Pieces come in three levels (atomic design).

| Level | Meaning | Examples |
|---|---|---|
| Atoms | Can't be split further | Text · code · shortcut · button · input · badge · divider |
| Molecules | A few atoms in one row or one field | List row · tree row · detail row · menu item · form field · tab |
| Organisms | A block of molecules | Toolbar · dialog · confirm · menu · popover · list · table |

## Tokens (`tokens.css`)

Colors are theme tokens: every theme's values are in `crates/shared/src/themes.json` (the TUI draws from it too), and `lib/theme.ts` sets them on `<html>`. The values below are Dark's.

| Kind | Token | Value (dark) | Used for |
|---|---|---|---|
| Background | `--ui-bg` | #1e1f22 | Editor, tables, inputs |
| | `--ui-surface` | #2b2d30 | Panels, toolbars, dialogs, menus |
| | `--ui-surface-2` | #26282b | A surface one step lower |
| | `--ui-hover` · `--ui-selected` | #33353a · #2e436e | Hover · selected row |
| Lines | `--ui-border` · `--ui-border-strong` | #393b40 · #4e5157 | Sections · input borders |
| Text | `--ui-text` · `--ui-text-muted` · `--ui-text-on-accent` | #dfe1e5 · #8b8f95 · #fff | Body · descriptions · on the accent color |
| Colors | `--ui-accent` · `--ui-danger` · `--ui-warn` · `--ui-ok` | #3574f0 · #f75464 · #e5a64b · #5fb865 | Primary button · danger · warning · success |
| JSON | `--ui-json-key` · `-string` · `-number` · `-keyword` · `-punct` | #c77dbb · #6aab73 · #2aacb8 · #cf8e6d · #8b8f95 | JSON cells in result grids (the TUI uses the same values) |
| SQL | `--c-cf8e6d` · `--c-56a8f5` · `--c-number` · `--ui-json-string` · `--c-comment` | #cf8e6d · #56a8f5 · #2aacb8 · #6aab73 · #7a7e85 | Keyword · function · number · string · comment in `HighlightInput` (the table tab's WHERE / ORDER BY; the console's Monaco theme and the TUI use the same values) |
| Spacing | `--ui-sp-1` … `--ui-sp-6` | 2 · 4 · 8 · 12 · 16 · 24px | Padding, gap and margin use only these |
| Radius | `--ui-radius-sm` / `-md` / `-lg` | 4 · 6 · 10px | Buttons / menus · lists / dialogs |
| Font size | `--ui-fs-xs` / `-sm` / `-md` / `-lg` | 11 · 12 · 13 · 14px | Badges / secondary / body / titles |
| Height | `--ui-h-sm` / `-md` / `-lg` | 22 · 26 · 30px | Small buttons / buttons · inputs / toolbars |
| Shadow · stacking | `--ui-shadow-pop` / `-modal` · `--ui-z-modal` 100 · `-menu` 200 · `-popover` 300 · `-dialog` 400 | | A confirm is always on top |

The light and high-contrast themes give the same names other values.

## GUI atoms (`atoms.tsx`)

| Piece | Rules |
|---|---|
| `Text` | A piece of text. `tone` (muted · subtle · accent · ok · warn · danger), `size` (xs · sm · md · lg), `mono`, `strong`, `truncate` (… when it overflows, the whole text in title) |
| `Code` · `CodeBlock` · `Kbd` | Inline code / multi-line code (`spans` color it: SQL previews use components/SqlBlock) / a shortcut |
| `Row` · `Spacer` · `Divider` · `ToolbarSep` | Horizontal layout (gap 1–6) / the remaining width / a divider (horizontal · vertical) |
| `Button` | `variant`: default · primary (one per window, rightmost) · danger · ghost · link. `size`: md 26px · sm 22px. The label is a verb |
| `IconButton` · `ToolButton` | Icon only (title required, `tone` ok · danger, `active`) / a text button on a toolbar (`active` · `accent`) |
| `Badge` · `Avatar` · `Swatch` · `ProgressBar` | Count · status (`tone`, a fixed color with `color`) / initials square / color sample / progress bar (`pct` null = unknown, `label` = accessible name) |
| `TextInput` · `Select` · `TextArea` · `Checkbox` · `Radio` | Input (`full` · `grow`) / pick one from a list / multi-line (monospace) / check (`indeterminate`) / pick one. Every text field has autocorrect, autocapitalize, spellcheck and writing suggestions off (`textAssistOff`; fields for names, types and hosts). Only fields for prose, such as comments, use `prose` (spellcheck only) |

## GUI molecules (`molecules.tsx`)

| Piece | Rules |
|---|---|
| `Field` · `RadioGroup` · `FormSection` · `Hint` · `ColorPicker` | Label + field (hint · error below; `group` = several fields with labels of their own: a `<div>`, so clicking the label doesn't go to the first field) / a description per choice / section title / small gray description / color picker |
| `Named` · `InfoTip` | The ⓘ right after the name it explains: a field's label (`Field`'s `info`), next to a checkbox (not inside its label, so clicking ⓘ doesn't check it), a hint line, a section title — the same spacing and the same popover everywhere. Opens on click or Space · Enter, closes on Esc (closes only the popover, focus back to the ⓘ), a click outside or a scroll. Its name is "About <name>". Leave visible only the one line people must read before choosing (a warning · the result); the rest of the explanation goes behind the ⓘ |
| `SearchField` | ⌕ + input (+ loading ◌). `bare` = larger, without a border, like a popover header |
| `HighlightInput` | One-line input whose text is drawn in SQL colors (`spans`: UTF-16 ranges with keyword · function · number · string · comment, from `shared().highlight`). The input's own text is transparent and a colored copy is drawn over it in the same font and padding, following its scroll: caret, selection and keys stay the input's (the table tab's WHERE / ORDER BY) |
| `Tabs` · `SectionTitle` · `KeyHints` | Switch sections (`size` sm) / a small title inside a list or window / "↩ Open · ⇥ Next tab" |
| `Notice` · `EmptyState` | One block of info · warning · error / when there is nothing |
| `ListItem` | One list row: [lead (check)] [color strip] [icon] title description ……… right, a second line (`subtitle`), `selected` · `dim` · `mono` |
| `TreeRow` | One tree row: indent ▸ [lead] icon name [badge] description ……… right |
| `DetailRow` | Code + a value on the right (row count · pid · version), value `tone`, a `note` under the code (why it asks) |
| `MenuItem` · `MenuSeparator` · `MenuLabel` | One menu row (name ……… shortcut · ▸) / separator / dim description line |
| `TabItem` · `RailItem` · `StatusItem` | A closable tab (editor · result, 📌 pinned) / a cell of the left vertical rail / a status bar cell |

## GUI organisms (`organisms.tsx`) · result grid (`grid.tsx`)

| Piece | Rules |
|---|---|
| `Toolbar` | 30px high, 2px between icons, `ToolbarSep` between groups |
| `Modal` · `Overlay` · `Popover` | Dialog (`size` sm 420 · md 520 · lg 760 · xl 900, closes on Esc · a click outside, buttons on the right: Cancel → primary) / the backdrop behind a popup / a floating panel (header · list · footer hint) |
| `confirm()` · `ConfirmHost` | The in-app confirm. `detail` = DetailList; with `danger`, a red button and focus on Cancel by default. Buttons are named for the action (Delete · Run …) |
| `Menu` | Right-click (`at`) · dropdown. Items: action · separator · description line (`heading`) · submenu. Closes on a click outside · ⎋ |
| `List` · `ListGroup` · `DetailList` | List (`boxed`, `empty` when empty) / a group title inside a list / detail list |
| `PopupList` · `ComboBox` | One list right below a field (aligned to the field's left edge, above it when there is no room below, inside the window; closes when the page around it scrolls; keys through `listKey`: ↑↓ PgUp PgDn · ↩ · ⎋) / free input + suggestions (type fields: typing filters — exact, prefix, then contains; nothing shows when nothing matches — ↩ · ⇥ pick, ⎋ closes only the list). For picking only, use FontPicker (a search field + PopupList). Never a bare `<datalist>` (WKWebView draws it in the wrong place) |
| `Table` · `Th` · `Td` | A small table (`num` · `mono` · `muted`) |
| `FormGrid` | A long settings window: `<label>` + field, a whole row is a `FormSection` |
| `GridHead` · `GridHeaderRow` · `GridHeaderCell` · `GridRow` · `RowNumberCell` · `GridCell` · `JsonText` · `CellEditor` | Result grid cells (states: selected · cursor · changed · found · added · deleted · null · number). Virtual scrolling and selection are in ResultGrid. `GridHead`: header strip above the scroller (not sticky inside it), follows its scrollLeft. `GridHeaderCell` `onResize` · `onFit`: its right edge resizes the column (drag) or fits it (double-click; widths in `lib/columnWidths.ts`). `GridCell` `align`: left · right · center per value class (`lib/gridAlign.ts`, Settings › Appearance › Data grid › Alignment). `JsonText`: a JSON cell's text in colors (`--ui-json-*`; pieces from `lib/jsonSpans.ts`) |

Icons (`icons.tsx`) use each database kind's own color. What has to follow the theme (the expand arrow) uses `currentColor`.

## TUI (`crates/tui/src/kit`)

The same names and colors as the GUI. Screen code doesn't use `Color::Rgb(…)` or hand-built lines.

| Level | Pieces |
|---|---|
| Atoms (`atoms.rs`) | Color tokens `ACCENT` `DIM` `SEL` `SEL_BLUR` `SURFACE` `POPUP_BG` `OK` `WARN` `DANGER` `NUMBER` `C_DB` … · `selected(focused)` · `key` · `muted` · `icon` · `swatch` · `badge` · `fit_spans` (exactly the width, handles a double-width character at the edge) · `clear` |
| Molecules (`molecules.rs`) | `Row` (indent · ▸ · lead pieces · name · description · right · color · selection → `line()` / `fit(skip, width)`) · `key_row` · `field_row` · `key_hint` · `key_hints` · `section` |
| Organisms (`organisms.rs`) | `panel(title, focused)` · `centered` · `popup` · `popup_danger` · `confirm_lines` (row count colors: 0 dim · 1–99 WARN · 100+ DANGER) · `wrapped_height` (height including wrapped lines) |

After changing a TUI screen, redraw the Storybook scenes with `pnpm tui-snapshots` and commit them with it (`crates/tui/src/kit_snapshots.rs`).

## Words

- Titles in Title Case ("Back Up Database"), buttons are verbs.
- Shortcuts on macOS are written ⌘ ⌥ ⌃ ⇧ (Alt+ / Ctrl+ only on Windows and Linux).
- Text that overflows shows an ellipsis and the whole text in `title` (never left cut off).
