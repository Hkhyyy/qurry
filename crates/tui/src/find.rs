//! Editor find/replace (DataGrip ⌘F bar): regex · match case · whole words
use regex::Regex;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct FindOpts {
    pub regex: bool,
    pub case: bool,
    pub word: bool,
}

/// Search text → regex. Literal if not regex; ignore case is (?i), whole words is \b…\b
pub fn build(pattern: &str, o: FindOpts) -> Result<Regex, String> {
    let mut p = if o.regex { pattern.to_string() } else { regex::escape(pattern) };
    if o.word {
        p = format!(r"\b(?:{p})\b");
    }
    if !o.case {
        p = format!("(?i){p}");
    }
    // Multi-line text: ^ $ match per line
    Regex::new(&format!("(?m){p}")).map_err(|e| e.to_string().lines().last().unwrap_or("bad pattern").trim().to_string())
}

/// Replaced text and replacement count. With regex, groups like $1 can be used
pub fn replace_all(text: &str, re: &Regex, with: &str, o: FindOpts) -> (String, usize) {
    let n = re.find_iter(text).count();
    let out = if o.regex { re.replace_all(text, with) } else { re.replace_all(text, regex::NoExpand(with)) };
    (out.into_owned(), n)
}

/// Replacement text for one match. With regex, groups are expanded
pub fn replacement(found: &str, re: &Regex, with: &str, o: FindOpts) -> String {
    if !o.regex {
        return with.to_string();
    }
    match re.captures(found) {
        Some(c) => {
            let mut out = String::new();
            c.expand(with, &mut out);
            out
        }
        None => with.to_string(),
    }
}

// ───────── Bar (Popup::Find) ─────────

use crate::app::{App, Focus, Popup, TabKind, byte_offset};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use tui_textarea::{CursorMove, TextArea};

pub struct FindBar {
    pub find: TextArea<'static>,
    pub replace: TextArea<'static>,
    /// Whether focus is in the replace field
    pub on_replace: bool,
    pub opts: FindOpts,
    /// Search starts here while typing (where the bar was opened)
    origin: usize,
    /// (current index, total) — 0 means none
    pub cur: usize,
    pub count: usize,
    pub error: Option<String>,
}

fn field(text: &str) -> TextArea<'static> {
    let mut t = TextArea::from([text.to_string()]);
    t.move_cursor(CursorMove::End);
    t.set_cursor_line_style(Style::default());
    t
}

/// Byte offset in the text → (line, column)
fn pos_of(text: &str, byte: usize) -> (u16, u16) {
    let before = &text[..byte];
    let row = before.matches('\n').count();
    let start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    (row as u16, text[start..byte].chars().count() as u16)
}

fn editor(app: &mut App) -> Option<&mut TextArea<'static>> {
    match &mut app.tabs.get_mut(app.active)?.kind {
        TabKind::Console(c) => Some(&mut c.editor),
        _ => None,
    }
}

/// Selected range (bytes)
fn selection(ed: &TextArea) -> Option<(usize, usize)> {
    let ((r0, c0), (r1, c1)) = ed.selection_range()?;
    Some((byte_offset(ed.lines(), r0, c0), byte_offset(ed.lines(), r1, c1)))
}

fn select(ed: &mut TextArea, text: &str, (s, e): (usize, usize)) {
    let (r0, c0) = pos_of(text, s);
    let (r1, c1) = pos_of(text, e);
    ed.cancel_selection();
    ed.move_cursor(CursorMove::Jump(r0, c0));
    ed.start_selection();
    ed.move_cursor(CursorMove::Jump(r1, c1));
}

impl App {
    /// ⌃F: find bar in the console editor. If a single line is selected, use that text
    pub fn find_open(&mut self) {
        let Some(ed) = editor(self) else {
            self.status = "Find works in a query console".into();
            return;
        };
        let text = ed.lines().join("\n");
        let sel = selection(ed);
        let (r, c) = ed.cursor();
        let origin = sel.map(|x| x.0).unwrap_or_else(|| byte_offset(ed.lines(), r, c));
        let init = sel.map(|(a, b)| text[a..b].to_string()).filter(|t| !t.contains('\n')).unwrap_or_default();
        self.focus = Focus::Editor;
        let bar = FindBar { find: field(&init), replace: field(""), on_replace: false, opts: FindOpts::default(), origin, cur: 0, count: 0, error: None };
        self.popup = Some(Popup::Find(bar));
        self.find_refresh(true);
    }

