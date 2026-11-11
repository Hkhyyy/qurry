//! Rebindable command shortcuts. Defaults and names come from the shared command table (qurry_core::keymap), user settings from the
//! "tui" section of keymap.json (only ones differing from defaults). Every Action can have keys that work anywhere; some also have keys
//! that work only in the Database Explorer or the data grid (the table's tuiPaneKeys: "n" New Data Source in the explorer)
//! Key notation: "Ctrl+L", "Alt+Shift+X", "F5", "Ctrl+Enter", "Alt+\\", pane keys also "N", "Shift+D", "?" … (modifier order: Ctrl, Alt, Shift)
use crate::app::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use qurry_core::keymap::{self as table, Scope};
use std::collections::HashMap;
use std::sync::LazyLock;

pub use qurry_core::keymap::bindable;

/// One line of the F1 window: an Action's keys anywhere (pane = Global) or its keys in one pane (Tree / Grid)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot {
    pub action: Action,
    pub pane: Scope,
}

impl Slot {
    pub fn global(action: Action) -> Self {
        Slot { action, pane: Scope::Global }
    }

    /// Its name in keymap.json: the Action's name, "<pane>.<Action>" for pane keys ("tree.NewDataSource")
    pub fn id(self) -> String {
        match self.pane {
            Scope::Global => action_id(self.action),
            p => format!("{}.{}", p.name(), action_id(self.action)),
        }
    }
}

/// The pane a pane key works in, as the F1 window and the palette name it
pub fn pane_name(pane: Scope) -> &'static str {
    if pane == Scope::Tree { "explorer" } else { "grid" }
}

/// Pane keys that stay fixed: they move, copy, filter, sort or edit and run no command (input/tree.rs, input/grid.rs). A pane key can't take them
pub fn fixed_in(pane: Scope, spec: &str) -> bool {
    let fixed: &[&str] = match pane {
        Scope::Tree => &["J", "K", "H", "L", "Y", "/", "F4"],
        Scope::Grid => &["J", "K", "H", "L", "G", "Shift+G", "Y", "S", "W", "O", "R", "/", "F2"],
        _ => &[],
    };
    fixed.contains(&spec)
}

/// The order the F1 window, --help and docs/shortcuts-tui.md list the commands with default keys in (scripts/gen-keymap-docs.py reads it here).
/// The table can't hold it: commands both apps have sit in the GUI's order there. Every TUI row with default keys is here once (a test checks)
const ORDER: [Action; 45] = [
    Action::Quit,
    Action::Help,
    Action::Palette,
    Action::SearchEverywhere,
    Action::GoToObject,
    Action::Ddl,
    Action::RecentTabs,
    Action::NewConsole,
    Action::CloseTab,
    Action::NextTab,
    Action::PrevTab,
    Action::ToggleExplorer,
    Action::WsNew,
    Action::WsRename,
    Action::CycleFocus,
    Action::ModifyTable,
    Action::RenameObject,
    Action::SplitRight,
    Action::SplitDown,
    Action::OtherSplit,
    Action::SplitShrink,
    Action::SplitGrow,
    Action::ExplorerNarrower,
    Action::ExplorerWider,
    Action::Execute,
    Action::QuickFix,
    Action::Explain,
    Action::Complete,
    Action::Reformat,
    Action::Cancel,
    Action::TxToggle,
    Action::Commit,
    Action::Rollback,
    Action::ToggleOutput,
    Action::ResultNext,
    Action::ResultPrev,
    Action::PaneUp,
    Action::PaneDown,
    Action::PaneLeft,
    Action::PaneRight,
    Action::ShrinkEditor,
    Action::GrowEditor,
    Action::Submit,
    Action::Find,
    Action::Disconnect,
];

