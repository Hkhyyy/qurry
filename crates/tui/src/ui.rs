//! Screen drawing
use crate::keys::disp;
use crate::app::{App, Drag, DropTarget, FieldKind, Focus, Popup, SplitDir, TabKind, WARNING};
use crate::tree::NodeKind;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use serde_json::Value;

pub use crate::kit::*;

/// Data Source colors, same as the GUI (src/lib/api.ts COLOR_PRESETS)
pub const COLOR_PRESETS: &[(&str, &str)] = &[
    ("red", "#6e2a2a"),
    ("orange", "#6e4a22"),
    ("yellow", "#5f5b24"),
    ("green", "#2f5634"),
    ("teal", "#22524f"),
    ("blue", "#28406e"),
    ("violet", "#4f2f68"),
    ("gray", "#45474d"),
];

/// Color name shown in the form (custom if not a preset)
pub fn color_name(c: Option<&str>) -> &'static str {
    match c {
        None => "none",
        Some(h) => COLOR_PRESETS.iter().find(|p| p.1.eq_ignore_ascii_case(h)).map(|p| p.0).unwrap_or("custom"),
    }
}

/// Bright color used for highlighting, like the active tab
pub fn type_color(t: &str) -> Color {
    match t {
        "string" => Color::Rgb(95, 158, 160),
        "hash" => Color::Rgb(176, 124, 198),
        "list" => Color::Rgb(106, 159, 216),
        "set" => Color::Rgb(208, 139, 76),
        "zset" => Color::Rgb(201, 96, 92),
        "stream" => Color::Rgb(90, 164, 105),
        _ => Color::Gray,
    }
}

