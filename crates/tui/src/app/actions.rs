//! The TUI's commands: one ActionDef per Action, with what it runs. Ids and titles come from the shared command table (qurry_shared::keymap)
use super::*;

pub struct ActionDef {
    pub action: Action,
    /// Palette hint: keys of the screen that run it without being a command key, shown while it has no key in the keymap (Keymap::hint)
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
    ActionDef { action: Action::QuickFix, hint: "", run: |app| app.quick_fix() },
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
    ActionDef { action: Action::Revert, hint: "", run: revert },
    ActionDef { action: Action::AddRow, hint: "", run: add_row },
    ActionDef { action: Action::DeleteRow, hint: "", run: delete_row },
    ActionDef { action: Action::SetNull, hint: "", run: set_null },
    ActionDef { action: Action::ExportCsv, hint: "", run: |app| app.export_start() },
    ActionDef { action: Action::CopyInList, hint: "", run: copy_in_list },
    ActionDef { action: Action::MarkRow, hint: "", run: mark_row },
    ActionDef { action: Action::ValueEditor, hint: "", run: toggle_value_panel },
    ActionDef { action: Action::ColumnNarrower, hint: "", run: |app| app.resize_columns(Resize::Step(-1)) },
    ActionDef { action: Action::ColumnWider, hint: "", run: |app| app.resize_columns(Resize::Step(1)) },
    ActionDef { action: Action::ColumnFit, hint: "", run: |app| app.resize_columns(Resize::Fit) },
    ActionDef { action: Action::ColumnFitAll, hint: "", run: |app| app.resize_columns(Resize::FitAll) },
    ActionDef { action: Action::ColumnReset, hint: "", run: |app| app.resize_columns(Resize::Reset) },
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
    ActionDef { action: Action::WsMoveConn, hint: "", run: |app| app.ws_move_conn_pick() },
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
    ActionDef { action: Action::ImportDataSources, hint: "", run: |app| app.ds_import_start() },
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
    ActionDef { action: Action::NewDataSource, hint: "", run: |app| app.conn_form(None) },
    ActionDef { action: Action::EditDataSource, hint: "", run: edit_data_source },
    ActionDef { action: Action::DeleteDataSource, hint: "", run: delete_data_source },
    ActionDef { action: Action::Connect, hint: "", run: connect },
    ActionDef { action: Action::Disconnect, hint: "", run: disconnect },
    ActionDef { action: Action::Refresh, hint: "", run: refresh },
    ActionDef { action: Action::MarkNode, hint: "", run: |app| app.mark_node() },
    ActionDef { action: Action::DropObject, hint: "", run: |app| app.drop_objects() },
    ActionDef { action: Action::ManageSchemas, hint: "", run: manage_schemas },
    ActionDef { action: Action::CloseTab, hint: "", run: close_tab },
    ActionDef { action: Action::NextTab, hint: "", run: |app| app.cycle_tab(1) },
    ActionDef { action: Action::PrevTab, hint: "", run: |app| app.cycle_tab(-1) },
    ActionDef { action: Action::ToggleExplorer, hint: "", run: toggle_explorer },
    ActionDef { action: Action::ToggleOutput, hint: "", run: toggle_output },
    ActionDef { action: Action::Complete, hint: "", run: complete },
    ActionDef { action: Action::Reformat, hint: "", run: |app| app.reformat() },
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
    ActionDef { action: Action::KeywordCase, hint: "", run: |app| app.palette_keyword_case() },
    ActionDef { action: Action::GridAlign, hint: "", run: |app| app.palette_grid_align(qurry_core::format::ValueClass::String) },
    ActionDef { action: Action::StatusBarItems, hint: "", run: |app| app.palette_status_items(0) },
    ActionDef { action: Action::Theme, hint: "", run: |app| app.palette_theme() },
    ActionDef { action: Action::CheckUpdate, hint: "", run: |app| app.check_update() },
    ActionDef { action: Action::UpdateChannel, hint: "", run: |app| app.palette_update_channel() },
    ActionDef { action: Action::InstallUpdate, hint: "", run: install_update },
    ActionDef { action: Action::OpenGitHub, hint: "", run: open_github },
    ActionDef { action: Action::Help, hint: "", run: |app| app.popup = Some(Popup::Help { sel: 0, scroll: 0, record: None, message: None }) },
    ActionDef { action: Action::ShowLog, hint: "", run: show_log },
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
        (Focus::Tree, Some(_)) => {
            for n in app.tree.targets() {
                app.refresh_node(&n)
            }
        }
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
    for n in app.tree_targets("Go to DDL", |n| matches!(n.kind, NodeKind::Object(_))).unwrap_or_default() {
        let k = match &n.kind {
            NodeKind::Object(k) if k == "function" || k == "procedure" => "routine".to_string(),
            NodeKind::Object(k) => k.clone(),
            _ => unreachable!(),
        };
        app.open_ddl(&n.conn, n.db.unwrap(), n.schema.unwrap(), n.name.unwrap(), k)
    }
}

