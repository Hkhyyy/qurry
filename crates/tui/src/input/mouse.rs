use super::*;

fn inside(r: Rect, x: u16, y: u16) -> bool {
    r.width > 0 && x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

/// Mouse wheel: scroll the area under the pointer
pub(super) fn scroll_at(app: &mut App, x: u16, y: u16, d: isize) {
    if app.popup.is_some() {
        return;
    }
    if inside(app.hits.tree, x, y) {
        app.focus = Focus::Tree;
    } else if inside(app.hits.results, x, y) || inside(app.hits.redis_value, x, y) {
        app.focus = Focus::Grid;
        if let Some(TabKind::Redis(r)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            r.focus_value = true;
        }
    } else if inside(app.hits.text, x, y) {
        // Text tabs (DDL·diagram): scroll the text
        app.focus = Focus::Grid;
        if let Some(TabKind::Text(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            t.scroll = (t.scroll as isize + d).max(0) as u16;
        }
        return;
    } else if inside(app.hits.redis_keys, x, y) {
        if let Some(TabKind::Redis(r)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            r.focus_value = false;
            let n = r.rows().len();
            r.sel = (r.sel as isize + d).clamp(0, n.saturating_sub(1) as isize) as usize;
        }
        return;
    } else if inside(app.hits.editor, x, y) {
        app.focus = Focus::Editor;
        if let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            let m = if d > 0 { tui_textarea::CursorMove::Down } else { tui_textarea::CursorMove::Up };
            for _ in 0..d.unsigned_abs() {
                c.editor.move_cursor(m);
            }
        }
        return;
    }
    scroll(app, d);
}

/// Horizontal scroll (trackpad left/right·Shift+wheel): shifts the tree sideways, moves the grid by columns
pub(super) fn hscroll_at(app: &mut App, x: u16, y: u16, d: isize) {
    if app.popup.is_some() {
        return;
    }
    if inside(app.hits.tree, x, y) {
        app.tree.hoff = (app.tree.hoff as isize + d).max(0) as usize;
    } else if inside(app.hits.results, x, y) || inside(app.hits.redis_value, x, y) {
        if let Some(g) = app.grid_mut() {
            g.hscroll(d.signum());
        }
    }
}

/// Whether the pointer is over the grid header or the horizontal scrollbar
pub(super) fn over_hbar(app: &mut App, x: u16, y: u16) -> bool {
    app.popup.is_none() && app.grid_mut().is_some_and(|g| inside(g.header, x, y) || inside(g.hbar, x, y))
}

/// Where a dragged tab lands if dropped at (x, y)
pub fn drop_target(app: &App, id: u64, x: u16, y: u16) -> Option<DropTarget> {
    let from = app.tab_index(id).map(|i| app.tabs[i].group)?;
    for &(g, bar, body) in &app.hits.groups {
        if inside(bar, x, y) {
            let before = app.hits.tabs.iter().find(|(_, x0, x1, ty)| *ty == y && x >= *x0 && x < *x1).map(|t| t.0);
            return Some(DropTarget::Group(g, before));
        }
        if inside(body, x, y) {
            // Dropping on a pane's right / bottom edge splits that pane (up to 4 panes; skipped for the only tab in its own pane, since splitting changes nothing)
            let alone = g == from && app.tabs.iter().filter(|t| t.group == g).count() < 2;
            if app.panes.leaves().len() < crate::panes::MAX && !alone {
                if x >= body.x + body.width * 2 / 3 {
                    return Some(DropTarget::NewSplit(g, SplitDir::Right));
                }
                if y >= body.y + body.height * 2 / 3 {
                    return Some(DropTarget::NewSplit(g, SplitDir::Down));
                }
            }
            return (g != from).then_some(DropTarget::Group(g, None));
        }
    }
    None
}

pub(super) fn mouse_down(app: &mut App, x: u16, y: u16) {
    if app.popup.is_none() {
        if let Some(&(i, _)) = app.hits.workspaces.iter().find(|w| inside(w.1, x, y)) {
            return app.ws_switch(i);
        }
        if inside(app.hits.cancel, x, y) {
            return app.cancel();
        }
        if inside(app.hits.divider, x, y) {
            app.drag = Some(Drag::Console);
            return;
        }
        if let Some(i) = app.hits.splits.iter().position(|s| inside(s.2, x, y)) {
            app.drag = Some(Drag::Split(i));
            return;
        }
        if inside(app.hits.explorer_divider, x, y) {
            app.drag = Some(Drag::Explorer);
            return;
        }
        if let Some(g) = app.grid_mut().filter(|g| inside(g.hbar, x, y)) {
            g.hscroll_to(x);
            app.drag = Some(Drag::HBar);
            return;
        }
        // Tabs: click to select, drag to move
        if let Some(&(id, ..)) = app.hits.tabs.iter().find(|(_, x0, x1, ty)| *ty == y && x >= *x0 && x < *x1) {
            if let Some(i) = app.tab_index(id) {
                app.activate(i);
            }
            app.drag = Some(Drag::Tab { id, moved: false });
            app.drag_pos = (x, y);
            return;
        }
        // Top/bottom split: the empty part of the lower pane's tab bar can also be grabbed like the divider (a one-line border is hard to hit)
        if let Some(i) = app.hits.splits.iter().position(|s| inside(s.3, x, y)) {
            app.drag = Some(Drag::Split(i));
            return;
        }
        // Clicking an unfocused pane focuses it
        let focused = app.focused_group();
        if let Some(&(g, ..)) = app.hits.groups.iter().find(|(g, bar, body)| *g != focused && (inside(*bar, x, y) || inside(*body, x, y))) {
            if let Some(i) = app.group_tab(g) {
                app.activate(i);
            }
            return;
        }
    }
    click(app, x, y);
}

pub(super) fn mouse_drag(app: &mut App, x: u16, y: u16) {
    match app.drag {
        Some(Drag::Console) => {
            let c = app.hits.console;
            if c.height > 0 {
                app.console_pct = ((y.saturating_sub(c.y) as u32 * 100 / c.height as u32) as u16).clamp(10, 90);
            }
        }
        Some(Drag::Split(i)) => {
            let Some((path, a, _, _, dir)) = app.hits.splits.get(i).cloned() else { return };
            let (p, start, len) = match dir {
                SplitDir::Right => (x, a.x, a.width),
                SplitDir::Down => (y, a.y, a.height),
            };
            if let (Some(pct), true) = (app.panes.pct_mut(&path), len > 0) {
                *pct = ((p.saturating_sub(start) as u32 * 100 / len as u32) as u16).clamp(15, 85);
            }
        }
        Some(Drag::Explorer) => {
            let w = x.saturating_sub(app.hits.main.x) as i16 + 1;
            app.set_explorer_width(w);
        }
        Some(Drag::HBar) => {
            if let Some(g) = app.grid_mut() {
                g.hscroll_to(x);
            }
        }
        Some(Drag::Grid) => {
            if let Some(g) = app.grid_mut() {
                if let Some((r, c)) = g.hit(x, y) {
                    let anchor = g.anchor.unwrap_or((g.row, g.col));
                    g.row = r;
                    if let Some(c) = c {
                        g.col = c;
                    }
                    g.anchor = (anchor != (g.row, g.col)).then_some(anchor).or(g.anchor);
                }
            }
        }
        Some(Drag::Editor { anchor, selecting }) => {
            let Some(pos) = editor_pos(app, x, y) else { return };
            if let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                if !selecting {
                    c.editor.move_cursor(tui_textarea::CursorMove::Jump(anchor.0 as u16, anchor.1 as u16));
                    c.editor.start_selection();
                }
                c.editor.move_cursor(tui_textarea::CursorMove::Jump(pos.0 as u16, pos.1 as u16));
            }
            app.drag = Some(Drag::Editor { anchor, selecting: true });
        }
        Some(Drag::Tab { id, moved }) => {
            if moved || (x, y) != app.drag_pos {
                app.drag = Some(Drag::Tab { id, moved: true });
                app.status = "Drop on another tab bar or pane to move · on the right / bottom edge to split".into();
            }
            app.drag_pos = (x, y);
        }
        None => {}
    }
}

