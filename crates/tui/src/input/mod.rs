//! Key input → actions (DataGrip keymap adapted for the terminal)
use crate::app::{Action, App, Drag, DropTarget, FieldKind, Focus, Popup, SplitDir, TabKind};
use crate::tree::NodeKind;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use std::time::Instant;

mod clipboard;
mod editor;
mod form;
mod grid;
mod mouse;
mod popup;
mod redis;
mod tree;

pub use clipboard::copy;
use editor::editor_key;
use form::{save_form, test_form};
use grid::{grid_key, nav};
pub use mouse::drop_target;
use mouse::{hscroll_at, mouse_down, mouse_drag, mouse_up, over_hbar, scroll_at};
use popup::popup_key;
use redis::redis_key;
use tree::tree_key;

pub fn on_event(app: &mut App, ev: Event) {
    // Under the 'terminal too small' message only the quit key acts (an unseen confirm could be answered, an unseen grid scrolled),
    // and only when nothing needs an answer first (the message says what waits for a larger window)
    if app.too_small {
        if let Event::Key(k) = ev
            && k.kind != KeyEventKind::Release
            && app.keys.lookup(&k) == Some(Action::Quit)
            && app.quit_waits_for().is_none()
        {
            app.run_action(Action::Quit);
        }
        return;
    }
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => on_key(app, k),
        Event::Paste(s) => paste(app, s),
        Event::Mouse(m) => match m.kind {
            // Shift+wheel = horizontal scroll
            MouseEventKind::ScrollDown if m.modifiers.contains(KeyModifiers::SHIFT) => hscroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::SHIFT) => hscroll_at(app, m.column, m.row, -3),
            // ⌥+wheel, or wheel over the grid header·scrollbar = horizontal scroll (for terminals that don't pass the horizontal wheel)
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::ALT) || over_hbar(app, m.column, m.row) => {
                hscroll_at(app, m.column, m.row, if m.kind == MouseEventKind::ScrollDown { 1 } else { -1 })
            }
            MouseEventKind::ScrollDown => scroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollUp => scroll_at(app, m.column, m.row, -3),
            MouseEventKind::ScrollRight => hscroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollLeft => hscroll_at(app, m.column, m.row, -3),
            MouseEventKind::Down(MouseButton::Left) => mouse_down(app, m.column, m.row),
            MouseEventKind::Drag(MouseButton::Left) => mouse_drag(app, m.column, m.row),
            MouseEventKind::Up(MouseButton::Left) => mouse_up(app, m.column, m.row),
            _ => {}
        },
        _ => {}
    }
    // A confirm that waited for the popup the user was in comes up once that one is closed (quiet, so the closing key doesn't answer it)
    if app.popup.is_none() && !app.waiting.is_empty() {
        let p = app.waiting.remove(0);
        app.open_after_notices(p);
    }
}

/// ⌘V / terminal paste: into the open input (single-line input·palette·Data Source form) if any, otherwise the console editor
fn paste(app: &mut App, s: String) {
    // Terminals (xterm.js, tmux, etc.) may send newlines as \r. A \r left in the editor breaks the display
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    // Newlines become spaces in single-line inputs
    let one = || s.trim_end_matches(['\r', '\n']).replace("\r\n", " ").replace(['\r', '\n'], " ");
    if matches!(app.popup, Some(Popup::Find(_))) {
        return app.find_paste(&s);
    }
    match &mut app.popup {
        Some(Popup::Input { input, .. }) => {
            input.insert_str(one());
        }
        Some(Popup::Palette(p)) => {
            if p.input.insert_str(one()) {
                p.sel = 0;
            }
        }
        Some(Popup::ConnForm(form)) => {
            let f = &mut form.fields[form.sel];
            if matches!(f.kind, FieldKind::Text | FieldKind::Secret) {
                if f.value == "<saved>" {
                    f.value.clear();
                }
                f.value.push_str(&one());
            }
        }
        Some(_) => {}
        None => match app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            Some(TabKind::Console(c)) if app.focus == Focus::Editor => {
                c.editor.insert_str(s);
            }
            // Table data: fill from the current cell (⌘V)
            Some(TabKind::Table(tt)) if app.focus == Focus::Grid => {
                if let Some(g) = tt.grid.as_mut().filter(|g| g.cols() > 0) {
                    g.paste(&s);
                }
            }
            _ => {}
        },
    }
}

