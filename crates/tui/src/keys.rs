//! Rebindable command shortcuts. Defaults and names come from the shared command table (qurry_core::keymap), user settings from the
//! "tui" section of keymap.json (only ones differing from defaults)
//! Key notation: "Ctrl+L", "Alt+Shift+X", "F5", "Ctrl+Enter", "Alt+\\" … (modifier order: Ctrl, Alt, Shift)
use crate::app::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use qurry_core::keymap::{self as table, Scope};
use std::collections::HashMap;
use std::sync::LazyLock;

pub use qurry_core::keymap::bindable;

/// Rebindable commands (the table's TUI rows with default keys) in table order: the F1 window, --help, docs.
/// Alt+1~9 are workspaces (input.rs, when the Alt+digit isn't in the keymap). Moving between panes is ⌃⌥ + arrows (⌥ + arrows is within a pane: ←/→ tabs)
pub static DEFAULT_KEYS: LazyLock<Vec<(Action, &'static [String])>> = LazyLock::new(|| {
    table::commands()
        .iter()
        .filter(|c| !c.tui_keys.is_empty())
        .map(|c| {
            let id = c.tui.as_deref().unwrap_or_default();
            let a = crate::app::ACTIONS.iter().map(|x| x.0).find(|a| action_id(*a) == id);
            (a.unwrap_or_else(|| panic!("keymap-table.json: tui id {id} is not an Action")), c.tui_keys.as_slice())
        })
        .collect()
});

/// Name written to keymap.json (the enum name as is)
pub fn action_id(a: Action) -> String {
    format!("{a:?}")
}

fn defaults(a: Action) -> Vec<String> {
    DEFAULT_KEYS.iter().find(|d| d.0 == a).map(|d| d.1.to_vec()).unwrap_or_default()
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

#[cfg(test)]
thread_local! {
    /// Tests only: ⌃⌥⇧ symbols on every OS while the Storybook scenes are drawn (the committed snapshots use the macOS notation)
    pub static FORCE_MAC_GLYPHS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn mac_glyphs() -> bool {
    #[cfg(test)]
    if FORCE_MAC_GLYPHS.get() {
        return true;
    }
    cfg!(target_os = "macos")
}

/// Display notation: ⌃⌥⇧ symbols on macOS (same as the GUI; terminal Alt = Option), Ctrl+ / Alt+ / Shift+ elsewhere.
/// Screen text is written with symbols ("⌃k"), and the storage format ("Alt+X") is accepted too
pub fn disp(s: &str) -> String {
    table::disp(s, mac_glyphs())
}

pub struct Keymap {
    map: HashMap<Action, Vec<String>>,
}

impl Keymap {
    /// Overlay user settings on the defaults. Unknown action names are ignored
    pub fn new(user: &qurry_core::store::Keymap) -> Self {
        let mut map: HashMap<Action, Vec<String>> = DEFAULT_KEYS.iter().map(|(a, _)| (*a, defaults(*a))).collect();
        for (a, _) in DEFAULT_KEYS.iter() {
            if let Some(v) = user.get(&action_id(*a)) {
                map.insert(*a, v.iter().map(|s| table::normalize(s)).collect());
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
        // One scope in the terminal: every command conflicts with every other
        let taken = table::conflicts(&a, Scope::Global, &key, self.map.iter().map(|(o, v)| (*o, Scope::Global, v.as_slice())));
        for o in &taken {
            if let Some(v) = self.map.get_mut(o) {
                v.retain(|x| x != &key);
            }
        }
        let v = self.map.entry(a).or_default();
        if !add {
            v.clear();
        }
        if !v.contains(&key) {
            v.push(key);
        }
        taken.into_iter().next()
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

    /// Every Action has a row in the shared table (its name on screen), and every TUI row is an Action
    #[test]
    fn every_action_has_a_table_row() {
        let ids: Vec<String> = crate::app::ACTIONS.iter().map(|(a, _)| action_id(*a)).collect();
        for id in &ids {
            assert!(table::tui_command(id).is_some(), "Action::{id} has no row in crates/shared/src/keymap-table.json: add one with \"tui\": \"{id}\"");
        }
        for c in table::commands() {
            if let Some(id) = &c.tui {
                assert!(ids.contains(id), "keymap-table.json row \"tui\": \"{id}\" is not an Action in the palette (app.rs ACTIONS): fix the name or add the Action");
            }
        }
        assert_eq!(DEFAULT_KEYS.iter().find(|d| d.0 == Action::NewConsole).map(|d| d.1), Some(&["Ctrl+L".to_string()][..]));
    }
}
