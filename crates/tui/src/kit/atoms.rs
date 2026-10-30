//! Atoms: color tokens and text pieces (Span) — badges · keys · dim text · icons. Plus line fitting (fit_spans) · area clearing (clear)
use ratatui::prelude::*;
use ratatui::widgets::Clear;

// ── Colors (same values as GUI tokens.css) ──
/// Accent: focused border, key names, links
pub const ACCENT: Color = Color::Rgb(84, 138, 247);
/// Dim text · unfocused border
pub const DIM: Color = Color::DarkGray;
/// Selected row (focused / unfocused)
pub const SEL: Color = Color::Rgb(46, 67, 110);
pub const SEL_BLUR: Color = Color::Rgb(50, 52, 56);
/// Surfaces like the status line · result header
pub const SURFACE: Color = Color::Rgb(43, 45, 48);
/// Popup · autocomplete background
pub const POPUP_BG: Color = Color::Rgb(30, 31, 34);
/// Drop target indicator
pub const DROP: Color = Color::Rgb(40, 70, 120);
pub const OK: Color = Color::Rgb(95, 184, 101);
pub const WARN: Color = Color::Rgb(201, 162, 109);
pub const DANGER: Color = Color::Rgb(247, 84, 100);
/// Number cells · table lines
pub const NUMBER: Color = Color::Rgb(42, 172, 184);
pub const GRID_LINE: Color = Color::Rgb(96, 99, 106);
pub const CELL_EDITED: Color = Color::Rgb(36, 52, 84);
pub const CELL_RANGE: Color = Color::Rgb(38, 40, 46);
// Tree · autocomplete icon colors
pub const C_DB: Color = Color::Rgb(111, 179, 210);
pub const C_VIEW: Color = Color::Rgb(127, 176, 110);
pub const C_SEQUENCE: Color = Color::Rgb(201, 162, 109);
pub const C_ROUTINE: Color = Color::Rgb(86, 168, 245);
pub const C_KEYWORD: Color = Color::Rgb(204, 120, 50);

/// Selected row background
pub fn selected(focused: bool) -> Color {
    if focused {
        SEL
    } else {
        SEL_BLUR
    }
}

/// Key notation (⌃k · F5): accent color
pub fn key(k: &str) -> Span<'static> {
    Span::styled(k.to_string(), Style::default().fg(ACCENT))
}
/// Dim text (descriptions · counts · positions)
pub fn muted(s: impl Into<String>) -> Span<'static> {
    Span::styled(s.into(), Style::default().fg(DIM))
}
/// Icon (one character + a space) and its color
pub fn icon(glyph: &str, color: Color) -> Span<'static> {
    Span::styled(format!("{glyph} "), Style::default().fg(color))
}

/// Color swatch (Data Source color): six cells of colored background
pub fn swatch(c: Color) -> Span<'static> {
    Span::styled("      ", Style::default().bg(c))
}

/// Badge: text on a colored background, like [ PROD ]
pub fn badge(text: &str, bg: Color) -> Span<'static> {
    Span::styled(format!(" {text} "), Style::default().fg(on(bg)).bg(bg).add_modifier(Modifier::BOLD))
}

pub fn lighten(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r.saturating_add(r / 2 + 16), g.saturating_add(g / 2 + 16), b.saturating_add(b / 2 + 16)),
        c => c,
    }
}

/// Clear the popup area. If a double-width character (e.g. Hangul) on the screen behind straddles the popup edge, the terminal draws it
/// over the popup and cuts the border, so straddling characters are replaced with spaces.
pub fn clear(f: &mut Frame, r: Rect) {
    use unicode_width::UnicodeWidthStr;
    let buf = f.buffer_mut();
    let area = buf.area;
    for y in r.top()..r.bottom().min(area.bottom()) {
        if r.x > area.x && buf[(r.x - 1, y)].symbol().width() > 1 {
            buf[(r.x - 1, y)].set_symbol(" ");
        }
        let last = r.right().saturating_sub(1);
        if r.right() < area.right() && last >= r.x && buf[(last, y)].symbol().width() > 1 {
            let st = buf[(last, y)].style();
            buf[(r.right(), y)].set_symbol(" ").set_style(st);
        }
    }
    f.render_widget(Clear, r);
}

/// Text color on a background: black on light backgrounds, white on dark
pub fn on(bg: Color) -> Color {
    match bg {
        Color::Rgb(r, g, b) if 299 * r as u32 + 587 * g as u32 + 114 * b as u32 > 128_000 => Color::Black,
        _ => Color::White,
    }
}

/// Make a line exactly width cells: skip the first skip cells (horizontal scroll), pad with spaces if short, cut if long.
/// If a double-width character (e.g. Hangul) is cut at the end or start, replace that spot with spaces in the same style —
/// otherwise the terminal leaves that cell without a background and the row color looks broken.
pub fn fit_spans(spans: Vec<Span<'static>>, skip: usize, width: usize) -> Vec<Span<'static>> {
    use unicode_width::UnicodeWidthChar;
    let mut out: Vec<Span<'static>> = vec![];
    let (mut col, mut used) = (0usize, 0usize);
    for sp in spans {
        let mut buf = String::new();
        for ch in sp.content.chars() {
            let w = ch.width().unwrap_or(0);
            let (start, end) = (col, col + w);
            col = end;
            if end <= skip || used >= width {
                continue;
            }
            if start < skip || used + w > width {
                // Double-width character cut at the start or end → spaces for the visible cells
                let vis = (end.min(skip + width) - start.max(skip)).min(width - used);
                buf.push_str(&" ".repeat(vis));
                used += vis;
            } else {
                buf.push(ch);
                used += w;
            }
        }
        if !buf.is_empty() {
            out.push(Span::styled(buf, sp.style));
        }
    }
    if used < width {
        out.push(Span::raw(" ".repeat(width - used)));
    }
    out
}

pub fn hex(c: &str) -> Option<Color> {
    let c = c.trim_start_matches('#');
    if c.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(c, 16).ok()?;
    Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

