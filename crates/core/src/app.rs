//! 앱 상태: Data Source 풀·세션·SSH 터널·비밀번호. GUI(Tauri)·TUI·MCP 가 같은 메서드를 쓴다.
use crate::db::{self, Pool, QueryResult};
use crate::error::{AppError, AppResult};
use crate::session::{self, Session};
use crate::store::{self, ConnectionConfig, Store};
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct SessionEntry {
    pub conn_id: String,
    pub pid: i64,
    pub session: std::sync::Arc<tokio::sync::Mutex<Session>>,
}

pub struct AppState {
    pub store: Store,
    pub pools: RwLock<HashMap<String, Pool>>,
    /// 콘솔 탭 id → 세션
    pub sessions: RwLock<HashMap<String, SessionEntry>>,
    /// 접속 id → SSH 터널 (풀과 수명을 같이 함)
    pub tunnels: RwLock<HashMap<String, crate::tunnel::Tunnel>>,
    /// 한 번 풀어 둔 비밀값 (앱이 켜져 있는 동안 파일을 다시 읽지 않도록)
    pub secrets: std::sync::Mutex<HashMap<String, Option<String>>>,
    /// 비밀번호 저장소 (암호화 파일). 키체인 팝업을 피한다.
    pub secret_file: crate::secrets::SecretFile,
    /// Data Source id → 마지막 사용 시각 (Auto-disconnect)
    pub last_used: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Data Source id → 마지막 keep-alive 시각
    pub last_ping: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// 탭 id → Manual 모드 (다시 접속해 세션을 새로 열 때도 같은 모드로)
    pub tab_manual: std::sync::Mutex<HashMap<String, bool>>,
    /// 쿼리 히스토리·스니펫 (GUI·TUI 공용)
    pub history: crate::history::History,
    /// 운영 작업 기록 (Production Data Source 의 변경 · 되돌리기 SQL)
    pub audit: crate::audit::Audit,
    /// 내보내기·가져오기 작업 id → 진행·취소
    pub jobs: std::sync::Mutex<HashMap<String, std::sync::Arc<crate::transfer::Job>>>,
    /// MCP 서버: MCP 에 보이지 않게 한 Data Source 는 비밀번호를 꺼내지 않는다 (crate::store::McpAccess)
    pub mcp: bool,
}

impl AppState {
    /// 키체인 조회는 처음 한 번만. 이후에는 메모리 캐시.
    pub fn secret(&self, key: &str) -> AppResult<Option<String>> {
        // MCP 서버: MCP 에 보이지 않게 한 Data Source 의 비밀번호는 꺼내지 않는다 (접속도 못 한다)
        if self.mcp {
            let id = key.strip_suffix("#ssh").unwrap_or(key);
            if let Some(c) = self.store.get(id)?.filter(|c| c.mcp_access(false) == store::McpAccess::Off) {
                return Err(AppError::Msg(format!("Data source \"{}\" is not shared with MCP (turn it on in the data source's Options)", c.name)));
            }
        }
        if let Some(v) = self.secrets.lock().unwrap().get(key) {
            return Ok(v.clone());
        }
        // 비밀번호는 암호화 파일에만 (키체인에는 많아야 마스터 키 하나: crate::secrets)
        let v = self.secret_file.get(key)?;
        self.secrets.lock().unwrap().insert(key.to_string(), v.clone());
        Ok(v)
    }

    /// 비밀번호 파일의 마스터 키가 있는 곳 (설정 화면에 보인다)
    pub fn secret_key_location(&self) -> crate::secrets::KeyLocation {
        crate::secrets::key_location(&self.store.dir())
    }

    /// 마스터 키를 OS 키체인으로 / 파일로 옮긴다. 키는 같으니 지금 열린 저장소는 그대로 쓴다
    pub fn set_secret_key_location(&self, keychain: bool) -> AppResult<crate::secrets::KeyLocation> {
        let to = if keychain { crate::secrets::KeyLocation::Keychain } else { crate::secrets::KeyLocation::File };
        crate::secrets::move_key(&self.store.dir(), to, &crate::secrets::OsKeychain)?;
        Ok(self.secret_key_location())
    }

    pub fn set_secret(&self, key: &str, value: Option<&str>) -> AppResult<()> {
        self.secret_file.set(key, value)?;
        self.secrets.lock().unwrap().insert(key.to_string(), value.map(str::to_string));
        Ok(())
    }
}