/// Every rebindable line of the F1 window with its default keys: the commands with default keys in ORDER, then the other commands (no
/// default key, table order), then the Database Explorer's and the data grid's pane keys.
/// Alt+1~9 are workspaces (input/mod.rs, when the Alt+digit isn't in the keymap). Moving between panes is ⌃⌥ + arrows (⌥ + arrows is within a pane: ←/→ tabs)
pub static COMMANDS: LazyLock<Vec<(Slot, &'static [String])>> = LazyLock::new(|| {
    let action = |id: &str| crate::app::ACTIONS.iter().map(|d| d.action).find(|a| action_id(*a) == id);
    let mut v: Vec<(Slot, &'static [String])> = ORDER
        .iter()
        .map(|&a| {
            let keys = table::tui_command(&action_id(a)).map(|c| c.tui_keys.as_slice()).filter(|k| !k.is_empty());
            (Slot::global(a), keys.unwrap_or_else(|| panic!("keys.rs ORDER: {a:?} has no default keys in keymap-table.json")))
        })
        .collect();
    let rows = || table::commands().iter().filter_map(|c| Some((c, action(c.tui.as_deref()?)?)));
    v.extend(rows().filter(|(c, _)| c.tui_keys.is_empty()).map(|(c, a)| (Slot::global(a), c.tui_keys.as_slice())));
    for pane in [Scope::Tree, Scope::Grid] {
        v.extend(rows().filter(|(c, _)| c.tui_pane == Some(pane)).map(|(c, a)| (Slot { action: a, pane }, c.tui_pane_keys.as_slice())));
    }
    v
});

/// Name written to keymap.json (the enum name as is)
pub fn action_id(a: Action) -> String {
    format!("{a:?}")
}

fn defaults(s: Slot) -> Vec<String> {
    COMMANDS.iter().find(|d| d.0 == s).map(|d| d.1.to_vec()).unwrap_or_default()
}

/// Whether `spec` can be recorded for `slot` (the F1 window): a command shortcut anywhere; in a pane also one character, Delete / Insert,
/// unless the pane keeps that key for moving or copying
pub fn bindable_for(slot: Slot, spec: &str) -> bool {
    if slot.pane == Scope::Global { bindable(spec) } else { table::pane_bindable(spec, true) && !fixed_in(slot.pane, spec) }
}

/// A pane key as typed: "N" is the n key, "Shift+N" is N (the notation writes letters uppercase)
pub fn pane_disp(spec: &str) -> String {
    match spec.strip_prefix("Shift+").unwrap_or(spec) {
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphabetic()) => if spec.starts_with("Shift+") { k.to_string() } else { k.to_ascii_lowercase() },
        _ => disp(spec),
    }
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
    map: HashMap<Slot, Vec<String>>,
    /// (pane, key spec) → command, rebuilt whenever map changes. A key two commands have goes to the first in COMMANDS
    by_key: HashMap<(Scope, String), Action>,
    /// Default keys left out because keymap.json gives them to another command: (the command left without it, the key, that command)
    pub shadowed: Vec<(Slot, String, Slot)>,
}

impl Keymap {
    /// Overlay user settings on the defaults. Unknown names are ignored. A key the user gave a command stays that command's, also
    /// when a later version makes it another command's default (⌥⇧F Reformat Code in 0.3.2): the other command goes without it
    pub fn new(user: &qurry_core::store::Keymap) -> Self {
        let mut map: HashMap<Slot, Vec<String>> = COMMANDS
            .iter()
            .map(|(s, keys)| (*s, user.get(&s.id()).map_or_else(|| keys.to_vec(), |v| v.iter().map(|k| table::normalize(k)).collect())))
            .collect();
        let mine: Vec<(Slot, Vec<String>)> = map.iter().filter(|(s, _)| user.contains_key(&s.id())).map(|(s, v)| (*s, v.clone())).collect();
        let mut shadowed = vec![];
        for (s, keys) in map.iter_mut().filter(|(s, _)| !user.contains_key(&s.id())) {
            keys.retain(|k| match table::conflicts(s, s.pane, k, mine.iter().map(|(o, v)| (*o, o.pane, v.as_slice()))).first() {
                Some(by) => {
                    shadowed.push((*s, k.clone(), *by));
                    false
                }
                None => true,
            });
        }
        shadowed.sort_by_key(|(s, k, _)| (s.id(), k.clone()));
        let mut km = Keymap { map, by_key: HashMap::new(), shadowed };
        km.index();
        km
    }

