//! The TUI's commands: one ActionDef per Action, with what it runs. Ids and titles come from the shared command table (qurry_shared::keymap)
use super::*;

pub struct ActionDef {
    pub action: Action,
    /// Palette hint for commands without a rebindable key (keys::DEFAULT_KEYS); empty = the current keymap's keys
    pub hint: &'static str,
    pub run: fn(&mut App),
}

/// Command name shown in the palette·keymap·--help (the shared table's TUI title)
pub fn title(a: Action) -> &'static str {
    qurry_core::keymap::tui_command(&crate::keys::action_id(a)).map_or("?", |c| c.tui_title())
}

/// The definition of a command (every Action has exactly one: tests/actions.rs)
pub fn def(a: Action) -> &'static ActionDef {
    ACTIONS.iter().find(|d| d.action == a).unwrap_or_else(|| panic!("Action::{a:?} has no ActionDef: add one to ACTIONS in crates/tui/src/app/actions.rs"))
}

/// Every command, in palette order
pub const ACTIONS: &[ActionDef] = &[
    ActionDef { action: Action::NewConsole, hint: "", run: new_console },
    ActionDef { action: Action::Execute, hint: "", run: |app| app.execute(false) },
    ActionDef { action: Action::Explain, hint: "", run: |app| app.execute(true) },
    ActionDef { action: Action::ExplainAnalyze, hint: "", run: explain_analyze },
    ActionDef { action: Action::Cancel, hint: "", run: |app| app.cancel() },
    ActionDef { action: Action::TxToggle, hint: "", run: |app| app.tx(Action::TxToggle) },
    ActionDef { action: Action::Commit, hint: "", run: |app| app.tx(Action::Commit) },
    ActionDef { action: Action::Rollback, hint: "", run: |app| app.tx(Action::Rollback) },
    ActionDef { action: Action::ExecSettings, hint: "", run: |app| app.palette_exec() },
    ActionDef { action: Action::ClearConsole, hint: "⌘A ⌫ (console)", run: clear_console },
    ActionDef { action: Action::SearchEverywhere, hint: "", run: |app| app.palette_everywhere() },
    ActionDef { action: Action::GoToObject, hint: "", run: |app| app.palette_objects("Go to Database Object", vec![]) },
    ActionDef { action: Action::RecentTabs, hint: "", run: |app| app.palette_tabs() },
    ActionDef { action: Action::EditData, hint: "F4 / Enter", run: edit_data },
    ActionDef { action: Action::Ddl, hint: "", run: ddl },
    ActionDef { action: Action::Submit, hint: "", run: |app| app.submit() },
    ActionDef { action: Action::Revert, hint: "⌃z (grid)", run: revert },
    ActionDef { action: Action::AddRow, hint: "Insert / a", run: add_row },
    ActionDef { action: Action::DeleteRow, hint: "Delete / d", run: delete_row },
    ActionDef { action: Action::SetNull, hint: "n (grid)", run: set_null },
    ActionDef { action: Action::ExportCsv, hint: "x (grid)", run: |app| app.export_start() },
    ActionDef { action: Action::CopyInList, hint: "Y (grid)", run: copy_in_list },
    ActionDef { action: Action::FindSimilar, hint: "", run: |app| app.find_similar() },
    ActionDef { action: Action::RagSearch, hint: "", run: |app| app.rag_search_start() },
    ActionDef { action: Action::RagBuild, hint: "", run: |app| app.rag_build_start() },
    ActionDef { action: Action::RagNew, hint: "", run: |app| app.rag_new_start() },
    ActionDef { action: Action::RagKey, hint: "", run: |app| app.rag_key_start() },
    ActionDef { action: Action::WsSwitch, hint: "⌥1…9", run: |app| app.ws_pick(false) },
    ActionDef { action: Action::WsNew, hint: "", run: |app| app.ws_new() },
    ActionDef { action: Action::WsRename, hint: "", run: |app| app.ws_rename() },
    ActionDef { action: Action::WsDelete, hint: "", run: |app| app.ws_delete() },
    ActionDef { action: Action::WsMoveTab, hint: "", run: |app| app.ws_pick(true) },
    ActionDef { action: Action::WsAddConn, hint: "", run: |app| app.ws_add_conn_pick() },
    ActionDef { action: Action::WsMoveConn, hint: "w (explorer)", run: |app| app.ws_move_conn_pick() },
    ActionDef { action: Action::WsRemoveConn, hint: "", run: |app| app.ws_remove_conn() },
    ActionDef { action: Action::Find, hint: "", run: |app| app.find_open() },
    ActionDef { action: Action::QueryHistory, hint: "", run: |app| app.history_palette() },
    ActionDef { action: Action::ProductionLog, hint: "", run: |app| app.production_log() },
    ActionDef { action: Action::Snippets, hint: "", run: |app| app.snippets_palette() },
    ActionDef { action: Action::SaveSnippet, hint: "", run: |app| app.save_snippet() },
    ActionDef { action: Action::ShowDiagram, hint: "", run: |app| app.erd_open() },
    ActionDef { action: Action::CompareSchemas, hint: "", run: |app| app.compare_schemas_start() },
    ActionDef { action: Action::CompareData, hint: "", run: |app| app.compare_data_start() },
    ActionDef { action: Action::RunMigrations, hint: "", run: |app| app.migrations_start() },
    ActionDef { action: Action::BackupDb, hint: "", run: |app| app.backup_start() },
    ActionDef { action: Action::RestoreDb, hint: "", run: |app| app.restore_start() },
    ActionDef { action: Action::DbaReports, hint: "", run: |app| app.dba_palette() },
    ActionDef { action: Action::ImportData, hint: "", run: |app| app.import_start() },
    ActionDef { action: Action::GenerateTestData, hint: "", run: |app| app.generate_data_start() },
    ActionDef { action: Action::NewTable, hint: "", run: |app| app.new_table() },
    ActionDef { action: Action::ModifyTable, hint: "", run: |app| app.modify_table() },
    ActionDef { action: Action::NewView, hint: "", run: |app| app.new_object("view") },
    ActionDef { action: Action::NewSchema, hint: "", run: |app| app.new_object("schema") },
    ActionDef { action: Action::NewDatabase, hint: "", run: |app| app.new_object("database") },
    ActionDef { action: Action::NewSequence, hint: "", run: |app| app.new_object("sequence") },
    ActionDef { action: Action::RenameObject, hint: "", run: |app| app.rename_object() },
    ActionDef { action: Action::ChartResult, hint: "", run: |app| app.chart_result() },
    ActionDef { action: Action::CopyRelatedRows, hint: "", run: |app| app.copy_rows_start() },
    ActionDef { action: Action::CancelTransfer, hint: "", run: cancel_transfer },
    ActionDef { action: Action::NewDataSource, hint: "n (explorer)", run: |app| app.conn_form(None) },
    ActionDef { action: Action::EditDataSource, hint: "e (explorer)", run: edit_data_source },
    ActionDef { action: Action::DeleteDataSource, hint: "D (explorer)", run: delete_data_source },
    ActionDef { action: Action::Connect, hint: "c (explorer)", run: connect },
    ActionDef { action: Action::Disconnect, hint: "", run: disconnect },
    ActionDef { action: Action::Refresh, hint: "r (explorer · table · Redis)", run: refresh },
    ActionDef { action: Action::ManageSchemas, hint: "s (explorer)", run: manage_schemas },
    ActionDef { action: Action::CloseTab, hint: "", run: |app| app.close_tab(app.active) },
    ActionDef { action: Action::NextTab, hint: "", run: |app| app.cycle_tab(1) },
    ActionDef { action: Action::PrevTab, hint: "", run: |app| app.cycle_tab(-1) },
    ActionDef { action: Action::ToggleExplorer, hint: "", run: toggle_explorer },
    ActionDef { action: Action::ToggleOutput, hint: "", run: toggle_output },
    ActionDef { action: Action::Complete, hint: "", run: complete },
    ActionDef { action: Action::SplitRight, hint: "", run: |app| app.split_focused(SplitDir::Right) },
    ActionDef { action: Action::SplitDown, hint: "", run: |app| app.split_focused(SplitDir::Down) },
    ActionDef { action: Action::Unsplit, hint: "", run: |app| app.unsplit() },
    ActionDef { action: Action::OtherSplit, hint: "", run: other_split },
    ActionDef { action: Action::SplitShrink, hint: "", run: |app| resize_split(app, -5) },
    ActionDef { action: Action::SplitGrow, hint: "", run: |app| resize_split(app, 5) },
    ActionDef { action: Action::ExplorerNarrower, hint: "", run: |app| resize_explorer(app, -4) },
    ActionDef { action: Action::ExplorerWider, hint: "", run: |app| resize_explorer(app, 4) },
    ActionDef { action: Action::ShrinkEditor, hint: "", run: |app| app.resize_console(-5) },
    ActionDef { action: Action::GrowEditor, hint: "", run: |app| app.resize_console(5) },
    ActionDef { action: Action::PaneUp, hint: "", run: |app| app.focus_pane(SplitDir::Down, true) },
    ActionDef { action: Action::PaneDown, hint: "", run: |app| app.focus_pane(SplitDir::Down, false) },
    ActionDef { action: Action::PaneLeft, hint: "", run: |app| app.focus_pane(SplitDir::Right, true) },
    ActionDef { action: Action::PaneRight, hint: "", run: |app| app.focus_pane(SplitDir::Right, false) },
    ActionDef { action: Action::CycleFocus, hint: "", run: cycle_focus },
    ActionDef { action: Action::Palette, hint: "", run: |app| app.palette_actions() },
    ActionDef { action: Action::TrustSshHost, hint: "", run: trust_ssh_host },
    ActionDef { action: Action::PasswordKey, hint: "", run: |app| app.password_key() },
    ActionDef { action: Action::GridAlign, hint: "", run: |app| app.palette_grid_align(qurry_core::format::ValueClass::String) },
    ActionDef { action: Action::CheckUpdate, hint: "", run: |app| app.check_update() },
    ActionDef { action: Action::UpdateChannel, hint: "", run: |app| app.palette_update_channel() },
    ActionDef { action: Action::InstallUpdate, hint: "", run: install_update },
    ActionDef { action: Action::Help, hint: "", run: |app| app.popup = Some(Popup::Help { sel: 0, scroll: 0, record: None, message: None }) },
    ActionDef { action: Action::Quit, hint: "", run: quit },
];