/// Screen cell → console text position (line, column). Based on the last drawn cursor cell (outside the editor, the nearest line·column)
fn editor_pos(app: &App, x: u16, y: u16) -> Option<(usize, usize)> {
    use unicode_width::UnicodeWidthChar;
    let (cx, cy, row, col) = app.hits.caret?;
    let Some(TabKind::Console(c)) = app.tabs.get(app.active).map(|t| &t.kind) else { return None };
    let lines = c.editor.lines();
    let w = |ch: char| ch.width().unwrap_or(0) as i32;
    let r = (row as i32 + y as i32 - cy as i32).clamp(0, lines.len().saturating_sub(1) as i32) as usize;
    // Screen x where column 0 was drawn (relative to the cursor line; correct even when scrolled horizontally)
    let x0 = cx as i32 - lines.get(row)?.chars().take(col).map(w).sum::<i32>();
    let (mut acc, mut n) = (0, 0);
    for ch in lines[r].chars() {
        if acc >= x as i32 - x0 {
            break;
        }
        acc += w(ch);
        n += 1;
    }
    Some((r, n))
}

pub(super) fn mouse_up(app: &mut App, x: u16, y: u16) {
    if let Some(Drag::Tab { id, moved: true }) = app.drag {
        app.status.clear();
        if let Some(t) = drop_target(app, id, x, y) {
            app.drop_tab(id, t);
        }
    }
    app.drag = None;
}