    /// What the status line says at startup when keymap.json keeps a default key for another command, and how to give one back
    pub fn shadowed_notice(&self) -> Option<String> {
        let (s, key, by) = self.shadowed.first()?;
        let name = |s: &Slot| crate::app::title(s.action);
        Some(format!("{} stays on {} (your keymap.json), so {} has no key — {} › Keymap gives it one", disp(key), name(by), name(s), self.label(Action::Palette)))
    }

    fn index(&mut self) {
        let Keymap { map, by_key, .. } = self;
        by_key.clear();
        for (s, _) in COMMANDS.iter() {
            for k in map.get(s).into_iter().flatten() {
                by_key.entry((s.pane, k.clone())).or_insert(s.action);
            }
        }
    }

    /// The command a key runs anywhere
    pub fn lookup(&self, k: &KeyEvent) -> Option<Action> {
        self.lookup_in(Scope::Global, k)
    }

    /// The command a key runs in a pane (Scope::Tree / Scope::Grid) by its pane keys. ⌘ combos are never pane keys (the terminal reports
    /// them without ⌘: ⌘C would read as c)
    pub fn lookup_in(&self, pane: Scope, k: &KeyEvent) -> Option<Action> {
        if pane != Scope::Global && k.modifiers.contains(KeyModifiers::SUPER) {
            return None;
        }
        self.by_key.get(&(pane, spec(k)?)).copied()
    }

    /// An Action's keys anywhere
    pub fn keys(&self, a: Action) -> &[String] {
        self.slot_keys(Slot::global(a))
    }

