//! 유기체: 칸 · 팝업 — 패널 테두리, 가운데 자리, 팝업 (보통 · 위험), 확인 창 내용
use super::atoms::*;
use super::molecules::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders};

/// 패널 테두리 (포커스면 굵게 · 강조색)
pub fn panel(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Plain })
        .title(title)
        .border_style(Style::default().fg(if focused { ACCENT } else { DIM }))
}

/// 화면 가운데 (위쪽 1/3) 에 w × h
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(2));
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height.saturating_sub(h)) / 3, width: w, height: h }
}

/// 팝업 자리: 뒤를 비우고 강조 테두리를 그린 뒤 안쪽 칸을 돌려준다
pub fn popup(f: &mut Frame, r: Rect, title: impl Into<Line<'static>>) -> Rect {
    clear(f, r);
    let b = panel(title, true);
    let inner = b.inner(r);
    f.render_widget(b.style(Style::default().bg(POPUP_BG)), r);
    inner
}

/// 위험한 확인 팝업: 테두리가 빨갛다 (운영 DB · DROP · WHERE 없는 DELETE)
pub fn popup_danger(f: &mut Frame, r: Rect, title: impl Into<Line<'static>>) -> Rect {
    clear(f, r);
    let b = panel(title, true).border_style(Style::default().fg(DANGER));
    let inner = b.inner(r);
    f.render_widget(b.style(Style::default().bg(POPUP_BG)), r);
    inner
}

/// 확인 창 내용: 질문 줄들 ("→ N rows" 는 0 흐리게 · 1~99 WARN · 100+ DANGER) + 키 안내
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

/// 줄바꿈 (Wrap) 했을 때 몇 줄이 되는지 — 팝업 높이를 잡을 때 (모자라면 아래 안내가 잘린다)
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
        // 10 칸 폭: 1 + 1 + 3
        assert_eq!(wrapped_height(&lines, 10), 5);
    }
}
