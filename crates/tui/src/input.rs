//! 키 입력 → 동작 (DataGrip 키맵을 터미널에 맞게)
use crate::app::{Action, App, Drag, DropTarget, FieldKind, Focus, Popup, SplitDir, TabKind};
use crate::tree::NodeKind;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

fn inside(r: Rect, x: u16, y: u16) -> bool {
    r.width > 0 && x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

/// 마우스 휠: 포인터 아래 영역을 스크롤
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
        // 글 탭 (DDL·다이어그램): 그 글을 스크롤
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

/// 가로 스크롤 (트랙패드 좌우·Shift+휠): 트리는 옆으로 밀고, 그리드는 열을 옮긴다
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

/// 포인터가 그리드 헤더나 가로 스크롤바 위인지
fn over_hbar(app: &mut App, x: u16, y: u16) -> bool {
    app.popup.is_none() && app.grid_mut().is_some_and(|g| inside(g.header, x, y) || inside(g.hbar, x, y))
}

/// 끌고 있는 탭을 (x, y) 에 놓으면 어디로 가는지
pub fn drop_target(app: &App, id: u64, x: u16, y: u16) -> Option<DropTarget> {
    let from = app.tab_index(id).map(|i| app.tabs[i].group)?;
    for &(g, bar, body) in &app.hits.groups {
        if inside(bar, x, y) {
            let before = app.hits.tabs.iter().find(|(_, x0, x1, ty)| *ty == y && x >= *x0 && x < *x1).map(|t| t.0);
            return Some(DropTarget::Group(g, before));
        }
        if inside(body, x, y) {
            // 칸의 오른쪽 / 아래쪽 가장자리에 놓으면 그 칸을 나눈다 (최대 4칸. 제 칸에 하나뿐인 탭은 나눠도 그대로라 빼고)
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
        // 탭: 누르면 고르고, 끌면 옮긴다
        if let Some(&(id, ..)) = app.hits.tabs.iter().find(|(_, x0, x1, ty)| *ty == y && x >= *x0 && x < *x1) {
            if let Some(i) = app.tab_index(id) {
                app.activate(i);
            }
            app.drag = Some(Drag::Tab { id, moved: false });
            app.drag_pos = (x, y);
            return;
        }
        // 상하 분할: 아래 칸 탭 바의 빈 곳도 경계선처럼 잡을 수 있다 (테두리 한 줄만으로는 맞히기 어렵다)
        if let Some(i) = app.hits.splits.iter().position(|s| inside(s.3, x, y)) {
            app.drag = Some(Drag::Split(i));
            return;
        }
        // 포커스 없는 칸을 누르면 그 칸으로
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

/// 화면 칸 → 콘솔 글자 위치 (줄, 칸). 마지막으로 그린 커서 칸을 기준으로 (편집기 밖이면 가장 가까운 줄·칸)
fn editor_pos(app: &App, x: u16, y: u16) -> Option<(usize, usize)> {
    use unicode_width::UnicodeWidthChar;
    let (cx, cy, row, col) = app.hits.caret?;
    let Some(TabKind::Console(c)) = app.tabs.get(app.active).map(|t| &t.kind) else { return None };
    let lines = c.editor.lines();
    let w = |ch: char| ch.width().unwrap_or(0) as i32;
    let r = (row as i32 + y as i32 - cy as i32).clamp(0, lines.len().saturating_sub(1) as i32) as usize;
    // 0번 칸이 그려졌을 화면 x (커서 줄 기준. 가로로 밀려 있어도 맞다)
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

/// 같은 곳을 0.4초 안에 두 번 누르면 더블클릭
fn double(app: &mut App, x: u16, y: u16) -> bool {
    let now = std::time::Instant::now();
    let dbl = matches!(app.hits.last_click, Some((t, px, py)) if now.duration_since(t).as_millis() < 400 && px == x && py == y);
    app.hits.last_click = if dbl { None } else { Some((now, x, y)) };
    dbl
}

fn click(app: &mut App, x: u16, y: u16) {
    let dbl = double(app, x, y);
    // 팝업: 팔레트 항목만 클릭으로 고른다 (그 밖은 닫기)
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
    // 트리
    if inside(app.hits.tree, x, y) {
        app.focus = Focus::Tree;
        let i = app.tree.offset + (y - app.hits.tree.y) as usize;
        // 누른 줄 하나만 복사한다
        let Some((depth, node)) = app.tree.visible().get(i).map(|(d, n)| (*d, (*n).clone())) else { return };
        app.tree.sel = i;
        // ▸ 위를 누르면 펼치기/접기
        let chev = app.hits.tree.x + (depth * 2) as u16;
        if node.expandable && (x == chev || x == chev + 1) {
            app.toggle_node(&node);
        } else if dbl {
            app.activate_node(&node, false);
        }
        return;
    }
    // 에디터: 누른 곳으로 커서, 끌면 선택
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
    // Redis 키 목록
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
                // 폴더를 누르면 펼치고 접는다
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
    // 그리드 (결과 / 테이블 / Redis 값)
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
        // 끌면 범위 선택
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
            // Shift+휠 = 가로 스크롤
            MouseEventKind::ScrollDown if m.modifiers.contains(KeyModifiers::SHIFT) => hscroll_at(app, m.column, m.row, 3),
            MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::SHIFT) => hscroll_at(app, m.column, m.row, -3),
            // ⌥+휠, 또는 그리드 헤더·스크롤바 위에서 휠 = 가로 스크롤 (가로 휠을 안 넘기는 터미널용)
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

/// ⌘V / 터미널 붙여넣기: 열린 입력칸(한 줄 입력·팔레트·Data Source 폼)이 있으면 거기로, 아니면 콘솔 에디터로
fn paste(app: &mut App, s: String) {
    // 터미널(xterm.js, tmux 등)은 줄바꿈을 \r 로 보내기도 한다. \r 이 편집기에 남으면 화면이 깨진다
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    // 한 줄 입력칸에는 줄바꿈을 공백으로
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
            // 테이블 데이터: 현재 칸부터 채운다 (⌘V)
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
    // ── 전역 명령 (키맵: keys.rs, 바꾸기: F1) ──
    if let Some(a) = app.keys.lookup(&k) {
        // 탐색기에서 실행 키 = 새로 고침
        if a == Action::Execute && app.focus == Focus::Tree {
            return app.run_action(Action::Refresh);
        }
        return app.run_action(a);
    }
    // Alt+1~9: 워크스페이스 (키맵에서 그 키를 다른 명령에 쓰면 그 명령이 먼저)
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

/// 커서 앞 단어로 후보를 거른다 (대소문자·따옴표 무시, 이미 다 친 것은 빼고, 8개까지)
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

/// 한 글자 명령(a 행 추가, d 삭제 …)을 쓰는 화면용: ⌘←/→ 는 Home/End (터미널이 ⌃A/⌃E 로 보내기도 한다), ⌘C 는 y(복사),
/// 그 밖에 ⌃/⌥/⌘ 이 같이 눌린 글자는 명령으로 받지 않는다 (⌘← 가 ⌃A 로 와서 행이 추가되던 문제)
fn plain(k: KeyEvent) -> Option<KeyEvent> {
    let m = k.modifiers;
    let to = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
    match k.code {
        KeyCode::Char('a') if m == KeyModifiers::CONTROL => to(KeyCode::Home),
        KeyCode::Char('e') if m == KeyModifiers::CONTROL => to(KeyCode::End),
        KeyCode::Left if m.contains(KeyModifiers::SUPER) => to(KeyCode::Home),
        KeyCode::Right if m.contains(KeyModifiers::SUPER) => to(KeyCode::End),
        // ⌘C = y (복사)
        KeyCode::Char('c') if m == KeyModifiers::SUPER => to(KeyCode::Char('y')),
        // ⌃Z = 편집 되돌리기
        KeyCode::Char('z') if m == KeyModifiers::CONTROL => Some(k),
        KeyCode::Char(_) if m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => None,
        _ => Some(k),
    }
}

fn tree_key(app: &mut App, k: KeyEvent) {
    let Some(k) = plain(k) else { return };
    let n = app.tree.visible().len();
    // 스피드 서치 입력 중
    if !app.tree.filter.is_empty() || k.code == KeyCode::Char('/') {
        match k.code {
            // 필터가 걸려 있으면 지금 필터를 채워서 연다 (비우고 Enter = 해제)
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
        // 고른 Data Source 를 다른 워크스페이스로
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

/// OSC 52 로 클립보드 복사 (대부분의 최신 터미널 지원)
pub fn copy(app: &mut App, text: &str) {
    use std::io::Write;
    // 이 컴퓨터에서 돌면 시스템 클립보드에 바로 (OSC 52 를 막거나 크기를 자르는 터미널이 많다). SSH 로 붙었으면 OSC 52 만 (원격 클립보드는 의미가 없다)
    let native = std::env::var_os("SSH_CONNECTION").is_none() && std::env::var_os("SSH_TTY").is_none() && native_copy(text);
    if !native {
        let b64 = base64_encode(text.as_bytes());
        let _ = write!(std::io::stdout(), "\x1b]52;c;{b64}\x07");
        let _ = std::io::stdout().flush();
    }
    let lines = text.lines().count();
    app.status = if lines > 1 { format!("Copied {lines} lines") } else { format!("Copied: {}", text.chars().take(60).collect::<String>()) };
}

/// pbcopy (macOS) · wl-copy / xclip (Linux). 성공하면 true
fn native_copy(text: &str) -> bool {
    use std::io::Write;
    // 테스트가 쓰는 사람의 클립보드를 덮어쓰지 않게
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
    // 자동완성 목록이 열려 있을 때
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
        // Esc: 탐색기로, Tab(빈 곳 아님)은 들여쓰기이므로 결과로 가는 건 Shift+Tab/F6
        KeyCode::Esc => {
            app.explorer = true;
            app.focus = Focus::Tree;
        }
        KeyCode::BackTab => app.focus = Focus::Grid,
        // ⌘A = 전부 고르기 (⌫ 로 지운다), ⌘C = 고른 글 복사. 그 밖의 ⌘+글자는 글자로 넣지 않는다
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
    // 단어나 '.' 을 치면 열고(GUI 처럼), 열린 채로 지우면 다시 거른다. 그 밖의 키는 닫는다
    // (macOS 는 Ctrl+Space 가 입력 소스 전환이라 타이핑으로 여는 게 기본)
    let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let typed = plain && matches!(k.code, KeyCode::Char(ch) if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
    if typed || (open && k.code == KeyCode::Backspace) {
        app.update_completion(false);
    } else if let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
        c.complete = None;
    }
}

fn grid_key(app: &mut App, k: KeyEvent) {
    // Esc: 고른 범위가 있으면 먼저 그것만 푼다
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
                // 움직이면 (사라진 세션 대신) 새로 고른 행
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
            // 화면에 맞춰 마지막 줄까지 (그릴 때 줄인다)
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

/// 그리드 이동
fn nav(app: &mut App, k: KeyEvent) {
    let Some(g) = app.grid_mut() else { return };
    let page = g.page as isize;
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    match k.code {
        // Shift+화살표 = 범위 선택 (DataGrip). 가로 스크롤은 트랙패드·Shift+휠·스크롤바
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
            // 범위를 골랐으면 TSV (시트에 칸으로 붙는다), 아니면 지금 칸
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
                // 선택 요소 편집
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
                        // 인덱스 위치를 표식으로 바꾼 뒤 LREM
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
        // 폴더: → / Enter 펼치기·접기. 키: 값으로
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
        // ←: 열린 폴더는 접고, 아니면 부모 폴더로
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
                // FLUSHALL·FLUSHDB·DEL 같은 지우는 명령은 한 번 더 묻는다
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
            // 새 키 녹화 중: Esc = 취소
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
            // 자동완성 목록이 열려 있으면: ↑↓ 고르기 · Tab/Enter 넣기 · Esc 닫기
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
                    // Tab·⌃Space = 지금 단어로 목록 열기 (한 줄 입력이라 Tab 글자는 넣지 않는다)
                    let force = k.code == KeyCode::Tab || (k.code == KeyCode::Char(' ') && k.modifiers.contains(KeyModifiers::CONTROL)) || k.code == KeyCode::Null;
                    if !force {
                        input.input(k);
                    }
                    complete = input_complete(&input, &words, force);
                    app.popup = Some(Popup::Input { title, input, on_ok, words, complete });
                }
            }
        }
        // 위에서 따로 받는다
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
                // SSH 처음 보는 서버: 지문을 확인한 뒤 신뢰
                KeyCode::Char('y') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    let v = |l: &str| form.fields.iter().find(|f| f.label == l).map(|f| f.value.trim().to_string()).unwrap_or_default();
                    let port = v("SSH port").parse().unwrap_or(22);
                    form.message = Some(match app.state.trust_ssh_host(v("SSH host"), port) {
                        Ok(m) => (true, format!("✓ {m} — press Ctrl+T to test again")),
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
                                        Err(e) if e.contains(qurry_core::tunnel::UNKNOWN_HOST) => (false, format!("✕ {e} Press Ctrl+Y to trust it.")),
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
                            // DB 종류를 바꾸면 기본 포트도
                            if f.label == "Type" {
                                const PORTS: &[(&str, &str)] = &[("postgres", "5432"), ("mysql", "3306"), ("redis", "6379"), ("sqlite", "0"), ("duckdb", "0"), ("mssql", "1433"), ("clickhouse", "8123"), ("mongodb", "27017"), ("oracle", "1521"), ("qdrant", "6333"), ("chroma", "8000"), ("milvus", "19530")];
                                let port = PORTS.iter().find(|x| x.0 == f.value).map(|x| x.1).unwrap_or("5432");
                                if let Some(p) = form.fields.iter_mut().find(|x| x.label == "Port") {
                                    // 손으로 바꾼 포트는 두고, 다른 종류의 기본 포트였으면 바꾼다
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
    match App::form_config(&form) {
        Ok((cfg, pw, ssh)) => {
            let st = app.state.clone();
            // 폼의 Workspace: 고친 Data Source 는 있던 곳에서 옮기고, 새 것은 고른 곳에 넣는다
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

/// 팔레트·키맵에 보이는 명령 이름
pub fn name_of(a: Action) -> &'static str {
    crate::app::ACTIONS.iter().find(|x| x.0 == a).map(|x| x.1).unwrap_or("?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_letters_are_not_commands() {
        let k = |c, m| KeyEvent::new(c, m);
        // ⌘← 가 ⌃A 로 와도 행 추가가 아니라 첫 열로
        assert_eq!(plain(k(KeyCode::Char('a'), KeyModifiers::CONTROL)).map(|x| x.code), Some(KeyCode::Home));
        assert_eq!(plain(k(KeyCode::Right, KeyModifiers::SUPER)).map(|x| x.code), Some(KeyCode::End));
        assert!(plain(k(KeyCode::Char('d'), KeyModifiers::ALT)).is_none());
        assert!(plain(k(KeyCode::Char('n'), KeyModifiers::SUPER)).is_none());
        assert_eq!(plain(k(KeyCode::Char('D'), KeyModifiers::SHIFT)).map(|x| x.code), Some(KeyCode::Char('D')));
        assert_eq!(plain(k(KeyCode::Char('z'), KeyModifiers::CONTROL)).map(|x| x.code), Some(KeyCode::Char('z')));
    }

    // ───────── 키를 넣어 확인 (testkit) ─────────
    use crate::testkit::Harness;

    /// ⌘← 가 ⌃A 로 와도 행을 추가하지 않는다 (a 는 행 추가)
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

    /// WHERE 입력에서 컬럼·키워드 자동완성
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

    /// ⌘C 는 y 처럼 셀 값을 복사한다 (터미널이 ⌘C 를 넘겨줄 때)
    #[tokio::test]
    async fn cmd_c_copies_cell() {
        let mut h = Harness::new().await;
        h.table_tab("fee_rule", &["idx", "fee_type"], &[&["7", "PERCENT"]], &["idx"]);
        h.key("Right");
        h.key("Cmd+c");
        assert_eq!(h.app.status, "Copied: PERCENT");
    }

    /// Recent Tabs 에는 열린 탭이 다 나온다 (되살린 뒤 아직 안 본 탭도). 최근에 본 탭이 먼저
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

    /// 터미널은 붙여넣기 줄바꿈을 \r 로 보내기도 한다 (xterm.js, tmux): 줄은 그대로, \r 은 남지 않는다
    #[tokio::test]
    async fn paste_into_console_keeps_lines() {
        let mut h = Harness::new().await;
        h.console_tab();
        h.paste("BEGIN;\r\r-- 1. 스키마\rGRANT USAGE\r\n  ON x;\rCOMMIT;\r");
        assert_eq!(h.console_text(), "BEGIN;\n\n-- 1. 스키마\nGRANT USAGE\n  ON x;\nCOMMIT;\n");
    }

    /// ⌃Enter: 문장이 여럿이면 무엇을 실행할지 묻는다 (DataGrip). 고른 것부터 실행한다
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
        // 접속이 없어 첫 문장에서 멈춘다: 고른 범위의 첫 문장이 GRANT b
        let out = h.console_output();
        assert!(out.iter().any(|l| l.ends_with("> GRANT b TO x")), "{out:?}");
        assert!(!out.iter().any(|l| l.contains("BEGIN")), "{out:?}");
    }

    /// 실행 설정을 Smallest 로 바꾸면 묻지 않고 커서 문장만
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

    /// Manage Shown Schemas: Space 로 고르고 Enter 로 저장 (Data Source 의 scope, GUI 와 공유). Esc 는 저장하지 않는다
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
        // 줄: All databases · Default database · app · postgres · shop
        h.key("Space"); // All databases 끄기
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
        // 트리도 바로 새 범위를 쓴다
        assert_eq!(h.app.conn("c1").unwrap().config.scope.databases, Some(vec!["shop".to_string()]));

        // 그 사이 다른 곳(GUI·MCP)에서 바꾼 설정을 예전 값으로 되돌리지 않는다 (읽기 전용을 켜 둔 것 등)
        let mut changed = h.app.state.store.get("c1").unwrap().unwrap();
        changed.read_only = true;
        changed.host = "db.internal".into();
        h.app.state.store.save(&changed).unwrap();
        h.app.open_scope("c1", dbs(), None);
        h.key("Down");
        h.key("Space"); // Default database 켜기
        h.key("Enter");
        let saved = h.app.state.store.get("c1").unwrap().unwrap();
        assert!(saved.read_only, "read_only reverted");
        assert_eq!(saved.host, "db.internal");
        assert_eq!(saved.scope.databases, Some(vec!["shop".to_string(), "@default".to_string()]));
    }

    /// 좌우로 나눈 화면: 두 칸의 탭 바 사이에 구분선이 있다 (이어진 한 줄로 보이지 않게)
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

    /// 탭이 탭 바보다 많으면 지금 탭이 보이도록 민다 (뒤쪽 탭이 잘려 안 보이지 않게). 클릭도 밀린 자리에 맞는다
    #[tokio::test]
    async fn tab_bar_scrolls_to_the_active_tab() {
        let mut h = Harness::new().await;
        for i in 0..8 {
            h.table_tab(&format!("long_table_name_{i}"), &["id"], &[&["1"]], &["id"]);
        }
        let s = h.screen(80, 20);
        let bar = s.lines().find(|l| l.contains("long_table_name_7")).unwrap_or_else(|| panic!("{s}"));
        assert!(!bar.contains("long_table_name_0"), "{bar}");
        // 첫 탭으로 가면 다시 앞쪽이 보인다
        h.key("Alt+Right");
        let s = h.screen(80, 20);
        let (y, bar) = s.lines().enumerate().find(|(_, l)| l.contains("long_table_name_0")).unwrap_or_else(|| panic!("{s}"));
        assert!(!bar.contains("long_table_name_7"), "{bar}");
        // 보이는 탭을 누르면 그 탭으로
        let x = bar[..bar.find("long_table_name_1").unwrap()].chars().count() as u16;
        h.mouse("down", x, y as u16);
        h.mouse("up", x, y as u16);
        assert_eq!(h.app.tabs[h.app.active].title, "long_table_name_1");
    }

    /// 나눈 화면에서 탭을 닫으면 같은 칸의 옆 탭으로 (되살린 탭처럼 아직 안 본 탭만 남아도 첫 칸으로 튀지 않게)
    #[tokio::test]
    async fn closing_a_tab_stays_in_its_pane() {
        let mut h = Harness::new().await;
        h.table_tab("left", &["id"], &[&["1"]], &["id"]);
        let right_a = h.table_tab("right_a", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::SplitRight);
        h.table_tab("right_b", &["id"], &[&["1"]], &["id"]);
        h.table_tab("right_c", &["id"], &[&["1"]], &["id"]);
        // 되살린 탭은 최근 목록에 없다
        h.app.mru.retain(|x| *x != right_a);
        h.key("Ctrl+w");
        assert_eq!(h.app.tabs[h.app.active].title, "right_b");
        h.key("Ctrl+w");
        assert_eq!(h.app.tabs[h.app.active].title, "right_a");
    }

    /// 내보내기 (x): 형식 → 파일/클립보드 → 경로 (확장자는 형식대로)
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

    /// 가져오기: 파일을 읽어 같은 이름 컬럼에 짝짓고, 무엇이 들어가는지 보여 주고 묻는다
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

    /// Redis 키 브라우저도 ":" 폴더 (Medis): → 펼치기, ← 접기 / 부모 폴더로, ↓↑ 는 보이는 줄
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
        // ← : 키에서는 부모 폴더로, 열린 폴더는 접는다
        h.key("Left");
        assert_eq!(sel(&h), 1);
        h.key("Left");
        assert_eq!(rows(&h), ["▾ dev 2", "  ▸ sessions 2", "▸ prod 1", "token"]);
        // 화면에도 폴더와 개수
        let s = h.screen(120, 20);
        assert!(s.contains("▸ ▤ sessions 2") && s.contains("token"), "{s}");
    }

    /// Y: 지금 열 전체를 IN 목록으로 복사 (결과의 IP 들을 다음 쿼리 WHERE ip IN 에 바로)
    #[tokio::test]
    async fn copy_column_as_in_list() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["1.2.3.4", "1"], &["5.6.7.8", "2"], &["1.2.3.4", "3"]], &[]);
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('1.2.3.4', '5.6.7.8')");
        // ⌃K 에서도
        assert!(crate::app::ACTIONS.iter().any(|a| a.0 == Action::CopyInList));
    }

    /// Ctrl+F: 편집기 찾기/바꾸기 바 (DataGrip). Alt+X 정규식, Tab 바꾸기 칸, Alt+A 전부 바꾸기 ($1 그룹), Esc 닫기
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

    /// 찾기: 칠 때마다 커서 뒤 첫 결과를 고르고, Enter 는 다음. 바꾸기 칸에서 Enter 는 지금 것만 바꾸고 다음으로
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
        // 대소문자 구분(Alt+C)이면 ID 는 안 찾는다
        h.key("Esc");
        h.console_with("id, ID", 0, 0);
        h.key("Ctrl+f");
        h.type_str("ID");
        h.key("Alt+c");
        assert!(h.screen(120, 30).contains("1/1"));
    }

    /// 그리드 범위 선택: Shift+↑↓←→ (또는 끌기), y = TSV, Y = 고른 칸만 IN 목록, Esc = 선택 풀기
    #[tokio::test]
    async fn grid_range_select_and_copy() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["1.2.3.4", "1"], &["5.6.7.8", "2"], &["9.9.9.9", "3"]], &[]);
        h.key("Shift+Down");
        h.key("y");
        assert_eq!(h.app.status, "Copied 2 rows × 1 column");
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('1.2.3.4', '5.6.7.8')");
        // Esc 는 먼저 선택만 푼다
        h.key("Esc");
        assert_eq!(h.app.focus, crate::app::Focus::Grid);
        h.key("Y");
        assert!(h.app.status.starts_with("Copied 3 values"), "{}", h.app.status);
        // 마우스로 끌어도 고른다
        let s = h.screen(120, 20);
        let (y0, line) = s.lines().enumerate().find(|(_, l)| l.contains("1.2.3.4")).unwrap();
        let x = line[..line.find("1.2.3.4").unwrap()].chars().count() as u16;
        h.mouse("down", x, y0 as u16);
        h.mouse("drag", x, y0 as u16 + 2);
        h.mouse("up", x, y0 as u16 + 2);
        h.key("Y");
        assert_eq!(h.app.status, "Copied 3 values as an IN list: ('1.2.3.4', '5.6.7.8', '9.9.9.9')");
    }

    /// ⌃K › Database Admin…: 리포트를 고르면 DBA 탭 (같은 접속 대상이면 그 탭에서 바꾼다)
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
        // 접속이 없으니 오류가 보인다
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Dba(d) if d.error.is_some()));
        h.app.run_action(Action::DbaReports);
        h.type_str("Table Sizes");
        h.key("Enter");
        assert_eq!(report(&h), "table_sizes");
        assert_eq!(h.app.tabs.len(), n);
    }

    /// 자동 새로고침으로 행 순서가 바뀌어도 고른 세션(pid)을 따라간다 (다른 세션을 끊지 않게)
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
        // 고른 세션이 사라지면 고르지 않는다 (엉뚱한 세션을 끊지 않게)
        h.app.dba_apply(id, Ok(res(&[30, 10])));
        let TabKind::Dba(d) = &h.app.tabs[h.app.active].kind else { panic!() };
        assert!(d.grid.as_ref().unwrap().result.rows.len() == 2);
        h.key("K");
        assert!(h.app.popup.is_none(), "must not offer to kill a session that is gone");
    }

    /// ⌃K › Query History / Snippets: GUI 와 같은 기록. 고르면 콘솔에 넣는다. Save Query as Snippet 은 이름을 묻는다
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
        // 스니펫으로 저장 → 목록에서 넣기
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

    /// Production Data Source: 데이터를 바꾸는 문장은 바뀔 행 수를 세어 보여 주고 묻는다 (읽기는 그냥 돈다)
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
        // 세는 동안 (접속 실패까지) 기다린다
        for _ in 0..100 {
            h.settle().await;
            if h.app.popup.is_some() {
                break;
            }
        }
        let Some(crate::app::Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
        assert!(text.contains("Production data source \"PROD\"") && text.contains("delete from t using s") && text.contains("can't count this statement"), "{text}");
        // 아니라고 하면 실행하지 않는다
        h.key("Esc");
        assert!(matches!(&h.app.tabs[h.app.active].kind, TabKind::Console(c) if !c.running && c.output.is_empty()));
    }

    /// 콘솔 마우스: 누른 곳으로 커서, 끌면 선택 (선택한 것만 실행), 다시 누르면 선택이 풀린다
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
        // ⌘C 는 고른 글을 클립보드로 (터미널의 끌어서 복사는 앱이 마우스를 잡아서 안 된다)
        h.key("Cmd+c");
        assert_eq!(h.app.status, "Copied: public");
        // 윗줄 "1" 을 누르면 커서가 거기로, 선택은 풀린다
        h.screen(120, 30);
        h.mouse("down", x - 7, y - 1);
        h.mouse("up", x - 7, y - 1);
        assert_eq!(h.console_cursor(), ((0, 7), None));
        // 줄 끝 너머를 누르면 그 줄 끝
        h.mouse("down", x + 40, y);
        h.mouse("up", x + 40, y);
        assert_eq!(h.console_cursor(), ((1, 26), None));
    }

    /// 하단 안내: 포커스마다 그 화면 키 + 자주 쓰는 전역 키. 좁으면 화면 키를 줄이고 전역 키는 남긴다
    #[tokio::test]
    async fn status_bar_guides_by_focus() {
        use crate::keys::disp;
        let mut h = Harness::new().await;
        let last = |s: &str| s.lines().last().unwrap().to_string();
        // 탐색기
        let s = last(&h.screen(220, 20));
        for want in ["Enter open", "F4 console", "s schemas", "/ filter", &disp("Ctrl+P search"), &disp("Ctrl+K actions")] {
            assert!(s.contains(want), "tree: {want:?} in {s}");
        }
        // 표 데이터
        let id = h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
        h.app.focus = crate::app::Focus::Grid;
        let s = last(&h.screen(220, 20));
        for want in ["Enter edit", "a add", "d delete", &disp("Ctrl+S submit"), "w where", &disp("Ctrl+L console"), &disp("Ctrl+E tabs"), &disp("Ctrl+K actions")] {
            assert!(s.contains(want), "table: {want:?} in {s}");
        }
        // 좁은 화면: 전역 키는 남는다
        let s = last(&h.screen(90, 20));
        assert!(s.contains(&disp("Ctrl+K actions")), "narrow: {s}");
        // 콘솔 편집기
        h.console_tab();
        let s = last(&h.screen(220, 20));
        for want in [&disp("F5 run"), &disp("Ctrl+Space complete"), "⇧Tab result", &disp("Ctrl+E tabs"), &disp("Ctrl+P search")] {
            assert!(s.contains(want), "editor: {want:?} in {s}");
        }
    }

    /// 처음 화면과 F1: 모르겠으면 ⌃P 부터
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

    /// Check for Updates 로 새 버전을 찾으면: 지금 버전 이후 바뀐 것을 버전마다 보여 주고 업데이트 방법을 알린다
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

    /// 키를 바꾸면 화면 안내도 바뀐 키로 (콘솔 Output/결과 제목, 표 저장 안내, 팔레트 힌트)
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
        // 팔레트 이름·힌트도 같은 표기 (Ctrl+Z → ⌃z)
        h.key("Ctrl+K");
        h.type_str("Revert Changes");
        let s = h.screen(160, 30);
        assert!(s.contains(&disp("Ctrl+Z (grid)")), "{s}");
    }

    /// 위아래로만 나눴을 때도 Move Split Divider 가 경계선을 옮긴다 (이름이 "Left / Up")
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

    /// 표 데이터에서 Explain 은 다시 읽기가 아니라 안내만 (Explain 은 쿼리 콘솔에서)
    #[tokio::test]
    async fn explain_on_table_tab_does_not_reload() {
        let mut h = Harness::new().await;
        let id = h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        h.app.run_action(crate::app::Action::Explain);
        assert!(!h.table(id).running);
        assert!(h.app.status.contains("query console"), "{}", h.app.status);
    }

    /// Redis: 키 목록과 값 쪽 상태줄 안내가 다르다 (값 쪽에선 D·t·R·: 가 안 된다)
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

    /// 새 버전 알림은 TUI 안에서 바로 업데이트하는 길을 알려준다
    #[tokio::test]
    async fn update_badge_points_to_in_app_update() {
        let mut h = Harness::new().await;
        h.app.update = Some("9.9.9".into());
        let s = h.screen(160, 30);
        let want = format!("v9.9.9 available — {} › Update Qurry & Restart", crate::keys::disp("Ctrl+K"));
        assert!(s.contains(&want), "{s}");
    }

    /// 소스에서 빌드한 qurry-tui(cargo test 도 그렇다)는 바꿔 넣지 않고 다시 빌드하라고 한다
    #[tokio::test]
    async fn in_app_update_skips_source_builds() {
        let mut h = Harness::new().await;
        h.app.run_action(crate::app::Action::InstallUpdate);
        h.settle().await;
        assert!(h.app.status.contains("built from source"), "{}", h.app.status);
        assert!(!h.app.quit && !h.app.restart);
    }

    /// 콘솔 전체 지우기: ⌘A 로 전부 고르고 ⌫, 또는 ⌃K › Clear Console
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
        // 모르는 ⌘ 조합은 글자를 넣지 않는다
        h.key("Cmd+x");
        assert_eq!(h.console_text(), "");
    }

    /// 표에 ⌘V (터미널 붙여넣기): 현재 칸부터 채운다. 탭·줄로 여러 칸, 모자라는 줄은 새 행
    #[tokio::test]
    async fn paste_into_table_cells() {
        let mut h = Harness::new().await;
        let id = h.table_tab("fee_rule", &["idx", "fee_type", "memo"], &[&["7", "PERCENT", ""], &["8", "FIXED", ""]], &["idx"]);
        h.key("Right");
        h.paste("FIXED");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!(g.value(0, 1).as_deref(), Some("FIXED"));
        // 한 줄에 쉼표가 있어도 한 칸 값
        h.key("Right");
        h.paste("a, b");
        assert_eq!(h.table(id).grid.as_ref().unwrap().value(0, 2).as_deref(), Some("a, b"));
        // 여러 칸·여러 줄 (스프레드시트에서 복사): 넘치는 열은 버리고 모자라는 줄은 새 행
        h.key("Left");
        h.key("Down");
        h.paste("RATE\tx\tdropped\r\nPERCENT\ty\r\n");
        let g = h.table(id).grid.as_ref().unwrap();
        assert_eq!((g.value(1, 1).as_deref(), g.value(1, 2).as_deref()), (Some("RATE"), Some("x")));
        assert_eq!(g.added, 1);
        assert_eq!((g.value(2, 1).as_deref(), g.value(2, 2).as_deref()), (Some("PERCENT"), Some("y")));
        // 원래 값과 같으면 편집이 아니다
        h.key("Up");
        h.paste("PERCENT");
        assert!(!h.table(id).grid.as_ref().unwrap().edits.contains_key(&(0, 1)));
    }

    /// 글 탭 (다이어그램·DDL): 휠로 스크롤, 끝을 넘지 않고, g/G 로 처음·끝
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