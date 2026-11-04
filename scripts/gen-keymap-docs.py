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

def pane_key(spec):
    """A pane key as typed ("N" → n, "Shift+D" → D), as crates/tui keys::pane_disp draws it"""
    k = spec.removeprefix("Shift+")
    if len(k) == 1 and k.isascii() and k.isalpha():
        return k if spec.startswith("Shift+") else k.lower()
    return sym(key_label(spec))

def code(k):
    """Key cell of the table: keys containing ` are wrapped as `` k ``"""
    k = esc(sym(key_label(k)))
    return f"`` {k} ``" if "`" in k else f"`{k}`"

def gui():
    src = (root / "src/lib/keymap.ts").read_text()
    # Rebindable rows ([null, id]) show the command's default keys (— for none) and its description in the table
    defaults = {c["gui"]: " / ".join(map(mac_spec, c.get("guiKeys", []))) or "—" for c in table if "gui" in c}
    descs = {c["gui"]: c.get("guiDesc", c["title"]) for c in table if "gui" in c}
    sections = [(m.group(1), re.findall(r'\[(null|"[^"]*"), "([^"]*)"\]', m.group(2))) for m in re.finditer(r'title: "([^"]+)",\s*rows: \[(.*?)\n    \],', src, re.S)]
    # The last section (keymap.ts OTHER_ACTIONS): every command no section lists, in table order
    listed = {d for _, rows in sections for k, d in rows if k == "null"}
    other = re.search(r'export const OTHER_ACTIONS = "([^"]+)";', src).group(1)
    sections.append((other, [("null", c["gui"]) for c in table if "gui" in c and c["gui"] not in listed]))
    out = []
    for title, rows in sections:
        out.append(f"\n## {title}\n\n| macOS | Action |\n|---|---|")
        for k, d in rows:
            k, d = (defaults[d], descs[d]) if k == "null" else (k[1:-1], d)
            out.append(f"| {code(k)} | {esc(key_label(d))} |")
    return "\n".join(out)

def tui():
    src = (root / "crates/tui/src/help.rs").read_text()
    # Rebindable commands: TUI rows of the command table with default keys, in the TUI's order (keys::ORDER, as keys::DEFAULT_KEYS)
    order = re.findall(r"Action::(\w+)", re.search(r"const ORDER: \[Action; \d+\] = \[(.*?)\];", (root / "crates/tui/src/keys.rs").read_text(), re.S).group(1))
    by_id = {c["tui"]: c for c in table if "tui" in c}
    title = lambda c: esc(sym(key_label(c.get("tuiTitle", c["title"]))))
    out = ["\n## Commands (default keys — change them in the F1 window)\n\n| Key | Action |\n|---|---|"]
    for c in (by_id[a] for a in order):
        if c.get("tuiKeys"):
            out.append(f"| {code(' / '.join(c['tuiKeys']))} | {title(c)} |")
    # As keys::COMMANDS lists them after ORDER: the commands without a default key (table order), then the pane keys
    out.append("\n## Commands without a default key (give them one in the F1 window)\n\n| Key | Action |\n|---|---|")
    out += [f"| — | {title(c)} |" for c in table if "tui" in c and not c.get("tuiKeys")]
    for pane, heading in (("tree", "Database Explorer keys (when the explorer has focus; change them in the F1 window)"), ("grid", "Data grid keys (when a grid has focus; change them in the F1 window)")):
        out.append(f"\n## {heading}\n\n| Key | Action |\n|---|---|")
        out += [f"| {code(' / '.join(map(pane_key, c['tuiPaneKeys'])))} | {title(c)} |" for c in table if c.get("tuiPane") == pane]
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
