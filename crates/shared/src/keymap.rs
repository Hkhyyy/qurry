//! Command shortcuts of both apps: one table (keymap-table.json) and the key notation the GUI and the TUI share.
//! The GUI imports the table as JSON (no WASM needed to know its keys) and calls the notation functions through WASM;
//! the TUI reads both from here; scripts/gen-keymap-docs.py reads the table for docs/shortcuts-*.md.
//!
//! Table rows: `gui` = GUI command id, `tui` = TUI Action name (its Debug name), each absent when the app has no such command.
//! keymap.json stores user keys under those ids ("gui" / "tui" sections), so an id never changes. A command both apps have is one row.
//! `title` is the command's name on screen (`tuiTitle` when the TUI names it differently), `scope` where a GUI key works,
//! `guiKeys` the GUI defaults ("Mod+Shift+L"), `tuiKeys` the TUI defaults ("Ctrl+L").
//! Row order is the order each app lists its commands in: the GUI keymap editor per scope, the TUI F1 window and --help
//! (where the two apps order a shared command differently, its row follows one of them).
//!
//! Spec notation: modifiers then the key, joined by "+": "Mod+Alt+O", "Ctrl+Shift+K", "F5", "Alt+\\".
//! Mod = ⌘ on macOS / Ctrl elsewhere (GUI only; the terminal has no ⌘). Modifier order: Mod, Ctrl, Alt, Shift.
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Where a GUI key works. Global overlaps every scope; tree (Database Explorer) and editor (query console) only themselves
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Global,
    Tree,
    Editor,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Command {
    pub gui: Option<String>,
    pub tui: Option<String>,
    pub title: String,
    pub tui_title: Option<String>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub gui_keys: Vec<String>,
    #[serde(default)]
    pub tui_keys: Vec<String>,
}

static TABLE: LazyLock<Vec<Command>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("keymap-table.json")).unwrap_or_else(|e| panic!("crates/shared/src/keymap-table.json is not a valid command table ({e}): fix that row, then run cargo test -p qurry-shared"))
});

impl Command {
    /// Name on the TUI's screen
    pub fn tui_title(&self) -> &str {
        self.tui_title.as_deref().unwrap_or(&self.title)
    }
}

/// Every command, in table order
pub fn commands() -> &'static [Command] {
    &TABLE
}

/// The row of a TUI Action (by its keymap.json name)
pub fn tui_command(id: &str) -> Option<&'static Command> {
    TABLE.iter().find(|c| c.tui.as_deref() == Some(id))
}

/// Whether two GUI scopes share keys (a key bound in both is a conflict)
pub fn scopes_overlap(a: Scope, b: Scope) -> bool {
    a == b || a == Scope::Global || b == Scope::Global
}

/// Other commands that use `spec` in a scope overlapping `scope`. `bound` is every command's current keys.
/// The TUI has one scope: it passes Global for all
pub fn conflicts<'a, K: PartialEq>(me: &K, scope: Scope, spec: &str, bound: impl IntoIterator<Item = (K, Scope, &'a [String])>) -> Vec<K> {
    bound.into_iter().filter(|(id, s, keys)| id != me && scopes_overlap(*s, scope) && keys.iter().any(|k| k == spec)).map(|(id, ..)| id).collect()
}

/// Whether a spec can be a command shortcut: a ⌘ (Mod) / ⌃ / ⌥ combo or an F key (other keys clash with typing and moving)
pub fn bindable(spec: &str) -> bool {
    let f_key = spec.rsplit('+').next().and_then(|k| k.strip_prefix('F')).is_some_and(|n| (1..=2).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()));
    spec.starts_with("Mod+") || spec.starts_with("Ctrl+") || spec.starts_with("Alt+") || f_key
}