/// Output in the same form as redis-cli
pub fn format_reply(v: &Value) -> String {
    fn go(v: &Value, indent: usize) -> String {
        match v {
            Value::Null => "(nil)".into(),
            Value::Number(n) => format!("(integer) {n}"),
            Value::Bool(b) => format!("({b})"),
            Value::String(s) => format!("{s:?}"),
            Value::Object(o) => o.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            Value::Array(a) if a.is_empty() => "(empty array)".into(),
            Value::Array(a) => {
                let w = a.len().to_string().len();
                a.iter()
                    .enumerate()
                    .map(|(i, x)| {
                        let head = format!("{:>w$}) ", i + 1);
                        let body = go(x, indent + head.len());
                        format!("{}{head}{body}", if i == 0 { String::new() } else { " ".repeat(indent) })
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }
    go(v, 0)
}

/// Smallest screen the layout is drawn for (explorer · tabs · status line still readable); below it, a message instead
pub const MIN_SIZE: (u16, u16) = (40, 10);

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.too_small = area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1;
    if app.too_small {
        draw_too_small(f, app);
    } else {
        draw_screen(f, app);
    }
    sanitize(f.buffer_mut());
    app.color.apply(f.buffer_mut());
}

/// Says the size and the size needed, centered and wrapped so it fits even a few cells
fn draw_too_small(f: &mut Frame, app: &App) {
    let a = f.area();
    if a.is_empty() {
        return;
    }
    let last = match app.quit_waits_for() {
        Some(why) => Line::styled(why, Style::default().fg(WARN)),
        None => Line::from(muted(format!("Make the window larger · {} quits", app.keys.label(crate::app::Action::Quit)))),
    };
    let lines = vec![
        Line::styled("Terminal too small", Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
        Line::raw(format!("{}×{} — needs {}×{}", a.width, a.height, MIN_SIZE.0, MIN_SIZE.1)),
        last,
    ];
    let p = Paragraph::new(lines).alignment(Alignment::Center).wrap(Wrap { trim: true });
    let h = (p.line_count(a.width) as u16).min(a.height);
    f.render_widget(p, Rect { y: a.y + (a.height - h) / 2, height: h, ..a });
}

fn draw_screen(f: &mut Frame, app: &mut App) {
    let [main, status] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(f.area());
    // With two or more workspaces, one line at the top: 1 Default │ 2 Reports (Alt+n · click)
    app.hits.workspaces.clear();
    app.hits.text = Rect::default();
    app.hits.cancel = Rect::default();
    let main = if app.workspaces.len() > 1 {
        let [bar, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).areas(main);
        draw_workspaces(f, app, bar);
        rest
    } else {
        main
    };
    app.hits.main = main;
    app.hits.explorer_divider = Rect::default();
    let right = if app.explorer {
        let w = app.explorer_width(main.width);
        let [l, r] = Layout::horizontal([Constraint::Length(w), Constraint::Min(10)]).areas(main);
        draw_tree(f, app, l);
        // Two cells up to the one touching the tree's right border (drag to resize; one cell is hard to hit)
        app.hits.explorer_divider = Rect { x: l.right().saturating_sub(1), y: l.y, width: 2, height: l.height };
        r
    } else {
        main
    };
    draw_tabs(f, app, right);
    draw_status(f, app, status);
    if app.designer.is_some() {
        crate::designer::draw(f, app, f.area());
    }
    if app.popup.is_some() {
        draw_popup(f, app);
    }
}

/// Whether a char can control the terminal: C0 · DEL · C1 (ESC sequences, OSC 52 clipboard, screen clear …) and text-direction overrides
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Strip such chars from the drawn screen. Done once at the end, whether they came from DB values · names · errors · the status line (ratatui's Span doesn't filter them)
fn sanitize(buf: &mut ratatui::buffer::Buffer) {
    for cell in buf.content.iter_mut() {
        if cell.symbol().chars().any(is_unsafe_char) {
            let clean: String = cell.symbol().chars().filter(|c| !is_unsafe_char(*c)).collect();
            cell.set_symbol(if clean.is_empty() { " " } else { &clean });
        }
    }
}

fn draw_workspaces(f: &mut Frame, app: &mut App, area: Rect) {
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

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Tree;
    let title = if app.tree.filter.is_empty() { " Database Explorer ".to_string() } else { format!(" Database Explorer — /{} (Esc clears) ", app.tree.filter) };
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
            NodeKind::Object(k) => match k.as_str() {
                "table" | "partitioned" | "foreign" => ("▦", Color::Gray),
                "view" | "matview" => ("◫", C_VIEW),
                "sequence" => ("#", C_SEQUENCE),
                "key" => ("●", type_color(&n.hint)),
                _ => ("ƒ", C_ROUTINE),
            },
            NodeKind::Column { pk, .. } => (if *pk { "⚿" } else { "·" }, if *pk { Color::Yellow } else { DIM }),
            NodeKind::Info => ("", DANGER),
        };
        let mut row = Row::new(n.label.clone())
            .indent(*depth)
            .chevron(n.expandable.then_some(open))
            .hint(if loading { "loading…".to_string() } else { n.hint.clone() })
            // Like the GUI, color everything under a Data Source (the selected line wins)
            .bg(conn_color)
            .selected(i == app.tree.sel, focused);
        if !glyph.is_empty() {
            row = row.lead(icon(glyph, icolor));
        }
        if matches!(n.kind, NodeKind::Conn) {
            row = row.bold();
        }
        // Exactly the explorer width (with horizontal scroll applied), so the background reaches the line end and a wide char cut at the end doesn't break the color
        lines.push(row.fit(app.tree.hoff, inner.width as usize));
    }
    if vis.is_empty() {
        lines.push(Line::from(muted("No data sources. Press n to add one.")));
    }
    // Horizontal scroll (Shift+←/→, trackpad left/right) is already done by fit_spans
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) {
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

/// One pane: tab bar + visible tab. g = None means an unsplit screen (all tabs)
fn draw_group(f: &mut Frame, app: &mut App, g: Option<usize>, area: Rect, focused: bool) {
    use unicode_width::UnicodeWidthStr;
    let [bar, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    let idx: Vec<usize> = (0..app.tabs.len()).filter(|&i| g.is_none_or(|g| app.tabs[i].group == g)).collect();
    let shown = if focused { app.active } else { g.and_then(|g| app.group_tab(g)).unwrap_or(app.active) };
    // Tab width: the Tabs widget draws " title " + "│". " " + icon(2) + title + " *"? + " "
    let widths: Vec<u16> = idx
        .iter()
        .map(|&i| {
            let t = &app.tabs[i];
            let dirty = matches!(&t.kind, TabKind::Table(tt) if tt.grid.as_ref().map(|g| g.dirty()).unwrap_or(false));
            1 + 2 + app.tab_label(t).width() as u16 + if dirty { 2 } else { 0 } + 1
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
            let icon = match t.kind {
                TabKind::Console(_) => "⌘ ",
                TabKind::Table(_) => "▦ ",
                TabKind::Text(_) => "ƒ ",
                TabKind::Dba(_) => "◉ ",
                TabKind::Redis(_) => "◆ ",
            };
            let dirty = matches!(&t.kind, TabKind::Table(tt) if tt.grid.as_ref().map(|g| g.dirty()).unwrap_or(false));
            let c = app.conn_color(&t.conn).and_then(|c| hex(&c));
            let l = Line::from(vec![Span::raw(icon), Span::raw(app.tab_label(t)), Span::raw(if dirty { " *" } else { "" })]);
            let mut st = Style::default();
            if let Some(c) = c {
                st = st.bg(c);
            }
            if i == shown {
                st = st.add_modifier(Modifier::BOLD);
                let bg = if focused { c.map(lighten).unwrap_or(SEL) } else { c.unwrap_or(SEL_BLUR) };
                st = st.fg(on(bg)).bg(bg);
                if !focused {
                    st = st.add_modifier(Modifier::UNDERLINED);
                }
            }
            l.style(st)
        })
        .collect();
    f.render_widget(Tabs::new(titles).select(sel - start).highlight_style(Style::default()).divider("│"), bar);
    draw_pane(f, app, shown, body, focused);
}

/// Body of one tab. focused = the focused pane (mouse hit areas are recorded only for this pane)
fn draw_pane(f: &mut Frame, app: &mut App, idx: usize, body: Rect, focused: bool) {
    // Unfocused panes are drawn like Tree focus (dimmed border·cursor)
    let focus = if focused { app.focus } else { Focus::Tree };
    let color = app.color;
    let ro = app.tabs.get(idx).and_then(|t| app.conn(&t.conn)).map(|c| c.config.read_only).unwrap_or(false);
    let pct = app.console_pct;
    // Rebindable commands are shown with their first key in the current keymap
    let k = |a: crate::app::Action| app.keys.keys(a).first().map(|s| disp(s)).unwrap_or_else(|| "—".into());
    use crate::app::Action as A;
    let (k_out, k_run, k_commit, k_rollback, k_cancel, k_submit) = (k(A::ToggleOutput), k(A::Execute), k(A::Commit), k(A::Rollback), k(A::Cancel), k(A::Submit));
    let finding = focused && matches!(app.popup, Some(Popup::Find(_)));
    // How long run · load has been going (redrawn every 0.5s)
    let took = app.run_elapsed(idx).map(|d| crate::app::elapsed_text(d.as_millis() as u64));
    let tab = &mut app.tabs[idx];
    let db = tab.db.clone();
    let title = tab.title.clone();
    match &mut tab.kind {
        TabKind::Console(c) => {
            let [ed, res] = Layout::vertical([Constraint::Percentage(pct), Constraint::Percentage(100 - pct)]).areas(body);
            // The find bar takes a spot above the editor (so it doesn't cover text)
            let (find_bar, ed) = if finding {
                let [b, e] = Layout::vertical([Constraint::Length(4), Constraint::Min(3)]).areas(ed);
                (b, e)
            } else {
                (Rect::default(), ed)
            };
            let title = format!(
                " {db} · Tx: {}{}{}{} ",
                if c.tx_manual { "Manual" } else { "Auto" },
                if c.in_tx { format!(" (pending — {k_commit} commit / {k_rollback} rollback)") } else { String::new() },
                if c.running { format!(" · running {} · [■ Cancel {k_cancel}]", took.as_deref().unwrap_or("")) } else { String::new() },
                if ro { " · read-only" } else { "" }
            );
            let editing = focus == Focus::Editor;
            if !editing {
                c.complete = None;
            }
            // [■ Cancel] spot (the title starts right after the top-left corner)
            app.hits.cancel = match (c.running, title.find("[■ Cancel")) {
                (true, Some(at)) => {
                    use unicode_width::UnicodeWidthStr;
                    let x = ed.x + 1 + title[..at].width() as u16;
                    let w = title[at..].find(']').map(|e| title[at..at + e + 1].width() as u16).unwrap_or(0);
                    Rect { x, y: ed.y, width: w.min(ed.right().saturating_sub(x)), height: 1 }
                }
                _ => Rect::default(),
            };
            c.editor.set_block(panel(title, editing));
            c.editor.set_line_number_style(Style::default().fg(DIM));
            // Put an invisible mark (underline color) on the cursor cell: found after drawing and used to place autocomplete·compute mouse positions
            let mark = Style::default().underline_color(CARET_MARK);
            c.editor.set_cursor_style(if editing { mark.add_modifier(Modifier::REVERSED) } else { mark });
            f.render_widget(&c.editor, ed);
            // tui-textarea doesn't expose its scroll position → use the cursor cell just drawn
            let drawn = find_caret(f, ed);
            let caret = if editing && c.complete.is_some() { drawn } else { None };
            if focused {
                app.hits.caret = drawn.map(|(x, y)| (x, y, c.editor.cursor().0, c.editor.cursor().1));
                app.hits.editor = ed;
                app.hits.find = find_bar;
                app.hits.results = res;
                app.hits.console = body;
                app.hits.divider = Rect { x: res.x, y: res.y, width: res.width, height: 1 };
            }
            if c.show_output || c.grid.is_none() {
                let lines: Vec<Line> = c
                    .output
                    .iter()
                    .rev()
                    .take(res.height as usize)
                    .rev()
                    .map(|(e, s)| Line::styled(s.clone(), Style::default().fg(if *e { DANGER } else if s.starts_with(WARNING) { WARN } else if s.contains("> ") { ACCENT } else { Color::Gray })))
                    .collect();
                let p = Paragraph::new(if lines.is_empty() { vec![Line::styled(format!("Execute a statement ({k_run}) to see the output."), Style::default().fg(DIM))] } else { lines })
                    .wrap(Wrap { trim: false })
                    .block(panel(format!(" Output ({k_out}: result) "), focus == Focus::Grid));
                f.render_widget(p, res);
            } else if let Some(g) = &mut c.grid {
                let t = format!(" {} — {}{} rows ({k_out}: output) ", c.result_title, g.rows(), if g.result.truncated { "+" } else { "" });
                g.render(f, res, t, focus == Focus::Grid, color);
            }
            if let (Some((x, y)), Some(cmp)) = (caret, &c.complete) {
                draw_completion(f, cmp, x, y);
            }
        }
        TabKind::Table(tt) => {
            let [filt, grid] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(body);
            if focused {
                app.hits.results = grid;
            }
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" WHERE ", Style::default().fg(DIM)),
                    Span::raw(if tt.where_.is_empty() { "—".to_string() } else { tt.where_.clone() }),
                    Span::styled("  ORDER BY ", Style::default().fg(DIM)),
                    Span::raw(if tt.order.is_empty() { "—".to_string() } else { tt.order.clone() }),
                    Span::styled("   (w / o to edit)", Style::default().fg(DIM)),
                ])),
                filt,
            );
            // If the query failed: one error line and how to fix it above the grid, grid left empty (so the way out is visible)
            let grid = if let Some(e) = &tt.error {
                let [msg, rest] = Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(grid);
                let mut hint = vec![Span::styled(" w ", Style::default().fg(ACCENT)), Span::raw("fix WHERE · "), Span::styled("o ", Style::default().fg(ACCENT)), Span::raw("fix ORDER BY · "), Span::styled("r ", Style::default().fg(ACCENT)), Span::raw("run again")];
                if let Some(n) = tt.grid.as_ref().filter(|g| g.dirty()).map(|g| g.edits.len() + g.deleted.len() + g.added) {
                    hint.push(Span::styled(format!("   · {n} pending change(s) not saved yet ({k_submit} submit · {} revert)", disp("⌃z")), Style::default().fg(Color::Yellow)));
                }
                let first = e.lines().next().unwrap_or_default().to_string();
                f.render_widget(Paragraph::new(vec![Line::styled(format!(" ✗ {first}"), Style::default().fg(DANGER)), Line::from(hint)]), msg);
                rest
            } else {
                grid
            };
            if let Some(g) = &mut tt.grid {
                let pend = if g.dirty() { format!(" · {} pending ({k_submit} submit, {} revert)", g.edits.len() + g.deleted.len() + g.added, disp("⌃z")) } else { String::new() };
                let rows = if tt.error.is_some() && !g.dirty() { "query failed — no rows".to_string() } else { format!("{}{} rows", g.rows(), if g.result.truncated { "+" } else { "" }) };
                let t = format!(" {}.{} — {rows}{}{}{} ", tt.schema, tt.table, if tt.pk.is_empty() { " · no PK (read-only)" } else { "" }, if ro { " · read-only" } else { "" }, pend);
                g.render(f, grid, t, focus == Focus::Grid, color);
            } else {
                let msg = if tt.error.is_some() { String::new() } else { format!("Loading… {}", took.as_deref().unwrap_or("")) };
                f.render_widget(Paragraph::new(msg).block(panel(format!(" {}.{} ", tt.schema, tt.table), focused)), grid);
            }
        }
        TabKind::Text(t) => {
            // Don't scroll past the end (so the last line sits at the bottom)
            let lines = t.text.lines().count() as u16;
            t.scroll = t.scroll.min(lines.saturating_sub(body.height.saturating_sub(2)));
            let p = Paragraph::new(t.text.as_str()).scroll((t.scroll, 0)).block(panel(format!(" {title} — {lines} lines · ↑↓ PgUp/PgDn wheel scroll · y copies all "), focus != Focus::Tree));
            f.render_widget(p, body);
            if focused {
                app.hits.text = body;
            }
        }
        TabKind::Dba(d) => {
            // Top line: what is shown · below: the grid (or what to turn on)
            let [help, grid] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(body);
            let live = ["sessions", "locks"].contains(&d.report.as_str());
            let state = if d.running { " · loading…" } else if live && d.auto { " · auto 3s" } else { "" };
            let text = d.info.as_ref().map(|i| i.help).unwrap_or("");
            f.render_widget(Paragraph::new(Line::from(vec![Span::styled(format!(" {}{state} ", d.info.as_ref().map(|i| i.title).unwrap_or("")), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::styled(text, Style::default().fg(DIM))])), help);
            let title = format!(" {db} — {} rows ", d.grid.as_ref().map(|g| g.rows()).unwrap_or(0));
            // Where the grid is drawn (mouse hits): below the message when a refresh failed
            let mut area = grid;
            match (&mut d.grid, &d.error) {
                (None, Some(e)) => f.render_widget(Paragraph::new(e.as_str()).style(Style::default().fg(DANGER)).wrap(Wrap { trim: false }).block(panel(" Error ", focus == Focus::Grid)), grid),
                (Some(g), Some(e)) => {
                    let [msg, rest] = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(grid);
                    let age = d.loaded.map(|t| format!(" from {} s ago", t.elapsed().as_secs())).unwrap_or_default();
                    let text = format!(" Refresh failed, showing data{age}: {}", e.lines().next().unwrap_or(""));
                    f.render_widget(Paragraph::new(text).style(Style::default().fg(WARN)).wrap(Wrap { trim: false }), msg);
                    g.render(f, rest, title, focus == Focus::Grid, color);
                    area = rest;
                }
                (Some(g), None) => g.render(f, grid, title, focus == Focus::Grid, color),
                (None, None) => f.render_widget(Paragraph::new("Loading…").block(panel(title, focus == Focus::Grid)), grid),
            }
            if focused {
                app.hits.results = area;
            }
        }
        TabKind::Redis(r) => {
            let [left, right] = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(body);
            let kf = focus == Focus::Grid && !r.focus_value;
            let title = format!(" Keys {} — {}{} {} ", r.pattern, r.keys.len(), if r.cursor != "0" { "+" } else { "" }, if r.scanning { "scanning…" } else { "" });
            let b = panel(title, kf);
            let inner = b.inner(left);
            f.render_widget(b, left);
            let h = inner.height as usize;
            if r.sel < r.off {
                r.off = r.sel;
            } else if h > 0 && r.sel >= r.off + h {
                r.off = r.sel + 1 - h;
            }
            let rows = r.rows();
            let items: Vec<ListItem> = rows
                .iter()
                .enumerate()
                .skip(r.off)
                .take(h)
                .map(|(i, row)| {
                    let w = inner.width as usize;
                    let line = match row {
                        crate::app::KRow::Folder { path, name, depth, count } => Row::new(name.clone()).indent(*depth).chevron(Some(r.open.contains(path))).lead(icon("▤", ACCENT)).trailing(count.to_string()),
                        crate::app::KRow::Key { idx, depth } => {
                            let k = &r.keys[*idx];
                            let ttl = if k.ttl >= 0 { format!("{}s", k.ttl / 1000) } else { String::new() };
                            Row::new(k.key.clone()).indent(*depth).chevron(None).lead(badge(&format!("{:<6}", k.ty.to_uppercase()), type_color(&k.ty))).lead(Span::raw(" ")).trailing(ttl)
                        }
                    };
                    ListItem::new(line.selected(i == r.sel, kf).fit(0, w))
                })
                .collect();
            f.render_widget(List::new(items), inner);
            if focused {
                app.hits.redis_keys = inner;
                app.hits.redis_value = right;
            }
            let vf = focus == Focus::Grid && r.focus_value;
            match &mut r.detail {
                None => f.render_widget(Paragraph::new("Select a key").block(panel(" Value ", vf)), right),
                Some((key, d)) => {
                    let ttl = if d.ttl >= 0 { format!("TTL {}s", d.ttl / 1000) } else { "no TTL".into() };
                    let title = format!(" {} {key} · {} · {} {} ", d.ty.to_uppercase(), ttl, d.length, if d.ty == "string" { "bytes" } else { "items" });
                    match &mut r.value {
                        Some(g) => g.render(f, right, title, vf, color),
                        None => {
                            f.render_widget(Paragraph::new(r.text.as_deref().unwrap_or_default()).wrap(Wrap { trim: false }).block(panel(title, vf)), right);
                        }
                    }
                }
            }
        }
    }
}

