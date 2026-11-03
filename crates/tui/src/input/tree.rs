use super::*;

pub(super) fn tree_key(app: &mut App, k: KeyEvent) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

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
}