/// Normalize a hand-written TUI spec ("ctrl+l" → "Ctrl+L", "shift+f1" → "Shift+F1")
pub fn normalize(s: &str) -> String {
    let parts: Vec<&str> = s.split('+').collect();
    // When the key itself is '+', like "Alt++"
    let (mods, key) = if s.ends_with("++") { (&parts[..parts.len() - 2], "+") } else { (&parts[..parts.len() - 1], parts[parts.len() - 1]) };
    let has = |m: &str| mods.iter().any(|x| x.eq_ignore_ascii_case(m));
    let mut out = String::new();
    for (m, name) in [("ctrl", "Ctrl+"), ("alt", "Alt+"), ("shift", "Shift+")] {
        if has(m) {
            out.push_str(name);
        }
    }
    let named = ["Space", "Enter", "Tab", "Esc", "Backspace", "Delete", "Insert", "Home", "End", "PgUp", "PgDn", "Up", "Down", "Left", "Right"];
    // Letters·F keys uppercase ("l" → "L", "f5" → "F5")
    let key = named.iter().find(|n| n.eq_ignore_ascii_case(key)).map(|n| n.to_string()).unwrap_or_else(|| key.to_uppercase());
    out + &key
}

/// GUI display of a spec: ⌃⌥⇧⌘ and key symbols (↩ ⌫ ↑ …) on macOS, Ctrl+Alt+Shift+ elsewhere (Mod is Ctrl there); letters as in key_label
pub fn format_spec(spec: &str, mac: bool) -> String {
    let mut parts: Vec<&str> = spec.split('+').collect();
    let key = parts.pop().unwrap_or_default();
    let has = |m: &str| parts.contains(&m);
    let s = if mac {
        let sym = match key {
            "Enter" => "↩",
            "Backspace" => "⌫",
            "Delete" => "⌦",
            "Esc" => "⎋",
            "Tab" => "⇥",
            "Up" => "↑",
            "Down" => "↓",
            "Left" => "←",
            "Right" => "→",
            "PageUp" => "⇞",
            "PageDown" => "⇟",
            k => k,
        };
        let glyphs = [("Ctrl", "⌃"), ("Alt", "⌥"), ("Shift", "⇧"), ("Mod", "⌘")];
        glyphs.iter().filter(|(m, _)| has(m)).map(|(_, g)| *g).collect::<String>() + sym
    } else {
        let mods = [(has("Mod") || has("Ctrl"), "Ctrl"), (has("Alt"), "Alt"), (has("Shift"), "Shift")];
        mods.iter().filter(|(on, _)| *on).map(|(_, m)| *m).chain([key]).collect::<Vec<_>>().join("+")
    };
    key_label(&s)
}

/// TUI display of text with keys in it: ⌃⌥⇧ symbols on macOS (terminal Alt = Option), Ctrl+ / Alt+ / Shift+ elsewhere.
/// Screen text is written with symbols ("⌃k"), and the storage format ("Alt+X") is accepted too
pub fn disp(s: &str, mac: bool) -> String {
    if mac { disp_mac(s) } else { key_label(&glyph_words(s)) }
}

/// disp's macOS notation
pub fn disp_mac(s: &str) -> String {
    key_label(s).replace("Ctrl+", "⌃").replace("Alt+", "⌥").replace("Shift+", "⇧")
}

/// ⌃⌥⇧ symbols → Ctrl+ / Alt+ / Shift+ (non-macOS terminal screens). Combos containing ⌘ (⇧⌘A: Mac terminal keys) stay as is
fn glyph_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let run: String = rest.chars().take_while(|c| "⌃⌥⇧⌘".contains(*c)).collect();
        if run.is_empty() {
            out.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        rest = &rest[run.len()..];
        if run.contains('⌘') {
            out.push_str(&run);
            continue;
        }
        for g in run.chars() {
            out.push_str(match g {
                '⌃' => "Ctrl+",
                '⌥' => "Alt+",
                _ => "Shift+",
            });
        }
        // Only one + in ⌥+wheel
        if let Some(r) = rest.strip_prefix('+') {
            rest = r;
        }
    }
    out
}

