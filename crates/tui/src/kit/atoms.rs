//! 원자: 색 토큰과 글자 조각 (Span) — 배지 · 키 · 흐린 글자 · 아이콘. 그리고 줄 맞추기 (fit_spans) · 자리 비우기 (clear)
use ratatui::prelude::*;
use ratatui::widgets::Clear;

// ── 색 (GUI tokens.css 와 같은 값) ──
/// 강조: 포커스된 테두리, 키 이름, 링크
pub const ACCENT: Color = Color::Rgb(84, 138, 247);
/// 흐린 글자 · 포커스 없는 테두리
pub const DIM: Color = Color::DarkGray;
/// 고른 줄 (포커스 있음 / 없음)
pub const SEL: Color = Color::Rgb(46, 67, 110);
pub const SEL_BLUR: Color = Color::Rgb(50, 52, 56);
/// 상태 줄 · 결과 머리 같은 면
pub const SURFACE: Color = Color::Rgb(43, 45, 48);
/// 팝업 · 자동완성 바탕
pub const POPUP_BG: Color = Color::Rgb(30, 31, 34);
/// 끌어다 놓을 자리 표시
pub const DROP: Color = Color::Rgb(40, 70, 120);
pub const OK: Color = Color::Rgb(95, 184, 101);
pub const WARN: Color = Color::Rgb(201, 162, 109);
pub const DANGER: Color = Color::Rgb(247, 84, 100);
/// 숫자 칸 · 표 선
pub const NUMBER: Color = Color::Rgb(42, 172, 184);
pub const GRID_LINE: Color = Color::Rgb(96, 99, 106);
pub const CELL_EDITED: Color = Color::Rgb(36, 52, 84);
pub const CELL_RANGE: Color = Color::Rgb(38, 40, 46);
// 트리 · 자동완성 아이콘 색
pub const C_DB: Color = Color::Rgb(111, 179, 210);
pub const C_VIEW: Color = Color::Rgb(127, 176, 110);
pub const C_SEQUENCE: Color = Color::Rgb(201, 162, 109);
pub const C_ROUTINE: Color = Color::Rgb(86, 168, 245);
pub const C_KEYWORD: Color = Color::Rgb(204, 120, 50);

/// 고른 줄 바탕
pub fn selected(focused: bool) -> Color {
    if focused {
        SEL
    } else {
        SEL_BLUR
    }
}

/// 키 표기 (⌃k · F5): 강조색
pub fn key(k: &str) -> Span<'static> {
    Span::styled(k.to_string(), Style::default().fg(ACCENT))
}
/// 흐린 글자 (설명 · 개수 · 위치)
pub fn muted(s: impl Into<String>) -> Span<'static> {
    Span::styled(s.into(), Style::default().fg(DIM))
}
/// 아이콘 (글자 하나 + 빈칸) 과 그 색
pub fn icon(glyph: &str, color: Color) -> Span<'static> {
    Span::styled(format!("{glyph} "), Style::default().fg(color))
}

/// 색 견본 (Data Source 색): 여섯 칸 색 바탕
pub fn swatch(c: Color) -> Span<'static> {
    Span::styled("      ", Style::default().bg(c))
}

/// 배지: [ PROD ] 처럼 색 바탕 글자
pub fn badge(text: &str, bg: Color) -> Span<'static> {
    Span::styled(format!(" {text} "), Style::default().fg(on(bg)).bg(bg).add_modifier(Modifier::BOLD))
}

pub fn lighten(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r.saturating_add(r / 2 + 16), g.saturating_add(g / 2 + 16), b.saturating_add(b / 2 + 16)),
        c => c,
    }
}

/// 팝업 자리를 비운다. 뒤 화면의 한글 같은 두 칸 글자가 팝업 가장자리에 걸치면 터미널이 그 글자를
/// 팝업 위에 그려 테두리가 잘리므로, 걸친 글자는 빈칸으로 바꾼다.
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

/// 배경 위 글자색: 밝은 배경엔 검정, 어두운 배경엔 흰색
pub fn on(bg: Color) -> Color {
    match bg {
        Color::Rgb(r, g, b) if 299 * r as u32 + 587 * g as u32 + 114 * b as u32 > 128_000 => Color::Black,
        _ => Color::White,
    }
}

/// 한 줄을 정확히 width 칸으로: 앞 skip 칸은 건너뛰고 (가로 스크롤), 모자라면 빈칸, 넘치면 자른다.
/// 두 칸 글자(한글)가 끝이나 처음에 걸리면 그 자리를 같은 스타일의 빈칸으로 바꾼다 —
/// 그냥 두면 터미널이 그 칸을 바탕색 없이 비워 줄 색이 끊겨 보인다.
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
                // 앞이나 끝에 걸친 두 칸 글자 → 보이는 칸만큼 빈칸
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

