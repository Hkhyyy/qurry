use super::*;

pub(super) fn draw_popup(f: &mut Frame, app: &mut App) {
    if matches!(app.popup, Some(Popup::Find(_))) {
        return crate::find::draw(f, app);
    }
    // A note stays over the popup it was opened from: draw that first
    if let Some(Popup::Note { title, text, back }) = app.popup.take_if(|p| matches!(p, Popup::Note { .. })) {
        app.popup = Some(*back);
        draw_popup(f, app);
        let back = Box::new(app.popup.take().expect("drawn"));
        app.popup = Some(Popup::Note { title, text, back });
    }
    let area = f.area();
    let search_scope = if matches!(&app.popup, Some(Popup::Palette(p)) if crate::app::is_search(&p.title)) { crate::app::scope_name(app.shown_scope()) } else { "" };
    match app.popup.as_mut().unwrap() {
        Popup::Help { sel, scroll, record, message } => {
            // Start here (non-selectable line) → Commands → fixed keys
            let mut lines = vec![section("Start here")];
            for (k, d) in crate::help::start_here(&app.keys) {
                lines.push(key_row(&k, d, 30, false, false));
            }
            lines.push(Line::raw(""));
            lines.push(section("Commands — ↑↓ select · Enter change · a add key · Del reset to default"));
            // Every command (with a default key first), then the keys that work only in the explorer or the grid
            let mut sel_line = 0;
            let mut pane = Scope::Global;
            for (i, (s, _)) in crate::keys::COMMANDS.iter().enumerate() {
                if s.pane != pane {
                    pane = s.pane;
                    lines.push(Line::raw(""));
                    lines.push(section(if pane == Scope::Tree { "Database Explorer keys (when the explorer has focus)" } else { "Data grid keys (when a grid has focus)" }));
                }
                if i == *sel {
                    sel_line = lines.len();
                }
                let keys = if i == *sel && record.is_some() { "press a key… (Esc cancels)".to_string() } else { app.keys.slot_label(*s) };
                lines.push(key_row(&keys, crate::app::title(s.action), 30, app.keys.is_custom(*s), i == *sel));
            }
            lines.push(Line::raw(""));
            // Fixed key list: selectable continuing from the number after Commands (not rebindable)
            let mut i = crate::keys::COMMANDS.len();
            for (title, rows) in crate::help::HELP {
                lines.push(section(disp(title)));
                for (k, d) in *rows {
                    if i == *sel {
                        sel_line = lines.len();
                    }
                    lines.push(key_row(&disp(k), disp(d), 30, false, i == *sel));
                    i += 1;
                }
                lines.push(Line::raw(""));
            }
            let r = centered(area, 104, area.height.saturating_sub(2));
            let h = r.height.saturating_sub(2);
            // Keep the selected line visible
            let row = sel_line as u16;
            if row < *scroll {
                *scroll = row.saturating_sub(1);
            } else if row >= *scroll + h {
                *scroll = row + 1 - h;
            }
            let max = (lines.len() as u16).saturating_sub(h);
            if *scroll > max {
                *scroll = max;
            }
            let title = match message {
                Some(m) => format!(" {m} "),
                None => " Keymap — saved to keymap.json · PgUp PgDn · Esc close ".into(),
            };
            let inner = popup(f, r, title);
            f.render_widget(Paragraph::new(lines).scroll((*scroll, 0)), inner);
        }
        // Drawn separately above
        Popup::Find(_) => {}
        Popup::Message { title, text } | Popup::Note { title, text, .. } => {
            // Sized by word wrap, so a long line isn't cut off in a narrow terminal
            let lines: Vec<Line> = text.lines().map(|l| Line::raw(l.to_string())).collect();
            let r = centered(area, 90, (wrapped_height(&lines, 90u16.min(area.width.saturating_sub(4)).saturating_sub(2)) + 4).min(30));
            let inner = popup(f, r, format!(" {} (Esc) ", disp(title)));
            f.render_widget(Paragraph::new(text.clone()).wrap(Wrap { trim: false }), inner);
        }
        Popup::Scope(e) => {
            use crate::kit::Row as KRow;
            use crate::scope::{Check, Row};
            let name = app.conns.iter().find(|c| c.config.id == e.conn).map(|c| c.config.name.clone()).unwrap_or_default();
            let rows = e.rows();
            let r = centered(area, 70, (rows.len() as u16 + 4).min(area.height.saturating_sub(4)).max(8));
            let inner_h = r.height.saturating_sub(3) as usize;
            let top = e.sel.saturating_sub(inner_h.saturating_sub(1));
            let mut lines: Vec<Line> = rows
                .iter()
                .enumerate()
                .skip(top)
                .take(inner_h)
                .map(|(i, row)| {
                    let box_ = muted(match e.check(row) {
                        Check::On => "[x] ",
                        Check::Implicit => "[~] ",
                        Check::Off => "[ ] ",
                    });
                    let r = match row {
                        Row::AllDbs => KRow::new("All databases").chevron(None).lead(box_),
                        Row::DefaultDb => KRow::new("Default database").chevron(None).lead(box_).hint(format!("({})", e.default_db)),
                        Row::Db(db) => {
                            let open = rows.get(i + 1).is_some_and(|n| !matches!(n, Row::Db(_)));
                            let on = e.check(row) != Check::Off;
                            let r = KRow::new(db.clone()).chevron(Some(open)).lead(box_).hint(if *db == e.default_db { "(Default database)".to_string() } else { String::new() });
                            if on {
                                r.bold()
                            } else {
                                r
                            }
                        }
                        Row::AllSchemas(_) => KRow::new("All schemas").indent(1).chevron(None).lead(box_),
                        Row::DefaultSchema(_) => KRow::new("Default schema").indent(1).chevron(None).lead(box_).hint("(public)"),
                        Row::Schema { name, system, .. } => {
                            let r = KRow::new(name.clone()).indent(1).chevron(None).lead(box_);
                            let r = if *system { r.label_style(Style::default().fg(Color::Yellow)).trailing("⚡ system") } else { r };
                            r.hint(if name == "public" { "(Default schema)".to_string() } else { String::new() })
                        }
                        Row::Note(_, t) => KRow::new(String::new()).indent(2).hint(t.clone()),
                    };
                    r.selected(i == e.sel, true).line()
                })
                .collect();
            lines.push(key_hints(&[("Space", "check"), ("→", "expand"), ("←", "collapse"), ("Enter", "apply"), ("Esc", "cancel")]));
            let inner = popup(f, r, format!(" Manage Shown Schemas — {name} "));
            f.render_widget(Paragraph::new(lines), inner);
        }
        Popup::Confirm { text, sql, .. } => app.hits.confirm_unseen = !draw_confirm(f, area, " Confirm ", text, *sql, key_hints(&[("y / Enter", "yes"), ("n / Esc", "no")])),
        Popup::ImportPick(p) => draw_import(f, area, p),
        // Like Confirm, but only y allows (no Enter): the popup opened on its own
        Popup::AllowPlaintext { text, .. } => app.hits.confirm_unseen = !draw_confirm(f, area, " Allow Unencrypted ", text, None, key_hints(&[("y", "allow"), ("n / Esc", "cancel")])),
        Popup::Input { title, body, input, complete, filter, redis, .. } => {
            // The title goes on the border; the body (answers, reasons) in a box below, wrapped, so it isn't cut off
            let body: Vec<Line> = body.lines().map(|l| Line::raw(disp(l))).collect();
            let bh = if body.is_empty() { 0 } else { wrapped_height(&body, 90u16.min(area.width.saturating_sub(4)).saturating_sub(2)) + 2 };
            let all = centered(area, 90, 3 + bh);
            let r = Rect { height: all.height.min(3), ..all };
            if bh > 0 {
                // A plain border: only the input above reads as focused
                let br = Rect { y: r.bottom(), height: all.height - r.height, ..all };
                clear(f, br);
                let b = panel("", false).style(Style::default().bg(POPUP_BG));
                f.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }).block(b), br);
            }
            clear(f, r);
            let hint = if redis.is_some() { "Enter / Esc · Tab complete · ↑↓ history · HELP command" } else if filter.is_none() { "Enter / Esc" } else { "Enter / Esc · Tab complete" };
            input.set_block(panel(format!(" {} ({hint}) ", disp(title)), true));
            f.render_widget(&*input, r);
            // WHERE / ORDER BY in the console's colors (qurry_shared::highlight, as the GUI's fields)
            if let Some(fs) = filter {
                let inner = Rect { x: r.x + 1, y: r.y + 1, width: r.width.saturating_sub(2), height: 1 };
                let (line, caret) = (input.lines().first().cloned().unwrap_or_default(), input.cursor().1);
                fs.scroll = textarea_scroll(fs.scroll, caret as u16, inner.width);
                color_sql(f.buffer_mut(), inner, &line, caret, fs.scroll, input.tab_length(), fs.kind);
            }
            // The Redis command line: the arguments still to type, muted, after the text
            if redis.is_some() {
                let line = input.lines().first().cloned().unwrap_or_default();
                let at = line.chars().count();
                let args = qurry_shared::redis_cmd::assist(&line).hint;
                if input.cursor().1 == at && !args.is_empty() {
                    let x = r.x + 1 + at as u16 + u16::from(!line.is_empty() && !line.ends_with(' '));
                    let w = r.right().saturating_sub(x + 1);
                    let ghost = Rect { x, y: r.y + 1, width: w, height: 1 };
                    f.render_widget(Paragraph::new(Span::styled(args, Style::default().fg(DIM))), ghost);
                }
            }
            // The console's completion list, right under the input at the word being completed
            if let Some(cmp) = complete {
                draw_completion(f, cmp, r.x + 1 + input.cursor().1 as u16, r.bottom() - 1);
            }
        }
        Popup::Palette(p) => {
            // The Execute chooser keeps to its options at the bottom, so the console rows it highlights stay in view
            let r = if p.title == "Execute" {
                let h = (p.items.len() as u16 + 5).min(area.height.saturating_sub(2));
                Rect { y: area.bottom().saturating_sub(h + 1), ..centered(area, 72, h) }
            } else {
                centered(area, 100, 24)
            };
            clear(f, r);
            let [inp, list] = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(r);
            app.hits.palette = Block::default().borders(Borders::ALL).inner(list);
            // A search shows its scope (the GUI's scope picker)
            let scope = if crate::app::is_search(&p.title) { format!(" · {search_scope}") } else { String::new() };
            let t = format!(" {}{scope}{} ", p.title, if p.loading > 0 { " — loading…" } else { "" });
            p.input.set_block(panel(t, true));
            p.input.set_cursor_line_style(Style::default());
            f.render_widget(&p.input, inp);
            let matches = App::palette_matches(p);
            let h = list.height.saturating_sub(2) as usize;
            let sel = p.sel.min(matches.len().saturating_sub(1));
            let off = sel.saturating_sub(h.saturating_sub(1));
            app.hits.palette_off = off;
            let items: Vec<ListItem> = matches
                .iter()
                .enumerate()
                .skip(off)
                .take(h)
                .map(|(i, it)| {
                    // Key labels inside names·hints follow the screen rules too ("Ctrl+Z (grid)" → ⌃z)
                    let mut row = Row::new(disp(&it.label));
                    // Tab rows (Recent Tabs, Search Everywhere): the tab bar's glyph
                    if let Some(t) = match it.action { crate::app::PAction::Tab(id) => app.tabs.iter().find(|t| t.id == id), _ => None } {
                        row = row.lead(icon(&tab_glyph(&t.kind), Color::Gray));
                    }
                    ListItem::new(row.hint(format!(" {}", disp(&it.hint))).selected(i == sel, true).line())
                })
                .collect();
            p.sel = sel;
            let ddl = app.keys.keys(crate::app::Action::Ddl).first().map(|k| disp(k)).unwrap_or_else(|| "—".into());
            let check = p.items.iter().all(|i| matches!(i.action, crate::app::PAction::StatusItem(_)));
            let hint = if crate::app::is_search(&p.title) {
                // Its open key again: the next scope
                let again = if p.title == "Search Everywhere" { crate::app::Action::SearchEverywhere } else { crate::app::Action::GoToObject };
                let again = app.keys.keys(again).first().map(|k| disp(k)).unwrap_or_else(|| "—".into());
                format!(" Enter open · {ddl} DDL · {again} scope · Esc ")
            } else if check {
                " ↑↓ move · Space toggle · Esc close ".into()
            } else {
                " Enter · Esc ".into()
            };
            f.render_widget(List::new(items).block(panel(hint, true)), list);
        }
        Popup::ConnForm(form) => draw_conn_form(f, area, form),
        Popup::Params { fields, sel, preview, kind, .. } => draw_params(f, area, fields, *sel, preview, *kind),
    }
}