impl AppState {
    pub fn new(store: Store, secret_file: crate::secrets::SecretFile) -> Self {
        let history = crate::history::History::new(&store.dir());
        let audit = crate::audit::Audit::new(&store.dir());
        Self { history, audit, store, pools: RwLock::new(HashMap::new()), sessions: RwLock::new(HashMap::new()), tunnels: RwLock::new(HashMap::new()), secrets: Default::default(), secret_file, last_used: Default::default(), last_ping: Default::default(), tab_manual: Default::default(), jobs: Default::default(), mcp: false }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    #[serde(flatten)]
    pub config: ConnectionConfig,
    pub has_password: bool,
    pub connected: bool,
}

/// 트리에 보이는 DB (Data Source › Schemas 탭의 범위, 시스템 DB 숨김). GUI·TUI·검색 캐시 공용
pub fn visible_dbs(cfg: &ConnectionConfig, dbs: Vec<String>, def: &str) -> Vec<String> {
    let hidden: &[&str] = match cfg.kind {
        store::DbKind::Postgres => &["template0", "template1", "rdsadmin"],
        store::DbKind::Mysql => &crate::mysql::SYSTEM_DBS,
        store::DbKind::Redis => &[],
        store::DbKind::Mssql => &["master", "tempdb", "model", "msdb"],
        store::DbKind::Mongodb => &["admin", "config", "local"],
        store::DbKind::Clickhouse => &["system", "information_schema", "INFORMATION_SCHEMA"],
        _ => &[],
    };
    let scoped = cfg.scope.databases.as_ref().map(|d| !d.is_empty()).unwrap_or(false);
    dbs.into_iter()
        .filter(|d| scoped || d == def || !hidden.contains(&d.as_str()))
        .filter(|d| !cfg.options.single_db || d == def)
        .filter(|d| cfg.options.single_db || cfg.scope.db_visible(d, def))
        .collect()
}

/// 접속 대상 id: "<data source id>" 또는 "<data source id>::<database>"
pub fn split_target(id: &str) -> (&str, Option<&str>) {
    match id.split_once("::") {
        Some((base, db)) if !db.is_empty() => (base, Some(db)),
        _ => (id, None),
    }
}

pub fn belongs(key: &str, id: &str) -> bool {
    key == id || key.starts_with(&format!("{id}::"))
}

/// 연결이 끊긴 오류인지 (네트워크·서버 재시작·SSH 터널). 문법 오류 같은 쿼리 오류는 아니다
pub fn is_conn_lost(e: &AppError) -> bool {
    match e {
        AppError::Db(sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::WorkerCrashed | sqlx::Error::Tls(_) | sqlx::Error::Protocol(_)) => true,
        AppError::Io(_) => true,
        AppError::Msg(m) => {
            let m = m.to_lowercase();
            ["connection reset", "connection refused", "connection closed", "broken pipe", "timed out", "unexpected end of file", "not connected"].iter().any(|k| m.contains(k))
        }
        _ => false,
    }
}

/// 연결이 끊겼으면: 탭 세션을 버리고 (계속 죽은 연결을 쓰지 않게, 다음 실행이 새로 연결한다),
/// 풀도 응답이 없으면 (SSH 터널이 죽은 경우 등) 닫는다. 같은 문장을 다시 보내지는 않는다 (서버가 이미 실행했을 수 있다)
async fn recover(state: &AppState, id: &str, tab: Option<&str>, lost_tx: bool, e: AppError) -> AppError {
    if !is_conn_lost(&e) {
        return e;
    }
    if let Some(tab) = tab {
        state.sessions.write().await.remove(tab);
    }
    let pool = state.pools.read().await.get(id).cloned();
    if let Some(p) = pool {
        let alive = tokio::time::timeout(std::time::Duration::from_secs(3), p.server_version()).await.is_ok_and(|r| r.is_ok());
        if !alive {
            drop_pool(state, split_target(id).0).await;
        }
    }
    crate::log!("[qurry] connection lost on {id}: {e}");
    AppError::Msg(format!(
        "{e} — connection lost{}. Run it again to reconnect.",
        if lost_tx { "; the open transaction was rolled back by the server" } else { "" }
    ))
}

/// 해당 Data Source의 모든 DB 풀·세션·터널을 닫는다.
pub async fn drop_pool(state: &AppState, id: &str) {
    state.sessions.write().await.retain(|_, e| !belongs(&e.conn_id, id));
    let removed: Vec<Pool> = {
        let mut pools = state.pools.write().await;
        let keys: Vec<String> = pools.keys().filter(|k| belongs(k, id)).cloned().collect();
        keys.into_iter().filter_map(|k| pools.remove(&k)).collect()
    };
    for p in removed {
        p.close().await;
    }
    state.tunnels.write().await.retain(|k, _| !belongs(k, id));
    state.last_ping.lock().unwrap().retain(|k, _| !belongs(k, id));
}

/// SSH 설정이 있으면 터널을 먼저 연 뒤 접속한다.
pub async fn connect_config(
    cfg: &ConnectionConfig,
    password: Option<&str>,
    ssh_secret: Option<&str>,
) -> AppResult<(Pool, Option<crate::tunnel::Tunnel>)> {
    match cfg.active_ssh() {
        Some(ssh) => {
            let t = crate::tunnel::open(ssh, ssh_secret, &cfg.host, cfg.port).await?;
            let pool = db::connect_via(cfg, password, Some(t.local_port)).await?;
            Ok((pool, Some(t)))
        }
        None => Ok((db::connect(cfg, password).await?, None)),
    }
}

pub fn touch(state: &AppState, id: &str) {
    let base = split_target(id).0.to_string();
    state.last_used.lock().unwrap().insert(base, std::time::Instant::now());
}

/// 한 번 돌 때마다: keep-alive 쿼리 / 유휴 접속 끊기. 끊은 Data Source id 목록을 돌려준다.
pub async fn housekeeping(state: &AppState) -> Vec<String> {
    use std::time::{Duration, Instant};
    let bases: Vec<String> = {
        let pools = state.pools.read().await;
        let mut b: Vec<String> = pools.keys().map(|k| split_target(k).0.to_string()).collect();
        b.sort();
        b.dedup();
        b
    };
    // 결과를 끝까지 안 읽고 오래 둔 커서 닫기 (열린 트랜잭션이 VACUUM·DDL 을 막지 않게). 실행 중인 세션은 건너뛴다
    let sessions: Vec<_> = state.sessions.read().await.values().map(|e| e.session.clone()).collect();
    for s in sessions {
        if let Ok(mut g) = s.try_lock() {
            g.close_idle_cursor(session::CURSOR_IDLE).await;
        }
    }
    let mut dropped = Vec::new();
    for base in bases {
        let Ok(Some(cfg)) = state.store.get(&base) else { continue };
        let now = Instant::now();
        if let Some(secs) = cfg.options.auto_disconnect_secs.filter(|s| *s > 0) {
            let last = *state.last_used.lock().unwrap().entry(base.clone()).or_insert(now);
            // 실행 중인 쿼리(세션 잠김)가 있으면 끊지 않는다
            // 실행 중인 쿼리가 있거나 커밋 안 한 변경이 있는 세션이 있으면 끊지 않는다 (끊으면 변경이 사라진다)
            let busy = state.sessions.read().await.values().any(|e| belongs(&e.conn_id, &base) && e.session.try_lock().map(|g| g.in_tx()).unwrap_or(true));
            if !busy && now.duration_since(last) >= Duration::from_secs(secs as u64) {
                drop_pool(state, &base).await;
                state.last_used.lock().unwrap().remove(&base);
                dropped.push(base);
                continue;
            }
        }
        if let Some(secs) = cfg.options.keep_alive_secs.filter(|s| *s > 0) {
            let due = {
                let mut m = state.last_ping.lock().unwrap();
                let last = *m.entry(base.clone()).or_insert(now);
                if now.duration_since(last) >= Duration::from_secs(secs as u64) {
                    m.insert(base.clone(), now);
                    true
                } else {
                    false
                }
            };
            if due {
                let pools: Vec<Pool> = state.pools.read().await.iter().filter(|(k, _)| belongs(k, &base)).map(|(_, p)| p.clone()).collect();
                for pool in pools {
                    pool.ping().await;
                }
                let sessions: Vec<_> = state.sessions.read().await.values().filter(|e| belongs(&e.conn_id, &base)).map(|e| e.session.clone()).collect();
                for s in sessions {
                    if let Ok(mut g) = s.try_lock() {
                        let _ = tokio::time::timeout(Duration::from_secs(5), g.ping()).await;
                    }
                }
            }
        }
    }
    dropped
}

/// 드라이버 풀의 내보내기 (전체를 한 번에 받아 쓴다)
pub async fn ensure_pool(state: &AppState, id: &str) -> AppResult<Pool> {
    touch(state, id);
    if let Some(p) = state.pools.read().await.get(id) {
        return Ok(p.clone());
    }
    let (base, db) = split_target(id);
    let mut cfg = state
        .store
        .get(base)?
        .ok_or_else(|| AppError::Msg("Data source not found".into()))?;
    // 파일 DB (SQLite·DuckDB) 는 database 가 파일 경로다: DB 이름(main)으로 덮지 않는다
    if let Some(db) = db.filter(|_| !cfg.kind.is_file()) {
        cfg.database = db.to_string();
    }
    let password = state.secret(base)?;
    let ssh_secret = if cfg.active_ssh().is_some() { state.secret(&store::ssh_secret_key(base))? } else { None };
    let (pool, tunnel) = connect_config(&cfg, password.as_deref(), ssh_secret.as_deref()).await.map_err(|e| match e {
        // sqlx 는 "pool timed out while waiting for an open connection" 이라고만 한다
        AppError::Db(sqlx::Error::PoolTimedOut) => AppError::Msg(format!(
            "Can't connect to {}:{} — the server didn't answer within 10s. Check that it's up (VPN / SSH tunnel), then run it again.",
            cfg.host, cfg.port
        )),
        e => e,
    })?;
    if let Some(t) = tunnel {
        state.tunnels.write().await.insert(id.to_string(), t);
    }
    state
        .pools
        .write()
        .await
        .insert(id.to_string(), pool.clone());
    Ok(pool)
}

pub async fn ensure_session(state: &AppState, id: &str, tab_id: &str) -> AppResult<std::sync::Arc<tokio::sync::Mutex<Session>>> {
    if let Some(e) = state.sessions.read().await.get(tab_id) {
        if e.conn_id == id {
            return Ok(e.session.clone());
        }
    }
    // 같은 탭이 다른 DB 로 바뀌었으면 이전 세션은 정리해서 돌려준다
    let old = state.sessions.write().await.remove(tab_id);
    if let Some(e) = old {
        e.session.lock().await.discard().await;
    }
    let pool = ensure_pool(state, id).await?;
    let mut s = Session::open(&pool).await?;
    // 탭이 Manual 이면 새 세션도 Manual (재접속 후 몰래 Auto 가 되면 다음 변경이 바로 커밋된다)
    if state.tab_manual.lock().unwrap().get(tab_id).copied().unwrap_or(false) {
        s.set_manual(true).await?;
    }
    let entry = SessionEntry { conn_id: id.to_string(), pid: s.backend_id(), session: std::sync::Arc::new(tokio::sync::Mutex::new(s)) };
    let arc = entry.session.clone();
    state.sessions.write().await.insert(tab_id.to_string(), entry);
    Ok(arc)
}

pub async fn redis_pool(state: &AppState, id: &str) -> AppResult<crate::redisdb::RedisConn> {
    match ensure_pool(state, id).await? {
        Pool::Redis(m, _) => Ok(m),
        _ => Err(AppError::Msg("Not a Redis data source".into())),
    }
}

/// 읽기 전용(MCP) 쿼리 시간 제한 (남이 보낸 쿼리로 서버를 오래 붙잡지 않게)
const SERVER_CHANGED: &str = "The server, user or SSH tunnel changed (or SSL was turned off) — enter the password again. The saved one is only sent to the server it was saved for.";
const SSH_CHANGED: &str = "The SSH server changed — enter the SSH password / passphrase again. The saved one is only sent to the server it was saved for.";
const READ_ONLY_TIMEOUT_PG: &str = "SET LOCAL statement_timeout = '60s'";
const READ_ONLY_TIMEOUT_MY: &str = "SET SESSION max_execution_time = 60000";
/// MCP CSV 내보내기 상한
const EXPORT_MAX_BYTES: u64 = 512 << 20;
const EXPORT_MAX_PLUGIN_ROWS: usize = 1_000_000;

#[derive(Serialize)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
}

