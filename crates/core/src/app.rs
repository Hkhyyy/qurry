//! App state: Data Source pools, sessions, SSH tunnels, passwords. GUI (Tauri), TUI and MCP use the same methods.
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
    /// Console tab id → session
    pub sessions: RwLock<HashMap<String, SessionEntry>>,
    /// Connection id → SSH tunnel (lives as long as the pool)
    pub tunnels: RwLock<HashMap<String, crate::tunnel::Tunnel>>,
    /// Password store (encrypted file). Avoids keychain popups.
    pub secret_file: crate::secrets::SecretFile,
    /// Data Source id → last used time (Auto-disconnect)
    pub last_used: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Data Source id → last keep-alive time
    pub last_ping: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Tab id → Manual mode (keep the same mode when reconnecting opens a new session)
    pub tab_manual: std::sync::Mutex<HashMap<String, bool>>,
    /// Query history and snippets (shared by GUI and TUI)
    pub history: crate::history::History,
    /// Production audit log (changes to Production Data Sources · undo SQL)
    pub audit: crate::audit::Audit,
    /// Export/import job id → progress and cancel
    pub jobs: std::sync::Mutex<HashMap<String, std::sync::Arc<crate::transfer::Job>>>,
    /// MCP server: never reads passwords of Data Sources hidden from MCP (crate::store::McpAccess)
    pub mcp: bool,
}

impl AppState {
    /// Saved password or SSH secret for a key, read from the secrets file each time (nothing is cached in memory).
    pub fn secret(&self, key: &str) -> AppResult<Option<String>> {
        // MCP server: never reads passwords of Data Sources hidden from MCP (so it cannot connect either)
        if self.mcp {
            let id = key.strip_suffix("#ssh").unwrap_or(key);
            if let Some(c) = self.store.get(id)?.filter(|c| c.mcp_access(false) == store::McpAccess::Off) {
                return Err(AppError::Msg(format!("Data source \"{}\" is not shared with MCP (turn it on in the data source's Options)", c.name)));
            }
        }
        // No in-memory cache: another app (the GUI) may have changed the server and its password since; the file is cheap to read
        // Passwords live only in the encrypted file (the keychain holds at most the master key: crate::secrets)
        self.secret_file.get(key)
    }

    /// Where the password file's master key is kept (shown in Settings)
    pub fn secret_key_location(&self) -> crate::secrets::KeyLocation {
        crate::secrets::key_location(&self.store.dir())
    }

    /// Move the master key to the OS keychain / to a file. The key is the same, so the open store stays in use
    pub fn set_secret_key_location(&self, keychain: bool) -> AppResult<crate::secrets::KeyLocation> {
        let to = if keychain { crate::secrets::KeyLocation::Keychain } else { crate::secrets::KeyLocation::File };
        self.secret_file.ensure_key()?;
        crate::secrets::move_key(&self.store.dir(), to, &crate::secrets::OsKeychain)?;
        Ok(self.secret_key_location())
    }

    /// The secret only for a source that has one (flag, or an entry in the file): sources without a password still connect when secrets.json is damaged
    fn saved_secret(&self, key: &str, saved: bool) -> AppResult<Option<String>> {
        if saved || self.secret_file.contains(key) { self.secret(key) } else { Ok(None) }
    }

    pub fn set_secret(&self, key: &str, value: Option<&str>) -> AppResult<()> {
        self.secret_file.set(key, value)?;
        Ok(())
    }
}

