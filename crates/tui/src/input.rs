//! Key input → actions (DataGrip keymap adapted for the terminal)
use crate::app::{Action, App, Drag, DropTarget, FieldKind, Focus, Popup, SplitDir, TabKind};
use crate::tree::NodeKind;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

fn inside(r: Rect, x: u16, y: u16) -> bool {
    r.width > 0 && x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

/// Mouse wheel: scroll the area under the pointer
fn scroll_at(app: &mut App, x: u16, y: u16, d: isize) {
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
fn hscroll_at(app: &mut App, x: u16, y: u16, d: isize) {
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
fn over_hbar(app: &mut App, x: u16, y: u16) -> bool {
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

fn mouse_down(app: &mut App, x: u16, y: u16) {
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

fn mouse_drag(app: &mut App, x: u16, y: u16) {
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

fn mouse_up(app: &mut App, x: u16, y: u16) {
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

pub fn on_event(app: &mut App, ev: Event) {
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => on_key(app, k),
        Event::Paste(s) => paste(app, s),
        Event::Mouse(m) => match m.kind {
            // Shift+wheel = horizontal scroll
            MouseEventKind::ScrollDown if m.modifiers.contains(KeyModifiers::SHIFT) => hscroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::SHIFT) => hscroll_at(app, m.column, m.row, -3),
            // ⌥+wheel, or wheel over the grid header·scrollbar = horizontal scroll (for terminals that don't pass the horizontal wheel)
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::ALT) || over_hbar(app, m.column, m.row) => {
                hscroll_at(app, m.column, m.row, if m.kind == MouseEventKind::ScrollDown { 1 } else { -1 })
            }
            MouseEventKind::ScrollDown => scroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollUp => scroll_at(app, m.column, m.row, -3),
            MouseEventKind::ScrollRight => hscroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollLeft => hscroll_at(app, m.column, m.row, -3),
            MouseEventKind::Down(MouseButton::Left) => mouse_down(app, m.column, m.row),
            MouseEventKind::Drag(MouseButton::Left) => mouse_drag(app, m.column, m.row),
            MouseEventKind::Up(MouseButton::Left) => mouse_up(app, m.column, m.row),
            _ => {}
        },
        _ => {}
    }
}

/// ⌘V / terminal paste: into the open input (single-line input·palette·Data Source form) if any, otherwise the console editor
fn paste(app: &mut App, s: String) {
    // Terminals (xterm.js, tmux, etc.) may send newlines as \r. A \r left in the editor breaks the display
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    // Newlines become spaces in single-line inputs
    let one = || s.trim_end_matches(['\r', '\n']).replace("\r\n", " ").replace(['\r', '\n'], " ");
    if matches!(app.popup, Some(Popup::Find(_))) {
        return app.find_paste(&s);
    }
    match &mut app.popup {
        Some(Popup::Input { input, .. }) => {
            input.insert_str(one());
        }
        Some(Popup::Palette(p)) => {
            if p.input.insert_str(one()) {
                p.sel = 0;
            }
        }
        Some(Popup::ConnForm(form)) => {
            let f = &mut form.fields[form.sel];
            if matches!(f.kind, FieldKind::Text | FieldKind::Secret) {
                if f.value == "<saved>" {
                    f.value.clear();
                }
                f.value.push_str(&one());
            }
        }
        Some(_) => {}
        None => match app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            Some(TabKind::Console(c)) if app.focus == Focus::Editor => {
                c.editor.insert_str(s);
            }
            // Table data: fill from the current cell (⌘V)
            Some(TabKind::Table(tt)) if app.focus == Focus::Grid => {
                if let Some(g) = tt.grid.as_mut().filter(|g| g.cols() > 0) {
                    g.paste(&s);
                }
            }
            _ => {}
        },
    }
}

fn scroll(app: &mut App, d: isize) {
    match app.focus {
        Focus::Tree => {
            let n = app.tree.visible().len();
            app.tree.sel = (app.tree.sel as isize + d).clamp(0, n.saturating_sub(1) as isize) as usize;
        }
        _ => {
            if let Some(g) = app.grid_mut() {
                g.move_by(d, 0);
            }
            app.fetch_more();
        }
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.popup.is_some() {
        return popup_key(app, k);
    }
    if app.designer.is_some() {
        return crate::designer::key(app, k);
    }
    // ── Global commands (keymap: keys.rs, rebind: F1) ──
    if let Some(a) = app.keys.lookup(&k) {
        // Run key in the explorer = refresh
        if a == Action::Execute && app.focus == Focus::Tree {
            return app.run_action(Action::Refresh);
        }
        return app.run_action(a);
    }
    // Alt+1~9: workspaces (if the keymap binds that key to another command, that command wins)
    if let KeyCode::Char(c @ '1'..='9') = k.code
        && k.modifiers == KeyModifiers::ALT
    {
        return app.ws_switch(c as usize - '1' as usize);
    }
    match app.focus {
        Focus::Tree => tree_key(app, k),
        Focus::Editor => editor_key(app, k),
        Focus::Grid => grid_key(app, k),
    }
}

/// Filter candidates by the word before the cursor (ignoring case·quotes, skipping ones already fully typed, up to 8)
fn input_complete(input: &tui_textarea::TextArea, words: &[String], force: bool) -> Option<(Vec<String>, usize, usize)> {
    if words.is_empty() {
        return None;
    }
    let (row, col) = input.cursor();
    let before: Vec<char> = input.lines().get(row)?.chars().take(col).collect();
    let n = before.iter().rev().take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '"' || **c == '`').count();
    if n == 0 && !force {
        return None;
    }
    let p: String = before[before.len() - n..].iter().collect::<String>().trim_start_matches(['"', '`']).to_lowercase();
    let items: Vec<String> = words
        .iter()
        .filter(|w| {
            let w = w.trim_matches(['"', '`']).to_lowercase();
            w.starts_with(&p) && w != p
        })
        .take(8)
        .cloned()
        .collect();
    (!items.is_empty()).then_some((items, 0, n))
}

/// For screens with single-letter commands (a add row, d delete …): ⌘←/→ are Home/End (terminals may send them as ⌃A/⌃E), ⌘C is y (copy),
/// other letters pressed with ⌃/⌥/⌘ are not taken as commands (fixes ⌘← arriving as ⌃A and adding a row)
fn plain(k: KeyEvent) -> Option<KeyEvent> {
    let m = k.modifiers;
    let to = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
    match k.code {
        KeyCode::Char('a') if m == KeyModifiers::CONTROL => to(KeyCode::Home),
        KeyCode::Char('e') if m == KeyModifiers::CONTROL => to(KeyCode::End),
        KeyCode::Left if m.contains(KeyModifiers::SUPER) => to(KeyCode::Home),
        KeyCode::Right if m.contains(KeyModifiers::SUPER) => to(KeyCode::End),
        // ⌘C = y (copy)
        KeyCode::Char('c') if m == KeyModifiers::SUPER => to(KeyCode::Char('y')),
        // ⌃Z = undo edit
        KeyCode::Char('z') if m == KeyModifiers::CONTROL => Some(k),
        KeyCode::Char(_) if m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => None,
        _ => Some(k),
    }
}

fn tree_key(app: &mut App, k: KeyEvent) {
    let Some(k) = plain(k) else { return };
    let n = app.tree.visible().len();
    // Typing a speed search
    if !app.tree.filter.is_empty() || k.code == KeyCode::Char('/') {
        match k.code {
            // If a filter is set, open with the current filter filled in (clear it and Enter = remove)
            KeyCode::Char('/') => {
                let cur = app.tree.filter.clone();
                app.input("Speed search (filters loaded nodes · empty = clear)", &cur, |a, s| {
                    a.tree.filter = s.trim().to_string();
                    a.tree.sel = 0;
                    a.tree.clamp();
                });
                return;
            }
            KeyCode::Esc => {
                app.tree.filter.clear();
                app.tree.clamp();
                return;
            }
            _ => {}
        }
    }
    let sel = app.tree.selected();
    match k.code {
        // Ctrl+↓ (⌘↓ Edit Data / Jump to Source)
        KeyCode::Down if k.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(nd) = sel {
                app.activate_node(&nd, true)
            }
        }
        KeyCode::Down | KeyCode::Char('j') => app.tree.sel = (app.tree.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => app.tree.sel = app.tree.sel.saturating_sub(1),
        KeyCode::PageDown => app.tree.sel = (app.tree.sel + 20).min(n.saturating_sub(1)),
        KeyCode::PageUp => app.tree.sel = app.tree.sel.saturating_sub(20),
        KeyCode::Home => app.tree.sel = 0,
        KeyCode::End => app.tree.sel = n.saturating_sub(1),
        KeyCode::Right if k.modifiers.contains(KeyModifiers::SHIFT) => app.tree.hoff += 4,
        KeyCode::Left if k.modifiers.contains(KeyModifiers::SHIFT) => app.tree.hoff = app.tree.hoff.saturating_sub(4),
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(nd) = sel {
                if nd.expandable && !app.tree.expanded.contains(&nd.key) {
                    app.toggle_node(&nd);
                } else if nd.expandable {
                    app.tree.sel = (app.tree.sel + 1).min(n.saturating_sub(1));
                }
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(nd) = sel {
                if app.tree.expanded.contains(&nd.key) {
                    app.toggle_node(&nd);
                } else if let Some(p) = app.tree.parent_index() {
                    app.tree.sel = p;
                }
            }
        }
        KeyCode::Enter => {
            if let Some(nd) = sel {
                app.activate_node(&nd, false)
            }
        }
        KeyCode::F(4) => {
            if let Some(nd) = sel {
                app.activate_node(&nd, true)
            }
        }
        KeyCode::Char('n') => app.run_action(Action::NewDataSource),
        KeyCode::Char('e') => app.run_action(Action::EditDataSource),
        // Move the selected Data Source to another workspace
        KeyCode::Char('w') => app.ws_move_conn_pick(),
        KeyCode::Char('D') => app.run_action(Action::DeleteDataSource),
        KeyCode::Char('c') => app.run_action(Action::Connect),
        KeyCode::Char('X') => app.run_action(Action::Disconnect),
        KeyCode::Char('r') => app.run_action(Action::Refresh),
        KeyCode::Char('s') => app.run_action(Action::ManageSchemas),
        KeyCode::Char('?') => app.run_action(Action::Help),
        KeyCode::Char('y') => {
            if let Some(nd) = sel {
                let r = match (&nd.schema, &nd.name) {
                    (Some(s), Some(n)) if !matches!(nd.kind, NodeKind::Column { .. }) => format!("{s}.{n}"),
                    (_, Some(n)) => n.clone(),
                    _ => nd.label.clone(),
                };
                copy(app, &r);
            }
        }
        KeyCode::Tab | KeyCode::Esc => {
            if !app.tabs.is_empty() {
                app.activate(app.active);
            }
        }
        _ => {}
    }
}

/// Copy to the clipboard via OSC 52 (supported by most modern terminals)
pub fn copy(app: &mut App, text: &str) {
    use std::io::Write;
    // When running on this machine, write straight to the system clipboard (many terminals block OSC 52 or truncate it). Over SSH, OSC 52 only (the remote clipboard is meaningless)
    let native = std::env::var_os("SSH_CONNECTION").is_none() && std::env::var_os("SSH_TTY").is_none() && native_copy(text);
    if !native {
        let b64 = base64_encode(text.as_bytes());
        let _ = write!(std::io::stdout(), "\x1b]52;c;{b64}\x07");
        let _ = std::io::stdout().flush();
    }
    let lines = text.lines().count();
    app.status = if lines > 1 { format!("Copied {lines} lines") } else { format!("Copied: {}", text.chars().take(60).collect::<String>()) };
}

/// pbcopy (macOS) · wl-copy / xclip (Linux). true on success
fn native_copy(text: &str) -> bool {
    use std::io::Write;
    // Don't overwrite the clipboard of the person running the tests
    if cfg!(test) {
        return false;
    }
    use std::process::{Command, Stdio};
    let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") { &[("pbcopy", &[])] } else { &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])] };
    for (cmd, args) in tools {
        let Ok(mut child) = Command::new(cmd).args(*args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { continue };
        let wrote = child.stdin.take().is_some_and(|mut w| w.write_all(text.as_bytes()).is_ok());
        if wrote && child.wait().is_ok_and(|s| s.success()) {
            return true;
        }
    }
    false
}
fn base64_encode(b: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn editor_key(app: &mut App, k: KeyEvent) {
    let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) else {
        app.focus = Focus::Grid;
        return grid_key(app, k);
    };
    // While the autocomplete list is open
    if let Some(cmp) = &mut c.complete {
        let n = cmp.items.len();
        match k.code {
            KeyCode::Up => return cmp.sel = (cmp.sel + n - 1) % n,
            KeyCode::Down => return cmp.sel = (cmp.sel + 1) % n,
            KeyCode::PageUp => return cmp.sel = cmp.sel.saturating_sub(10),
            KeyCode::PageDown => return cmp.sel = (cmp.sel + 10).min(n - 1),
            KeyCode::Enter | KeyCode::Tab => return app.accept_completion(),
            KeyCode::Esc => return c.complete = None,
            _ => {}
        }
    }
    let open = c.complete.is_some();
    match k.code {
        // Esc: to the explorer; Tab (when not at an empty spot) indents, so Shift+Tab/F6 goes to results
        KeyCode::Esc => {
            app.explorer = true;
            app.focus = Focus::Tree;
        }
        KeyCode::BackTab => app.focus = Focus::Grid,
        // ⌘A = select all (delete with ⌫), ⌘C = copy selected text. Other ⌘+letter combos don't insert text
        KeyCode::Char('a') if k.modifiers == KeyModifiers::SUPER => c.editor.select_all(),
        KeyCode::Char('c') if k.modifiers == KeyModifiers::SUPER => {
            if let Some(((r0, c0), (r1, c1))) = c.editor.selection_range() {
                let t = crate::app::selected_text(c.editor.lines(), r0, c0, r1, c1);
                return copy(app, &t);
            }
        }
        KeyCode::Char(_) if k.modifiers.contains(KeyModifiers::SUPER) => {}
        _ => {
            c.editor.input(k);
        }
    }
    // Typing a word or '.' opens it (like the GUI), deleting while open re-filters. Any other key closes it
    // (On macOS Ctrl+Space switches input sources, so opening by typing is the default)
    let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let typed = plain && matches!(k.code, KeyCode::Char(ch) if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '\'' || ch == ' ' || ch == '>');
    if typed || (open && k.code == KeyCode::Backspace) {
        app.update_completion(false);
    } else if let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
        c.complete = None;
    }
}

fn grid_key(app: &mut App, k: KeyEvent) {
    // Esc: if there's a selection, clear just that first
    if k.code == KeyCode::Esc {
        if let Some(g) = app.grid_mut().filter(|g| g.anchor.is_some()) {
            g.anchor = None;
            return;
        }
    }
    let Some(k) = plain(k) else { return };
    let Some(t) = app.tabs.get_mut(app.active) else {
        app.focus = Focus::Tree;
        return;
    };
    let id = t.id;
    match &mut t.kind {
        TabKind::Dba(d) => match k.code {
            KeyCode::Char('r') => app.dba_reload(id),
            KeyCode::Char('a') => app.dba_toggle_auto(),
            KeyCode::Char('c') => app.dba_kill(false),
            KeyCode::Char('K') => app.dba_kill(true),
            KeyCode::Enter => app.dba_actions(true),
            KeyCode::Char('g') => app.dba_actions(false),
            KeyCode::Char('p') => app.dba_palette(),
            KeyCode::Esc => app.focus = Focus::Tree,
            _ => {
                // Moving selects a new row (instead of the vanished session)
                d.gone = None;
                nav(app, k)
            }
        },
        TabKind::Text(tt) => match k.code {
            KeyCode::Down | KeyCode::Char('j') => tt.scroll = tt.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => tt.scroll = tt.scroll.saturating_sub(1),
            KeyCode::PageDown => tt.scroll = tt.scroll.saturating_add(20),
            KeyCode::PageUp => tt.scroll = tt.scroll.saturating_sub(20),
            KeyCode::Home | KeyCode::Char('g') => tt.scroll = 0,
            // Up to the last line that fits the screen (trimmed when drawing)
            KeyCode::End | KeyCode::Char('G') => tt.scroll = u16::MAX,
            KeyCode::Char('y') => {
                let s = tt.text.clone();
                copy(app, &s)
            }
            KeyCode::Esc => app.focus = Focus::Tree,
            _ => {}
        },
        TabKind::Redis(_) => redis_key(app, id, k),
        TabKind::Console(c) => {
            if matches!(k.code, KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab) {
                app.focus = Focus::Editor;
                return;
            }
            if c.show_output || c.grid.is_none() {
                return;
            }
            nav(app, k);
        }
        TabKind::Table(tt) => {
            match k.code {
                KeyCode::Char('w') => {
                    let w = tt.where_.clone();
                    let words = app.table_words(id, false);
                    app.input_words("WHERE", &w, words, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            tt.where_ = s;
                        }
                        a.reload_table_checked(id);
                    });
                }
                KeyCode::Char('o') => {
                    let o = tt.order.clone();
                    let words = app.table_words(id, true);
                    app.input_words("ORDER BY", &o, words, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            tt.order = s;
                        }
                        a.reload_table_checked(id);
                    });
                }
                KeyCode::Enter | KeyCode::F(2) => {
                    let Some(g) = &tt.grid else { return };
                    let (r, c) = (g.row, g.col);
                    if g.cols() == 0 {
                        return;
                    }
                    let name = g.result.columns[c].name.clone();
                    let cur = g.value(r, c).unwrap_or_default();
                    app.input(format!("{name} (row {})", r + 1), &cur, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            if let Some(g) = &mut tt.grid {
                                g.set_value(r, c, s);
                            }
                        }
                    });
                }
                KeyCode::Char('n') => app.run_action(Action::SetNull),
                KeyCode::Char('a') | KeyCode::Insert => app.run_action(Action::AddRow),
                KeyCode::Char('d') | KeyCode::Delete => app.run_action(Action::DeleteRow),
                KeyCode::Char('z') if k.modifiers.contains(KeyModifiers::CONTROL) => app.run_action(Action::Revert),
                KeyCode::Char('r') => app.reload_table_checked(id),
                KeyCode::Char('s') => app.sort_table(id),
                KeyCode::Char('x') => app.run_action(Action::ExportCsv),
                KeyCode::Char('/') => {
                    let w = tt.where_.clone();
                    let words = app.table_words(id, false);
                    app.input_words("WHERE", &w, words, move |a, s| {
                        if let Some(TabKind::Table(tt)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            tt.where_ = s;
                        }
                        a.reload_table_checked(id);
                    });
                }
                KeyCode::Esc => app.focus = Focus::Tree,
                _ => nav(app, k),
            }
        }
    }
}

