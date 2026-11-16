use super::*;

/// Cursor (inverted cell) position in the editor
/// Console cursor cell mark (invisible since underline isn't turned on)
pub(super) const CARET_MARK: Color = Color::Rgb(1, 2, 3);

/// The result tab strip over the top border of `area` (result_strip); each label's area and its tab
pub(super) fn draw_result_strip(f: &mut Frame, area: Rect, r: &crate::app::Results, tail: String) -> Vec<(Rect, Option<usize>)> {
    let width = area.width.saturating_sub(2);
    let (line, hits) = result_strip(r, tail, width);
    f.buffer_mut().set_line(area.x + 1, area.y, &line, width);
    hits.into_iter().filter(|&(x, ..)| x < width).map(|(x, w, id)| (Rect { x: area.x + 1 + x, y: area.y, width: w.min(width - x), height: 1 }, id)).collect()
}

/// The console result panel's title: " Output │ 1: customers │ 2: orders" then `tail`, the shown tab accented, bold and underlined.
/// Too many for `width`: the leftmost tabs other than the shown one fold into ‹n after Output. With each label's (x, width, tab id)
fn result_strip(r: &crate::app::Results, tail: String, width: u16) -> (Line<'static>, Vec<(u16, u16, Option<usize>)>) {
    use unicode_width::UnicodeWidthStr;
    let mut all: Vec<(String, Option<usize>)> = std::iter::once(("Output".to_string(), None)).chain(r.tabs.iter().map(|t| (t.label(), Some(t.id)))).collect();
    let sep = " │ ";
    let total = |all: &[(String, Option<usize>)], hidden: usize| {
        1 + all.iter().map(|(l, _)| l.width()).sum::<usize>() + sep.width() * (all.len() - 1) + if hidden > 0 { format!("‹{hidden}").width() + sep.width() } else { 0 }
    };
    let room = (width as usize).saturating_sub(tail.width());
    let mut hidden = 0;
    while total(&all, hidden) > room
        && let Some(i) = all.iter().skip(1).position(|(_, id)| *id != r.shown).map(|i| i + 1)
    {
        all.remove(i);
        hidden += 1;
    }
    let (on, off) = (Style::default().fg(ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED), Style::default().fg(Color::Gray));
    let mut spans = vec![Span::raw(" ")];
    let mut hits = vec![];
    let mut x = 1u16;
    for (i, (label, id)) in all.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(sep, Style::default().fg(DIM)));
            x += sep.width() as u16;
        }
        if i == 1 && hidden > 0 {
            let n = format!("‹{hidden}");
            x += n.width() as u16 + sep.width() as u16;
            spans.push(Span::styled(n, off));
            spans.push(Span::styled(sep, Style::default().fg(DIM)));
        }
        let w = label.width() as u16;
        hits.push((x, w, id));
        x += w;
        spans.push(Span::styled(label, if id == r.shown { on } else { off }));
    }
    spans.push(Span::styled(tail, off));
    (Line::from(spans), hits)
}

/// A grid's area and, with the value panel on and room for both, the panel's on its right (40%; the grid keeps at least 30 columns)
pub(super) fn value_split(area: Rect, on: bool) -> (Rect, Option<Rect>) {
    if !on || area.width < 60 {
        return (area, None);
    }
    let [grid, panel] = Layout::horizontal([Constraint::Min(30), Constraint::Percentage(40)]).areas(area);
    (grid, Some(panel))
}

pub(super) fn find_caret(f: &mut Frame, area: Rect) -> Option<(u16, u16)> {
    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if buf[(x, y)].underline_color == CARET_MARK {
                return Some((x, y));
            }
        }
    }
    None
}