/// Lines of the statements the Parameters popup shows under its fields
const PARAMS_PREVIEW: usize = 6;

/// Parameters (#173): a `:name` field per parameter (the data source form's field rows), the focused one with its caret; the
/// statements under them in the console's colors with the names marked, their first lines; the keys at the bottom. Its size
/// comes from the names and the statements, so typing never moves it; in a short window the fields keep the focused one in view
fn draw_params(f: &mut Frame, area: Rect, fields: &[(String, tui_textarea::TextArea<'static>)], sel: usize, preview: &str, kind: qurry_shared::DbKind) {
    let lines: Vec<&str> = preview.lines().collect();
    let shown = lines.len().min(PARAMS_PREVIEW) + usize::from(lines.len() > PARAMS_PREVIEW);
    let r = centered(area, 72, (fields.len() + 1 + shown + 1 + 1) as u16 + 2);
    let inner = popup(f, r, " Parameters ");
    let label_w = fields.iter().map(|(n, _)| n.chars().count() + 2).max().unwrap_or(0).min(16);
    // The value's room right of its label: the focused one scrolls to keep its caret in view
    let room = (inner.width as usize).saturating_sub(label_w + 2).max(1);
    let rows: Vec<Line> = fields
        .iter()
        .enumerate()
        .map(|(i, (name, input))| {
            // A name longer than the label column ends in … (the statement below shows it whole)
            let label = format!(":{name}");
            let label = if label.chars().count() < label_w { label } else { label.chars().take(label_w - 2).collect::<String>() + "…" };
            let value = input.lines().first().cloned().unwrap_or_default();
            if i != sel {
                return Line::from(field_row(&label, label_w, value, false));
            }
            let caret = input.cursor().1;
            let before: String = value.chars().take(caret).skip(caret.saturating_sub(room - 1)).collect();
            let after: String = value.chars().skip(caret).collect();
            Line::from(field_row(&label, label_w, format!("{before}▏{after}"), true))
        })
        .collect();
    // The names marked where they are, line by line (found once in the whole text: a string or comment can span lines)
    let mut marks = qurry_shared::params::find_params(preview, kind).into_iter().map(|p| (p.start, p.end)).peekable();
    let mut code = vec![];
    let mut at = 0;
    for l in lines.iter().take(PARAMS_PREVIEW) {
        let mut own = vec![];
        while let Some((a, b)) = marks.next_if(|m| m.0 < at + l.len()) {
            own.push((a - at, b.min(at + l.len()) - at));
        }
        code.push(sql_line_marked(l, kind, &own));
        at += l.len() + 1;
    }
    if lines.len() > PARAMS_PREVIEW {
        code.push(Line::styled("⋯", Style::default().fg(DIM)));
    }
    let hint = key_hints(&[("Enter", "run"), ("Tab ⇧Tab ↑↓", "move"), ("Esc", "cancel")]);
    let [body, _, sql, _, keys] = Layout::vertical([Constraint::Length(rows.len() as u16), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    let top = (sel + 1).saturating_sub(body.height as usize) as u16;
    f.render_widget(Paragraph::new(rows).scroll((top, 0)), body);
    f.render_widget(Paragraph::new(code), Rect { x: sql.x + 2, width: sql.width.saturating_sub(2), ..sql });
    f.render_widget(Paragraph::new(hint), keys);
}

/// tui-textarea's horizontal scroll after a frame: the first column stays until the caret leaves the view (its next_scroll_top)
fn textarea_scroll(prev: u16, caret: u16, width: u16) -> u16 {
    if caret < prev {
        caret
    } else if prev + width <= caret {
        caret + 1 - width
    } else {
        prev
    }
}

/// SQL colors over a one-line input's text as drawn in `inner` from column `scroll`: only the color changes (comments italic), and
/// the caret's cell (character `caret`) keeps the input's look. A tab (pasted) spans up to the next stop of `tab`, as tui-textarea draws it
fn color_sql(buf: &mut Buffer, inner: Rect, line: &str, caret: usize, scroll: u16, tab: u8, kind: qurry_shared::DbKind) {
    use qurry_shared::highlight::highlight;
    use unicode_width::UnicodeWidthChar;
    let spans = highlight(line, kind);
    let (mut col, mut next) = (0u16, 0);
    for (i, (b, ch)) in line.char_indices().enumerate() {
        let w = if ch == '\t' { tab.max(1) as u16 - col % tab.max(1) as u16 } else { ch.width().unwrap_or(0) as u16 };
        while spans.get(next).is_some_and(|s| s.end <= b) {
            next += 1;
        }
        let style = spans.get(next).filter(|s| s.start <= b).and_then(|s| sql_style(s.class));
        if let Some(style) = style.filter(|_| i != caret) {
            for x in (col..col + w).filter(|x| (scroll..scroll + inner.width).contains(x)) {
                buf[(inner.x + x - scroll, inner.y)].set_style(style);
            }
        }
        col += w;
    }
}

/// Confirm: multiple lines (reason · statement · question). Red border for production DB · dangerous statements; "→ N rows" stands out more the larger it is.
/// In a window too short for all of it, the popup takes the whole height; the first line, the questions (a line ending in "?" or
/// with a "? " in it), the warnings (⚠) and the keys stay, and the details in between (statements, columns, tables) give way to a line saying how many are
/// hidden and how many rows they would change. If even that doesn't fit, the keys line says to enlarge the window and the result is
/// false: y / Enter don't answer a question that isn't on screen. With `sql`, the statement lines (two-space indent) are drawn in the
/// console's SQL colors; the other lines and the line counting stay as they are
fn draw_confirm(f: &mut Frame, area: Rect, title: &'static str, text: &str, sql: Option<qurry_shared::DbKind>, keys: Line<'static>) -> bool {
    let danger = text.starts_with("Production")
        || text.ends_with("Run it anyway?")
        || text.ends_with("Run them anyway?")
        || text.lines().any(|l| l.starts_with('⚠') && l.contains("Production"));
    let mut lines = confirm_lines(text);
    if let Some(kind) = sql {
        for (l, t) in lines.iter_mut().zip(text.lines()) {
            if t.strip_prefix("  ").is_some_and(|s| !s.starts_with(char::is_whitespace)) {
                *l = sql_line(t, kind);
            }
        }
    }
    let last = lines.len() - 1;
    lines[last] = keys;
    let keep: Vec<bool> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let t = l.to_string();
            i == 0 || i == last || t.starts_with('⚠') || (!t.starts_with(' ') && (t.trim_end().ends_with('?') || t.contains("? ")))
        })
        .collect();
    // Long statements wrap → count wrapped lines for the height (so the y/n hint below isn't cut)
    let w = 90u16.min(area.width.saturating_sub(4)).saturating_sub(2);
    let want = wrapped_height(&lines, w).saturating_add(2);
    let mut r = centered(area, 90, want);
    if r.height < want {
        r = Rect { y: area.y, height: area.height, ..r };
    }
    let inner = if danger { popup_danger(f, r, title) } else { popup(f, r, title) };
    let fits = wrapped_height(&cut_lines(lines.clone(), &keep, inner.width, inner.height), inner.width) <= inner.height;
    if !fits {
        let mut l = key_hints(&[("n / Esc", "no")]);
        l.spans.insert(0, Span::styled("Enlarge the window to answer · ", Style::default().fg(WARN)));
        lines[last] = l;
    }
    let ask = lines.split_off(last);
    let ah = wrapped_height(&ask, inner.width).min(inner.height);
    let [body, bottom] = Layout::vertical([Constraint::Min(0), Constraint::Length(ah)]).areas(inner);
    f.render_widget(Paragraph::new(cut_lines(lines, &keep[..last], body.width, body.height)).wrap(Wrap { trim: false }), body);
    f.render_widget(Paragraph::new(ask).wrap(Wrap { trim: false }), bottom);
    fits
}

/// Import Data Sources' confirm: the question (from, path, counts, MCP Off and passwords said once), then two lines per source —
/// [x] / [ ] and what is added (skipped rows have no box) with the value on the right, the mapped settings or the reason under it.
/// The row under the cursor is highlighted and kept in view; the keys stay at the bottom
fn draw_import(f: &mut Frame, area: Rect, p: &crate::import::ImportPick) {
    let w = 110u16.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(2);
    let msg: Vec<Line> = p.plan.message.lines().map(|l| Line::raw(disp(l))).collect();
    let mh = wrapped_height(&msg, iw);
    let n = p.count();
    let hint = key_hints(&[("Space", "check"), ("↑↓", "move"), ("y / Enter", &format!("import {n}")), ("n / Esc", "cancel")]);
    let rows = p.plan.rows.len() as u16;
    let mut r = centered(area, 110, mh + 1 + rows * 2 + 1 + 1 + 2);
    if r.height < 8 {
        r = Rect { y: area.y, height: area.height, ..r };
    }
    let inner = popup(f, r, " Confirm ");
    let [top, _, list, _, bottom] = Layout::vertical([Constraint::Length(mh.min(inner.height.saturating_sub(5))), Constraint::Length(1), Constraint::Min(2), Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: false }), top);
    f.render_widget(Paragraph::new(hint), bottom);
    // Cut to n cells with … (a long warning or server name never runs into the other)
    let cut = |s: String, n: usize| if s.chars().count() <= n { s } else { s.chars().take(n.saturating_sub(1)).collect::<String>() + "…" };
    // Two lines a row, scrolled so the cursor row shows
    let fit = (list.height / 2).max(1) as usize;
    let first = p.sel.saturating_sub(fit - 1).min(p.plan.rows.len().saturating_sub(fit));
    let mut lines = vec![];
    for (i, row) in p.plan.rows.iter().enumerate().skip(first).take(fit) {
        let bg = if i == p.sel { Style::default().bg(SEL) } else { Style::default() };
        let boxed = if !row.checkable { "    " } else if p.checked[i] { "[x] " } else { "[ ] " };
        let tone = if row.tone.as_deref() == Some("warn") { Style::default().fg(WARN) } else { Style::default().fg(DIM) };
        let text_style = if row.checkable { Style::default() } else { Style::default().fg(DIM) };
        let w = iw as usize;
        let value = cut(disp(&row.value), w * 2 / 5);
        let left = cut(format!("{boxed}{}", disp(&row.text)), w.saturating_sub(value.chars().count() + 2));
        let pad = w.saturating_sub(left.chars().count() + value.chars().count());
        lines.push(Line::from(vec![Span::styled(left, text_style), Span::raw(" ".repeat(pad)), Span::styled(value, tone)]).style(bg));
        lines.push(Line::styled(cut(format!("      {}", disp(&row.note)), w), Style::default().fg(DIM)).style(bg));
    }
    f.render_widget(Paragraph::new(lines), list);
}

/// Text cut to `rows` wrapped rows of `width`, ending in "⋯" when cut
pub(super) fn fit_rows(text: &str, width: u16, rows: u16) -> String {
    let fits = |s: &str| wrapped_height(&[Line::raw(s.to_string())], width) <= rows;
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // The longest prefix (in chars) that fits with "⋯" after it
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&format!("{}⋯", chars[..mid].iter().collect::<String>())) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    format!("{}⋯", chars[..lo].iter().collect::<String>().trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Production confirm at the minimum size keeps its question and keys; the statements that don't fit are counted
    /// with the rows they would change, so y is never an answer to an unseen row count
    #[tokio::test]
    async fn production_confirm_keeps_its_question_in_small_windows() {
        let mut h = crate::testkit::Harness::new().await;
        let text = "Production data source \"[PROD] kr\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n  delete from payment where created_at < now() - interval '1 year'\n     → 12,408 rows\n\nRun it?";
        h.app.popup = Some(Popup::Confirm { opened: None, sql: None, text: text.into(), on_yes: Box::new(|a| a.status = "ran".into()) });
        let s = h.screen(80, 24);
        assert!(s.contains("12,408 rows") && s.contains("Run it?") && !s.contains("hidden"), "{s}");
        // Every row that would change shows, as its own line or in the hidden total
        for (w, ht) in [MIN_SIZE, (60, 12)] {
            let s = h.screen(w, ht);
            assert!(s.contains("Production data") && s.contains("Run it?") && s.contains("y / Enter"), "{w}×{ht}\n{s}");
            let all = s.contains("→ 1 row") && s.contains("→ 12,408 rows");
            assert!(all || s.contains("12,409 rows") || (s.contains("→ 1 row") && s.contains("hidden · 12,408 rows")), "{w}×{ht}\n{s}");
        }
        // One statement at the minimum size: its row count shows
        let one = "Production data source \"[PROD] kr\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n\nRun it?";
        h.app.popup = Some(Popup::Confirm { opened: None, sql: None, text: one.into(), on_yes: Box::new(|a| a.status = "ran".into()) });
        let (w, ht) = MIN_SIZE;
        let s = h.screen(w, ht);
        assert!(s.contains("1 row") && s.contains("Run it?"), "{s}");
        h.key("y");
        assert_eq!(h.app.status, "ran");
    }

    /// A confirm of 1000 statements draws in a few ms, cut or not (each line wraps once per frame)
    #[tokio::test]
    async fn big_confirm_draws_fast() {
        let mut h = crate::testkit::Harness::new().await;
        let lines: Vec<String> = (0..1000).map(|i| format!("  update payment set amount = 0 where id = {i}\n     → 1 row")).collect();
        let text = format!("Production data source \"[PROD] kr\" — this changes data:\n\n{}\n\nRun it?", lines.join("\n"));
        h.app.popup = Some(Popup::Confirm { opened: None, sql: None, text, on_yes: Box::new(|_| {}) });
        for (w, ht) in [MIN_SIZE, (80, 24)] {
            let t = std::time::Instant::now();
            let s = h.screen(w, ht);
            assert!(s.contains("lines hidden · ") && s.contains("Run it?"), "{w}×{ht}\n{s}");
            // Debug build, other tests running: generous; the quadratic cut took seconds here
            assert!(t.elapsed() < std::time::Duration::from_millis(500), "{w}×{ht}: {:?}", t.elapsed());
        }
    }

    #[test]
    fn popup_clears_wide_chars_straddling_its_edges() {
        let mut term = Terminal::new(ratatui::backend::TestBackend::new(20, 1)).unwrap();
        term.draw(|f| {
            // "가" is at cells 3·4, "나" at 9·10 → straddling both edges of the popup 4..10
            f.buffer_mut().set_string(0, 0, "abc가defg나", Style::default().bg(Color::Yellow));
            clear(f, Rect::new(4, 0, 6, 1));
            f.render_widget(Paragraph::new("│popup").style(Style::default()), Rect::new(4, 0, 6, 1));
        })
        .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(3, 0)].symbol(), " ", "left half of 가 would hide the popup's left edge");
        assert_eq!(buf[(4, 0)].symbol(), "│");
        assert_eq!(buf[(10, 0)].symbol(), " ");
        assert_eq!(buf[(10, 0)].bg, Color::Yellow, "the cell after the popup keeps the row's colour");
    }

    /// #136: the WHERE / ORDER BY inputs draw their SQL in the console's colors (the theme's keyword, number, string, function and
    /// comment), symbol operators and names in the text color; the caret's cell keeps the input's look
    #[tokio::test]
    async fn filter_input_draws_sql_in_colors() {
        use crate::testkit::Harness;
        let mut h = Harness::new().await;
        let id = h.table_tab("users", &["id", "name"], &[&["1", "a"]], &["id"]);
        h.app.focus = crate::app::Focus::Grid;
        let look = |h: &Harness, tok: &str| {
            let t = h.app.themes.resolve(qurry_shared::theme::SYSTEM, !h.app.auto_light);
            let (r, g, b) = qurry_shared::theme::rgb(t.color(tok)).unwrap();
            Color::Rgb(r, g, b)
        };
        for theme in ["dark", "light"] {
            h.app.themes.theme = Some(theme.into());
            h.app.input_filter(id, false);
            h.type_str("lower(name) = 'a' AND id IN (1, 2) /* x */");
            h.key("Home");
            let buf = h.buffer(100, 30);
            let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
            let y = (0..30).find(|&y| row(y).contains("lower(name)")).unwrap();
            let r = row(y);
            let at = |pat: &str| r[..r.find(pat).unwrap()].chars().count() as u16;
            let fg = |pat: &str| buf[(at(pat), y)].fg;
            assert_eq!(fg("AND"), look(&h, "keyword"), "{theme}");
            assert_eq!(fg("IN"), look(&h, "keyword"), "{theme}");
            assert_eq!(fg("1,"), look(&h, "number"), "{theme}");
            assert_eq!(fg("'a'"), look(&h, "string"), "{theme}");
            assert_eq!(fg("ower("), look(&h, "function"), "{theme}");
            assert_eq!(fg("/* x"), look(&h, "comment"), "{theme}");
            assert!(buf[(at("/* x"), y)].modifier.contains(Modifier::ITALIC));
            for plain in ["name)", "=", "id IN", "("] {
                assert!(![look(&h, "keyword"), look(&h, "number"), look(&h, "string"), look(&h, "function")].contains(&fg(plain)), "{theme}: {plain}");
            }
            // The caret (Home: on `lower`'s l) keeps the input's look
            assert!(buf[(at("lower"), y)].modifier.contains(Modifier::REVERSED), "{theme}");
            assert_ne!(buf[(at("lower"), y)].fg, look(&h, "function"), "{theme}");
            h.app.popup = None;
        }
        // A pasted tab is drawn as spaces up to the next stop: the colors after it stay on their characters
        h.app.input_filter(id, false);
        h.paste("id\t= 1 AND name = 'x'");
        h.key("Home");
        let buf = h.buffer(100, 30);
        let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let y = (0..30).find(|&y| row(y).contains("AND name")).unwrap();
        let r = row(y);
        let at = |pat: &str| r[..r.find(pat).unwrap()].chars().count() as u16;
        let fg = |pat: &str| buf[(at(pat), y)].fg;
        assert_eq!(fg("1 A"), look(&h, "number"));
        assert_eq!(fg("AND"), look(&h, "keyword"));
        assert_eq!(fg("ND"), look(&h, "keyword"));
        assert_eq!(fg("'x'"), look(&h, "string"));
        assert_eq!(fg("x'"), look(&h, "string"));
        for plain in ["= 1", "name", "= '"] {
            assert!(![look(&h, "keyword"), look(&h, "number"), look(&h, "string")].contains(&fg(plain)), "{plain}");
        }
    }

    /// A text longer than the input scrolls with the caret: the colors move with it (tui-textarea's scroll, mirrored)
    #[tokio::test]
    async fn filter_input_colors_follow_its_scroll() {
        use crate::testkit::Harness;
        let mut h = Harness::new().await;
        let id = h.table_tab("users", &["id", "name"], &[&["1", "a"]], &["id"]);
        h.app.focus = crate::app::Focus::Grid;
        h.app.input_filter(id, false);
        let text = (0..20).map(|i| format!("id = {i}")).collect::<Vec<_>>().join(" or ");
        h.type_str(&text);
        let check = |h: &mut Harness| {
            let buf = h.buffer(100, 30);
            let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
            let y = (0..30).find(|&y| row(y).contains(" or id = ")).unwrap();
            let r = row(y);
            // Every visible `or` is a keyword, every `=` is not
            let cells: Vec<_> = r.chars().enumerate().collect();
            let mut n = 0;
            for w in cells.windows(4) {
                if w[0].1 == ' ' && w[1].1 == 'o' && w[2].1 == 'r' && w[3].1 == ' ' {
                    assert_eq!(buf[(w[1].0 as u16, y)].fg, JSON_KEYWORD, "{r}");
                    n += 1;
                }
                if w[1].1 == '=' {
                    assert_ne!(buf[(w[1].0 as u16, y)].fg, JSON_KEYWORD, "{r}");
                }
            }
            assert!(n >= 5, "{r}");
        };
        // At the end (scrolled), then back at the start
        check(&mut h);
        h.key("Home");
        check(&mut h);
        for _ in 0..3 {
            h.key("Right");
        }
        check(&mut h);
    }
}