/// Grid navigation
fn nav(app: &mut App, k: KeyEvent) {
    let Some(g) = app.grid_mut() else { return };
    let page = g.page as isize;
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    match k.code {
        // Shift+arrows = range selection (DataGrip). Horizontal scroll is trackpad·Shift+wheel·scrollbar
        KeyCode::Down if shift => g.extend(1, 0),
        KeyCode::Up if shift => g.extend(-1, 0),
        KeyCode::Right if shift => g.extend(0, 1),
        KeyCode::Left if shift => g.extend(0, -1),
        KeyCode::Down | KeyCode::Char('j') => g.move_by(1, 0),
        KeyCode::Up | KeyCode::Char('k') => g.move_by(-1, 0),
        KeyCode::Right | KeyCode::Char('l') => g.move_by(0, 1),
        KeyCode::Left | KeyCode::Char('h') => g.move_by(0, -1),
        KeyCode::PageDown => g.move_by(page, 0),
        KeyCode::PageUp => g.move_by(-page, 0),
        KeyCode::Home => g.col = 0,
        KeyCode::End => g.col = g.cols().saturating_sub(1),
        KeyCode::Char('g') => g.row = 0,
        KeyCode::Char('G') => g.row = g.rows().saturating_sub(1),
        KeyCode::Char('y') => {
            // TSV if a range is selected (pastes into cells in a sheet), otherwise the current cell
            let Some((r0, c0, r1, c1)) = g.sel_rect() else {
                let v = g.value(g.row, g.col).unwrap_or_else(|| "NULL".into());
                return copy(app, &v);
            };
            let text = g.selection_tsv();
            copy(app, &text);
            let (nr, nc) = (r1 - r0 + 1, c1 - c0 + 1);
            app.status = format!("Copied {nr} row{} × {nc} column{}", if nr == 1 { "" } else { "s" }, if nc == 1 { "" } else { "s" });
            return;
        }
        KeyCode::Char('Y') => return app.run_action(Action::CopyInList),
        KeyCode::Char('x') => return app.run_action(Action::ExportCsv),
        KeyCode::Char('s') => {
            g.toggle_local_sort();
            if g.result.truncated {
                app.status = "Sorted the loaded rows only".into();
            }
            return;
        }
        _ => return,
    }
    app.fetch_more();
}

