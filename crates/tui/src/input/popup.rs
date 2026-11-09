use super::*;

/// A yes pressed during a quiet confirm's pause (TLS_ASK_GRACE) doesn't count: say so instead of ignoring it
const TOO_SOON: &str = "Wait a moment, then answer (n / Esc cancels)";

pub(super) fn popup_key(app: &mut App, k: KeyEvent) {
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
                let s = crate::keys::COMMANDS[sel].0;
                let spec = crate::keys::spec(&k);
                match spec.clone().filter(|x| crate::keys::bindable_for(s, x)) {
                    None => {
                        let key = spec.map(|x| if s.pane == Scope::Global { crate::keys::disp(&x) } else { crate::keys::pane_disp(&x) }).unwrap_or_else(|| "That key".into());
                        let mods = if cfg!(target_os = "macos") { "⌃/⌥" } else { "Ctrl/Alt" };
                        Some(if s.pane == Scope::Global {
                            format!("{key} can't be a command key — use a {mods} combination or an F key")
                        } else {
                            format!("{key} can't be a key in the {} — use a letter, a {mods} combination or an F key (arrows, Enter, Tab and the moving and copying letters stay fixed)", crate::keys::pane_name(s.pane))
                        })
                    }
                    Some(key) if app.keys.reserved(s, &key).is_some() => {
                        let a = app.keys.reserved(s, &key).unwrap();
                        Some(format!("{} is the key of {} — pick another key, so the keymap stays reachable", crate::keys::pane_disp(&key), crate::app::title(a)))
                    }
                    Some(key) => {
                        let taken = app.keys.bind(s, key.clone(), add);
                        let mut m = format!("{} → {}", crate::app::title(s.action), app.keys.slot_label(s));
                        if let Some(t) = taken {
                            m += &format!(" · removed {key} from {}", crate::app::title(t.action));
                        }
                        if let Err(e) = app.save_keys() {
                            m = format!("Save failed: {e}");
                        }
                        Some(m)
                    }
                }
            };
            sel = sel.min(crate::keys::COMMANDS.len() - 1);
            app.popup = Some(Popup::Help { sel, scroll, record: None, message });
        }
        Popup::Help { sel, scroll, message, .. } => {
            let n = crate::keys::COMMANDS.len();
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
                    let s = crate::keys::COMMANDS[sel].0;
                    app.keys.reset(s);
                    message = Some(match app.save_keys() {
                        Ok(()) => format!("{} reset to {}", crate::app::title(s.action), app.keys.slot_label(s)),
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
        // Closing it goes back to what it was opened from. A question under it comes up quiet, so a second press meant for the
        // notice (or typing that goes on) doesn't answer it
        Popup::Note { title, text, back } => {
            app.popup = match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::F(1) => Some(match *back {
                    Popup::Confirm { text, on_yes, .. } => Popup::Confirm { text, on_yes, opened: Some(Instant::now()) },
                    Popup::AllowPlaintext { text, on_yes, .. } => Popup::AllowPlaintext { text, on_yes, opened: Instant::now() },
                    back => back,
                }),
                _ => Some(Popup::Note { title, text, back }),
            };
        }
        // n / Esc always cancel. A confirm that came up on its own (`opened`) takes y / Enter only after a pause, like AllowPlaintext below
        Popup::Confirm { text, on_yes, opened } => match k.code {
            KeyCode::Char('n') | KeyCode::Esc => {}
            _ if opened.is_some_and(|t| t.elapsed() < App::TLS_ASK_GRACE) => {
                if matches!(k.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
                    app.status = TOO_SOON.into();
                }
                app.popup = Some(Popup::Confirm { text, on_yes, opened: Some(Instant::now()) })
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter if app.hits.confirm_unseen => {
                app.status = "The window is too small to show the whole question: enlarge it, then answer (n / Esc cancels)".into();
                app.popup = Some(Popup::Confirm { text, on_yes, opened });
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => on_yes(app),
            _ => app.popup = Some(Popup::Confirm { text, on_yes, opened: opened.map(|_| Instant::now()) }),
        },
        // Only a y after a pause counts: every other key (and any key inside the grace) starts the grace again, so typing that
        // goes on into the popup never allows plaintext. n / Esc cancel at once
        Popup::AllowPlaintext { text, on_yes, opened } => match k.code {
            KeyCode::Char('n') | KeyCode::Esc => {}
            _ if opened.elapsed() < App::TLS_ASK_GRACE => {
                if matches!(k.code, KeyCode::Char('y' | 'Y')) {
                    app.status = TOO_SOON.into();
                }
                app.popup = Some(Popup::AllowPlaintext { text, on_yes, opened: Instant::now() })
            }
            KeyCode::Char('y') | KeyCode::Char('Y') if app.hits.confirm_unseen => {
                app.status = "The window is too small to show the whole question: enlarge it, then answer (n / Esc cancels)".into();
                app.popup = Some(Popup::AllowPlaintext { text, on_yes, opened });
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => on_yes(app),
            _ => app.popup = Some(Popup::AllowPlaintext { text, on_yes, opened: Instant::now() }),
        },
        Popup::Input { title, body, mut input, on_ok, filter, mut redis, mut complete } => {
            // The open list (a table filter's completion) takes the console's list keys: ↑↓ PgUp PgDn move, ↩ / ⇥ pick, ⎋ closes it
            // The Redis command line's key names: ↩ runs the line unless ↑↓ moved to one (⇥ picks), so a key typed out in full
            // isn't swapped for a longer one the SCAN found
            let keys_unmoved = redis.as_ref().is_some_and(|p| !p.moved) && complete.as_ref().is_some_and(|c| c.items.first().is_some_and(|i| i.kind == qurry_core::complete::ItemKind::Value));
            if let Some(cmp) = &mut complete
                && !(keys_unmoved && k.code == KeyCode::Enter)
                && let Some(key) = list_key(cmp, k.code)
            {
                match key {
                    ListKey::Pick => {
                        for _ in 0..cmp.prefix {
                            input.delete_char();
                        }
                        input.insert_str(&cmp.items[cmp.sel].insert);
                        complete = None;
                    }
                    ListKey::Close => complete = None,
                    ListKey::Moved => {
                        if let Some(p) = &mut redis {
                            p.moved = true;
                        }
                    }
                }
                app.popup = Some(Popup::Input { title, body, input, on_ok, filter, redis, complete });
                return;
            }
            match k.code {
                KeyCode::Esc => {}
                KeyCode::Enter => on_ok(app, input.lines().join("")),
                // The Redis command line: ↑↓ through this data source's history, newest first (↓ past the newest: an empty line)
                KeyCode::Up | KeyCode::Down if redis.is_some() => {
                    let p = redis.as_mut().expect("redis prompt");
                    p.hi = match (k.code, p.hi) {
                        (KeyCode::Up, None) if !p.hist.is_empty() => Some(0),
                        (KeyCode::Up, Some(i)) => Some((i + 1).min(p.hist.len() - 1)),
                        (KeyCode::Down, Some(i)) => i.checked_sub(1),
                        (_, h) => h,
                    };
                    input = tui_textarea::TextArea::new(vec![p.hi.map(|i| p.hist[i].clone()).unwrap_or_default()]);
                    input.set_cursor_line_style(ratatui::style::Style::default());
                    input.move_cursor(tui_textarea::CursorMove::End);
                    app.popup = Some(Popup::Input { title, body, input, on_ok, filter, redis, complete: None });
                }
                _ if redis.is_some() => {
                    // Commands and their arguments complete as you type; key names once a bounded SCAN has read them
                    if k.code != KeyCode::Tab {
                        input.input(k);
                    }
                    let p = redis.as_mut().expect("redis prompt");
                    p.hi = None;
                    p.moved = false;
                    let (c, ask_keys) = App::redis_complete(&input, p);
                    let tab = p.tab;
                    app.popup = Some(Popup::Input { title, body, input, on_ok, filter, redis, complete: c });
                    if let Some(prefix) = ask_keys {
                        app.redis_key_names(tab, prefix);
                    }
                }
                _ => {
                    // Typing a word or '.' opens the list, deleting re-filters an open one, ⇥ / ⌃Space ask at the cursor (a
                    // single-line input inserts no tab); other keys close it
                    let ask = k.code == KeyCode::Tab || (k.code == KeyCode::Char(' ') && k.modifiers.contains(KeyModifiers::CONTROL)) || k.code == KeyCode::Null;
                    let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                    let typed = plain && matches!(k.code, KeyCode::Char(c) if c.is_alphanumeric() || c == '_' || c == '.' || c == '\'' || c == ' ');
                    let refilter = complete.is_some() && k.code == KeyCode::Backspace;
                    // The WHERE / ORDER BY inputs pair up quotes and brackets, as the console does
                    let paired = !ask && filter.as_ref().is_some_and(|f| pair_key(&mut input, &k, f.kind));
                    if !ask && !paired {
                        input.input(k);
                    }
                    complete = filter.as_ref().filter(|_| ask || typed || refilter).and_then(|f| filter_complete(&input, f, ask));
                    app.popup = Some(Popup::Input { title, body, input, on_ok, filter, redis, complete });
                }
            }
        }
        // Handled separately above
        Popup::Find(bar) => app.popup = Some(Popup::Find(bar)),
        // Space checks the row under the cursor, y / Enter imports the checked ones, n / Esc cancel
        Popup::ImportPick(mut p) => {
            match k.code {
                KeyCode::Esc | KeyCode::Char('n') => return,
                KeyCode::Down | KeyCode::Char('j') => p.move_by(1),
                KeyCode::Up | KeyCode::Char('k') => p.move_by(-1),
                KeyCode::PageDown => p.move_by(10),
                KeyCode::PageUp => p.move_by(-10),
                KeyCode::Char(' ') => p.toggle(),
                KeyCode::Char('y' | 'Y') | KeyCode::Enter if p.count() > 0 => return app.ds_import_apply(p),
                KeyCode::Char('y' | 'Y') | KeyCode::Enter => app.status = "Check at least one data source to import (Space), or n / Esc to cancel".into(),
                _ => {}
            }
            app.popup = Some(Popup::ImportPick(p));
        }
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
                // A checklist (⌃k › Status Bar Items…): Space toggles as Enter does, unless a filter is being typed
                KeyCode::Enter | KeyCode::Char(' ') if k.code == KeyCode::Enter || (p.input.lines().concat().is_empty() && p.items.iter().all(|i| matches!(i.action, crate::app::PAction::StatusItem(_)))) => {
                    if let Some(it) = App::palette_matches(&p).get(p.sel).map(|x| (*x).clone()) {
                        // Shown while it runs, as on a click: an item can come back to it (Production Log's note)
                        app.popup = Some(Popup::Palette(p));
                        return app.run_palette(it, false);
                    }
                }
                // The Go to DDL key (⌃b): DDL of the selected object
                _ if app.keys.lookup(&k) == Some(Action::Ddl) => {
                    if let Some(it) = App::palette_matches(&p).get(p.sel).map(|x| (*x).clone()) {
                        app.popup = Some(Popup::Palette(p));
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
            match k.code {
                // Esc or ⌃t again while testing cancels the test (an SSM session can take up to 30s); otherwise Esc closes the form
                KeyCode::Esc if form.cancel_test() => {}
                KeyCode::Char('t') if k.modifiers.contains(KeyModifiers::CONTROL) && form.cancel_test() => {}
                KeyCode::Esc => return,
                KeyCode::Down | KeyCode::Tab => form.step(1),
                KeyCode::Up | KeyCode::BackTab => form.step(-1),
                // SSH unknown server: trust after confirming the fingerprint (only with the SSH tunnel: its fields are hidden otherwise)
                KeyCode::Char('y') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    form.message = Some(if form.value("Tunnel") != "ssh" {
                        (false, "⌃y trusts an SSH tunnel's server; set Tunnel to ssh first".into())
                    } else {
                        let port = form.value("SSH port").trim().parse().unwrap_or(22);
                        match app.state.trust_ssh_host(form.value("SSH host").trim().to_string(), port) {
                            Ok(m) => (true, format!("✓ {m} — press ⌃t to test again")),
                            Err(e) => (false, format!("✕ {e}")),
                        }
                    });
                }
                KeyCode::Char('t') if k.modifiers.contains(KeyModifiers::CONTROL) => test_form(app, &mut form),
                KeyCode::Char('s') if k.modifiers.contains(KeyModifiers::CONTROL) => return save_form(app, form),
                KeyCode::Enter => return save_form(app, form),
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if !matches!(form.fields[form.sel].kind, FieldKind::Text | FieldKind::Secret) => {
                    // A running test and the last message are for the other tunnel (⌃y would act on the SSH server): drop both
                    if form.fields[form.sel].label == "Tunnel" {
                        if let Some((_, h)) = form.testing.take() {
                            h.abort();
                        }
                        form.message = None;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    /// The WHERE input completes like the console (the table's columns, then keywords), in the console's list
    #[tokio::test]
    async fn where_input_completes_columns_and_keywords() {
        let mut h = Harness::new().await;
        let id = h.table_tab("bank_info", &["bank_code", "bank_name"], &[&["002", "산업"]], &["bank_code"]);
        h.key("w");
        h.type_str("ban");
        let s = h.screen(120, 30);
        // Only the table's columns: not the table itself (bank_info) or other tables and schemas
        assert!(s.contains("bank_code") && s.contains("bank_name") && s.contains(" 1/2 "), "{s}");
        h.key("Down");
        h.key("Tab");
        // Not in a string literal
        h.type_str(" = '00");
        assert!(!h.screen(120, 30).contains(" 1/"), "list inside a string");
        h.type_str("2' an");
        h.key("Enter");
        h.key("Enter");
        assert_eq!(h.table(id).where_, "bank_name = '002' and");
    }

    /// ORDER BY: a column, then ASC / DESC; ⎋ closes the list first, the input only on the second ⎋; a whole word leaves ↩ to apply
    #[tokio::test]
    async fn order_input_completes_and_esc_closes_the_list_first() {
        let mut h = Harness::new().await;
        let id = h.table_tab("bank_info", &["bank_code", "bank_name"], &[&["002", "산업"]], &["bank_code"]);
        h.key("o");
        h.type_str("bank_c");
        h.key("Enter");
        h.type_str(" de");
        let s = h.screen(120, 30);
        assert!(s.contains("desc") && s.contains("delete"), "{s}");
        h.key("Esc");
        assert!(matches!(&h.app.popup, Some(crate::app::Popup::Input { complete: None, .. })), "first ⎋ closes only the list");
        h.type_str("sc");
        assert!(!h.screen(120, 30).contains(" 1/"), "desc is a whole keyword: no list");
        h.key("Enter");
        assert!(h.app.popup.is_none());
        assert_eq!(h.table(id).order, "bank_code desc");
        h.key("o");
        h.key("Esc");
        assert!(h.app.popup.is_none());
        assert_eq!(h.table(id).order, "bank_code desc");
    }

    /// ⇥ asks for the list where typing doesn't open it: at an empty word the table's columns come first, then keywords
    #[tokio::test]
    async fn where_input_tab_lists_at_an_empty_word() {
        let mut h = Harness::new().await;
        let id = h.table_tab("bank_info", &["bank_code", "bank_name"], &[&["002", "산업"]], &["bank_code"]);
        h.key("w");
        h.key("Tab");
        let Some(crate::app::Popup::Input { complete: Some(c), .. }) = &h.app.popup else { panic!("⇥ opens the list") };
        assert_eq!(c.items.iter().take(2).map(|i| i.label.as_str()).collect::<Vec<_>>(), ["bank_code", "bank_name"]);
        assert!(c.items.iter().any(|i| i.label == "and"), "keywords after the columns");
        h.key("Tab");
        h.type_str(" = '002' and ");
        assert!(!h.screen(120, 30).contains(" 1/"), "typing a space opens nothing");
        h.key("Tab");
        h.key("Down");
        h.key("Enter");
        h.key("Enter");
        assert_eq!(h.table(id).where_, "bank_code = '002' and bank_name");
    }

    /// Manage Shown Schemas: Space to toggle, Enter to save (the Data Source's scope, shared with the GUI). Esc doesn't save
    #[tokio::test]
    async fn manage_shown_schemas_saves_scope() {
        use qurry_core::store::{ConnectionConfig, DbKind};
        let mut h = Harness::new().await;
        let cfg = ConnectionConfig {
            id: "c1".into(), name: "pg".into(), kind: DbKind::Postgres, host: "localhost".into(), port: 5432, user: "postgres".into(), database: "postgres".into(),
            scope: Default::default(), password_saved: false, ssh_secret_saved: false, ssh: None, color: None, read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
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

    /// A name with a newline (PostgreSQL quoted identifier, Redis key) stays on the input's border: it can't open an explanation box of its own
    #[tokio::test]
    async fn input_title_newline_does_not_make_a_body() {
        let mut h = Harness::new().await;
        h.app.input("Generate test data into public.orders\nThis is a scratch copy, safe to fill", "", |_, _| {});
        let screen = h.screen(120, 24);
        let row = screen.lines().find(|l| l.contains("Generate test data into public.orders")).expect(&screen);
        assert!(row.contains("safe to fill") && row.contains("(Enter / Esc)"), "{screen}");
        assert_eq!(screen.matches("safe to fill").count(), 1, "{screen}");
    }

    /// A notice (a failed Commit) shown while a connection test runs: the result goes to the form under it, not lost with
    /// 'Testing…' left on after Esc
    #[tokio::test]
    async fn async_result_reaches_the_popup_under_a_notice() {
        use qurry_core::store::{ConnectionConfig, DbKind};
        let mut h = Harness::new().await;
        let cfg = ConnectionConfig {
            id: "c1".into(), name: "pg".into(), kind: DbKind::Postgres, host: "qurry.invalid".into(), port: 1, user: "postgres".into(), database: "postgres".into(),
            scope: Default::default(), password_saved: false, ssh_secret_saved: false, ssh: None, color: None, read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        h.app.conn_form(Some(cfg));
        h.key("Ctrl+t");
        h.app.notice("Commit outcome unknown".into(), "Check the data".into());
        let testing = |h: &mut Harness| matches!(h.app.popup_under(), Some(crate::app::Popup::ConnForm(f)) if f.testing.is_some());
        for _ in 0..100 {
            if !testing(&mut h) {
                break;
            }
            h.settle().await;
        }
        assert!(matches!(h.app.popup, Some(crate::app::Popup::Note { .. })));
        h.key("Esc");
        let Some(crate::app::Popup::ConnForm(f)) = &h.app.popup else { panic!("no form") };
        let m = &f.message.as_ref().unwrap().1;
        assert!(m.starts_with('✕'), "{m}");
    }

    /// Popups that open later (a run's warning, Submit, an update) stack over a notice instead of replacing it: Esc goes back to it
    #[tokio::test]
    async fn later_popup_keeps_the_notice() {
        let mut h = Harness::new().await;
        h.app.notice("Commit outcome unknown".into(), "Check the data".into());
        h.app.warn_ran(Some("The Production log couldn't record it".into()));
        let Some(crate::app::Popup::Note { title, .. }) = &h.app.popup else { panic!("no warning") };
        assert_eq!(title, "Ran, with a warning");
        h.key("Esc");
        let Some(crate::app::Popup::Message { title, .. }) = &h.app.popup else { panic!("notice lost") };
        assert_eq!(title, "Commit outcome unknown");
        h.key("Esc");
        assert!(h.app.popup.is_none());
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
            started: false,
        };
        // Logged before it ran with no result: its outcome is unknown, so it says so (not a row count, not "committed")
        h.app.state.audit.add(qurry_core::audit::AuditEntry { rows: None, started: true, ..entry("c", "COMMIT", None) }).unwrap();
        h.app.state.audit.add(entry("a", "delete from big", None)).unwrap();
        h.app.state.audit.add(entry("b", "update t set a = 1 where id = 2", Some("UPDATE t SET a = 0 WHERE id = 2;"))).unwrap();
        h.console_tab();
        let tabs = h.app.tabs.len();
        h.app.run_action(Action::ProductionLog);
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no log") };
        assert_eq!(p.items[0].label, "update t set a = 1 where id = 2");
        assert!(p.items[0].hint.starts_with("↶ undo · 2 row(s)") && p.items[0].hint.ends_with("PROD"), "{}", p.items[0].hint);
        assert!(p.items[2].hint.starts_with("no result recorded"), "{}", p.items[2].hint);
        h.key("Enter");
        assert_eq!(h.app.tabs.len(), tabs + 1);
        assert_eq!(h.console_text(), "UPDATE t SET a = 0 WHERE id = 2;");
        h.app.run_action(Action::ProductionLog);
        h.key("Down");
        h.key("Enter");
        assert_eq!(h.app.tabs.len(), tabs + 1);
        // Without undo SQL, its note in a popup (the status bar would cut a long one off)
        let Some(crate::app::Popup::Note { text, .. }) = &h.app.popup else { panic!("no note") };
        assert!(text.contains("no undo kept"), "{text}");
        // The log stays in view behind it, and Esc goes back to it with the same entry and filter
        assert!(h.screen(100, 30).contains("update t set a = 1 where id = 2"));
        h.key("Esc");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("log closed") };
        assert!(p.title.starts_with("Production Log") && p.sel == 1, "{}", p.sel);
        h.type_str("big");
        h.key("Enter");
        h.key("Esc");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("log closed") };
        assert_eq!((p.input.lines().join(""), crate::app::App::palette_matches(p)[p.sel].label.as_str()), ("big".to_string(), "delete from big"));
        h.key("Esc");
        assert!(h.app.popup.is_none());
        let unknown = qurry_core::audit::AuditEntry {
            rows: None, error: Some("Connection reset by peer (os error 54)".into()), note: Some(qurry_core::audit::COMMIT_UNKNOWN_NOTE.into()),
            ..entry("d", qurry_core::audit::COMMIT_UNKNOWN, None)
        };
        h.app.state.audit.add(unknown).unwrap();
        h.app.run_action(Action::ProductionLog);
        h.key("Enter");
        let Some(crate::app::Popup::Note { text, .. }) = &h.app.popup else { panic!("no note") };
        assert!(text.contains(qurry_core::audit::COMMIT_UNKNOWN_NOTE) && text.contains("os error 54"), "{text}");
        // The whole note shows, wrapped, in a narrow terminal too
        let screen = h.screen(50, 20);
        assert!(screen.contains("Check the data before") && screen.contains("os error 54"), "{screen}");
        // A transaction a CALL ended: of unknown outcome too, and ↩ shows what to do
        h.key("Esc");
        h.key("Esc");
        let ended = qurry_core::audit::AuditEntry { rows: None, note: Some(qurry_core::audit::ENDED_UNKNOWN_NOTE.into()), ..entry("e", qurry_core::audit::ENDED_UNKNOWN, None) };
        h.app.state.audit.add(ended).unwrap();
        h.app.run_action(Action::ProductionLog);
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no log") };
        assert!(p.items[0].hint.starts_with("outcome unknown"), "{}", p.items[0].hint);
        h.key("Enter");
        let Some(crate::app::Popup::Note { text, .. }) = &h.app.popup else { panic!("no note") };
        assert_eq!(text, qurry_core::audit::ENDED_UNKNOWN_NOTE);
        // A transaction a statement or the server ended says how, as in the GUI
        h.key("Esc");
        h.key("Esc");
        for (id, sql) in [("i", "COMMIT (implicit)"), ("s", "ROLLBACK (by the server)")] {
            h.app.state.audit.add(qurry_core::audit::AuditEntry { rows: None, ..entry(id, sql, None) }).unwrap();
        }
        h.app.run_action(Action::ProductionLog);
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no log") };
        assert!(p.items[0].hint.starts_with("rolled back") && p.items[1].hint.starts_with("committed"), "{} | {}", p.items[0].hint, p.items[1].hint);
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
            prerelease: false,
        };
        h.app.show_update(r);
        assert_eq!(h.app.update.as_deref(), Some("9.0.1"));
        let s = h.screen(120, 30);
        for want in ["Qurry 9.0.1 is available", "• [fix] b (#2)", "• [feat] a (#1)", "Update Qurry & Restart"] {
            assert!(s.contains(want), "{want:?} in {s}");
        }
        assert!(!s.contains("old"), "{s}");
    }

    /// A check that finds nothing clears a ⬆ badge set on another channel (the GUI switched the shared channel), and names the channel
    #[tokio::test]
    async fn update_check_without_update_clears_the_badge() {
        let mut h = Harness::new().await;
        h.app.update = Some("9.0.1-beta.2".into());
        h.app.update_checked(qurry_core::update::Channel::Stable, None);
        assert_eq!(h.app.update, None);
        assert_eq!(h.app.status, format!("You have the latest stable version ({}).", env!("CARGO_PKG_VERSION")));
    }

    /// ⌃k › Update Channel…: opens on the saved channel; the pick is saved for the GUI too and the ⬆ badge follows it from the saved check (no request)
    #[tokio::test]
    async fn update_channel_is_picked_from_the_palette() {
        use qurry_core::update::Channel;
        let mut h = Harness::new().await;
        h.app.update = Some("9.0.1".into());
        h.app.palette_update_channel();
        let s = h.screen(120, 30);
        assert!(s.contains("Update Channel") && s.contains("● Stable") && s.contains("  Beta"), "{s}");
        h.key("Down");
        h.key("Enter");
        assert_eq!(h.app.state.store.update_channel(), Channel::Beta);
        // Nothing checked yet: no badge, and how to check
        assert_eq!(h.app.update, None);
        assert!(h.app.status.starts_with("Update channel: Beta") && h.app.status.ends_with("Check for Updates looks now"), "{}", h.app.status);
        // Reopened on Beta: Enter keeps it
        h.app.palette_update_channel();
        assert!(h.screen(120, 30).contains("● Beta"));
        h.key("Enter");
        assert_eq!(h.app.state.store.update_channel(), Channel::Beta);
        // A saved check (any age) answers for either channel
        let rel = |v: &str| serde_json::json!({"version": v, "url": "u", "notes": "", "assets": [], "prerelease": v.contains('-')});
        std::fs::write(h.app.state.store.dir().join("update-releases.json"), serde_json::json!({"checked_at": 0, "latest": {"stable": null, "beta": rel("99.0.0-beta.1")}}).to_string()).unwrap();
        h.app.palette_update_channel();
        h.key("Up");
        h.key("Enter");
        assert_eq!(h.app.update, None);
        assert!(h.app.status.starts_with("Update channel: Stable — you have the latest stable version"), "{}", h.app.status);
        h.app.palette_update_channel();
        h.key("Down");
        h.key("Enter");
        assert_eq!(h.app.update.as_deref(), Some("99.0.0-beta.1"));
        assert!(h.app.status.contains("Qurry 99.0.0-beta.1 is available"), "{}", h.app.status);
    }

    /// ⌃k › Theme…: the built-in themes, the custom ones made in the GUI and Sync with OS (the GUI's dark / light pick by COLORFGBG);
    /// opens on the current one. The pick switches the colors at once and is saved in settings.json for both apps (read again at
    /// the next start). Sync with OS on a light terminal says at startup how to get the dark colors back
    #[tokio::test]
    async fn theme_is_picked_from_the_palette() {
        use qurry_core::theme::CustomTheme;
        let mut h = Harness::new().await;
        h.app.auto_light = true;
        assert_eq!(h.app.theme_pick(), "system");
        let status_bg = |h: &mut Harness| h.buffer(120, 30)[(0, 29)].bg;
        let light_bg = status_bg(&mut h);
        assert!(h.app.startup_notice().unwrap().contains("⌃k › Theme… › Dark"), "{:?}", h.app.startup_notice());
        // A custom theme saved by the GUI meanwhile shows up when the list opens
        let mine = CustomTheme { id: String::new(), name: "Night shift".into(), base: "dracula".into(), colors: [("sel".into(), "#123456".into())].into() };
        h.app.state.store.save_custom_theme(&mine).unwrap();
        h.app.palette_theme();
        let s = h.screen(120, 30);
        for want in ["  Dark", "  Light", "  High contrast", "  Dracula", "  One Dark", "  Solarized Dark", "  Solarized Light", "  Night shift", "custom, dark", "● Sync with OS", "Dark / Light by the terminal background (COLORFGBG): Light now"] {
            assert!(s.contains(want), "{want}: {s}");
        }
        // Up from Sync with OS: the custom theme
        h.key("Up");
        h.key("Enter");
        assert_eq!((h.app.theme_pick(), h.app.state.store.theme_settings().theme.as_deref()), ("custom-1", Some("custom-1")));
        assert_eq!(h.app.status, "Theme: Night shift — saved for the GUI too");
        assert_eq!(h.app.startup_notice(), None, "picked by hand: nothing to say at startup");
        let buf = h.buffer(120, 30);
        assert_eq!(buf[(0, 29)].bg, ratatui::style::Color::Rgb(0x21, 0x22, 0x2c), "the status line in Dracula's panel color");
        assert_eq!(buf[(60, 10)].bg, ratatui::style::Color::Rgb(0x28, 0x2a, 0x36), "Dracula's background drawn");
        // Reopened on it; Dark by typing its name, on a dark terminal: the terminal's own background, the status line SURFACE
        h.app.auto_light = false;
        h.app.palette_theme();
        assert!(h.screen(120, 30).contains("● Night shift"));
        h.type_str("dark");
        h.key("Enter");
        assert_eq!(h.app.state.store.theme_settings().theme.as_deref(), Some("dark"));
        assert_eq!(status_bg(&mut h), crate::kit::SURFACE);
        assert_ne!(light_bg, crate::kit::SURFACE);
        assert_eq!(h.buffer(120, 30)[(60, 10)].bg, ratatui::style::Color::Reset);
        // Light on a dark terminal: the light palette
        h.app.palette_theme();
        h.key("Down");
        h.key("Enter");
        assert_eq!(h.app.theme_pick(), "light");
        assert_eq!(status_bg(&mut h), light_bg);
        // The next start reads it
        let next = crate::app::App::new(h.app.state.clone(), h.app.tx.clone());
        assert_eq!(next.theme_pick(), "light");
    }

    /// ⌃k › Grid Alignment…: a row per value class, Enter picks Left / Center / Right (NULL also Column type, its default); saved to
    /// settings.json for the GUI and applied to the grids at once. Opening it reads the file again (the GUI may have changed it), and
    /// the default drops the override
    #[tokio::test]
    async fn grid_alignment_is_picked_from_the_palette() {
        use qurry_core::format::{Align, Alignments, ValueClass};
        let mut h = Harness::new().await;
        h.app.palette_grid_align(ValueClass::String);
        let s = h.screen(120, 30);
        assert!(s.contains("Grid Alignment") && s.contains("NULL          Column type") && s.contains("String        Left"), "{s}");
        // NULL is the fifth row; it opens on Column type (the default, first), Down = Left
        for k in ["Down", "Down", "Down", "Down", "Enter"] {
            h.key(k);
        }
        let s = h.screen(120, 30);
        assert!(s.contains("NULL Alignment") && s.contains("● Column type") && s.contains("  Left") && s.contains("  Center") && s.contains("  Right"), "{s}");
        h.key("Down");
        h.key("Enter");
        let left = Alignments::from([(ValueClass::Null, Align::Left)]);
        assert_eq!(h.app.state.store.grid_align(), left);
        assert_eq!(h.app.align, left);
        assert!(h.app.status.starts_with("Grid alignment: NULL on the left"), "{}", h.app.status);
        // Back on the class list, at NULL, marked changed
        let s = h.screen(120, 30);
        assert!(s.contains("NULL          Left") && s.contains("changed — default Column type"), "{s}");
        // Right (0.3.0's look) is saved too: it is no longer the default
        h.key("Enter");
        for k in ["Down", "Down", "Enter"] {
            h.key(k);
        }
        assert_eq!(h.app.state.store.grid_align(), Alignments::from([(ValueClass::Null, Align::Right)]));
        // Column type again: the override is removed
        h.key("Enter");
        for k in ["Up", "Up", "Up", "Enter"] {
            h.key(k);
        }
        assert!(h.app.state.store.grid_align().is_empty());
        assert!(h.app.status.starts_with("Grid alignment: NULL where its column's type sits"), "{}", h.app.status);
        // Other classes offer no Column type
        h.key("Up");
        h.key("Enter");
        let s = h.screen(120, 30);
        assert!(s.contains("Boolean Alignment") && !s.contains("Column type"), "{s}");
        // A change made elsewhere (the GUI) shows when the palette opens
        h.key("Esc");
        h.app.state.store.set_grid_align(ValueClass::Number, Align::Center).unwrap();
        h.app.palette_grid_align(ValueClass::Number);
        assert_eq!(h.app.align, Alignments::from([(ValueClass::Number, Align::Center)]));
        // Number back to Right, its default: the override is removed
        h.key("Enter");
        h.key("Down");
        h.key("Enter");
        assert!(h.app.state.store.grid_align().is_empty());
        assert!(h.app.align.is_empty());
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
}
