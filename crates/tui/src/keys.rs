//! Rebindable command shortcuts. Defaults in DEFAULT_KEYS, user settings in the "tui" section of keymap.json (only ones differing from defaults)
//! Key notation: "Ctrl+L", "Alt+Shift+X", "F5", "Ctrl+Enter", "Alt+\\" … (modifier order: Ctrl, Alt, Shift)
use crate::app::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

pub const DEFAULT_KEYS: &[(Action, &[&str])] = &[
    (Action::Quit, &["Ctrl+Q"]),
    (Action::Help, &["F1"]),
    (Action::Palette, &["Ctrl+K"]),
    (Action::SearchEverywhere, &["Ctrl+P"]),
    (Action::GoToObject, &["Ctrl+O"]),
    (Action::Ddl, &["Ctrl+B"]),
    (Action::RecentTabs, &["Ctrl+E"]),
    (Action::NewConsole, &["Ctrl+L"]),
    (Action::CloseTab, &["Ctrl+W"]),
    (Action::NextTab, &["Alt+Right", "Ctrl+PgDn"]),
    (Action::PrevTab, &["Alt+Left", "Ctrl+PgUp"]),
    // Alt+1~9 are workspaces (input.rs, when the Alt+digit isn't in the keymap)
    (Action::ToggleExplorer, &["Alt+0"]),
    (Action::WsNew, &["Alt+N"]),
    (Action::WsRename, &["Alt+Shift+N"]),
    (Action::CycleFocus, &["F6"]),
    (Action::ModifyTable, &["Ctrl+F6"]),
    (Action::RenameObject, &["Shift+F6"]),
    (Action::SplitRight, &["Alt+\\"]),
    (Action::SplitDown, &["Alt+-"]),
    (Action::OtherSplit, &["Alt+Tab"]),
    (Action::SplitShrink, &["Alt+Shift+Left"]),
    (Action::SplitGrow, &["Alt+Shift+Right"]),
    (Action::ExplorerNarrower, &["Alt+["]),
    (Action::ExplorerWider, &["Alt+]"]),
    (Action::Execute, &["F5", "Ctrl+Enter", "Alt+Enter"]),
    (Action::Explain, &["Alt+E"]),
    (Action::Complete, &["Ctrl+Space"]),
    (Action::Cancel, &["Ctrl+C"]),
    (Action::TxToggle, &["Alt+T"]),
    (Action::Commit, &["Alt+C"]),
    (Action::Rollback, &["Alt+R"]),
    (Action::ToggleOutput, &["Alt+O"]),
    // Moving between panes is ⌃⌥ + arrows (⌥ + arrows is within a pane: ←/→ tabs)
    (Action::PaneUp, &["Ctrl+Alt+Up"]),
    (Action::PaneDown, &["Ctrl+Alt+Down"]),
    (Action::PaneLeft, &["Ctrl+Alt+Left"]),
    (Action::PaneRight, &["Ctrl+Alt+Right"]),
    (Action::ShrinkEditor, &["Alt+Shift+Up"]),
    (Action::GrowEditor, &["Alt+Shift+Down"]),
    (Action::Submit, &["Ctrl+S"]),
    (Action::Find, &["Ctrl+F"]),
    (Action::Disconnect, &["Ctrl+F2"]),
];

/// Name written to keymap.json (the enum name as is)
pub fn action_id(a: Action) -> String {
    format!("{a:?}")
}

fn defaults(a: Action) -> Vec<String> {
    DEFAULT_KEYS.iter().find(|d| d.0 == a).map(|d| d.1.iter().map(|s| s.to_string()).collect()).unwrap_or_default()
}

/// Key event → notation. None if not a rebindable key (e.g. a char without modifiers)
pub fn spec(k: &KeyEvent) -> Option<String> {
    let mut ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let mut shift = k.modifiers.contains(KeyModifiers::SHIFT);
    let key = match k.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) if c.is_ascii_alphabetic() => {
            shift |= c.is_ascii_uppercase();
            c.to_ascii_uppercase().to_string()
        }
        // Korean keyboards send the \ key as ₩
        KeyCode::Char('₩') => {
            shift = false;
            "\\".to_string()
        }
        KeyCode::Char(c) => {
            // Symbols like ? ! already include Shift in the char
            shift = false;
            c.to_string()
        }
        KeyCode::Null => {
            // Ctrl+Space on some terminals
            ctrl = true;
            "Space".to_string()
        }
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::BackTab => {
            shift = true;
            "Tab".into()
        }
        KeyCode::Esc => "Esc".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PgUp".into(),
        KeyCode::PageDown => "PgDn".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::F(n) => format!("F{n}"),
        _ => return None,
    };
    let mut s = String::new();
    for (on, m) in [(ctrl, "Ctrl+"), (alt, "Alt+"), (shift, "Shift+")] {
        if on {
            s.push_str(m);
        }
    }
    Some(s + &key)
}