/// The console's SQL colors (Console::colors in kit::sql_style) on the rows on screen, under the marks and underlines drawn after.
/// Rows and columns are counted from the cursor cell just drawn, as for the gutter marks; a tab spans up to the next stop, as
/// tui-textarea draws it. Cells with a background (selection, search matches) and the caret's keep their look
pub(super) fn draw_sql_colors(f: &mut Frame, c: &crate::app::Console, ed: Rect, caret: Option<(u16, u16)>) {
    use unicode_width::UnicodeWidthChar;
    // Each character's byte, display column and width
    fn cells(l: &str, tab: i64) -> impl Iterator<Item = (usize, i64, i64)> + '_ {
        let mut col = 0i64;
        l.char_indices().map(move |(b, ch)| {
            let w = if ch == '\t' { tab - col % tab } else { ch.width().unwrap_or(0) as i64 };
            col += w;
            (b, col - w, w)
        })
    }
    let Some((cx, cy)) = caret else { return };
    let (lines, (crow, ccol), tab) = (c.editor.lines(), c.editor.cursor(), c.editor.tab_length().max(1) as i64);
    let cells = |l| cells(l, tab);
    let top = crow as i64 - (cy as i64 - ed.y as i64 - 1);
    let caret_col = lines.get(crow).map_or(0, |l| cells(l).nth(ccol).map_or_else(|| cells(l).last().map_or(0, |(_, x, w)| x + w), |(_, x, _)| x));
    let shift = cx as i64 - caret_col;
    let (left, right) = (ed.x as i64 + 1, ed.right() as i64 - 1);
    let buf = f.buffer_mut();
    for y in ed.y + 1..ed.bottom().saturating_sub(1) {
        let row = top + (y - ed.y - 1) as i64;
        let (Some(line), Some(spans)) = (lines.get(row.max(0) as usize).filter(|_| row >= 0), c.colors.rows.get(row.max(0) as usize)) else { continue };
        let mut spans = spans.iter().peekable();
        for (b, col, w) in cells(line) {
            while spans.peek().is_some_and(|s| s.1 <= b) {
                spans.next();
            }
            let Some(style) = spans.peek().filter(|s| s.0 <= b).and_then(|s| sql_style(s.2)) else { continue };
            for x in (shift + col..shift + col + w).filter(|x| (left..right).contains(x)) {
                let cell = &mut buf[(x as u16, y)];
                if cell.bg == Color::Reset && !cell.modifier.contains(Modifier::REVERSED) {
                    cell.set_style(style);
                }
            }
            if shift + col >= right {
                break;
            }
        }
    }
}

/// Gutter marks (✓ done, ✗ failed, … running) in the blank first cell of the line-number column, on each run statement's first line.
/// Rows on screen are counted from the cursor cell just drawn; a row whose line number isn't drawn there (scrolled sideways) gets none
pub(super) fn draw_marks(f: &mut Frame, c: &crate::app::Console, ed: Rect, caret: Option<(u16, u16)>) {
    use crate::app::MarkState;
    let digits = c.editor.lines().len().to_string().len();
    let Some((_, cy)) = caret.filter(|_| ed.width as usize > digits + 4) else { return };
    let top = c.editor.cursor().0 as i64 - (cy as i64 - ed.y as i64 - 1);
    let buf = f.buffer_mut();
    for y in ed.y + 1..ed.bottom().saturating_sub(1) {
        let row = top + (y - ed.y - 1) as i64;
        let Some(state) = (row >= 0).then(|| c.marks.at(row as usize)).flatten() else { continue };
        let num = format!(" {:>digits$} ", row + 1);
        if (0..num.len() as u16).any(|i| buf[(ed.x + 1 + i, y)].symbol() != &num[i as usize..i as usize + 1]) {
            continue;
        }
        let color = match state {
            MarkState::Running => ACCENT,
            MarkState::Ok => OK,
            MarkState::Err => DANGER,
        };
        buf[(ed.x + 1, y)].set_char(state.symbol()).set_fg(color);
    }
}

