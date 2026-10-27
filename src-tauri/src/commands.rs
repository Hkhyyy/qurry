//! 프론트엔드에서 호출하는 Tauri 커맨드. 실제 동작은 qurry_core::app::AppState 에 있다.
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

/// `password`: None이면 기존 비밀번호 유지, 빈 문자열이면 삭제.
#[tauri::command]
pub async fn save_connection(state: State<'_, AppState>, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<ConnectionConfig> {
    state.save_connection(config, password, ssh_secret).await
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.delete_connection(id).await
}

/// 저장하지 않고 접속만 시험한다. `password`가 None이면 저장된 비밀번호를 쓴다.
#[tauri::command]
pub async fn test_connection(state: State<'_, AppState>, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<String> {
    state.test_connection(config, password, ssh_secret).await
}

/// SSH/SSL 탭의 Test Connection: SSH 로그인만 확인한다.
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

/// `tab_id`가 있으면 그 콘솔의 세션에서 실행한다 (커서·취소 지원).
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
/// exact_one = 표 편집 저장 (UPDATE/DELETE 가 정확히 1행이 아니면 전부 롤백)
pub async fn execute_batch(state: State<'_, AppState>, id: String, statements: Vec<String>, exact_one: Option<bool>) -> AppResult<u64> {
    state.execute_batch(id, statements, exact_one.unwrap_or(false)).await
}

/// 접속하면 검색 캐시를 채운다 (프론트는 기다리지 않는다)
#[tauri::command]
pub async fn warm_search_cache(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.warm_search_cache(id).await
}

/// 접속하지 않은 Data Source 도 Search Everywhere 에서 찾도록 저장해 둔 객체·컬럼 목록
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

/// 터미널: redis-cli 처럼 한 줄을 그대로
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

/// 가져올 파일 고르기 → 무엇이 바뀌는지 미리보기. 실제 가져오기는 import_picked (사용자가 확인한 뒤)
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

/// 고른 가져오기 파일 (미리보기와 실제 가져오기 사이)
#[derive(Default)]
pub struct PendingImport(pub std::sync::Mutex<Option<std::path::PathBuf>>);

/// 저장 대화상자는 여기(백엔드)에서 연다: 웹뷰가 임의 경로를 넘겨 파일을 쓰지 못하게, 사용자가 고른 경로에만 쓴다
fn save_dialog(app: &tauri::AppHandle, default_name: &str, filter: &str, exts: &[&str]) -> Option<String> {
    app.dialog().file().set_file_name(default_name).add_filter(filter, exts).blocking_save_file().and_then(|p| p.into_path().ok()).map(|p| p.display().to_string())
}

/// SQL Generator·DDL "Save to File" → 저장한 경로 (취소하면 None)
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
    /// 파일로 썼으면 경로
    path: Option<String>,
    /// 클립보드면 글 (프론트엔드가 클립보드에 넣는다)
    text: Option<String>,
}

/// 작업을 등록하고, 도는 동안 진행을 알린다 ("transfer-progress": job, 행, done, total). 끝나면 abort
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

/// 데이터 내보내기: 형식을 골라 파일(저장 창) 또는 클립보드로. 저장 창을 닫으면 None
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

// ───────── 쿼리 히스토리·스니펫 (TUI 와 같은 파일) ─────────

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

/// 예전 GUI 기록(localStorage) 옮기기
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

// ───────── 스키마·데이터 비교, 마이그레이션 ─────────

