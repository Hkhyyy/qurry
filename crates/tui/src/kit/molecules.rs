//! Molecules: a Line made of atoms — list · tree · palette · autocomplete rows (Row), key hints, a form field, section titles
use super::atoms::*;
use ratatui::prelude::*;

/// Hint fragment: "key description" — key in the accent color
pub fn key_hint(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![Span::styled(key.to_string(), Style::default().fg(ACCENT)), Span::styled(format!(" {label}"), Style::default().fg(Color::Gray))]
}

/// Key hint line: "y / Enter yes · n / Esc no"
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

/// Section title line (group names in the F1 window etc.): bold white text
pub fn section(title: impl Into<String>) -> Line<'static> {
    Line::styled(title.into(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
}

/// Key · description line (F1 keymap): keys padded and in the accent color. custom = key changed by the user (yellow)
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

/// Form field: right-aligned label + value (when selected, label in accent color, value on the selection background)
pub fn field_row(label: &str, label_width: usize, value: impl Into<String>, selected: bool) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{label:>label_width$} "), Style::default().fg(if selected { ACCENT } else { DIM })),
        Span::styled(value.into(), if selected { Style::default().bg(SEL) } else { Style::default() }),
    ]
}

/// List row (tree · palette · autocomplete · schema picker): [indent][▸][icon] name description ……… right.
/// With width, exactly that width (horizontal scroll skip, handles double-width chars cut at the edge) — the row color runs to the end
pub struct Row {
    indent: usize,
    /// Reserves the expander slot (tree: leaves get a blank so rows line up)
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
    /// Tree depth (two cells each)
    pub fn indent(mut self, depth: usize) -> Self {
        self.indent = depth;
        self
    }
    /// Expander: Some(true) ▾ · Some(false) ▸ · None blank (leaf). Always takes two cells when called
    pub fn chevron(mut self, open: Option<bool>) -> Self {
        self.slot = true;
        self.chevron = open;
        self
    }
    /// Fragment before the name (icon · check mark)
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
    /// Dimmed description after the name
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        let h = h.into();
        if !h.is_empty() {
            self.hint = Some(Span::styled(format!(" {h}"), Style::default().fg(DIM)));
        }
        self
    }
    /// Right end (right-aligned when width is set)
    pub fn trailing(mut self, t: impl Into<String>) -> Self {
        self.trailing = Some(t.into());
        self
    }
    /// Row background (Data Source color). selected wins
    pub fn bg(mut self, c: Option<Color>) -> Self {
        if c.is_some() {
            self.bg = c;
        }
        self
    }
    /// Selected row (focused / unfocused)
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
    /// A row without fitted width (Paragraph truncates it)
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
    /// Exactly width cells (skip = horizontal scroll). The right fragment is aligned to the end
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
