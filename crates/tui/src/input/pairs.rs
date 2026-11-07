//! Quotes and brackets in the console editor and the table tab's WHERE / ORDER BY inputs: qurry_shared::pairs, the rule of the
//! GUI's filter fields (and of its Monaco console). ' " ( [ close themselves, a closing one types over, ⌫ in an empty pair removes
//! both, around a selection they wrap it
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use qurry_core::store::DbKind;
use qurry_shared::pairs;
use tui_textarea::{CursorMove, TextArea};

/// Handles `k` when it edits a pair (true), otherwise leaves the text alone for the usual input
pub(super) fn pair_key(t: &mut TextArea, k: &KeyEvent, kind: DbKind) -> bool {
    if k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return false;
    }
    let text = t.lines().join("\n");
    let caret = offset(t.lines(), t.cursor());
    let edit = match k.code {
        KeyCode::Backspace if t.selection_range().is_none() => pairs::backspace(&text, caret, kind),
        KeyCode::Char(ch) => {
            let sel = t.selection_range().map_or((caret, caret), |(a, b)| (offset(t.lines(), a), offset(t.lines(), b)));
            pairs::typed(&text, sel, ch, kind)
        }
        _ => None,
    };
    let Some(e) = edit else { return false };
    let new = format!("{}{}{}", &text[..e.start], e.insert, &text[e.end..]);
    t.cancel_selection();
    jump(t, &text, e.start);
    if e.end > e.start {
        t.start_selection();
        jump(t, &text, e.end);
    }
    t.insert_str(&e.insert);
    t.cancel_selection();
    jump(t, &new, e.sel.0);
    if e.sel.1 != e.sel.0 {
        t.start_selection();
        jump(t, &new, e.sel.1);
    }
    true
}

/// (row, column in chars) → byte offset in the lines joined with \n
pub fn offset(lines: &[String], (row, col): (usize, usize)) -> usize {
    let before: usize = lines[..row].iter().map(|l| l.len() + 1).sum();
    before + lines[row].char_indices().nth(col).map_or(lines[row].len(), |(b, _)| b)
}

/// Byte offset in `text` (the lines joined with \n) → (row, column in chars)
pub fn position(text: &str, at: usize) -> (usize, usize) {
    let head = &text[..at];
    let row = head.matches('\n').count();
    let line = head.rfind('\n').map_or(head, |i| &head[i + 1..]);
    (row, line.chars().count())
}

fn jump(t: &mut TextArea, text: &str, at: usize) {
    let (row, col) = position(text, at);
    t.move_cursor(CursorMove::Jump(row as u16, col as u16));
}

#[cfg(test)]
mod tests {
    use crate::app::Popup;
    use crate::testkit::Harness;

    /// The console editor: ' " ( [ close themselves, the closing one types over, ⌫ in an empty pair removes both
    #[tokio::test]
    async fn editor_closes_types_over_and_deletes_pairs() {
        let mut h = Harness::new().await;
        h.console_tab();
        h.type_str("select f(");
        assert_eq!((h.console_text().as_str(), h.console_cursor().0), ("select f()", (0, 9)));
        h.type_str("'a");
        assert_eq!(h.console_text(), "select f('a')");
        // Typed over: the text doesn't grow, the caret ends past the )
        h.type_str("')");
        assert_eq!((h.console_text().as_str(), h.console_cursor().0), ("select f('a')", (0, 13)));
        h.type_str(" [");
        assert_eq!(h.console_text(), "select f('a') []");
        h.key("Backspace");
        assert_eq!((h.console_text().as_str(), h.console_cursor().0), ("select f('a') ", (0, 14)));
        // A quote right after a word, and anything inside a comment, stays alone
        h.type_str("it'");
        assert_eq!(h.console_text(), "select f('a') it'");
        h.console_with("-- note ", 0, 8);
        h.type_str("(\"");
        assert_eq!(h.console_text(), "-- note (\"");
    }

    /// Typing an opening quote or bracket over a selection wraps it and keeps it selected
    #[tokio::test]
    async fn editor_wraps_the_selection() {
        let mut h = Harness::new().await;
        h.console_with("a + b", 0, 0);
        h.key("Cmd+a");
        h.type_str("(");
        assert_eq!(h.console_text(), "(a + b)");
        assert_eq!(h.console_cursor().1.as_deref(), Some("a + b"));
        h.type_str("'");
        assert_eq!(h.console_text(), "('a + b')");
        // Another character replaces the selection as usual
        h.type_str("x");
        assert_eq!(h.console_text(), "('x')");
    }

    /// An unclosed ( is underlined in the danger color, with its line number; on its line the bottom border says what is open
    #[tokio::test]
    async fn editor_marks_unbalanced() {
        let mut h = Harness::new().await;
        h.console_with("select count(*\nfrom t where a = $$x", 0, 0);
        let buf = h.buffer(100, 30);
        let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
        // The cell where `pat` starts on row y (the symbols before it, not bytes: the borders are multi-byte)
        let cell = |y: u16, pat: &str| {
            let r = row(y);
            r[..r.find(pat).unwrap()].chars().count() as u16
        };
        let y = (0..30).find(|&y| row(y).contains("select count(*")).unwrap();
        let x = cell(y, "(*");
        assert_eq!(buf[(x, y)].fg, crate::ui::DANGER);
        assert!(buf[(x, y)].modifier.contains(ratatui::style::Modifier::UNDERLINED));
        assert_ne!(buf[(x + 1, y)].fg, crate::ui::DANGER);
        let num = cell(y, " 1 select") + 1;
        assert_eq!(buf[(num, y)].fg, crate::ui::DANGER);
        // The dollar quote on line 2 is marked too
        let x2 = cell(y + 1, "$$");
        assert_eq!(buf[(x2, y + 1)].fg, crate::ui::DANGER);
        assert!(h.screen(100, 30).contains("Unclosed '('"), "{}", h.screen(100, 30));
        // Closed: nothing marked
        h.console_with("select count(*) from t", 0, 0);
        let s = h.screen(100, 30);
        assert!(!s.contains("Unclosed"), "{s}");
    }

    /// The table tab's WHERE input pairs up as the editor does; ↩ applies what it shows
    #[tokio::test]
    async fn filter_input_pairs() {
        let mut h = Harness::new().await;
        let id = h.table_tab("users", &["id", "name"], &[&["1", "a"]], &["id"]);
        h.app.focus = crate::app::Focus::Grid;
        h.app.input_filter(id, false);
        h.type_str("name in ('a");
        let text = |h: &Harness| match &h.app.popup {
            Some(Popup::Input { input, .. }) => input.lines().join("\n"),
            _ => panic!("no input"),
        };
        assert_eq!(text(&h), "name in ('a')");
        h.type_str("', '");
        assert_eq!(text(&h), "name in ('a', '')");
        h.key("Backspace");
        assert_eq!(text(&h), "name in ('a', )");
        h.type_str("'b')");
        assert_eq!(text(&h), "name in ('a', 'b')");
        // Esc closes the list the quote may have opened, ↩ applies
        if matches!(&h.app.popup, Some(Popup::Input { complete: Some(_), .. })) {
            h.key("Esc");
        }
        h.key("Enter");
        assert_eq!(h.table(id).where_, "name in ('a', 'b')");
    }
}
