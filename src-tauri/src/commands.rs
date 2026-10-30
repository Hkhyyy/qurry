//! Tauri commands called from the frontend. The actual logic lives in qurry_core::app::AppState.
use crate::app::{AppState, ConnectionView, ImportResult};
use crate::db::QueryResult;
use crate::error::AppResult;
use crate::store::ConnectionConfig;
use qurry_core::transfer::{Dest, Format, ImportSpec, Preview};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> AppResult<Vec<ConnectionView>> {
    state.list_connections().await
}

/// `password`: None keeps the existing password, an empty string deletes it.
#[tauri::command]
pub async fn save_connection(state: State<'_, AppState>, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<ConnectionConfig> {
    state.save_connection(config, password, ssh_secret).await
}

/// Explorer menu: change only the color (other settings are read from the store, not the possibly stale list)
#[tauri::command]
pub async fn set_connection_color(state: State<'_, AppState>, id: String, color: Option<String>) -> AppResult<ConnectionConfig> {
    state.set_connection_color(id, color).await
}

/// Explorer menu: change only the shown databases and schemas
#[tauri::command]
pub async fn set_connection_scope(state: State<'_, AppState>, id: String, scope: qurry_core::store::Scope) -> AppResult<ConnectionConfig> {
    state.set_connection_scope(id, scope).await
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.delete_connection(id).await
}

/// Only tests the connection without saving. If `password` is None, the saved password is used.
#[tauri::command]
pub async fn test_connection(state: State<'_, AppState>, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<String> {
    state.test_connection(config, password, ssh_secret).await
}

/// Test Connection on the SSH/SSL tab: checks the SSH login only.
#[tauri::command]
pub async fn test_ssh(state: State<'_, AppState>, config: ConnectionConfig, ssh_secret: Option<String>) -> AppResult<String> {
    state.test_ssh(config, ssh_secret).await
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, id: String) -> AppResult<String> {
    state.connect(id).await
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.disconnect(id).await
}

/// If `tab_id` is given, runs in that console's session (supports cursors and cancel).
#[tauri::command]
pub async fn run_query(state: State<'_, AppState>, id: String, sql: String, limit: Option<usize>, tab_id: Option<String>) -> AppResult<QueryResult> {
    state.run_query(id, sql, limit, tab_id).await
}

#[tauri::command]
pub async fn fetch_more(state: State<'_, AppState>, tab_id: String, limit: Option<usize>) -> AppResult<QueryResult> {
    state.fetch_more(tab_id, limit).await
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, tab_id: String) -> AppResult<bool> {
    state.cancel_query(tab_id).await
}

/// Tx: Auto / Manual
#[tauri::command]
pub async fn set_tx_mode(state: State<'_, AppState>, id: String, tab_id: String, manual: bool) -> AppResult<()> {
    state.set_tx_mode(id, tab_id, manual).await
}

#[tauri::command]
pub async fn commit_tx(state: State<'_, AppState>, tab_id: String) -> AppResult<()> {
    state.commit_tx(tab_id).await
}

#[tauri::command]
pub async fn rollback_tx(state: State<'_, AppState>, tab_id: String) -> AppResult<()> {
    state.rollback_tx(tab_id).await
}

#[tauri::command]
pub async fn close_session(state: State<'_, AppState>, tab_id: String) -> AppResult<()> {
    state.close_session(tab_id).await
}

#[tauri::command]
/// exact_one = saving a table edit (roll everything back unless UPDATE/DELETE hits exactly 1 row)
pub async fn execute_batch(state: State<'_, AppState>, id: String, statements: Vec<String>, exact_one: Option<bool>) -> AppResult<u64> {
    state.execute_batch(id, statements, exact_one.unwrap_or(false)).await
}

/// Fill the search cache after connecting (the frontend doesn't wait)
#[tauri::command]
pub async fn warm_search_cache(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.warm_search_cache(id).await
}

/// Saved object/column lists so Search Everywhere finds Data Sources that aren't connected too
#[tauri::command]
pub fn cached_index(state: State<'_, AppState>) -> AppResult<Vec<qurry_core::search_cache::CachedIndex>> {
    state.cached_index()
}