fn scroll(app: &mut App, d: isize) {
    match app.focus {
        Focus::Tree => {
            let n = app.tree.visible().len();
            app.tree.sel = (app.tree.sel as isize + d).clamp(0, n.saturating_sub(1) as isize) as usize;
        }
        _ => {
            if let Some(g) = app.grid_mut() {
                g.move_by(d, 0);
            }
            app.fetch_more();
        }
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.popup.is_some() {
        return popup_key(app, k);
    }
    if app.designer.is_some() {
        return crate::designer::key(app, k);
    }
    // ── Global commands (keymap: keys.rs, rebind: F1) ──
    if let Some(a) = app.keys.lookup(&k) {
        // Run key in the explorer = refresh
        if a == Action::Execute && app.focus == Focus::Tree {
            return app.run_action(Action::Refresh);
        }
        return app.run_action(a);
    }
    // Alt+1~9: workspaces (if the keymap binds that key to another command, that command wins)
    if let KeyCode::Char(c @ '1'..='9') = k.code
        && k.modifiers == KeyModifiers::ALT
    {
        return app.ws_switch(c as usize - '1' as usize);
    }
    match app.focus {
        Focus::Tree => tree_key(app, k),
        Focus::Editor => editor_key(app, k),
        Focus::Grid => grid_key(app, k),
    }
}

/// Filter candidates by the word before the cursor (ignoring case·quotes, skipping ones already fully typed, up to 8)
fn input_complete(input: &tui_textarea::TextArea, words: &[String], force: bool) -> Option<(Vec<String>, usize, usize)> {
    if words.is_empty() {
        return None;
    }
    let (row, col) = input.cursor();
    let before: Vec<char> = input.lines().get(row)?.chars().take(col).collect();
    let n = before.iter().rev().take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '"' || **c == '`').count();
    if n == 0 && !force {
        return None;
    }
    let p: String = before[before.len() - n..].iter().collect::<String>().trim_start_matches(['"', '`']).to_lowercase();
    let items: Vec<String> = words
        .iter()
        .filter(|w| {
            let w = w.trim_matches(['"', '`']).to_lowercase();
            w.starts_with(&p) && w != p
        })
        .take(8)
        .cloned()
        .collect();
    (!items.is_empty()).then_some((items, 0, n))
}