/// Two clicks on the same spot within 0.4s = double-click
fn double(app: &mut App, x: u16, y: u16) -> bool {
    let now = std::time::Instant::now();
    let dbl = matches!(app.hits.last_click, Some((t, px, py)) if now.duration_since(t).as_millis() < 400 && px == x && py == y);
    app.hits.last_click = if dbl { None } else { Some((now, x, y)) };
    dbl
}

fn click(app: &mut App, x: u16, y: u16) {
    let dbl = double(app, x, y);
    // Popups: only palette items are clickable (clicking elsewhere closes)
    if let Some(p) = &mut app.popup {
        if let Popup::Palette(pal) = p {
            if inside(app.hits.palette, x, y) {
                let i = app.hits.palette_off + (y - app.hits.palette.y) as usize;
                let items = App::palette_matches(pal);
                if let Some(it) = items.get(i).map(|x| (*x).clone()) {
                    pal.sel = i;
                    app.run_palette(it, false);
                }
            }
        }
        return;
    }
    // Tree
    if inside(app.hits.tree, x, y) {
        app.focus = Focus::Tree;
        let i = app.tree.offset + (y - app.hits.tree.y) as usize;
        // Copy only the clicked line
        let Some((depth, node)) = app.tree.visible().get(i).map(|(d, n)| (*d, (*n).clone())) else { return };
        app.tree.sel = i;
        // Clicking ▸ expands/collapses
        let chev = app.hits.tree.x + (depth * 2) as u16;
        if node.expandable && (x == chev || x == chev + 1) {
            app.toggle_node(&node);
        } else if dbl {
            app.activate_node(&node, false);
        }
        return;
    }
    // Editor: click moves the cursor, drag selects
    if inside(app.hits.editor, x, y) {
        app.focus = Focus::Editor;
        if let Some(pos) = editor_pos(app, x, y) {
            if let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                c.editor.cancel_selection();
                c.editor.move_cursor(tui_textarea::CursorMove::Jump(pos.0 as u16, pos.1 as u16));
                c.complete = None;
            }
            app.drag = Some(Drag::Editor { anchor: pos, selecting: false });
        }
        return;
    }
    // Redis key list
    if inside(app.hits.redis_keys, x, y) {
        let id = app.tabs[app.active].id;
        if let Some(TabKind::Redis(r)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            app.focus = Focus::Grid;
            r.focus_value = false;
            let i = r.off + (y - app.hits.redis_keys.y) as usize;
            match r.rows().get(i).cloned() {
                Some(crate::app::KRow::Key { idx, .. }) => {
                    r.sel = i;
                    let k = r.keys[idx].key.clone();
                    app.redis_load_key(id, k);
                }
                // Clicking a folder expands/collapses it
                Some(crate::app::KRow::Folder { path, .. }) => {
                    r.sel = i;
                    if !r.open.remove(&path) {
                        r.open.insert(path);
                    }
                }
                None => {}
            }
        }
        return;
    }
    // Grid (results / table / Redis value)
    if inside(app.hits.results, x, y) || inside(app.hits.redis_value, x, y) {
        app.focus = Focus::Grid;
        if let Some(TabKind::Redis(r)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            r.focus_value = true;
        }
        let is_table = matches!(app.tabs.get(app.active).map(|t| &t.kind), Some(TabKind::Table(_)));
        let mut start = false;
        if let Some(g) = app.grid_mut() {
            if let Some((r, c)) = g.hit(x, y) {
                g.row = r;
                if let Some(c) = c {
                    g.col = c;
                }
                g.anchor = None;
                start = true;
                if dbl && is_table {
                    grid_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    start = false;
                }
            }
        }
        // Drag to select a range
        if start {
            app.drag = Some(Drag::Grid);
        }
        app.fetch_more();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    /// When tabs overflow the tab bar, scroll so the current tab is visible (so later tabs aren't cut off). Clicks match the scrolled positions too
    #[tokio::test]
    async fn tab_bar_scrolls_to_the_active_tab() {
        let mut h = Harness::new().await;
        for i in 0..8 {
            h.table_tab(&format!("long_table_name_{i}"), &["id"], &[&["1"]], &["id"]);
        }
        let s = h.screen(80, 20);
        let bar = s.lines().find(|l| l.contains("long_table_name_7")).unwrap_or_else(|| panic!("{s}"));
        assert!(!bar.contains("long_table_name_0"), "{bar}");
        // Going to the first tab shows the front again
        h.key("Alt+Right");
        let s = h.screen(80, 20);
        let (y, bar) = s.lines().enumerate().find(|(_, l)| l.contains("long_table_name_0")).unwrap_or_else(|| panic!("{s}"));
        assert!(!bar.contains("long_table_name_7"), "{bar}");
        // Clicking a visible tab switches to it
        let x = bar[..bar.find("long_table_name_1").unwrap()].chars().count() as u16;
        h.mouse("down", x, y as u16);
        h.mouse("up", x, y as u16);
        assert_eq!(h.app.tabs[h.app.active].title, "long_table_name_1");
    }

    /// Console mouse: click moves the cursor, drag selects (only the selection runs), clicking again clears the selection
    #[tokio::test]
    async fn console_mouse_click_and_drag_select() {
        let mut h = Harness::new().await;
        h.console_with("select 1\nselect * from public.users", 0, 0);
        let s = h.screen(120, 30);
        let (y, line) = s.lines().enumerate().find(|(_, l)| l.contains("public.users")).unwrap();
        let x = line[..line.find("public").unwrap()].chars().count() as u16;
        let y = y as u16;
        h.mouse("down", x, y);
        h.mouse("drag", x + 3, y);
        h.screen(120, 30);
        h.mouse("drag", x + 6, y);
        h.mouse("up", x + 6, y);
        assert_eq!(h.console_cursor(), ((1, 20), Some("public".to_string())));
        // ⌘C copies the selected text to the clipboard (the terminal's drag-to-copy doesn't work because the app captures the mouse)
        h.key("Cmd+c");
        assert_eq!(h.app.status, "Copied: public");
        // Clicking "1" on the line above moves the cursor there and clears the selection
        h.screen(120, 30);
        h.mouse("down", x - 7, y - 1);
        h.mouse("up", x - 7, y - 1);
        assert_eq!(h.console_cursor(), ((0, 7), None));
        // Clicking past the end of a line goes to that line's end
        h.mouse("down", x + 40, y);
        h.mouse("up", x + 40, y);
        assert_eq!(h.console_cursor(), ((1, 26), None));
    }

    #[tokio::test]
    async fn text_tab_scrolls_with_the_wheel() {
        let mut h = Harness::new().await;
        let text = (1..=200).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        h.app.push_tab("test", "postgres".into(), "Diagram: public".into(), TabKind::Text(crate::app::TextTab { text, scroll: 0 }));
        h.app.focus = Focus::Tree;
        let screen = h.screen(100, 30);
        assert!(screen.contains("y copies all"), "{screen}");
        let r = h.app.hits.text;
        crate::input::on_event(&mut h.app, Event::Mouse(crossterm::event::MouseEvent { kind: MouseEventKind::ScrollDown, column: r.x + 5, row: r.y + 5, modifiers: KeyModifiers::NONE }));
        let scroll = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Text(t) => t.scroll,
            _ => panic!(),
        };
        assert_eq!((scroll(&h), h.app.focus), (3, Focus::Grid), "the wheel scrolls the text, not the explorer");
        h.key("G");
        h.screen(100, 30);
        assert_eq!(scroll(&h), 200 - (r.height - 2), "End stops at the last line");
        h.key("g");
        assert_eq!(scroll(&h), 0);
    }
}