/// Unknown names (complete::marks) underlined in red. Rows on screen are counted from the cursor cell just drawn, as for the gutter
/// marks, and so are the text's columns (tui-textarea scrolls sideways without saying how far)
pub(super) fn draw_names(f: &mut Frame, c: &crate::app::Console, ed: Rect, caret: Option<(u16, u16)>) {
    use unicode_width::UnicodeWidthStr;
    let Some((cx, cy)) = caret.filter(|_| !c.assist.names.is_empty()) else { return };
    let (crow, ccol) = c.editor.cursor();
    let lines = c.editor.lines();
    let top = crow as i64 - (cy as i64 - ed.y as i64 - 1);
    let x0 = (ed.x + 1 + lines.len().to_string().len() as u16 + 2) as i64;
    let width = |row: usize, col: usize| lines.get(row).map_or(0, |l| l.chars().take(col).collect::<String>().width() as i64);
    let scroll = x0 + width(crow, ccol) - cx as i64;
    let (right, bottom) = (ed.right().saturating_sub(1) as i64, ed.bottom().saturating_sub(1) as i64);
    let buf = f.buffer_mut();
    for m in &c.assist.names {
        let y = ed.y as i64 + 1 + m.row as i64 - top;
        if y <= ed.y as i64 || y >= bottom {
            continue;
        }
        for x in (x0 + width(m.row, m.from) - scroll).max(x0)..(x0 + width(m.row, m.to) - scroll).min(right) {
            let cell = &mut buf[(x as u16, y as u16)];
            // The cursor cell keeps its mark (find_caret reads it)
            if cell.underline_color != CARET_MARK {
                cell.modifier.insert(Modifier::UNDERLINED);
                cell.underline_color = DANGER;
            }
        }
    }
}

/// Unclosed quotes and brackets (qurry_shared::pairs::unbalanced): the character underlined in the danger color, and its line's number
/// in the danger color. Rows and the sideways scroll are read from the cursor cell just drawn, as for the gutter marks
pub(super) fn draw_unbalanced(f: &mut Frame, c: &crate::app::Console, ed: Rect, caret: Option<(u16, u16)>, text: &str, open: &[qurry_shared::pairs::Unbalanced]) {
    use unicode_width::UnicodeWidthStr;
    let Some((cx, cy)) = caret.filter(|_| !open.is_empty()) else { return };
    let lines = c.editor.lines();
    let digits = lines.len().to_string().len();
    let x0 = (ed.x + 1 + digits as u16 + 2) as i64;
    let (crow, ccol) = c.editor.cursor();
    // Display cells before `col`, a tab running to the next tab stop as tui-textarea draws it
    let tab = c.editor.tab_length().max(1) as i64;
    let width = |row: usize, col: usize| {
        lines[row].chars().take(col).fold(0, |w, ch| if ch == '\t' { w + tab - w % tab } else { w + ch.to_string().width() as i64 })
    };
    // Columns scrolled off to the left
    let shift = x0 + width(crow, ccol) - cx as i64;
    let top = crow as i64 - (cy as i64 - ed.y as i64 - 1);
    let buf = f.buffer_mut();
    for u in open {
        let (row, col) = crate::input::pairs::position(text, u.start);
        let y = ed.y as i64 + 1 + row as i64 - top;
        if y <= ed.y as i64 || y >= ed.bottom() as i64 - 1 {
            continue;
        }
        let y = y as u16;
        for x in ed.x + 2..ed.x + 2 + digits as u16 {
            buf[(x, y)].set_fg(DANGER);
        }
        let x = x0 + width(row, col) - shift;
        if x >= x0 && x < ed.right() as i64 - 1 {
            let cell = &mut buf[(x as u16, y)];
            cell.set_fg(DANGER);
            cell.modifier |= Modifier::UNDERLINED | Modifier::BOLD;
        }
    }
}

