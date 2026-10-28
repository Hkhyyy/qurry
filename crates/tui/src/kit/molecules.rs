//! 분자: 원자를 묶은 한 줄 (Line) — 목록 · 트리 · 팔레트 · 자동완성 줄 (Row), 키 안내, 폼 한 칸, 구역 제목
use super::atoms::*;
use ratatui::prelude::*;

/// 안내 줄 조각: "키 설명" — 키는 강조색
pub fn key_hint(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![Span::styled(key.to_string(), Style::default().fg(ACCENT)), Span::styled(format!(" {label}"), Style::default().fg(Color::Gray))]
}

/// 키 안내 줄: "y / Enter yes · n / Esc no"
pub fn key_hints(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![];
    for (i, (k, l)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(DIM)));
        }
        spans.extend(key_hint(k, l));
    }
    Line::from(spans)
}

/// 구역 제목 줄 (F1 창의 묶음 이름 등): 흰 굵은 글자
pub fn section(title: impl Into<String>) -> Line<'static> {
    Line::styled(title.into(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
}

/// 키 · 설명 한 줄 (F1 키맵): 키는 폭을 맞춰 강조색. custom = 사용자가 바꾼 키 (노랑)
pub fn key_row(keys: &str, desc: impl Into<String>, key_width: usize, custom: bool, selected: bool) -> Line<'static> {
    let mut l = Line::from(vec![
        Span::styled(format!("  {keys:<key_width$}"), Style::default().fg(if custom { Color::Yellow } else { ACCENT })),
        Span::raw(desc.into()),
        Span::styled(if custom { "  (custom)" } else { "" }, Style::default().fg(DIM)),
    ]);
    if selected {
        l = l.style(Style::default().bg(SEL));
    }
    l
}

/// 폼 한 칸: 오른쪽 맞춘 라벨 + 값 (고르면 라벨은 강조색, 값은 선택 바탕)
pub fn field_row(label: &str, label_width: usize, value: impl Into<String>, selected: bool) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{label:>label_width$} "), Style::default().fg(if selected { ACCENT } else { DIM })),
        Span::styled(value.into(), if selected { Style::default().bg(SEL) } else { Style::default() }),
    ]
}

/// 목록 한 줄 (트리 · 팔레트 · 자동완성 · 스키마 고르기): [들여쓰기][▸][아이콘] 이름 설명 ……… 오른쪽.
/// width 를 주면 정확히 그 폭 (가로 스크롤 skip, 끝에 걸린 두 칸 글자 처리) — 줄 색이 끝까지 간다
pub struct Row {
    indent: usize,
    /// 펼침 자리를 잡는다 (트리: 잎도 빈칸으로 줄을 맞춘다)
    slot: bool,
    chevron: Option<bool>,
    lead: Vec<Span<'static>>,
    label: Span<'static>,
    hint: Option<Span<'static>>,
    trailing: Option<String>,
    bg: Option<Color>,
}
impl Row {
    pub fn new(label: impl Into<String>) -> Self {
        Row { indent: 0, slot: false, chevron: None, lead: vec![], label: Span::raw(label.into()), hint: None, trailing: None, bg: None }
    }
    /// 트리 깊이 (두 칸씩)
    pub fn indent(mut self, depth: usize) -> Self {
        self.indent = depth;
        self
    }
    /// 펼침 표시: Some(true) ▾ · Some(false) ▸ · None 빈칸 (잎). 부르면 늘 두 칸을 잡는다
    pub fn chevron(mut self, open: Option<bool>) -> Self {
        self.slot = true;
        self.chevron = open;
        self
    }
    /// 이름 앞 조각 (아이콘 · 체크 표시)
    pub fn lead(mut self, s: Span<'static>) -> Self {
        self.lead.push(s);
        self
    }
    pub fn bold(mut self) -> Self {
        self.label = self.label.add_modifier(Modifier::BOLD);
        self
    }
    pub fn label_style(mut self, st: Style) -> Self {
        self.label = self.label.style(st);
        self
    }
    /// 이름 뒤 흐린 설명
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        let h = h.into();
        if !h.is_empty() {
            self.hint = Some(Span::styled(format!(" {h}"), Style::default().fg(DIM)));
        }
        self
    }
    /// 오른쪽 끝 (width 가 있을 때 오른쪽 맞춤)
    pub fn trailing(mut self, t: impl Into<String>) -> Self {
        self.trailing = Some(t.into());
        self
    }
    /// 줄 바탕 (Data Source 색). selected 가 이긴다
    pub fn bg(mut self, c: Option<Color>) -> Self {
        if c.is_some() {
            self.bg = c;
        }
        self
    }
    /// 고른 줄 (포커스 있음 / 없음)
    pub fn selected(mut self, on: bool, focused: bool) -> Self {
        if on {
            self.bg = Some(selected(focused));
        }
        self
    }
    fn spans(self) -> (Vec<Span<'static>>, Option<String>, Option<Color>) {
        let mut v = vec![Span::raw("  ".repeat(self.indent))];
        match (self.slot, self.chevron) {
            (_, Some(open)) => v.push(Span::styled(if open { "▾ " } else { "▸ " }, Style::default().fg(DIM))),
            (true, None) => v.push(Span::raw("  ")),
            _ => {}
        }
        v.extend(self.lead);
        v.push(self.label);
        v.extend(self.hint);
        (v, self.trailing, self.bg)
    }
    /// 폭을 맞추지 않은 한 줄 (Paragraph 가 자른다)
    pub fn line(self) -> Line<'static> {
        let (mut v, t, bg) = self.spans();
        if let Some(t) = t {
            v.push(Span::styled(format!("  {t}"), Style::default().fg(DIM)));
        }
        let l = Line::from(v);
        match bg {
            Some(c) => l.style(Style::default().bg(c)),
            None => l,
        }
    }
    /// 정확히 width 칸 (skip = 가로 스크롤). 오른쪽 조각은 끝에 맞춘다
    pub fn fit(self, skip: usize, width: usize) -> Line<'static> {
        use unicode_width::UnicodeWidthStr;
        let (v, t, bg) = self.spans();
        let mut v = v;
        if let Some(t) = t {
            let used: usize = v.iter().map(|s| s.content.width()).sum();
            let tw = t.width() + 1;
            let room = (width + skip).saturating_sub(used);
            if room > tw {
                v.push(Span::raw(" ".repeat(room - tw)));
                v.push(Span::styled(format!("{t} "), Style::default().fg(DIM)));
            }
        }
        let l = Line::from(fit_spans(v, skip, width));
        match bg {
            Some(c) => l.style(Style::default().bg(c)),
            None => l,
        }
    }
}