/// Cursor (inverted cell) position in the editor
/// Console cursor cell mark (invisible since underline isn't turned on)
const CARET_MARK: Color = Color::Rgb(1, 2, 3);

fn find_caret(f: &mut Frame, area: Rect) -> Option<(u16, u16)> {
    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if buf[(x, y)].underline_color == CARET_MARK {
                return Some((x, y));
            }
        }
    }
    None
}

/// Autocomplete list: below the cursor (above if there is no room)
fn draw_completion(f: &mut Frame, cmp: &crate::app::Completion, x: u16, y: u16) {
    use qurry_core::complete::ItemKind;
    use unicode_width::UnicodeWidthStr;
    let screen = f.area();
    let shown = cmp.items.len().min(10);
    let lw = cmp.items.iter().map(|i| i.label.width()).max().unwrap_or(0);
    let dw = cmp.items.iter().map(|i| i.detail.width()).max().unwrap_or(0);
    let w = ((2 + lw + 2 + dw + 2) as u16).clamp(20, 70).min(screen.width);
    let h = shown as u16 + 2;
    let x = x.saturating_sub(cmp.prefix as u16 + 3).min(screen.right().saturating_sub(w));
    let y = if y + 1 + h <= screen.bottom() { y + 1 } else { y.saturating_sub(h) };
    let r = Rect { x, y, width: w, height: h.min(screen.height) };
    let off = cmp.sel.saturating_sub(shown.saturating_sub(1));
    let items: Vec<ListItem> = cmp
        .items
        .iter()
        .enumerate()
        .skip(off)
        .take(shown)
        .map(|(i, it)| {
            let (glyph, color) = match it.kind {
                ItemKind::Column => ("·", C_SEQUENCE),
                ItemKind::Table => ("▦", C_DB),
                ItemKind::Schema => ("◇", Color::Gray),
                ItemKind::Keyword => ("k", C_KEYWORD),
                ItemKind::Value => ("=", crate::kit::NUMBER),
            };
            ListItem::new(Row::new(format!("{:<lw$} ", it.label)).lead(icon(glyph, color)).hint(it.detail.clone()).selected(i == cmp.sel, true).line())
        })
        .collect();
    clear(f, r);
    let title = format!(" {}/{} ", cmp.sel + 1, cmp.items.len());
    f.render_widget(List::new(items).block(panel(title, true)).style(Style::default().bg(POPUP_BG)), r);
}

