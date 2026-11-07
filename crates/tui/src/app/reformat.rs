//! Reformat Code (⌥⇧F, the GUI's ⌘⌥l) and the keyword case as you type, by qurry_shared::format (the GUI's rules). Each changes
//! a range of the console's text in one undo step of ⌃u / ⌃r
use super::*;
use qurry_core::format::{self, KeywordCase};
use tui_textarea::CursorMove;

/// A replacement, which the textarea keeps as two edits (the old text deleted, the new inserted): the text between them and after
pub struct Joined {
    mid: String,
    after: String,
}

/// How many recent replacements ⌃u undoes in one step (an older one takes two)
const JOINED_MAX: usize = 20;

impl Console {
    fn text(&self) -> String {
        self.editor.lines().join("\n")
    }

    /// text[start..end] (bytes) becomes `with` in one undo step; the cursor ends after it
    pub fn replace(&mut self, start: usize, end: usize, with: &str) {
        let text = self.text();
        let to = |b| {
            let (r, c) = crate::find::pos_of(&text, b);
            CursorMove::Jump(r, c)
        };
        self.editor.cancel_selection();
        self.editor.move_cursor(to(start));
        if start < end {
            self.editor.start_selection();
            self.editor.move_cursor(to(end));
        }
        self.editor.insert_str(with);
        if start < end && !with.is_empty() {
            let (head, tail) = (&text[..start], &text[end..]);
            self.joined.push(Joined { mid: format!("{head}{tail}"), after: format!("{head}{with}{tail}") });
            if self.joined.len() > JOINED_MAX {
                self.joined.remove(0);
            }
        }
    }

    /// ⌃u: a replacement's two edits at once
    pub fn undo(&mut self) {
        let before = self.text();
        if self.editor.undo() {
            let now = self.text();
            if self.joined.iter().any(|j| j.after == before && j.mid == now) {
                self.editor.undo();
            }
        }
    }

    /// ⌃r: a replacement's two edits at once
    pub fn redo(&mut self) {
        if self.editor.redo() {
            let now = self.text();
            if self.joined.iter().any(|j| j.mid == now) {
                self.editor.redo();
            }
        }
    }
}

impl App {
    /// ⌥⇧F Reformat Code: the selection, or the statement at the cursor, laid out as the GUI's ⌘⌥l does (keywords in the keyword
    /// case, upper case when it is Off)
    pub fn reformat(&mut self) {
        let kind = match self.tabs.get(self.active) {
            Some(t) if matches!(t.kind, TabKind::Console(_)) => self.kind_of(&t.conn),
            _ => {
                self.status = format!("Reformat Code works in a console — {} opens one", self.keys.label(Action::NewConsole));
                return;
            }
        };
        let case = self.keyword_case;
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let lines = c.editor.lines();
        let (row, col) = c.editor.cursor();
        let sel = c.editor.selection_range().map(|((r0, c0), (r1, c1))| (byte_offset(lines, r0, c0), byte_offset(lines, r1, c1)));
        let caret = byte_offset(lines, row, col);
        let text = c.text();
        self.status = match format::reformat(&text, sel, caret, kind, case) {
            Some(r) => {
                c.replace(r.start, r.end, &r.text);
                c.complete = None;
                format!("Reformatted — {} undoes it", crate::keys::disp("Ctrl+U"))
            }
            None if !format::holds_sql(kind) => "Reformat Code is for SQL consoles".into(),
            None => "Nothing to reformat: it is laid out already".into(),
        };
    }

    /// Keyword case as you type: a space, newline or punctuation just typed in the console gives the keyword it ends the setting's
    /// case (outside strings, comments and quoted names: qurry_shared::format::keyword_case_at)
    pub(crate) fn case_typed_keyword(&mut self) {
        let case = self.keyword_case;
        let Some(t) = self.tabs.get(self.active).filter(|_| case != KeywordCase::Off) else { return };
        let kind = self.kind_of(&t.conn);
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let (row, col) = c.editor.cursor();
        let lines = c.editor.lines();
        // The typed character is the one before the cursor; a newline ends the line above. Text after the cursor's line can't matter
        let end = match (row, col) {
            (0, 0) => return,
            (_, 0) => byte_offset(lines, row, 0) - 1,
            _ => byte_offset(lines, row, col - 1),
        };
        let text = lines[..=row].join("\n");
        if let Some((start, cased)) = format::keyword_case_at(&text, end, kind, case) {
            c.replace(start, end, &cased);
            c.editor.move_cursor(CursorMove::Jump(row as u16, col as u16));
        }
    }

    /// Save the keyword case for the GUI and the TUI (⌃k › Keyword Case…)
    pub(super) fn set_keyword_case(&mut self, case: KeywordCase) {
        self.status = match self.state.store.set_keyword_case(case) {
            Ok(()) => {
                self.keyword_case = case;
                format!("Keyword case: {} — saved for the GUI too", keyword_case_name(case))
            }
            Err(e) => format!("Couldn't save the keyword case: {e}"),
        };
    }
}

/// The GUI's names (Settings › Appearance › Keyword case)
pub(super) fn keyword_case_name(c: KeywordCase) -> &'static str {
    match c {
        KeywordCase::Off => "Off",
        KeywordCase::Upper => "UPPER",
        KeywordCase::Lower => "lower",
    }
}