/// The hint line above the caret (below on the editor's first row): the unknown name the caret is on with its fixes and the key
/// that lists them, else the signature of the call the caret is in, its current parameter highlighted
pub(super) fn draw_hint(f: &mut Frame, c: &crate::app::Console, ed: Rect, x: u16, y: u16, fix_key: &str) {
    let (row, col) = c.editor.cursor();
    let dim = Style::default().fg(DIM);
    let line = if let Some(m) = c.assist.at(row, col) {
        let fixes = if m.fixes.is_empty() { String::new() } else { format!(" · {fix_key} {}", m.fixes.join(", ")) };
        Line::from(vec![Span::styled(format!(" {}", m.message), Style::default().fg(DANGER)), Span::styled(format!("{fixes} "), dim)])
    } else if let Some(sig) = &c.assist.sig {
        // The overload with a parameter at the caret's, else the first
        let s = sig.signatures.iter().find(|s| s.params.len() > sig.active).unwrap_or(&sig.signatures[0]);
        let (head, tail) = (&s.label[..s.label.find('(').unwrap_or(0)], s.label.rfind(')').map_or("", |i| &s.label[i..]));
        let mut spans = vec![Span::raw(format!(" {head}("))];
        for (i, p) in s.params.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(", "));
            }
            let on = i == sig.active || i + 1 == s.params.len() && sig.active >= i && p.contains('…');
            spans.push(Span::styled(p.clone(), if on { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default() }));
        }
        spans.push(Span::raw(format!("{tail} ")));
        if sig.signatures.len() > 1 {
            spans.push(Span::styled(format!("(+{} more) ", sig.signatures.len() - 1), dim));
        }
        Line::from(spans)
    } else {
        return;
    };
    let screen = f.area();
    let w = (line.width() as u16).min(screen.width);
    let y = if y > ed.y + 1 { y - 1 } else { y + 1 };
    let r = Rect { x: x.min(screen.right().saturating_sub(w)), y: y.min(screen.bottom().saturating_sub(1)), width: w, height: 1 };
    clear(f, r);
    f.render_widget(Paragraph::new(line).style(Style::default().bg(POPUP_BG)), r);
}

/// The text an Execute chooser option would run, on the selection background (exactly its range, as the GUI highlights it)
pub(super) fn draw_preview(f: &mut Frame, c: &crate::app::Console, ed: Rect, caret: Option<(u16, u16)>, spans: &[crate::app::Span]) {
    for &span in spans {
        paint_span(f, c, ed, caret, span, |cell| {
            cell.set_bg(SEL);
        });
    }
}

/// The last run's error, underlined in the danger colour where the server points (else over the statement). An edit clears it
pub(super) fn draw_run_error(f: &mut Frame, c: &mut crate::app::Console, ed: Rect, caret: Option<(u16, u16)>) {
    if c.run_error.as_ref().is_some_and(|e| e.lines != c.editor.lines()) {
        c.run_error = None;
    }
    if let Some(e) = &c.run_error {
        paint_span(f, c, ed, caret, e.span, |cell| {
            cell.set_style(Style::default().fg(DANGER).add_modifier(Modifier::UNDERLINED));
        });
    }
}

/// Restyle the cells of an editor span (text columns, past the line numbers). Rows and columns on screen are counted from the
/// cursor cell just drawn (tui-textarea scrolls both ways and doesn't say how far), as for the gutter marks
fn paint_span(f: &mut Frame, c: &crate::app::Console, ed: Rect, caret: Option<(u16, u16)>, ((r0, c0), (r1, c1)): crate::app::Span, style: impl Fn(&mut ratatui::buffer::Cell)) {
    use unicode_width::UnicodeWidthChar;
    let Some((cx, cy)) = caret else { return };
    let lines = c.editor.lines();
    let (crow, ccol) = c.editor.cursor();
    let width = |l: &str, n: usize| l.chars().take(n).map(|ch| ch.width().unwrap_or(0) as i64).sum::<i64>();
    let top = crow as i64 - (cy as i64 - ed.y as i64 - 1);
    let x0 = ed.x + 1 + lines.len().to_string().len() as u16 + 2;
    let left = width(lines.get(crow).map_or("", |l| l.as_str()), ccol) - (cx as i64 - x0 as i64);
    let right = ed.right().saturating_sub(1) as i64;
    let buf = f.buffer_mut();
    for y in ed.y + 1..ed.bottom().saturating_sub(1) {
        let row = top + (y - ed.y - 1) as i64;
        let Some(line) = (row >= r0 as i64 && row <= r1 as i64).then(|| lines.get(row as usize)).flatten() else { continue };
        let from = if row as usize == r0 { c0 } else { 0 };
        let to = if row as usize == r1 { c1 } else { line.chars().count() };
        for (k, ch) in line.chars().enumerate().take(to).skip(from) {
            let x = x0 as i64 + width(line, k) - left;
            for dx in 0..ch.width().unwrap_or(0).max(1) as i64 {
                if (x0 as i64..right).contains(&(x + dx)) {
                    style(&mut buf[((x + dx) as u16, y)]);
                }
            }
        }
    }
}