/// Bottom hints: (keys used on this screen, common global keys). Rebindable commands use their first key in the current keymap
fn status_hints(app: &App) -> (Vec<(String, &'static str)>, Vec<(String, &'static str)>) {
    use crate::app::Action as A;
    let key = |a: A| app.keys.keys(a).first().map(|k| disp(k));
    let fixed = |k: &str, l: &'static str| Some((disp(k), l));
    let cmd = |a: A, l: &'static str| key(a).map(|k| (k, l));
    let (ctx, glob) = match app.focus {
        Focus::Tree => (
            vec![fixed("Enter", "open"), fixed("F4", "console"), fixed("s", "schemas"), fixed("/", "filter"), fixed("n", "new"), fixed("e", "edit"), fixed("w", "workspace"), fixed("r", "refresh"), cmd(A::Ddl, "DDL")],
            vec![cmd(A::SearchEverywhere, "search"), cmd(A::Palette, "actions"), cmd(A::Help, "keys")],
        ),
        Focus::Editor => (
            vec![cmd(A::Execute, "run"), cmd(A::Complete, "complete"), cmd(A::Explain, "explain"), cmd(A::TxToggle, "tx"), fixed("⇧Tab", "result"), fixed("⌘A ⌫", "clear"), fixed("Esc", "explorer")],
            vec![cmd(A::SearchEverywhere, "search"), cmd(A::RecentTabs, "tabs"), cmd(A::Palette, "actions")],
        ),
        Focus::Grid => (
            match app.tabs.get(app.active).map(|t| &t.kind) {
                Some(TabKind::Table(_)) => vec![
                    fixed("Enter", "edit"), fixed("a", "add"), fixed("d", "delete"), fixed("n", "NULL"), cmd(A::Submit, "submit"), fixed("⌃z", "revert"), fixed("w", "where"), fixed("o", "order"), fixed("s", "sort"), fixed("x", "export"),
                ],
                Some(TabKind::Redis(r)) if r.focus_value => vec![fixed("e", "edit"), fixed("a", "add"), fixed("x", "delete"), fixed("Esc", "keys")],
                Some(TabKind::Redis(_)) => vec![fixed("Enter", "value"), fixed("/", "pattern"), fixed("a", "add"), fixed("D", "del key"), fixed("t", "TTL"), fixed("R", "rename"), fixed(":", "command")],
                Some(TabKind::Text(_)) => vec![fixed("y", "copy"), fixed("Esc", "explorer")],
                Some(TabKind::Dba(d)) => {
                    let has = |t: Option<bool>| d.info.as_ref().is_some_and(|i| i.actions.iter().any(|a| matches!(a, qurry_core::dba::Action::Kill { terminate, .. } if t.is_none_or(|t| *terminate == t))));
                    let mut v = vec![fixed("r", "refresh"), fixed("p", "reports")];
                    if has(None) {
                        v.push(fixed("c", "cancel query"));
                        if has(Some(true)) {
                            v.push(fixed("K", "terminate"));
                        }
                        v.push(fixed("a", "auto"));
                    }
                    v.extend([fixed("Enter", "query→console"), fixed("g", "SQL"), fixed("y", "copy")]);
                    v
                }
                _ => vec![fixed("s", "sort"), fixed("y", "copy"), fixed("x", "export"), fixed("Esc", "editor")],
            },
            vec![cmd(A::NewConsole, "console"), cmd(A::RecentTabs, "tabs"), cmd(A::SearchEverywhere, "search"), cmd(A::Palette, "actions")],
        ),
    };
    (ctx.into_iter().flatten().collect(), glob.into_iter().flatten().collect())
}

/// One hint line: screen keys · … │ global keys · … (if wider than w, drop screen keys from the end)
fn hint_line(mut ctx: Vec<(String, &'static str)>, glob: Vec<(String, &'static str)>, w: usize) -> Line<'static> {
    use unicode_width::UnicodeWidthStr;
    let width = |v: &[(String, &str)]| v.iter().map(|(k, l)| k.width() + 1 + l.width() + 3).sum::<usize>();
    while !ctx.is_empty() && width(&ctx) + width(&glob) + 1 > w {
        ctx.pop();
    }
    let mut spans = vec![];
    let push = |v: Vec<(String, &'static str)>, spans: &mut Vec<Span<'static>>| {
        for (i, (k, l)) in v.into_iter().enumerate() {
            if i > 0 {
                spans.push(muted(" · "));
            }
            spans.push(key(&k));
            spans.push(muted(format!(" {l}")));
        }
    };
    let has_ctx = !ctx.is_empty();
    push(ctx, &mut spans);
    if has_ctx {
        spans.push(Span::styled(" │ ", Style::default().fg(DIM)));
    }
    push(glob, &mut spans);
    spans.push(Span::raw(" "));
    Line::from(spans)
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    // The new-version notice stays visible in front of the status text
    let badge = app.update.as_ref().map(|v| format!(" ⬆ v{v} available — {} › Update Qurry & Restart ", app.keys.label(crate::app::Action::Palette)));
    let bw = badge.as_ref().map(|b| b.chars().count() as u16).unwrap_or(0);
    // If width is short, shrink the right-side hints (so the update notice·status text isn't cut)
    let (ctx, glob) = status_hints(app);
    // While export·import runs, a progress bar instead of status text (stop with ⌃K › Cancel Export/Import)
    let status = if app.jobs.is_empty() { disp(&app.status) } else { app.jobs.iter().map(|(_, label, j)| job_line(label, j.progress())).collect::<Vec<_>>().join("  ·  ") };
    // Status text claims up to half the screen width first (so error·result messages aren't cut)
    let sw = (status.chars().count() as u16 + 2).clamp(12, area.width / 2);
    let hints = hint_line(ctx, glob, area.width.saturating_sub(bw + sw) as usize);
    let hw = (hints.width() as u16).min(area.width.saturating_sub(bw + 12));
    let [u, l, r] = Layout::horizontal([Constraint::Length(bw), Constraint::Min(10), Constraint::Length(hw)]).areas(area);
    if let Some(b) = badge {
        f.render_widget(Paragraph::new(b).style(Style::default().fg(Color::Black).bg(WARN)), u);
    }
    // Badge before the status text if the current tab's Data Source is production · read-only
    let conn = app.tabs.get(app.active).and_then(|t| app.conn(&t.conn));
    let mut line = vec![Span::raw(" ")];
    if let Some(c) = conn {
        if c.config.options.confirm_changes {
            line.push(crate::kit::badge("PROD", DANGER));
            line.push(Span::raw(" "));
        }
        if c.config.read_only {
            line.push(crate::kit::badge("READ-ONLY", SEL));
            line.push(Span::raw(" "));
        }
    }
    line.push(Span::raw(status));
    f.render_widget(Paragraph::new(Line::from(line)).style(Style::default().fg(Color::Gray).bg(SURFACE)), l);
    f.render_widget(Paragraph::new(hints).style(Style::default().bg(SURFACE)), r);
}