/// For screens with single-letter commands (a add row, d delete …): ⌘←/→ are Home/End (terminals may send them as ⌃A/⌃E), ⌘C is y (copy),
/// other letters pressed with ⌃/⌥/⌘ are not taken as commands (fixes ⌘← arriving as ⌃A and adding a row)
fn plain(k: KeyEvent) -> Option<KeyEvent> {
    let m = k.modifiers;
    let to = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
    match k.code {
        KeyCode::Char('a') if m == KeyModifiers::CONTROL => to(KeyCode::Home),
        KeyCode::Char('e') if m == KeyModifiers::CONTROL => to(KeyCode::End),
        KeyCode::Left if m.contains(KeyModifiers::SUPER) => to(KeyCode::Home),
        KeyCode::Right if m.contains(KeyModifiers::SUPER) => to(KeyCode::End),
        // ⌘C = y (copy)
        KeyCode::Char('c') if m == KeyModifiers::SUPER => to(KeyCode::Char('y')),
        // ⌃Z = undo edit
        KeyCode::Char('z') if m == KeyModifiers::CONTROL => Some(k),
        KeyCode::Char(_) if m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => None,
        _ => Some(k),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_letters_are_not_commands() {
        let k = |c, m| KeyEvent::new(c, m);
        // ⌘← arriving as ⌃A goes to the first column instead of adding a row
        assert_eq!(plain(k(KeyCode::Char('a'), KeyModifiers::CONTROL)).map(|x| x.code), Some(KeyCode::Home));
        assert_eq!(plain(k(KeyCode::Right, KeyModifiers::SUPER)).map(|x| x.code), Some(KeyCode::End));
        assert!(plain(k(KeyCode::Char('d'), KeyModifiers::ALT)).is_none());
        assert!(plain(k(KeyCode::Char('n'), KeyModifiers::SUPER)).is_none());
        assert_eq!(plain(k(KeyCode::Char('D'), KeyModifiers::SHIFT)).map(|x| x.code), Some(KeyCode::Char('D')));
        assert_eq!(plain(k(KeyCode::Char('z'), KeyModifiers::CONTROL)).map(|x| x.code), Some(KeyCode::Char('z')));
    }

    // ───────── Check by feeding keys (testkit) ─────────
    use crate::testkit::Harness;

    /// Table tabs with the same name: schema.table if schemas differ, db.schema.table if the schemas are the same too
    #[tokio::test]
    async fn same_table_tabs_show_schema() {
        let mut h = Harness::new().await;
        let a = h.table_tab("orders", &["id"], &[], &[]);
        let label = |h: &Harness, id: u64| {
            let t = h.app.tabs.iter().find(|t| t.id == id).unwrap();
            h.app.tab_label(t)
        };
        assert_eq!(label(&h, a), "orders");
        let b = h.table_tab("orders", &["id"], &[], &[]);
        if let Some(TabKind::Table(tt)) = h.app.tabs.iter_mut().find(|t| t.id == b).map(|t| &mut t.kind) {
            tt.schema = "billing".into();
        }
        assert_eq!((label(&h, a), label(&h, b)), ("public.orders".into(), "billing.orders".into()));
        let c = h.table_tab("orders", &["id"], &[], &[]);
        h.app.tabs.iter_mut().find(|t| t.id == c).unwrap().db = "app_test".into();
        assert_eq!((label(&h, a), label(&h, c), label(&h, b)), ("postgres.public.orders".into(), "app_test.public.orders".into(), "billing.orders".into()));
    }

    /// Recent Tabs lists all open tabs (including restored tabs not yet viewed). Most recently viewed first
    #[tokio::test]
    async fn recent_tabs_lists_every_open_tab() {
        let mut h = Harness::new().await;
        let a = h.table_tab("a", &["x"], &[], &[]);
        h.table_tab("b", &["x"], &[], &[]);
        h.app.restore_table("test", "postgres".into(), "public".into(), "c".into(), "table".into(), String::new(), String::new());
        h.app.activate(0);
        h.app.palette_tabs();
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
        let labels: Vec<_> = p.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["a", "b", "c"]);
        assert!(matches!(p.items[0].action, crate::app::PAction::Tab(id) if id == a));
    }

    /// Terminals may send pasted newlines as \r (xterm.js, tmux): lines are kept, no \r remains
    #[tokio::test]
    async fn paste_into_console_keeps_lines() {
        let mut h = Harness::new().await;
        h.console_tab();
        h.paste("BEGIN;\r\r-- 1. 스키마\rGRANT USAGE\r\n  ON x;\rCOMMIT;\r");
        assert_eq!(h.console_text(), "BEGIN;\n\n-- 1. 스키마\nGRANT USAGE\n  ON x;\nCOMMIT;\n");
    }

    /// Left/right split: there's a separator between the two panes' tab bars (so they don't look like one continuous line)
    #[tokio::test]
    async fn split_tab_bars_are_separated() {
        let mut h = Harness::new().await;
        h.table_tab("left_table", &["id"], &[&["1"]], &["id"]);
        h.table_tab("right_table", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::SplitRight);
        let s = h.screen(160, 20);
        let bar = s.lines().find(|l| l.contains("left_table") && l.contains("right_table")).unwrap_or_else(|| panic!("{s}"));
        let between = &bar[bar.find("left_table").unwrap()..bar.find("right_table").unwrap()];
        assert!(between.contains('┃'), "{bar}");
    }

    /// Closing a tab in a split view goes to the neighboring tab in the same pane (no jump to the first pane even if only unviewed tabs, like restored ones, remain)
    #[tokio::test]
    async fn closing_a_tab_stays_in_its_pane() {
        let mut h = Harness::new().await;
        h.table_tab("left", &["id"], &[&["1"]], &["id"]);
        let right_a = h.table_tab("right_a", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::SplitRight);
        h.table_tab("right_b", &["id"], &[&["1"]], &["id"]);
        h.table_tab("right_c", &["id"], &[&["1"]], &["id"]);
        // Restored tabs aren't in the recent list
        h.app.mru.retain(|x| *x != right_a);
        h.key("Ctrl+w");
        assert_eq!(h.app.tabs[h.app.active].title, "right_b");
        h.key("Ctrl+w");
        assert_eq!(h.app.tabs[h.app.active].title, "right_a");
    }

    /// Control characters (ESC, C1, direction overrides) in DB values · names · errors · the status line don't reach the terminal
    #[tokio::test]
    async fn control_characters_never_reach_the_terminal() {
        let mut h = Harness::new().await;
        let evil = "a\u{1b}]52;c;cm0gLXJmIH4=\u{7}b\u{1b}[2Jc\u{9b}31md\u{202e}gpj.exe";
        h.table_tab(evil, &[evil, "id"], &[&[evil, "1"]], &[]);
        h.app.status = format!("Copied: {evil}");
        let s = h.screen(160, 20);
        let bad: Vec<char> = s.chars().filter(|c| (c.is_control() && *c != '\n') || ('\u{202a}'..='\u{202e}').contains(c) || ('\u{2066}'..='\u{2069}').contains(c)).collect();
        assert!(bad.is_empty(), "{bad:?}");
        assert!(s.contains("]52;c;"), "the rest of the text is still shown");
    }

    /// ⌃K › Chart Result: result table as horizontal bars (popup)
    #[tokio::test]
    async fn chart_result_popup() {
        let mut h = Harness::new().await;
        h.table_tab("sales", &["region", "amount"], &[&["Seoul", "120"], &["Busan", "40"], &["Seoul", "30"]], &[]);
        h.app.run_action(Action::ChartResult);
        let screen = h.screen(100, 24);
        assert!(screen.contains("Seoul │") && screen.contains("█ 150") && screen.contains("Busan │"), "{screen}");
    }

    /// Bottom hints: each focus's screen keys + common global keys. When narrow, screen keys are trimmed and global keys stay
    #[tokio::test]
    async fn status_bar_guides_by_focus() {
        use crate::keys::disp;
        let mut h = Harness::new().await;
        let last = |s: &str| s.lines().last().unwrap().to_string();
        // Explorer
        let s = last(&h.screen(220, 20));
        for want in ["Enter open", "F4 console", "s schemas", "/ filter", &disp("Ctrl+P search"), &disp("Ctrl+K actions")] {
            assert!(s.contains(want), "tree: {want:?} in {s}");
        }
        // Table data
        let id = h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
        h.app.focus = crate::app::Focus::Grid;
        let s = last(&h.screen(220, 20));
        for want in ["Enter edit", "a add", "d delete", &disp("Ctrl+S submit"), "w where", &disp("Ctrl+L console"), &disp("Ctrl+E tabs"), &disp("Ctrl+K actions")] {
            assert!(s.contains(want), "table: {want:?} in {s}");
        }
        // Narrow screen: global keys remain
        let s = last(&h.screen(90, 20));
        assert!(s.contains(&disp("Ctrl+K actions")), "narrow: {s}");
        // Console editor
        h.console_tab();
        let s = last(&h.screen(220, 20));
        for want in [&disp("F5 run"), &disp("Ctrl+Space complete"), &disp("⇧Tab result"), &disp("Ctrl+E tabs"), &disp("Ctrl+P search")] {
            assert!(s.contains(want), "editor: {want:?} in {s}");
        }
    }

    /// First screen and F1: if unsure, start with ⌃P
    #[tokio::test]
    async fn start_here_points_to_search() {
        use crate::keys::disp;
        let mut h = Harness::new().await;
        assert!(h.screen(140, 30).contains(&disp("Not sure? Press Ctrl+P")));
        h.key("F1");
        let s = h.screen(140, 40);
        let start = s.find("Start here").expect("F1 starts with Start here");
        assert!(start < s.find("Commands").unwrap());
        assert!(s.contains("Search everything"));
    }

    /// Rebinding a key updates on-screen hints too (console Output/result titles, table save hint, palette hints)
    #[tokio::test]
    async fn labels_follow_remapped_keys() {
        use crate::app::Action;
        use crate::keys::disp;
        let mut h = Harness::new().await;
        h.app.keys.bind(Action::ToggleOutput, "Alt+P".into(), false);
        h.app.keys.bind(Action::Submit, "Ctrl+G".into(), false);
        h.app.keys.bind(Action::Execute, "F9".into(), false);
        h.app.keys.bind(Action::Commit, "Alt+M".into(), false);
        h.console_tab();
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Alt+P: result")), "{s}");
        assert!(s.contains("(F9)"), "{s}");
        assert!(!s.contains(&disp("Alt+O")), "{s}");
        let id = h.table_tab("t", &["id", "name"], &[&["1", "a"]], &["id"]);
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
        if let crate::app::TabKind::Table(tt) = &mut h.app.tabs[i].kind {
            tt.grid.as_mut().unwrap().set_value(0, 1, "b".into());
        }
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Ctrl+G submit")), "{s}");
        assert!(!s.contains(&disp("Ctrl+S")), "{s}");
        // Palette names·hints use the same notation (Ctrl+Z → ⌃z)
        h.key("Ctrl+K");
        h.type_str("Revert Changes");
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Ctrl+Z (grid)")), "{s}");
    }

    /// Move Split Divider moves the divider even with only a top/bottom split (named "Left / Up")
    #[tokio::test]
    async fn split_divider_moves_in_top_bottom_split() {
        use crate::app::Action;
        let mut h = Harness::new().await;
        h.table_tab("a", &["id"], &[&["1"]], &["id"]);
        h.table_tab("b", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(Action::SplitDown);
        let pct = |h: &mut Harness| h.app.panes.pct_mut(&vec![]).map(|p| *p);
        let before = pct(&mut h).unwrap();
        h.app.run_action(Action::SplitGrow);
        assert_eq!(pct(&mut h), Some(before + 5));
        h.app.run_action(Action::SplitShrink);
        assert_eq!(pct(&mut h), Some(before));
    }

    /// In table data, Explain only shows a hint instead of reloading (Explain is for the query console)
    #[tokio::test]
    async fn explain_on_table_tab_does_not_reload() {
        let mut h = Harness::new().await;
        let id = h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::Explain);
        assert!(!h.table(id).running);
        assert!(h.app.status.contains("query console"), "{}", h.app.status);
    }

    /// Clear the whole console: select all with ⌘A then ⌫, or ⌃K › Clear Console
    #[tokio::test]
    async fn console_select_all_and_clear() {
        let mut h = Harness::new().await;
        h.console_with("select 1;\nselect 2;", 1, 3);
        h.key("Cmd+a");
        h.key("Backspace");
        assert_eq!(h.console_text(), "");
        h.paste("select 3;");
        h.app.run_action(crate::app::Action::ClearConsole);
        assert_eq!(h.console_text(), "");
        // Unknown ⌘ combos don't insert text
        h.key("Cmd+x");
        assert_eq!(h.console_text(), "");
    }

    /// ⌘V into a table (terminal paste): fills from the current cell. Tabs·lines span multiple cells, missing rows become new rows
    #[tokio::test]
    async fn paste_into_table_cells() {
        let mut h = Harness::new().await;
        let id = h.table_tab("fee_rule", &["idx", "fee_type", "memo"], &[&["7", "PERCENT", ""], &["8", "FIXED", ""]], &["idx"]);
        h.key("Right");
        h.paste("FIXED");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!(g.value(0, 1).as_deref(), Some("FIXED"));
        // A comma within a line is still one cell value
        h.key("Right");
        h.paste("a, b");
        assert_eq!(h.table(id).grid.as_ref().unwrap().value(0, 2).as_deref(), Some("a, b"));
        // Multiple cells·lines (copied from a spreadsheet): extra columns are dropped, missing rows become new rows
        h.key("Left");
        h.key("Down");
        h.paste("RATE\tx\tdropped\r\nPERCENT\ty\r\n");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!((g.value(1, 1).as_deref(), g.value(1, 2).as_deref()), (Some("RATE"), Some("x")));
        assert_eq!(g.added, 1);
        assert_eq!((g.value(2, 1).as_deref(), g.value(2, 2).as_deref()), (Some("PERCENT"), Some("y")));
        // Same as the original value is not an edit
        h.key("Up");
        h.paste("PERCENT");
        assert!(!h.table(id).grid.as_ref().unwrap().edits.contains_key(&(0, 1)));
    }

    /// Text tabs (diagram·DDL): wheel scrolls, not past the end, g/G for start·end
    /// Console title bar while running: elapsed time · [■ Cancel ⌃c], clicking sends a cancel
    #[tokio::test]
    async fn running_console_shows_time_and_a_cancel_button() {
        let mut h = Harness::new().await;
        let id = h.console_tab();
        if let Some(TabKind::Console(c)) = h.app.tab_mut(id).map(|t| &mut t.kind) {
            c.running = true;
        }
        let screen = h.screen(140, 30);
        assert!(screen.contains("running 0.0 s · [■ Cancel"), "{screen}");
        let r = h.app.hits.cancel;
        assert!(r.width > 8, "{r:?}");
        let row = screen.lines().nth(r.y as usize).unwrap();
        assert!(row.chars().skip(r.x as usize).collect::<String>().starts_with("[■ Cancel"), "{row}");
        crate::input::on_event(&mut h.app, Event::Mouse(crossterm::event::MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: r.x + 2, row: r.y, modifiers: KeyModifiers::NONE }));
        h.settle().await;
        assert!(!h.app.status.is_empty() && h.app.status != "Executing…", "cancel was sent: {}", h.app.status);
        // The button goes away when finished
        if let Some(TabKind::Console(c)) = h.app.tab_mut(id).map(|t| &mut t.kind) {
            c.running = false;
        }
        h.screen(140, 30);
        assert_eq!(h.app.hits.cancel, ratatui::layout::Rect::default());
    }

}