fn new_console(app: &mut App) {
    let sel = app.tree.selected();
    let tab_conn = app.tabs.get(app.active).map(|t| (t.conn.clone(), t.db.clone()));
    if let Some(n) = sel.as_ref().filter(|_| app.focus == Focus::Tree).or(None) {
        app.open_console(&n.conn.clone(), n.db.clone())
    } else if let Some((c, d)) = tab_conn {
        app.open_console(&c, Some(d))
    } else if let Some(n) = sel {
        app.open_console(&n.conn.clone(), n.db.clone())
    }
}

fn edit_data_source(app: &mut App) {
    if let Some(c) = app.tree.selected().and_then(|n| app.conn(&n.conn).map(|c| c.config.clone())) {
        app.conn_form(Some(c))
    }
}

fn delete_data_source(app: &mut App) {
    if let Some(c) = app.tree.selected().and_then(|n| app.conn(&n.conn).map(|c| c.config.clone())) {
        app.popup = Some(Popup::Confirm {
            opened: None,
            text: format!("Remove data source \"{}\"?", c.name),
            on_yes: Box::new(move |a| {
                let st = a.state.clone();
                let id = c.id.clone();
                a.spawn(async move { st.delete_connection(id).await.map_err(err) }, |a, r| {
                    if let Err(e) = r {
                        a.status = e;
                    }
                    a.reload_conns();
                });
            }),
        });
    }
}