    fn with_bar<T>(&mut self, f: impl FnOnce(&mut App, &mut FindBar) -> T) -> Option<T> {
        let Some(Popup::Find(mut bar)) = self.popup.take() else { return None };
        let r = f(self, &mut bar);
        self.popup = Some(Popup::Find(bar));
        Some(r)
    }

    /// Search again. jump = select the first match from where the bar was opened (while typing)
    pub fn find_refresh(&mut self, jump: bool) {
        self.with_bar(|a, bar| {
            let pat = bar.find.lines()[0].clone();
            let Some(ed) = editor(a) else { return };
            bar.error = None;
            bar.count = 0;
            bar.cur = 0;
            if pat.is_empty() {
                let _ = ed.set_search_pattern("");
                return;
            }
            let re = match build(&pat, bar.opts) {
                Ok(re) => re,
                Err(e) => {
                    bar.error = Some(e);
                    let _ = ed.set_search_pattern("");
                    return;
                }
            };
            let _ = ed.set_search_pattern(re.as_str());
            let text = ed.lines().join("\n");
            let found: Vec<(usize, usize)> = re.find_iter(&text).filter(|m| m.start() < m.end()).map(|m| (m.start(), m.end())).collect();
            bar.count = found.len();
            if found.is_empty() {
                return;
            }
            let i = if jump {
                found.iter().position(|m| m.0 >= bar.origin).unwrap_or(0)
            } else {
                match selection(ed).and_then(|s| found.iter().position(|m| *m == s)) {
                    Some(i) => i,
                    None => {
                        let (r, c) = ed.cursor();
                        let at = byte_offset(ed.lines(), r, c);
                        found.iter().position(|m| m.0 >= at).unwrap_or(0)
                    }
                }
            };
            if jump || selection(ed) != Some(found[i]) {
                select(ed, &text, found[i]);
            }
            bar.cur = i + 1;
        });
    }

    /// Next / previous match (wraps from end to start)
    fn find_step(&mut self, forward: bool) {
        self.with_bar(|a, bar| {
            let Ok(re) = build(&bar.find.lines()[0], bar.opts) else { return };
            let Some(ed) = editor(a) else { return };
            let text = ed.lines().join("\n");
            let found: Vec<(usize, usize)> = re.find_iter(&text).filter(|m| m.start() < m.end()).map(|m| (m.start(), m.end())).collect();
            if found.is_empty() {
                return;
            }
            let here = selection(ed).unwrap_or_else(|| {
                let (r, c) = ed.cursor();
                let b = byte_offset(ed.lines(), r, c);
                (b, b)
            });
            let i = if forward {
                found.iter().position(|m| m.0 >= here.1 && *m != here).unwrap_or(0)
            } else {
                found.iter().rposition(|m| m.1 <= here.0 && *m != here).unwrap_or(found.len() - 1)
            };
            select(ed, &text, found[i]);
            bar.cur = i + 1;
            bar.count = found.len();
        });
    }

