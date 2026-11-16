//! Screen drawing
use crate::keys::disp;
use crate::app::{App, Drag, DropTarget, FieldKind, Focus, Popup, SERVER_WARNING, SplitDir, TabKind, WARNING};
use crate::tree::NodeKind;
use qurry_core::keymap::Scope;
use qurry_core::statusbar::{self, Segment};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use serde_json::Value;

pub use crate::kit::*;

mod tree;
mod tabs;
mod editor;
mod completion;
mod redis;
mod status_items;
mod popup;
use self::tree::{draw_tree, draw_workspaces, object_glyph};
use self::tabs::{draw_tabs, tab_glyph};
use self::editor::{find_caret, draw_sql_colors, draw_marks, draw_names, draw_unbalanced, draw_hint, draw_preview, draw_run_error, CARET_MARK, draw_result_strip, value_split};
use self::completion::draw_completion;
pub use self::redis::{format_reply, format_typed, type_color};
use self::status_items::draw_status;
use self::popup::{draw_popup, fit_rows};

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
    app.color.apply(f.buffer_mut(), &crate::theme::Look::of(&app.themes, app.auto_light));
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
    app.hits.run = Rect::default();
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

/// Body of one tab. focused = the focused pane (mouse hit areas are recorded only for this pane)
fn draw_pane(f: &mut Frame, app: &mut App, idx: usize, body: Rect, focused: bool) {
    // Unfocused panes are drawn like Tree focus (dimmed border·cursor)
    let focus = if focused { app.focus } else { Focus::Tree };
    let color = app.color;
    let align = app.align.clone();
    let ro = app.tabs.get(idx).and_then(|t| app.conn(&t.conn)).map(|c| c.config.read_only).unwrap_or(false);
    let kind = app.tabs.get(idx).map(|t| app.kind_of(&t.conn)).unwrap_or(qurry_core::store::DbKind::Postgres);
    let pct = app.console_pct;
    // Rebindable commands are shown with their first key in the current keymap
    let k = |a: crate::app::Action| app.keys.keys(a).first().map(|s| disp(s)).unwrap_or_else(|| "—".into());
    use crate::app::Action as A;
    let (k_out, k_run, k_commit, k_rollback, k_cancel, k_submit) = (k(A::ToggleOutput), k(A::Execute), k(A::Commit), k(A::Rollback), k(A::Cancel), k(A::Submit));
    let k_fix = k(A::QuickFix);
    // The value panel beside result and table grids, and its keys (v toggles it, ⌃w closes it from the grid)
    let value_panel = app.value_panel;
    let value_keys = app.keys.slot_keys(crate::keys::Slot { action: A::ValueEditor, pane: Scope::Grid }).first().map(|k| crate::keys::pane_disp(k));
    let value_keys = format!(" {} / {} close ", value_keys.unwrap_or_else(|| "—".into()), k(A::CloseTab));
    let finding = focused && matches!(app.popup, Some(Popup::Find(_)));
    // A run or a load-more in flight: the console's [▶ Run] is [■ Cancel] (the status line says what and for how long). The TUI has no
    // grid toolbar (the GUI's ↻ / ■), so a console result's load-more is this button's too
    let busy = app.tabs[idx].busy().is_some();
    // The Execute chooser's highlighted option: the console text it would run
    let preview = match &app.popup {
        Some(Popup::Palette(p)) if focused => match App::palette_matches(p).get(p.sel).map(|i| &i.action) {
            Some(crate::app::PAction::Run { spans, .. }) => spans.clone(),
            _ => vec![],
        },
        _ => vec![],
    };
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
            let button = if busy { format!("[■ Cancel {k_cancel}]") } else { format!("[▶ Run {k_run}]") };
            let title = format!(
                " {db} · Tx: {}{} · {button}{} ",
                if c.tx_manual { "Manual" } else { "Auto" },
                if c.in_tx { format!(" (pending — {k_commit} commit / {k_rollback} rollback)") } else { String::new() },
                if ro { " · read-only" } else { "" }
            );
            let editing = focus == Focus::Editor;
            if !editing {
                c.complete = None;
            }
            // The button's spot (the title starts right after the top-left corner)
            let spot = {
                use unicode_width::UnicodeWidthStr;
                let at = title.find(&button).unwrap_or_default();
                let x = ed.x + 1 + title[..at].width() as u16;
                Rect { x, y: ed.y, width: (button.width() as u16).min(ed.right().saturating_sub(x)), height: 1 }
            };
            // Only the focused pane's: a click acts on the active tab
            if focused {
                *(if busy { &mut app.hits.cancel } else { &mut app.hits.run }) = spot;
            }
            // Unclosed quotes and brackets (the GUI console's markers); on the caret's line the bottom border says what is open
            let text = c.editor.lines().join("\n");
            let open = qurry_shared::pairs::unbalanced(&text, kind);
            let at_caret = open.iter().find(|u| crate::input::pairs::position(&text, u.start).0 == c.editor.cursor().0);
            let block = panel(title, editing);
            c.editor.set_block(match at_caret {
                Some(u) => block.title_bottom(Line::styled(format!(" {} ", u.message), Style::default().fg(DANGER)).right_aligned()),
                None => block,
            });
            c.editor.set_line_number_style(Style::default().fg(DIM));
            // Put an invisible mark (underline color) on the cursor cell: found after drawing and used to place autocomplete·compute mouse positions
            let mark = Style::default().underline_color(CARET_MARK);
            c.editor.set_cursor_style(if editing { mark.add_modifier(Modifier::REVERSED) } else { mark });
            f.render_widget(&c.editor, ed);
            // tui-textarea doesn't expose its scroll position → use the cursor cell just drawn
            let drawn = find_caret(f, ed);
            c.colors.update(&text, kind);
            draw_sql_colors(f, c, ed, drawn);
            draw_marks(f, c, ed, drawn);
            draw_names(f, c, ed, drawn);
            draw_unbalanced(f, c, ed, drawn, &text, &open);
            draw_preview(f, c, ed, drawn, &preview);
            draw_run_error(f, c, ed, drawn);
            let caret = if editing && c.complete.is_some() { drawn } else { None };
            if focused {
                app.hits.caret = drawn.map(|(x, y)| (x, y, c.editor.cursor().0, c.editor.cursor().1));
                app.hits.editor = ed;
                app.hits.find = find_bar;
                app.hits.results = res;
                app.hits.console = body;
                app.hits.divider = Rect { x: res.x, y: res.y, width: res.width, height: 1 };
            }
            // The panel's title is the result tab strip: Output, then one tab per statement (click or ⌥. / ⌥, to pick)
            let tabs_hit = if c.results.grid().is_none() {
                let lines: Vec<Line> = c
                    .output
                    .iter()
                    .rev()
                    .take(res.height as usize)
                    .rev()
                    .map(|(e, s)| Line::styled(s.clone(), Style::default().fg(if *e { DANGER } else if s.starts_with(WARNING) || s.starts_with(SERVER_WARNING) { WARN } else if s.contains("> ") { ACCENT } else { Color::Gray })))
                    .collect();
                let block = panel("", focus == Focus::Grid);
                let inner = block.inner(res);
                let p = Paragraph::new(if lines.is_empty() { vec![Line::styled(format!("Execute a statement ({k_run}) to see the output."), Style::default().fg(DIM))] } else { lines })
                    .wrap(Wrap { trim: false });
                // A long line wraps onto several rows: scroll so the newest line ends on the bottom row
                let rows = p.line_count(inner.width).min(u16::MAX as usize) as u16;
                f.render_widget(p.scroll((rows.saturating_sub(inner.height), 0)).block(block), res);
                draw_result_strip(f, res, &c.results, format!(" ({k_out}: result) "))
            } else {
                let g = c.results.grid().unwrap();
                let pend = if g.dirty() { format!(" · {} pending ({k_submit} submit, {} revert)", g.pending(), disp("⌃z")) } else { String::new() };
                let tail = format!(" — {}{} rows{pend} ({k_out}: output) ", g.rows(), if g.result.truncated { "+" } else { "" });
                let (res, side) = value_split(res, value_panel);
                let g = c.results.grid_mut().unwrap();
                g.render(f, res, String::new(), focus == Focus::Grid, color, &align);
                if let Some(side) = side {
                    g.render_value_panel(f, side, value_keys);
                }
                draw_result_strip(f, res, &c.results, tail)
            };
            if focused {
                app.hits.result_tabs = tabs_hit;
            }
            if let (Some((x, y)), Some(cmp)) = (caret, &c.complete) {
                draw_completion(f, cmp, x, y);
            } else if let Some((x, y)) = drawn.filter(|_| editing) {
                draw_hint(f, c, ed, x, y, &k_fix);
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
                let pend = if g.dirty() { format!(" · {} pending ({k_submit} submit, {} revert)", g.pending(), disp("⌃z")) } else { String::new() };
                let rows = if tt.error.is_some() && !g.dirty() { "query failed — no rows".to_string() } else { format!("{}{} rows", g.rows(), if g.result.truncated { "+" } else { "" }) };
                let t = format!(" {}.{} — {rows}{}{}{} ", tt.schema, tt.table, if tt.pk.is_empty() { " · no PK (read-only)" } else { "" }, if ro { " · read-only" } else { "" }, pend);
                let (grid, side) = value_split(grid, value_panel);
                g.render(f, grid, t, focus == Focus::Grid, color, &align);
                if let Some(side) = side {
                    g.render_value_panel(f, side, value_keys);
                }
            } else {
                // Nothing while nothing loads: a failed load says why on the status line, a cancelled one how to load again
                let msg = if tt.error.is_some() || !tt.running { "" } else { "Loading…" };
                f.render_widget(Paragraph::new(msg).block(panel(format!(" {}.{} ", tt.schema, tt.table), focused)), grid);
            }
        }
        TabKind::Text(t) => {
            // A table's DDL: v switches to the Structure view (sections as tables)
            let view = match (t.has_structure(), t.structure_view) {
                (true, true) => "Structure · v SQL · ",
                (true, false) => "v Structure · ",
                (false, _) => "",
            };
            let text = crate::structure::body(t, body.width.saturating_sub(2) as usize);
            // Don't scroll past the end (so the last line sits at the bottom)
            let lines = text.lines.len() as u16;
            let scroll = t.scroll.min(lines.saturating_sub(body.height.saturating_sub(2)));
            let copies = if t.has_structure() { "y copies the SQL" } else { "y copies all" };
            let p = Paragraph::new(text).scroll((scroll, 0)).block(panel(format!(" {title} — {view}{lines} lines · ↑↓ PgUp/PgDn scroll · {copies} "), focus != Focus::Tree));
            f.render_widget(p, body);
            t.scroll = scroll;
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
            let title = format!(" {} — {} rows ", crate::dba::db_label(&db), d.grid.as_ref().map(|g| g.rows()).unwrap_or(0));
            // Where the grid is drawn (mouse hits): below the message when a refresh failed
            let mut area = grid;
            match (&mut d.grid, &d.error) {
                (None, Some(e)) => f.render_widget(Paragraph::new(e.as_str()).style(Style::default().fg(DANGER)).wrap(Wrap { trim: false }).block(panel(" Error ", focus == Focus::Grid)), grid),
                (Some(g), Some(e)) => {
                    let [msg, rest] = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(grid);
                    let age = d.loaded.map(|t| format!(" from {} s ago", t.elapsed().as_secs())).unwrap_or_default();
                    let text = format!(" Refresh failed, showing data{age}: {}", e.lines().next().unwrap_or(""));
                    f.render_widget(Paragraph::new(text).style(Style::default().fg(WARN)).wrap(Wrap { trim: false }), msg);
                    g.render(f, rest, title, focus == Focus::Grid, color, &align);
                    area = rest;
                }
                (Some(g), None) => g.render(f, grid, title, focus == Focus::Grid, color, &align),
                (None, None) => f.render_widget(Paragraph::new("Loading…").block(panel(title, focus == Focus::Grid)), grid),
            }
            if focused {
                app.hits.results = area;
            }
        }
        TabKind::Redis(r) => {
            let [left, right] = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(body);
            let kf = focus == Focus::Grid && !r.focus_value;
            // " db0 · 500 keys · prod:user:* — 3 " : the database (d switches it), the filter (/), the keys scanned so far
            let db = r.dbs.iter().find(|d| d.name == tab.db).map(|d| qurry_shared::redis_cmd::db_label(&d.name, d.keys)).unwrap_or_else(|| tab.db.clone());
            let filter = if r.pattern.is_empty() { String::new() } else { format!(" · {}", r.pattern) };
            let title = format!(" {db}{filter} — {}{} {}", r.keys.len(), if r.cursor != "0" { "+" } else { "" }, if r.scanning { "scanning… " } else { "" });
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
                        // The folder's key count beside its name (muted), like "tables 68"; the right column only has key TTLs
                        crate::app::KRow::Folder { path, name, depth, count } => Row::new(name.clone()).indent(*depth).chevron(Some(r.open.contains(path))).lead(icon("▤", ACCENT)).hint(count.to_string()),
                        crate::app::KRow::Key { idx, depth } => {
                            let k = &r.keys[*idx];
                            let ttl = qurry_shared::redis_cmd::ttl_label(k.ttl);
                            // A long name is cut with … so the TTL column stays (indent · slot · badge · gaps)
                            let room = w.saturating_sub(depth * 2 + 2 + 9 + ttl.chars().count() + 2);
                            let name = if k.key.chars().count() <= room { k.key.clone() } else { k.key.chars().take(room.saturating_sub(1)).collect::<String>() + "…" };
                            Row::new(name).indent(*depth).chevron(None).lead(badge(&format!("{:<6}", k.ty.to_uppercase()), type_color(&k.ty))).lead(Span::raw(" ")).trailing(ttl)
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
                        Some(g) => g.render(f, right, title, vf, color, &align),
                        None => {
                            f.render_widget(Paragraph::new(r.text.as_deref().unwrap_or_default()).wrap(Wrap { trim: false }).block(panel(title, vf)), right);
                        }
                    }
                }
            }
        }
    }
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
fn empty_field_note(label: &str, ssh_auth: &str) -> Option<&'static str> {
    match label {
        "AWS profile" => Some("(default profile)"),
        "AWS region" => Some("(the profile's region)"),
        "SSH key file" => Some("(ssh-agent, then ~/.ssh/config IdentityFile)"),
        "SSH secret" if ssh_auth == "password" => Some("(ssh-agent)"),
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
        // A local Host with AWS SSM is the instance itself: a hint, not an error (a database on the instance is fine)
        (None, None) => {
            let hint = (form.value("Tunnel") == "aws ssm").then_some(form.value("Host")).and_then(qurry_shared::errors::ssm_local_host);
            return hint.map(|h| vec![Line::styled(h, Style::default().fg(WARN))]).unwrap_or_default();
        }
    };
    // One row per line of it (a TLS error is followed by its hint)
    m.lines().map(|l| Line::styled(l.to_string(), Style::default().fg(if ok { OK } else { DANGER }))).collect()
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
            let disabled = form.disabled(fl.label);
            let shown = match fl.kind {
                FieldKind::Secret if fl.value != "<saved>" => "•".repeat(fl.value.chars().count()),
                FieldKind::Toggle => format!("[{}]", if disabled.unwrap_or(fl.value == "on") { "x" } else { " " }),
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
            if let Some(note) = empty_field_note(fl.label, form.value("SSH auth")).filter(|_| fl.value.trim().is_empty()) {
                spans.push(Span::styled(format!("  {note}"), Style::default().fg(DIM)));
            }
            if disabled.is_some() {
                // Dim as a whole
                return Line::from(spans).style(Style::default().fg(DIM).add_modifier(Modifier::DIM));
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

#[cfg(test)]
mod tests {
    use super::*;

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
        h.app.popup = Some(crate::app::Popup::Confirm { opened: None, sql: None, text: "Run it?".into(), on_yes: Box::new(|a| a.status = "ran".into()) });
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
        // n fields shown; Tab skips the disabled ones (a new source's Admin reports: MCP is Off)
        let (n, stops) = match &h.app.popup {
            Some(Popup::ConnForm(f)) => (f.visible().len(), f.visible().iter().filter(|&&i| f.disabled(f.fields[i].label).is_none()).count()),
            _ => unreachable!(),
        };
        // Everything fits at 120×40: no position shown
        assert!(!h.screen(120, 40).contains(&format!("1/{n}")));
        for _ in 1..stops {
            h.key("Tab");
        }
        for (w, ht) in [(80, 24), MIN_SIZE] {
            let s = h.screen(w, ht);
            assert!(s.contains("Redis nodes ▏"), "{w}×{ht}\n{s}");
            assert!(s.contains("Esc cancel") && s.contains(&format!("{n}/{n}")), "{w}×{ht}\n{s}");
        }
        for _ in 1..stops {
            h.key("Up");
        }
        for (w, ht) in [(80, 24), MIN_SIZE] {
            let s = h.screen(w, ht);
            assert!(s.contains("Name ▏") && s.contains("Esc cancel"), "{w}×{ht}\n{s}");
        }
    }

    /// An SSH tunnel with an empty key file or password says it uses ssh-agent, like the GUI's placeholders
    #[tokio::test]
    async fn conn_form_says_empty_ssh_credentials_use_the_agent() {
        let mut h = crate::testkit::Harness::new().await;
        let cfg = serde_json::from_value(serde_json::json!({ "id": "", "name": "t", "kind": "postgres", "host": "db", "port": 5432, "user": "u", "database": "d",
            "ssh": { "enabled": true, "host": "bastion", "port": 22, "user": "ops", "auth": "key", "keyPath": "" } })).unwrap();
        h.app.conn_form(Some(cfg));
        let s = h.screen(120, 40);
        assert!(s.contains("(ssh-agent, then ~/.ssh/config IdentityFile)"), "{s}");
        let Some(Popup::ConnForm(f)) = &mut h.app.popup else { unreachable!() };
        f.fields.iter_mut().find(|x| x.label == "SSH auth").unwrap().value = "password".into();
        let s = h.screen(120, 40);
        assert!(s.lines().any(|l| l.contains("SSH secret") && l.contains("(ssh-agent)")), "{s}");
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

    /// The startup notices fit an 80-column status line whole, with a gap before the key hints: one at a time, the light
    /// palette's first (unless NO_COLOR, which draws no palette)
    #[tokio::test]
    async fn color_notices_fit_80_columns() {
        use crate::theme::ColorMode;
        let mut h = crate::testkit::Harness::new().await;
        let light = "Light palette — ⌃k › Theme… › Dark to undo";
        for (mode, auto_light, want) in [
            (ColorMode::Ansi16, false, "16 colors: try COLORTERM=truecolor"),
            (ColorMode::NoColor, false, "Colors off: unset NO_COLOR"),
            (ColorMode::TrueColor, true, light),
            (ColorMode::Ansi16, true, light),
            (ColorMode::NoColor, true, "Colors off: unset NO_COLOR"),
        ] {
            (h.app.color, h.app.auto_light) = (mode, auto_light);
            assert_eq!(h.app.startup_notice().as_deref(), Some(want));
            h.app.status = want.into();
            let s = h.screen(80, 24);
            assert!(s.lines().last().unwrap().contains(&format!("{want} ")), "{s}");
        }
        (h.app.color, h.app.auto_light) = (ColorMode::TrueColor, false);
        assert_eq!(h.app.startup_notice(), None);
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
}
