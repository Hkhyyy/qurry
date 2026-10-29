#[cfg(target_os = "macos")]
mod bundle;
mod commands;
// 엔진은 qurry-core (TUI·MCP 와 공유)
pub use qurry_core::{app, db, error, meta, redisdb, secrets, store};

use tauri::Manager;

pub(crate) fn app_context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // 기본 메뉴의 "Close Window(⌘W)"가 탭 닫기 단축키를 가로채지 않도록 메뉴를 직접 구성
            use tauri::menu::{MenuBuilder, SubmenuBuilder};
            let check_updates = tauri::menu::MenuItemBuilder::with_id("check-update", "Check for Updates…").build(app)?;
            // Settings (⌘, 는 웹뷰 키맵이 처리 — 메뉴 가속키를 달면 이중 실행)
            let settings = tauri::menu::MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
            let app_menu = SubmenuBuilder::new(app, "Qurry")
                .about(None)
                .item(&check_updates)
                .separator()
                .item(&settings)
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;
            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            let window_menu = SubmenuBuilder::new(app, "Window").minimize().fullscreen().build()?;
            // Help › Keymap Reference (단축키는 웹뷰에서 ⇧⌘/ 로 처리 — 메뉴 가속키를 달면 이중 실행)
            let keymap = tauri::menu::MenuItemBuilder::with_id("keymap", "Keymap Reference").build(app)?;
            let help_menu = SubmenuBuilder::new(app, "Help").item(&keymap).build()?;
            app.set_menu(MenuBuilder::new(app).items(&[&app_menu, &edit_menu, &window_menu, &help_menu]).build()?)?;
            app.on_menu_event(|app, ev| {
                use tauri::Emitter;
                if ev.id() == "keymap" {
                    let _ = app.emit("open-keymap", ());
                } else if ev.id() == "settings" {
                    let _ = app.emit("open-settings", ());
                } else if ev.id() == "check-update" {
                    let _ = app.emit("check-update", ());
                }
            });

            let dir = app.path().app_config_dir()?;
            let secret_file = secrets::SecretFile::open(&dir)?;
            app.manage(app::AppState::new(store::Store::new(dir), secret_file));
            app.manage(commands::PendingImport::default());
            app.manage(commands::PendingData::default());
            app.manage(commands::PickedDirs::default());
            // keep-alive / auto-disconnect
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let state = handle.state::<app::AppState>();
                    for id in app::housekeeping(&state).await {
                        eprintln!("[qurry] auto-disconnect {id}");
                        let _ = handle.emit("conn-disconnected", id);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::test_ssh,
            commands::connect,
            commands::disconnect,
            commands::run_query,
            commands::export_connections,
            commands::export_data,
            commands::cancel_job,
            commands::dba_reports,
            commands::driver_status,
            commands::install_driver,
            commands::backup_db,
            commands::restore_db,
            commands::compare_schemas,
            commands::compare_data,
            commands::pick_folder,
            commands::migration_status,
            commands::apply_migrations,
            commands::save_migration,
            commands::erd,
            commands::save_file,
            commands::preview_impact,
            commands::fake_preview,
            commands::fake_fill,
            commands::subset_preview,
            commands::subset_copy,
            commands::table_def,
            commands::table_ddl,
            commands::new_object_ddl,
            commands::rename_ddl,
            commands::run_ddl,
            commands::history_add,
            commands::history_list,
            commands::history_delete,
            commands::history_import,
            commands::audit_log,
            commands::snippets,
            commands::snippet_save,
            commands::snippet_delete,
            commands::analyze_plan,
            commands::dba_report,
            commands::kill_session,
            commands::dba_action_sql,
            commands::pick_data_file,
            commands::import_data,
            commands::save_text_file,
            commands::pick_import_file,
            commands::import_picked,
            commands::tx_state,
            commands::trust_ssh_host,
            commands::unsafe_reason,
            commands::list_databases,
            commands::list_schemas,
            commands::list_objects,
            commands::table_detail,
            commands::similar_rows_sql,
            commands::rag_indexes,
            commands::rag_save,
            commands::rag_delete,
            commands::rag_has_key,
            commands::rag_set_key,
            commands::rag_build,
            commands::rag_search,
            commands::completion_meta,
            commands::column_hints,
            commands::object_ddl,
            commands::fetch_more,
            commands::cancel_query,
            commands::close_session,
            commands::set_tx_mode,
            commands::commit_tx,
            commands::rollback_tx,
            commands::execute_batch,
            commands::object_index,
            commands::cached_index,
            commands::warm_search_cache,
            commands::full_text_search,
            commands::redis_scan,
            commands::redis_key,
            commands::redis_exec,
            commands::redis_exec_line,
            commands::load_keymap,
            commands::load_workspace,
            commands::save_workspace,
            commands::save_keymap,
            commands::check_update,
            commands::install_update,
        ])
        .run(app_context())
        .expect("error while running tauri application");
}