fn connect(app: &mut App) {
    if let Some(n) = app.tree.selected() {
        app.connect(&n.conn)
    }
}

fn disconnect(app: &mut App) {
    if let Some(n) = app.tree.selected() {
        app.disconnect(&n.conn)
    }
}

fn refresh(app: &mut App) {
    match (app.focus, app.tree.selected()) {
        (Focus::Tree, Some(n)) => app.refresh_node(&n),
        _ => {
            if let Some(t) = app.tabs.get(app.active) {
                let id = t.id;
                match t.kind {
                    TabKind::Table(_) => app.reload_table_checked(id),
                    TabKind::Redis(_) => app.redis_scan(id, true),
                    _ => {}
                }
            }
        }
    }
}

fn explain_analyze(app: &mut App) {
    app.analyze_next = true;
    app.execute(true);
    app.analyze_next = false;
}

fn clear_console(app: &mut App) {
    if let Some(TabKind::Console(c)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
        c.editor.select_all();
        c.editor.cut();
        c.complete = None;
        app.focus = Focus::Editor;
    }
}

fn ddl(app: &mut App) {
    // In a table data tab, that table's DDL
    if app.focus != Focus::Tree {
        if let Some(Tab { kind: TabKind::Table(tt), conn, db, .. }) = app.tabs.get(app.active) {
            return app.open_ddl(&conn.clone(), db.clone(), tt.schema.clone(), tt.table.clone(), tt.kind.clone());
        }
        app.status = "Go to DDL: select an object in the explorer or open a table".into();
        return;
    }
    if let Some(n) = app.tree.selected().filter(|n| matches!(n.kind, NodeKind::Object(_))) {
        let k = match &n.kind {
            NodeKind::Object(k) if k == "function" || k == "procedure" => "routine".to_string(),
            NodeKind::Object(k) => k.clone(),
            _ => unreachable!(),
        };
        app.open_ddl(&n.conn, n.db.unwrap(), n.schema.unwrap(), n.name.unwrap(), k)
    }
}