#[tauri::command]
pub async fn compare_schemas(state: State<'_, AppState>, source_id: String, source_schema: String, target_id: String, target_schema: String) -> AppResult<Vec<qurry_core::migrate::Change>> {
    state.compare_schemas(source_id, source_schema, target_id, target_schema).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn compare_data(state: State<'_, AppState>, source_id: String, source_schema: String, source_table: String, target_id: String, target_schema: String, target_table: String) -> AppResult<qurry_core::migrate::DataDiff> {
    state.compare_data(source_id, source_schema, source_table, target_id, target_schema, target_table).await
}

/// 이번에 고른 폴더만 쓴다 (웹뷰가 아무 폴더의 SQL 을 돌리게 하지 않도록)
#[derive(Default)]
pub struct PickedDirs(pub std::sync::Mutex<std::collections::HashSet<String>>);

fn picked(dirs: &PickedDirs, dir: &str) -> AppResult<()> {
    if dirs.0.lock().unwrap().contains(dir) { Ok(()) } else { Err(crate::error::AppError::Msg("Choose the migrations folder first".into())) }
}

/// 마이그레이션 폴더 고르기
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

/// 비교 결과를 다음 버전 파일로 (V{n}__{설명}.sql)
#[tauri::command]
pub async fn save_migration(dirs: State<'_, PickedDirs>, dir: String, description: String, sql: String) -> AppResult<String> {
    picked(&dirs, &dir)?;
    qurry_core::migrate::save_migration(&dir, &description, &sql)
}

/// ERD: 스키마들의 테이블·관계 (+ Mermaid 글)
#[tauri::command]
pub async fn erd(state: State<'_, AppState>, id: String, schemas: Vec<String>) -> AppResult<(qurry_core::erd::Erd, String)> {
    let e = state.erd(id, schemas).await?;
    let m = e.mermaid();
    Ok((e, m))
}

/// 글을 파일로 (저장 창: 이름·형식). 취소하면 None
#[tauri::command]
pub async fn save_file(app: tauri::AppHandle, default_name: String, text: String, filter: String, ext: String) -> AppResult<Option<String>> {
    let Some(path) = save_dialog(&app, &default_name, &filter, &[ext.as_str()]) else { return Ok(None) };
    std::fs::write(&path, text)?;
    Ok(Some(path))
}

/// Production 확인: 이 문장이 바꿀 행 수
#[tauri::command]
pub async fn preview_impact(state: State<'_, AppState>, id: String, sql: String) -> AppResult<Option<i64>> {
    state.preview_impact(id, sql).await
}

/// 실행 계획 (EXPLAIN FORMAT JSON) → 트리 + 노드마다 몫(%)·경고
#[tauri::command]
pub fn analyze_plan(json: String) -> AppResult<qurry_core::plan::PlanNode> {
    qurry_core::plan::parse_pg(&json)
}

/// DBA 리포트 목록 (이 Data Source 의 DB 종류에 맞는 것)
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

/// 리포트 행 동작의 SQL (템플릿에 행 값)
#[tauri::command]
pub fn dba_action_sql(state: State<'_, AppState>, id: String, template: String, row: std::collections::HashMap<String, String>) -> AppResult<String> {
    Ok(qurry_core::dba::action_sql(state.kind_of(&id)?, &template, &row))
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job: String) {
    state.cancel_job(&job)
}

/// 가져올 데이터 파일 (고른 경로는 여기에만 둔다: 프론트엔드가 다른 파일을 읽게 하지 않도록)
#[derive(Default)]
pub struct PendingData(pub std::sync::Mutex<Option<std::path::PathBuf>>);

/// 가져올 파일 고르기 → 미리보기 (첫 20행)
#[tauri::command]
pub async fn pick_data_file(app: tauri::AppHandle, pending: State<'_, PendingData>) -> AppResult<Option<(String, Preview)>> {
    let picked = app.dialog().file().add_filter("CSV, TSV, JSON", &["csv", "tsv", "json"]).blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let pv = qurry_core::transfer::preview(&path, 20)?;
    *pending.0.lock().unwrap() = Some(path.clone());
    Ok(Some((path.display().to_string(), pv)))
}

/// 고른 파일을 테이블에 넣는다. 넣은 행 수
#[tauri::command]
pub async fn import_data(app: tauri::AppHandle, state: State<'_, AppState>, pending: State<'_, PendingData>, id: String, mut spec: ImportSpec, job: String) -> AppResult<u64> {
    let path = pending.0.lock().unwrap().clone().ok_or_else(|| crate::error::AppError::Msg("Pick a file first".into()))?;
    spec.path = path.display().to_string();
    let ticker = report_progress(&app, &state, &job);
    let r = state.import(id, spec, job).await;
    ticker.abort();
    r
}

/// 탭 세션에 커밋 안 한 변경이 있는지 (오류 뒤 Commit/Roll back 표시)
#[tauri::command]
pub async fn tx_state(state: State<'_, AppState>, tab_id: String) -> AppResult<bool> {
    Ok(state.tx_state(tab_id).await)
}

/// 처음 보는 SSH 서버 키를 신뢰 (사용자가 지문을 확인한 뒤)
#[tauri::command]
pub async fn trust_ssh_host(state: State<'_, AppState>, host: String, port: u16) -> AppResult<String> {
    state.trust_ssh_host(host, port)
}

/// 실행 전에 확인을 받을 위험한 문장이면 이유 (WHERE 없는 UPDATE/DELETE, DROP, TRUNCATE, Redis FLUSH*·DEL)
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

#[tauri::command]
pub async fn object_ddl(state: State<'_, AppState>, id: String, schema: String, name: String, kind: String) -> AppResult<String> {
    state.object_ddl(id, schema, name, kind).await
}

#[tauri::command]
pub async fn completion_meta(state: State<'_, AppState>, id: String) -> AppResult<Vec<crate::meta::CompletionTable>> {
    state.completion_meta(id).await
}

/// 열어 둔 탭·화면 분할 (workspace-gui.json). 다시 켜면 그대로
#[tauri::command]
pub fn load_workspace(state: State<'_, AppState>) -> Option<serde_json::Value> {
    state.store.workspace("gui")
}

#[tauri::command]
pub fn save_workspace(state: State<'_, AppState>, ws: serde_json::Value) -> AppResult<()> {
    state.store.save_workspace("gui", &ws)
}

/// 단축키 사용자 설정 (keymap.json 의 "gui" 구역)
#[tauri::command]
pub fn load_keymap(state: State<'_, AppState>) -> AppResult<crate::store::Keymap> {
    state.store.keymap("gui")
}

#[tauri::command]
pub fn save_keymap(state: State<'_, AppState>, map: crate::store::Keymap) -> AppResult<()> {
    state.store.save_keymap("gui", &map)
}

/// 새 버전 확인 결과 (current = 이 앱 버전)
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    latest: Option<qurry_core::update::Release>,
    /// 지금 버전보다 새 버전들의 바뀐 것 (최신부터)
    changes: Vec<qurry_core::update::VersionNotes>,
}