#[tauri::command]
pub async fn object_index(state: State<'_, AppState>, id: String) -> AppResult<Vec<crate::meta::IndexEntry>> {
    state.object_index(id).await
}

#[tauri::command]
pub async fn full_text_search(state: State<'_, AppState>, id: String, req: crate::meta::FtsRequest) -> AppResult<Vec<crate::meta::FtsHit>> {
    state.full_text_search(id, req).await
}

#[tauri::command]
pub async fn redis_scan(state: State<'_, AppState>, id: String, cursor: String, pattern: String, count: usize, type_filter: Option<String>) -> AppResult<crate::redisdb::ScanPage> {
    state.redis_scan(id, cursor, pattern, count, type_filter).await
}

#[tauri::command]
pub async fn redis_key(state: State<'_, AppState>, id: String, key: String, limit: Option<usize>) -> AppResult<crate::redisdb::KeyDetail> {
    state.redis_key(id, key, limit).await
}

/// Terminal: one line as is, like redis-cli
#[tauri::command]
pub async fn redis_exec_line(state: State<'_, AppState>, id: String, line: String) -> AppResult<serde_json::Value> {
    state.redis_exec_line(id, line).await
}

#[tauri::command]
pub async fn redis_exec(state: State<'_, AppState>, id: String, args: Vec<String>) -> AppResult<serde_json::Value> {
    state.redis_exec(id, args).await
}

#[tauri::command]
pub async fn export_connections(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<Option<(usize, String)>> {
    let Some(path) = save_dialog(&app, "qurry-datasources.json", "JSON", &["json"]) else { return Ok(None) };
    Ok(Some((state.export_connections(path.clone()).await?, path)))
}

/// Pick a file to import → preview what changes. The actual import is import_picked (after the user confirms)
#[tauri::command]
pub async fn pick_import_file(app: tauri::AppHandle, state: State<'_, AppState>, pending: State<'_, PendingImport>) -> AppResult<Option<Vec<String>>> {
    let picked = app.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let lines = state.import_preview(path.display().to_string())?;
    *pending.0.lock().unwrap() = Some(path);
    Ok(Some(lines))
}

#[tauri::command]
pub async fn import_picked(state: State<'_, AppState>, pending: State<'_, PendingImport>) -> AppResult<ImportResult> {
    let path = pending.0.lock().unwrap().take().ok_or_else(|| crate::error::AppError::Msg("Choose a file first".into()))?;
    state.import_connections(path.display().to_string()).await
}

/// The picked import file (between preview and the actual import)
#[derive(Default)]
pub struct PendingImport(pub std::sync::Mutex<Option<std::path::PathBuf>>);

/// The save dialog is opened here (backend): write only to the path the user picked, so the webview can't pass an arbitrary path to write
fn save_dialog(app: &tauri::AppHandle, default_name: &str, filter: &str, exts: &[&str]) -> Option<String> {
    app.dialog().file().set_file_name(default_name).add_filter(filter, exts).blocking_save_file().and_then(|p| p.into_path().ok()).map(|p| p.display().to_string())
}

/// SQL Generator·DDL "Save to File" → the saved path (None if cancelled)
#[tauri::command]
pub async fn save_text_file(app: tauri::AppHandle, default_name: String, text: String) -> AppResult<Option<String>> {
    let Some(path) = save_dialog(&app, &default_name, "SQL", &["sql"]) else { return Ok(None) };
    std::fs::write(&path, text)?;
    Ok(Some(path))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDone {
    rows: u64,
    /// Path, if written to a file
    path: Option<String>,
    /// Text, if clipboard (the frontend puts it on the clipboard)
    text: Option<String>,
}

/// Register the job and report progress while it runs ("transfer-progress": job, rows, done, total). Abort when finished
fn report_progress(app: &tauri::AppHandle, state: &AppState, job: &str) -> tokio::task::JoinHandle<()> {
    use tauri::Emitter;
    let (app, job, j) = (app.clone(), job.to_string(), state.start_job(job));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let (rows, done, total) = j.progress();
            let _ = app.emit("transfer-progress", (&job, rows, done, total));
        }
    })
}

