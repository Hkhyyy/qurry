use super::*;

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
        "Password reuse" => Some("(seconds; empty: run the command at every login)"),
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
pub(super) fn draw_conn_form(f: &mut Frame, area: Rect, form: &crate::app::ConnForm) {
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
}
