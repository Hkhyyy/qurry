use super::*;

pub(super) fn editor_key(app: &mut App, k: KeyEvent) {
    let kind = app.tabs.get(app.active).map(|t| app.kind_of(&t.conn)).unwrap_or(qurry_core::store::DbKind::Postgres);
    let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) else {
        app.focus = Focus::Grid;
        return grid_key(app, k);
    };
    // While the autocomplete list is open
    if let Some(cmp) = &mut c.complete {
        match list_key(cmp, k.code) {
            Some(ListKey::Moved) => return,
            Some(ListKey::Pick) => return app.accept_completion(),
            Some(ListKey::Close) => return c.complete = None,
            None => {}
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
        // ' " ( [ close themselves, a closing one types over, ⌫ in an empty pair removes both, around a selection they wrap it
        _ if pair_key(&mut c.editor, &k, kind) => {}
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
    app.update_assist();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

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

    /// The chooser is modal: the selected option's rows are highlighted in the editor (and nothing once it closes); keys go to the
    /// chooser (typing filters it), Esc closes it without running and leaves the editor as it was
    #[tokio::test]
    async fn execute_chooser_previews_the_rows_it_would_run() {
        let mut h = Harness::new().await;
        h.console_with("select 1;\nselect 2;\nselect 3;", 1, 0);
        h.key("F5");
        let Some(crate::app::Popup::Palette(p)) = &h.app.popup else { panic!("no chooser") };
        assert_eq!(p.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["select 2", "All statements (3)", "From caret to end"]);
        // Which of the three statement rows are drawn on the selection background
        let lines = |h: &mut Harness| {
            let buf = h.buffer(100, 30);
            let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
            ["1", "2", "3"].map(|n| {
                let y = (0..30).find(|&y| row(y).contains(&format!("select {n};"))).unwrap();
                buf[(60, y)].bg == crate::ui::SEL
            })
        };
        assert_eq!(lines(&mut h), [false, true, false]);
        h.key("Down");
        assert_eq!(lines(&mut h), [true, true, true]);
        h.key("Down");
        assert_eq!(lines(&mut h), [false, true, true]);
        h.type_str("all");
        assert!(h.app.popup.is_some());
        assert_eq!(lines(&mut h), [true, true, true]);
        h.key("Esc");
        assert!(h.app.popup.is_none());
        assert_eq!(lines(&mut h), [false, false, false]);
        assert_eq!(h.console_text(), "select 1;\nselect 2;\nselect 3;");
        assert!(h.console_output().is_empty(), "{:?}", h.console_output());
    }

    /// With the execution setting set to Smallest, runs only the cursor statement without asking
    #[tokio::test]
    async fn execute_smallest_runs_caret_statement() {
        let mut h = Harness::new().await;
        h.app.exec.inside = qurry_core::exec::Inside::Smallest;
        h.console_with("BEGIN;\nGRANT a TO x;\n\nGRANT b TO x;\nCOMMIT;", 1, 2);
        h.key("F5");
        assert!(h.app.popup.is_none());
        h.settle().await;
        assert!(h.console_output().iter().any(|l| l.ends_with("> GRANT a TO x")));
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
}