fn edit_data(app: &mut App) {
    if let Some(n) = app.tree.selected() {
        app.activate_node(&n, true)
    }
}

fn revert(app: &mut App) {
    if let Some(g) = app.grid_mut() {
        g.revert();
    }
}

fn add_row(app: &mut App) {
    if let Some(Tab { kind: TabKind::Table(tt), .. }) = app.tabs.get_mut(app.active) {
        if let Some(g) = &mut tt.grid {
            g.add_row();
        }
    }
}

fn delete_row(app: &mut App) {
    if let Some(Tab { kind: TabKind::Table(tt), .. }) = app.tabs.get_mut(app.active) {
        if let Some(g) = &mut tt.grid {
            let r = g.row;
            if g.is_added(r) {
                g.result.rows.remove(r);
                g.added -= 1;
                g.edits.retain(|(rr, _), _| *rr != r);
                g.row = g.row.min(g.rows().saturating_sub(1));
            } else if !g.deleted.remove(&r) {
                g.deleted.insert(r);
            }
        }
    }
}

fn set_null(app: &mut App) {
    if let Some(Tab { kind: TabKind::Table(tt), .. }) = app.tabs.get_mut(app.active) {
        if let Some(g) = &mut tt.grid {
            g.edits.insert((g.row, g.col), None);
        }
    }
}

fn copy_in_list(app: &mut App) {
    let kind = app.tabs.get(app.active).map_or(DbKind::Postgres, |t| app.kind_of(&t.conn));
    let Some(g) = app.grid_mut() else {
        app.status = "Open a result or table to copy a column".into();
        return;
    };
    let (text, n) = g.in_list(kind);
    crate::input::copy(app, &text);
    let short: String = text.chars().take(80).collect();
    app.status = format!("Copied {n} values as an IN list: {short}{}", if text.chars().count() > 80 { "…" } else { "" });
}