/// "Export users ▕████░░░░░░▏ 1,234 rows · 42%". If the total is unknown the bar bounces back and forth
fn job_line(label: &str, (rows, done, total): (u64, u64, u64)) -> String {
    const W: usize = 10;
    // Backup·restore progress in bytes instead of rows
    let n = if rows == 0 && done > 0 { format!("{:.1} MB", done as f64 / 1048576.0) } else { format!("{} rows", crate::transfer::thousands(rows)) };
    let (bar, pct) = if total > 0 {
        let pct = (done * 100 / total).min(99) as usize;
        ("█".repeat(pct * W / 100) + &"░".repeat(W - pct * W / 100), format!(" · {pct}%"))
    } else {
        let t = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) / 250) as usize % (W - 2);
        ("░".repeat(t) + "███" + &"░".repeat(W - 3 - t), String::new())
    };
    format!("{label} ▕{bar}▏ {n}{pct}")
}

/// Shown next to the data source form's SSL mode: Require encrypts but doesn't stop someone in the middle
fn ssl_mode_note(mode: &str) -> &'static str {
    match mode {
        "verify-full" => "checks the CA and host name",
        "verify-ca" => "checks the certificate's CA",
        _ => "encrypt, no certificate check",
    }
}

/// What an empty form field means (the GUI shows the same as a placeholder)
fn empty_field_note(label: &str) -> Option<&'static str> {
    match label {
        "AWS profile" => Some("(default profile)"),
        "AWS region" => Some("(the profile's region)"),
        _ => None,
    }
}

/// Rows the data source form keeps for its message, so a test result appearing doesn't resize it
const FORM_MESSAGE_ROWS: u16 = 3;

/// The data source form's message: the running test with its elapsed time and how to cancel it, or the last result
fn conn_form_message(form: &crate::app::ConnForm) -> Vec<Line<'static>> {
    let (ok, m) = match (&form.testing, &form.message) {
        (Some((started, _)), _) => (true, format!("Testing… {}s · ⌃t / Esc cancel", started.elapsed().as_secs())),
        (None, Some((ok, m))) => (*ok, disp(m)),
        (None, None) => return vec![],
    };
    // One row per line of it (a TLS error is followed by its hint)
    m.lines().map(|l| Line::styled(l.to_string(), Style::default().fg(if ok { OK } else { DANGER }))).collect()
}

fn draw_popup(f: &mut Frame, app: &mut App) {
    if matches!(app.popup, Some(Popup::Find(_))) {
        return crate::find::draw(f, app);
    }
    // A note stays over the popup it was opened from: draw that first
    if let Some(Popup::Note { title, text, back }) = app.popup.take_if(|p| matches!(p, Popup::Note { .. })) {
        app.popup = Some(*back);
        draw_popup(f, app);
        let back = Box::new(app.popup.take().expect("drawn"));
        app.popup = Some(Popup::Note { title, text, back });
    }
    let area = f.area();
    match app.popup.as_mut().unwrap() {
        Popup::Help { sel, scroll, record, message } => {
            // Start here (non-selectable line) → Commands → fixed keys
            let mut lines = vec![section("Start here")];
            for (k, d) in crate::help::start_here(&app.keys) {
                lines.push(key_row(&k, d, 30, false, false));
            }
            lines.push(Line::raw(""));
            lines.push(section("Commands — ↑↓ select · Enter change · a add key · Del reset to default"));
            let top = lines.len();
            for (i, (a, _)) in crate::keys::DEFAULT_KEYS.iter().enumerate() {
                let custom = app.keys.is_custom(*a);
                let keys = if i == *sel && record.is_some() { "press a key… (Esc cancels)".to_string() } else { app.keys.label(*a) };
                lines.push(key_row(&keys, crate::input::name_of(*a), 30, custom, i == *sel));
            }
            lines.push(Line::raw(""));
            // Fixed key list: selectable continuing from the number after Commands (not rebindable)
            let mut i = crate::keys::DEFAULT_KEYS.len();
            let mut sel_line = top + *sel;
            for (title, rows) in crate::help::HELP {
                lines.push(section(disp(title)));
                for (k, d) in *rows {
                    if i == *sel {
                        sel_line = lines.len();
                    }
                    lines.push(key_row(&disp(k), disp(d), 30, false, i == *sel));
                    i += 1;
                }
                lines.push(Line::raw(""));
            }
            let r = centered(area, 104, area.height.saturating_sub(2));
            let h = r.height.saturating_sub(2);
            // Keep the selected line visible
            let row = sel_line as u16;
            if row < *scroll {
                *scroll = row.saturating_sub(1);
            } else if row >= *scroll + h {
                *scroll = row + 1 - h;
            }
            let max = (lines.len() as u16).saturating_sub(h);
            if *scroll > max {
                *scroll = max;
            }
            let title = match message {
                Some(m) => format!(" {m} "),
                None => " Keymap — saved to keymap.json · PgUp PgDn · Esc close ".into(),
            };
            let inner = popup(f, r, title);
            f.render_widget(Paragraph::new(lines).scroll((*scroll, 0)), inner);
        }
        // Drawn separately above
        Popup::Find(_) => {}
        Popup::Message { title, text } | Popup::Note { title, text, .. } => {
            // Sized by word wrap, so a long line isn't cut off in a narrow terminal
            let lines: Vec<Line> = text.lines().map(|l| Line::raw(l.to_string())).collect();
            let r = centered(area, 90, (wrapped_height(&lines, 90u16.min(area.width.saturating_sub(4)).saturating_sub(2)) + 4).min(30));
            let inner = popup(f, r, format!(" {} (Esc) ", disp(title)));
            f.render_widget(Paragraph::new(text.clone()).wrap(Wrap { trim: false }), inner);
        }
        Popup::Scope(e) => {
            use crate::kit::Row as KRow;
            use crate::scope::{Check, Row};
            let name = app.conns.iter().find(|c| c.config.id == e.conn).map(|c| c.config.name.clone()).unwrap_or_default();
            let rows = e.rows();
            let r = centered(area, 70, (rows.len() as u16 + 4).min(area.height.saturating_sub(4)).max(8));
            let inner_h = r.height.saturating_sub(3) as usize;
            let top = e.sel.saturating_sub(inner_h.saturating_sub(1));
            let mut lines: Vec<Line> = rows
                .iter()
                .enumerate()
                .skip(top)
                .take(inner_h)
                .map(|(i, row)| {
                    let box_ = muted(match e.check(row) {
                        Check::On => "[x] ",
                        Check::Implicit => "[~] ",
                        Check::Off => "[ ] ",
                    });
                    let r = match row {
                        Row::AllDbs => KRow::new("All databases").chevron(None).lead(box_),
                        Row::DefaultDb => KRow::new("Default database").chevron(None).lead(box_).hint(format!("({})", e.default_db)),
                        Row::Db(db) => {
                            let open = rows.get(i + 1).is_some_and(|n| !matches!(n, Row::Db(_)));
                            let on = e.check(row) != Check::Off;
                            let r = KRow::new(db.clone()).chevron(Some(open)).lead(box_).hint(if *db == e.default_db { "(Default database)".to_string() } else { String::new() });
                            if on {
                                r.bold()
                            } else {
                                r
                            }
                        }
                        Row::AllSchemas(_) => KRow::new("All schemas").indent(1).chevron(None).lead(box_),
                        Row::DefaultSchema(_) => KRow::new("Default schema").indent(1).chevron(None).lead(box_).hint("(public)"),
                        Row::Schema { name, system, .. } => {
                            let r = KRow::new(name.clone()).indent(1).chevron(None).lead(box_);
                            let r = if *system { r.label_style(Style::default().fg(Color::Yellow)).trailing("⚡ system") } else { r };
                            r.hint(if name == "public" { "(Default schema)".to_string() } else { String::new() })
                        }
                        Row::Note(_, t) => KRow::new(String::new()).indent(2).hint(t.clone()),
                    };
                    r.selected(i == e.sel, true).line()
                })
                .collect();
            lines.push(key_hints(&[("Space", "check"), ("→", "expand"), ("←", "collapse"), ("Enter", "apply"), ("Esc", "cancel")]));
            let inner = popup(f, r, format!(" Manage Shown Schemas — {name} "));
            f.render_widget(Paragraph::new(lines), inner);
        }
        Popup::Confirm { text, .. } => app.hits.confirm_unseen = !draw_confirm(f, area, " Confirm ", text, key_hints(&[("y / Enter", "yes"), ("n / Esc", "no")])),
        // Like Confirm, but only y allows (no Enter): the popup opened on its own
        Popup::AllowPlaintext { text, .. } => app.hits.confirm_unseen = !draw_confirm(f, area, " Allow Unencrypted ", text, key_hints(&[("y", "allow"), ("n / Esc", "cancel")])),
        Popup::Input { title, body, input, complete, words, .. } => {
            // The title goes on the border; the body (answers, reasons) in a box below, wrapped, so it isn't cut off
            let body: Vec<Line> = body.lines().map(|l| Line::raw(disp(l))).collect();
            let bh = if body.is_empty() { 0 } else { wrapped_height(&body, 90u16.min(area.width.saturating_sub(4)).saturating_sub(2)) + 2 };
            let all = centered(area, 90, 3 + bh);
            let r = Rect { height: all.height.min(3), ..all };
            if bh > 0 {
                // A plain border: only the input above reads as focused
                let br = Rect { y: r.bottom(), height: all.height - r.height, ..all };
                clear(f, br);
                let b = panel("", false).style(Style::default().bg(POPUP_BG));
                f.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }).block(b), br);
            }
            clear(f, r);
            let hint = if words.is_empty() { "Enter / Esc" } else { "Enter / Esc · Tab complete" };
            input.set_block(panel(format!(" {} ({hint}) ", disp(title)), true));
            f.render_widget(&*input, r);
            // Autocomplete list: right below the input, near the cursor
            if let Some((items, sel, _)) = complete {
                let w = items.iter().map(|s| s.chars().count()).max().unwrap_or(0) as u16 + 4;
                let x = (r.x + 1 + input.cursor().1 as u16).min(r.right().saturating_sub(w));
                let lr = Rect { x, y: r.bottom(), width: w.min(r.width), height: (items.len() as u16 + 2).min(area.bottom().saturating_sub(r.bottom())) };
                let rows: Vec<ListItem> = items
                    .iter()
                    .enumerate()
                    .map(|(i, s)| ListItem::new(format!(" {s}")).style(if i == *sel { Style::default().bg(SEL) } else { Style::default() }))
                    .collect();
                clear(f, lr);
                f.render_widget(List::new(rows).block(panel("", true)).style(Style::default().bg(POPUP_BG)), lr);
            }
        }
        Popup::Palette(p) => {
            let r = centered(area, 100, 24);
            clear(f, r);
            let [inp, list] = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(r);
            app.hits.palette = Block::default().borders(Borders::ALL).inner(list);
            let t = format!(" {}{} ", p.title, if p.loading > 0 { " — loading…" } else { "" });
            p.input.set_block(panel(t, true));
            p.input.set_cursor_line_style(Style::default());
            f.render_widget(&p.input, inp);
            let matches = App::palette_matches(p);
            let h = list.height.saturating_sub(2) as usize;
            let sel = p.sel.min(matches.len().saturating_sub(1));
            let off = sel.saturating_sub(h.saturating_sub(1));
            app.hits.palette_off = off;
            let items: Vec<ListItem> = matches
                .iter()
                .enumerate()
                .skip(off)
                .take(h)
                .map(|(i, it)| {
                    // Key labels inside names·hints follow the screen rules too ("Ctrl+Z (grid)" → ⌃z)
                    ListItem::new(Row::new(disp(&it.label)).hint(format!(" {}", disp(&it.hint))).selected(i == sel, true).line())
                })
                .collect();
            p.sel = sel;
            let ddl = app.keys.keys(crate::app::Action::Ddl).first().map(|k| disp(k)).unwrap_or_else(|| "—".into());
            let hint = if p.title.starts_with("Go to") || p.title == "Search Everywhere" { format!(" Enter open · {ddl} DDL · Esc ") } else { " Enter · Esc ".into() };
            f.render_widget(List::new(items).block(panel(hint, true)), list);
        }
        Popup::ConnForm(form) => draw_conn_form(f, area, form),
    }
}