impl AppState {
    /// 읽기 전용으로 실행 (MCP 기본). 검사는 guard (한 문장·읽기 명령·부작용 함수 금지), 실행은 READ ONLY 트랜잭션 안에서
    /// 60초 제한을 걸고 끝나면 롤백. MySQL 은 prepared 프로토콜 (서버가 한 문장만 받는다). Redis: 읽기 명령만 (CONFIG·KEYS 제외)
    pub async fn run_read_only(&self, id: String, sql: String, limit: usize) -> AppResult<QueryResult> {
        use sqlx::Executor;
        let kind = self.kind_of(&id)?;
        if let Some(why) = crate::guard::read_only_violation(&sql, kind, true) {
            return Err(AppError::Msg(format!("{why} (start the MCP server with --allow-write to change data)")));
        }
        match ensure_pool(self, &id).await? {
            Pool::Pg(p) => {
                let mut tx = p.begin().await?;
                tx.execute("SET TRANSACTION READ ONLY").await?;
                tx.execute(READ_ONLY_TIMEOUT_PG).await?;
                let r = db::run_on(&mut tx, &sql, limit).await;
                let _ = tx.rollback().await;
                r
            }
            Pool::My(p) => {
                let mut conn = p.acquire().await?;
                conn.execute(READ_ONLY_TIMEOUT_MY).await?;
                conn.execute("START TRANSACTION READ ONLY").await?;
                let r = crate::mysql::run_read_only_on(&mut conn, &sql, limit).await;
                let _ = conn.execute("ROLLBACK").await;
                let _ = conn.execute("SET SESSION max_execution_time = 0").await;
                r
            }
            Pool::Redis(m, _) => {
                let args = crate::redisdb::parse_command(&sql)?;
                let cmd = String::from_utf8_lossy(&args[0]).to_uppercase();
                let sub = args.get(1).map(|a| String::from_utf8_lossy(a).to_uppercase()).unwrap_or_default();
                if !crate::redisdb::is_read_command(&cmd, &sub) {
                    return Err(AppError::Msg(format!("{cmd} is not a read-only command")));
                }
                let rest: Vec<String> = args.iter().skip(1).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
                if let Some(why) = crate::guard::redis_strict_violation(&cmd, &rest) {
                    return Err(AppError::Msg(why));
                }
                crate::redisdb::run(&m, &sql, limit).await
            }
            // 드라이버: 서버 쪽 READ ONLY 트랜잭션이 없다 — guard 검사(한 문장·읽기 명령)를 통과한 것만
            Pool::Plugin(c) => c.query_opts(&sql, limit, true).await,
        }
    }

