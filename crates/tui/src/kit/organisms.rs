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
            Some(v) => Line::from(vec![Span::raw(l[..l.len() - v.len()].to_string()), Span::styled(v.to_string(), count_style(row_count(v)))]),
            None => Line::raw(l.to_string()),
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(key_hints(&[("y / Enter", "yes"), ("n / Esc", "no")]));
    lines
}

/// The N of "→ N rows" (None when it isn't a number: "schema change", "can't count")
fn row_count(v: &str) -> Option<u64> {
    v.split_whitespace().next().and_then(|x| x.replace(',', "").parse().ok())
}

/// "→ N rows": 0 dimmed · 1~99 WARN · 100+ or unknown DANGER
fn count_style(n: Option<u64>) -> Style {
    let c = match n {
        Some(0) => DIM,
        Some(n) if n < 100 => WARN,
        _ => DANGER,
    };
    Style::default().fg(c).add_modifier(Modifier::BOLD)
}

/// How many lines it becomes when wrapped (Wrap, by word as Paragraph wraps it) — for sizing the popup (if short, the hints below get cut)
pub fn wrapped_height(lines: &[Line], width: u16) -> u16 {
    Paragraph::new(lines.to_vec()).wrap(Wrap { trim: false }).line_count(width.max(1)) as u16
}

/// Lines cut to `height` rows of `width` (wrapped): blank lines go first, then the lines that don't fit give way to one
/// saying how many are hidden and what their "→ N rows" add up to, so a confirm never hides what it would change.
/// Wraps each line once and stops at the first that doesn't fit: a confirm of thousands of statements draws in a frame
pub fn cut_lines(mut lines: Vec<Line<'static>>, width: u16, height: u16) -> Vec<Line<'static>> {
    let mut ends = fitting_rows(&lines, width, height);
    if ends.len() == lines.len() {
        return lines;
    }
    lines.retain(|l| !l.to_string().trim().is_empty());
    ends = fitting_rows(&lines, width, height);
    if ends.len() == lines.len() {
        return lines;
    }
    let counts: Vec<Option<Option<u64>>> = lines.iter().map(|l| l.to_string().trim_start().strip_prefix("→ ").map(row_count)).collect();
    let mut keep = ends.len();
    loop {
        let marker = hidden_marker(lines.len() - keep, &counts[keep..]);
        let above = keep.checked_sub(1).map_or(0, |i| ends[i]);
        if keep == 0 || above + wrapped_height(std::slice::from_ref(&marker), width) <= height {
            lines.truncate(keep);
            lines.push(marker);
            return lines;
        }
        keep -= 1;
    }
}

/// Wrapped rows up to the end of each line, for the first lines that fit in `height`
fn fitting_rows(lines: &[Line], width: u16, height: u16) -> Vec<u16> {
    let mut ends = Vec::new();
    let mut total = 0;
    for l in lines {
        total += wrapped_height(std::slice::from_ref(l), width);
        if total > height {
            break;
        }
        ends.push(total);
    }
    ends
}

/// "⋯ 4 lines hidden · 12,408 rows · 1 not counted": the hidden lines and the row counts among them
fn hidden_marker(n: usize, counts: &[Option<Option<u64>>]) -> Line<'static> {
    let mut text = format!("⋯ {n} lines hidden");
    let known: Vec<u64> = counts.iter().flatten().flatten().copied().collect();
    let total: u64 = known.iter().sum();
    if !known.is_empty() {
        text += &format!(" · {} row{}", crate::transfer::thousands(total), if total == 1 { "" } else { "s" });
    }
    let unknown = counts.iter().flatten().filter(|c| c.is_none()).count();
    if unknown > 0 {
        text += &format!(" · {unknown} not counted");
    }
    Line::styled(text, if known.is_empty() && unknown == 0 { Style::default().fg(WARN) } else { count_style(if unknown > 0 { None } else { Some(total) }) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_lines_says_how_many_are_hidden() {
        let lines: Vec<Line<'static>> = ["one", "two", "three", "four"].into_iter().map(Line::raw).collect();
        assert_eq!(cut_lines(lines.clone(), 30, 4).len(), 4);
        let cut: Vec<String> = cut_lines(lines, 30, 3).iter().map(|l| l.to_string()).collect();
        assert_eq!(cut, ["one", "two", "⋯ 2 lines hidden"]);
    }

    #[test]
    fn cut_lines_drops_blank_lines_first_and_adds_up_hidden_rows() {
        let lines: Vec<Line<'static>> = ["head", "", "a", "  → 1 row", "b", "  → 12,408 rows", "c", "  → schema change", ""].into_iter().map(Line::raw).collect();
        let cut: Vec<String> = cut_lines(lines.clone(), 60, 7).iter().map(|l| l.to_string()).collect();
        assert_eq!(cut, ["head", "a", "  → 1 row", "b", "  → 12,408 rows", "c", "  → schema change"]);
        let cut: Vec<String> = cut_lines(lines, 60, 3).iter().map(|l| l.to_string()).collect();
        assert_eq!(cut, ["head", "a", "⋯ 5 lines hidden · 12,409 rows · 1 not counted"]);
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