/// Data export: pick a format, to a file (save dialog) or the clipboard. None if the save dialog is closed
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn export_data(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    sql: String,
    format: Format,
    clipboard: bool,
    default_name: String,
    table: Option<(String, String)>,
    job: String,
) -> AppResult<Option<ExportDone>> {
    let dest = if clipboard {
        Dest::Clipboard
    } else {
        let name = format!("{default_name}.{}", format.ext());
        let Some(path) = save_dialog(&app, &name, format.label(), &[format.ext()]) else { return Ok(None) };
        Dest::File(path.into())
    };
    let path = match &dest {
        Dest::File(p) => Some(p.display().to_string()),
        Dest::Clipboard => None,
    };
    let ticker = report_progress(&app, &state, &job);
    let r = state.export(id, sql, format, dest, table, job).await;
    ticker.abort();
    let r = r?;
    Ok(Some(ExportDone { rows: r.rows, path, text: r.text }))
}

// ───────── Query history·snippets (same files as the TUI) ─────────

#[tauri::command]
pub fn history_add(state: State<'_, AppState>, conn_id: String, database: Option<String>, sql: String) -> AppResult<()> {
    state.history.add(&conn_id, database.as_deref(), &sql)
}

#[tauri::command]
pub fn history_list(state: State<'_, AppState>, conn_id: Option<String>, filter: String, limit: usize) -> Vec<qurry_core::history::HistoryItem> {
    state.history.list(conn_id.as_deref(), &filter, limit)
}

#[tauri::command]
pub fn history_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.history.delete(&id)
}

/// Production operation log (changes on Production Data Sources · undo SQL). Most recent first
#[tauri::command]
pub fn audit_log(state: State<'_, AppState>, conn_id: Option<String>, filter: String, limit: usize) -> Vec<qurry_core::audit::AuditEntry> {
    state.audit_log(conn_id, filter, limit)
}

/// Migrate old GUI history (localStorage)
#[tauri::command]
pub fn history_import(state: State<'_, AppState>, items: Vec<qurry_core::history::HistoryItem>) -> AppResult<usize> {
    state.history.import(items)
}

#[tauri::command]
pub fn snippets(state: State<'_, AppState>, conn_id: Option<String>) -> Vec<qurry_core::history::Snippet> {
    state.history.snippets(conn_id.as_deref())
}

#[tauri::command]
pub fn snippet_save(state: State<'_, AppState>, snippet: qurry_core::history::Snippet) -> AppResult<qurry_core::history::Snippet> {
    state.history.save_snippet(snippet)
}

#[tauri::command]
pub fn snippet_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.history.delete_snippet(&id)
}

// ───────── Backup·restore (pg_dump / mysqldump) ─────────

/// Backup: save dialog → dump (progress via "transfer-progress", bytes written). None if the dialog is closed
#[tauri::command]
pub async fn backup_db(app: tauri::AppHandle, state: State<'_, AppState>, id: String, opts: qurry_core::backup::BackupOptions, default_name: String, job: String) -> AppResult<Option<(u64, String)>> {
    let (filter, ext) = if opts.format == "custom" { ("PostgreSQL dump", "dump") } else { ("SQL", "sql") };
    let Some(path) = save_dialog(&app, &format!("{default_name}.{ext}"), filter, &[ext]) else { return Ok(None) };
    let ticker = report_progress(&app, &state, &job);
    let r = state.backup(id, path.clone(), opts, job).await;
    ticker.abort();
    Ok(Some((r?, path)))
}

/// Restore: pick a file → restore it. None if nothing is picked
#[tauri::command]
pub async fn restore_db(app: tauri::AppHandle, state: State<'_, AppState>, id: String, opts: qurry_core::backup::RestoreOptions, job: String) -> AppResult<Option<String>> {
    let picked = app.dialog().file().add_filter("Backup (dump, sql)", &["dump", "sql", "backup"]).blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()) else { return Ok(None) };
    let ticker = report_progress(&app, &state, &job);
    let r = state.restore(id, path.clone(), opts, job).await;
    ticker.abort();
    r?;
    Ok(Some(path))
}

// ───────── RAG (embedding → vector store → semantic search) ─────────