    /// 읽기 전용 쿼리 결과를 CSV 로 (MCP). 검사·실행 모두 run_read_only 와 같은 규칙 (전체 결과도 READ ONLY 트랜잭션 안에서)
    pub async fn export_csv_read_only(&self, id: String, sql: String, path: std::path::PathBuf) -> AppResult<u64> {
        let kind = self.kind_of(&id)?;
        if let Some(why) = crate::guard::read_only_violation(&sql, kind, true) {
            return Err(AppError::Msg(why));
        }
        let pool = ensure_pool(self, &id).await?;
        // run_read_only 와 같은 시간 제한, 파일 크기 제한 (디스크를 채우지 않게). 실패하면 쓰던 파일은 지운다
        let mut out = crate::transfer::Exporter::csv_file(&path).await?.max_bytes(EXPORT_MAX_BYTES);
        let r = match Self::export_read_only_into(pool, &sql, &mut out).await {
            Ok(()) => out.finish().await.map(|e| e.rows),
            Err(e) => Err(e),
        };
        if r.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        r
    }

    async fn export_read_only_into(pool: Pool, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
        use sqlx::Executor;
        match pool {
            Pool::Pg(p) => {
                let mut tx = p.begin().await?;
                tx.execute("SET TRANSACTION READ ONLY").await?;
                tx.execute(READ_ONLY_TIMEOUT_PG).await?;
                let r = db::export_pg_on(&mut tx, sql, out).await;
                let _ = tx.rollback().await;
                r?
            }
            Pool::My(p) => {
                let mut conn = p.acquire().await?;
                conn.execute(READ_ONLY_TIMEOUT_MY).await?;
                conn.execute("START TRANSACTION READ ONLY").await?;
                let r = crate::mysql::export_on(&mut conn, sql, out, true).await;
                let _ = conn.execute("ROLLBACK").await;
                let _ = conn.execute("SET SESSION max_execution_time = 0").await;
                r?
            }
            Pool::Redis(..) => return Err(AppError::Msg("CSV export of Redis commands is not available in read-only mode".into())),
            Pool::Plugin(c) => {
                let r = tokio::time::timeout(std::time::Duration::from_secs(60), c.query_opts(sql, EXPORT_MAX_PLUGIN_ROWS, true))
                    .await
                    .map_err(|_| AppError::Msg("The export took over 60 s — narrow the query".into()))??;
                out.header(&r.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()).await?;
                for row in &r.rows {
                    out.row(row).await?;
                }
            }
        }
        Ok(())
    }

    pub fn kind_of(&self, id: &str) -> AppResult<crate::store::DbKind> {
        let (base, _) = split_target(id);
        Ok(self.store.get(base)?.ok_or_else(|| AppError::Msg(format!("Data source not found: {base}")))?.kind)
    }
}

impl AppState {
    pub async fn list_connections(&self) -> AppResult<Vec<ConnectionView>> {
        crate::log!("[qurry] list_connections");
        let pools = self.pools.read().await;
        self
            .store
            .list()?
            .into_iter()
            .map(|c| {
                Ok(ConnectionView {
                    has_password: c.password_saved || self.secret_file.contains(&c.id),
                    connected: pools.keys().any(|k| belongs(k, &c.id)),
                    config: c,
                })
            })
            .collect()
    }