/// "Ctrl+K" → "Ctrl+k", "⌘K" → "⌘k": a single letter with ⌘/⌃/⌥ and no Shift is shown lowercase (uppercase reads as a key needing Shift)
pub fn key_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        let at_word = out.chars().last().is_none_or(|c| !c.is_alphanumeric());
        if at_word && (rest.starts_with("Ctrl+") || rest.starts_with("Alt+")) {
            // Modifier group (Ctrl+Alt+Shift+ …)
            let mut chain = 0;
            let mut shift = false;
            while let Some(m) = ["Ctrl+", "Alt+", "Shift+"].iter().find(|m| rest[chain..].starts_with(**m)) {
                shift |= *m == "Shift+";
                chain += m.len();
            }
            out.push_str(&rest[..chain]);
            rest = &rest[chain..];
            let key_len = rest.find(|c: char| !c.is_alphanumeric()).unwrap_or(rest.len());
            let key = &rest[..key_len];
            if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) && !shift {
                out.push_str(&key.to_ascii_lowercase());
            } else {
                out.push_str(key);
            }
            rest = &rest[key_len..];
            continue;
        }
        // Keys written with symbols (⌘C, ⌥⌘Y): modifier symbol group + one char
        let glyphs: String = rest.chars().take_while(|c| "⌃⌥⇧⌘".contains(*c)).collect();
        if !glyphs.is_empty() {
            let after = &rest[glyphs.len()..];
            let mut it = after.chars();
            if let Some(k) = it.next().filter(|k| k.is_ascii_uppercase() && !it.next().is_some_and(|n| n.is_alphanumeric())) {
                out.push_str(&glyphs);
                out.push(if glyphs.contains('⇧') { k } else { k.to_ascii_lowercase() });
                rest = &after[1..];
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn letters_without_shift_are_lowercase() {
        assert_eq!(key_label("Ctrl+K actions"), "Ctrl+k actions");
        assert_eq!(key_label("Alt+Shift+X · Ctrl+Enter / F5"), "Alt+Shift+X · Ctrl+Enter / F5");
        assert_eq!(key_label("y  ⌘C · ⌘A ⌫ · ⇧⌘A · ⌘F2"), "y  ⌘c · ⌘a ⌫ · ⇧⌘A · ⌘F2");
        assert_eq!(key_label("⌥⌘Y · ⌃⇧F · Run (⌘↩) · ⌘["), "⌥⌘y · ⌃⇧F · Run (⌘↩) · ⌘[");
    }

    #[test]
    fn gui_notation() {
        assert_eq!(format_spec("Mod+Shift+L", true), "⇧⌘L");
        assert_eq!(format_spec("Mod+Alt+O", true), "⌥⌘o");
        assert_eq!(format_spec("Ctrl+Alt+Shift+Up", true), "⌃⌥⇧↑");
        assert_eq!(format_spec("Mod+Enter", true), "⌘↩");
        assert_eq!(format_spec("Mod+Shift+L", false), "Ctrl+Shift+L");
        assert_eq!(format_spec("Mod+Ctrl+G", false), "Ctrl+g");
        assert_eq!(format_spec("Alt+Enter", false), "Alt+Enter");
    }

    #[test]
    fn tui_notation() {
        assert_eq!(disp("Ctrl+L new · Alt+E explain · Ctrl+S", true), "⌃l new · ⌥e explain · ⌃s");
        assert_eq!(disp("Ctrl+L new · Alt+E explain · Ctrl+S", false), "Ctrl+l new · Alt+e explain · Ctrl+s");
        assert_eq!(disp_mac("Mod+K · Ctrl+K · Alt+Shift+X · ⌥+wheel"), "Mod+K · ⌃k · ⌥⇧X · ⌥+wheel");
        // Symbols → words off macOS (one + in ⌥+wheel, ⌘ combos kept)
        assert_eq!(disp("⌃k · ⌥⇧N · ⌥+wheel · ⇧Tab · ⇧⌘A", false), "Ctrl+k · Alt+Shift+N · Alt+wheel · Shift+Tab · ⇧⌘A");
    }

    #[test]
    fn bindable_keys() {
        assert!(["Mod+K", "Ctrl+L", "Alt+\\", "F5", "Shift+F1", "Mod+Shift+F12"].iter().all(|s| bindable(s)));
        assert!(!["L", "Enter", "Shift+Tab", "Up", "Delete", "Shift+F"].iter().any(|s| bindable(s)));
    }

    #[test]
    fn hand_written_specs_normalize() {
        assert_eq!(normalize("ctrl+l"), "Ctrl+L");
        assert_eq!(normalize("shift+f1"), "Shift+F1");
        assert_eq!(normalize("alt+shift+pgdn"), "Alt+Shift+PgDn");
        assert_eq!(normalize("Alt++"), "Alt++");
    }

    #[test]
    fn conflicts_follow_scopes() {
        let k = |s: &str| vec![s.to_string()];
        let (a, b, c) = (k("Mod+D"), k("Mod+D"), k("Mod+D"));
        let bound = [("tree.duplicate", Scope::Tree, &a[..]), ("editor.duplicate", Scope::Editor, &b[..]), ("other", Scope::Global, &c[..])];
        assert_eq!(conflicts(&"tree.duplicate", Scope::Tree, "Mod+D", bound), ["other"]);
        assert_eq!(conflicts(&"x", Scope::Global, "Mod+D", bound), ["tree.duplicate", "editor.duplicate", "other"]);
        assert!(conflicts(&"x", Scope::Global, "Mod+E", bound).is_empty());
    }

    /// Ids are what keymap.json stores: unique, and every default is a key the app can record
    #[test]
    fn table_is_consistent() {
        let (mut gui, mut tui) = (HashSet::new(), HashSet::new());
        let mut gui_keys: Vec<(Scope, &str)> = Vec::new();
        let mut tui_keys = HashSet::new();
        for c in commands() {
            assert!(c.gui.is_some() || c.tui.is_some(), "{}: a row needs a gui or tui id", c.title);
            if let Some(id) = &c.gui {
                assert!(gui.insert(id), "duplicate gui id {id}");
                for k in &c.gui_keys {
                    // tree.remove's Delete is the one default that can't be recorded (Reset brings it back)
                    assert!(bindable(k) || k == "Delete", "{id}: {k} can't be recorded as a shortcut");
                    assert!(!gui_keys.iter().any(|(s, x)| x == k && scopes_overlap(*s, c.scope)), "{id}: default {k} is taken");
                    gui_keys.push((c.scope, k));
                }
            } else {
                assert!(c.gui_keys.is_empty() && c.scope == Scope::Global, "{}: GUI keys or scope without a gui id", c.title);
            }
            if let Some(id) = &c.tui {
                assert!(tui.insert(id), "duplicate tui id {id}");
                for k in &c.tui_keys {
                    assert!(bindable(k), "{id}: {k} can't be recorded as a shortcut");
                    assert_eq!(&normalize(k), k, "{id}: write {k} in normalized form");
                    assert!(tui_keys.insert(k), "{id}: default {k} is taken");
                }
            } else {
                assert!(c.tui_keys.is_empty() && c.tui_title.is_none(), "{}: TUI keys or title without a tui id", c.title);
            }
        }
        assert_eq!(tui_command("NewConsole").and_then(|c| c.gui.as_deref()), Some("console.new"));
    }

    /// A command both apps have is one row, so either app's id finds the other's
    #[test]
    fn shared_commands_are_one_row() {
        let pairs = [
            ("goto.object", "GoToObject"),
            ("find.action", "Palette"),
            ("keymap", "Help"),
            ("explorer", "ToggleExplorer"),
            ("tree.ddl", "Ddl"),
            ("tree.modify", "ModifyTable"),
            ("tree.rename", "RenameObject"),
            ("tree.properties", "EditDataSource"),
            ("tree.newDs", "NewDataSource"),
            ("tree.disconnect", "Disconnect"),
            ("console.new", "NewConsole"),
            ("refresh", "Refresh"),
            ("tab.close", "CloseTab"),
            ("tab.next", "NextTab"),
            ("tab.prev", "PrevTab"),
            ("split.other", "OtherSplit"),
            ("editor.run", "Execute"),
            ("editor.cancel", "Cancel"),
            ("editor.history", "QueryHistory"),
        ];
        for (gui, tui) in pairs {
            assert_eq!(tui_command(tui).and_then(|c| c.gui.as_deref()), Some(gui), "{tui} should share a row with {gui}");
        }
        let shared: Vec<_> = commands().iter().filter_map(|c| c.gui.as_deref().zip(c.tui.as_deref())).collect();
        assert_eq!(shared.len(), pairs.len(), "a new shared row: add it to this list ({shared:?})");
        let modify = tui_command("ModifyTable").unwrap();
        assert_eq!((modify.title.as_str(), modify.tui_title()), ("Modify Table", "Modify Table… (change columns, keys, indexes, foreign keys → ALTER)"));
        assert_eq!(tui_command("Ddl").unwrap().tui_title(), "Go to DDL");
    }
}