/// Confirm: multiple lines (reason · statement · question). Red border for production DB · dangerous statements; "→ N rows" stands out more the larger it is.
/// In a window too short for all of it, the popup takes the whole height; the first line, the questions (a line ending in "?" or
/// with a "? " in it), the warnings (⚠) and the keys stay, and the details in between (statements, columns, tables) give way to a line saying how many are
/// hidden and how many rows they would change. If even that doesn't fit, the keys line says to enlarge the window and the result is
/// false: y / Enter don't answer a question that isn't on screen
fn draw_confirm(f: &mut Frame, area: Rect, title: &'static str, text: &str, keys: Line<'static>) -> bool {
    let danger = text.starts_with("Production") || text.ends_with("Run it anyway?") || text.lines().any(|l| l.starts_with('⚠') && l.contains("Production"));
    let mut lines = confirm_lines(text);
    let last = lines.len() - 1;
    lines[last] = keys;
    let keep: Vec<bool> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let t = l.to_string();
            i == 0 || i == last || t.starts_with('⚠') || (!t.starts_with(' ') && (t.trim_end().ends_with('?') || t.contains("? ")))
        })
        .collect();
    // Long statements wrap → count wrapped lines for the height (so the y/n hint below isn't cut)
    let w = 90u16.min(area.width.saturating_sub(4)).saturating_sub(2);
    let want = wrapped_height(&lines, w).saturating_add(2);
    let mut r = centered(area, 90, want);
    if r.height < want {
        r = Rect { y: area.y, height: area.height, ..r };
    }
    let inner = if danger { popup_danger(f, r, title) } else { popup(f, r, title) };
    let fits = wrapped_height(&cut_lines(lines.clone(), &keep, inner.width, inner.height), inner.width) <= inner.height;
    if !fits {
        let mut l = key_hints(&[("n / Esc", "no")]);
        l.spans.insert(0, Span::styled("Enlarge the window to answer · ", Style::default().fg(WARN)));
        lines[last] = l;
    }
    let ask = lines.split_off(last);
    let ah = wrapped_height(&ask, inner.width).min(inner.height);
    let [body, bottom] = Layout::vertical([Constraint::Min(0), Constraint::Length(ah)]).areas(inner);
    f.render_widget(Paragraph::new(cut_lines(lines, &keep[..last], body.width, body.height)).wrap(Wrap { trim: false }), body);
    f.render_widget(Paragraph::new(ask).wrap(Wrap { trim: false }), bottom);
    fits
}