/// F4 / ⌃↓: the cursor's node, or every marked object (a table tab each)
fn edit_data(app: &mut App) {
    let marks = !app.tree.marked.is_empty();
    for n in app.tree_targets("Open", |n| !marks || matches!(n.kind, NodeKind::Object(_))).unwrap_or_default() {
        app.activate_node(&n, true)
    }
}

fn revert(app: &mut App) {
    if let Some(g) = app.grid_mut() {
        g.revert();
    }
}

// Table data and console results from one table with its key, not a join's (App::edit_rows; otherwise the status line says why)
fn add_row(app: &mut App) {
    if let Some(g) = app.edit_rows() {
        g.add_row();
    }
}

fn delete_row(app: &mut App) {
    if let Some(g) = app.edit_rows() {
        g.delete_rows();
    }
}

/// m in a result or table grid: the current row into the selection or out of it (the GUI's ⌘-click on a row number)
fn mark_row(app: &mut App) {
    if !matches!(app.tabs.get(app.active).map(|t| &t.kind), Some(TabKind::Table(_) | TabKind::Console(_))) {
        return;
    }
    let table = matches!(app.edit_to(), Ok(crate::app::tables::EditTo::Table(_)));
    let Some(g) = app.grid_mut() else { return };
    g.toggle_mark();
    let n = g.marked.len();
    app.status = match n {
        0 => "No rows marked".into(),
        _ => format!("{n} row{} marked: y copies{} · Esc clears", if n == 1 { "" } else { "s" }, if table { ", d deletes them" } else { " them" }),
    };
}

fn set_null(app: &mut App) {
    if let Some(g) = app.edit_grid() {
        g.edits.insert((g.row, g.col), None);
    }
}

/// v in a result or table grid: the value panel beside it, on or off
fn toggle_value_panel(app: &mut App) {
    if matches!(app.tabs.get(app.active).map(|t| &t.kind), Some(TabKind::Table(_) | TabKind::Console(_))) && app.grid_mut().is_some() {
        app.value_panel = !app.value_panel;
    } else {
        app.status = "Open a result or table to see a value in the value panel".into();
    }
}

/// ⌃w: from a grid with the value panel open, closes the panel first (as the GUI's ⌘w), then the tab
fn close_tab(app: &mut App) {
    if app.value_panel && app.focus == Focus::Grid && matches!(app.tabs.get(app.active).map(|t| &t.kind), Some(TabKind::Table(_) | TabKind::Console(_))) {
        app.value_panel = false;
        return;
    }
    app.close_tab(app.active)
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

/// Reveals the log file in Finder (`open -R`; its folder until the file exists), or opens the folder on Linux (`xdg-open`): fixed argv,
/// no shell. The status line shows the file's path; over SSH (and in tests) nothing opens, the path is what matters
fn show_log(app: &mut App) {
    let file = app.log_file.clone();
    let dir = file.parent().unwrap_or(&file);
    app.status = format!("Show Log: {}", file.display());
    let ssh = std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
    if cfg!(test) || ssh {
        return;
    }
    let mut cmd = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
    match cfg!(target_os = "macos") && file.exists() {
        true => cmd.arg("-R").arg(&file),
        false => cmd.arg(dir),
    };
    let null = std::process::Stdio::null;
    match std::fs::create_dir_all(dir).and_then(|_| cmd.stdin(null()).stdout(null()).stderr(null()).spawn()) {
        // Reaped off the UI thread so it doesn't linger as a zombie
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => app.status = format!("Show Log: couldn't open it ({e}). Open it yourself: {}", file.display()),
    }
}

/// The project page ⌃k › Open GitHub opens (the GUI's Settings › Updates › Open GitHub)
pub const REPO_URL: &str = "https://github.com/Hkhyyy/qurry";

/// Opens REPO_URL in the browser and always puts the address on the status line: over SSH no browser may open, and a started
/// opener (xdg-open without a display) can still fail, so success says "Opening", not "Opened"
fn open_github(app: &mut App) {
    app.status = match spawn_opener(REPO_URL) {
        Ok(()) => format!("Opening {REPO_URL} in the browser"),
        // The address before the reason: a long reason is what gets cut on a narrow status line
        Err(e) => format!("Couldn't open a browser: {REPO_URL} ({e})"),
    };
}

#[cfg(test)]
thread_local! {
    /// Tests: what starting the browser gives (None: it started); nothing is launched
    pub static OPENER_FAILS: std::cell::Cell<Option<std::io::ErrorKind>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn spawn_opener(_url: &str) -> std::io::Result<()> {
    OPENER_FAILS.get().map_or(Ok(()), |k| Err(k.into()))
}
/// Starts the platform's opener for a URL: fixed argv, no shell (on Windows only cmd's built-in `start`)
#[cfg(not(test))]
fn spawn_opener(url: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        Command::new("xdg-open")
    };
    let mut child = cmd.arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    // Reaped off the UI thread so it doesn't linger as a zombie
    drop(std::thread::spawn(move || child.wait()));
    Ok(())
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