fn redis_key(app: &mut App, id: u64, k: KeyEvent) {
    let Some(TabKind::Redis(r)) = app.tab_mut(id).map(|t| &mut t.kind) else { return };
    let cur_key = r.detail.as_ref().map(|d| d.0.clone());
    let cur_ty = r.detail.as_ref().map(|d| d.1.ty.clone()).unwrap_or_default();
    if r.focus_value {
        match k.code {
            KeyCode::Esc | KeyCode::Tab | KeyCode::Left if r.value.is_none() || k.code != KeyCode::Left => {
                r.focus_value = false;
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                let Some(key) = cur_key else { return };
                // Edit the selected element
                let (field, val) = match (&r.value, &r.detail) {
                    (Some(g), _) => (g.value(g.row, 0).unwrap_or_default(), g.value(g.row, g.cols().saturating_sub(1)).unwrap_or_default()),
                    (None, Some((_, d))) => (String::new(), d.value.as_str().unwrap_or("").to_string()),
                    _ => return,
                };
                let ty = cur_ty.clone();
                let title = match ty.as_str() {
                    "hash" => format!("HSET {key} {field}"),
                    "list" => format!("LSET {key} {field}"),
                    "zset" => format!("score of {field}"),
                    "set" => "replace member".into(),
                    "string" => format!("SET {key}"),
                    _ => return,
                };
                let init = if ty == "set" { field.clone() } else { val };
                app.input(title, &init, move |a, v| {
                    let cmds: Vec<Vec<String>> = match ty.as_str() {
                        "hash" => vec![vec!["HSET".into(), key.clone(), field.clone(), v]],
                        "list" => vec![vec!["LSET".into(), key.clone(), field.clone(), v]],
                        "zset" => vec![vec!["ZADD".into(), key.clone(), v, field.clone()]],
                        "set" => vec![vec!["SREM".into(), key.clone(), field.clone()], vec!["SADD".into(), key.clone(), v]],
                        "string" => vec![vec!["SET".into(), key.clone(), v, "KEEPTTL".into()]],
                        _ => vec![],
                    };
                    for c in cmds {
                        a.redis_cmd(id, c, false);
                    }
                });
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                let (Some(key), Some(g)) = (cur_key, &r.value) else { return };
                let item = g.value(g.row, 0).unwrap_or_default();
                let cmd: Vec<String> = match cur_ty.as_str() {
                    "hash" => vec!["HDEL".into(), key, item],
                    "set" => vec!["SREM".into(), key, item],
                    "zset" => vec!["ZREM".into(), key, item],
                    "stream" => vec!["XDEL".into(), key, item],
                    "list" => {
                        // Replace the element at the index with a marker, then LREM
                        let mark = format!("__dbtool_del_{}", uuid::Uuid::new_v4());
                        app.redis_cmd(id, vec!["LSET".into(), key.clone(), item, mark.clone()], false);
                        vec!["LREM".into(), key, "1".into(), mark]
                    }
                    _ => return,
                };
                app.popup = Some(Popup::Confirm { text: format!("{}?", cmd.join(" ")), on_yes: Box::new(move |a| a.redis_cmd(id, cmd, false)) });
            }
            KeyCode::Char('a') => {
                let Some(key) = cur_key else { return };
                let ty = cur_ty.clone();
                let hint = match ty.as_str() {
                    "hash" => "field value",
                    "zset" => "score member",
                    "stream" => "field value",
                    _ => "value",
                };
                app.input(format!("Add to {key} ({hint})"), "", move |a, v| {
                    let parts: Vec<String> = v.splitn(2, ' ').map(String::from).collect();
                    let two = |p: &Vec<String>| p.len() == 2;
                    let cmd: Vec<String> = match ty.as_str() {
                        "hash" if two(&parts) => vec!["HSET".into(), key.clone(), parts[0].clone(), parts[1].clone()],
                        "zset" if two(&parts) => vec!["ZADD".into(), key.clone(), parts[0].clone(), parts[1].clone()],
                        "stream" if two(&parts) => vec!["XADD".into(), key.clone(), "*".into(), parts[0].clone(), parts[1].clone()],
                        "list" => vec!["RPUSH".into(), key.clone(), v],
                        "set" => vec!["SADD".into(), key.clone(), v],
                        _ => {
                            a.status = format!("Expected: {hint}");
                            return;
                        }
                    };
                    a.redis_cmd(id, cmd, false);
                });
            }
            _ => {
                if r.value.is_some() {
                    nav(app, k)
                }
            }
        }
        return;
    }
    let rows = r.rows();
    let n = rows.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            r.sel = (r.sel + 1).min(n.saturating_sub(1));
            if let Some(k) = r.sel_key() {
                app.redis_load_key(id, k)
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            r.sel = r.sel.saturating_sub(1);
            if let Some(k) = r.sel_key() {
                app.redis_load_key(id, k)
            }
        }
        // Folders: → / Enter expand·collapse. Keys: go to the value
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if matches!(rows.get(r.sel), Some(crate::app::KRow::Folder { .. })) => {
            if let Some(crate::app::KRow::Folder { path, .. }) = rows.get(r.sel) {
                let open = r.open.contains(path);
                if open && k.code == KeyCode::Right {
                    r.sel += 1;
                } else if open {
                    r.open.remove(path);
                } else {
                    r.open.insert(path.clone());
                }
            }
        }
        // ←: collapse an open folder, otherwise go to the parent folder
        KeyCode::Left | KeyCode::Char('h') => match rows.get(r.sel) {
            Some(crate::app::KRow::Folder { path, .. }) if r.open.contains(path) => {
                r.open.remove(path);
            }
            Some(row) => {
                let depth = match row {
                    crate::app::KRow::Folder { depth, .. } | crate::app::KRow::Key { depth, .. } => *depth,
                };
                if depth > 0 {
                    if let Some(p) = rows[..r.sel].iter().rposition(|x| matches!(x, crate::app::KRow::Folder { depth: d, .. } if *d == depth - 1)) {
                        r.sel = p;
                    }
                }
            }
            None => {}
        },
        KeyCode::Enter | KeyCode::Right | KeyCode::Tab => r.focus_value = true,
        KeyCode::Char('/') => {
            let p = r.pattern.clone();
            app.input("Key pattern (SCAN MATCH)", &p, move |a, s| {
                if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    r.pattern = if s.trim().is_empty() { "*".into() } else { s };
                }
                a.redis_scan(id, true);
            });
        }
        KeyCode::Char('m') => app.redis_scan(id, false),
        KeyCode::Char('r') => app.redis_scan(id, true),
        KeyCode::Char('D') | KeyCode::Delete => {
            if let Some(key) = cur_key {
                app.popup = Some(Popup::Confirm { text: format!("Delete key \"{key}\"?"), on_yes: Box::new(move |a| {
                    if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                        r.detail = None;
                        r.value = None;
                    }
                    a.redis_cmd(id, vec!["DEL".into(), key], true)
                }) });
            }
        }
        KeyCode::Char('R') => {
            if let Some(key) = cur_key {
                let k2 = key.clone();
                app.input("Rename key", &key, move |a, to| {
                    if to != k2 {
                        if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            r.detail = None;
                        }
                        a.redis_cmd(id, vec!["RENAME".into(), k2, to], true);
                    }
                });
            }
        }
        KeyCode::Char('t') => {
            if let Some(key) = cur_key {
                let ttl = r.detail.as_ref().map(|d| if d.1.ttl >= 0 { (d.1.ttl / 1000).to_string() } else { String::new() }).unwrap_or_default();
                app.input("TTL seconds (empty = no expiry)", &ttl, move |a, s| {
                    let cmd = if s.trim().is_empty() { vec!["PERSIST".into(), key] } else { vec!["EXPIRE".into(), key, s.trim().into()] };
                    a.redis_cmd(id, cmd, true);
                });
            }
        }
        KeyCode::Char('a') => {
            app.input("Add key: SET key value | HSET key field value | RPUSH key v | SADD key m | ZADD key score m", "SET ", move |a, line| {
                let st = a.state.clone();
                let target = a.tab_mut(id).map(|t| t.target.clone()).unwrap_or_default();
                a.spawn(async move { st.redis_exec_line(target, line).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                    Ok(v) => {
                        a.status = crate::ui::format_reply(&v);
                        a.redis_scan(id, true);
                    }
                    Err(e) => a.status = e,
                });
            });
        }
        KeyCode::Char(':') => {
            app.input("redis>", "", move |a, line| {
                let run = move |a: &mut App, line: String| {
                    let st = a.state.clone();
                    let target = a.tab_mut(id).map(|t| t.target.clone()).unwrap_or_default();
                    let shown = line.clone();
                    a.spawn(async move { st.redis_exec_line(target, line).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                        Ok(v) => a.popup = Some(Popup::Message { title: shown, text: crate::ui::format_reply(&v) }),
                        Err(e) => a.popup = Some(Popup::Message { title: shown, text: format!("(error) {e}") }),
                    });
                };
                // Destructive commands like FLUSHALL·FLUSHDB·DEL ask once more
                match qurry_core::guard::unsafe_statement(&line, qurry_core::store::DbKind::Redis) {
                    Some(why) => a.popup = Some(Popup::Confirm { text: format!("{why}:\n  {line}\nRun it anyway?"), on_yes: Box::new(move |a| run(a, line)) }),
                    None => run(a, line),
                }
            });
        }
        KeyCode::Esc => app.focus = Focus::Tree,
        _ => {}
    }
}