#[tauri::command]
pub fn rag_indexes(state: State<'_, AppState>) -> AppResult<Vec<qurry_core::rag::RagIndex>> {
    state.rag_indexes()
}

#[tauri::command]
pub fn rag_save(state: State<'_, AppState>, index: qurry_core::rag::RagIndex) -> AppResult<qurry_core::rag::RagIndex> {
    state.rag_save(index)
}

#[tauri::command]
pub fn rag_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.rag_delete(&id)
}

/// Where the password file's master key lives: "file" | "keychain"
#[tauri::command]
pub fn secret_key_location(state: State<'_, AppState>) -> qurry_core::secrets::KeyLocation {
    state.secret_key_location()
}

#[tauri::command]
pub fn set_secret_key_location(state: State<'_, AppState>, keychain: bool) -> AppResult<qurry_core::secrets::KeyLocation> {
    state.set_secret_key_location(keychain)
}

/// Whether an OpenAI API key is saved (the key itself is never returned to the webview)
#[tauri::command]
pub fn rag_has_key(state: State<'_, AppState>) -> bool {
    state.rag_has_key()
}

#[tauri::command]
pub fn rag_set_key(state: State<'_, AppState>, key: Option<String>) -> AppResult<()> {
    state.rag_set_key(key)
}

/// Embed and insert (progress via "transfer-progress", rows)
#[tauri::command]
pub async fn rag_build(app: tauri::AppHandle, state: State<'_, AppState>, id: String, job: String) -> AppResult<u64> {
    let ticker = report_progress(&app, &state, &job);
    let r = state.rag_build(id, job).await;
    ticker.abort();
    r
}

#[tauri::command]
pub async fn rag_search(state: State<'_, AppState>, id: String, query: String, limit: usize) -> AppResult<QueryResult> {
    state.rag_search(id, query, limit).await
}

// ───────── Schema·data compare, migrations ─────────

#[tauri::command]
pub async fn compare_schemas(state: State<'_, AppState>, source_id: String, source_schema: String, target_id: String, target_schema: String) -> AppResult<Vec<qurry_core::migrate::Change>> {
    state.compare_schemas(source_id, source_schema, target_id, target_schema).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn compare_data(state: State<'_, AppState>, source_id: String, source_schema: String, source_table: String, target_id: String, target_schema: String, target_table: String) -> AppResult<qurry_core::migrate::DataDiff> {
    state.compare_data(source_id, source_schema, source_table, target_id, target_schema, target_table).await
}

/// Only use the folder picked this time (so the webview can't run SQL from any folder)
#[derive(Default)]
pub struct PickedDirs(pub std::sync::Mutex<std::collections::HashSet<String>>);

fn picked(dirs: &PickedDirs, dir: &str) -> AppResult<()> {
    if dirs.0.lock().unwrap().contains(dir) { Ok(()) } else { Err(crate::error::AppError::Msg("Choose the migrations folder first".into())) }
}

/// Pick the migrations folder
#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle, dirs: State<'_, PickedDirs>) -> AppResult<Option<String>> {
    let Some(p) = app.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let s = p.display().to_string();
    dirs.0.lock().unwrap().insert(s.clone());
    Ok(Some(s))
}

#[tauri::command]
pub async fn migration_status(state: State<'_, AppState>, dirs: State<'_, PickedDirs>, id: String, dir: String) -> AppResult<Vec<qurry_core::migrate::Migration>> {
    picked(&dirs, &dir)?;
    state.migration_status(id, dir).await
}

#[tauri::command]
pub async fn apply_migrations(state: State<'_, AppState>, dirs: State<'_, PickedDirs>, id: String, dir: String) -> AppResult<Vec<String>> {
    picked(&dirs, &dir)?;
    state.apply_migrations(id, dir).await
}

/// Write the compare result as the next version file (V{n}__{description}.sql)
#[tauri::command]
pub async fn save_migration(dirs: State<'_, PickedDirs>, dir: String, description: String, sql: String) -> AppResult<String> {
    picked(&dirs, &dir)?;
    qurry_core::migrate::save_migration(&dir, &description, &sql)
}

