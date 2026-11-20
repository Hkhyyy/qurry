use super::*;

/// Body of one tab. focused = the focused pane (mouse hit areas are recorded only for this pane)
pub(super) fn draw_pane(f: &mut Frame, app: &mut App, idx: usize, body: Rect, focused: bool) {
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
    // A cut value in the value panel: that its whole value is loading, or why it can't
    let cut_note = value_panel.then(|| crate::app::cut_note(&app.tabs[idx])).flatten();
    let tab = &mut app.tabs[idx];
    let db = tab.db.clone();
    let title = tab.title.clone();
    match &mut tab.kind {
        TabKind::Console(c) => {
            let [ed, res] = Layout::vertical([Constraint::Percentage(pct), Constraint::Percentage(100 - pct)]).areas(body);
            // The find bar takes a spot above the editor (so it doesn't cover text)
            let (find_bar, ed) = if finding {
                // Whole in a short window too (over the editor's lines then)
                let bh = 4.min(ed.height);
                (Rect { height: bh, ..ed }, Rect { y: ed.y + bh, height: ed.height - bh, ..ed })
            } else {
                (Rect::default(), ed)
            };
            let button = if busy { format!("[■ Cancel {k_cancel}]") } else { format!("[▶ Run {k_run}]") };
            let title = format!(
                " {db}{} · Tx: {}{} · {button}{} ",
                c.schema.as_ref().map(|s| format!(" · {s}")).unwrap_or_default(),
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
                    g.render_value_panel(f, side, value_keys, cut_note);
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
                    g.render_value_panel(f, side, value_keys, cut_note);
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
