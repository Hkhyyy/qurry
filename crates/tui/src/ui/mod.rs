//! Screen drawing
use crate::keys::disp;
use crate::app::{App, Drag, DropTarget, FieldKind, Focus, Popup, SERVER_WARNING, SplitDir, TabKind, WARNING};
use crate::tree::NodeKind;
use qurry_core::keymap::Scope;
use qurry_core::statusbar::{self, Segment};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use serde_json::Value;

pub use crate::kit::*;

mod tree;
mod tabs;
mod editor;
mod completion;
mod redis;
mod status_items;
mod popup;
mod form;
mod pane;
use self::tree::{draw_tree, draw_workspaces, object_glyph};
use self::tabs::{draw_tabs, tab_glyph};
use self::editor::{find_caret, draw_sql_colors, draw_marks, draw_names, draw_unbalanced, draw_hint, draw_preview, draw_run_error, CARET_MARK, draw_result_strip, value_split};
use self::completion::draw_completion;
pub use self::redis::{format_reply, format_typed, type_color};
use self::status_items::draw_status;
use self::popup::{draw_popup, fit_rows};
use self::form::draw_conn_form;
use self::pane::draw_pane;

/// Data Source colors, same as the GUI (src/lib/api.ts COLOR_PRESETS)
pub const COLOR_PRESETS: &[(&str, &str)] = &[
    ("red", "#6e2a2a"),
    ("orange", "#6e4a22"),
    ("yellow", "#5f5b24"),
    ("green", "#2f5634"),
    ("teal", "#22524f"),
    ("blue", "#28406e"),
    ("violet", "#4f2f68"),
    ("gray", "#45474d"),
];

/// Color name shown in the form (custom if not a preset)
pub fn color_name(c: Option<&str>) -> &'static str {
    match c {
        None => "none",
        Some(h) => COLOR_PRESETS.iter().find(|p| p.1.eq_ignore_ascii_case(h)).map(|p| p.0).unwrap_or("custom"),
    }
}

/// Smallest screen the layout is drawn for (explorer · tabs · status line still readable); below it, a message instead
pub const MIN_SIZE: (u16, u16) = (40, 10);

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.too_small = area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1;
    if app.too_small {
        draw_too_small(f, app);
    } else {
        draw_screen(f, app);
    }
    sanitize(f.buffer_mut());
    app.color.apply(f.buffer_mut(), &crate::theme::Look::of(&app.themes, app.auto_light));
}

