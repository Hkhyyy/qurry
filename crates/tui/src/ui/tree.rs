use super::*;

pub(super) fn draw_workspaces(f: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = vec![Span::styled(" ◧ ", Style::default().fg(Color::DarkGray))];
    let mut x = area.x + 3;
    for (i, w) in app.workspaces.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, w.name);
        let width = label.chars().count() as u16;
        let style = if i == app.cur_ws { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) };
        app.hits.workspaces.push((i, Rect { x, y: area.y, width, height: 1 }));
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        x += width + 1;
    }
    spans.push(Span::styled(disp(" ⌥1…9 · ⌃k Switch Workspace"), Style::default().fg(Color::DarkGray)));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Tree;
    // The mark count first: the explorer is narrow. Commands act on the marked nodes on screen; marks in collapsed or filtered-out
    // folders are counted apart
    let marked = match (app.tree.marked.len(), app.tree.targets().len()) {
        (0, _) => String::new(),
        (n, on) if on < n => format!("{on} marked ({} hidden) · ", n - on),
        (n, _) => format!("{n} marked · "),
    };
    let title = if app.tree.filter.is_empty() { format!(" {marked}Database Explorer ") } else { format!(" {marked}Database Explorer — /{} (Esc clears) ", app.tree.filter) };
    let b = panel(title, focused);
    let inner = b.inner(area);
    f.render_widget(b, area);
    app.hits.tree = inner;
    let h = inner.height as usize;
    if app.tree.sel < app.tree.offset {
        app.tree.offset = app.tree.sel;
    } else if h > 0 && app.tree.sel >= app.tree.offset + h {
        app.tree.offset = app.tree.sel + 1 - h;
    }
    let vis = app.tree.visible();
    let mut lines = vec![];
    for (i, (depth, n)) in vis.iter().enumerate().skip(app.tree.offset).take(h) {
        let open = app.tree.expanded.contains(&n.key);
        let loading = app.tree.loading.contains(&n.key);
        let conn_color = app.conn_color(&n.conn).and_then(|c| hex(&c));
        let connected = app.conn(&n.conn).map(|c| c.connected).unwrap_or(false);
        let (glyph, icolor) = match &n.kind {
            NodeKind::Conn => (if connected { "◉" } else { "○" }, conn_color.unwrap_or(C_DB)),
            NodeKind::Db => ("⛁", C_DB),
            NodeKind::Schema => ("◇", Color::Gray),
            NodeKind::Group => ("▤", ACCENT),
            NodeKind::Object(k) if k == "key" => ("●", type_color(&n.hint)),
            NodeKind::Object(k) => object_glyph(k).unwrap_or(("ƒ", C_ROUTINE)),
            NodeKind::Column { pk, .. } => (if *pk { "⚿" } else { "·" }, if *pk { Color::Yellow } else { DIM }),
            NodeKind::Info => ("", DANGER),
        };
        let mut row = Row::new(n.label.clone())
            .indent(*depth)
            .chevron(n.expandable.then_some(open))
            .hint(if loading { "loading…".to_string() } else { n.hint.clone() })
            // Like the GUI, color everything under a Data Source (the selected line wins)
            .bg(conn_color)
            // Marked rows: the selection color without focus (the cursor's row keeps the focused one)
            .selected(app.tree.marked.contains(&n.key), false)
            .selected(i == app.tree.sel, focused);
        if !glyph.is_empty() {
            row = row.lead(icon(glyph, icolor));
        }
        if matches!(n.kind, NodeKind::Conn) {
            row = row.bold();
            // Before the name: a long URL name would cut it off at the end of the row
            if let Some(c) = app.conn(&n.conn).filter(|c| c.temporary) {
                row = row.lead(temporary_badge(c.config.read_only)).lead(Span::raw(" "));
            }
        }
        // Exactly the explorer width (with horizontal scroll applied), so the background reaches the line end and a wide char cut at the end doesn't break the color
        lines.push(row.fit(app.tree.hoff, inner.width as usize));
    }
    if vis.is_empty() {
        lines.push(Line::from(muted("No data sources. Press n to add one,")));
        lines.push(Line::from(muted(format!("or {} › Import Data Sources… (DataGrip, DBeaver).", app.keys.label(crate::app::Action::Palette)))));
    }
    // Horizontal scroll (Shift+←/→, trackpad left/right) is already done by fit_spans
    f.render_widget(Paragraph::new(lines), inner);
}

/// The explorer's mark on a temporary data source (qurry-tui postgres://…)
pub fn temporary_badge(read_only: bool) -> Span<'static> {
    crate::kit::badge(if read_only { "TEMP · READ-ONLY" } else { "TEMP" }, WARN)
}

/// Explorer tree object kind → glyph and color (the tree, and the tabs that show the object)
pub(super) fn object_glyph(kind: &str) -> Option<(&'static str, Color)> {
    Some(match kind {
        "table" | "partitioned" | "foreign" => ("▦", Color::Gray),
        "view" | "matview" => ("◫", C_VIEW),
        "sequence" => ("#", C_SEQUENCE),
        "routine" | "function" | "procedure" => ("ƒ", C_ROUTINE),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_row_fills_exactly_the_width_even_when_a_wide_char_straddles_the_edge() {
        // "ab" + "가나" (two cells each) = 6 cells. Fitting to 5 cuts "나" at the end → pad with a blank (same style) to 5 cells
        let st = Style::default().bg(Color::Green);
        let out = fit_spans(vec![Span::styled("ab", st), Span::styled("가나", st)], 0, 5);
        let text: String = out.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "ab가 ");
        assert!(out.iter().all(|s| s.style.bg == Some(Color::Green)), "{out:?}");
        // Shorter content is padded with blanks
        let out = fit_spans(vec![Span::raw("ab")], 0, 4);
        assert_eq!(out.iter().map(|s| s.content.as_ref()).collect::<String>(), "ab  ");
        // Horizontal scroll (skip the first 3 cells): starts on the right half of "가" → blank
        let out = fit_spans(vec![Span::raw("ab가나다")], 3, 4);
        assert_eq!(out.iter().map(|s| s.content.as_ref()).collect::<String>(), " 나 ");
    }
}