fn cancel_transfer(app: &mut App) {
    for (id, ..) in &app.jobs {
        app.state.cancel_job(id);
    }
}

fn toggle_explorer(app: &mut App) {
    if app.focus == Focus::Tree && app.explorer && !app.tabs.is_empty() {
        app.explorer = false;
        app.activate(app.active);
    } else {
        app.explorer = true;
        app.focus = Focus::Tree;
    }
}

fn toggle_output(app: &mut App) {
    if let Some(Tab { kind: TabKind::Console(c), .. }) = app.tabs.get_mut(app.active) {
        c.show_output = !c.show_output;
    }
}

fn other_split(app: &mut App) {
    // To the next pane (cycles from left/top in order)
    let ls = app.panes.leaves();
    if ls.len() > 1 {
        let p = ls.iter().position(|&g| g == app.focused_group()).unwrap_or(0);
        if let Some(i) = app.group_tab(ls[(p + 1) % ls.len()]) {
            app.activate(i);
        }
    }
}

fn resize_explorer(app: &mut App, d: i16) {
    let w = app.explorer_width(app.hits.main.width) as i16 + d;
    app.set_explorer_width(w);
}

fn resize_split(app: &mut App, d: i16) {
    // Move the left/right divider next to the focused pane (even if the nearest one is a top/bottom divider). Top/bottom if there's no left/right
    let g = app.focused_group();
    let path = app.panes.ancestor_with_dir(g, SplitDir::Right).or_else(|| app.panes.ancestor_with_dir(g, SplitDir::Down));
    if let Some(pct) = path.and_then(|p| app.panes.pct_mut(&p)) {
        *pct = (*pct as i16 + d).clamp(15, 85) as u16;
    }
}

fn complete(app: &mut App) {
    if matches!(app.tabs.get(app.active).map(|t| &t.kind), Some(TabKind::Console(_))) {
        app.focus = Focus::Editor;
        app.update_completion(true);
    }
}

fn cycle_focus(app: &mut App) {
    app.focus = match app.focus {
        Focus::Tree if !app.tabs.is_empty() => match app.tabs[app.active].kind {
            TabKind::Console(_) => Focus::Editor,
            _ => Focus::Grid,
        },
        Focus::Editor => Focus::Grid,
        _ => Focus::Tree,
    };
    if app.focus == Focus::Tree {
        app.explorer = true;
    }
}

fn manage_schemas(app: &mut App) {
    match app.tree.selected() {
        Some(n) if app.kind_of(&n.conn) != DbKind::Redis => app.manage_schemas(&n.conn, n.db.clone()),
        _ => app.status = "Select a PostgreSQL / MySQL data source in the explorer".into(),
    }
}

fn install_update(app: &mut App) {
    let n = app.pending_tabs(None).len();
    if n > 0 {
        app.popup = Some(Popup::Confirm { opened: None, text: format!("{n} console(s) have uncommitted changes. Roll them back, update and restart?"), on_yes: Box::new(|a| a.install_update()) });
    } else {
        app.install_update();
    }
}

fn quit(app: &mut App) {
    let n = app.pending_tabs(None).len();
    if n > 0 {
        app.popup = Some(Popup::Confirm { opened: None, text: format!("{n} console(s) have uncommitted changes. Roll them back and quit?"), on_yes: Box::new(|a| a.quit = true) });
    } else {
        app.quit = true;
    }
}

fn trust_ssh_host(app: &mut App) {
    let ssh = app.tree.selected().as_ref().and_then(|n| app.conn(&n.conn)).and_then(|c| c.config.active_ssh().cloned());
    match ssh {
        Some(s) => app.status = app.state.trust_ssh_host(s.host, s.port).unwrap_or_else(|e| e.to_string()),
        None => app.status = "Select a data source that uses an SSH tunnel".into(),
    }
}