    /// Replace only the selected match and go to the next
    fn find_replace_one(&mut self) {
        let done = self.with_bar(|a, bar| {
            let Ok(re) = build(&bar.find.lines()[0], bar.opts) else { return false };
            let with = bar.replace.lines()[0].clone();
            let o = bar.opts;
            let Some(ed) = editor(a) else { return false };
            let text = ed.lines().join("\n");
            let Some(sel) = selection(ed) else { return false };
            if !re.find_iter(&text).any(|m| (m.start(), m.end()) == sel) {
                return false;
            }
            let rep = replacement(&text[sel.0..sel.1], &re, &with, o);
            ed.cut();
            ed.insert_str(rep);
            true
        });
        if done == Some(true) {
            self.find_refresh(false);
            // After replacing, select the next match after the cursor
            self.with_bar(|a, bar| {
                let Ok(re) = build(&bar.find.lines()[0], bar.opts) else { return };
                let Some(ed) = editor(a) else { return };
                let text = ed.lines().join("\n");
                let (r, c) = ed.cursor();
                let at = byte_offset(ed.lines(), r, c);
                let found: Vec<(usize, usize)> = re.find_iter(&text).filter(|m| m.start() < m.end()).map(|m| (m.start(), m.end())).collect();
                bar.count = found.len();
                bar.cur = 0;
                if let Some(i) = found.iter().position(|m| m.0 >= at).or(if found.is_empty() { None } else { Some(0) }) {
                    select(ed, &text, found[i]);
                    bar.cur = i + 1;
                }
            });
        } else {
            self.find_step(true);
        }
    }

    /// Replace all (as a single edit so it can be undone at once)
    fn find_replace_all(&mut self) {
        let n = self.with_bar(|a, bar| {
            let Ok(re) = build(&bar.find.lines()[0], bar.opts) else { return 0 };
            let with = bar.replace.lines()[0].clone();
            let o = bar.opts;
            let Some(ed) = editor(a) else { return 0 };
            let text = ed.lines().join("\n");
            let (out, n) = replace_all(&text, &re, &with, o);
            if n > 0 {
                let (r, c) = ed.cursor();
                ed.select_all();
                ed.cut();
                ed.insert_str(out);
                ed.move_cursor(CursorMove::Jump(r as u16, c as u16));
            }
            n
        });
        self.find_refresh(false);
        if let Some(n) = n {
            self.status = format!("Replaced {n} occurrence{}", if n == 1 { "" } else { "s" });
        }
    }

    fn find_close(&mut self) {
        self.popup = None;
        if let Some(ed) = editor(self) {
            let _ = ed.set_search_pattern("");
        }
    }

    /// Paste: into the current field (as one line)
    pub fn find_paste(&mut self, s: &str) {
        let on_find = self.with_bar(|_, bar| {
            let t = if bar.on_replace { &mut bar.replace } else { &mut bar.find };
            t.insert_str(s.replace(['\r', '\n'], " "));
            !bar.on_replace
        });
        if on_find == Some(true) {
            self.find_refresh(true);
        }
    }
}

/// Keys while the bar is open
pub fn key(app: &mut App, k: KeyEvent) {
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let toggle = |app: &mut App, f: fn(&mut FindOpts)| {
        app.with_bar(|_, bar| f(&mut bar.opts));
        app.find_refresh(true);
    };
    match k.code {
        KeyCode::Esc => app.find_close(),
        KeyCode::Tab | KeyCode::BackTab => {
            app.with_bar(|_, bar| bar.on_replace = !bar.on_replace);
        }
        KeyCode::Char('x') if alt => toggle(app, |o| o.regex = !o.regex),
        KeyCode::Char('c') if alt => toggle(app, |o| o.case = !o.case),
        KeyCode::Char('w') if alt => toggle(app, |o| o.word = !o.word),
        KeyCode::Char('a') if alt => app.find_replace_all(),
        KeyCode::Char('f') if k.modifiers.contains(KeyModifiers::CONTROL) => {
            app.with_bar(|_, bar| bar.on_replace = false);
        }
        KeyCode::Enter if app.with_bar(|_, b| b.on_replace) == Some(true) => app.find_replace_one(),
        KeyCode::Enter | KeyCode::Down | KeyCode::F(3) if !k.modifiers.contains(KeyModifiers::SHIFT) => app.find_step(true),
        KeyCode::Up | KeyCode::F(3) => app.find_step(false),
        KeyCode::Enter => app.find_step(false),
        _ => {
            let changed = app.with_bar(|_, bar| {
                let t = if bar.on_replace { &mut bar.replace } else { &mut bar.find };
                let before = t.lines()[0].clone();
                t.input(k);
                !bar.on_replace && t.lines()[0] != before
            });
            if changed == Some(true) {
                app.find_refresh(true);
            }
        }
    }
}