/// ERD: tables·relations of the schemas (+ Mermaid text)
#[tauri::command]
pub async fn erd(state: State<'_, AppState>, id: String, schemas: Vec<String>) -> AppResult<(qurry_core::erd::Erd, String)> {
    let e = state.erd(id, schemas).await?;
    let m = e.mermaid();
    Ok((e, m))
}

/// Text to a file (save dialog: name·format). None if cancelled
#[tauri::command]
pub async fn save_file(app: tauri::AppHandle, default_name: String, text: String, filter: String, ext: String) -> AppResult<Option<String>> {
    let Some(path) = save_dialog(&app, &default_name, &filter, &[ext.as_str()]) else { return Ok(None) };
    std::fs::write(&path, text)?;
    Ok(Some(path))
}

/// Test data: how to fill each column + sample rows (inserts nothing)
#[tauri::command]
pub async fn fake_preview(state: State<'_, AppState>, id: String, schema: String, table: String, rows: usize) -> AppResult<qurry_core::fake::FakePreview> {
    state.fake_preview(id, schema, table, rows).await
}

/// Insert test data in one transaction. Number of rows inserted
#[tauri::command]
pub async fn fake_fill(state: State<'_, AppState>, id: String, schema: String, table: String, rows: usize) -> AppResult<u64> {
    state.fake_fill(id, schema, table, rows).await
}

/// Partial copy of production data: what gets copied and how many rows (read only)
#[tauri::command]
pub async fn subset_preview(state: State<'_, AppState>, source: String, spec: qurry_core::subset::SubsetSpec) -> AppResult<qurry_core::subset::SubsetPlan> {
    state.subset_preview(source, spec).await
}

/// Partial copy of production data: insert into target in one transaction
#[tauri::command]
pub async fn subset_copy(state: State<'_, AppState>, source: String, spec: qurry_core::subset::SubsetSpec, target: String) -> AppResult<qurry_core::subset::SubsetPlan> {
    state.subset_copy(source, spec, target).await
}

/// Modify Table…: current structure
#[tauri::command]
pub async fn table_def(state: State<'_, AppState>, id: String, schema: String, table: String) -> AppResult<qurry_core::ddl::TableDef> {
    state.table_def(id, schema, table).await
}

/// Table DDL preview (ALTER if before is given)
#[tauri::command]
pub fn table_ddl(state: State<'_, AppState>, id: String, before: Option<qurry_core::ddl::TableDef>, after: qurry_core::ddl::TableDef) -> AppResult<Vec<String>> {
    state.table_ddl(id, before, after)
}

/// DDL for New ▸ Database · Schema · View · Sequence
#[tauri::command]
pub fn new_object_ddl(state: State<'_, AppState>, id: String, object: qurry_core::ddl::NewObject) -> AppResult<Vec<String>> {
    state.new_object_ddl(id, object)
}

/// DDL for Rename…
#[tauri::command]
pub fn rename_ddl(state: State<'_, AppState>, id: String, what: String, schema: String, old: String, new: String) -> AppResult<String> {
    state.rename_ddl(id, what, schema, old, new)
}

/// Run the generated DDL (PG: one transaction)
#[tauri::command]
pub async fn run_ddl(state: State<'_, AppState>, id: String, statements: Vec<String>) -> AppResult<()> {
    state.run_ddl(id, statements).await
}

/// Production confirmation: number of rows this statement will change
#[tauri::command]
pub async fn preview_impact(state: State<'_, AppState>, id: String, sql: String) -> AppResult<Option<i64>> {
    state.preview_impact(id, sql).await
}

/// Driver plugin: whether it's installed (None for built-in DBs)
#[tauri::command]
pub fn driver_status(kind: crate::store::DbKind) -> Option<bool> {
    kind.driver().map(|d| qurry_core::plugin::driver_path(d).is_some())
}

/// Download and install the driver (from this version's release, signature checked)
#[tauri::command]
pub async fn install_driver(kind: crate::store::DbKind) -> AppResult<String> {
    let d = kind.driver().ok_or_else(|| crate::error::AppError::Msg("This database needs no driver".into()))?;
    Ok(qurry_core::plugin::install(d).await?.display().to_string())
}

