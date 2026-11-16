use super::*;

pub(super) fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    if app.tabs.is_empty() {
        let key = |a| app.keys.keys(a).first().cloned().unwrap_or_default();
        let p = Paragraph::new(vec![
            Line::raw(""),
            Line::styled("Qurry", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::raw(""),
            Line::styled(format!("Not sure? Press {}", disp(&key(crate::app::Action::SearchEverywhere))), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Line::raw("search tables, columns, open tabs and actions"),
            Line::raw(""),
            Line::styled(
                format!(
                    "Enter on a data source connects · {} new console · {} all actions · F1 keymap",
                    disp(&key(crate::app::Action::NewConsole)),
                    disp(&key(crate::app::Action::Palette))
                ),
                Style::default().fg(DIM),
            ),
        ])
        .alignment(Alignment::Center)
        .block(panel("", false));
        f.render_widget(p, area);
        return;
    }
    app.hits.caret = None;
    for r in [&mut app.hits.editor, &mut app.hits.find, &mut app.hits.results, &mut app.hits.redis_keys, &mut app.hits.redis_value, &mut app.hits.console, &mut app.hits.divider] {
        *r = Rect::default();
    }
    app.hits.tabs.clear();
    app.hits.result_tabs.clear();
    app.hits.groups.clear();
    app.hits.splits.clear();
    if app.panes.is_split() {
        let panes = app.panes.clone();
        let fg = app.focused_group();
        draw_panes(f, app, &panes, area, &mut vec![], fg);
    } else {
        draw_group(f, app, None, area, true);
    }
    // Dragging a tab: paint where it will land
    if let Some(Drag::Tab { id, moved: true }) = app.drag {
        let (x, y) = app.drag_pos;
        let r = crate::input::drop_target(app, id, x, y).and_then(|t| {
            let body = |g: usize| app.hits.groups.iter().find(|x| x.0 == g).map(|x| (x.1, x.2));
            match t {
                DropTarget::NewSplit(g, SplitDir::Right) => body(g).map(|(_, b)| Rect { x: b.x + b.width / 2, width: b.width - b.width / 2, ..b }),
                DropTarget::NewSplit(g, SplitDir::Down) => body(g).map(|(_, b)| Rect { y: b.y + b.height / 2, height: b.height - b.height / 2, ..b }),
                DropTarget::Group(g, None) => body(g).map(|(bar, b)| if bar.y == y { bar } else { b }),
                DropTarget::Group(_, Some(before)) => app.hits.tabs.iter().find(|t| t.0 == before).map(|t| Rect { x: t.1, y: t.3, width: t.2 - t.1, height: 1 }),
            }
        });
        if let Some(r) = r {
            f.buffer_mut().set_style(r, Style::default().bg(DROP));
        }
    }
}

/// Split screen: split branches in two and draw leaves (panes). Records divider positions (drag to change the ratio)
fn draw_panes(f: &mut Frame, app: &mut App, p: &crate::panes::Pane, area: Rect, path: &mut Vec<bool>, fg: usize) {
    use crate::panes::Pane;
    match p {
        Pane::Leaf(g) => draw_group(f, app, Some(*g), area, *g == fg),
        Pane::Split { dir, pct, a, b } => {
            let (p, q) = (Constraint::Percentage(*pct), Constraint::Percentage(100 - pct));
            let [ra, rb] = match dir {
                SplitDir::Right => Layout::horizontal([p, q]).areas(area),
                SplitDir::Down => Layout::vertical([p, q]).areas(area),
            };
            // Divider: for left/right, the two touching borders; for top/bottom, the first pane's bottom border (excluding the tab bar line)
            let divider = match dir {
                SplitDir::Right => Rect { x: ra.right().saturating_sub(1), y: ra.y + 1, width: 2, height: ra.height.saturating_sub(1) },
                SplitDir::Down => Rect { x: ra.x, y: ra.bottom().saturating_sub(1), width: ra.width, height: 1 },
            };
            // Top/bottom: the tab bar of the lower first pane (grabbing an empty spot drags it like a divider; a single border line is hard to hit)
            let grab = if *dir == SplitDir::Down { Rect { height: 1, ..rb } } else { Rect::default() };
            app.hits.splits.push((path.clone(), area, divider, grab, *dir));
            path.push(false);
            draw_panes(f, app, a, ra, path, fg);
            path.pop();
            path.push(true);
            draw_panes(f, app, b, rb, path, fg);
            path.pop();
            // Left/right: a separator in the first cell (margin) of the right pane's tab bar (so the two tab bars don't look like one line)
            if *dir == SplitDir::Right && rb.width > 0 {
                f.buffer_mut().set_string(rb.x, rb.y, "┃", Style::default().fg(DIM));
            }
        }
    }
}

/// Tab glyph (like the GUI's TabIcon) for the tab bar, Recent Tabs and Search Everywhere: a table tab shows its object's
/// tree glyph, a DDL tab that glyph + ≡ (text), other text (diagram) ≡. Uncolored in the tab bar: it takes the tab's own
/// colors, so it reads on a Data Source color and in 16 colors / NO_COLOR
pub(super) fn tab_glyph(kind: &TabKind) -> String {
    match kind {
        TabKind::Console(_) => "❯".into(),
        TabKind::Table(t) => object_glyph(&t.kind).map_or("▦", |g| g.0).into(),
        TabKind::Text(t) => object_glyph(&t.kind).map_or("≡".into(), |g| format!("{}≡", g.0)),
        TabKind::Dba(_) => "◔".into(),
        TabKind::Redis(_) => "◆".into(),
    }
}

/// One pane: tab bar + visible tab. g = None means an unsplit screen (all tabs)
fn draw_group(f: &mut Frame, app: &mut App, g: Option<usize>, area: Rect, focused: bool) {
    use unicode_width::UnicodeWidthStr;
    let [bar, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    let idx: Vec<usize> = (0..app.tabs.len()).filter(|&i| g.is_none_or(|g| app.tabs[i].group == g)).collect();
    let shown = if focused { app.active } else { g.and_then(|g| app.group_tab(g)).unwrap_or(app.active) };
    // Tab width: the Tabs widget draws " title " + "│". " " + glyph + " " + title + " *"? + " "
    let widths: Vec<u16> = idx
        .iter()
        .map(|&i| {
            let t = &app.tabs[i];
            let dirty = matches!(&t.kind, TabKind::Table(tt) if tt.grid.as_ref().map(|g| g.dirty()).unwrap_or(false));
            1 + tab_glyph(&t.kind).width() as u16 + 1 + app.tab_label(t).width() as u16 + if dirty { 2 } else { 0 } + 1
        })
        .collect();
    let sel = idx.iter().position(|&i| i == shown).unwrap_or(0);
    // If the tab bar overflows, drop leading tabs so the visible tab fits
    let mut start = 0;
    while start < sel && widths[start..=sel].iter().map(|w| w + 1).sum::<u16>() > bar.width {
        start += 1;
    }
    let mut x = bar.x;
    for (&i, &w) in idx.iter().zip(&widths).skip(start) {
        app.hits.tabs.push((app.tabs[i].id, x, (x + w).min(bar.right()), bar.y));
        x += w + 1;
    }
    app.hits.groups.push((g.unwrap_or(0), bar, body));
    // Data Source color as the tab background (like the GUI). The visible tab is bold + bright (less bright in unfocused panes)
    let titles: Vec<Line> = idx[start..]
        .iter()
        .map(|&i| {
            let t = &app.tabs[i];
            let dirty = matches!(&t.kind, TabKind::Table(tt) if tt.grid.as_ref().map(|g| g.dirty()).unwrap_or(false));
            let c = app.conn_color(&t.conn).and_then(|c| hex(&c));
            let l = Line::from(vec![Span::raw(format!("{} ", tab_glyph(&t.kind))), Span::raw(app.tab_label(t)), Span::raw(if dirty { " *" } else { "" })]);
            let mut st = Style::default();
            if let Some(c) = c {
                st = st.bg(c);
            }
            if i == shown {
                st = st.add_modifier(Modifier::BOLD);
                if app.color.reverses_selection() {
                    // 16 colors / NO_COLOR: a lighter hue of the Data Source color is often the same terminal color, so the
                    // focused pane's tab is drawn in reverse video — its own color inverted, so a [PROD] tab stays red (SEL
                    // when it has none) — and the others' underlined on their own color
                    st = match (focused, c) {
                        (true, Some(_)) => st.add_modifier(Modifier::REVERSED),
                        (true, None) => st.bg(SEL),
                        (false, _) => st.add_modifier(Modifier::UNDERLINED),
                    };
                } else {
                    let bg = if focused { c.map(lighten).unwrap_or(SEL) } else { c.unwrap_or(SEL_BLUR) };
                    st = st.fg(on(bg)).bg(bg);
                    if !focused {
                        st = st.add_modifier(Modifier::UNDERLINED);
                    }
                }
            }
            l.style(st)
        })
        .collect();
    f.render_widget(Tabs::new(titles).select(sel - start).highlight_style(Style::default()).divider("│"), bar);
    draw_pane(f, app, shown, body, focused);
}