/// Data Source form. Only the chosen Tunnel's fields show. The size stays the same whatever the Tunnel and the message (a test
/// starting or failing doesn't move the form). The message and key hints stay at the bottom; when the window is too short for
/// every field, the fields scroll to keep the focused one in view and the bottom border says which field it is
fn draw_conn_form(f: &mut Frame, area: Rect, form: &crate::app::ConnForm) {
    let full_hint = Line::styled(disp("↑↓/Tab move · type to edit · Space/←→ toggle · ⌃t test · ⌃s / Enter save · Esc cancel"), Style::default().fg(DIM));
    let w = 72.min(area.width.saturating_sub(4)).saturating_sub(2);
    let r = centered(area, 72, form.max_visible() as u16 + 1 + FORM_MESSAGE_ROWS + wrapped_height(std::slice::from_ref(&full_hint), w) + 2);
    clear(f, r);
    let vis = form.visible();
    let fields: Vec<Line> = vis
        .iter()
        .map(|&i| (i, &form.fields[i]))
        .map(|(i, fl)| {
            let shown = match fl.kind {
                FieldKind::Secret if fl.value != "<saved>" => "•".repeat(fl.value.chars().count()),
                FieldKind::Toggle => format!("[{}]", if fl.value == "on" { "x" } else { " " }),
                FieldKind::Choice(_) => format!("◂ {} ▸", fl.value),
                _ => fl.value.clone(),
            };
            let sel = i == form.sel;
            let mut spans = field_row(fl.label, 17, shown + if sel && matches!(fl.kind, FieldKind::Text | FieldKind::Secret) { "▏" } else { "" }, sel);
            if fl.label == "Color" {
                let c = COLOR_PRESETS.iter().find(|p| p.0 == fl.value).and_then(|p| hex(p.1)).or_else(|| form.edit.as_ref().and_then(|e| e.color.as_deref()).filter(|_| fl.value == "custom").and_then(hex));
                if let Some(c) = c {
                    spans.push(Span::raw(" "));
                    spans.push(swatch(c));
                }
            }
            if fl.label == "SSL mode" {
                spans.push(Span::styled(format!("  {}", ssl_mode_note(&fl.value)), Style::default().fg(DIM)));
            }
            if let Some(note) = empty_field_note(fl.label).filter(|_| fl.value.trim().is_empty()) {
                spans.push(Span::styled(format!("  {note}"), Style::default().fg(DIM)));
            }
            Line::from(spans)
        })
        .collect();
    let msg = conn_form_message(form);
    let t = if form.edit.is_some() { " Data Source Properties " } else { " New Data Source " };
    let block = panel(t, true);
    let inner = block.inner(r);
    let width = inner.width;
    let fields_h = wrapped_height(&fields, width);
    let msg_h = wrapped_height(&msg, width);
    // The message keeps FORM_MESSAGE_ROWS even when empty, so a result appearing doesn't move the fields
    let msg_want = msg_h.max(FORM_MESSAGE_ROWS);
    // The full key hints when everything fits, else the short ones. The hints keep their row, then at least the focused field;
    // the message gets what is left (cut with ⋯) and the blank line above it only when there's room
    let mut hint = full_hint;
    if fields_h + 1 + msg_want + wrapped_height(std::slice::from_ref(&hint), width) > inner.height {
        hint = Line::styled(disp("⌃t test · ⌃s save · Esc cancel"), Style::default().fg(DIM));
    }
    let hint_h = wrapped_height(std::slice::from_ref(&hint), width).min(inner.height);
    let room = inner.height - hint_h;
    let gap = u16::from(fields_h + 1 + msg_want <= room);
    let rows = msg_want.min(room.saturating_sub(gap + 1));
    let mut foot = vec![Line::raw(""); gap as usize];
    if msg_h <= rows {
        foot.extend(msg);
    } else if rows > 0 {
        // Too long for the rows left: one text, cut with ⋯ (the first line's color)
        let st = msg.first().map_or_else(Style::default, |l| l.style);
        let text: Vec<String> = msg.iter().map(|l| l.to_string()).collect();
        foot.push(Line::styled(fit_rows(&text.join(" "), width, rows), st));
    }
    let used = wrapped_height(&foot, width);
    foot.extend(std::iter::repeat_n(Line::raw(""), (gap + rows).saturating_sub(used) as usize));
    foot.push(hint);
    let fh = wrapped_height(&foot, width).min(inner.height);
    let [body, foot_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(fh)]).areas(inner);
    // Scroll in wrapped rows: the focused field's last row at the bottom when it would be below
    let pos = vis.iter().position(|&i| i == form.sel).unwrap_or(0);
    let rows_to = |n: usize| wrapped_height(&fields[..n], body.width);
    let top = rows_to(pos + 1).saturating_sub(body.height);
    let block = if rows_to(fields.len()) > body.height { block.title_bottom(Line::from(format!(" {}/{} ", pos + 1, fields.len())).right_aligned()) } else { block };
    f.render_widget(block, r);
    f.render_widget(Paragraph::new(fields).wrap(Wrap { trim: false }).scroll((top, 0)), body);
    f.render_widget(Paragraph::new(foot).wrap(Wrap { trim: false }), foot_area);
}