/// Says the size and the size needed, centered and wrapped so it fits even a few cells
fn draw_too_small(f: &mut Frame, app: &App) {
    let a = f.area();
    if a.is_empty() {
        return;
    }
    let last = match app.quit_waits_for() {
        Some(why) => Line::styled(why, Style::default().fg(WARN)),
        None => Line::from(muted(format!("Make the window larger · {} quits", app.keys.label(crate::app::Action::Quit)))),
    };
    let lines = vec![
        Line::styled("Terminal too small", Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
        Line::raw(format!("{}×{} — needs {}×{}", a.width, a.height, MIN_SIZE.0, MIN_SIZE.1)),
        last,
    ];
    let p = Paragraph::new(lines).alignment(Alignment::Center).wrap(Wrap { trim: true });
    let h = (p.line_count(a.width) as u16).min(a.height);
    f.render_widget(p, Rect { y: a.y + (a.height - h) / 2, height: h, ..a });
}

fn draw_screen(f: &mut Frame, app: &mut App) {
    let [main, status] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(f.area());
    // With two or more workspaces, one line at the top: 1 Default │ 2 Reports (Alt+n · click)
    app.hits.workspaces.clear();
    app.hits.text = Rect::default();
    app.hits.run = Rect::default();
    app.hits.cancel = Rect::default();
    let main = if app.workspaces.len() > 1 {
        let [bar, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).areas(main);
        draw_workspaces(f, app, bar);
        rest
    } else {
        main
    };
    app.hits.main = main;
    app.hits.explorer_divider = Rect::default();
    let right = if app.explorer {
        let w = app.explorer_width(main.width);
        let [l, r] = Layout::horizontal([Constraint::Length(w), Constraint::Min(10)]).areas(main);
        draw_tree(f, app, l);
        // Two cells up to the one touching the tree's right border (drag to resize; one cell is hard to hit)
        app.hits.explorer_divider = Rect { x: l.right().saturating_sub(1), y: l.y, width: 2, height: l.height };
        r
    } else {
        main
    };
    draw_tabs(f, app, right);
    draw_status(f, app, status);
    if app.designer.is_some() {
        crate::designer::draw(f, app, f.area());
    }
    if app.popup.is_some() {
        draw_popup(f, app);
    }
}

/// Whether a char can control the terminal: C0 · DEL · C1 (ESC sequences, OSC 52 clipboard, screen clear …) and text-direction overrides
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Strip such chars from the drawn screen. Done once at the end, whether they came from DB values · names · errors · the status line (ratatui's Span doesn't filter them)
fn sanitize(buf: &mut ratatui::buffer::Buffer) {
    for cell in buf.content.iter_mut() {
        if cell.symbol().chars().any(is_unsafe_char) {
            let clean: String = cell.symbol().chars().filter(|c| !is_unsafe_char(*c)).collect();
            cell.set_symbol(if clean.is_empty() { " " } else { &clean });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Below the minimum size: a message with the size needed instead of a broken layout (narrower than 24 cells used to panic)
    #[tokio::test]
    async fn too_small_terminal_says_so_instead_of_drawing_the_layout() {
        let mut h = crate::testkit::Harness::new().await;
        h.table_tab("payment", &["id", "amount"], &[&["1", "2"]], &["id"]);
        let s = h.screen(39, 10);
        assert!(s.contains("Terminal too small") && s.contains("39×10 — needs 40×10"), "{s}");
        assert!(s.contains("Make the window larger · ⌃q quits"), "{s}");
        h.app.conn_form(None);
        assert!(!h.screen(39, 10).contains("New Data Source"));
        assert!(h.screen(80, 9).contains("80×9 — needs 40×10"));
        // At the minimum the layout is back
        assert!(!h.screen(40, 10).contains("too small"));
        // Tiny screens don't panic; the message wraps
        for (w, ht) in [(1, 1), (10, 3), (23, 24), (24, 5)] {
            h.screen(w, ht);
        }
        assert!(h.screen(20, 6).contains("Terminal too"));
    }

    /// Under the message keys and clicks would act on a screen that isn't drawn: only the quit key works
    #[tokio::test]
    async fn too_small_terminal_ignores_keys_and_clicks_but_quit() {
        let mut h = crate::testkit::Harness::new().await;
        let cols: Vec<String> = (0..30).map(|i| format!("column_{i}")).collect();
        let cols: Vec<&str> = cols.iter().map(|c| c.as_str()).collect();
        h.table_tab("wide", &cols, &[&["1"; 30]], &["column_0"]);
        h.app.focus = crate::app::Focus::Grid;
        h.screen(120, 30);
        let g = h.app.grid_mut().unwrap();
        g.col_off = 10;
        let bar = g.hbar;
        assert!(bar.width > 0);
        // A confirm that arrives (or was open) when the window shrinks
        h.app.popup = Some(crate::app::Popup::Confirm { opened: None, sql: None, text: "Run it?".into(), on_yes: Box::new(|a| a.status = "ran".into()) });
        assert!(h.screen(39, 30).contains("Terminal too small"));
        h.key("Enter");
        h.key("y");
        h.paste("x");
        assert!(h.app.popup.is_some() && h.app.status != "ran");
        h.app.popup = None;
        h.mouse("down", bar.x + 6, bar.y);
        h.key("Right");
        let g = h.app.grid_mut().unwrap();
        assert_eq!((g.col_off, g.col), (10, 0), "the hidden grid doesn't scroll or move");
        assert!(h.app.drag.is_none());
        // Back at full size keys act again; the quit key works under the message too
        h.screen(120, 30);
        h.key("Right");
        assert_eq!(h.app.grid_mut().unwrap().col, 1);
        h.screen(39, 30);
        h.key("Ctrl+Q");
        assert!(h.app.quit);
    }

    /// Under the message the quit key never answers or replaces a question nobody can see: the message says what waits
    #[tokio::test]
    async fn too_small_terminal_quit_waits_for_open_questions() {
        let mut h = crate::testkit::Harness::new().await;
        // A half-filled form stays as it is
        h.app.conn_form(None);
        h.type_str("half");
        let s = h.screen(100, 9);
        assert!(s.contains("A dialog is open: make the window larger to answer it") && !s.contains("quits"), "{s}");
        h.key("Ctrl+Q");
        assert!(!h.app.quit);
        assert!(matches!(&h.app.popup, Some(Popup::ConnForm(f)) if f.fields[0].value == "half"));
        h.app.popup = None;
        // Uncommitted changes: no hidden 'Roll them back and quit?' that y or Enter could answer
        h.console_tab();
        if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.get_mut(h.app.active).map(|t| &mut t.kind) {
            c.in_tx = true;
        }
        let s = h.screen(100, 9);
        assert!(s.contains("1 console(s) have uncommitted changes: make the window larger to quit"), "{s}");
        for k in ["Ctrl+Q", "y", "Enter"] {
            h.key(k);
        }
        assert!(!h.app.quit && h.app.popup.is_none());
        // Larger again: the quit key asks, and the answer is seen
        h.screen(120, 30);
        h.key("Ctrl+Q");
        assert!(h.screen(120, 30).contains("Roll them back and quit?"));
        h.key("y");
        assert!(h.app.quit);
    }

    /// The startup notices fit an 80-column status line whole, with a gap before the key hints: one at a time, the light
    /// palette's first (unless NO_COLOR, which draws no palette)
    #[tokio::test]
    async fn color_notices_fit_80_columns() {
        use crate::theme::ColorMode;
        let mut h = crate::testkit::Harness::new().await;
        let light = "Light palette — ⌃k › Theme… › Dark to undo";
        for (mode, auto_light, want) in [
            (ColorMode::Ansi16, false, "16 colors: try COLORTERM=truecolor"),
            (ColorMode::NoColor, false, "Colors off: unset NO_COLOR"),
            (ColorMode::TrueColor, true, light),
            (ColorMode::Ansi16, true, light),
            (ColorMode::NoColor, true, "Colors off: unset NO_COLOR"),
        ] {
            (h.app.color, h.app.auto_light) = (mode, auto_light);
            assert_eq!(h.app.startup_notice().as_deref(), Some(want));
            h.app.status = want.into();
            let s = h.screen(80, 24);
            assert!(s.lines().last().unwrap().contains(&format!("{want} ")), "{s}");
        }
        (h.app.color, h.app.auto_light) = (ColorMode::TrueColor, false);
        assert_eq!(h.app.startup_notice(), None);
    }

    /// NO_COLOR: text selected in the editor stays visible (tui-textarea draws it on a color, which NO_COLOR drops)
    #[tokio::test]
    async fn no_color_shows_selected_text_in_reverse_video() {
        let mut h = crate::testkit::Harness::new().await;
        h.console_with("select 12345 from t", 0, 0);
        h.app.focus = crate::app::Focus::Editor;
        for _ in 0..6 {
            h.key("Shift+Right");
        }
        assert_eq!(h.console_cursor().1.as_deref(), Some("select"));
        h.app.color = crate::theme::ColorMode::NoColor;
        let buf = h.buffer(100, 30);
        let (x, y) = (0..30).find_map(|y| {
            let l: String = (0..100).map(|x| buf[(x, y)].symbol()).collect();
            l.find("select 12345").map(|i| (l[..i].chars().count() as u16, y))
        }).unwrap();
        assert!((0..6).all(|i| buf[(x + i, y)].modifier.contains(Modifier::REVERSED)), "selected text is reversed");
        assert!(!buf[(x + 7, y)].modifier.contains(Modifier::REVERSED), "the rest isn't");
    }
}
