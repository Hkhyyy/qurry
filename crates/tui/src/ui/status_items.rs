use super::*;

type Hints = Vec<(String, &'static str)>;

/// Bottom hints: (keys used on this screen, common global keys). Rebindable commands use their first key in the current keymap
fn status_hints(app: &App) -> (Hints, Hints) {
    use crate::app::Action as A;
    let key = |a: A| app.keys.keys(a).first().map(|k| disp(k));
    let fixed = |k: &str, l: &'static str| Some((disp(k), l));
    let cmd = |a: A, l: &'static str| key(a).map(|k| (k, l));
    // Pane keys (rebindable: F1 › Database Explorer keys / Data grid keys)
    let pane = |pane: Scope, a: A, l: &'static str| app.keys.slot_keys(crate::keys::Slot { action: a, pane }).first().map(|k| (crate::keys::pane_disp(k), l));
    let (tree, grid) = (|a, l| pane(Scope::Tree, a, l), |a, l| pane(Scope::Grid, a, l));
    // Column widths: narrower · wider · fit (< > =) in one hint
    let widths = || {
        let k: Vec<String> = [A::ColumnNarrower, A::ColumnWider, A::ColumnFit].into_iter().filter_map(|a| grid(a, "").map(|(k, _)| k)).collect();
        (!k.is_empty()).then(|| (k.join(" "), "width"))
    };
    // Column layout: hide · pin · Columns… (H P C) in one hint
    let layout = || {
        let k: Vec<String> = [A::ColumnHide, A::ColumnPinToggle, A::Columns].into_iter().filter_map(|a| grid(a, "").map(|(k, _)| k)).collect();
        (!k.is_empty()).then(|| (k.join(" "), "columns"))
    };
    let (ctx, glob) = match app.focus {
        Focus::Tree => (
            vec![fixed("Enter", "open"), fixed("F4", "console"), tree(A::ManageSchemas, "schemas"), fixed("/", "filter"), tree(A::NewDataSource, "new"), tree(A::EditDataSource, "edit"), tree(A::WsMoveConn, "workspace"), tree(A::Refresh, "refresh"), cmd(A::Ddl, "DDL")],
            vec![cmd(A::SearchEverywhere, "search"), cmd(A::Palette, "actions"), cmd(A::Help, "keys")],
        ),
        Focus::Editor => (
            vec![cmd(A::Execute, "run"), cmd(A::Complete, "complete"), cmd(A::Explain, "explain"), cmd(A::TxToggle, "tx"), fixed("⇧Tab", "result"), fixed("⌘A ⌫", "clear"), fixed("Esc", "explorer")],
            vec![cmd(A::SearchEverywhere, "search"), cmd(A::RecentTabs, "tabs"), cmd(A::Palette, "actions")],
        ),
        Focus::Grid => (
            match app.tabs.get(app.active).map(|t| &t.kind) {
                Some(TabKind::Table(_)) => vec![
                    fixed("Enter", "edit"), grid(A::ValueEditor, "value"), widths(), grid(A::AddRow, "add"), grid(A::DeleteRow, "delete"), grid(A::SetNull, "NULL"), cmd(A::Submit, "submit"), grid(A::Revert, "revert"), fixed("w", "where"), fixed("o", "order"), fixed("s", "sort"), grid(A::ExportCsv, "export"), layout(),
                ],
                Some(TabKind::Redis(r)) if r.focus_value => vec![fixed("e", "edit"), fixed("a", "add"), fixed("x", "delete"), fixed("Esc", "keys")],
                Some(TabKind::Redis(_)) => vec![fixed("Enter", "value"), fixed("d", "database"), fixed("/", "filter"), fixed("a", "add"), fixed("D", "del key"), fixed("t", "TTL"), fixed("R", "rename"), fixed(":", "command")],
                Some(TabKind::Text(t)) if t.structure_view => vec![fixed("v", "SQL"), fixed("r", "reload"), fixed("y", "copy"), fixed("Esc", "explorer")],
                Some(TabKind::Text(t)) if t.has_structure() => vec![fixed("v", "structure"), fixed("r", "reload"), fixed("y", "copy"), fixed("Esc", "explorer")],
                Some(TabKind::Text(t)) if t.ddl_of.is_some() => vec![fixed("r", "reload"), fixed("y", "copy"), fixed("Esc", "explorer")],
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
                    v.extend([fixed("Enter", "query→console"), fixed("g", "SQL"), fixed("y", "copy"), fixed("d", "database")]);
                    v
                }
                _ => vec![fixed("s", "sort"), fixed("y", "copy"), grid(A::ValueEditor, "value"), widths(), grid(A::ExportCsv, "export"), fixed("Esc", "editor"), layout()],
            },
            vec![cmd(A::NewConsole, "console"), cmd(A::RecentTabs, "tabs"), cmd(A::SearchEverywhere, "search"), cmd(A::Palette, "actions")],
        ),
    };
    (ctx.into_iter().flatten().collect(), glob.into_iter().flatten().collect())
}

