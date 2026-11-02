"""Regenerate the user guide's shortcut tables from the command table both apps share (crates/shared/src/keymap-table.json),
the GUI Keymap Reference (src/lib/keymap.ts) and the TUI's fixed keys (crates/tui/src/help.rs).
Usage: python3 scripts/gen-keymap-docs.py  (replaces the content between <!-- keymap:…:start/end --> in shortcuts-gui.md · shortcuts-tui.md under docs/ and docs/ko/)"""
import json, re, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
table = json.loads((root / "crates/shared/src/keymap-table.json").read_text())

def esc(s): return s.replace("|", "\\|")

def key_label(s):
    """⌘/⌃/⌥ (Ctrl+/Alt+) + a single letter without Shift is lowercase (same as src/lib/shortcuts.ts keyLabel, crates/tui keys::disp)"""
    s = re.sub(r"([⌃⌥⇧⌘]+)([A-Z])(?![A-Za-z0-9])", lambda m: m.group(0) if "⇧" in m.group(1) else m.group(1) + m.group(2).lower(), s)
    return re.sub(r"\b((?:(?:Ctrl|Alt|Shift|Win|Cmd)\+)+)([A-Z])(?![A-Za-z0-9])", lambda m: m.group(0) if "Shift+" in m.group(1) else m.group(1) + m.group(2).lower(), s)

def sym(s):
    """Ctrl+/Alt+/Shift+ → ⌃/⌥/⇧ (same as the macOS notation of crates/tui keys::disp)"""
    return re.sub(r"\b(?:(?:Ctrl|Alt|Shift)\+)+", lambda m: m.group(0).replace("Ctrl+", "⌃").replace("Alt+", "⌥").replace("Shift+", "⇧"), s)

MAC_KEYS = {"Enter": "↩", "Backspace": "⌫", "Delete": "⌦", "Esc": "⎋", "Tab": "⇥", "Up": "↑", "Down": "↓", "Left": "←", "Right": "→", "PageUp": "⇞", "PageDown": "⇟"}

def mac_spec(spec):
    """GUI default key ("Mod+Shift+L") → macOS notation ("⇧⌘L"), as qurry_shared::keymap::format_spec draws it (letters: key_label in code()).
    src/lib/shortcuts.test.ts checks the generated docs against format_spec"""
    *mods, key = spec.split("+")
    return "".join(g for m, g in (("Ctrl", "⌃"), ("Alt", "⌥"), ("Shift", "⇧"), ("Mod", "⌘")) if m in mods) + MAC_KEYS.get(key, key)

def code(k):
    """Key cell of the table: keys containing ` are wrapped as `` k ``"""
    k = esc(sym(key_label(k)))
    return f"`` {k} ``" if "`" in k else f"`{k}`"

def gui():
    src = (root / "src/lib/keymap.ts").read_text()
    # Rebindable rows ([null, desc, id]) show the command's default keys
    defaults = {c["gui"]: " / ".join(map(mac_spec, c.get("guiKeys", []))) for c in table if "gui" in c}
    out = []
    for m in re.finditer(r'title: "([^"]+)",\s*rows: \[(.*?)\n    \],', src, re.S):
        out.append(f"\n## {m.group(1)}\n\n| macOS | Action |\n|---|---|")
        for k, d, i in re.findall(r'\[(null|"[^"]*"), "([^"]*)"(?:, "([^"]*)")?\]', m.group(2)):
            k = defaults[i] if k == "null" else k[1:-1]
            out.append(f"| {code(k)} | {esc(key_label(d))} |")
    return "\n".join(out)

def tui():
    src = (root / "crates/tui/src/help.rs").read_text()
    # Rebindable commands: TUI rows of the command table with default keys, in table order (as keys::DEFAULT_KEYS)
    out = ["\n## Commands (default keys — change them in the F1 window)\n\n| Key | Action |\n|---|---|"]
    for c in table:
        if "tui" in c and c.get("tuiKeys"):
            out.append(f"| {code(' / '.join(c['tuiKeys']))} | {esc(sym(key_label(c.get('tuiTitle', c['title']))))} |")
    for m in re.finditer(r'\("([^"]+)", &\[(.*?)\]\),', src, re.S):
        out.append(f"\n## {sym(key_label(m.group(1)))}\n\n| Key | Action |\n|---|---|")
        for k, d in re.findall(r'\("([^"]*)", "([^"]*)"\)', m.group(2)):
            k = k.replace("\\\\", "\\")  # \\ in Rust strings → \
            out.append(f"| {code(k)} | {esc(sym(key_label(d)))} |")
    return "\n".join(out)

for name, body in (("gui", gui()), ("tui", tui())):
    # English and Korean editions (same tables, descriptions are the app's on-screen text as is)
    for doc in (root / "docs" / f"shortcuts-{name}.md", root / "docs" / "ko" / f"shortcuts-{name}.md"):
        text = re.sub(rf"(<!-- keymap:{name}:start -->).*?(<!-- keymap:{name}:end -->)", lambda m: m.group(1) + "\n" + body + "\n\n" + m.group(2), doc.read_text(), flags=re.S)
        doc.write_text(text)
print("docs keymap updated")
