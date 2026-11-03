# Settings

English | [한국어](ko/settings.md)

Open Settings with `⌘,` (GUI). The TUI has no settings window: its options are in `⌃k` (Find Action), and its font is the terminal's (see [TUI in your terminal › Fonts](tui.md#fonts)).

## Appearance

| Setting | What it does |
|---|---|
| **Theme** | Dark, Light or High contrast, or **Sync with OS**. Quick switch: `` ⌃` `` |
| **Editor font** | Font family and size of the query console editor. Size 8–32 px (default 13); `⌃⇧.` / `⌃⇧,` change it too, and Settings shows the new size |
| **Grid font** | Font family and size of the result and table grids. Size 9–16 px (default 12): rows keep their height, so larger text would be cut. Columns get wider with the size |
| **Updates**, **Channel** | The automatic update check and the stable / beta channel: [Updates](updates.md) |

Font changes apply at once, to every open console and grid.

**Font family**: the list suggests the monospace fonts installed on this computer (JetBrains Mono, Menlo, Fira Code, D2Coding, Cascadia Code and other common ones); any other installed font can be typed in. Each field shows its text in the chosen font. Leave the field empty for the default (JetBrains Mono, or Menlo when it isn't installed).

A font that isn't installed (a typo, or a font removed since) never breaks the screen: Qurry falls back to the default monospace font, and Settings says `"<name>" isn't installed, so the default monospace font is used.` Pick a font from the list, or clear the field to go back to the default.

The theme and fonts are kept by the GUI on this computer (not in the [config folder](getting-started.md#where-qurry-keeps-your-settings)), so they don't affect the TUI.

## Keymap

Every GUI shortcut, and the keys you changed: [GUI keyboard shortcuts](shortcuts-gui.md). The TUI's keys are changed in its `F1` window.

## Passwords

Where the key for saved passwords is kept (config folder or system keychain): [Getting started › Where Qurry keeps your settings](getting-started.md#where-qurry-keeps-your-settings).

---

[← User guide](README.md)