/// The status line's right side: key hints (screen keys · … │ global keys · …), then the focused grid's items (item │ item │ Position,
/// qurry_shared::statusbar), fitted into `w` cells: the screen keys go first (from the end), then the global keys but the last (⌃k
/// actions, where the items are chosen), then items in qurry_shared::statusbar::DROP order, then that last key; Position stays.
/// Also says whether an item had to go
fn status_right(mut ctx: Vec<(String, &'static str)>, mut glob: Vec<(String, &'static str)>, mut segs: Vec<Segment>, w: usize) -> (Line<'static>, bool) {
    let all = segs.len();
    use unicode_width::UnicodeWidthStr;
    let hints = |v: &[(String, &str)]| v.iter().map(|(k, l)| k.width() + 1 + l.width()).sum::<usize>() + v.len().saturating_sub(1) * 3;
    let items = |v: &[Segment]| v.iter().map(|g| g.label.as_ref().map_or(0, |l| l.width() + 2) + g.value.width()).sum::<usize>() + v.len().saturating_sub(1) * 3;
    let width = |ctx: &[_], glob: &[_], segs: &[Segment]| hints(ctx) + if ctx.is_empty() || glob.is_empty() { 0 } else { 3 } + hints(glob) + if segs.is_empty() { 0 } else { 3 + items(segs) } + 1;
    while width(&ctx, &glob, &segs) > w {
        if ctx.pop().is_some() {
            continue;
        }
        if glob.len() > 1 {
            glob.remove(0);
        } else if let Some(i) = statusbar::DROP.iter().find_map(|d| segs.iter().position(|g| g.id == *d)) {
            segs.remove(i);
        } else if !glob.is_empty() {
            glob.clear();
        } else {
            break;
        }
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
    let dropped = segs.len() < all;
    let has_ctx = !ctx.is_empty();
    let has_glob = !glob.is_empty();
    push(ctx, &mut spans);
    if has_ctx && has_glob {
        spans.push(Span::styled(" │ ", Style::default().fg(DIM)));
    }
    push(glob, &mut spans);
    if !segs.is_empty() {
        spans.push(Span::raw("   "));
    }
    for (i, g) in segs.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(DIM)));
        }
        if let Some(l) = g.label {
            spans.push(muted(format!("{l}: ")));
        }
        spans.push(Span::styled(g.value, Style::default().fg(Color::Gray)));
    }
    spans.push(Span::raw(" "));
    (Line::from(spans), dropped)
}