/// Execution plan (EXPLAIN FORMAT JSON) → tree + share (%)·warnings per node
#[tauri::command]
pub fn analyze_plan(json: String) -> AppResult<qurry_core::plan::PlanNode> {
    qurry_core::plan::parse_pg(&json)
}

/// DBA report list (the ones matching this Data Source's DB kind)
#[tauri::command]
pub fn dba_reports(state: State<'_, AppState>, id: String) -> AppResult<Vec<qurry_core::dba::ReportInfo>> {
    Ok(qurry_core::dba::reports(state.kind_of(&id)?))
}

#[tauri::command]
pub async fn dba_report(state: State<'_, AppState>, id: String, report: String) -> AppResult<QueryResult> {
    state.dba_report(id, report).await
}

#[tauri::command]
pub async fn kill_session(state: State<'_, AppState>, id: String, pid: i64, terminate: bool) -> AppResult<bool> {
    state.kill_session(id, pid, terminate).await
}

/// SQL for a report row action (row values in the template)
#[tauri::command]
pub fn dba_action_sql(state: State<'_, AppState>, id: String, template: String, row: std::collections::HashMap<String, String>) -> AppResult<String> {
    Ok(qurry_core::dba::action_sql(state.kind_of(&id)?, &template, &row))
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job: String) {
    state.cancel_job(&job)
}

/// Data file to import (the picked path is kept only here, so the frontend can't read other files)
#[derive(Default)]
pub struct PendingData(pub std::sync::Mutex<Option<std::path::PathBuf>>);

/// Pick a file to import → preview (first 20 rows)
#[tauri::command]
pub async fn pick_data_file(app: tauri::AppHandle, pending: State<'_, PendingData>) -> AppResult<Option<(String, Preview)>> {
    let picked = app.dialog().file().add_filter("CSV, TSV, JSON, Excel", &["csv", "tsv", "json", "xlsx", "xlsm", "xls", "ods"]).blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let pv = qurry_core::transfer::preview(&path, 20)?;
    *pending.0.lock().unwrap() = Some(path.clone());
    Ok(Some((path.display().to_string(), pv)))
}

/// Insert the picked file into the table. Number of rows inserted
#[tauri::command]
pub async fn import_data(app: tauri::AppHandle, state: State<'_, AppState>, pending: State<'_, PendingData>, id: String, mut spec: ImportSpec, job: String) -> AppResult<u64> {
    let path = pending.0.lock().unwrap().clone().ok_or_else(|| crate::error::AppError::Msg("Pick a file first".into()))?;
    spec.path = path.display().to_string();
    let ticker = report_progress(&app, &state, &job);
    let r = state.import(id, spec, job).await;
    ticker.abort();
    r
}

/// Whether the tab session has uncommitted changes (shows Commit/Roll back after an error)
#[tauri::command]
pub async fn tx_state(state: State<'_, AppState>, tab_id: String) -> AppResult<bool> {
    Ok(state.tx_state(tab_id).await)
}

/// Trust a first-seen SSH server key (after the user checks the fingerprint)
#[tauri::command]
pub async fn trust_ssh_host(state: State<'_, AppState>, host: String, port: u16) -> AppResult<String> {
    state.trust_ssh_host(host, port)
}

/// The reason, if this is a dangerous statement needing confirmation before running (UPDATE/DELETE without WHERE, DROP, TRUNCATE, Redis FLUSH*·DEL)
#[tauri::command]
pub fn unsafe_reason(kind: crate::store::DbKind, sql: String) -> Option<String> {
    qurry_core::guard::unsafe_statement(&sql, kind)
}

#[tauri::command]
pub async fn list_databases(state: State<'_, AppState>, id: String) -> AppResult<Vec<String>> {
    state.list_databases(id).await
}

#[tauri::command]
pub async fn list_schemas(state: State<'_, AppState>, id: String) -> AppResult<Vec<crate::meta::SchemaInfo>> {
    state.list_schemas(id).await
}

#[tauri::command]
pub async fn list_objects(state: State<'_, AppState>, id: String, schema: String) -> AppResult<crate::meta::SchemaObjects> {
    state.list_objects(id, schema).await
}