/// Display notation: ⌃⌥⇧ symbols on macOS (same as the GUI; terminal Alt = Option), Ctrl+ / Alt+ / Shift+ elsewhere.
/// Screen text is written with symbols ("⌃k"), and the storage format ("Alt+X") is accepted too
pub fn disp(s: &str) -> String {
    if cfg!(target_os = "macos") {
        lower_unshifted(s).replace("Ctrl+", "⌃").replace("Alt+", "⌥").replace("Shift+", "⇧")
    } else {
        lower_unshifted(&glyph_words(s))
    }
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

/// "Ctrl+K" → "Ctrl+k": a single letter with ⌃/⌥ and no Shift is shown lowercase (uppercase reads as a key needing Shift)
fn lower_unshifted(s: &str) -> String {
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

/// Whether usable as a command shortcut: only Ctrl/Alt combos or F keys (others clash with typing·movement keys)
pub fn bindable(s: &str) -> bool {
    s.starts_with("Ctrl+") || s.starts_with("Alt+") || s.trim_start_matches("Shift+").strip_prefix('F').is_some_and(|n| n.parse::<u8>().is_ok())
}

/// Normalize user-setting notation ("ctrl+l" → "Ctrl+L")
fn normalize(s: &str) -> String {
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

pub struct Keymap {
    map: HashMap<Action, Vec<String>>,
}

impl Keymap {
    /// Overlay user settings on the defaults. Unknown action names are ignored
    pub fn new(user: &qurry_core::store::Keymap) -> Self {
        let mut map: HashMap<Action, Vec<String>> = DEFAULT_KEYS.iter().map(|(a, _)| (*a, defaults(*a))).collect();
        for (a, _) in DEFAULT_KEYS {
            if let Some(v) = user.get(&action_id(*a)) {
                map.insert(*a, v.iter().map(|s| normalize(s)).collect());
            }
        }
        Keymap { map }
    }

    pub fn lookup(&self, k: &KeyEvent) -> Option<Action> {
        let s = spec(k)?;
        DEFAULT_KEYS.iter().map(|d| d.0).find(|a| self.map.get(a).is_some_and(|v| v.contains(&s)))
    }

    pub fn keys(&self, a: Action) -> &[String] {
        self.map.get(&a).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// For display ("F5 / Ctrl+Enter")
    pub fn label(&self, a: Action) -> String {
        let k = self.keys(a);
        if k.is_empty() { "—".into() } else { disp(&k.join(" / ")) }
    }

    pub fn is_custom(&self, a: Action) -> bool {
        self.keys(a) != defaults(a).as_slice()
    }

    /// Assign a key (add = add to existing keys). Removed from any other command using the same key → the commands it was removed from
    pub fn bind(&mut self, a: Action, key: String, add: bool) -> Option<Action> {
        let mut taken = None;
        for (other, v) in self.map.iter_mut() {
            if *other != a && v.contains(&key) {
                v.retain(|x| x != &key);
                taken = Some(*other);
            }
        }
        let v = self.map.entry(a).or_default();
        if !add {
            v.clear();
        }
        if !v.contains(&key) {
            v.push(key);
        }
        taken
    }

    pub fn reset(&mut self, a: Action) {
        self.map.insert(a, defaults(a));
    }

    /// What to write to keymap.json: only commands differing from defaults
    pub fn overrides(&self) -> qurry_core::store::Keymap {
        DEFAULT_KEYS.iter().filter(|(a, _)| self.is_custom(*a)).map(|(a, _)| (action_id(*a), self.keys(*a).to_vec())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    /// Display: ⌃/⌥+letter without Shift is lowercase (so it doesn't look like uppercase = needs Shift)
    #[test]
    fn labels_lowercase_letters_without_shift() {
        let mac = cfg!(target_os = "macos");
        let d = |s: &str| disp(s);
        assert_eq!(d("Ctrl+K actions"), if mac { "⌃k actions" } else { "Ctrl+k actions" });
        assert_eq!(d("Ctrl+L new · Alt+E explain · Ctrl+S"), if mac { "⌃l new · ⌥e explain · ⌃s" } else { "Ctrl+l new · Alt+e explain · Ctrl+s" });
        // Uppercase if Shift is present, non-letter keys as is
        assert_eq!(d("Alt+Shift+X"), if mac { "⌥⇧X" } else { "Alt+Shift+X" });
        assert_eq!(d("Ctrl+Enter / Ctrl+Alt+Up / Ctrl+PgDn / F5"), if mac { "⌃Enter / ⌃⌥Up / ⌃PgDn / F5" } else { "Ctrl+Enter / Ctrl+Alt+Up / Ctrl+PgDn / F5" });
        // Letters·text without modifiers as is
        assert_eq!(d("D delete · Ctrl+Space"), if mac { "D delete · ⌃Space" } else { "D delete · Ctrl+Space" });
        // Keys written with symbols follow the same rule (⌘C → ⌘c, ⇧⌘A unchanged)
        assert_eq!(d("y  ⌘C · ⌘A ⌫ · ⇧⌘A · ⌘F2"), "y  ⌘c · ⌘a ⌫ · ⇧⌘A · ⌘F2");
    }

    #[test]
    fn spec_of_events() {
        assert_eq!(spec(&ev(KeyCode::Char('l'), KeyModifiers::CONTROL)).unwrap(), "Ctrl+L");
        assert_eq!(spec(&ev(KeyCode::Char(' '), KeyModifiers::CONTROL)).unwrap(), "Ctrl+Space");
        assert_eq!(spec(&ev(KeyCode::Char('\\'), KeyModifiers::ALT)).unwrap(), "Alt+\\");
        assert_eq!(spec(&ev(KeyCode::Char('₩'), KeyModifiers::ALT)).unwrap(), "Alt+\\");
        assert_eq!(spec(&ev(KeyCode::Char('D'), KeyModifiers::SHIFT)).unwrap(), "Shift+D");
        assert_eq!(spec(&ev(KeyCode::Char('?'), KeyModifiers::SHIFT)).unwrap(), "?");
        assert_eq!(spec(&ev(KeyCode::BackTab, KeyModifiers::SHIFT)).unwrap(), "Shift+Tab");
        assert_eq!(spec(&ev(KeyCode::PageDown, KeyModifiers::CONTROL)).unwrap(), "Ctrl+PgDn");
    }

    #[test]
    fn defaults_and_overrides() {
        let mut km = Keymap::new(&Default::default());
        assert_eq!(km.lookup(&ev(KeyCode::Char('l'), KeyModifiers::CONTROL)), Some(Action::NewConsole));
        assert_eq!(km.lookup(&ev(KeyCode::Enter, KeyModifiers::ALT)), Some(Action::Execute));
        assert!(km.overrides().is_empty());

        // Taking a key another command used removes it from that command
        assert_eq!(km.bind(Action::NewConsole, "Ctrl+K".into(), false), Some(Action::Palette));
        assert_eq!(km.lookup(&ev(KeyCode::Char('k'), KeyModifiers::CONTROL)), Some(Action::NewConsole));
        assert_eq!(km.lookup(&ev(KeyCode::Char('l'), KeyModifiers::CONTROL)), None);
        assert_eq!(km.label(Action::Palette), "—");
        let o = km.overrides();
        assert_eq!(o.get("NewConsole").unwrap(), &vec!["Ctrl+K".to_string()]);
        assert_eq!(o.get("Palette").unwrap(), &Vec::<String>::new());

        // Re-reading saved settings gives the same result (notation normalized)
        let mut user = o.clone();
        user.insert("Help".into(), vec!["shift+f1".into()]);
        let km2 = Keymap::new(&user);
        assert_eq!(km2.lookup(&ev(KeyCode::Char('k'), KeyModifiers::CONTROL)), Some(Action::NewConsole));
        assert_eq!(km2.keys(Action::Help), ["Shift+F1"]);

        km.reset(Action::Palette);
        km.reset(Action::NewConsole);
        assert!(km.overrides().is_empty());
    }

    #[test]
    fn only_modified_or_function_keys_are_bindable() {
        assert!(bindable("Ctrl+L") && bindable("Alt+\\") && bindable("F5") && bindable("Shift+F1"));
        assert!(!bindable("L") && !bindable("Enter") && !bindable("Shift+Tab") && !bindable("Up"));
    }

    #[test]
    fn every_default_is_bindable_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (a, keys) in DEFAULT_KEYS {
            for k in *keys {
                assert!(bindable(k), "{a:?} {k}");
                assert!(seen.insert(*k), "duplicate default {k}");
                assert_eq!(normalize(k), *k);
            }
        }
    }

    /// Screen text uses symbols → words when not on macOS (only one + in ⌥+wheel)
    #[test]
    fn glyphs_become_words_off_macos() {
        assert_eq!(lower_unshifted(&glyph_words("⌃k · ⌥⇧N · ⌥+wheel · ⇧Tab · ⇧⌘A")), "Ctrl+k · Alt+Shift+N · Alt+wheel · Shift+Tab · ⇧⌘A");
    }
}