    pub fn slot_keys(&self, s: Slot) -> &[String] {
        self.map.get(&s).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// For display ("F5 / Ctrl+Enter")
    pub fn label(&self, a: Action) -> String {
        self.slot_label(Slot::global(a))
    }

    /// For display; pane keys as typed ("n", "D")
    pub fn slot_label(&self, s: Slot) -> String {
        let k = self.slot_keys(s);
        if k.is_empty() {
            "—".into()
        } else if s.pane == Scope::Global {
            disp(&k.join(" / "))
        } else {
            k.iter().map(|k| pane_disp(k)).collect::<Vec<_>>().join(" / ")
        }
    }

    /// The palette's key column: the keys anywhere, else the pane keys ("n (explorer)"), else `fixed` (keys of the screen that run
    /// it without being a command key), else —
    pub fn hint(&self, a: Action, fixed: &str) -> String {
        if !self.keys(a).is_empty() {
            return self.label(a);
        }
        for pane in [Scope::Tree, Scope::Grid] {
            let s = Slot { action: a, pane };
            if !self.slot_keys(s).is_empty() {
                return format!("{} ({})", self.slot_label(s), pane_name(pane));
            }
        }
        if fixed.is_empty() { "—".into() } else { fixed.into() }
    }

    /// Whether the line differs from its defaults, not counting a default key keymap.json gives another command (`shadowed`): saving
    /// that as the user's own keys would keep the default away after the other command lets the key go
    pub fn is_custom(&self, s: Slot) -> bool {
        let mut base = defaults(s);
        base.retain(|k| !self.shadowed.iter().any(|(o, key, _)| *o == s && key == k));
        self.slot_keys(s) != base.as_slice()
    }

    /// The Keymap window's or the palette's command whose key this is, when a pane key may not take it: a pane key with a modifier or an
    /// F key takes it from the commands anywhere, and F1 (or ⌃k) gone would leave no way back into the F1 window
    pub fn reserved(&self, s: Slot, key: &str) -> Option<Action> {
        if s.pane == Scope::Global {
            return None;
        }
        [Action::Help, Action::Palette].into_iter().find(|&a| a != s.action && COMMANDS.iter().any(|(o, _)| o.action == a && self.slot_keys(*o).iter().any(|k| k == key)))
    }

    /// Assign a key (add = add to existing keys). Removed from any other line using the same key where both work (a key anywhere
    /// overlaps every pane) → the line it was removed from
    pub fn bind(&mut self, s: Slot, key: String, add: bool) -> Option<Slot> {
        self.shadowed.retain(|(o, ..)| *o != s);
        let taken = table::conflicts(&s, s.pane, &key, self.map.iter().map(|(o, v)| (*o, o.pane, v.as_slice())));
        for o in &taken {
            if let Some(v) = self.map.get_mut(o) {
                v.retain(|x| x != &key);
            }
        }
        let v = self.map.entry(s).or_default();
        if !add {
            v.clear();
        }
        if !v.contains(&key) {
            v.push(key);
        }
        self.index();
        taken.into_iter().next()
    }

    pub fn reset(&mut self, s: Slot) {
        self.shadowed.retain(|(o, ..)| *o != s);
        self.map.insert(s, defaults(s));
        self.index();
    }

    /// What to write to keymap.json: only lines differing from defaults
    pub fn overrides(&self) -> qurry_core::store::Keymap {
        COMMANDS.iter().filter(|(s, _)| self.is_custom(*s)).map(|(s, _)| (s.id(), self.slot_keys(*s).to_vec())).collect()
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

    /// A default key keymap.json gives another command isn't saved as the user's own empty line: once that command lets the key go,
    /// the default comes back
    #[test]
    fn shadowed_defaults_are_not_saved() {
        let user = qurry_core::store::Keymap::from([("Find".into(), vec!["Alt+Shift+F".into()])]);
        let mut km = Keymap::new(&user);
        assert!(km.keys(Action::Reformat).is_empty());
        assert!(!km.is_custom(Slot::global(Action::Reformat)));
        // Another change in the Keymap window saves everything custom
        km.bind(Slot::global(Action::NewConsole), "Ctrl+Alt+N".into(), false);
        let saved = km.overrides();
        assert!(!saved.contains_key("Reformat"), "{saved:?}");
        // Find lets ⌥⇧F go: Reformat Code has it again on the next start
        let mut next = Keymap::new(&saved);
        next.bind(Slot::global(Action::Find), "Ctrl+Alt+F".into(), false);
        assert_eq!(Keymap::new(&next.overrides()).keys(Action::Reformat), ["Alt+Shift+F"]);
        // A key the user gives Reformat Code itself is saved
        let mut km = Keymap::new(&user);
        km.bind(Slot::global(Action::Reformat), "Ctrl+Alt+R".into(), false);
        assert_eq!(km.overrides().get("Reformat").unwrap(), &vec!["Ctrl+Alt+R".to_string()]);
    }

    #[test]
    fn defaults_and_overrides() {
        let mut km = Keymap::new(&Default::default());
        assert_eq!(km.lookup(&ev(KeyCode::Char('l'), KeyModifiers::CONTROL)), Some(Action::NewConsole));
        // ⌥↩ fixes an underlined name, and runs anywhere else (App::quick_fix)
        assert_eq!(km.lookup(&ev(KeyCode::Enter, KeyModifiers::ALT)), Some(Action::QuickFix));
        assert!(km.overrides().is_empty());

        // Taking a key another command used removes it from that command
        assert_eq!(km.bind(Slot::global(Action::NewConsole), "Ctrl+K".into(), false), Some(Slot::global(Action::Palette)));
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

        km.reset(Slot::global(Action::Palette));
        km.reset(Slot::global(Action::NewConsole));
        assert!(km.overrides().is_empty());
    }

    /// Pane keys work in their pane only, are saved under "<pane>.<Action>", and a key anywhere takes the same key from a pane
    #[test]
    fn pane_keys_and_overrides() {
        let mut km = Keymap::new(&Default::default());
        let n = ev(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(km.lookup_in(Scope::Tree, &n), Some(Action::NewDataSource));
        assert_eq!(km.lookup_in(Scope::Grid, &n), Some(Action::SetNull));
        assert_eq!(km.lookup(&n), None);
        // ⌘C reaches the terminal as c: never a pane key (c is Connect in the explorer)
        assert_eq!(km.lookup_in(Scope::Tree, &ev(KeyCode::Char('c'), KeyModifiers::SUPER)), None);
        let tree = |a| Slot { action: a, pane: Scope::Tree };
        assert_eq!(km.slot_label(tree(Action::DeleteDataSource)), "D");
        assert_eq!(km.hint(Action::NewDataSource, ""), "n (explorer)");
        assert_eq!(km.hint(Action::Disconnect, ""), disp("Ctrl+F2"));
        // Rebinding a pane key: the old one stops, the new one is saved apart from the Action's own keys
        assert_eq!(km.bind(tree(Action::NewDataSource), "Shift+N".into(), false), None);
        assert_eq!(km.lookup_in(Scope::Tree, &n), None);
        let big_n = ev(KeyCode::Char('N'), KeyModifiers::SHIFT);
        assert_eq!(km.lookup_in(Scope::Tree, &big_n), Some(Action::NewDataSource));
        assert_eq!(km.overrides().get("tree.NewDataSource"), Some(&vec!["Shift+N".to_string()]));
        assert_eq!(Keymap::new(&km.overrides()).lookup_in(Scope::Tree, &big_n), Some(Action::NewDataSource));
        // A key anywhere overlaps the panes
        assert_eq!(km.bind(Slot::global(Action::ClearConsole), "Ctrl+Z".into(), false), Some(Slot { action: Action::Revert, pane: Scope::Grid }));
        assert!(bindable_for(tree(Action::Connect), "Shift+C") && !bindable_for(tree(Action::Connect), "J") && !bindable_for(Slot::global(Action::Connect), "C"));
    }

    /// Every Action has a row in the shared table (its name on screen), and every TUI row is an Action (that every Action has one
    /// ActionDef is checked in tests/actions.rs)
    #[test]
    fn every_action_has_a_table_row() {
        let ids: Vec<String> = crate::app::ACTIONS.iter().map(|d| action_id(d.action)).collect();
        for id in &ids {
            assert!(table::tui_command(id).is_some(), "Action::{id} has no row in crates/shared/src/keymap-table.json: add one with \"tui\": \"{id}\"");
        }
        for c in table::commands() {
            if let Some(id) = &c.tui {
                assert!(ids.contains(id), "keymap-table.json row \"tui\": \"{id}\" is not an Action in the palette (app/actions.rs ACTIONS): fix the name or add the Action");
            }
        }
        assert_eq!(COMMANDS.iter().find(|d| d.0 == Slot::global(Action::NewConsole)).map(|d| d.1), Some(&["Ctrl+L".to_string()][..]));
        // Every Action is in the F1 window once with its keys anywhere: rebindable, with or without a default key
        for d in crate::app::ACTIONS {
            assert_eq!(COMMANDS.iter().filter(|c| c.0 == Slot::global(d.action)).count(), 1, "{:?}", d.action);
        }
    }

    #[test]
    fn order_lists_every_table_command_with_keys_once() {
        let mut want: Vec<&str> = table::commands().iter().filter(|c| !c.tui_keys.is_empty()).filter_map(|c| c.tui.as_deref()).collect();
        let mut got: Vec<String> = ORDER.iter().map(|a| action_id(*a)).collect();
        want.sort_unstable();
        got.sort_unstable();
        assert_eq!(got, want, "keys.rs ORDER lists each TUI row of keymap-table.json that has tuiKeys, once");
    }
}