#[tauri::command]
pub async fn table_detail(state: State<'_, AppState>, id: String, schema: String, table: String) -> AppResult<crate::meta::TableDetail> {
    state.table_detail(id, schema, table).await
}

/// pgvector: SQL that finds rows close to this row
#[tauri::command]
pub async fn similar_rows_sql(state: State<'_, AppState>, id: String, schema: String, table: String, column: String, pk: Vec<(String, Option<String>)>) -> AppResult<String> {
    state.similar_rows_sql(id, schema, table, column, pk).await
}

#[tauri::command]
pub async fn object_ddl(state: State<'_, AppState>, id: String, schema: String, name: String, kind: String) -> AppResult<String> {
    state.object_ddl(id, schema, name, kind).await
}

#[tauri::command]
pub async fn completion_meta(state: State<'_, AppState>, id: String) -> AppResult<Vec<crate::meta::CompletionTable>> {
    state.completion_meta(id).await
}

/// Autocomplete value suggestions: column values · json keys (from the first rows, most frequent first)
#[tauri::command]
pub async fn column_hints(state: State<'_, AppState>, id: String, req: qurry_core::hints::HintReq) -> AppResult<Vec<qurry_core::hints::Hint>> {
    state.column_hints(id, req).await
}

/// Open tabs·split layout (workspace-gui.json). Restored as is on restart
#[tauri::command]
pub fn load_workspace(state: State<'_, AppState>) -> Option<serde_json::Value> {
    state.store.workspace("gui")
}

#[tauri::command]
pub fn save_workspace(state: State<'_, AppState>, ws: serde_json::Value) -> AppResult<()> {
    state.store.save_workspace("gui", &ws)
}

/// The workspace list (name · Data Source) is shared with the TUI (workspaces.json)
#[tauri::command]
pub fn shared_workspaces(state: State<'_, AppState>) -> Option<Vec<qurry_core::workspaces::WsInfo>> {
    state.store.shared_workspaces()
}

#[tauri::command]
pub fn adopt_workspaces(state: State<'_, AppState>, local: Vec<qurry_core::workspaces::WsInfo>, synced: bool) -> AppResult<qurry_core::workspaces::Adopted> {
    state.store.adopt_workspaces(&local, synced)
}

#[tauri::command]
pub fn sync_workspaces(state: State<'_, AppState>, prev: Vec<qurry_core::workspaces::WsInfo>, next: Vec<qurry_core::workspaces::WsInfo>) -> AppResult<Vec<qurry_core::workspaces::WsInfo>> {
    state.store.sync_workspaces(&prev, &next)
}

/// User keyboard shortcut settings (the "gui" section of keymap.json)
#[tauri::command]
pub fn load_keymap(state: State<'_, AppState>) -> AppResult<crate::store::Keymap> {
    state.store.keymap("gui")
}

#[tauri::command]
pub fn save_keymap(state: State<'_, AppState>, map: crate::store::Keymap) -> AppResult<()> {
    state.store.save_keymap("gui", &map)
}

/// Update check result (current = this app's version)
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    latest: Option<qurry_core::update::Release>,
    /// Changes in versions newer than the current one (newest first)
    changes: Vec<qurry_core::update::VersionNotes>,
}

/// Check for a new version. Unless force, asks GitHub at most once an hour. Dev builds (pnpm tauri dev) only when force
#[tauri::command]
pub async fn check_update(state: State<'_, AppState>, force: bool) -> AppResult<UpdateInfo> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    if (cfg!(debug_assertions) && !force) || (qurry_core::update::disabled() && !force) {
        return Ok(UpdateInfo { current, latest: None, changes: vec![] });
    }
    let latest = qurry_core::update::check(&state.store.dir(), &current, force).await?;
    let changes = latest.as_ref().map(|r| r.changes_since(&current)).unwrap_or_default();
    Ok(UpdateInfo { current, latest, changes })
}