fn popup_key(app: &mut App, k: KeyEvent) {
    if matches!(app.popup, Some(Popup::Find(_))) {
        return crate::find::key(app, k);
    }
    let popup = app.popup.take().unwrap();
    match popup {
        Popup::Help { mut sel, scroll, record: Some(add), .. } => {
            // Recording a new key: Esc = cancel
            let message = if k.code == KeyCode::Esc {
                None
            } else {
                let a = crate::keys::DEFAULT_KEYS[sel].0;
                match crate::keys::spec(&k).filter(|s| crate::keys::bindable(s)) {
                    None => Some(format!("{} can't be a command key — use a {} combination or an F key", crate::keys::spec(&k).map(|s| crate::keys::disp(&s)).unwrap_or_else(|| "That key".into()), if cfg!(target_os = "macos") { "⌃/⌥" } else { "Ctrl/Alt" })),
                    Some(key) => {
                        let taken = app.keys.bind(a, key.clone(), add);
                        let mut m = format!("{} → {}", name_of(a), app.keys.label(a));
                        if let Some(t) = taken {
                            m += &format!(" · removed {key} from {}", name_of(t));
                        }
                        if let Err(e) = app.save_keys() {
                            m = format!("Save failed: {e}");
                        }
                        Some(m)
                    }
                }
            };
            sel = sel.min(crate::keys::DEFAULT_KEYS.len() - 1);
            app.popup = Some(Popup::Help { sel, scroll, record: None, message });
        }
        Popup::Help { sel, scroll, message, .. } => {
            let n = crate::keys::DEFAULT_KEYS.len();
            let last = n + crate::help::HELP.iter().map(|s| s.1.len()).sum::<usize>() - 1;
            let (mut sel, mut record) = (sel, None);
            let mut message = message;
            let fixed = || Some("Fixed key — only the Commands at the top can be changed".to_string());
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) => return,
                KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1).min(last),
                KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
                KeyCode::PageDown | KeyCode::Char(' ') => sel = (sel + 15).min(last),
                KeyCode::PageUp => sel = sel.saturating_sub(15),
                KeyCode::Home | KeyCode::Char('g') => sel = 0,
                KeyCode::End | KeyCode::Char('G') => sel = last,
                KeyCode::Enter if sel < n => record = Some(false),
                KeyCode::Char('a') if sel < n => record = Some(true),
                KeyCode::Enter | KeyCode::Char('a') | KeyCode::Delete | KeyCode::Backspace if sel >= n => message = fixed(),
                KeyCode::Delete | KeyCode::Backspace => {
                    let a = crate::keys::DEFAULT_KEYS[sel].0;
                    app.keys.reset(a);
                    message = Some(match app.save_keys() {
                        Ok(()) => format!("{} reset to {}", name_of(a), app.keys.label(a)),
                        Err(e) => format!("Save failed: {e}"),
                    });
                }
                _ => {}
            }
            app.popup = Some(Popup::Help { sel, scroll, record, message });
        }
        Popup::Message { .. } => {
            if !matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::F(1)) {
                app.popup = Some(popup);
            }
        }
        Popup::Confirm { text, on_yes } => match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => on_yes(app),
            KeyCode::Char('n') | KeyCode::Esc => {}
            _ => app.popup = Some(Popup::Confirm { text, on_yes }),
        },
        Popup::Input { title, mut input, on_ok, words, mut complete } => {
            // If the autocomplete list is open: ↑↓ select · Tab/Enter insert · Esc close
            if let Some((items, sel, prefix)) = &mut complete {
                match k.code {
                    KeyCode::Up => *sel = sel.saturating_sub(1),
                    KeyCode::Down => *sel = (*sel + 1).min(items.len() - 1),
                    KeyCode::Tab | KeyCode::Enter => {
                        for _ in 0..*prefix {
                            input.delete_char();
                        }
                        input.insert_str(&items[*sel]);
                        complete = None;
                    }
                    KeyCode::Esc => complete = None,
                    _ => complete = None,
                }
                if complete.is_some() || matches!(k.code, KeyCode::Tab | KeyCode::Enter | KeyCode::Esc) {
                    app.popup = Some(Popup::Input { title, input, on_ok, words, complete });
                    return;
                }
            }
            match k.code {
                KeyCode::Esc => {}
                KeyCode::Enter => on_ok(app, input.lines().join("")),
                _ => {
                    // Tab·⌃Space = open the list for the current word (single-line input, so no Tab character is inserted)
                    let force = k.code == KeyCode::Tab || (k.code == KeyCode::Char(' ') && k.modifiers.contains(KeyModifiers::CONTROL)) || k.code == KeyCode::Null;
                    if !force {
                        input.input(k);
                    }
                    complete = input_complete(&input, &words, force);
                    app.popup = Some(Popup::Input { title, input, on_ok, words, complete });
                }
            }
        }
        // Handled separately above
        Popup::Find(bar) => app.popup = Some(Popup::Find(bar)),
        Popup::Scope(mut e) => {
            match k.code {
                KeyCode::Esc => return,
                KeyCode::Enter => return app.apply_scope(e),
                KeyCode::Down | KeyCode::Char('j') => e.move_by(1),
                KeyCode::Up | KeyCode::Char('k') => e.move_by(-1),
                KeyCode::PageDown => e.move_by(15),
                KeyCode::PageUp => e.move_by(-15),
                KeyCode::Char(' ') => e.toggle(),
                KeyCode::Left | KeyCode::Char('h') => e.collapse(),
                KeyCode::Right | KeyCode::Char('l') => {
                    if let Some(db) = e.expand() {
                        app.popup = Some(Popup::Scope(e));
                        return app.load_scope_schemas(db);
                    }
                }
                _ => {}
            }
            app.popup = Some(Popup::Scope(e));
        }
        Popup::Palette(mut p) => {
            let n = App::palette_matches(&p).len();
            match k.code {
                KeyCode::Esc => return,
                KeyCode::Down => p.sel = (p.sel + 1).min(n.saturating_sub(1)),
                KeyCode::Up => p.sel = p.sel.saturating_sub(1),
                KeyCode::PageDown => p.sel = (p.sel + 15).min(n.saturating_sub(1)),
                KeyCode::PageUp => p.sel = p.sel.saturating_sub(15),
                KeyCode::Enter => {
                    if let Some(it) = App::palette_matches(&p).get(p.sel).map(|x| (*x).clone()) {
                        return app.run_palette(it, false);
                    }
                }
                KeyCode::Char('b') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(it) = App::palette_matches(&p).get(p.sel).map(|x| (*x).clone()) {
                        return app.run_palette(it, true);
                    }
                }
                _ => {
                    if p.input.input(k) {
                        p.sel = 0;
                    }
                }
            }
            app.popup = Some(Popup::Palette(p));
        }
        Popup::ConnForm(mut form) => {
            let n = form.fields.len();
            match k.code {
                KeyCode::Esc => return,
                KeyCode::Down | KeyCode::Tab => form.sel = (form.sel + 1) % n,
                KeyCode::Up | KeyCode::BackTab => form.sel = (form.sel + n - 1) % n,
                // SSH unknown server: trust after confirming the fingerprint
                KeyCode::Char('y') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    let v = |l: &str| form.fields.iter().find(|f| f.label == l).map(|f| f.value.trim().to_string()).unwrap_or_default();
                    let port = v("SSH port").parse().unwrap_or(22);
                    form.message = Some(match app.state.trust_ssh_host(v("SSH host"), port) {
                        Ok(m) => (true, format!("✓ {m} — press ⌃t to test again")),
                        Err(e) => (false, format!("✕ {e}")),
                    });
                }
                KeyCode::Char('t') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    match App::form_config(&form) {
                        Ok((cfg, pw, ssh)) => {
                            let st = app.state.clone();
                            form.message = Some((true, "Testing…".into()));
                            app.spawn(async move { st.test_connection(cfg, pw, ssh).await.map_err(|e| e.to_string()) }, |a, r| {
                                if let Some(Popup::ConnForm(f)) = &mut a.popup {
                                    f.message = Some(match r {
                                        Ok(v) => (true, format!("✓ {v}")),
                                        Err(e) if e.contains(qurry_core::tunnel::UNKNOWN_HOST) => (false, format!("✕ {e} Press ⌃y to trust it.")),
                                        Err(e) => (false, format!("✕ {e}")),
                                    });
                                }
                            });
                        }
                        Err(e) => form.message = Some((false, e)),
                    }
                }
                KeyCode::Char('s') if k.modifiers.contains(KeyModifiers::CONTROL) => return save_form(app, form),
                KeyCode::Enter => return save_form(app, form),
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if !matches!(form.fields[form.sel].kind, FieldKind::Text | FieldKind::Secret) => {
                    let f = &mut form.fields[form.sel];
                    match &f.kind {
                        FieldKind::Toggle => f.value = if f.value == "on" { "off".into() } else { "on".into() },
                        FieldKind::Choice(opts) => {
                            let i = opts.iter().position(|o| *o == f.value).unwrap_or(0);
                            let j = if k.code == KeyCode::Left { (i + opts.len() - 1) % opts.len() } else { (i + 1) % opts.len() };
                            f.value = opts[j].to_string();
                            // Changing the DB type also changes the default port
                            if f.label == "Type" {
                                const PORTS: &[(&str, &str)] = &[("postgres", "5432"), ("mysql", "3306"), ("redis", "6379"), ("sqlite", "0"), ("duckdb", "0"), ("mssql", "1433"), ("clickhouse", "8123"), ("mongodb", "27017"), ("oracle", "1521"), ("qdrant", "6333"), ("chroma", "8000"), ("milvus", "19530")];
                                let port = PORTS.iter().find(|x| x.0 == f.value).map(|x| x.1).unwrap_or("5432");
                                if let Some(p) = form.fields.iter_mut().find(|x| x.label == "Port") {
                                    // Keep a manually changed port; replace it if it was another type's default port
                                    if p.value.is_empty() || PORTS.iter().any(|x| x.1 == p.value) {
                                        p.value = port.into();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Backspace => {
                    let f = &mut form.fields[form.sel];
                    if f.value == "<saved>" {
                        f.value.clear();
                    } else {
                        f.value.pop();
                    }
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    let f = &mut form.fields[form.sel];
                    if matches!(f.kind, FieldKind::Text | FieldKind::Secret) {
                        if f.value == "<saved>" {
                            f.value.clear();
                        }
                        f.value.push(c);
                    }
                }
                _ => {}
            }
            app.popup = Some(Popup::ConnForm(form));
        }
    }
}

fn save_form(app: &mut App, mut form: crate::app::ConnForm) {
    // What the form doesn't show (scope, startup script …) comes from the source as saved now, not as it was when the form opened
    if let Some(fresh) = form.edit.as_ref().and_then(|e| app.state.store.get(&e.id).ok().flatten()) {
        form.edit = Some(fresh);
    }
    match App::form_config(&form) {
        Ok((cfg, pw, ssh)) => {
            let st = app.state.clone();
            // Form's Workspace: an edited Data Source moves out of where it was, a new one goes into the selected one
            let from = form.edit.as_ref().map(|e| app.ws_of_conn(&e.id));
            let to = form.fields.iter().find(|f| f.label == "Workspace").and_then(|f| app.ws_from_label(&f.value)).unwrap_or(app.cur_ws);
            app.spawn(async move { st.save_connection(cfg, pw, ssh).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                Ok(c) => {
                    a.status = format!("Saved data source {}", c.name);
                    a.ws_place_conn(&c.id, from, to);
                    a.tree.forget(&c.id);
                    a.tree.expanded.remove(&c.id);
                    a.reload_conns();
                }
                Err(e) => a.status = format!("Save failed: {e}"),
            });
        }
        Err(e) => {
            form.message = Some((false, e));
            app.popup = Some(Popup::ConnForm(form));
        }
    }
}

/// Command names shown in the palette·keymap
pub fn name_of(a: Action) -> &'static str {
    crate::app::ACTIONS.iter().find(|x| x.0 == a).map(|x| x.1).unwrap_or("?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_letters_are_not_commands() {
        let k = |c, m| KeyEvent::new(c, m);
        // ⌘← arriving as ⌃A goes to the first column instead of adding a row
        assert_eq!(plain(k(KeyCode::Char('a'), KeyModifiers::CONTROL)).map(|x| x.code), Some(KeyCode::Home));
        assert_eq!(plain(k(KeyCode::Right, KeyModifiers::SUPER)).map(|x| x.code), Some(KeyCode::End));
        assert!(plain(k(KeyCode::Char('d'), KeyModifiers::ALT)).is_none());
        assert!(plain(k(KeyCode::Char('n'), KeyModifiers::SUPER)).is_none());
        assert_eq!(plain(k(KeyCode::Char('D'), KeyModifiers::SHIFT)).map(|x| x.code), Some(KeyCode::Char('D')));
        assert_eq!(plain(k(KeyCode::Char('z'), KeyModifiers::CONTROL)).map(|x| x.code), Some(KeyCode::Char('z')));
    }

    // ───────── Check by feeding keys (testkit) ─────────
    use crate::testkit::Harness;

    /// Table tabs with the same name: schema.table if schemas differ, db.schema.table if the schemas are the same too
    #[tokio::test]
    async fn same_table_tabs_show_schema() {
        let mut h = Harness::new().await;
        let a = h.table_tab("orders", &["id"], &[], &[]);
        let label = |h: &Harness, id: u64| {
            let t = h.app.tabs.iter().find(|t| t.id == id).unwrap();
            h.app.tab_label(t)
        };
        assert_eq!(label(&h, a), "orders");
        let b = h.table_tab("orders", &["id"], &[], &[]);
        if let Some(TabKind::Table(tt)) = h.app.tabs.iter_mut().find(|t| t.id == b).map(|t| &mut t.kind) {
            tt.schema = "billing".into();
        }
        assert_eq!((label(&h, a), label(&h, b)), ("public.orders".into(), "billing.orders".into()));
        let c = h.table_tab("orders", &["id"], &[], &[]);
        h.app.tabs.iter_mut().find(|t| t.id == c).unwrap().db = "app_test".into();
        assert_eq!((label(&h, a), label(&h, c), label(&h, b)), ("postgres.public.orders".into(), "app_test.public.orders".into(), "billing.orders".into()));
    }

    /// ⌘← arriving as ⌃A doesn't add a row (a adds a row)
    #[tokio::test]
    async fn ctrl_a_in_table_moves_to_first_column() {
        let mut h = Harness::new().await;
        let id = h.table_tab("bank_info", &["bank_code", "bank_name"], &[&["002", "산업"]], &["bank_code"]);
        h.key("Right");
        h.key("Ctrl+A");
        assert_eq!(h.table(id).grid.as_ref().unwrap().added, 0);
        assert_eq!(h.table(id).grid.as_ref().unwrap().col, 0);
        h.key("a");
        assert_eq!(h.table(id).grid.as_ref().unwrap().added, 1);
    }

    /// Column·keyword autocomplete in the WHERE input
    #[tokio::test]
    async fn where_input_completes_columns_and_keywords() {
        let mut h = Harness::new().await;
        let id = h.table_tab("bank_info", &["bank_code", "bank_name"], &[&["002", "산업"]], &["bank_code"]);
        h.key("w");
        h.type_str("ban");
        let s = h.screen(120, 30);
        assert!(s.contains("bank_code") && s.contains("bank_name"), "{s}");
        h.key("Tab");
        h.type_str(" = '002' an");
        h.key("Tab");
        h.key("Enter");
        assert_eq!(h.table(id).where_, "bank_code = '002' AND");
    }

    /// ⌘C copies the cell value like y (when the terminal passes ⌘C through)
    #[tokio::test]
    async fn cmd_c_copies_cell() {
        let mut h = Harness::new().await;
        h.table_tab("fee_rule", &["idx", "fee_type"], &[&["7", "PERCENT"]], &["idx"]);
        h.key("Right");
        h.key("Cmd+c");
        assert_eq!(h.app.status, "Copied: PERCENT");
    }

    /// Recent Tabs lists all open tabs (including restored tabs not yet viewed). Most recently viewed first
    #[tokio::test]
    async fn recent_tabs_lists_every_open_tab() {
        let mut h = Harness::new().await;
        let a = h.table_tab("a", &["x"], &[], &[]);
        h.table_tab("b", &["x"], &[], &[]);
        h.app.restore_table("test", "postgres".into(), "public".into(), "c".into(), "table".into(), String::new(), String::new());
        h.app.activate(0);
        h.app.palette_tabs();
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
        let labels: Vec<_> = p.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["a", "b", "c"]);
        assert!(matches!(p.items[0].action, crate::app::PAction::Tab(id) if id == a));
    }

    /// Terminals may send pasted newlines as \r (xterm.js, tmux): lines are kept, no \r remains
    #[tokio::test]
    async fn paste_into_console_keeps_lines() {
        let mut h = Harness::new().await;
        h.console_tab();
        h.paste("BEGIN;\r\r-- 1. 스키마\rGRANT USAGE\r\n  ON x;\rCOMMIT;\r");
        assert_eq!(h.console_text(), "BEGIN;\n\n-- 1. 스키마\nGRANT USAGE\n  ON x;\nCOMMIT;\n");
    }

    /// ⌃Enter: with multiple statements, asks what to run (DataGrip). Runs from the chosen one
    #[tokio::test]
    async fn execute_asks_what_to_run() {
        let mut h = Harness::new().await;
        h.console_with("BEGIN;\nGRANT a TO x;\n\nGRANT b TO x;\nCOMMIT;", 3, 2);
        h.key("F5");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no chooser") };
        let labels: Vec<_> = p.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["GRANT b TO x", "All statements (4)", "From caret to end"]);
        h.key("Down");
        h.key("Down");
        h.key("Enter");
        h.settle().await;
        // Stops at the first statement for lack of a connection: the first statement of the chosen range is GRANT b
        let out = h.console_output();
        assert!(out.iter().any(|l| l.ends_with("> GRANT b TO x")), "{out:?}");
        assert!(!out.iter().any(|l| l.contains("BEGIN")), "{out:?}");
    }

    /// With the execution setting set to Smallest, runs only the cursor statement without asking
    #[tokio::test]
    async fn execute_smallest_runs_caret_statement() {
        let mut h = Harness::new().await;
        h.app.exec.inside = crate::exec::Inside::Smallest;
        h.console_with("BEGIN;\nGRANT a TO x;\n\nGRANT b TO x;\nCOMMIT;", 1, 2);
        h.key("F5");
        assert!(h.app.popup.is_none());
        h.settle().await;
        assert!(h.console_output().iter().any(|l| l.ends_with("> GRANT a TO x")));
    }

    /// Manage Shown Schemas: Space to toggle, Enter to save (the Data Source's scope, shared with the GUI). Esc doesn't save
    #[tokio::test]
    async fn manage_shown_schemas_saves_scope() {
        use qurry_core::store::{ConnectionConfig, DbKind};
        let mut h = Harness::new().await;
        let cfg = ConnectionConfig {
            id: "c1".into(), name: "pg".into(), kind: DbKind::Postgres, host: "localhost".into(), port: 5432, user: "postgres".into(), database: "postgres".into(),
            scope: Default::default(), password_saved: false, ssh_secret_saved: false, ssh: None, color: None, read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.settle().await;
        let dbs = || vec!["app".to_string(), "postgres".to_string(), "shop".to_string()];

        h.app.open_scope("c1", dbs(), None);
        assert!(h.screen(120, 30).contains("Manage Shown Schemas"));
        // Lines: All databases · Default database · app · postgres · shop
        h.key("Space"); // turn off All databases
        for _ in 0..4 {
            h.key("Down");
        }
        h.key("Space"); // shop
        h.key("Esc");
        assert!(h.app.popup.is_none());
        assert_eq!(h.app.state.store.get("c1").unwrap().unwrap().scope.databases, None);

        h.app.open_scope("c1", dbs(), None);
        h.key("Space");
        for _ in 0..4 {
            h.key("Down");
        }
        h.key("Space");
        h.key("Enter");
        assert!(h.app.popup.is_none());
        assert_eq!(h.app.state.store.get("c1").unwrap().unwrap().scope.databases, Some(vec!["shop".to_string()]));
        // The tree uses the new scope right away too
        assert_eq!(h.app.conn("c1").unwrap().config.scope.databases, Some(vec!["shop".to_string()]));

        // Don't revert settings changed elsewhere meanwhile (GUI·MCP) to old values (e.g. read-only turned on)
        let mut changed = h.app.state.store.get("c1").unwrap().unwrap();
        changed.read_only = true;
        changed.host = "db.internal".into();
        h.app.state.store.save(&changed).unwrap();
        h.app.open_scope("c1", dbs(), None);
        h.key("Down");
        h.key("Space"); // turn on Default database
        h.key("Enter");
        let saved = h.app.state.store.get("c1").unwrap().unwrap();
        assert!(saved.read_only, "read_only reverted");
        assert_eq!(saved.host, "db.internal");
        assert_eq!(saved.scope.databases, Some(vec!["shop".to_string(), "@default".to_string()]));
    }

    /// Left/right split: there's a separator between the two panes' tab bars (so they don't look like one continuous line)
    #[tokio::test]
    async fn split_tab_bars_are_separated() {
        let mut h = Harness::new().await;
        h.table_tab("left_table", &["id"], &[&["1"]], &["id"]);
        h.table_tab("right_table", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::SplitRight);
        let s = h.screen(160, 20);
        let bar = s.lines().find(|l| l.contains("left_table") && l.contains("right_table")).unwrap_or_else(|| panic!("{s}"));
        let between = &bar[bar.find("left_table").unwrap()..bar.find("right_table").unwrap()];
        assert!(between.contains('┃'), "{bar}");
    }

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

    /// Closing a tab in a split view goes to the neighboring tab in the same pane (no jump to the first pane even if only unviewed tabs, like restored ones, remain)
    #[tokio::test]
    async fn closing_a_tab_stays_in_its_pane() {
        let mut h = Harness::new().await;
        h.table_tab("left", &["id"], &[&["1"]], &["id"]);
        let right_a = h.table_tab("right_a", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::SplitRight);
        h.table_tab("right_b", &["id"], &[&["1"]], &["id"]);
        h.table_tab("right_c", &["id"], &[&["1"]], &["id"]);
        // Restored tabs aren't in the recent list
        h.app.mru.retain(|x| *x != right_a);
        h.key("Ctrl+w");
        assert_eq!(h.app.tabs[h.app.active].title, "right_b");
        h.key("Ctrl+w");
        assert_eq!(h.app.tabs[h.app.active].title, "right_a");
    }

    /// Export (x): format → file/clipboard → path (extension follows the format)
    #[tokio::test]
    async fn export_asks_format_then_destination() {
        let mut h = Harness::new().await;
        h.table_tab("users", &["id"], &[&["1"]], &["id"]);
        h.key("x");
        let labels = |h: &Harness| match &h.app.popup {
            Some(crate::app::Popup::Palette(p)) => p.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>(),
            _ => panic!("no chooser"),
        };
        assert_eq!(labels(&h), ["CSV", "TSV", "JSON", "SQL INSERT", "Markdown"]);
        h.key("Down");
        h.key("Down");
        h.key("Enter");
        assert_eq!(labels(&h), ["File…", "Clipboard"]);
        h.key("Enter");
        let Some(crate::app::Popup::Input { title, input, .. }) = &h.app.popup else { panic!("no path input") };
        assert_eq!(title, "Export JSON to file");
        assert!(input.lines()[0].ends_with("/Downloads/users.json"), "{:?}", input.lines());
    }

    /// Import: read the file, match columns by name, show what goes in, and ask
    #[tokio::test]
    async fn import_maps_file_columns_by_name() {
        let mut h = Harness::new().await;
        let path = std::env::temp_dir().join(format!("qurry-tui-import-{}.csv", uuid::Uuid::new_v4()));
        std::fs::write(&path, "ID,Name,extra\n1,a,x\n").unwrap();
        h.table_tab("users", &["id", "name"], &[&["1", "a"]], &["id"]);
        h.app.run_action(crate::app::Action::ImportData);
        let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no path input") };
        *input = tui_textarea::TextArea::from([path.display().to_string()]);
        h.key("Enter");
        h.settle().await;
        let Some(crate::app::Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm") };
        assert!(text.contains("into public.users"), "{text}");
        assert!(text.contains("ID → id") && text.contains("Name → name") && text.contains("extra → (skip)"), "{text}");
        std::fs::remove_file(path).unwrap();
    }

    /// The Redis key browser also has ":" folders (Medis): → expand, ← collapse / to parent folder, ↓↑ over visible lines
    #[tokio::test]
    async fn redis_browser_groups_keys_into_folders() {
        let mut h = Harness::new().await;
        h.redis_tab(&["dev:sessions:{1}", "dev:sessions:{2}", "prod:x", "token"]);
        let rows = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Redis(r) => r
                .rows()
                .iter()
                .map(|x| match x {
                    crate::app::KRow::Folder { name, depth, count, path } => format!("{}{} {name} {count}", "  ".repeat(*depth), if r.open.contains(path) { "▾" } else { "▸" }),
                    crate::app::KRow::Key { idx, depth } => format!("{}{}", "  ".repeat(*depth), r.keys[*idx].key),
                })
                .collect::<Vec<_>>(),
            _ => panic!("not redis"),
        };
        let sel = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Redis(r) => r.sel,
            _ => panic!(),
        };
        assert_eq!(rows(&h), ["▸ dev 2", "▸ prod 1", "token"]);
        h.key("Right");
        assert_eq!(rows(&h), ["▾ dev 2", "  ▸ sessions 2", "▸ prod 1", "token"]);
        h.key("Down");
        h.key("Right");
        h.key("Down");
        assert_eq!(rows(&h)[sel(&h)], "    dev:sessions:{1}");
        // ← : on a key goes to the parent folder, on an open folder collapses it
        h.key("Left");
        assert_eq!(sel(&h), 1);
        h.key("Left");
        assert_eq!(rows(&h), ["▾ dev 2", "  ▸ sessions 2", "▸ prod 1", "token"]);
        // Folders and counts on screen too (counts at the right edge, like the GUI)
        let s = h.screen(120, 20);
        let line = s.lines().find(|l| l.contains("▸ ▤ sessions")).unwrap_or_else(|| panic!("{s}"));
        assert!(line.trim_end_matches(['┃', '│', ' ']).ends_with('2') && s.contains("token"), "{s}");
    }

    /// Y: copy the whole current column as an IN list (IPs from results straight into the next query's WHERE ip IN)
    #[tokio::test]
    async fn copy_column_as_in_list() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["1.2.3.4", "1"], &["5.6.7.8", "2"], &["1.2.3.4", "3"]], &[]);
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('1.2.3.4', '5.6.7.8')");
        // From ⌃K too
        assert!(crate::app::ACTIONS.iter().any(|a| a.0 == Action::CopyInList));
    }

    /// Ctrl+F: editor find/replace bar (DataGrip). Alt+X regex, Tab replace field, Alt+A replace all ($1 groups), Esc close
    #[tokio::test]
    async fn console_find_replace_all_with_regex() {
        let mut h = Harness::new().await;
        h.console_with("1.2.3.4 ok\n5.6.7.8 fail", 0, 0);
        h.key("Ctrl+f");
        h.type_str(r"(\d+\.\d+\.\d+\.\d+).*");
        h.key("Alt+x");
        assert!(h.screen(120, 30).contains("1/2"), "{}", h.screen(120, 30));
        h.key("Tab");
        h.type_str("'$1',");
        h.key("Alt+a");
        assert_eq!(h.console_text(), "'1.2.3.4',\n'5.6.7.8',");
        h.key("Esc");
        assert!(h.app.popup.is_none());
    }

    /// Find: each keystroke selects the first match after the cursor, Enter goes to the next. In the replace field, Enter replaces only the current one and moves on
    #[tokio::test]
    async fn console_find_next_and_replace_one() {
        let mut h = Harness::new().await;
        h.console_with("id, ID, x", 0, 0);
        h.key("Ctrl+f");
        h.type_str("id");
        assert_eq!(h.console_cursor().1.as_deref(), Some("id"));
        h.key("Enter");
        assert_eq!(h.console_cursor(), ((0, 6), Some("ID".to_string())));
        h.key("Tab");
        h.type_str("k");
        h.key("Enter");
        assert_eq!(h.console_text(), "id, k, x");
        // Case-sensitive (Alt+C) doesn't match ID
        h.key("Esc");
        h.console_with("id, ID", 0, 0);
        h.key("Ctrl+f");
        h.type_str("ID");
        h.key("Alt+c");
        assert!(h.screen(120, 30).contains("1/1"));
    }

    /// Grid range selection: Shift+↑↓←→ (or drag), y = TSV, Y = IN list of selected cells only, Esc = clear selection
    #[tokio::test]
    async fn grid_range_select_and_copy() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["1.2.3.4", "1"], &["5.6.7.8", "2"], &["9.9.9.9", "3"]], &[]);
        h.key("Shift+Down");
        h.key("y");
        assert_eq!(h.app.status, "Copied 2 rows × 1 column");
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('1.2.3.4', '5.6.7.8')");
        // Esc clears only the selection first
        h.key("Esc");
        assert_eq!(h.app.focus, crate::app::Focus::Grid);
        h.key("Y");
        assert!(h.app.status.starts_with("Copied 3 values"), "{}", h.app.status);
        // Dragging with the mouse selects too
        let s = h.screen(120, 20);
        let (y0, line) = s.lines().enumerate().find(|(_, l)| l.contains("1.2.3.4")).unwrap();
        let x = line[..line.find("1.2.3.4").unwrap()].chars().count() as u16;
        h.mouse("down", x, y0 as u16);
        h.mouse("drag", x, y0 as u16 + 2);
        h.mouse("up", x, y0 as u16 + 2);
        h.key("Y");
        assert_eq!(h.app.status, "Copied 3 values as an IN list: ('1.2.3.4', '5.6.7.8', '9.9.9.9')");
    }

    /// ⌃K › Database Admin…: picking a report opens a DBA tab (reuses that tab for the same connection target)
    #[tokio::test]
    async fn dba_report_tab_opens_and_switches() {
        let mut h = Harness::new().await;
        h.table_tab("t", &["id"], &[&["1"]], &[]);
        h.app.run_action(Action::DbaReports);
        let labels = |h: &Harness| match &h.app.popup {
            Some(crate::app::Popup::Palette(p)) => p.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>(),
            _ => panic!("no chooser"),
        };
        let l = labels(&h);
        assert!(l.contains(&"Activity › Sessions".to_string()) && l.contains(&"Storage › Table Sizes".to_string()), "{l:?}");
        h.key("Enter");
        let n = h.app.tabs.len();
        let report = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Dba(d) => d.report.clone(),
            _ => panic!("not a dba tab"),
        };
        assert_eq!(report(&h), "sessions");
        h.settle().await;
        // No connection, so an error is shown
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.error.is_some()));
        h.app.run_action(Action::DbaReports);
        h.type_str("Table Sizes");
        h.key("Enter");
        assert_eq!(report(&h), "table_sizes");
        assert_eq!(h.app.tabs.len(), n);
    }

    /// Follows the selected session (pid) even when auto-refresh reorders rows (so another session isn't killed)
    #[tokio::test]
    async fn dba_selection_follows_pid_across_reloads() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        let res = |pids: &[i64]| QueryResult {
            columns: vec![ColumnInfo { name: "pid".into(), type_name: "INT4".into() }],
            rows: pids.iter().map(|p| vec![serde_json::json!(p)]).collect(),
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "sessions");
        let d = crate::app::DbaTab { report: "sessions".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res(&[10, 20, 30]))), error: None, running: false, auto: true };
        let id = h.app.push_tab("test", "postgres".into(), "Sessions".into(), TabKind::Dba(d));
        h.key("Down");
        h.app.dba_apply(id, Ok(res(&[30, 20, 10])));
        let TabKind::Dba(d) = &h.app.tabs[h.app.active].kind else { panic!() };
        let g = d.grid.as_ref().unwrap();
        assert_eq!(g.value(g.row, 0).as_deref(), Some("20"));
        // If the selected session disappears, nothing is selected (so the wrong session isn't killed)
        h.app.dba_apply(id, Ok(res(&[30, 10])));
        let TabKind::Dba(d) = &h.app.tabs[h.app.active].kind else { panic!() };
        assert!(d.grid.as_ref().unwrap().result.rows.len() == 2);
        h.key("K");
        assert!(h.app.popup.is_none(), "must not offer to kill a session that is gone");
    }

    /// Control characters (ESC, C1, direction overrides) in DB values · names · errors · the status line don't reach the terminal
    #[tokio::test]
    async fn control_characters_never_reach_the_terminal() {
        let mut h = Harness::new().await;
        let evil = "a\u{1b}]52;c;cm0gLXJmIH4=\u{7}b\u{1b}[2Jc\u{9b}31md\u{202e}gpj.exe";
        h.table_tab(evil, &[evil, "id"], &[&[evil, "1"]], &[]);
        h.app.status = format!("Copied: {evil}");
        let s = h.screen(160, 20);
        let bad: Vec<char> = s.chars().filter(|c| (c.is_control() && *c != '\n') || ('\u{202a}'..='\u{202e}').contains(c) || ('\u{2066}'..='\u{2069}').contains(c)).collect();
        assert!(bad.is_empty(), "{bad:?}");
        assert!(s.contains("]52;c;"), "the rest of the text is still shown");
    }

    /// Saving the form keeps what it doesn't show (scope, startup script …) as saved now, not as it was when the form opened
    #[tokio::test]
    async fn conn_form_keeps_settings_it_does_not_show() {
        use qurry_core::store::{ConnectionConfig, DbKind};
        let mut h = Harness::new().await;
        let cfg = ConnectionConfig {
            id: "c1".into(), name: "pg".into(), kind: DbKind::Postgres, host: "localhost".into(), port: 5432, user: "postgres".into(), database: "postgres".into(),
            scope: Default::default(), password_saved: false, ssh_secret_saved: false, ssh: None, color: None, read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.app.conn_form(Some(cfg.clone()));
        // Meanwhile the GUI changes what the TUI form doesn't show
        let mut other = cfg.clone();
        other.scope.databases = Some(vec!["shop".into()]);
        other.options.startup_script = "SET search_path = shop".into();
        h.app.state.store.save(&other).unwrap();
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "Name").unwrap().value = "renamed".into();
        h.key("Enter");
        h.settle().await;
        let saved = h.app.state.store.get("c1").unwrap().unwrap();
        assert_eq!(saved.name, "renamed");
        assert_eq!(saved.scope.databases, Some(vec!["shop".to_string()]));
        assert_eq!(saved.options.startup_script, "SET search_path = shop");
    }

    /// A new Data Source starts hidden from MCP. Saves the MCP value shown in the form as is
    #[tokio::test]
    async fn conn_form_mcp_default() {
        let mut h = Harness::new().await;
        let cfg_of = |h: &Harness| match &h.app.popup {
            Some(crate::app::Popup::ConnForm(f)) => App::form_config(f).unwrap().0,
            _ => panic!("no form"),
        };
        h.app.conn_form(None);
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"));
        let mut old = cfg_of(&h);
        old.options.mcp = None;
        old.options.confirm_changes = true;
        h.app.conn_form(Some(old.clone()));
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"), "what the form shows is saved");
        // Turning off Production keeps the off that was shown (doesn't open up by itself)
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "Production").unwrap().value = "off".into();
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"));
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "MCP").unwrap().value = "read".into();
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("read"));
        // A value core reads as off ("On", a typo) is shown and saved as off, never as write
        old.options.confirm_changes = false;
        old.options.mcp = Some("On".into());
        h.app.conn_form(Some(old));
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"));
    }

    /// Vacuum Progress only cancels: K (kill connection) doesn't even ask, and isn't in the hint line
    #[tokio::test]
    async fn dba_vacuum_progress_only_cancels() {
        use qurry_core::db::{ColumnInfo, QueryResult};
        let mut h = Harness::new().await;
        let res = QueryResult {
            columns: vec![ColumnInfo { name: "pid".into(), type_name: "INT4".into() }, ColumnInfo { name: "query".into(), type_name: "TEXT".into() }],
            rows: vec![vec![serde_json::json!(77), serde_json::json!("autovacuum: VACUUM public.big")]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
        };
        let info = qurry_core::dba::reports(qurry_core::store::DbKind::Postgres).into_iter().find(|r| r.id == "vacuum_progress");
        let d = crate::app::DbaTab { report: "vacuum_progress".into(), info, gone: None, grid: Some(crate::grid::Grid::new(res)), error: None, running: false, auto: true };
        h.app.push_tab("test", "postgres".into(), "Vacuum Progress".into(), TabKind::Dba(d));
        h.key("K");
        assert!(h.app.popup.is_none(), "must not offer to terminate a vacuum");
        let s = h.screen(160, 20);
        assert!(s.contains("cancel query") && !s.contains("terminate"), "{s}");
        h.key("c");
        assert!(matches!(&h.app.popup, Some(crate::app::Popup::Confirm { text, .. }) if text.contains("Cancel the query of session 77")));
    }

    /// ⌃K › Query History / Snippets: same history as the GUI. Picking one inserts it into the console. Save Query as Snippet asks for a name
    #[tokio::test]
    async fn history_and_snippets_insert_into_console() {
        let mut h = Harness::new().await;
        h.app.state.history.add("test", None, "select 42").unwrap();
        h.console_tab();
        h.app.run_action(Action::QueryHistory);
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no history") };
        assert_eq!(p.items[0].label, "select 42");
        h.key("Enter");
        assert_eq!(h.console_text(), "select 42");
        // Save as a snippet → insert from the list
        h.app.run_action(Action::SaveSnippet);
        let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no name input") };
        *input = tui_textarea::TextArea::from(["answer"]);
        h.key("Enter");
        assert_eq!(h.app.state.history.snippets(Some("test"))[0].name, "answer");
        h.key("Ctrl+a");
        h.app.run_action(Action::Snippets);
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no snippets") };
        assert_eq!(p.items[0].label, "answer");
    }

    /// ⌃K › Production Log: log of production operations. If there's undo SQL, open it in a new console (you run it yourself), otherwise just the reason
    #[tokio::test]
    async fn production_log_opens_undo_in_new_console() {
        let mut h = Harness::new().await;
        let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
            "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
        })).unwrap();
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.settle().await;
        let entry = |id: &str, sql: &str, undo: Option<&str>| qurry_core::audit::AuditEntry {
            id: id.into(), ts: 1, conn_id: "p".into(), conn_name: "PROD".into(), database: None, sql: sql.into(), rows: Some(2), error: None, in_tx: false,
            undo: undo.map(String::from), note: undo.is_none().then(|| "More than 1000 rows change: no undo kept".into()),
        };
        h.app.state.audit.add(entry("a", "delete from big", None)).unwrap();
        h.app.state.audit.add(entry("b", "update t set a = 1 where id = 2", Some("UPDATE t SET a = 0 WHERE id = 2;"))).unwrap();
        h.console_tab();
        let tabs = h.app.tabs.len();
        h.app.run_action(Action::ProductionLog);
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no log") };
        assert_eq!(p.items[0].label, "update t set a = 1 where id = 2");
        assert!(p.items[0].hint.starts_with("↶ undo · 2 row(s)") && p.items[0].hint.ends_with("PROD"), "{}", p.items[0].hint);
        h.key("Enter");
        assert_eq!(h.app.tabs.len(), tabs + 1);
        assert_eq!(h.console_text(), "UPDATE t SET a = 0 WHERE id = 2;");
        h.app.run_action(Action::ProductionLog);
        h.key("Down");
        h.key("Enter");
        assert_eq!(h.app.tabs.len(), tabs + 1);
        assert!(h.app.status.contains("no undo kept"), "{}", h.app.status);
    }

    /// ⌃K › Copy Related Rows: condition → child steps → destination (only other Data Sources of the same type). Generate Test Data asks for the row count
    #[tokio::test]
    async fn copy_rows_and_test_data_flows() {
        let mut h = Harness::new().await;
        for (id, name, kind) in [("test", "PROD", "postgres"), ("dev", "DEV", "postgres"), ("my", "MY", "mysql")] {
            let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
                "id": id, "name": name, "kind": kind, "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
            })).unwrap();
            h.app.state.store.save(&cfg).unwrap();
        }
        h.app.reload_conns();
        h.settle().await;
        h.table_tab("users", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(Action::GenerateTestData);
        let Some(crate::app::Popup::Input { title, .. }) = &h.app.popup else { panic!("no rows input") };
        assert!(title.contains("public.users"), "{title}");
        h.key("Esc");
        h.app.run_action(Action::CopyRelatedRows);
        let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no where input") };
        *input = tui_textarea::TextArea::from(["id = 1"]);
        h.key("Enter");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no depth") };
        assert_eq!(p.items.len(), 4);
        h.key("Enter");
        // Personal data: masked by default, below is as is
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no mask choice") };
        assert_eq!(p.items.len(), 2);
        h.key("Enter");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no target") };
        assert_eq!(p.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), vec!["DEV"]);
        let Some(crate::migrate::Flow::CopyTarget(b)) = &h.app.flow else { panic!("no flow") };
        assert_eq!((b.2.filter.as_str(), b.2.depth, b.2.mask), ("id = 1", 1, true));
    }

    /// ⌃K › Chart Result: result table as horizontal bars (popup)
    #[tokio::test]
    async fn chart_result_popup() {
        let mut h = Harness::new().await;
        h.table_tab("sales", &["region", "amount"], &[&["Seoul", "120"], &["Busan", "40"], &["Seoul", "30"]], &[]);
        h.app.run_action(Action::ChartResult);
        let screen = h.screen(100, 24);
        assert!(screen.contains("Seoul │") && screen.contains("█ 150") && screen.contains("Busan │"), "{screen}");
    }

    /// ⌃k › New Table: name → designer (a column · p PK · d delete), with CREATE TABLE shown right below
    #[tokio::test]
    async fn new_table_designer() {
        let mut h = Harness::new().await;
        let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
            "id": "p", "name": "PG", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
        })).unwrap();
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.settle().await;
        h.app.focus = crate::app::Focus::Tree;
        h.app.run_action(Action::NewTable);
        let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no name input") };
        *input = tui_textarea::TextArea::from(["orders"]);
        h.key("Enter");
        h.key("a");
        let Some(crate::app::Popup::Input { input, .. }) = &mut h.app.popup else { panic!("no column input") };
        *input = tui_textarea::TextArea::from(["total numeric(10,2) not null default 0"]);
        h.key("Enter");
        let d = h.app.designer.as_ref().unwrap();
        let sql = d.sql.as_ref().unwrap().join("\n");
        assert!(sql.contains("CREATE TABLE public.orders") && sql.contains("total numeric(10,2) NOT NULL DEFAULT 0") && sql.contains("PRIMARY KEY (id)"), "{sql}");
        assert!(h.screen(140, 40).contains("New Table"));
        // Turn off the PK on the id column and delete it
        h.key("Up");
        h.key("p");
        h.key("d");
        let sql = h.app.designer.as_ref().unwrap().sql.as_ref().unwrap().join("\n");
        assert!(!sql.contains("PRIMARY KEY") && !sql.contains(" id "), "{sql}");
        h.key("Esc");
        assert!(h.app.designer.is_none());
    }

    /// Production Data Source: statements that change data count the affected rows, show them, and ask (reads just run)
    #[tokio::test]
    async fn production_asks_before_changes() {
        let mut h = Harness::new().await;
        let mut cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({
            "id": "p", "name": "PROD", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "d"
        })).unwrap();
        cfg.options.confirm_changes = true;
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.settle().await;
        let id = h.app.restore_console("p", "d".into(), "console".into(), "delete from t using s where t.id = s.id").unwrap();
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
        h.key("F5");
        // Wait while counting (until the connection fails)
        for _ in 0..100 {
            h.settle().await;
            if h.app.popup.is_some() {
                break;
            }
        }
        let Some(crate::app::Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
        assert!(text.contains("Production data source \"PROD\"") && text.contains("delete from t using s") && text.contains("can't count this statement"), "{text}");
        // Saying no doesn't run it
        h.key("Esc");
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Console(c) if !c.running && c.output.is_empty()));
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

    /// Bottom hints: each focus's screen keys + common global keys. When narrow, screen keys are trimmed and global keys stay
    #[tokio::test]
    async fn status_bar_guides_by_focus() {
        use crate::keys::disp;
        let mut h = Harness::new().await;
        let last = |s: &str| s.lines().last().unwrap().to_string();
        // Explorer
        let s = last(&h.screen(220, 20));
        for want in ["Enter open", "F4 console", "s schemas", "/ filter", &disp("Ctrl+P search"), &disp("Ctrl+K actions")] {
            assert!(s.contains(want), "tree: {want:?} in {s}");
        }
        // Table data
        let id = h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
        h.app.focus = crate::app::Focus::Grid;
        let s = last(&h.screen(220, 20));
        for want in ["Enter edit", "a add", "d delete", &disp("Ctrl+S submit"), "w where", &disp("Ctrl+L console"), &disp("Ctrl+E tabs"), &disp("Ctrl+K actions")] {
            assert!(s.contains(want), "table: {want:?} in {s}");
        }
        // Narrow screen: global keys remain
        let s = last(&h.screen(90, 20));
        assert!(s.contains(&disp("Ctrl+K actions")), "narrow: {s}");
        // Console editor
        h.console_tab();
        let s = last(&h.screen(220, 20));
        for want in [&disp("F5 run"), &disp("Ctrl+Space complete"), &disp("⇧Tab result"), &disp("Ctrl+E tabs"), &disp("Ctrl+P search")] {
            assert!(s.contains(want), "editor: {want:?} in {s}");
        }
    }

    /// First screen and F1: if unsure, start with ⌃P
    #[tokio::test]
    async fn start_here_points_to_search() {
        use crate::keys::disp;
        let mut h = Harness::new().await;
        assert!(h.screen(140, 30).contains(&disp("Not sure? Press Ctrl+P")));
        h.key("F1");
        let s = h.screen(140, 40);
        let start = s.find("Start here").expect("F1 starts with Start here");
        assert!(start < s.find("Commands").unwrap());
        assert!(s.contains("Search everything"));
    }

    /// When Check for Updates finds a new version: show the changes since the current version, per version, and how to update
    #[tokio::test]
    async fn update_found_shows_changes() {
        use qurry_core::update::{Release, VersionNotes};
        let mut h = Harness::new().await;
        let r = Release {
            version: "9.0.1".into(), url: "u".into(), notes: String::new(), assets: vec![],
            history: vec![
                VersionNotes { version: "9.0.1".into(), changes: vec!["[fix] b (#2)".into()] },
                VersionNotes { version: "9.0.0".into(), changes: vec!["[feat] a (#1)".into()] },
                VersionNotes { version: "0.0.1".into(), changes: vec!["old".into()] },
            ],
        };
        h.app.show_update(r);
        assert_eq!(h.app.update.as_deref(), Some("9.0.1"));
        let s = h.screen(120, 30);
        for want in ["Qurry 9.0.1 is available", "• [fix] b (#2)", "• [feat] a (#1)", "Update Qurry & Restart"] {
            assert!(s.contains(want), "{want:?} in {s}");
        }
        assert!(!s.contains("old"), "{s}");
    }

    /// Rebinding a key updates on-screen hints too (console Output/result titles, table save hint, palette hints)
    #[tokio::test]
    async fn labels_follow_remapped_keys() {
        use crate::app::Action;
        use crate::keys::disp;
        let mut h = Harness::new().await;
        h.app.keys.bind(Action::ToggleOutput, "Alt+P".into(), false);
        h.app.keys.bind(Action::Submit, "Ctrl+G".into(), false);
        h.app.keys.bind(Action::Execute, "F9".into(), false);
        h.app.keys.bind(Action::Commit, "Alt+M".into(), false);
        h.console_tab();
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Alt+P: result")), "{s}");
        assert!(s.contains("(F9)"), "{s}");
        assert!(!s.contains(&disp("Alt+O")), "{s}");
        let id = h.table_tab("t", &["id", "name"], &[&["1", "a"]], &["id"]);
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
        if let crate::app::TabKind::Table(tt) = &mut h.app.tabs[i].kind {
            tt.grid.as_mut().unwrap().set_value(0, 1, "b".into());
        }
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Ctrl+G submit")), "{s}");
        assert!(!s.contains(&disp("Ctrl+S")), "{s}");
        // Palette names·hints use the same notation (Ctrl+Z → ⌃z)
        h.key("Ctrl+K");
        h.type_str("Revert Changes");
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Ctrl+Z (grid)")), "{s}");
    }

    /// Move Split Divider moves the divider even with only a top/bottom split (named "Left / Up")
    #[tokio::test]
    async fn split_divider_moves_in_top_bottom_split() {
        use crate::app::Action;
        let mut h = Harness::new().await;
        h.table_tab("a", &["id"], &[&["1"]], &["id"]);
        h.table_tab("b", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(Action::SplitDown);
        let pct = |h: &mut Harness| h.app.panes.pct_mut(&vec![]).map(|p| *p);
        let before = pct(&mut h).unwrap();
        h.app.run_action(Action::SplitGrow);
        assert_eq!(pct(&mut h), Some(before + 5));
        h.app.run_action(Action::SplitShrink);
        assert_eq!(pct(&mut h), Some(before));
    }

    /// In table data, Explain only shows a hint instead of reloading (Explain is for the query console)
    #[tokio::test]
    async fn explain_on_table_tab_does_not_reload() {
        let mut h = Harness::new().await;
        let id = h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::Explain);
        assert!(!h.table(id).running);
        assert!(h.app.status.contains("query console"), "{}", h.app.status);
    }

    /// Redis: the key list and value side have different status-line hints (D·t·R·: don't work on the value side)
    #[tokio::test]
    async fn redis_status_bar_follows_side() {
        use crate::app::{RedisTab, TabKind};
        let mut h = Harness::new().await;
        let r = RedisTab { pattern: "*".into(), keys: vec![], cursor: "0".into(), sel: 0, off: 0, open: Default::default(), detail: None, text: None, value: None, focus_value: false, scanning: false, want: None };
        h.app.push_tab("test", "db0".into(), "redis".into(), TabKind::Redis(r));
        h.app.focus = crate::app::Focus::Grid;
        let last = |h: &mut Harness| h.screen(220, 20).lines().last().unwrap().to_string();
        let s = last(&mut h);
        assert!(s.contains("D del key") && s.contains("/ pattern"), "{s}");
        if let Some(TabKind::Redis(r)) = h.app.tabs.last_mut().map(|t| &mut t.kind) {
            r.focus_value = true;
        }
        let s = last(&mut h);
        assert!(s.contains("e edit") && s.contains("x delete") && s.contains("Esc keys"), "{s}");
        assert!(!s.contains("D del key"), "{s}");
    }

    /// The new-version notice shows how to update right inside the TUI
    #[tokio::test]
    async fn update_badge_points_to_in_app_update() {
        let mut h = Harness::new().await;
        h.app.update = Some("9.9.9".into());
        let s = h.screen(160, 30);
        let want = format!("v9.9.9 available — {} › Update Qurry & Restart", crate::keys::disp("Ctrl+K"));
        assert!(s.contains(&want), "{s}");
    }

    /// qurry-tui built from source (cargo test too) isn't replaced; it says to rebuild
    #[tokio::test]
    async fn in_app_update_skips_source_builds() {
        let mut h = Harness::new().await;
        h.app.run_action(crate::app::Action::InstallUpdate);
        h.settle().await;
        assert!(h.app.status.contains("built from source"), "{}", h.app.status);
        assert!(!h.app.quit && !h.app.restart);
    }

    /// Clear the whole console: select all with ⌘A then ⌫, or ⌃K › Clear Console
    #[tokio::test]
    async fn console_select_all_and_clear() {
        let mut h = Harness::new().await;
        h.console_with("select 1;\nselect 2;", 1, 3);
        h.key("Cmd+a");
        h.key("Backspace");
        assert_eq!(h.console_text(), "");
        h.paste("select 3;");
        h.app.run_action(crate::app::Action::ClearConsole);
        assert_eq!(h.console_text(), "");
        // Unknown ⌘ combos don't insert text
        h.key("Cmd+x");
        assert_eq!(h.console_text(), "");
    }

    /// ⌘V into a table (terminal paste): fills from the current cell. Tabs·lines span multiple cells, missing rows become new rows
    #[tokio::test]
    async fn paste_into_table_cells() {
        let mut h = Harness::new().await;
        let id = h.table_tab("fee_rule", &["idx", "fee_type", "memo"], &[&["7", "PERCENT", ""], &["8", "FIXED", ""]], &["idx"]);
        h.key("Right");
        h.paste("FIXED");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!(g.value(0, 1).as_deref(), Some("FIXED"));
        // A comma within a line is still one cell value
        h.key("Right");
        h.paste("a, b");
        assert_eq!(h.table(id).grid.as_ref().unwrap().value(0, 2).as_deref(), Some("a, b"));
        // Multiple cells·lines (copied from a spreadsheet): extra columns are dropped, missing rows become new rows
        h.key("Left");
        h.key("Down");
        h.paste("RATE\tx\tdropped\r\nPERCENT\ty\r\n");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!((g.value(1, 1).as_deref(), g.value(1, 2).as_deref()), (Some("RATE"), Some("x")));
        assert_eq!(g.added, 1);
        assert_eq!((g.value(2, 1).as_deref(), g.value(2, 2).as_deref()), (Some("PERCENT"), Some("y")));
        // Same as the original value is not an edit
        h.key("Up");
        h.paste("PERCENT");
        assert!(!h.table(id).grid.as_ref().unwrap().edits.contains_key(&(0, 1)));
    }

    /// Text tabs (diagram·DDL): wheel scrolls, not past the end, g/G for start·end
    /// Console title bar while running: elapsed time · [■ Cancel ⌃c], clicking sends a cancel
    #[tokio::test]
    async fn running_console_shows_time_and_a_cancel_button() {
        let mut h = Harness::new().await;
        let id = h.console_tab();
        if let Some(TabKind::Console(c)) = h.app.tab_mut(id).map(|t| &mut t.kind) {
            c.running = true;
        }
        let screen = h.screen(140, 30);
        assert!(screen.contains("running 0.0 s · [■ Cancel"), "{screen}");
        let r = h.app.hits.cancel;
        assert!(r.width > 8, "{r:?}");
        let row = screen.lines().nth(r.y as usize).unwrap();
        assert!(row.chars().skip(r.x as usize).collect::<String>().starts_with("[■ Cancel"), "{row}");
        crate::input::on_event(&mut h.app, Event::Mouse(crossterm::event::MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: r.x + 2, row: r.y, modifiers: KeyModifiers::NONE }));
        h.settle().await;
        assert!(!h.app.status.is_empty() && h.app.status != "Executing…", "cancel was sent: {}", h.app.status);
        // The button goes away when finished
        if let Some(TabKind::Console(c)) = h.app.tab_mut(id).map(|t| &mut t.kind) {
            c.running = false;
        }
        h.screen(140, 30);
        assert_eq!(h.app.hits.cancel, ratatui::layout::Rect::default());
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