pub(super) fn draw_status(f: &mut Frame, app: &mut App, area: Rect) {
    // The new-version notice stays visible in front of the status text
    let badge = app.update.as_ref().map(|v| format!(" ⬆ v{v} available — {} › Update Qurry & Restart ", app.keys.label(crate::app::Action::Palette)));
    let bw = badge.as_ref().map(|b| b.chars().count() as u16).unwrap_or(0);
    // If width is short, shrink the right-side hints (so the update notice·status text isn't cut)
    let (ctx, glob) = status_hints(app);
    // While the active tab runs or loads more rows: a moving bar, what it does and for how long (as the GUI's status bar), cancel
    // with the Cancel key. While export·import runs, a progress bar (stop with ⌃K › Cancel Export/Import). Else the status text
    // A source that reconnects while a load waits on it says so first, with its own time (#135; the GUI's running indicator too)
    // (not while the active tab runs on its own: the Cancel key stops that, App::cancels_own)
    let reconnect = app.reconnect_line().filter(|_| !app.cancels_own());
    // Its time and Cancel key come after the source's name: it may take up to three quarters of the line. So may what its Cancel or
    // failure leaves, which ends with the key that loads it again (App::retry_status)
    let retry = !app.status.is_empty() && app.status == app.retry_status;
    let cap = if reconnect.is_some() || retry { area.width * 3 / 4 } else { area.width / 2 };
    let busy = match reconnect {
        Some((line, ms)) => Some(format!("▕{}▏ {line}", bounce(ms))),
        None => app.tabs.get(app.active).and_then(|t| t.busy()).map(|what| {
            let ms = app.run_elapsed(app.active).unwrap_or_default().as_millis() as u64;
            format!("▕{}▏ {what}… {} · {} cancel", bounce(ms), crate::app::elapsed_text(ms), app.keys.label(crate::app::Action::Cancel))
        }),
    };
    // A run's or a transfer's line (and the retry status) is never cut below its length (up to half the line) to make room for the items
    let progress = busy.is_some() || !app.jobs.is_empty() || retry;
    let status = match busy {
        Some(b) => b,
        None if app.jobs.is_empty() => disp(&app.status),
        None => app.jobs.iter().map(|(_, label, j)| job_line(label, j.progress())).collect::<Vec<_>>().join("  ·  "),
    };
    // Status text claims up to half the screen width first (so error·result messages aren't cut); only a third when an item of the
    // focused grid would otherwise go (not for a run's or a transfer's line)
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
    let lead = Line::from(line.clone()).width() as u16;
    let segs = app.status_segments();
    let sw = |cap: u16| (lead + status.chars().count() as u16 + 1).clamp(12, cap.max(12));
    let right = |sw: u16| status_right(ctx.clone(), glob.clone(), segs.clone(), area.width.saturating_sub(bw + sw) as usize);
    let (mut hints, dropped) = right(sw(cap));
    if dropped && !progress {
        hints = right(sw(area.width / 3)).0;
    }
    let hw = (hints.width() as u16).min(area.width.saturating_sub(bw + 12));
    let [u, l, r] = Layout::horizontal([Constraint::Length(bw), Constraint::Min(10), Constraint::Length(hw)]).areas(area);
    if let Some(b) = badge {
        f.render_widget(Paragraph::new(b).style(Style::default().fg(Color::Black).bg(WARN)), u);
    }
    // Cut with … and a space before the key hints when it doesn't fit
    let room = (l.width as usize).saturating_sub(lead as usize + 1);
    line.push(Span::raw(if unicode_width::UnicodeWidthStr::width(status.as_str()) > room { cut(&status, room) } else { status }));
    f.render_widget(Paragraph::new(Line::from(line)).style(Style::default().fg(Color::Gray).bg(SURFACE)), l);
    f.render_widget(Paragraph::new(hints).style(Style::default().bg(SURFACE)), r);
}

/// `s` cut to `w` cells, the last one an …
fn cut(s: &str, w: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if used + cw + 1 > w {
            break;
        }
        used += cw;
        out.push(ch);
    }
    if w > 0 {
        out.push('…');
    }
    out
}

/// "Export users ▕████░░░░░░▏ 1,234 rows · 42%". If the total is unknown the bar bounces back and forth
fn job_line(label: &str, (rows, done, total): (u64, u64, u64)) -> String {
    const W: usize = 10;
    // Backup·restore progress in bytes instead of rows
    let n = if rows == 0 && done > 0 { format!("{:.1} MB", done as f64 / 1048576.0) } else { format!("{} rows", crate::transfer::thousands(rows)) };
    let (bar, pct) = match (done * 100).checked_div(total) {
        Some(pct) => {
            let pct = pct.min(99) as usize;
            ("█".repeat(pct * W / 100) + &"░".repeat(W - pct * W / 100), format!(" · {pct}%"))
        }
        None => (bounce(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)), String::new()),
    };
    format!("{label} ▕{bar}▏ {n}{pct}")
}

/// A 10-cell bar whose end is unknown: a block moving along it, one cell per 250 ms
fn bounce(ms: u64) -> String {
    const W: usize = 10;
    let t = (ms / 250) as usize % (W - 2);
    "░".repeat(t) + "███" + &"░".repeat(W - 3 - t)
}