/// Download the new version, replace the running app, and restart (Tauri updater: the release's latest.json + signature check).
/// Sends (bytes received, total bytes) via "update-progress" while downloading
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> AppResult<()> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;
    let msg = |e: tauri_plugin_updater::Error| crate::error::AppError::Msg(format!("Update failed: {e}"));
    let Some(update) = app.updater().map_err(msg)?.check().await.map_err(msg)? else {
        return Err(crate::error::AppError::Msg("No update to install (the latest release has no in-app update for this platform — download it from the Release Page).".into()));
    };
    let mut got = 0u64;
    // download also verifies the signature
    let bytes = update
        .download(
            |n, total| {
                got += n as u64;
                let _ = app.emit("update-progress", (got, total));
            },
            || {},
        )
        .await
        .map_err(msg)?;
    // macOS: keep the app folder and replace only Contents (so a pinned Dock icon keeps pointing at this app).
    // If that fails (e.g. a location needing admin rights), fall back to the updater's default install
    #[cfg(target_os = "macos")]
    if let Some(bundle) = std::env::current_exe().ok().and_then(|e| crate::bundle::app_bundle(&e))
        && crate::bundle::install_contents(&bundle, &bytes).is_ok()
    {
        app.restart()
    }
    update.install(&bytes).map_err(msg)?;
    app.restart()
}

#[cfg(test)]
mod ipc_tests {
    use super::*;
    use crate::store::Store;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, INVOKE_KEY};
    use tauri::webview::InvokeRequest;

    fn req(cmd: &str, body: serde_json::Value) -> InvokeRequest {
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    /// Call commands with the same JSON the frontend sends.
    #[test]
    fn save_and_list_via_ipc() {
        let dir = std::env::temp_dir().join(format!("dbtool-ipc-{}", uuid::Uuid::new_v4()));
        let app = tauri::test::mock_builder()
            .manage(AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap()))
            .invoke_handler(tauri::generate_handler![save_connection, list_connections, test_connection, list_databases, list_schemas, run_query])
            .build(crate::app_context())
            .unwrap();
        let wv = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();

        let config = serde_json::json!({
            "id": "", "name": "", "kind": "postgres", "host": "localhost", "port": 5432,
            "user": std::env::var("USER").unwrap(), "database": "postgres", "color": null, "ssh": null, "passwordSaved": false
        });
        let t = get_ipc_response(&wv, req("test_connection", serde_json::json!({ "config": config, "password": null })));
        println!("test_connection: {:?}", t.as_ref().map(|b| b.clone().deserialize::<serde_json::Value>().unwrap()));
        let r = get_ipc_response(&wv, req("save_connection", serde_json::json!({ "config": config, "password": null })));
        println!("save_connection: {:?}", r.as_ref().map(|b| b.clone().deserialize::<serde_json::Value>().unwrap()));
        let l = get_ipc_response(&wv, req("list_connections", serde_json::json!({})));
        println!("list_connections: {:?}", l.as_ref().map(|b| b.clone().deserialize::<serde_json::Value>().unwrap()));
        assert!(r.is_ok() && l.is_ok());

        // Schema lookup on the DB list and "id::db" targets
        let id = r.as_ref().unwrap().clone().deserialize::<serde_json::Value>().unwrap()["id"].as_str().unwrap().to_string();
        let dbs = get_ipc_response(&wv, req("list_databases", serde_json::json!({ "id": id })))
            .map(|b| b.deserialize::<Vec<String>>().unwrap())
            .expect("list_databases");
        println!("databases: {dbs:?}");
        let target = format!("{id}::{}", dbs.iter().find(|d| d.as_str() != "postgres").unwrap_or(&dbs[0]));
        let sch = get_ipc_response(&wv, req("list_schemas", serde_json::json!({ "id": target })))
            .map(|b| b.deserialize::<serde_json::Value>().unwrap())
            .expect("list_schemas on target db");
        println!("schemas in {target}: {}", sch.as_array().unwrap().len());
        let q = get_ipc_response(&wv, req("run_query", serde_json::json!({ "id": target, "sql": "select current_database()", "limit": 5, "tabId": null })))
            .map(|b| b.deserialize::<serde_json::Value>().unwrap())
            .expect("run_query");
        println!("current_database on target: {}", q["rows"][0][0]);
        assert_eq!(q["rows"][0][0].as_str().unwrap(), target.split("::").nth(1).unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
