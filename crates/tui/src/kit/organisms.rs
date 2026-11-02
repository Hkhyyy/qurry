//! Organisms: areas · popups — panel borders, centered area, popups (normal · danger), confirm dialog content
use super::atoms::*;
use super::molecules::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

/// Panel border (bold · accent color when focused)
pub fn panel(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Plain })
        .title(title)
        .border_style(Style::default().fg(if focused { ACCENT } else { DIM }))
}

/// w × h at the center of the screen (upper third)
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(2));
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height.saturating_sub(h)) / 3, width: w, height: h }
}

/// Popup area: clears behind, draws an accent border, and returns the inner area
pub fn popup(f: &mut Frame, r: Rect, title: impl Into<Line<'static>>) -> Rect {
    clear(f, r);
    let b = panel(title, true);
    let inner = b.inner(r);
    f.render_widget(b.style(Style::default().bg(POPUP_BG)), r);
    inner
}

/// Dangerous confirm popup: red border (production DB · DROP · DELETE without WHERE)
pub fn popup_danger(f: &mut Frame, r: Rect, title: impl Into<Line<'static>>) -> Rect {
    clear(f, r);
    let b = panel(title, true).border_style(Style::default().fg(DANGER));
    let inner = b.inner(r);
    f.render_widget(b.style(Style::default().bg(POPUP_BG)), r);
    inner
}

/// Confirm dialog content: question lines ("→ N rows": 0 dimmed · 1~99 WARN · 100+ DANGER) + key hints
pub fn confirm_lines(text: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = text
        .lines()
        .map(|l| match l.trim_start().strip_prefix("→ ") {
            Some(v) => {
                let n: Option<u64> = v.split_whitespace().next().and_then(|x| x.replace(',', "").parse().ok());
                let c = match n {
                    Some(0) => DIM,
                    Some(n) if n < 100 => WARN,
                    _ => DANGER,
                };
                Line::from(vec![Span::raw(l[..l.len() - v.len()].to_string()), Span::styled(v.to_string(), Style::default().fg(c).add_modifier(Modifier::BOLD))])
            }
            None => Line::raw(l.to_string()),
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(key_hints(&[("y / Enter", "yes"), ("n / Esc", "no")]));
    lines
}

/// How many lines it becomes when wrapped (Wrap, by word as Paragraph wraps it) — for sizing the popup (if short, the hints below get cut)
pub fn wrapped_height(lines: &[Line], width: u16) -> u16 {
    Paragraph::new(lines.to_vec()).wrap(Wrap { trim: false }).line_count(width.max(1)) as u16
}

/// Lines cut to `height` rows of `width` (wrapped): the lines that don't fit give way to one saying how many are hidden
pub fn cut_lines(lines: Vec<Line<'static>>, width: u16, height: u16) -> Vec<Line<'static>> {
    if wrapped_height(&lines, width) <= height {
        return lines;
    }
    let mut keep = lines.len();
    loop {
        keep -= 1;
        let mut v = lines[..keep].to_vec();
        v.push(Line::styled(format!("⋯ {} more lines hidden", lines.len() - keep), Style::default().fg(WARN)));
        if keep == 0 || wrapped_height(&v, width) <= height {
            return v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_lines_says_how_many_are_hidden() {
        let lines: Vec<Line<'static>> = ["one", "two", "three", "four"].into_iter().map(Line::raw).collect();
        assert_eq!(cut_lines(lines.clone(), 30, 4).len(), 4);
        let cut: Vec<String> = cut_lines(lines, 30, 3).iter().map(|l| l.to_string()).collect();
        assert_eq!(cut, ["one", "two", "⋯ 2 more lines hidden"]);
    }

    #[test]
    fn wrapped_height_counts_folded_lines() {
        let lines = vec![Line::raw("a".repeat(10)), Line::raw(""), Line::raw("b".repeat(25))];
        // 10 cells wide: 1 + 1 + 3
        assert_eq!(wrapped_height(&lines, 10), 5);
    }

    #[test]
    fn wrapped_height_wraps_by_word() {
        // 20 cells fit 2 rows of 10 by count, but no two words share a row
        let lines = vec![Line::raw("aaaaaa bbbbbb cccccc")];
        assert_eq!(wrapped_height(&lines, 10), 3);
    }
}