/// 새 버전 확인. force 가 아니면 한 시간에 한 번만 GitHub 에 묻는다. 개발 빌드(pnpm tauri dev)는 force 일 때만
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

/// 새 버전을 받아 지금 앱을 바꿔 넣고 다시 켠다 (Tauri updater: 릴리스의 latest.json + 서명 확인).
/// 받는 동안 "update-progress" 로 (받은 바이트, 전체 바이트) 를 보낸다
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> AppResult<()> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;
    let msg = |e: tauri_plugin_updater::Error| crate::error::AppError::Msg(format!("Update failed: {e}"));
    let Some(update) = app.updater().map_err(msg)?.check().await.map_err(msg)? else {
        return Err(crate::error::AppError::Msg("No update to install (the latest release has no in-app update for this platform — download it from the Release Page).".into()));
    };
    let mut got = 0u64;
    // download 가 서명까지 확인한다
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
    // macOS: 앱 폴더는 그대로 두고 Contents 만 바꾼다 (Dock 고정 아이콘이 계속 이 앱을 가리키게).
    // 못 하면(관리자 권한이 필요한 위치 등) updater 기본 설치로
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

    /// 프론트엔드가 보내는 것과 같은 JSON으로 커맨드를 호출해 본다.
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

        // DB 목록과 "id::db" 대상으로 스키마 조회
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