impl AppState {
    pub fn new(store: Store, secret_file: crate::secrets::SecretFile) -> Self {
        let history = crate::history::History::new(&store.dir());
        let audit = crate::audit::Audit::new(&store.dir());
        Self { history, audit, store, pools: RwLock::new(HashMap::new()), sessions: RwLock::new(HashMap::new()), tunnels: RwLock::new(HashMap::new()), secret_file, last_used: Default::default(), last_ping: Default::default(), tab_manual: Default::default(), jobs: Default::default(), mcp: false }
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

/// DBs shown in the tree (scope from the Data Source › Schemas tab, system DBs hidden). Shared by GUI, TUI and the search cache
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

/// Connection target id: "<data source id>" or "<data source id>::<database>"
pub fn split_target(id: &str) -> (&str, Option<&str>) {
    match id.split_once("::") {
        Some((base, db)) if !db.is_empty() => (base, Some(db)),
        _ => (id, None),
    }
}

pub fn belongs(key: &str, id: &str) -> bool {
    key == id || key.starts_with(&format!("{id}::"))
}

/// Whether the error means the connection was lost (network, server restart, SSH tunnel). Not query errors such as syntax errors
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

/// If the connection was lost: drop the tab session (so the dead connection is not reused; the next run reconnects),
/// and close the pool too if it does not respond (e.g. the SSH tunnel died). Never resend the same statement (the server may have already run it)
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

/// Close all DB pools, sessions and tunnels of the Data Source.
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

/// If SSH is configured, open the tunnel first, then connect.
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

/// One pass: keep-alive queries / drop idle connections. Returns the ids of the disconnected Data Sources.
pub async fn housekeeping(state: &AppState) -> Vec<String> {
    use std::time::{Duration, Instant};
    let bases: Vec<String> = {
        let pools = state.pools.read().await;
        let mut b: Vec<String> = pools.keys().map(|k| split_target(k).0.to_string()).collect();
        b.sort();
        b.dedup();
        b
    };
    // Close cursors left unread for a long time (so open transactions do not block VACUUM/DDL). Skip sessions that are running
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
            // Do not disconnect while a query is running (session locked)
            // Do not disconnect if any session has a running query or uncommitted changes (disconnecting would lose the changes)
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

/// Export from a driver pool (fetch the whole result at once, then write)
pub async fn ensure_pool(state: &AppState, id: &str) -> AppResult<Pool> {
    // MCP server: never connect to hidden Data Sources (sources without a password · pools already open too)
    if state.mcp
        && let Some(c) = state.store.get(split_target(id).0)?.filter(|c| c.mcp_access(false) == store::McpAccess::Off)
    {
        return Err(AppError::Msg(format!("Data source \"{}\" is not shared with MCP", c.name)));
    }
    touch(state, id);
    if let Some(p) = state.pools.read().await.get(id) {
        return Ok(p.clone());
    }
    let (base, db) = split_target(id);
    let mut cfg = state
        .store
        .get(base)?
        .ok_or_else(|| AppError::Msg("Data source not found".into()))?;
    // For file DBs (SQLite, DuckDB) database is the file path: do not overwrite it with the DB name (main)
    if let Some(db) = db.filter(|_| !cfg.kind.is_file()) {
        cfg.database = db.to_string();
    }
    let password = state.saved_secret(base, cfg.password_saved)?;
    let ssh_secret = if cfg.active_ssh().is_some() { state.saved_secret(&store::ssh_secret_key(base), cfg.ssh_secret_saved)? } else { None };
    let (pool, tunnel) = connect_config(&cfg, password.as_deref(), ssh_secret.as_deref()).await.map_err(|e| match e {
        // sqlx only says "pool timed out while waiting for an open connection"
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
    // If the same tab switched to another DB, clean up and return the previous session
    let old = state.sessions.write().await.remove(tab_id);
    if let Some(e) = old {
        e.session.lock().await.discard().await;
    }
    let pool = ensure_pool(state, id).await?;
    let mut s = Session::open(&pool).await?;
    // If the tab is Manual, the new session is Manual too (silently becoming Auto after a reconnect would commit the next change right away)
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

/// Time limit for read-only (MCP) queries (so queries sent by others do not hold the server for long)
const SERVER_CHANGED: &str = "The server, port, user, type, SSH tunnel or Redis nodes changed, or SSL was turned off, its verification lowered or its CA file changed — enter the password again. The saved one is only sent to the server it was saved for.";
const SSH_CHANGED: &str = "The SSH server changed — enter the SSH password / passphrase again. The saved one is only sent to the server it was saved for.";
/// MCP reads: 60 seconds, string parsing matches guard (even if the server or role sets standard_conforming_strings = off)
const READ_ONLY_TIMEOUT_PG: &str = "SET LOCAL statement_timeout = '60s'";
const READ_ONLY_STRINGS_PG: &str = "SET LOCAL standard_conforming_strings = on";
const READ_ONLY_TIMEOUT_MY: &str = "SET SESSION max_execution_time = 60000";
/// Max size of a single Redis value readable from MCP
const REDIS_READ_MAX_BYTES: i64 = 16 << 20;
/// Max size of an MCP CSV export
const EXPORT_MAX_BYTES: u64 = 512 << 20;
const EXPORT_MAX_PLUGIN_ROWS: usize = 1_000_000;

/// MCP Redis reads (query, and reads sent to execute): the guard's limits, then what only the server knows — whole-key reads
/// (HGETALL, LRANGE 0 -1, XRANGE without COUNT …) up to REDIS_STRICT_MAX elements, and values up to REDIS_READ_MAX_BYTES.
/// Commands that aren't reads are left to the caller
async fn check_redis_read(m: &crate::redisdb::RedisConn, line: &str) -> AppResult<()> {
    use crate::guard::REDIS_STRICT_MAX;
    let args = crate::redisdb::parse_command(line.trim().trim_end_matches(';').trim())?;
    let cmd = String::from_utf8_lossy(&args[0]).to_uppercase();
    let sub = args.get(1).map(|a| String::from_utf8_lossy(a).to_uppercase()).unwrap_or_default();
    if !crate::redisdb::is_read_command(&cmd, &sub) {
        return Ok(());
    }
    let rest: Vec<String> = args.iter().skip(1).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    if let Some(why) = crate::guard::redis_strict_violation(&cmd, &rest) {
        return Err(AppError::Msg(why));
    }
    // Whole keys: count the elements (and then the bytes) before reading
    if let Some(c) = crate::guard::redis_size_check(&cmd, &rest) {
        let mut n = 0i64;
        for k in &c.keys {
            n = n.saturating_add(crate::redisdb::element_count(m, c.count, k.as_bytes()).await?);
        }
        if n > REDIS_STRICT_MAX {
            return Err(AppError::Msg(format!("{cmd} would read {n} elements — at most {REDIS_STRICT_MAX} in read-only mode; use {} (or redis_get for a preview)", c.instead)));
        }
        let mut bytes = 0i64;
        for k in &c.keys {
            bytes = bytes.saturating_add(crate::redisdb::memory_usage(m, k.as_bytes()).await?);
        }
        if bytes > REDIS_READ_MAX_BYTES {
            return Err(AppError::Msg(format!("{cmd} would read {} MB — too big to read here; use {}", bytes >> 20, c.instead)));
        }
    }
    // Commands that return a whole value: check the size before reading
    // GETRANGE with a bounded range reads at most that much
    let range_small = cmd == "GETRANGE" && matches!((rest.get(1).and_then(|a| a.parse::<i64>().ok()), rest.get(2).and_then(|a| a.parse::<i64>().ok())), (Some(a), Some(b)) if a >= 0 && b >= a && b - a < REDIS_READ_MAX_BYTES);
    let keys: &[String] = match cmd.as_str() {
        "GET" | "JSON.GET" => rest.get(..1).unwrap_or(&[]),
        "GETRANGE" if !range_small => rest.get(..1).unwrap_or(&[]),
        "MGET" => &rest,
        "JSON.MGET" => rest.split_last().map(|(_, k)| k).unwrap_or(&[]),
        _ => &[],
    };
    for k in keys {
        let n = crate::redisdb::memory_usage(m, k.as_bytes()).await?;
        if n > REDIS_READ_MAX_BYTES {
            return Err(AppError::Msg(format!("{k} is {} MB — too big to read here (use GETRANGE, or redis_get for a preview)", n >> 20)));
        }
    }
    // HGET / HMGET: one hash field can be big too
    if matches!(cmd.as_str(), "HGET" | "HMGET") && rest.len() > 1 {
        for f in &rest[1..] {
            let n = crate::redisdb::field_len(m, rest[0].as_bytes(), f.as_bytes()).await?;
            if n > REDIS_READ_MAX_BYTES {
                return Err(AppError::Msg(format!("{}.{f} is {} MB — too big to read here", rest[0], n >> 20)));
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
}

impl AppState {
    /// Run read-only (the MCP default). guard checks it (one statement, read commands, no side-effect functions); it runs inside a READ ONLY transaction
    /// with a 60-second limit, rolled back at the end. MySQL uses the prepared protocol (the server accepts only one statement). Redis: read commands only (no CONFIG, KEYS)
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
                tx.execute(READ_ONLY_STRINGS_PG).await?;
                let r = db::run_on(&mut tx, &sql, limit).await;
                let _ = tx.rollback().await;
                r
            }
            Pool::My(p) => {
                let mut conn = p.acquire().await?;
                // Always reset the time limit, even on failure, so it does not stay on the pooled connection
                let r = async {
                    conn.execute(READ_ONLY_TIMEOUT_MY).await?;
                    conn.execute("START TRANSACTION READ ONLY").await?;
                    crate::mysql::run_read_only_on(&mut conn, &sql, limit).await
                }
                .await;
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
                check_redis_read(&m, &sql).await?;
                crate::redisdb::run(&m, &sql, limit).await
            }
            // Drivers: no server-side READ ONLY transaction — only what passes the guard check (one statement, read commands)
            Pool::Plugin(c) => c.query_opts(&sql, limit, true).await,
        }
    }

    /// Read-only query result to CSV (MCP). Same rules as run_read_only for checking and running (the full result also runs in a READ ONLY transaction)
    pub async fn export_csv_read_only(&self, id: String, sql: String, path: std::path::PathBuf) -> AppResult<u64> {
        let kind = self.kind_of(&id)?;
        if let Some(why) = crate::guard::read_only_violation(&sql, kind, true) {
            return Err(AppError::Msg(why));
        }
        let pool = ensure_pool(self, &id).await?;
        // Same time limit as run_read_only, plus a file size limit (so it does not fill the disk). On failure, delete the partial file
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
                tx.execute(READ_ONLY_STRINGS_PG).await?;
                let r = db::export_pg_on(&mut tx, sql, out).await;
                let _ = tx.rollback().await;
                r?
            }
            Pool::My(p) => {
                let mut conn = p.acquire().await?;
                let r = async {
                    conn.execute(READ_ONLY_TIMEOUT_MY).await?;
                    conn.execute("START TRANSACTION READ ONLY").await?;
                    crate::mysql::export_on(&mut conn, sql, out, true).await
                }
                .await;
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

    /// `password`: None keeps the existing password, an empty string deletes it.
    pub async fn save_connection(&self, mut config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<ConnectionConfig> {
        crate::log!("[qurry] save_connection host={} user={} db={}", config.host, config.user, config.database);
        if config.id.is_empty() {
            config.id = uuid::Uuid::new_v4().to_string();
            // New sources (including duplicates) default to MCP Off unless set
            if config.options.mcp.is_none() {
                config.options.mcp = Some("off".into());
            }
        }
        // The saved-password flag is managed on the backend (None = keep the existing value)
        let prev = self.store.get(&config.id)?;
        // If the server changed, do not use the saved password for the new server: ask for it again
        // "Saved" means the password actually exists, not the flag (old imports left entries with only the flag turned off)
        if let Some(p) = &prev {
            if password.is_none() && self.secret_file.contains(&p.id) && !p.same_server(&config) {
                return Err(AppError::Msg(SERVER_CHANGED.into()));
            }
            if ssh_secret.is_none() && self.secret_file.contains(&store::ssh_secret_key(&p.id)) && config.active_ssh().is_some() && !p.same_ssh(&config) {
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
        drop_pool(self, &config.id).await; // The settings may have changed, so force a reconnect
        Ok(config)
    }

    pub async fn delete_connection(&self, id: String) -> AppResult<()> {
        drop_pool(self, &id).await;
        // A damaged secrets.json (left untouched) doesn't keep the data source: it is removed, and the error says the password stayed
        let forgot = self.set_secret(&id, None).and_then(|_| self.set_secret(&store::ssh_secret_key(&id), None));
        crate::search_cache::remove_conn(&self.store.dir(), &id);
        self.store.delete(&id)?;
        forgot.map_err(|e| AppError::Msg(format!("The data source was removed, but its saved password couldn't be: {e}")))
    }

    /// The saved password, if it may be sent to config (only when it is the same server as the saved settings)
    fn saved_password_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if self.secret_file.contains(&p.id) && !p.same_server(config) => Err(AppError::Msg(SERVER_CHANGED.into())),
            Some(p) => self.saved_secret(&config.id, p.password_saved),
            None => Ok(None),
        }
    }

    fn saved_ssh_secret_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if self.secret_file.contains(&store::ssh_secret_key(&p.id)) && !p.same_ssh(config) => Err(AppError::Msg(SSH_CHANGED.into())),
            Some(p) => self.saved_secret(&store::ssh_secret_key(&config.id), p.ssh_secret_saved),
            None => Ok(None),
        }
    }

    /// Test the connection without saving. If `password` is None, use the saved password (only when it is the same server as the saved settings).
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
            Pool::Pg(p) => db::pg_plaintext_note(p, &config).await,
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

    /// Test Connection on the SSH/SSL tab: checks only the SSH login.
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
        // If connected in plain text, say so next to the version (status bar · TUI status)
        let cfg = self.source(&id)?;
        let note = match &pool {
            Pool::My(p) => crate::mysql::plaintext_note(p, &cfg).await,
            Pool::Pg(p) => db::pg_plaintext_note(p, &cfg).await,
            _ => None,
        };
        Ok(match note {
            Some(n) => format!("{v} — {n}"),
            None => v,
        })
    }

    pub async fn disconnect(&self, id: String) -> AppResult<()> {
        drop_pool(self, &id).await;
        Ok(())
    }

    /// If `tab_id` is set, run in that console's session (cursor and cancel support).
    /// For Production Data Sources, log modifying statements to the audit log, with undo SQL for simple UPDATE / DELETE (crate::audit)
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

    /// Production audit log (newest first)
    pub fn audit_log(&self, conn: Option<String>, filter: String, limit: usize) -> Vec<crate::audit::AuditEntry> {
        self.audit.list(conn.as_deref(), &filter, limit)
    }

    /// Log the console's Commit / Roll back to the audit log (only if the logged changes were uncommitted)
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

    /// Whether the tab session has uncommitted changes (to show the Commit/Roll back buttons correctly even after an error)
    pub async fn tx_state(&self, tab_id: String) -> bool {
        let sess = self.sessions.read().await.get(&tab_id).map(|e| e.session.clone());
        match sess {
            Some(s) => s.lock().await.in_tx(),
            None => false,
        }
    }

    /// Block write commands on Read-only Redis Data Sources
    fn check_read_only_redis(&self, id: &str, cmd: &str, sub: &str) -> AppResult<()> {
        let cfg = self.source(id)?;
        let (cmd, sub) = (cmd.to_uppercase(), sub.to_uppercase());
        if cfg.read_only && !crate::redisdb::is_read_command(&cmd, &sub) {
            return Err(AppError::Msg(format!("Data source \"{}\" is read-only: {cmd} changes data", cfg.name)));
        }
        Ok(())
    }

    /// Data Source settings (read-only flag, etc.)
    pub(crate) fn source(&self, id: &str) -> AppResult<ConnectionConfig> {
        self.store.get(split_target(id).0)?.ok_or_else(|| AppError::Msg("Data source not found".into()))
    }

    /// Block data-modifying statements on Read-only Data Sources (enforced here, regardless of the UI)
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
        // Cancel a running query first (otherwise the connection and cleanup stay tied up until it ends)
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

    /// Statements in one transaction. exact_one = saving table edits (roll back everything unless each UPDATE/DELETE hits exactly 1 row)
    pub async fn execute_batch(&self, id: String, statements: Vec<String>, exact_one: bool) -> AppResult<u64> {
        self.execute_batch_with(id, statements, exact_one, false).await
    }

    /// MCP execute: like execute_batch, but PostgreSQL / MySQL run each statement as a prepared statement (one statement per item, enforced by the server)
    /// Redis reads among the items get the same size checks as run_read_only
    pub async fn execute_batch_prepared(&self, id: String, statements: Vec<String>) -> AppResult<u64> {
        if self.kind_of(&id)? == store::DbKind::Redis {
            let m = redis_pool(self, &id).await?;
            for st in &statements {
                check_redis_read(&m, st).await?;
            }
        }
        self.execute_batch_with(id, statements, false, true).await
    }

    async fn execute_batch_with(&self, id: String, statements: Vec<String>, exact_one: bool, prepared: bool) -> AppResult<u64> {
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
            Pool::Pg(pg) => session::execute_batch_with(&pg, &statements, exact_one, prepared).await,
            Pool::My(my) => crate::mysql::execute_batch_with(&my, &statements, exact_one, prepared).await,
            Pool::Redis(m, _) => {
                for st in &statements {
                    crate::redisdb::run(&m, st, 1).await?;
                }
                Ok(statements.len() as u64)
            }
            // Drivers: one by one (drivers know nothing about transactions — only exact_one is enforced)
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

    /// Save to the search cache on every read (Redis keys are not saved)
    pub async fn object_index(&self, id: String) -> AppResult<Vec<crate::meta::IndexEntry>> {
        let pool = ensure_pool(self, &id).await?;
        let r = crate::meta::object_index(&pool).await?;
        if !matches!(pool, Pool::Redis(..)) {
            crate::search_cache::save_objects(&self.store.dir(), &id, &r);
        }
        Ok(r)
    }

    /// On connect, immediately read (in the background) the objects and columns of the visible DBs to fill the search cache.
    /// That way Search Everywhere finds them next time without connecting. Skip DBs filled within the last 10 minutes
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

    /// Saved object and column lists, so Data Sources that are not connected can be searched too
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

    /// Terminal: one line as is, like redis-cli
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

    /// Import preview: Data Sources to be added or changed, and risky changes (Read-only turned off, server/SSH/startup script changed)
    pub fn import_preview(&self, path: String) -> AppResult<Vec<String>> {
        self.store.import_preview(std::path::Path::new(&path))
    }

    /// The user confirms and trusts an unknown SSH server key (added to known_hosts)
    pub fn trust_ssh_host(&self, host: String, port: u16) -> AppResult<String> {
        crate::tunnel::trust_host_key(&host, port)
    }

    pub async fn import_connections(&self, path: String) -> AppResult<ImportResult> {
        let (added, updated, forget) = self.store.import_from(std::path::Path::new(&path))?;
        // Importing a different server under the same id deletes the old password (so it is not sent to the new server)
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

    /// pgvector: SQL that finds rows close to this row (using the vector index's distance operator). pk = (column, value)
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

    /// Changing only the host and doing Test / Save does not send the saved password to the new server; it asks for it again (before connecting)
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
        // Keep the saved password even if only the flag is off (old imports)
        let mut flag_off = saved.clone();
        flag_off.password_saved = false;
        st.store.save(&flag_off).unwrap();
        let e = st.test_connection(moved.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("enter the password again"), "{e}");
        // Giving a new password saves it · renaming only leaves it as is
        st.save_connection(moved, Some("pw2".into()), None).await.unwrap();
        let mut renamed = st.store.get("a").unwrap().unwrap();
        renamed.name = "b".into();
        st.save_connection(renamed, None, None).await.unwrap();
        assert_eq!(st.secret("a").unwrap().as_deref(), Some("pw2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// New or duplicated sources are MCP Off. The MCP server does not connect to hidden sources (even without a password)
    #[tokio::test]
    async fn new_sources_start_hidden_from_mcp() {
        let dir = std::env::temp_dir().join(format!("qurry-mcpnew-{}", uuid::Uuid::new_v4()));
        let mut st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: String::new(), name: "trust".into(), kind: crate::store::DbKind::Postgres, host: "127.0.0.1".into(), port: 1,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        let saved = st.save_connection(cfg, None, None).await.unwrap();
        assert_eq!(saved.options.mcp.as_deref(), Some("off"));
        st.mcp = true;
        let e = st.connect(saved.id.clone()).await.unwrap_err().to_string();
        assert!(e.contains("not shared with MCP"), "{e}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The MCP server does not read passwords of Data Sources hidden from MCP. The app is unaffected
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

    /// A damaged secrets.json only blocks sources that have a saved password; deleting still removes the source (the file is left as is)
    #[tokio::test]
    async fn damaged_secrets_only_block_sources_with_a_password() {
        let dir = std::env::temp_dir().join(format!("qurry-dmg-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mk = |id: &str| ConnectionConfig {
            id: id.into(), name: id.into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, password_saved: false,
            ssh: Some(crate::store::SshConfig { enabled: true, host: "127.0.0.1".into(), port: 1, user: "me".into(), auth: Default::default(), key_path: String::new() }),
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        st.save_connection(mk("pw"), Some("secret".into()), None).await.unwrap();
        st.save_connection(mk("trust"), None, None).await.unwrap();
        std::fs::write(dir.join("secrets.json"), "{ \"pw\": \"tru").unwrap();
        let e = st.connect("pw".into()).await.unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
        // No saved password or SSH secret: tries the SSH server (nothing listens on port 1), not the secrets file
        let e = st.connect("trust".into()).await.unwrap_err().to_string();
        assert!(!e.contains("damaged"), "{e}");
        let e = st.test_connection(mk("trust"), None, None).await.unwrap_err().to_string();
        assert!(!e.contains("damaged"), "{e}");
        let e = st.delete_connection("pw".into()).await.unwrap_err().to_string();
        assert!(e.contains("was removed") && e.contains("damaged"), "{e}");
        assert!(st.store.get("pw").unwrap().is_none());
        assert_eq!(std::fs::read_to_string(dir.join("secrets.json")).unwrap(), "{ \"pw\": \"tru");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Auto-disconnect: close pools and sessions after the idle time. keep-alive keeps the session alive.
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