    /// `password`: None이면 기존 비밀번호 유지, 빈 문자열이면 삭제.
    pub async fn save_connection(&self, mut config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<ConnectionConfig> {
        crate::log!("[qurry] save_connection host={} user={} db={}", config.host, config.user, config.database);
        if config.id.is_empty() {
            config.id = uuid::Uuid::new_v4().to_string();
        }
        // 저장 여부 플래그는 서버가 관리한다 (None = 기존 값 유지)
        let prev = self.store.get(&config.id)?;
        // 서버를 바꾸면 저장된 비밀번호를 새 서버에 쓰지 않는다: 다시 입력하게 한다
        if let Some(p) = &prev {
            if password.is_none() && p.password_saved && !p.same_server(&config) {
                return Err(AppError::Msg(SERVER_CHANGED.into()));
            }
            if ssh_secret.is_none() && p.ssh_secret_saved && config.active_ssh().is_some() && !p.same_ssh(&config) {
                return Err(AppError::Msg(SSH_CHANGED.into()));
            }
        }
        config.password_saved = prev.as_ref().is_some_and(|p| p.password_saved);
        config.ssh_secret_saved = prev.as_ref().is_some_and(|p| p.ssh_secret_saved);
        match password.as_deref() {
            Some("") => {
                self.set_secret(&config.id, None)?;
                config.password_saved = false;
            }
            Some(p) => {
                self.set_secret(&config.id, Some(p))?;
                config.password_saved = true;
            }
            None => {}
        }
        let sk = store::ssh_secret_key(&config.id);
        match ssh_secret.as_deref() {
            Some("") => {
                self.set_secret(&sk, None)?;
                config.ssh_secret_saved = false;
            }
            Some(p) => {
                self.set_secret(&sk, Some(p))?;
                config.ssh_secret_saved = true;
            }
            None => {}
        }
        self.store.save(&config)?;
        drop_pool(self, &config.id).await; // 설정이 바뀌었을 수 있으므로 다시 접속하게 한다
        Ok(config)
    }

    pub async fn delete_connection(&self, id: String) -> AppResult<()> {
        drop_pool(self, &id).await;
        self.set_secret(&id, None)?;
        self.set_secret(&store::ssh_secret_key(&id), None)?;
        crate::search_cache::remove_conn(&self.store.dir(), &id);
        self.store.delete(&id)
    }

    /// 저장된 비밀번호를 config 로 보내도 되면 그 값 (저장된 설정과 같은 서버일 때만)
    fn saved_password_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if p.password_saved && !p.same_server(config) => Err(AppError::Msg(SERVER_CHANGED.into())),
            Some(_) => self.secret(&config.id),
            None => Ok(None),
        }
    }

    fn saved_ssh_secret_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if p.ssh_secret_saved && !p.same_ssh(config) => Err(AppError::Msg(SSH_CHANGED.into())),
            Some(_) => self.secret(&store::ssh_secret_key(&config.id)),
            None => Ok(None),
        }
    }

    /// 저장하지 않고 접속만 시험한다. `password`가 None이면 저장된 비밀번호를 쓴다 (저장된 설정과 같은 서버일 때만).
    pub async fn test_connection(&self, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<String> {
        let password = match password {
            Some(p) => Some(p),
            None if !config.id.is_empty() => self.saved_password_for(&config)?,
            None => None,
        };
        let ssh_secret = match ssh_secret {
            Some(p) => Some(p),
            None if !config.id.is_empty() && config.active_ssh().is_some() => self.saved_ssh_secret_for(&config)?,
            None => None,
        };
        let (pool, tunnel) = connect_config(&config, password.as_deref(), ssh_secret.as_deref()).await?;
        let version = pool.server_version().await;
        let plain = match &pool {
            Pool::My(p) => crate::mysql::plaintext_note(p, &config).await,
            _ => None,
        };
        pool.close().await;
        drop(tunnel);
        let via = config.active_ssh().map(|s| format!("  (via SSH {}@{})", s.user, s.host)).unwrap_or_default();
        version.map(|v| {
            let notes: Vec<String> = db::version_note(config.kind, &v).into_iter().chain(plain).collect();
            if notes.is_empty() { format!("{v}{via}") } else { format!("{v}{via}\n{}", notes.join("\n")) }
        })
    }

    /// SSH/SSL 탭의 Test Connection: SSH 로그인만 확인한다.
    pub async fn test_ssh(&self, config: ConnectionConfig, ssh_secret: Option<String>) -> AppResult<String> {
        let ssh = config.ssh.as_ref().filter(|s| !s.host.is_empty()).ok_or_else(|| AppError::Msg("Enter the SSH host".into()))?;
        let secret = match ssh_secret {
            Some(p) => Some(p),
            None if !config.id.is_empty() => self.saved_ssh_secret_for(&config)?,
            None => None,
        };
        crate::tunnel::test_login(ssh, secret.as_deref()).await
    }

    pub async fn connect(&self, id: String) -> AppResult<String> {
        crate::log!("[qurry] connect {id}");
        let pool = ensure_pool(self, &id).await?;
        let v = pool.server_version().await?;
        // 평문으로 붙었으면 버전 옆에 알린다 (상태줄 · TUI 상태)
        Ok(match &pool {
            Pool::My(p) => match crate::mysql::plaintext_note(p, &self.source(&id)?).await {
                Some(n) => format!("{v} — {n}"),
                None => v,
            },
            _ => v,
        })
    }

    pub async fn disconnect(&self, id: String) -> AppResult<()> {
        drop_pool(self, &id).await;
        Ok(())
    }

    /// `tab_id`가 있으면 그 콘솔의 세션에서 실행한다 (커서·취소 지원).
    /// Production Data Source 면 바꾸는 문장을 운영 작업 기록에 남기고, 단순한 UPDATE / DELETE 는 되돌리기 SQL 도 (crate::audit)
    pub async fn run_query(&self, id: String, sql: String, limit: Option<usize>, tab_id: Option<String>) -> AppResult<QueryResult> {
        let limit = limit.unwrap_or(500);
        self.check_read_only(&id, &sql)?;
        let cfg = self.source(&id)?;
        let watch = cfg.options.confirm_changes;
        let change = watch && crate::audit::is_change(&sql, cfg.kind);
        let r = match tab_id.as_deref() {
            Some(tab) => {
                let sess = match ensure_session(self, &id, tab).await {
                    Ok(s) => s,
                    Err(e) => return Err(recover(self, &id, Some(tab), false, e).await),
                };
                let (r, in_tx, undo) = {
                    let mut s = sess.lock().await;
                    let undo = match change {
                        true => Some(crate::audit::prepare(&ensure_pool(self, &id).await?, Some(&mut s), &sql, cfg.kind).await),
                        false => None,
                    };
                    let in_tx = s.in_tx();
                    (s.run(&sql, limit).await, in_tx, undo)
                };
                if watch {
                    self.audit.after(&id, &cfg.name, cfg.kind, Some(tab), &sql, &r, undo);
                }
                match r {
                    Err(e) => Err(recover(self, &id, Some(tab), in_tx, e).await),
                    ok => ok,
                }
            }
            None => {
                let pool = ensure_pool(self, &id).await?;
                let undo = match change {
                    true => Some(crate::audit::prepare(&pool, None, &sql, cfg.kind).await),
                    false => None,
                };
                let r = pool.run_query(&sql, limit).await;
                if watch {
                    self.audit.after(&id, &cfg.name, cfg.kind, None, &sql, &r, undo);
                }
                match r {
                    Err(e) => Err(recover(self, &id, None, false, e).await),
                    ok => ok,
                }
            }
        };
        r
    }

    /// 운영 작업 기록 (최근 것부터)
    pub fn audit_log(&self, conn: Option<String>, filter: String, limit: usize) -> Vec<crate::audit::AuditEntry> {
        self.audit.list(conn.as_deref(), &filter, limit)
    }

    /// 콘솔의 Commit / Roll back 을 운영 작업 기록에 (기록한 변경이 커밋 전이었을 때만)
    async fn audit_tx_end(&self, tab_id: &str, word: &str) {
        let conn = self.sessions.read().await.get(tab_id).map(|e| e.conn_id.clone());
        if let Some(c) = conn
            && let Ok(cfg) = self.source(&c)
        {
            self.audit.tx_closed(&c, &cfg.name, tab_id, word);
        }
    }

    pub async fn fetch_more(&self, tab_id: String, limit: Option<usize>) -> AppResult<QueryResult> {
        if let Some(e) = self.sessions.read().await.get(&tab_id) {
            touch(self, &e.conn_id);
        }
        let entry = self.sessions.read().await.get(&tab_id).map(|e| (e.session.clone(), e.conn_id.clone()));
        let (sess, conn_id) = entry.ok_or_else(|| AppError::Msg("Session closed".into()))?;
        let (r, in_tx) = {
            let mut s = sess.lock().await;
            let in_tx = s.in_tx();
            (s.fetch(limit.unwrap_or(500)).await, in_tx)
        };
        match r {
            Err(e) => Err(recover(self, &conn_id, Some(&tab_id), in_tx, e).await),
            ok => ok,
        }
    }

    pub async fn cancel_query(&self, tab_id: String) -> AppResult<bool> {
        let (conn_id, pid) = match self.sessions.read().await.get(&tab_id) {
            Some(e) => (e.conn_id.clone(), e.pid),
            None => return Ok(false),
        };
        match ensure_pool(self, &conn_id).await? {
            Pool::Pg(pg) => session::cancel(&pg, pid as i32).await,
            Pool::My(my) => crate::mysql::cancel(&my, pid).await,
            Pool::Redis(..) | Pool::Plugin(_) => Ok(false),
        }
    }

    /// Tx: Auto / Manual
    pub async fn set_tx_mode(&self, id: String, tab_id: String, manual: bool) -> AppResult<()> {
        let sess = ensure_session(self, &id, &tab_id).await?;
        let mut s = sess.lock().await;
        s.set_manual(manual).await?;
        self.tab_manual.lock().unwrap().insert(tab_id, manual);
        Ok(())
    }

    /// 탭 세션에 커밋 안 한 변경이 있는지 (오류 뒤에도 Commit/Roll back 버튼을 맞게 보이려고)
    pub async fn tx_state(&self, tab_id: String) -> bool {
        let sess = self.sessions.read().await.get(&tab_id).map(|e| e.session.clone());
        match sess {
            Some(s) => s.lock().await.in_tx(),
            None => false,
        }
    }

    /// Read-only Redis Data Source 면 쓰기 명령을 막는다
    fn check_read_only_redis(&self, id: &str, cmd: &str, sub: &str) -> AppResult<()> {
        let cfg = self.source(id)?;
        let (cmd, sub) = (cmd.to_uppercase(), sub.to_uppercase());
        if cfg.read_only && !crate::redisdb::is_read_command(&cmd, &sub) {
            return Err(AppError::Msg(format!("Data source \"{}\" is read-only: {cmd} changes data", cfg.name)));
        }
        Ok(())
    }

    /// Data Source 설정 (읽기 전용 여부 등)
    pub(crate) fn source(&self, id: &str) -> AppResult<ConnectionConfig> {
        self.store.get(split_target(id).0)?.ok_or_else(|| AppError::Msg("Data source not found".into()))
    }

    /// Read-only Data Source 면 데이터를 바꾸는 문장을 막는다 (UI 와 상관없이 여기서 지킨다)
    pub(crate) fn check_read_only(&self, id: &str, sql: &str) -> AppResult<()> {
        let cfg = self.source(id)?;
        if !cfg.read_only {
            return Ok(());
        }
        let blocked = if cfg.kind == crate::store::DbKind::Redis {
            let args = crate::redisdb::parse_command(sql)?;
            let arg = |i: usize| args.get(i).map(|a| String::from_utf8_lossy(a).into_owned()).unwrap_or_default();
            return self.check_read_only_redis(id, &arg(0), &arg(1));
        } else {
            crate::guard::read_only_violation(sql, cfg.kind, false)
        };
        match blocked {
            Some(why) => Err(AppError::Msg(format!("Data source \"{}\" is read-only: {why}", cfg.name))),
            None => Ok(()),
        }
    }

    pub async fn commit_tx(&self, tab_id: String) -> AppResult<()> {
        let sess = self.sessions.read().await.get(&tab_id).map(|e| e.session.clone()).ok_or_else(|| AppError::Msg("Session closed".into()))?;
        sess.lock().await.commit().await?;
        self.audit_tx_end(&tab_id, "COMMIT").await;
        Ok(())
    }

    pub async fn rollback_tx(&self, tab_id: String) -> AppResult<()> {
        let sess = self.sessions.read().await.get(&tab_id).map(|e| e.session.clone()).ok_or_else(|| AppError::Msg("Session closed".into()))?;
        sess.lock().await.rollback_tx().await?;
        self.audit_tx_end(&tab_id, "ROLLBACK").await;
        Ok(())
    }

    pub async fn close_session(&self, tab_id: String) -> AppResult<()> {
        self.tab_manual.lock().unwrap().remove(&tab_id);
        // 쿼리가 돌고 있으면 먼저 취소한다 (안 그러면 끝날 때까지 연결과 정리 작업이 묶인다)
        let busy = self.sessions.read().await.get(&tab_id).is_some_and(|e| e.session.try_lock().is_err());
        if busy {
            let _ = self.cancel_query(tab_id.clone()).await;
        }
        self.audit_tx_end(&tab_id, "ROLLBACK (console closed)").await;
        let removed = self.sessions.write().await.remove(&tab_id);
        if let Some(e) = removed {
            e.session.lock().await.discard().await;
        }
        Ok(())
    }

    /// 문장들을 한 트랜잭션으로. exact_one = 표 편집 저장 (UPDATE/DELETE 가 정확히 1행이 아니면 전부 롤백)
    pub async fn execute_batch(&self, id: String, statements: Vec<String>, exact_one: bool) -> AppResult<u64> {
        for st in &statements {
            self.check_read_only(&id, st)?;
        }
        let cfg = self.source(&id)?;
        let mut undos = Vec::new();
        if cfg.options.confirm_changes {
            let pool = ensure_pool(self, &id).await?;
            for st in &statements {
                undos.push(match crate::audit::is_change(st, cfg.kind) {
                    true => Some(crate::audit::prepare(&pool, None, st, cfg.kind).await),
                    false => None,
                });
            }
        }
        let r = match ensure_pool(self, &id).await? {
            Pool::Pg(pg) => session::execute_batch(&pg, &statements, exact_one).await,
            Pool::My(my) => crate::mysql::execute_batch(&my, &statements, exact_one).await,
            Pool::Redis(m, _) => {
                for st in &statements {
                    crate::redisdb::run(&m, st, 1).await?;
                }
                Ok(statements.len() as u64)
            }
            // 드라이버: 차례로 (트랜잭션은 드라이버가 모른다 — exact_one 만 지킨다)
            Pool::Plugin(c) => {
                let mut affected = 0;
                for st in &statements {
                    let n = c.query(st, 1).await?.rows_affected;
                    session::check_exact_one(st, n, exact_one)?;
                    affected += n;
                }
                Ok(affected)
            }
        };
        if cfg.options.confirm_changes {
            self.audit.after_batch(&id, &cfg.name, cfg.kind, &statements, r.as_ref().err().map(|e| e.to_string()), exact_one, undos);
        }
        match r {
            Err(e) => Err(recover(self, &id, None, false, e).await),
            ok => ok,
        }
    }

    /// 읽을 때마다 검색 캐시에 저장한다 (Redis 키는 저장하지 않는다)
    pub async fn object_index(&self, id: String) -> AppResult<Vec<crate::meta::IndexEntry>> {
        let pool = ensure_pool(self, &id).await?;
        let r = crate::meta::object_index(&pool).await?;
        if !matches!(pool, Pool::Redis(..)) {
            crate::search_cache::save_objects(&self.store.dir(), &id, &r);
        }
        Ok(r)
    }

    /// 접속하면 바로 (백그라운드로) 보이는 DB 의 객체·컬럼 목록을 읽어 검색 캐시를 채운다.
    /// 그래야 다음에 접속하지 않고도 Search Everywhere 에서 찾는다. 10분 안에 채운 DB 는 건너뛴다
    pub async fn warm_search_cache(&self, conn: String) -> AppResult<()> {
        let cfg = self.store.get(&conn)?.ok_or_else(|| AppError::Msg("Data source not found".into()))?;
        if cfg.kind == store::DbKind::Redis {
            return Ok(());
        }
        let def = crate::sql::default_db(&cfg);
        let mut dbs = if cfg.options.single_db { vec![] } else { visible_dbs(&cfg, self.list_databases(conn.clone()).await.unwrap_or_default(), &def) };
        dbs.retain(|d| d != &def);
        dbs.insert(0, def);
        dbs.retain(|d| !d.is_empty());
        dbs.truncate(8);
        let base = self.store.dir();
        for db in dbs {
            let t = crate::sql::target_id(&conn, Some(&db));
            if crate::search_cache::is_fresh(&base, &t, std::time::Duration::from_secs(600)) {
                continue;
            }
            let _ = self.object_index(t.clone()).await;
            let _ = self.completion_meta(t).await;
        }
        Ok(())
    }

    /// 접속하지 않은 Data Source 도 찾을 수 있게 저장해 둔 객체·컬럼 목록
    pub fn cached_index(&self) -> AppResult<Vec<crate::search_cache::CachedIndex>> {
        let ids: Vec<String> = self.store.list()?.into_iter().map(|c| c.id).collect();
        Ok(crate::search_cache::load_all(&self.store.dir(), |c| ids.iter().any(|i| i == c)))
    }

    pub async fn full_text_search(&self, id: String, req: crate::meta::FtsRequest) -> AppResult<Vec<crate::meta::FtsHit>> {
        crate::meta::full_text_search(&ensure_pool(self, &id).await?, &req).await
    }

    pub async fn redis_scan(&self, id: String, cursor: String, pattern: String, count: usize, type_filter: Option<String>) -> AppResult<crate::redisdb::ScanPage> {
        crate::redisdb::scan_page(&redis_pool(self, &id).await?, &cursor, &pattern, count, type_filter.as_deref()).await
    }

    pub async fn redis_key(&self, id: String, key: String, limit: Option<usize>) -> AppResult<crate::redisdb::KeyDetail> {
        crate::redisdb::key_detail(&redis_pool(self, &id).await?, &key, limit.unwrap_or(1000)).await
    }

    /// 터미널: redis-cli 처럼 한 줄을 그대로
    pub async fn redis_exec_line(&self, id: String, line: String) -> AppResult<serde_json::Value> {
        self.check_read_only(&id, &line)?;
        let args: Vec<String> = crate::redisdb::parse_command(&line)?.into_iter().map(|a| String::from_utf8_lossy(&a).into_owned()).collect();
        crate::redisdb::exec_args(&redis_pool(self, &id).await?, &args).await
    }

    pub async fn redis_exec(&self, id: String, args: Vec<String>) -> AppResult<serde_json::Value> {
        self.check_read_only_redis(&id, args.first().map(|s| s.as_str()).unwrap_or(""), args.get(1).map(|s| s.as_str()).unwrap_or(""))?;
        crate::redisdb::exec_args(&redis_pool(self, &id).await?, &args).await
    }

    pub async fn export_connections(&self, path: String) -> AppResult<usize> {
        self.store.export_to(std::path::Path::new(&path))
    }

    /// 가져오기 미리보기: 추가·변경될 Data Source 와 위험한 변경 (Read-only 해제, 서버·SSH·시작 스크립트 변경)
    pub fn import_preview(&self, path: String) -> AppResult<Vec<String>> {
        self.store.import_preview(std::path::Path::new(&path))
    }

    /// 처음 보는 SSH 서버 키를 사용자가 확인하고 신뢰 (known_hosts 에 등록)
    pub fn trust_ssh_host(&self, host: String, port: u16) -> AppResult<String> {
        crate::tunnel::trust_host_key(&host, port)
    }

    pub async fn import_connections(&self, path: String) -> AppResult<ImportResult> {
        let (added, updated, forget) = self.store.import_from(std::path::Path::new(&path))?;
        // 같은 id 로 다른 서버를 가져오면 예전 비밀번호를 지운다 (새 서버로 보내지 않게)
        for (id, password, ssh) in forget {
            if password {
                self.set_secret(&id, None)?;
            }
            if ssh {
                self.set_secret(&store::ssh_secret_key(&id), None)?;
            }
            drop_pool(self, &id).await;
        }
        Ok(ImportResult { added, updated })
    }

    pub async fn export_csv(&self, id: String, sql: String, path: String) -> AppResult<u64> {
        self.check_read_only(&id, &sql)?;
        let pool = ensure_pool(self, &id).await?;
        pool.export_csv(&sql, std::path::Path::new(&path)).await
    }

    pub async fn list_databases(&self, id: String) -> AppResult<Vec<String>> {
        crate::log!("[qurry] list_databases {id}");
        let r = async { crate::meta::list_databases(&ensure_pool(self, &id).await?).await }.await;
        if let Err(e) = &r {
            crate::log!("[qurry] list_databases error: {e}");
        }
        r
    }

    pub async fn list_schemas(&self, id: String) -> AppResult<Vec<crate::meta::SchemaInfo>> {
        crate::log!("[qurry] list_schemas {id}");
        let r = async { crate::meta::list_schemas(&ensure_pool(self, &id).await?).await }.await;
        match &r {
            Ok(v) => crate::log!("[qurry] list_schemas → {} schemas", v.len()),
            Err(e) => crate::log!("[qurry] list_schemas error: {e}"),
        }
        r
    }

    pub async fn list_objects(&self, id: String, schema: String) -> AppResult<crate::meta::SchemaObjects> {
        crate::meta::list_objects(&ensure_pool(self, &id).await?, &schema).await
    }

    pub async fn table_detail(&self, id: String, schema: String, table: String) -> AppResult<crate::meta::TableDetail> {
        crate::meta::table_detail(&ensure_pool(self, &id).await?, &schema, &table).await
    }

    /// pgvector: 이 행과 가까운 행을 찾는 SQL (벡터 인덱스의 거리 연산자로). pk = (열, 값)
    pub async fn similar_rows_sql(&self, id: String, schema: String, table: String, column: String, pk: Vec<(String, Option<String>)>) -> AppResult<String> {
        if pk.is_empty() {
            return Err(AppError::Msg(format!("{schema}.{table} has no primary key — can't pick the row to compare")));
        }
        let d = self.table_detail(id, schema.clone(), table.clone()).await?;
        let defs: Vec<&str> = d.indexes.iter().map(|i| i.definition.as_str()).collect();
        Ok(crate::sql::similar_rows_sql(&schema, &table, &column, &pk, crate::sql::vector_operator(&column, &defs), 20))
    }

    pub async fn object_ddl(&self, id: String, schema: String, name: String, kind: String) -> AppResult<String> {
        crate::meta::object_ddl(&ensure_pool(self, &id).await?, &schema, &name, &kind).await
    }

    pub async fn completion_meta(&self, id: String) -> AppResult<Vec<crate::meta::CompletionTable>> {
        let pool = ensure_pool(self, &id).await?;
        let r = crate::meta::completion_meta(&pool).await?;
        if !matches!(pool, Pool::Redis(..)) {
            crate::search_cache::save_columns(&self.store.dir(), &id, &r);
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 호스트만 바꿔 Test / Save 하면 저장된 비밀번호를 새 서버로 보내지 않고 다시 입력하라고 한다 (접속하기 전에)
    #[tokio::test]
    async fn saved_password_is_not_sent_to_a_changed_server() {
        let dir = std::env::temp_dir().join(format!("qurry-pw-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "a".into(), name: "a".into(), kind: crate::store::DbKind::Postgres, host: "127.0.0.1".into(), port: 1,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        let saved = st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        let mut moved = saved.clone();
        moved.host = "attacker.example".into();
        let e = st.test_connection(moved.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("enter the password again"), "{e}");
        let e = st.save_connection(moved.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("enter the password again"), "{e}");
        // 새 비밀번호를 주면 저장된다 · 이름만 바꾸는 것은 그대로
        st.save_connection(moved, Some("pw2".into()), None).await.unwrap();
        let mut renamed = st.store.get("a").unwrap().unwrap();
        renamed.name = "b".into();
        st.save_connection(renamed, None, None).await.unwrap();
        assert_eq!(st.secret("a").unwrap().as_deref(), Some("pw2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// MCP 서버는 MCP 에 보이지 않는 Data Source 의 비밀번호를 꺼내지 않는다. 앱은 그대로
    #[test]
    fn mcp_hidden_source_secrets_stay_locked() {
        let dir = std::env::temp_dir().join(format!("qurry-mcp-{}", uuid::Uuid::new_v4()));
        let mut st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mut cfg = ConnectionConfig {
            id: "prod".into(), name: "prod".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: true,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        cfg.options.confirm_changes = true;
        st.store.save(&cfg).unwrap();
        st.set_secret("prod", Some("pw")).unwrap();
        st.set_secret("prod#ssh", Some("sshpw")).unwrap();
        assert_eq!(st.secret("prod").unwrap().as_deref(), Some("pw"));
        st.mcp = true;
        assert!(st.secret("prod").is_err() && st.secret("prod#ssh").is_err());
        cfg.options.mcp = Some("read".into());
        st.store.save(&cfg).unwrap();
        assert_eq!(st.secret("prod").unwrap().as_deref(), Some("pw"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Auto-disconnect: 유휴 시간이 지나면 풀·세션을 닫는다. keep-alive 는 세션을 살려 둔다.
    #[tokio::test]
    #[ignore]
    async fn housekeeping_keepalive_and_auto_disconnect() {
        let dir = std::env::temp_dir().join(format!("dbtool-hk-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mut cfg = ConnectionConfig {
            id: "hk".into(), name: "hk".into(), kind: crate::store::DbKind::Postgres, host: "localhost".into(), port: 5432,
            user: std::env::var("USER").unwrap(), database: "postgres".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        cfg.options.keep_alive_secs = Some(1);
        cfg.options.auto_disconnect_secs = Some(2);
        state.store.save(&cfg).unwrap();
        let sess = ensure_session(&state, "hk", "tab1").await.unwrap();
        drop(sess);
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert!(housekeeping(&state).await.is_empty(), "not idle long enough yet");
        assert!(state.last_ping.lock().unwrap().contains_key("hk"), "keep-alive ran");
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert_eq!(housekeeping(&state).await, vec!["hk".to_string()]);
        assert!(state.pools.read().await.is_empty() && state.sessions.read().await.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
