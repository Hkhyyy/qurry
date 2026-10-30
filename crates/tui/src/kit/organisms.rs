//! Organisms: areas · popups — panel borders, centered area, popups (normal · danger), confirm dialog content
use super::atoms::*;
use super::molecules::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders};

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

/// How many lines it becomes when wrapped (Wrap) — for sizing the popup (if short, the hints below get cut)
pub fn wrapped_height(lines: &[Line], width: u16) -> u16 {
    let w = width.max(1) as usize;
    lines.iter().map(|l| l.width().max(1).div_ceil(w)).sum::<usize>() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_height_counts_folded_lines() {
        let lines = vec![Line::raw("a".repeat(10)), Line::raw(""), Line::raw("b".repeat(25))];
        // 10 cells wide: 1 + 1 + 3
        assert_eq!(wrapped_height(&lines, 10), 5);
    }
}