/// Text cut to `rows` wrapped rows of `width`, ending in "⋯" when cut
fn fit_rows(text: &str, width: u16, rows: u16) -> String {
    let fits = |s: &str| wrapped_height(&[Line::raw(s.to_string())], width) <= rows;
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // The longest prefix (in chars) that fits with "⋯" after it
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&format!("{}⋯", chars[..mid].iter().collect::<String>())) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    format!("{}⋯", chars[..lo].iter().collect::<String>().trim_end())
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

    /// The SSL mode choice says what each mode checks (Require doesn't check the certificate)
    #[test]
    fn ssl_mode_note_says_what_is_checked() {
        assert_eq!(ssl_mode_note("require"), "encrypt, no certificate check");
        assert_eq!(ssl_mode_note("verify-ca"), "checks the certificate's CA");
        assert_eq!(ssl_mode_note("verify-full"), "checks the CA and host name");
    }

    /// Below the minimum size: a message with the size needed instead of a broken layout (narrower than 24 cells used to panic)
    #[tokio::test]
    async fn too_small_terminal_says_so_instead_of_drawing_the_layout() {
        let mut h = crate::testkit::Harness::new().await;
        h.table_tab("payment", &["id", "amount"], &[&["1", "2"]], &["id"]);
        let s = h.screen(39, 10);
        assert!(s.contains("Terminal too small") && s.contains("39×10 — needs 40×10"), "{s}");
        assert!(s.contains("Make the window larger · ⌃q quits"), "{s}");
        h.app.conn_form(None);
        assert!(!h.screen(39, 10).contains("New Data Source"));
        assert!(h.screen(80, 9).contains("80×9 — needs 40×10"));
        // At the minimum the layout is back
        assert!(!h.screen(40, 10).contains("too small"));
        // Tiny screens don't panic; the message wraps
        for (w, ht) in [(1, 1), (10, 3), (23, 24), (24, 5)] {
            h.screen(w, ht);
        }
        assert!(h.screen(20, 6).contains("Terminal too"));
    }

    /// Under the message keys and clicks would act on a screen that isn't drawn: only the quit key works
    #[tokio::test]
    async fn too_small_terminal_ignores_keys_and_clicks_but_quit() {
        let mut h = crate::testkit::Harness::new().await;
        let cols: Vec<String> = (0..30).map(|i| format!("column_{i}")).collect();
        let cols: Vec<&str> = cols.iter().map(|c| c.as_str()).collect();
        h.table_tab("wide", &cols, &[&["1"; 30]], &["column_0"]);
        h.app.focus = crate::app::Focus::Grid;
        h.screen(120, 30);
        let g = h.app.grid_mut().unwrap();
        g.col_off = 10;
        let bar = g.hbar;
        assert!(bar.width > 0);
        // A confirm that arrives (or was open) when the window shrinks
        h.app.popup = Some(crate::app::Popup::Confirm { opened: None, text: "Run it?".into(), on_yes: Box::new(|a| a.status = "ran".into()) });
        assert!(h.screen(39, 30).contains("Terminal too small"));
        h.key("Enter");
        h.key("y");
        h.paste("x");
        assert!(h.app.popup.is_some() && h.app.status != "ran");
        h.app.popup = None;
        h.mouse("down", bar.x + 6, bar.y);
        h.key("Right");
        let g = h.app.grid_mut().unwrap();
        assert_eq!((g.col_off, g.col), (10, 0), "the hidden grid doesn't scroll or move");
        assert!(h.app.drag.is_none());
        // Back at full size keys act again; the quit key works under the message too
        h.screen(120, 30);
        h.key("Right");
        assert_eq!(h.app.grid_mut().unwrap().col, 1);
        h.screen(39, 30);
        h.key("Ctrl+Q");
        assert!(h.app.quit);
    }

    /// Under the message the quit key never answers or replaces a question nobody can see: the message says what waits
    #[tokio::test]
    async fn too_small_terminal_quit_waits_for_open_questions() {
        let mut h = crate::testkit::Harness::new().await;
        // A half-filled form stays as it is
        h.app.conn_form(None);
        h.type_str("half");
        let s = h.screen(100, 9);
        assert!(s.contains("A dialog is open: make the window larger to answer it") && !s.contains("quits"), "{s}");
        h.key("Ctrl+Q");
        assert!(!h.app.quit);
        assert!(matches!(&h.app.popup, Some(Popup::ConnForm(f)) if f.fields[0].value == "half"));
        h.app.popup = None;
        // Uncommitted changes: no hidden 'Roll them back and quit?' that y or Enter could answer
        h.console_tab();
        if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.get_mut(h.app.active).map(|t| &mut t.kind) {
            c.in_tx = true;
        }
        let s = h.screen(100, 9);
        assert!(s.contains("1 console(s) have uncommitted changes: make the window larger to quit"), "{s}");
        for k in ["Ctrl+Q", "y", "Enter"] {
            h.key(k);
        }
        assert!(!h.app.quit && h.app.popup.is_none());
        // Larger again: the quit key asks, and the answer is seen
        h.screen(120, 30);
        h.key("Ctrl+Q");
        assert!(h.screen(120, 30).contains("Roll them back and quit?"));
        h.key("y");
        assert!(h.app.quit);
    }

    /// The Data Source form at 80×24 and at the minimum size: the focused field scrolls into view and the key hints stay
    #[tokio::test]
    async fn conn_form_scrolls_to_the_focused_field_in_small_windows() {
        let mut h = crate::testkit::Harness::new().await;
        h.app.conn_form(None);
        let n = match &h.app.popup {
            Some(Popup::ConnForm(f)) => f.visible().len(),
            _ => unreachable!(),
        };
        // Everything fits at 120×40: no position shown
        assert!(!h.screen(120, 40).contains(&format!("1/{n}")));
        for _ in 1..n {
            h.key("Tab");
        }
        for (w, ht) in [(80, 24), MIN_SIZE] {
            let s = h.screen(w, ht);
            assert!(s.contains("Redis nodes ▏"), "{w}×{ht}\n{s}");
            assert!(s.contains("Esc cancel") && s.contains(&format!("{n}/{n}")), "{w}×{ht}\n{s}");
        }
        for _ in 1..n {
            h.key("Up");
        }
        for (w, ht) in [(80, 24), MIN_SIZE] {
            let s = h.screen(w, ht);
            assert!(s.contains("Name ▏") && s.contains("Esc cancel"), "{w}×{ht}\n{s}");
        }
    }

    /// The Production confirm at the minimum size keeps its question and keys; the statements that don't fit are counted
    /// with the rows they would change, so y is never an answer to an unseen row count
    #[tokio::test]
    async fn production_confirm_keeps_its_question_in_small_windows() {
        let mut h = crate::testkit::Harness::new().await;
        let text = "Production data source \"[PROD] kr\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n  delete from payment where created_at < now() - interval '1 year'\n     → 12,408 rows\n\nRun it?";
        h.app.popup = Some(Popup::Confirm { opened: None, text: text.into(), on_yes: Box::new(|a| a.status = "ran".into()) });
        let s = h.screen(80, 24);
        assert!(s.contains("12,408 rows") && s.contains("Run it?") && !s.contains("hidden"), "{s}");
        // Every row that would change shows, as its own line or in the hidden total
        for (w, ht) in [MIN_SIZE, (60, 12)] {
            let s = h.screen(w, ht);
            assert!(s.contains("Production data") && s.contains("Run it?") && s.contains("y / Enter"), "{w}×{ht}\n{s}");
            let all = s.contains("→ 1 row") && s.contains("→ 12,408 rows");
            assert!(all || s.contains("12,409 rows") || (s.contains("→ 1 row") && s.contains("hidden · 12,408 rows")), "{w}×{ht}\n{s}");
        }
        // One statement at the minimum size: its row count shows
        let one = "Production data source \"[PROD] kr\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n\nRun it?";
        h.app.popup = Some(Popup::Confirm { opened: None, text: one.into(), on_yes: Box::new(|a| a.status = "ran".into()) });
        let (w, ht) = MIN_SIZE;
        let s = h.screen(w, ht);
        assert!(s.contains("1 row") && s.contains("Run it?"), "{s}");
        h.key("y");
        assert_eq!(h.app.status, "ran");
    }

    /// A confirm of 1000 statements draws in a few ms, cut or not (each line wraps once per frame)
    #[tokio::test]
    async fn big_confirm_draws_fast() {
        let mut h = crate::testkit::Harness::new().await;
        let lines: Vec<String> = (0..1000).map(|i| format!("  update payment set amount = 0 where id = {i}\n     → 1 row")).collect();
        let text = format!("Production data source \"[PROD] kr\" — this changes data:\n\n{}\n\nRun it?", lines.join("\n"));
        h.app.popup = Some(Popup::Confirm { opened: None, text, on_yes: Box::new(|_| {}) });
        for (w, ht) in [MIN_SIZE, (80, 24)] {
            let t = std::time::Instant::now();
            let s = h.screen(w, ht);
            assert!(s.contains("lines hidden · ") && s.contains("Run it?"), "{w}×{ht}\n{s}");
            // Debug build, other tests running: generous; the quadratic cut took seconds here
            assert!(t.elapsed() < std::time::Duration::from_millis(500), "{w}×{ht}: {:?}", t.elapsed());
        }
    }

    /// A long test message at the minimum size: the key hints and the focused field stay, the message ends in ⋯
    #[tokio::test]
    async fn conn_form_keeps_its_hints_under_a_long_message() {
        let mut h = crate::testkit::Harness::new().await;
        h.app.conn_form(None);
        let Some(Popup::ConnForm(f)) = &mut h.app.popup else { unreachable!() };
        f.message = Some((false, "Connection failed: could not connect to server: Connection refused. Is the server running on host db.example.com and accepting TCP/IP connections on port 5432?".into()));
        let (w, ht) = MIN_SIZE;
        let s = h.screen(w, ht);
        assert!(s.contains("Esc cancel") && s.contains("Name ▏") && s.contains("Connection failed") && s.contains('⋯'), "{s}");
        let s = h.screen(80, 24);
        assert!(s.contains("Esc cancel") && s.contains("port 5432?") && !s.contains('⋯'), "{s}");
    }

    /// The startup color notices fit an 80-column status line whole, with a gap before the key hints
    #[tokio::test]
    async fn color_notices_fit_80_columns() {
        let mut h = crate::testkit::Harness::new().await;
        for mode in [crate::theme::ColorMode::Ansi16, crate::theme::ColorMode::NoColor] {
            let n = mode.notice().unwrap();
            h.app.status = n.into();
            let s = h.screen(80, 24);
            assert!(s.lines().last().unwrap().contains(&format!("{n} ")), "{s}");
        }
    }

    /// NO_COLOR: text selected in the editor stays visible (tui-textarea draws it on a color, which NO_COLOR drops)
    #[tokio::test]
    async fn no_color_shows_selected_text_in_reverse_video() {
        let mut h = crate::testkit::Harness::new().await;
        h.console_with("select 12345 from t", 0, 0);
        h.app.focus = crate::app::Focus::Editor;
        for _ in 0..6 {
            h.key("Shift+Right");
        }
        assert_eq!(h.console_cursor().1.as_deref(), Some("select"));
        h.app.color = crate::theme::ColorMode::NoColor;
        let buf = h.buffer(100, 30);
        let (x, y) = (0..30).find_map(|y| {
            let l: String = (0..100).map(|x| buf[(x, y)].symbol()).collect();
            l.find("select 12345").map(|i| (l[..i].chars().count() as u16, y))
        }).unwrap();
        assert!((0..6).all(|i| buf[(x + i, y)].modifier.contains(Modifier::REVERSED)), "selected text is reversed");
        assert!(!buf[(x + 7, y)].modifier.contains(Modifier::REVERSED), "the rest isn't");
    }

    #[test]
    fn popup_clears_wide_chars_straddling_its_edges() {
        let mut term = Terminal::new(ratatui::backend::TestBackend::new(20, 1)).unwrap();
        term.draw(|f| {
            // "가" is at cells 3·4, "나" at 9·10 → straddling both edges of the popup 4..10
            f.buffer_mut().set_string(0, 0, "abc가defg나", Style::default().bg(Color::Yellow));
            clear(f, Rect::new(4, 0, 6, 1));
            f.render_widget(Paragraph::new("│popup").style(Style::default()), Rect::new(4, 0, 6, 1));
        })
        .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(3, 0)].symbol(), " ", "left half of 가 would hide the popup's left edge");
        assert_eq!(buf[(4, 0)].symbol(), "│");
        assert_eq!(buf[(10, 0)].symbol(), " ");
        assert_eq!(buf[(10, 0)].bg, Color::Yellow, "the cell after the popup keeps the row's colour");
    }
}