/// Two-line bar at the top right of the editor: find [.* Cc W] n/m · replace (↩ one · ⌥A all)
pub fn draw(f: &mut Frame, app: &mut App) {
    // Area above the editor (captured when drawing that pane). If none, top of the screen
    let rect = if app.hits.find.width > 0 { app.hits.find } else { Rect { height: 4.min(f.area().height), ..f.area() } };
    let Some(Popup::Find(bar)) = &mut app.popup else { return };
    f.render_widget(Clear, rect);
    let accent = crate::kit::ACCENT;
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Thick).border_style(Style::default().fg(accent)).title(" Find / Replace · Tab switch · Esc close ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let [l0, l1] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    let on = |b: bool| if b { Style::default().fg(Color::Black).bg(accent) } else { Style::default().fg(Color::DarkGray) };
    let o = bar.opts;
    let info0 = match (&bar.error, bar.count) {
        (Some(e), _) => Line::styled(e.chars().take(18).collect::<String>(), Style::default().fg(crate::kit::DANGER)),
        (None, 0) if !bar.find.lines()[0].is_empty() => Line::styled("0 results", Style::default().fg(crate::kit::DANGER)),
        (None, 0) => Line::raw(""),
        (None, n) => Line::raw(format!("{}/{n}", bar.cur)),
    };
    let toggles = Line::from(vec![Span::styled(".*", on(o.regex)), Span::raw(" "), Span::styled("Cc", on(o.case)), Span::raw(" "), Span::styled("W", on(o.word)), Span::raw(" ")]);
    for (line, ta, focused, label) in [(l0, &mut bar.find, !bar.on_replace, "Find "), (l1, &mut bar.replace, bar.on_replace, "Repl ")] {
        let [lab, field, right] = Layout::horizontal([Constraint::Length(5), Constraint::Min(8), Constraint::Length(28)]).areas(line);
        f.render_widget(Paragraph::new(label).style(Style::default().fg(if focused { Color::White } else { Color::DarkGray })), lab);
        ta.set_cursor_style(if focused { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() });
        f.render_widget(&*ta, field);
        let info = if label == "Find " {
            let mut spans = toggles.spans.clone();
            spans.extend(info0.spans.clone());
            Line::from(spans)
        } else {
            Line::styled("↩ one · ⌥A all", Style::default().fg(Color::DarkGray))
        };
        f.render_widget(Paragraph::new(info), right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found<'a>(text: &'a str, pat: &str, o: FindOpts) -> Vec<&'a str> {
        let re = build(pat, o).unwrap();
        re.find_iter(text).map(|m| m.as_str()).collect()
    }

    #[test]
    fn plain_case_and_word() {
        let t = "select ID, id_x, Id from t";
        let o = FindOpts::default();
        assert_eq!(found(t, "id", o), ["ID", "id", "Id"]);
        assert_eq!(found(t, "id", FindOpts { case: true, ..o }), ["id"]);
        assert_eq!(found(t, "id", FindOpts { word: true, ..o }), ["ID", "Id"]);
        // Without regex, special characters are literal too
        assert_eq!(found("a.b axb", "a.b", o), ["a.b"]);
    }

    #[test]
    fn regex_find_and_replace_with_groups() {
        let o = FindOpts { regex: true, ..Default::default() };
        let t = "1.2.3.4 ok\n5.6.7.8 fail";
        assert_eq!(found(t, r"\d+\.\d+\.\d+\.\d+", o), ["1.2.3.4", "5.6.7.8"]);
        let re = build(r"(\d+\.\d+\.\d+\.\d+).*", o).unwrap();
        assert_eq!(replace_all(t, &re, "'$1',", o), ("'1.2.3.4',\n'5.6.7.8',".to_string(), 2));
        assert_eq!(replacement("1.2.3.4 ok", &re, "[$1]", o), "[1.2.3.4]");
        // Without regex, $1 is literal too
        let p = FindOpts::default();
        let re = build("ok", p).unwrap();
        assert_eq!(replace_all("ok ok", &re, "$1", p), ("$1 $1".to_string(), 2));
        assert!(build("(", o).is_err());
    }
}
