//! App state: Data Source pools, sessions, SSH / AWS SSM tunnels, passwords. GUI (Tauri), TUI and MCP use the same methods.
use crate::backend::{Backend, dispatch};
use crate::db::{self, Pool, QueryResult};
use crate::error::{AppError, AppResult};
use crate::session::{self, Session};
use crate::store::{self, ConnectionConfig, Store};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::Ordering::SeqCst;
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
    /// Data source id (not `id::db`) → SSH / AWS SSM tunnel, shared by the pools of all its databases (they go to the same host:port)
    pub tunnels: RwLock<HashMap<String, crate::tunnel::Tunnel>>,
    /// Data source id → its tunnel opening and its disconnect count (see Connecting)
    connecting: std::sync::Mutex<HashMap<String, std::sync::Arc<Connecting>>>,
    /// Password store (encrypted file). Avoids keychain popups.
    pub secret_file: crate::secrets::SecretFile,
    /// Data Source id → last used time (Auto-disconnect)
    pub last_used: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Data Source id → last keep-alive time
    pub last_ping: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    /// Tab id → Manual mode (keep the same mode when reconnecting opens a new session)
    pub tab_manual: std::sync::Mutex<HashMap<String, bool>>,
    /// Tab id → its result with more rows to load: outlives the tab session, so loading more can reopen it on a new one
    more: std::sync::Mutex<HashMap<String, Kept>>,
    /// Counts the console runs (Kept::run)
    runs: std::sync::atomic::AtomicU64,
    /// Query history and snippets (shared by GUI and TUI)
    pub history: crate::history::History,
    /// Production audit log (changes to Production Data Sources · undo SQL)
    pub audit: crate::audit::Audit,
    /// Export/import job id → progress and cancel
    pub jobs: std::sync::Mutex<HashMap<String, std::sync::Arc<crate::transfer::Job>>>,
    /// MCP server: never reads passwords of Data Sources hidden from MCP (crate::store::McpAccess)
    pub mcp: bool,
    /// Secrets key → password of a temporary data source (Store::temporary): kept in memory only, never in the secrets file
    temporary: std::sync::Mutex<HashMap<String, String>>,
}

impl AppState {
    /// Opens a temporary data source (`qurry-tui postgres://…`) for this process: it is never saved, so no other app (MCP included) sees it
    pub fn open_temporary(&self, t: crate::dsn::Temporary) -> ConnectionConfig {
        let mut config = t.config;
        config.id = format!("tmp-{}", uuid::Uuid::new_v4());
        config.password_saved = t.password.is_some();
        config.ssh_secret_saved = false;
        if let Some(p) = t.password {
            self.temporary.lock().unwrap().insert(config.password_key(), p);
        }
        self.store.add_temporary(config.clone());
        config
    }

    /// Saved password or SSH secret for a key, read from the secrets file each time (nothing is cached in memory).
    pub fn secret(&self, key: &str) -> AppResult<Option<String>> {
        if self.store.is_temporary(store::secret_owner(key)) {
            return Ok(self.temporary.lock().unwrap().get(key).cloned());
        }
        // MCP server: never reads passwords of Data Sources hidden from MCP (so it cannot connect either)
        if self.mcp {
            let id = store::secret_owner(key);
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
    pub(crate) fn saved_secret(&self, key: &str, saved: bool) -> AppResult<Option<String>> {
        if saved || self.secret_file.contains(key)? { self.secret(key) } else { Ok(None) }
    }

    pub fn set_secret(&self, key: &str, value: Option<&str>) -> AppResult<()> {
        if self.store.is_temporary(store::secret_owner(key)) {
            let mut m = self.temporary.lock().unwrap();
            match value {
                Some(v) => m.insert(key.to_string(), v.to_string()),
                None => m.remove(key),
            };
            return Ok(());
        }
        self.secret_file.set(key, value)?;
        Ok(())
    }

    /// Forgets DB passwords saved under `<id>#ssm` whose data source is gone: an older app that deletes an AWS SSM source removes
    /// only `<id>` and `<id>#ssh`. Only `#ssm` entries: older apps never write them, so a source an older app is saving right now
    /// can't lose its password. Under the store lock, so a new source this version is saving (its `#ssm` set, connections.json
    /// not written yet) isn't taken for a deleted one. Nothing is removed while connections.json can't be read
    pub fn forget_orphan_ssm_passwords(&self) -> AppResult<()> {
        self.store.locked(|all| {
            let ids: std::collections::HashSet<String> = all.into_iter().map(|c| c.id).collect();
            for key in self.secret_file.names()? {
                if key.ends_with("#ssm") && !ids.contains(store::secret_owner(&key)) {
                    self.secret_file.set(&key, None)?;
                }
            }
            Ok(())
        })
    }
}

impl AppState {
    pub fn new(store: Store, secret_file: crate::secrets::SecretFile) -> Self {
        let history = crate::history::History::new(&store.dir());
        let audit = crate::audit::Audit::new(&store.dir());
        Self { history, audit, store, pools: RwLock::new(HashMap::new()), sessions: RwLock::new(HashMap::new()), tunnels: RwLock::new(HashMap::new()), connecting: Default::default(), secret_file, last_used: Default::default(), last_ping: Default::default(), tab_manual: Default::default(), more: Default::default(), runs: Default::default(), jobs: Default::default(), mcp: false, temporary: Default::default() }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    #[serde(flatten)]
    pub config: ConnectionConfig,
    pub has_password: bool,
    pub connected: bool,
    /// From the qurry-tui command line, only in this process (Store::temporary)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub temporary: bool,
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
        // PostgreSQL ended the session (FATAL): terminated by an administrator or a server restart, crash, idle (in transaction) session timeout
        // MySQL: idle session timeout (4031), connection killed (1927), server shutdown (1053), gone away / lost during query (2006 / 2013)
        AppError::Db(sqlx::Error::Database(d)) => {
            matches!(d.code().as_deref(), Some("57P01" | "57P02" | "57P05" | "25P03")) || matches!(crate::mysql::error_number(e), Some(4031 | 1927 | 1053 | 2006 | 2013))
        }
        AppError::Io(_) => true,
        AppError::Msg(m) => {
            let m = m.to_lowercase();
            ["connection reset", "connection refused", "connection closed", "broken pipe", "timed out", "unexpected end of file", "not connected"].iter().any(|k| m.contains(k))
        }
        _ => false,
    }
}

/// What a console statement saw of its session's transaction (AppState::run_with_undo)
#[derive(Default)]
struct TxSeen {
    /// A transaction was open before it ran
    open: bool,
    /// A change had run in it (Session::tx_changed)
    changed: bool,
    /// How the statement ended it by itself (Session::tx_ended)
    ended: Option<&'static str>,
}

/// A tab's result with more rows to load (AppState::more)
#[derive(Clone)]
struct Kept {
    conn_id: String,
    /// The data source's settings when it ran: reopened only on the same server and database
    cfg: ConnectionConfig,
    /// Which console run it came from (AppState::runs): a reopen that lost a race with a newer run in the tab stops
    run: u64,
    more: session::More,
}

/// Why a result whose session was closed isn't reopened on a new one (AppState::fetch_more): None if it may be.
/// now = the data source's settings now; manual = the tab is in Manual mode now (the new session would read it in a transaction of its own)
fn reopen_refused(k: &Kept, now: &ConnectionConfig, manual: bool) -> Option<String> {
    use session::RECONNECTED;
    let (m, kind) = (&k.more, now.kind);
    // What a new connection runs before the statement (backend after_connect): it can change what the rows say
    let setup = |c: &ConnectionConfig| (c.options.startup_script.trim().to_string(), c.options.time_zone.as_deref().map(str::trim).unwrap_or_default().to_string(), c.options.isolation.clone());
    let why = if m.tx || manual {
        "it was read in a transaction (Manual mode or BEGIN) that ended with the old connection, so the next rows might not line up"
    } else if !k.cfg.same_server(now) || k.cfg.database != now.database {
        "the data source's server or database changed since"
    } else if setup(&k.cfg) != setup(now) {
        "the data source's startup script, time zone or isolation level changed since"
    } else if m.schema.is_none() {
        "Qurry couldn't read which schema it ran in"
    // Loading more runs the statement again: only a plain read, which nobody is asked about (Production confirmation, dangerous
    // statements), that takes no row locks
    } else if !crate::guard::is_read(&m.sql, kind) || crate::guard::unsafe_statement(&m.sql, kind).is_some() {
        return Some(format!("{RECONNECTED}, and loading more would run the statement again, but it isn't a plain read. Run it again yourself to load more rows."));
    } else if crate::guard::locks_rows(&m.sql, kind) {
        return Some(format!("{RECONNECTED}, and loading more would run the statement again, but it locks rows (FOR UPDATE, FOR SHARE, LOCK IN SHARE MODE …). Run it again yourself to load more rows."));
    } else {
        return None;
    };
    Some(format!("{RECONNECTED}, and {why}. Run the query again to load more rows."))
}

/// If the connection was lost: drop the tab session (so the dead connection is not reused; the next run reconnects),
/// and close the pool too if it does not respond (e.g. the SSH tunnel died). Never resend the same statement (the server may have already run it).
/// tx = how the console's open transaction ended (audit::ended_by), None if none was open
async fn recover(state: &AppState, id: &str, tab: Option<&str>, tx: Option<&str>, e: AppError) -> AppError {
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
            // The Data Source's other consoles lost their connections too: the server rolled back their open transactions
            drop_pool_logged(state, split_target(id).0, crate::audit::BY_SERVER).await;
        }
    }
    crate::log!("[qurry] connection lost on {id}: {e}");
    AppError::Msg(match tx {
        Some(crate::audit::COMMIT_UNKNOWN) => format!("{e} — {}", qurry_shared::errors::COMMIT_UNKNOWN_ERR),
        Some(_) => format!("{e} — connection lost; the open transaction was rolled back by the server. Run it again to reconnect."),
        None => format!("{e} — connection lost. Run it again to reconnect."),
    })
}

/// One data source's connects
#[derive(Default)]
struct Connecting {
    /// Held while the shared tunnel opens, so callers arriving together start one SSH / SSM session. Then the time and message
    /// of its failure (only the tunnel's: a database's own connect error stays with its caller)
    failed: tokio::sync::Mutex<Option<(std::time::Instant, String)>>,
    /// drop_pool counts up: a connect that started before keeps no pool or tunnel (its tunnel may be gone, its settings old)
    generation: std::sync::atomic::AtomicU64,
}

impl AppState {
    /// Created only for a source that exists (ensure_pool looks it up first) and removed when it is deleted
    fn connecting(&self, base: &str) -> std::sync::Arc<Connecting> {
        self.connecting.lock().unwrap().entry(base.to_string()).or_default().clone()
    }
}

/// drop_pool, and close in the Production log the logged transactions of the consoles it closed (the server rolls them back): word = how
pub(crate) async fn drop_pool_logged(state: &AppState, id: &str, word: &str) {
    let tabs = drop_pool(state, id).await;
    let Ok(cfg) = state.source(id) else { return };
    for (t, conn) in tabs {
        if let Err(e) = state.audit.tx_closed(&conn, &cfg.name, &t, word) {
            crate::log!("[qurry] {e}");
        }
    }
}

/// Close all DB pools, sessions and tunnels of the Data Source. → the consoles whose sessions it closed (tab id, connection id)
pub async fn drop_pool(state: &AppState, id: &str) -> Vec<(String, String)> {
    // No entry: no connect of it has started, so none to stop
    if let Some(c) = state.connecting.lock().unwrap().get(split_target(id).0) {
        c.generation.fetch_add(1, SeqCst);
    }
    // The tunnel goes before the pools: a connect that saw this generation and kept its pool while it was still open (keep_pool)
    // has that pool closed below
    state.tunnels.write().await.retain(|k, _| !belongs(k, id));
    let mut tabs = Vec::new();
    state.sessions.write().await.retain(|t, e| {
        let gone = belongs(&e.conn_id, id);
        if gone {
            tabs.push((t.clone(), e.conn_id.clone()));
        }
        !gone
    });
    let removed: Vec<Pool> = {
        let mut pools = state.pools.write().await;
        let keys: Vec<String> = pools.keys().filter(|k| belongs(k, id)).cloned().collect();
        keys.into_iter().filter_map(|k| pools.remove(&k)).collect()
    };
    for p in removed {
        p.close().await;
    }
    state.last_ping.lock().unwrap().retain(|k, _| !belongs(k, id));
    tabs
}

/// If an SSH or AWS SSM tunnel is configured, open it first, then connect through its local port.
pub async fn connect_config(
    cfg: &ConnectionConfig,
    password: Option<&str>,
    ssh_secret: Option<&str>,
) -> AppResult<(Pool, Option<crate::tunnel::Tunnel>)> {
    let tunnel = open_tunnel(cfg, ssh_secret).await?;
    let mark = tunnel.as_ref().and_then(|t| t.ssm_mark());
    match db::connect_via(cfg, password, tunnel.as_ref().map(|t| t.local_port)).await {
        Ok(pool) => Ok((pool, tunnel)),
        Err(e) => Err(crate::ssm::connect_error(mark, &cfg.host, cfg.port, e).await),
    }
}

async fn open_tunnel(cfg: &ConnectionConfig, ssh_secret: Option<&str>) -> AppResult<Option<crate::tunnel::Tunnel>> {
    Ok(match (cfg.active_ssh(), cfg.active_ssm()) {
        (Some(_), Some(_)) => return Err(AppError::Msg("Both an SSH and an AWS SSM tunnel are turned on — choose one (GUI: SSH/SSL tab · TUI: Tunnel).".into())),
        (Some(ssh), None) => Some(crate::tunnel::open(ssh, ssh_secret, &cfg.host, cfg.port).await?),
        (None, Some(ssm)) => Some(crate::ssm::open(ssm, &cfg.host, cfg.port).await?),
        (None, None) => None,
    })
}

/// The data source's tunnel: None without one, Some(false) once it has stopped (the aws CLI exited: idle timeout, network, session ended)
async fn tunnel_alive(state: &AppState, base: &str) -> Option<bool> {
    state.tunnels.read().await.get(base).map(|t| t.alive())
}

/// The local port of the data source's open tunnel
pub async fn tunnel_port(state: &AppState, id: &str) -> Option<u16> {
    state.tunnels.read().await.get(split_target(id).0).map(|t| t.local_port)
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
        // A stopped tunnel's port is free for anyone on this machine: don't let the pools (keep-alive) connect to it again
        if tunnel_alive(state, &base).await == Some(false) {
            drop_pool_logged(state, &base, crate::audit::BY_SERVER).await;
            dropped.push(base);
            continue;
        }
        let Ok(Some(cfg)) = state.store.get(&base) else { continue };
        let now = Instant::now();
        if let Some(secs) = cfg.options.auto_disconnect_secs.filter(|s| *s > 0) {
            let last = *state.last_used.lock().unwrap().entry(base.clone()).or_insert(now);
            // Do not disconnect while a query is running (session locked)
            // Do not disconnect if any session has a running query or uncommitted changes (disconnecting would lose the changes)
            let busy = state.sessions.read().await.values().any(|e| belongs(&e.conn_id, &base) && e.session.try_lock().map(|g| g.in_tx()).unwrap_or(true));
            if !busy && now.duration_since(last) >= Duration::from_secs(secs as u64) {
                drop_pool_logged(state, &base, crate::audit::DISCONNECTED).await;
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
    let (base, db) = split_target(id);
    let pool = state.pools.read().await.get(id).cloned();
    if let Some(p) = pool
        && tunnel_alive(state, base).await != Some(false)
    {
        return Ok(p);
    }
    let mut cfg = state
        .store
        .get(base)?
        .ok_or_else(|| AppError::Msg("Data source not found".into()))?;
    // For file DBs (SQLite, DuckDB) database is the file path: do not overwrite it with the DB name (main)
    if let Some(db) = db.filter(|_| !cfg.kind.is_file()) {
        cfg.database = db.to_string();
    }
    let password = state.saved_secret(&cfg.password_key(), cfg.password_saved)?;
    let ssh_secret = if cfg.active_ssh().is_some() { state.saved_secret(&store::ssh_secret_key(base), cfg.ssh_secret_saved)? } else { None };
    let c = state.connecting(base);
    let (port, g) = if cfg.tunneled() {
        let (port, g) = shared_tunnel(state, &c, base, &cfg, ssh_secret.as_deref()).await?;
        // Another caller for the same database may have connected while this one waited for the tunnel
        if let Some(p) = state.pools.read().await.get(id) {
            return Ok(p.clone());
        }
        (Some(port), g)
    } else {
        (None, c.generation.load(SeqCst))
    };
    let mark = state.tunnels.read().await.get(base).and_then(|t| t.ssm_mark());
    // Each database connects on its own: one that is missing or refused fails only for its caller, not for the whole source
    let pool = match connect_pool(&cfg, password.as_deref(), port).await {
        Ok(p) => p,
        Err(e) => return Err(crate::ssm::connect_error(mark, &cfg.host, cfg.port, e).await),
    };
    keep_pool(state, &c, g, id, port, pool).await
}

/// The local port of the source's tunnel, and the generation it belongs to. The first caller opens it: one SSH / SSM session
/// per source, not per database. Callers that waited while it failed get its error instead of trying again one after
/// another (each try can take the whole timeout)
async fn shared_tunnel(state: &AppState, c: &Connecting, base: &str, cfg: &ConnectionConfig, ssh_secret: Option<&str>) -> AppResult<(u16, u64)> {
    let asked = std::time::Instant::now();
    let mut failed = c.failed.lock().await;
    // Read before the tunnel: a disconnect after this also drops the pool that connects through it
    let g = c.generation.load(SeqCst);
    // Its own statement: the read lock must be released before drop_pool takes the write lock
    let open = state.tunnels.read().await.get(base).map(|t| (t.alive(), t.local_port));
    match open {
        Some((true, port)) => return Ok((port, g)),
        // Its pools would connect to a port nobody holds now (or someone else does): open the tunnel and the pools again
        Some((false, _)) => {
            crate::log!("[qurry] the tunnel of {base} has stopped; reconnecting");
            drop_pool_logged(state, base, crate::audit::BY_SERVER).await;
        }
        None => {
            if let Some((_, e)) = failed.as_ref().filter(|(at, _)| *at >= asked) {
                return Err(AppError::Msg(e.clone()));
            }
        }
    }
    let g = c.generation.load(SeqCst);
    let t = match open_tunnel(cfg, ssh_secret).await.and_then(|t| t.ok_or_else(|| AppError::Msg("No tunnel is turned on".into()))) {
        Ok(t) => t,
        Err(e) => {
            *failed = Some((std::time::Instant::now(), e.to_string()));
            return Err(e);
        }
    };
    *failed = None;
    let port = t.local_port;
    let mut tunnels = state.tunnels.write().await;
    // Disconnected meanwhile (the settings may be old): the tunnel isn't kept
    if c.generation.load(SeqCst) != g {
        return Err(AppError::Msg(DISCONNECTED_WHILE_CONNECTING.into()));
    }
    tunnels.insert(base.to_string(), t);
    Ok((port, g))
}

const DISCONNECTED_WHILE_CONNECTING: &str = "The data source was disconnected while connecting. Run it again.";

async fn connect_pool(cfg: &ConnectionConfig, password: Option<&str>, port: Option<u16>) -> AppResult<Pool> {
    db::connect_via(cfg, password, port).await.map_err(|e| match e {
        // sqlx only says "pool timed out while waiting for an open connection"
        AppError::Db(sqlx::Error::PoolTimedOut) => AppError::Msg(format!(
            "Can't connect to {}:{} — the server didn't answer within 10s. Check that it's up (VPN / SSH tunnel), then run it again.",
            cfg.host, cfg.port
        )),
        e => e,
    })
}

/// Keeps the new pool unless the source was disconnected meanwhile (drop_pool): then the pool may point at a closed tunnel's
/// port, which any program on this machine could take. `port` = the tunnel port it connected through: that tunnel must still be
/// open, since drop_pool counts the generation up before it closes the tunnel and a connect may have read both in between
async fn keep_pool(state: &AppState, c: &Connecting, generation: u64, id: &str, port: Option<u16>, pool: Pool) -> AppResult<Pool> {
    {
        let mut pools = state.pools.write().await;
        let tunnel_open = match port {
            Some(p) => state.tunnels.read().await.get(split_target(id).0).is_some_and(|t| t.local_port == p),
            None => true,
        };
        if c.generation.load(SeqCst) == generation && tunnel_open {
            pools.insert(id.to_string(), pool.clone());
            return Ok(pool);
        }
    }
    pool.close().await;
    Err(AppError::Msg(DISCONNECTED_WHILE_CONNECTING.into()))
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
    // Another caller for the tab (a run, a load-more reopening its result) may have opened one meanwhile: both use the first one
    let mut all = state.sessions.write().await;
    if let Some(e) = all.get(tab_id).filter(|e| e.conn_id == id) {
        let first = e.session.clone();
        drop(all);
        s.discard().await;
        return Ok(first);
    }
    let entry = SessionEntry { conn_id: id.to_string(), pid: s.backend_id(), session: std::sync::Arc::new(tokio::sync::Mutex::new(s)) };
    let arc = entry.session.clone();
    let replaced = all.insert(tab_id.to_string(), entry);
    drop(all);
    if let Some(e) = replaced {
        e.session.lock().await.discard().await;
    }
    Ok(arc)
}

pub async fn redis_pool(state: &AppState, id: &str) -> AppResult<crate::redisdb::RedisConn> {
    match ensure_pool(state, id).await? {
        Pool::Redis(m, _) => Ok(m),
        _ => Err(AppError::Msg("Not a Redis data source".into())),
    }
}

const SERVER_CHANGED: &str = "The server, port, user, type, SSH / AWS SSM tunnel or Redis nodes changed, or SSL was turned off, its verification lowered or its CA file changed — enter the password again. The saved one is only sent to the server it was saved for.";
const SSH_CHANGED: &str = "The SSH server changed — enter the SSH password / passphrase again. The saved one is only sent to the server it was saved for.";
/// Max size of an MCP CSV export
const EXPORT_MAX_BYTES: u64 = 512 << 20;

#[derive(Serialize)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
}

/// Statements run in one transaction (execute_batch): rows changed, and a warning when they committed but their result couldn't be logged
#[derive(Debug, Serialize)]
pub struct BatchResult {
    pub rows: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl AppState {
    /// Run read-only (the MCP default). guard checks it (one statement, read commands, no side-effect functions); it runs inside a READ ONLY transaction
    /// with a 60-second limit, rolled back at the end. MySQL uses the prepared protocol (the server accepts only one statement). Redis: read commands only (no CONFIG, KEYS)
    pub async fn run_read_only(&self, id: String, sql: String, limit: usize) -> AppResult<QueryResult> {
        let kind = self.kind_of(&id)?;
        if let Some(why) = crate::guard::read_only_violation(&sql, kind, true) {
            return Err(AppError::Msg(format!("{why} (start the MCP server with --allow-write to change data)")));
        }
        // Redis has no SQL guard: refuse a write here too, before anything connects
        if kind == store::DbKind::Redis {
            let args = crate::redisdb::parse_command(&sql)?;
            let cmd = String::from_utf8_lossy(&args[0]).to_uppercase();
            let sub = args.get(1).map(|a| String::from_utf8_lossy(a).to_uppercase()).unwrap_or_default();
            if !crate::redisdb::is_read_command(&cmd, &sub) {
                return Err(AppError::Msg(format!("{cmd} is not a read-only command")));
            }
        }
        dispatch!(ensure_pool(self, &id).await?, b => b.run_read_only(&sql, limit).await)
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
        let r = match dispatch!(pool, b => b.export_read_only(&sql, &mut out).await) {
            Ok(()) => out.finish().await.map(|e| e.rows),
            Err(e) => Err(e),
        };
        if r.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        r
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
                    has_password: c.password_saved || self.secret_file.contains(&c.password_key()).unwrap_or(true),
                    connected: pools.keys().any(|k| belongs(k, &c.id)),
                    temporary: self.store.is_temporary(&c.id),
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
        // Read the key now: in keychain mode that may show a prompt, and other apps would wait on the store lock meanwhile
        if [&password, &ssh_secret].iter().any(|s| s.as_deref().is_some_and(|s| !s.is_empty())) {
            self.secret_file.ensure_key()?;
        }
        // Check against the saved source, set the secrets and write under the store lock: another app's save in between
        // (another server, a password saved or removed) can't be decided on stale settings
        let config = self.store.save_with(config, |prev, config| {
            // If the server changed, do not use the saved password for the new server: ask for it again
            // "Saved" means the password actually exists, not the flag (old imports left entries with only the flag turned off).
            // A damaged secrets.json that may hold it counts as saved
            if let Some(p) = prev {
                if password.is_none() && self.secret_file.contains(&p.password_key()).unwrap_or(true) && !p.same_server(config) {
                    return Err(AppError::Msg(SERVER_CHANGED.into()));
                }
                if ssh_secret.is_none() && self.secret_file.contains(&store::ssh_secret_key(&p.id)).unwrap_or(true) && config.active_ssh().is_some() && !p.same_ssh(config) {
                    return Err(AppError::Msg(SSH_CHANGED.into()));
                }
            }
            // The saved-password flag is managed on the backend (None = keep the existing value)
            config.password_saved = prev.is_some_and(|p| p.password_saved);
            config.ssh_secret_saved = prev.is_some_and(|p| p.ssh_secret_saved);
            match password.as_deref() {
                Some("") => {
                    for k in store::password_keys(&config.id) {
                        self.set_secret(&k, None)?;
                    }
                    config.password_saved = false;
                }
                Some(p) => {
                    // Only under the key for this setup (switching to or from AWS SSM moves it)
                    let key = config.password_key();
                    for k in store::password_keys(&config.id).iter().filter(|k| **k != key) {
                        self.set_secret(k, None)?;
                    }
                    self.set_secret(&key, Some(p))?;
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
            Ok(())
        })?;
        drop_pool_logged(self, &config.id, crate::audit::DISCONNECTED).await; // The settings may have changed, so force a reconnect
        Ok(config)
    }

    /// Explorer menu: change only the color (another app's change to the source since the list was read stays; no reconnect)
    pub async fn set_connection_color(&self, id: String, color: Option<String>) -> AppResult<ConnectionConfig> {
        self.modify_connection(&id, |c| {
            c.color = color;
            Ok(())
        })
    }

    /// Explorer menu: change only the shown databases and schemas
    pub async fn set_connection_scope(&self, id: String, scope: store::Scope) -> AppResult<ConnectionConfig> {
        self.modify_connection(&id, |c| {
            c.scope = scope;
            Ok(())
        })
    }

    /// The answer to a NEEDS_TLS / DROPPED_TLS connect error: turn on "Allow unencrypted connection" for this source and reconnect.
    /// The saved password stays (same server, and the user chose this right at the error). Decided on the source as saved now, under the store lock
    pub async fn allow_plaintext(&self, id: String) -> AppResult<ConnectionConfig> {
        let c = self.modify_connection(&id, |c| {
            if !matches!(c.kind, store::DbKind::Postgres | store::DbKind::Mysql) {
                return Err(AppError::Msg("\"Allow unencrypted connection\" is only for PostgreSQL and MySQL data sources.".into()));
            }
            if c.options.confirm_changes {
                return Err(AppError::Msg(format!("\"{}\" is a Production data source, so Qurry won't turn off encryption for it from here. If the server really has no TLS and the network is trusted, turn on \"Allow unencrypted connection\" yourself (GUI: data source Options tab · TUI: data source form, \"Allow unencrypted\").", c.name)));
            }
            if c.active_ssl().is_some() {
                return Err(AppError::Msg(format!("\"{}\" has SSL turned on, so it always requires TLS. Check its SSL settings (GUI: SSH/SSL tab · TUI: data source form), or turn SSL off there first if the server has no TLS.", c.name)));
            }
            c.options.allow_plaintext = true;
            Ok(())
        })?;
        drop_pool(self, &id).await; // The next connect uses the new setting
        Ok(c)
    }

    fn modify_connection(&self, id: &str, f: impl FnOnce(&mut ConnectionConfig) -> AppResult<()>) -> AppResult<ConnectionConfig> {
        self.store
            .modify(id, |c| {
                f(c)?;
                Ok(c.clone())
            })?
            .ok_or_else(|| AppError::Msg("Data source not found".into()))
    }

    pub async fn delete_connection(&self, id: String) -> AppResult<()> {
        drop_pool_logged(self, &id, crate::audit::DISCONNECTED).await;
        // A damaged secrets.json (left untouched) doesn't keep the data source: it is removed, and the error says the password stayed
        let forgot = store::password_keys(&id).iter().chain([&store::ssh_secret_key(&id)]).try_for_each(|k| self.set_secret(k, None));
        crate::search_cache::remove_conn(&self.store.dir(), &id);
        self.store.delete(&id)?;
        self.connecting.lock().unwrap().remove(&id);
        forgot.map_err(|e| AppError::Msg(format!("The data source was removed, but its saved password couldn't be: {e}")))
    }

    /// Whether a password or SSH secret may be stored for this key (a temporary source's lives in memory, never in the secrets file)
    fn holds_secret(&self, id: &str, key: &str) -> bool {
        if self.store.is_temporary(id) {
            return self.temporary.lock().unwrap().contains_key(key);
        }
        self.secret_file.contains(key).unwrap_or(true)
    }

    /// The saved password, if it may be sent to config (only when it is the same server as the saved settings)
    fn saved_password_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if self.holds_secret(&p.id, &p.password_key()) && !p.same_server(config) => Err(AppError::Msg(SERVER_CHANGED.into())),
            Some(p) => self.saved_secret(&p.password_key(), p.password_saved),
            None => Ok(None),
        }
    }

    fn saved_ssh_secret_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if self.holds_secret(&p.id, &store::ssh_secret_key(&p.id)) && !p.same_ssh(config) => Err(AppError::Msg(SSH_CHANGED.into())),
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
        let plain = dispatch!(pool, b => b.plaintext_note(&config).await);
        pool.close().await;
        drop(tunnel);
        let via = match (config.active_ssh(), config.active_ssm()) {
            (Some(s), _) => format!("  (via SSH {}@{})", s.user, s.host),
            (_, Some(s)) => format!("  (via AWS SSM {})", s.target.trim()),
            _ => String::new(),
        };
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
        let note = dispatch!(pool, b => b.plaintext_note(&cfg).await);
        Ok(match note {
            Some(n) => format!("{v} — {n}"),
            None => v,
        })
    }

    pub async fn disconnect(&self, id: String) -> AppResult<()> {
        drop_pool_logged(self, &id, crate::audit::DISCONNECTED).await;
        Ok(())
    }

    /// If `tab_id` is set, run in that console's session (cursor and cancel support).
    /// For Production Data Sources, log modifying statements to the audit log before they run (not run if that fails),
    /// with undo SQL for simple UPDATE / DELETE (crate::audit)
    pub async fn run_query(&self, id: String, sql: String, limit: Option<usize>, tab_id: Option<String>) -> AppResult<QueryResult> {
        let limit = limit.unwrap_or(500);
        self.check_read_only(&id, &sql)?;
        let cfg = self.source(&id)?;
        let watch = cfg.options.confirm_changes;
        let tab = tab_id.as_deref();
        let started = match watch {
            true => self.audit.before(&id, &cfg.name, cfg.kind, tab, &sql)?,
            false => None,
        };
        let undo = watch && crate::audit::is_change(&sql, cfg.kind);
        let (r, undo, tx) = self.run_with_undo(&id, &cfg, &sql, limit, tab, undo).await;
        let logged = match watch {
            true => {
                let a = self.audit.after(started, &id, &cfg.name, cfg.kind, tab, &sql, &r, undo);
                a.and(self.audit_lost_tx(&id, &cfg.name, tab, &sql, &r, tx.ended).await)
            }
            false => Ok(()),
        };
        // The statement ended the open MySQL transaction by itself, whether it ran or failed: say what became of the changes in it
        let ended = match tx.ended {
            Some(w) if tx.changed => Some(format!(
                "{}, and Roll back can't undo them{}",
                match w {
                    crate::audit::IMPLICIT_COMMIT => "MySQL committed the open transaction before running it: the changes made in it before are saved",
                    _ => "the open transaction ended while it ran, and MySQL doesn't say whether it committed or rolled back the changes made in it before: check the data before you run them again",
                },
                if watch { " (the Production log has their undo SQL where it could keep it)" } else { "" }
            )),
            _ => None,
        };
        match r {
            Err(e) => match ended {
                Some(n) => Err(AppError::Msg(format!("{e} — {n}."))),
                None => {
                    let lost = if tx.open { crate::audit::ended_by(&sql, &e, true) } else { None };
                    Err(recover(self, &id, tab, lost, e).await)
                }
            },
            // It ran (and may be committed): a log failure now is a warning, so nobody runs it again
            Ok(mut q) => {
                let warnings: Vec<String> = ended.into_iter().chain(logged.err().map(|e| e.to_string())).collect();
                q.warning = (!warnings.is_empty()).then(|| warnings.join(". "));
                Ok(q)
            }
        }
    }

    /// Run in the console's session (or on the pool). undo = also keep undo SQL: the snapshot runs in the change's transaction (Session::run_with_snapshot).
    /// → (result, undo SQL or why there is none, what happened to the console's transaction)
    async fn run_with_undo(&self, id: &str, cfg: &ConnectionConfig, sql: &str, limit: usize, tab: Option<&str>, undo: bool) -> (AppResult<QueryResult>, Option<Result<String, String>>, TxSeen) {
        let pool = match ensure_pool(self, id).await {
            Ok(p) => p,
            Err(e) => return (Err(e), None, TxSeen::default()),
        };
        let plan = match undo {
            true => Some(crate::audit::plan(&pool, sql, cfg.kind).await),
            false => None,
        };
        let sess = match tab {
            Some(t) => match ensure_session(self, id, t).await {
                Ok(s) => Some(s),
                Err(e) => return (Err(e), None, TxSeen::default()),
            },
            None => None,
        };
        let (mut guard, mut temp);
        let s: &mut Session = match &sess {
            Some(s) => {
                guard = s.lock().await;
                &mut guard
            }
            // On the pool: a connection of its own for the snapshot's transaction
            None if matches!(plan, Some(Ok(_))) => match Session::open(&pool).await {
                Ok(s) => {
                    temp = s;
                    &mut temp
                }
                Err(e) => return (Err(e), None, TxSeen::default()),
            },
            None => return (pool.run_query(sql, limit).await, plan.and_then(|p| p.err()).map(Err), TxSeen::default()),
        };
        let (open, changed) = (s.in_tx(), s.tx_changed());
        let (mut r, undo) = match plan {
            Some(Ok(p)) => {
                let (snap, r) = s.run_with_snapshot(&p.snapshot, sql, limit).await;
                let changed = r.as_ref().map_or(0, |q| q.rows_affected);
                (r, Some(p.undo(snap, changed)))
            }
            other => (s.run(sql, limit).await, other.and_then(|p| p.err()).map(Err)),
        };
        let ended = s.tx_ended();
        match tab {
            // Under the session's lock, like fetch_more's update: the two can't overwrite each other's result
            Some(t) if sess.is_some() => {
                // A console's result: can its rows be edited, and where (the GUI and TUI edit it like a table tab)
                if let Ok(q) = &mut r
                    && q.has_result_set
                {
                    q.editable = crate::editable::check(&pool, s, sql, &q.columns).await;
                }
                let more = match r.is_ok() {
                    true => s.more(true).await,
                    false => None,
                };
                let run = self.runs.fetch_add(1, SeqCst);
                let mut all = self.more.lock().unwrap();
                match more {
                    Some(more) => all.insert(t.to_string(), Kept { conn_id: id.to_string(), cfg: cfg.clone(), run, more }),
                    None => all.remove(t),
                };
            }
            _ => s.discard().await,
        }
        (r, undo, TxSeen { open, changed, ended })
    }

    /// Production audit log (newest first)
    pub fn audit_log(&self, conn: Option<String>, filter: String, limit: usize) -> Vec<crate::audit::AuditEntry> {
        self.audit.list(conn.as_deref(), &filter, limit)
    }

    /// Log the console's Commit / Roll back to the audit log (only if the logged changes were uncommitted)
    async fn audit_tx_end(&self, tab_id: &str, word: &str) -> AppResult<()> {
        let conn = self.sessions.read().await.get(tab_id).map(|e| e.conn_id.clone());
        match conn.map(|c| (self.source(&c), c)) {
            Some((Ok(cfg), c)) => self.audit.tx_closed(&c, &cfg.name, tab_id, word),
            _ => Ok(()),
        }
    }

    /// The console's transaction with logged changes ended without a COMMIT after `sql` ran: as the MySQL session says the statement
    /// ended it (`ended`: DDL, SET PASSWORD, BEGIN … committed it; a CALL, deadlock or lock wait timeout either way), else rolled back
    /// by the server (deadlock, failed COMMIT, lost connection), or a COMMIT cut off by a lost connection has an unknown outcome (audit::ended_by)
    async fn audit_lost_tx(&self, id: &str, name: &str, tab: Option<&str>, sql: &str, r: &AppResult<QueryResult>, ended: Option<&'static str>) -> AppResult<()> {
        let Some(t) = tab else { return Ok(()) };
        let open = self.tx_state(t.into()).await;
        let word = ended.or(match r {
            Ok(_) => (!open).then_some(crate::audit::BY_SERVER),
            Err(e) => crate::audit::ended_by(sql, e, open),
        });
        match word {
            Some(w) => self.audit.tx_closed(id, name, t, w),
            None => Ok(()),
        }
    }

    /// Load the next rows of the tab's result. When its session was closed (drop_pool: settings saved, Disconnect, the idle drop),
    /// reopen the statement on a new session after the rows already read, unless that could give rows that don't line up or
    /// run something again (reopen_refused)
    pub async fn fetch_more(&self, tab_id: String, limit: Option<usize>) -> AppResult<QueryResult> {
        let limit = limit.unwrap_or(500);
        let closed = || AppError::Msg("Session closed".into());
        // The result this load-more is for, read before it waits for anything: a run queued ahead of it has a result of its own
        let issued = self.more.lock().unwrap().get(&tab_id).cloned();
        let entry = self.sessions.read().await.get(&tab_id).map(|e| (e.session.clone(), e.conn_id.clone()));
        let (sess, conn_id, old) = match entry {
            Some((s, c)) => (s, c, None),
            None => {
                let k = issued.clone().ok_or_else(closed)?;
                // The guard first: nothing reaches the server for a statement it refuses
                self.check_read_only(&k.conn_id, &k.more.sql)?;
                let manual = self.tab_manual.lock().unwrap().get(&tab_id).copied().unwrap_or(false);
                if let Some(why) = reopen_refused(&k, &self.source(&k.conn_id)?, manual) {
                    return Err(AppError::Msg(why));
                }
                (ensure_session(self, &k.conn_id, &tab_id).await?, k.conn_id.clone(), Some(k))
            }
        };
        touch(self, &conn_id);
        let (r, in_tx) = {
            let mut s = sess.lock().await;
            // A run that took the session first replaced the result: never move the new result's cursor for the old one
            let now = self.more.lock().unwrap().get(&tab_id).map(|k| k.run);
            if now != issued.as_ref().map(|k| k.run) {
                return Err(closed());
            }
            let reopen = match &old {
                // A disconnect may have closed the session meanwhile; another load-more may have reopened the result already (go on from its cursor)
                Some(k) => {
                    if !self.sessions.read().await.get(&tab_id).is_some_and(|e| std::sync::Arc::ptr_eq(&e.session, &sess)) {
                        return Err(closed());
                    }
                    s.more(false).await.is_none().then_some(k)
                }
                None => None,
            };
            let in_tx = s.in_tx();
            let r = match reopen {
                Some(k) => s.reopen(&k.more, limit).await,
                None => s.fetch(limit).await,
            };
            match &r {
                // Same statement, schema and run: only the rows read move on (under the session's lock, like run_with_undo's)
                Ok(_) => {
                    let more = s.more(false).await;
                    let mut all = self.more.lock().unwrap();
                    match (more, all.get_mut(&tab_id)) {
                        (Some(m), Some(k)) => k.more.fetched = m.fetched,
                        _ => drop(all.remove(&tab_id)),
                    }
                }
                // The new session has nothing to load from: drop it, so trying again reopens it again (and says why if it fails)
                Err(_) if reopen.is_some() => {
                    s.discard().await;
                    let mut all = self.sessions.write().await;
                    if all.get(&tab_id).is_some_and(|e| std::sync::Arc::ptr_eq(&e.session, &sess)) {
                        all.remove(&tab_id);
                    }
                }
                Err(_) => {}
            }
            (r, in_tx)
        };
        match r {
            Err(e) => {
                let tx = (in_tx && is_conn_lost(&e)).then_some(crate::audit::BY_SERVER);
                if let Some(w) = tx
                    && let Err(le) = self.audit_tx_end(&tab_id, w).await
                {
                    crate::log!("[qurry] {le}");
                }
                Err(recover(self, &conn_id, Some(&tab_id), tx, e).await)
            }
            ok => ok,
        }
    }

    pub async fn cancel_query(&self, tab_id: String) -> AppResult<bool> {
        let (conn_id, pid) = match self.sessions.read().await.get(&tab_id) {
            Some(e) => (e.conn_id.clone(), e.pid),
            None => return Ok(false),
        };
        dispatch!(ensure_pool(self, &conn_id).await?, b => b.cancel(pid).await)
    }

    /// Tx: Auto / Manual
    pub async fn set_tx_mode(&self, id: String, tab_id: String, manual: bool) -> AppResult<()> {
        let sess = ensure_session(self, &id, &tab_id).await?;
        let mut s = sess.lock().await;
        s.set_manual(manual).await?;
        self.tab_manual.lock().unwrap().insert(tab_id, manual);
        Ok(())
    }

    /// Explain Analyse (PostgreSQL · MySQL): the statements to run before and after it in the console's session, so it is rolled back
    /// without committing what is pending (Session::explain_wrap)
    pub async fn explain_wrap(&self, id: String, tab_id: String) -> AppResult<[&'static str; 2]> {
        let sess = ensure_session(self, &id, &tab_id).await?;
        let mut s = sess.lock().await;
        s.explain_wrap().await
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

    /// Production: the COMMIT of logged changes goes into the log first; if it can't be written, the transaction stays open.
    /// → a warning when it committed but its result couldn't be logged
    pub async fn commit_tx(&self, tab_id: String) -> AppResult<Option<String>> {
        let (sess, conn) = self.sessions.read().await.get(&tab_id).map(|e| (e.session.clone(), e.conn_id.clone())).ok_or_else(|| AppError::Msg("Session closed".into()))?;
        let cfg = self.source(&conn).ok();
        let started = match &cfg {
            Some(c) => self.audit.before(&conn, &c.name, c.kind, Some(&tab_id), "COMMIT")?,
            None => None,
        };
        let r = sess.lock().await.commit().await.map(|_| db::query_result(vec![], vec![], 0, false, std::time::Instant::now(), false));
        let logged = match &cfg {
            Some(c) => {
                let a = self.audit.after(started, &conn, &c.name, c.kind, Some(&tab_id), "COMMIT", &r, None);
                a.and(self.audit_lost_tx(&conn, &c.name, Some(&tab_id), "COMMIT", &r, None).await)
            }
            None => Ok(()),
        };
        match r {
            Err(e) => Err(recover(self, &conn, Some(&tab_id), Some(crate::audit::COMMIT_UNKNOWN), e).await),
            Ok(_) => Ok(logged.err().map(|e| e.to_string())),
        }
    }

    /// → a warning when it rolled back but that couldn't be logged
    pub async fn rollback_tx(&self, tab_id: String) -> AppResult<Option<String>> {
        let sess = self.sessions.read().await.get(&tab_id).map(|e| e.session.clone()).ok_or_else(|| AppError::Msg("Session closed".into()))?;
        sess.lock().await.rollback_tx().await?;
        Ok(self.audit_tx_end(&tab_id, "ROLLBACK").await.err().map(|e| e.to_string()))
    }

    pub async fn close_session(&self, tab_id: String) -> AppResult<()> {
        self.tab_manual.lock().unwrap().remove(&tab_id);
        self.more.lock().unwrap().remove(&tab_id);
        // Cancel a running query first (otherwise the connection and cleanup stay tied up until it ends)
        let busy = self.sessions.read().await.get(&tab_id).is_some_and(|e| e.session.try_lock().is_err());
        if busy {
            let _ = self.cancel_query(tab_id.clone()).await;
        }
        if let Err(e) = self.audit_tx_end(&tab_id, "ROLLBACK (console closed)").await {
            crate::log!("[qurry] {e}");
        }
        let removed = self.sessions.write().await.remove(&tab_id);
        if let Some(e) = removed {
            e.session.lock().await.discard().await;
        }
        Ok(())
    }

    /// Statements in one transaction. exact_one = saving table edits (roll back everything unless each UPDATE/DELETE hits exactly 1 row)
    pub async fn execute_batch(&self, id: String, statements: Vec<String>, exact_one: bool) -> AppResult<BatchResult> {
        self.execute_batch_with(id, statements, exact_one, false).await
    }

    /// MCP execute: like execute_batch, but PostgreSQL / MySQL run each statement as a prepared statement (one statement per item, enforced by the server)
    /// Redis reads among the items get the same size checks as run_read_only
    pub async fn execute_batch_prepared(&self, id: String, statements: Vec<String>) -> AppResult<BatchResult> {
        if self.kind_of(&id)? == store::DbKind::Redis {
            // A read-only source refuses a write before the size checks connect
            for st in &statements {
                self.check_read_only(&id, st)?;
            }
            let m = redis_pool(self, &id).await?;
            for st in &statements {
                crate::redisdb::check_read(&m, st).await?;
            }
        }
        self.execute_batch_with(id, statements, false, true).await
    }

    async fn execute_batch_with(&self, id: String, statements: Vec<String>, exact_one: bool, prepared: bool) -> AppResult<BatchResult> {
        for st in &statements {
            self.check_read_only(&id, st)?;
        }
        let cfg = self.source(&id)?;
        let watch = cfg.options.confirm_changes;
        // Production: the changes go into the log before they run (not run if that fails)
        let started = match watch {
            true => self.audit.before_batch(&id, &cfg.name, cfg.kind, &statements)?,
            false => vec![],
        };
        let mut undos = Vec::new();
        let r = self.run_batch(&id, cfg.kind, &statements, exact_one, prepared, watch, &mut undos).await;
        let logged = match watch {
            true => self.audit.after_batch(started, r.as_ref().err().map(|e| e.to_string()), exact_one, undos),
            false => Ok(()),
        };
        match r {
            Err(e) => Err(recover(self, &id, None, None, e).await),
            // Committed: a log failure now is a warning, so nobody saves it again
            Ok(rows) => Ok(BatchResult { rows, warning: logged.err().map(|e| e.to_string()) }),
        }
    }

    /// undo = also keep undo SQL for each statement (into `undos`): the snapshots run in the batch's transaction, right before their statements
    #[allow(clippy::too_many_arguments)]
    async fn run_batch(&self, id: &str, kind: store::DbKind, statements: &[String], exact_one: bool, prepared: bool, undo: bool, undos: &mut Vec<Option<Result<String, String>>>) -> AppResult<u64> {
        let pool = ensure_pool(self, id).await?;
        let mut plans = Vec::new();
        for st in statements {
            plans.push(match undo && crate::audit::is_change(st, kind) {
                true => Some(crate::audit::plan(&pool, st, kind).await),
                false => None,
            });
        }
        let snaps: Vec<Option<&str>> = plans.iter().map(|p| p.as_ref().and_then(|p| p.as_ref().ok()).map(|p| p.snapshot.as_str())).collect();
        let (n, mut shots) = dispatch!(pool, b => b.execute_batch(statements, exact_one, prepared, &snaps).await)?;
        shots.resize_with(plans.len(), || None);
        *undos = plans
            .into_iter()
            .zip(shots)
            .map(|(p, shot)| match (p, shot) {
                (Some(Ok(p)), Some((shot, changed))) => Some(p.undo(shot, changed)),
                (p, _) => p.and_then(|p| p.err()).map(Err),
            })
            .collect();
        Ok(n)
    }

    /// Whether a source's object lists go into the search cache on disk: not Redis keys, nothing of a temporary source
    fn cached(&self, pool: &Pool, id: &str) -> bool {
        !matches!(pool, Pool::Redis(..)) && !self.store.is_temporary(split_target(id).0)
    }

    /// Save to the search cache on every read (Redis keys are not saved)
    pub async fn object_index(&self, id: String) -> AppResult<Vec<crate::meta::IndexEntry>> {
        let pool = ensure_pool(self, &id).await?;
        let r = crate::meta::object_index(&pool).await?;
        if self.cached(&pool, &id) {
            crate::search_cache::save_objects(&self.store.dir(), &id, &r);
        }
        Ok(r)
    }

    /// On connect, immediately read (in the background) the objects and columns of the visible DBs to fill the search cache.
    /// That way Search Everywhere finds them next time without connecting. Skip DBs filled within the last 10 minutes
    pub async fn warm_search_cache(&self, conn: String) -> AppResult<()> {
        let cfg = self.store.get(&conn)?.ok_or_else(|| AppError::Msg("Data source not found".into()))?;
        if cfg.kind == store::DbKind::Redis || self.store.is_temporary(&conn) {
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
        // Importing a different server under the same id deletes the old password (so it is not sent to the new server) —
        // before the store is written, so a failure (damaged secrets.json) imports nothing
        let (added, updated, forget) = self.store.import_from(std::path::Path::new(&path), |forget| {
            for (id, password, ssh) in forget {
                if *password {
                    for k in store::password_keys(id) {
                        self.set_secret(&k, None)?;
                    }
                }
                if *ssh {
                    self.set_secret(&store::ssh_secret_key(id), None)?;
                }
            }
            Ok(())
        })?;
        for (id, _, _) in forget {
            drop_pool_logged(self, &id, crate::audit::DISCONNECTED).await;
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

    pub async fn table_structure(&self, id: String, schema: String, table: String) -> AppResult<crate::meta::TableStructure> {
        crate::meta::table_structure(&ensure_pool(self, &id).await?, &schema, &table).await
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
        if self.cached(&pool, &id) {
            crate::search_cache::save_columns(&self.store.dir(), &id, &r);
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary source (qurry-tui URL) is listed and connectable in this process only: never in connections.json or the secrets
    /// file, so another app on the same folder (GUI, MCP) never sees it. Its form can't save it; read-only is enforced as for saved ones
    #[tokio::test]
    async fn temporary_sources_stay_in_this_process() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let saved = st.save_connection(silent_source("", 1, None), Some("savedpw".into()), None).await.unwrap();
        let var = |k: &str| (k == "USER").then(|| "me".to_string());
        let t = crate::dsn::from_url("postgres://u:s3cret-pw@db.example/app", true, &crate::dsn::Env { var: &var, home: None }).unwrap();
        let c = st.open_temporary(t);
        assert!(c.id.starts_with("tmp-") && c.password_saved);
        let views = st.list_connections().await.unwrap();
        assert!(views.iter().any(|v| v.config.id == c.id && v.temporary && v.has_password));
        assert!(views.iter().any(|v| v.config.id == saved.id && !v.temporary));
        assert_eq!(st.secret(&c.id).unwrap().as_deref(), Some("s3cret-pw"));
        // Changes to it stay in memory; saving its form is refused
        st.set_connection_color(c.id.clone(), Some("#5c2b2b".into())).await.unwrap();
        assert_eq!(st.store.get(&c.id).unwrap().unwrap().color.as_deref(), Some("#5c2b2b"));
        let e = st.save_connection(c.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("temporary"), "{e}");
        // Its form tested with another host never sends its password there
        let mut moved = c.clone();
        moved.host = "evil.example".into();
        assert_eq!(st.saved_password_for(&moved).unwrap_err().to_string(), SERVER_CHANGED);
        assert_eq!(st.saved_password_for(&c).unwrap().as_deref(), Some("s3cret-pw"));
        // Saving another source writes connections.json without it
        st.save_connection(silent_source("", 2, None), None, None).await.unwrap();
        let file = std::fs::read_to_string(dir.path().join("connections.json")).unwrap();
        assert!(!file.contains(&c.id) && !file.contains("db.example"), "{file}");
        assert!(!st.secret_file.names().unwrap().iter().any(|k| k.contains("tmp-")));
        let other = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        assert!(other.store.get(&c.id).unwrap().is_none());
        assert_eq!(other.list_connections().await.unwrap().len(), 2);
        // Read-only
        let e = st.check_read_only(&c.id, "delete from t").unwrap_err().to_string();
        assert!(e.contains("read-only"), "{e}");
        assert!(st.check_read_only(&c.id, "select 1").is_ok());
        // Deleting it forgets it and its password, and leaves the saved ones alone
        st.delete_connection(c.id.clone()).await.unwrap();
        assert!(st.store.get(&c.id).unwrap().is_none());
        assert_eq!(st.secret(&c.id).unwrap(), None);
        assert_eq!(st.secret(&saved.id).unwrap().as_deref(), Some("savedpw"));
    }

    /// A server that accepts and never answers (a DB or SSH host behind a firewall that drops packets)
    async fn silent_server() -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((s, _)) = l.accept().await {
                held.push(s);
            }
        });
        port
    }

    fn silent_source(id: &str, port: u16, ssh: Option<serde_json::Value>) -> ConnectionConfig {
        let mut c = serde_json::json!({ "id": id, "name": id, "kind": "postgres", "host": "127.0.0.1", "port": port, "user": "u", "database": "d" });
        if let Some(ssh) = ssh {
            c["ssh"] = ssh;
        }
        serde_json::from_value(c).unwrap()
    }

    /// The tree, autocomplete and restored tabs open a source and its databases together. When the server doesn't answer they
    /// all fail after one timeout, not one after another (4 callers used to take 4 × 10s)
    #[tokio::test]
    async fn callers_of_an_unreachable_source_wait_one_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        st.store.save(&silent_source("x", silent_server().await, None)).unwrap();
        let start = std::time::Instant::now();
        let r = futures::future::join_all(["x", "x::db1", "x", "x::db3"].map(|id| ensure_pool(&st, id))).await;
        assert!(r.iter().all(|r| r.as_ref().err().is_some_and(|e| e.to_string().contains("didn't answer within 10s"))));
        assert!(start.elapsed() < std::time::Duration::from_secs(15), "{:?}", start.elapsed());
    }

    /// Callers that waited for a tunnel that couldn't be opened get its error instead of opening another one in turn
    #[tokio::test]
    async fn callers_waiting_for_a_failed_tunnel_get_its_error() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let ssh = serde_json::json!({ "enabled": true, "host": "127.0.0.1", "port": silent_server().await, "user": "me" });
        st.store.save(&silent_source("t", 5432, Some(ssh))).unwrap();
        let start = std::time::Instant::now();
        let r = futures::future::join_all(["t", "t::db1", "t", "t::db3"].map(|id| ensure_pool(&st, id))).await;
        assert!(r.iter().all(|r| r.as_ref().err().is_some_and(|e| e.to_string().contains("SSH connect timeout"))), "{:?}", r.iter().map(|r| r.as_ref().err().map(|e| e.to_string())).collect::<Vec<_>>());
        assert!(start.elapsed() < std::time::Duration::from_secs(20), "{:?}", start.elapsed());
        // A later call tries again
        let start = std::time::Instant::now();
        assert!(ensure_pool(&st, "t").await.is_err());
        assert!(start.elapsed() >= std::time::Duration::from_secs(10));
    }

    /// Disconnecting (or saving new settings) while a pool is still connecting: that pool isn't kept, since it may go through a
    /// tunnel that was just closed. Run: QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-core disconnect_while_connecting -- --ignored
    #[tokio::test]
    #[ignore]
    async fn disconnect_while_connecting_keeps_no_pool() {
        let pg = crate::testenv::pg().expect("QURRY_TEST_PG").cfg;
        let upstream = format!("{}:{}", pg.host, pg.port);
        // A slow link: each connection starts forwarding after 1s
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut a, _)) = l.accept().await {
                let upstream = upstream.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let mut b = tokio::net::TcpStream::connect(upstream).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut a, &mut b).await;
                });
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut c = silent_source("slow", port, None);
        (c.user, c.database) = (pg.user.clone(), pg.database.clone());
        st.store.save(&c).unwrap();
        let (r, _) = tokio::join!(ensure_pool(&st, "slow"), async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            drop_pool(&st, "slow").await;
        });
        let e = r.err().expect("the pool from before the disconnect is not used").to_string();
        assert!(e.contains("disconnected while connecting"), "{e}");
        assert!(st.pools.read().await.is_empty());
        assert!(ensure_pool(&st, "slow").await.is_ok(), "the next call connects");
    }

    /// drop_pool racing a connect through the shared AWS SSM tunnel (fake aws): no pool may stay on the closed tunnel's port, which
    /// the fast path would keep returning (tunnel_alive is None once the tunnel is gone)
    #[cfg(unix)]
    #[tokio::test]
    async fn a_connect_across_a_disconnect_keeps_no_pool_on_the_closed_tunnel() {
        let lazy = |port| Pool::Pg(sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new().host("127.0.0.1").port(port)));
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let c = st.connecting("src");

        // The connect read the tunnel's port, then drop_pool counted the generation up, and the connect read that generation
        // before drop_pool closed the tunnel (shared_tunnel reads both in its own statements)
        let (fake, t) = crate::ssm::tests::fake_tunnel().await;
        let port = t.local_port;
        st.tunnels.write().await.insert("src".into(), t);
        drop_pool(&st, "src").await;
        crate::ssm::tests::gone(fake.path()).await;
        let g = c.generation.load(SeqCst);
        let e = keep_pool(&st, &c, g, "src::db_b", Some(port), lazy(port)).await.err().expect("not kept: its tunnel is closed").to_string();
        assert!(e.contains("disconnected while connecting") && e.contains("Run it again"), "{e}");
        assert!(st.pools.read().await.is_empty());

        // A pool kept while the tunnel was still open is closed with it: drop_pool closes the tunnel before it takes the pools
        let (fake, t) = crate::ssm::tests::fake_tunnel().await;
        st.tunnels.write().await.insert("src".into(), t);
        let pools = st.pools.write().await;
        let ((), tunnel_first) = tokio::join!(async { drop_pool(&st, "src").await; }, async {
            let mut closed = false;
            for _ in 0..50 {
                if st.tunnels.read().await.is_empty() {
                    closed = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            drop(pools);
            closed
        });
        assert!(tunnel_first, "drop_pool waited for the pools before it closed the tunnel");
        crate::ssm::tests::gone(fake.path()).await;

        // A connect through the open tunnel is kept
        let (_fake, t) = crate::ssm::tests::fake_tunnel().await;
        let port = t.local_port;
        st.tunnels.write().await.insert("src".into(), t);
        assert!(keep_pool(&st, &c, c.generation.load(SeqCst), "src::db_b", Some(port), lazy(port)).await.is_ok());
        drop_pool(&st, "src").await;
    }

    /// Changing only the host and doing Test / Save does not send the saved password to the new server; it asks for it again (before connecting)
    #[tokio::test]
    async fn saved_password_is_not_sent_to_a_changed_server() {
        let dir = std::env::temp_dir().join(format!("qurry-pw-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "a".into(), name: "a".into(), kind: crate::store::DbKind::Postgres, host: "127.0.0.1".into(), port: 1,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
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
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        let saved = st.save_connection(cfg, None, None).await.unwrap();
        assert_eq!(saved.options.mcp.as_deref(), Some("off"));
        st.mcp = true;
        let e = st.connect(saved.id.clone()).await.unwrap_err().to_string();
        assert!(e.contains("not shared with MCP"), "{e}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An SSM source's DB password is saved under `<id>#ssm`, which older apps (that would connect straight to the host) don't read.
    /// Switching the tunnel moves it; deleting removes both; MCP locks it like the others
    #[tokio::test]
    async fn ssm_password_is_kept_from_older_apps() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut cfg: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "prod", "kind": "postgres", "host": "localhost", "port": 5432, "user": "app",
            "ssm": { "enabled": true, "profile": "prod", "region": "ap-northeast-2", "target": "i-0123456789abcdef0" } })).unwrap();
        cfg = st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        let id = cfg.id.clone();
        assert!(!st.secret_file.contains(&id).unwrap(), "an older app finds no password under the id");
        assert_eq!(st.secret(&format!("{id}#ssm")).unwrap().as_deref(), Some("pw"));
        assert!(st.list_connections().await.unwrap()[0].has_password);
        assert_eq!(st.saved_password_for(&cfg).unwrap().as_deref(), Some("pw"));
        // Turning SSM off is another server: the password is asked again, and then saved under the id only
        cfg.ssm.as_mut().unwrap().enabled = false;
        assert!(st.save_connection(cfg.clone(), None, None).await.is_err());
        cfg = st.save_connection(cfg, Some("pw2".into()), None).await.unwrap();
        assert_eq!(st.secret(&id).unwrap().as_deref(), Some("pw2"));
        assert!(!st.secret_file.contains(&format!("{id}#ssm")).unwrap());
        cfg.ssm.as_mut().unwrap().enabled = true;
        cfg = st.save_connection(cfg, Some("pw3".into()), None).await.unwrap();
        assert!(!st.secret_file.contains(&id).unwrap());
        st.mcp = true;
        assert!(st.secret(&format!("{id}#ssm")).unwrap_err().to_string().contains("not shared with MCP"));
        st.mcp = false;
        st.delete_connection(cfg.id).await.unwrap();
        assert!(!st.secret_file.contains(&format!("{id}#ssm")).unwrap());
    }

    /// An older app deleting an SSM source leaves `<id>#ssm` behind (it removes `<id>` and `<id>#ssh` only): the next start forgets it
    #[tokio::test]
    async fn ssm_password_of_a_source_deleted_by_an_older_app_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let src = |name: &str| -> ConnectionConfig {
            serde_json::from_value(serde_json::json!({ "name": name, "kind": "postgres", "host": "localhost", "port": 5432, "user": "app",
                "ssm": { "enabled": true, "target": "i-0123456789abcdef0" } })).unwrap()
        };
        let gone = st.save_connection(src("gone"), Some("pw".into()), None).await.unwrap().id;
        let kept = st.save_connection(src("kept"), Some("pw".into()), None).await.unwrap().id;
        st.set_secret("api-key", Some("x")).unwrap();
        // What the older app does
        st.store.delete(&gone).unwrap();
        st.set_secret(&gone, None).unwrap();
        st.set_secret(&format!("{gone}#ssh"), None).unwrap();
        drop(st);
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        assert!(!st.secret_file.contains(&format!("{gone}#ssm")).unwrap());
        assert_eq!(st.secret(&format!("{kept}#ssm")).unwrap().as_deref(), Some("pw"));
        assert!(st.secret_file.contains("api-key").unwrap());
        // connections.json can't be read: nothing is removed
        std::fs::write(dir.path().join("connections.json"), "{damaged").unwrap();
        drop(st);
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        assert!(st.secret_file.contains(&format!("{kept}#ssm")).unwrap());
    }

    /// Another app starting while this one saves a new SSM source (its `#ssm` password set, connections.json not written yet)
    /// waits for the save instead of taking the new source for a deleted one
    #[test]
    fn starting_during_a_save_keeps_the_new_ssm_password() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let cfg: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "new", "name": "new", "kind": "postgres", "host": "localhost", "port": 5432, "user": "app",
            "ssm": { "enabled": true, "target": "i-0123456789abcdef0" } })).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|s| {
            s.spawn(|| {
                st.store
                    .save_with(cfg, |_, c| {
                        st.set_secret(&c.password_key(), Some("pw"))?;
                        tx.send(()).unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        Ok(())
                    })
                    .unwrap();
            });
            rx.recv().unwrap();
            drop(crate::open_state(Some(dir.path().to_path_buf())).unwrap());
        });
        assert_eq!(st.secret("new#ssm").unwrap().as_deref(), Some("pw"));
    }

    /// The MCP server does not read passwords of Data Sources hidden from MCP. The app is unaffected
    #[test]
    fn mcp_hidden_source_secrets_stay_locked() {
        let dir = std::env::temp_dir().join(format!("qurry-mcp-{}", uuid::Uuid::new_v4()));
        let mut st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mut cfg = ConnectionConfig {
            id: "prod".into(), name: "prod".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: true,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
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
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(mk("pw"), Some("secret".into()), None).await.unwrap();
        st.save_connection(mk("trust"), None, None).await.unwrap();
        // A password in the file whose flag was cleared (old imports did that)
        st.save_connection(mk("flagless"), Some("secret".into()), None).await.unwrap();
        st.store.save(&mk("flagless")).unwrap();
        let damaged = "{ \"flagless\": \"x\", \"pw\": \"tru";
        std::fs::write(dir.join("secrets.json"), damaged).unwrap();
        let e = st.connect("pw".into()).await.unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
        let e = st.connect("flagless".into()).await.unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
        // No saved password or SSH secret: tries the SSH server (nothing listens on port 1), not the secrets file
        let e = st.connect("trust".into()).await.unwrap_err().to_string();
        assert!(!e.contains("damaged"), "{e}");
        let e = st.test_connection(mk("trust"), None, None).await.unwrap_err().to_string();
        assert!(!e.contains("damaged"), "{e}");
        let e = st.delete_connection("pw".into()).await.unwrap_err().to_string();
        assert!(e.contains("was removed") && e.contains("damaged"), "{e}");
        assert!(st.store.get("pw").unwrap().is_none());
        assert_eq!(std::fs::read_to_string(dir.join("secrets.json")).unwrap(), damaged);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The explorer's color and scope menus change only that part: another app's change since the list was read stays
    #[tokio::test]
    async fn color_and_scope_change_only_their_part() {
        let dir = std::env::temp_dir().join(format!("qurry-part-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "p".into(), name: "p".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        // Another app turns on Read-only
        let other = Store::new(dir.clone());
        let mut c = other.get("p").unwrap().unwrap();
        c.read_only = true;
        other.save(&c).unwrap();
        st.set_connection_color("p".into(), Some("#f00".into())).await.unwrap();
        let scope = crate::store::Scope { databases: Some(vec!["d".into()]), schemas: Default::default() };
        st.set_connection_scope("p".into(), scope).await.unwrap();
        let c = st.store.get("p").unwrap().unwrap();
        assert!(c.read_only && c.password_saved, "{c:?}");
        assert_eq!((c.color.as_deref(), c.scope.databases.as_deref()), (Some("#f00"), Some(&["d".to_string()][..])));
        assert!(st.set_connection_color("missing".into(), None).await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// "Allow unencrypted" from the TLS error: keeps the saved password and another app's change, writes the side file, reconnects;
    /// Production, SSL-configured and non-SQL sources are refused (nothing written)
    #[tokio::test]
    async fn allow_plaintext_keeps_the_password_and_reconnects() {
        let dir = std::env::temp_dir().join(format!("qurry-plain-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mk = |id: &str, kind| ConnectionConfig {
            id: id.into(), name: id.into(), kind, host: "db.example".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(mk("p", crate::store::DbKind::Postgres), Some("pw".into()), None).await.unwrap();
        // A pool that is open now (lazy: never connects)
        let lazy = sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new().host("db.example"));
        st.pools.write().await.insert("p".into(), Pool::Pg(lazy));
        // Another app turns on Read-only in between
        let other = Store::new(dir.clone());
        let mut c = other.get("p").unwrap().unwrap();
        c.read_only = true;
        other.save(&c).unwrap();
        let got = st.allow_plaintext("p".into()).await.unwrap();
        assert!(got.options.allow_plaintext && got.password_saved && got.read_only, "{got:?}");
        let saved = st.store.get("p").unwrap().unwrap();
        assert!(saved.options.allow_plaintext && saved.password_saved && saved.read_only, "{saved:?}");
        assert_eq!(st.secret("p").unwrap().as_deref(), Some("pw"));
        let side: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("connection-access.json")).unwrap()).unwrap();
        assert_eq!(side["p"]["allowPlaintext"], true);
        assert!(st.pools.read().await.is_empty(), "the pool was dropped");

        let mut prod = mk("prod", crate::store::DbKind::Postgres);
        prod.options.confirm_changes = true;
        let mut ssl = mk("ssl", crate::store::DbKind::Mysql);
        ssl.ssl = Some(serde_json::from_value(serde_json::json!({ "enabled": true, "mode": "require" })).unwrap());
        for (c, want) in [(prod, "Production"), (ssl, "SSL turned on"), (mk("r", crate::store::DbKind::Redis), "only for PostgreSQL and MySQL")] {
            let id = c.id.clone();
            st.save_connection(c, Some("pw".into()), None).await.unwrap();
            let e = st.allow_plaintext(id.clone()).await.unwrap_err().to_string();
            assert!(e.contains(want), "{e}");
            let c = st.store.get(&id).unwrap().unwrap();
            assert!(!c.options.allow_plaintext && c.password_saved, "{c:?}");
        }
        assert!(st.allow_plaintext("missing".into()).await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A form save (password kept) racing with another app saving or removing the password keeps the "password saved" flag
    /// matching the file: the saved source is read under the store lock, not before
    #[test]
    fn form_save_racing_a_password_save_keeps_the_flag() {
        for round in 0..4 {
            form_save_race(round % 2 == 0);
        }
    }

    fn form_save_race(end_with_password: bool) {
        let dir = std::env::temp_dir().join(format!("qurry-race-{}", uuid::Uuid::new_v4()));
        let cfg = ConnectionConfig {
            id: "r".into(), name: "r".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        Store::new(dir.clone()).save(&cfg).unwrap();
        // Create the key before the threads open the file
        crate::secrets::SecretFile::open(&dir).unwrap();
        let run = |pw: bool| {
            let (dir, cfg) = (dir.clone(), cfg.clone());
            move || {
                let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                // The password side sets and removes it in turn, ending on `end_with_password`
                let n = if pw { 20 + usize::from(end_with_password) } else { 40 };
                for i in 0..n {
                    let mut c = cfg.clone();
                    c.name = format!("r{i}");
                    let p = pw.then(|| if i % 2 == 0 { format!("pw{i}") } else { String::new() });
                    rt.block_on(st.save_connection(c, p, None)).unwrap();
                }
            }
        };
        std::thread::scope(|sc| {
            sc.spawn(run(false));
            sc.spawn(run(true));
        });
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        assert_eq!(st.store.get("r").unwrap().unwrap().password_saved, st.secret_file.contains("r").unwrap());
        assert_eq!(st.secret_file.contains("r").unwrap(), end_with_password);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An import that turns on "Allow unencrypted" while secrets.json is damaged fails as a whole: the store isn't changed,
    /// so after the file is repaired the old password isn't sent on the plaintext path
    #[tokio::test]
    async fn import_needing_a_password_removed_fails_before_writing() {
        let dir = std::env::temp_dir().join(format!("qurry-impdmg-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "m".into(), name: "m".into(), kind: crate::store::DbKind::Mysql, host: "db.example".into(), port: 3306,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        let file = dir.join("import.json");
        st.export_connections(file.display().to_string()).await.unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        v["connections"][0]["options"]["allowPlaintext"] = true.into();
        std::fs::write(&file, v.to_string()).unwrap();
        let good = std::fs::read(dir.join("secrets.json")).unwrap();
        std::fs::write(dir.join("secrets.json"), b"{ \"m\": \"tru").unwrap();
        let e = st.import_connections(file.display().to_string()).await.err().expect("import must fail").to_string();
        assert!(e.contains("damaged"), "{e}");
        let c = st.store.get("m").unwrap().unwrap();
        assert!(!c.options.allow_plaintext && c.password_saved, "{c:?}");
        // Repaired: the import goes through and the password is gone
        std::fs::write(dir.join("secrets.json"), good).unwrap();
        st.import_connections(file.display().to_string()).await.unwrap();
        let c = st.store.get("m").unwrap().unwrap();
        assert!(c.options.allow_plaintext && !c.password_saved);
        assert_eq!(st.saved_secret("m", c.password_saved).unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Each AppState run path asks the guard before it reaches a backend (console and tab queries, batches, CSV and full exports,
    /// Redis command lines and argument lists, MCP reads and CSV exports): each source's server is a tripwire (a listener that counts any
    /// connection), and a statement the guard refuses comes back with the guard's error while nothing connected.
    /// Redis has no SQL guard: its MCP read refuses a write command by name before connecting. (MCP CSV export of Redis commands is
    /// refused by the backend for every command, so it isn't a guard path and isn't checked here)
    #[tokio::test]
    async fn guard_runs_before_the_backend_on_every_run_path() {
        use crate::store::DbKind;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let trip = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = trip.local_addr().unwrap().port();
        let hits = std::sync::Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        tokio::spawn(async move {
            while let Ok((s, _)) = trip.accept().await {
                h.fetch_add(1, Ordering::SeqCst);
                drop(s);
            }
        });
        let dir = std::env::temp_dir().join(format!("qurry-guard-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        for (kind, write) in [(DbKind::Postgres, "delete from t"), (DbKind::Mysql, "delete from t"), (DbKind::Clickhouse, "delete from t where 1"), (DbKind::Redis, "DEL k")] {
            let id = format!("{kind:?}");
            let cfg = ConnectionConfig {
                id: id.clone(), name: id.clone(), kind, host: "127.0.0.1".into(), port, user: "u".into(), database: "d".into(), color: None, ssh: None,
                password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: true, options: Default::default(), ssl: None, redis: None, ssm: None,
            };
            st.save_connection(cfg, None, None).await.unwrap();
            let err = |r: AppResult<BatchResult>| r.expect_err("refused").to_string();
            let source = |e: String| assert!(e.contains(&format!("\"{id}\" is read-only")), "{kind:?}: {e}");
            source(st.run_query(id.clone(), write.into(), None, None).await.unwrap_err().to_string());
            source(st.run_query(id.clone(), write.into(), None, Some("tab".into())).await.unwrap_err().to_string());
            source(err(st.execute_batch(id.clone(), vec![write.into()], false).await));
            source(err(st.execute_batch_prepared(id.clone(), vec![write.into()]).await));
            source(st.export_csv(id.clone(), write.into(), dir.join("x.csv").display().to_string()).await.unwrap_err().to_string());
            let export = st.export(id.clone(), write.into(), crate::transfer::Format::Csv, crate::transfer::Dest::Clipboard, None, "job".into());
            source(export.await.err().expect("refused").to_string());
            if kind == DbKind::Redis {
                source(st.redis_exec_line(id.clone(), write.into()).await.unwrap_err().to_string());
                source(st.redis_exec(id.clone(), vec!["DEL".into(), "k".into()]).await.unwrap_err().to_string());
            } else {
                // Loading more of a result whose session was closed runs its statement again on a new one
                let more = session::More { sql: write.into(), fetched: 500, tx: false, schema: Some(String::new()) };
                st.more.lock().unwrap().insert("more".into(), Kept { conn_id: id.clone(), cfg: st.source(&id).unwrap(), run: 0, more });
                source(st.fetch_more("more".into(), None).await.unwrap_err().to_string());
            }
            let e = st.run_read_only(id.clone(), write.into(), 10).await.unwrap_err().to_string();
            match crate::guard::read_only_violation(write, kind, true) {
                Some(why) => {
                    assert!(e.contains(&why) && e.contains("--allow-write"), "{kind:?}: {e}");
                    let e = st.export_csv_read_only(id.clone(), write.into(), dir.join("x.csv")).await.unwrap_err().to_string();
                    assert_eq!(e, why, "{kind:?}");
                }
                None => assert_eq!(e, "DEL is not a read-only command", "{kind:?}"),
            }
            assert_eq!(hits.load(Ordering::SeqCst), 0, "{kind:?}: a refused statement reached the server");
        }
        assert!(st.pools.read().await.is_empty() && st.sessions.read().await.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Loading more after a reconnect runs the statement again: only a plain read outside a transaction
    #[test]
    fn reopen_only_plain_reads_outside_a_transaction() {
        use crate::store::DbKind::{Mysql, Postgres};
        let src = |kind| ConnectionConfig { kind, ..silent_source("s", 5432, None) };
        let k = |sql: &str, kind, tx: bool| Kept { conn_id: "s".into(), cfg: src(kind), run: 0, more: session::More { sql: sql.into(), fetched: 500, tx, schema: Some("public".into()) } };
        let refused = |sql: &str, kind, tx, manual| reopen_refused(&k(sql, kind, tx), &src(kind), manual);
        for (sql, kind) in [("select * from t order by id", Postgres), ("with a as (select 1) select * from a", Postgres), ("table t", Postgres), ("select * from t", Mysql)] {
            assert_eq!(refused(sql, kind, false, false), None, "{sql}");
        }
        let tx = refused("select * from t", Postgres, true, false).unwrap();
        assert!(tx.contains("transaction (Manual mode or BEGIN)") && tx.contains("Run the query again"), "{tx}");
        assert_eq!(refused("select * from t", Postgres, false, true), Some(tx), "the tab is in Manual mode now");
        // Another server or database since: its rows would be appended to the old result
        let changed = "The data source reconnected since this result was read (settings saved, disconnected or idle), and the data source's server or database changed since. Run the query again to load more rows.";
        for now in [ConnectionConfig { database: "template1".into(), ..src(Postgres) }, ConnectionConfig { host: "10.0.0.2".into(), ..src(Postgres) }, ConnectionConfig { port: 5433, ..src(Postgres) }, ConnectionConfig { user: "other".into(), ..src(Postgres) }] {
            assert_eq!(reopen_refused(&k("select * from t", Postgres, false), &now, false).as_deref(), Some(changed));
        }
        assert_eq!(reopen_refused(&k("select * from t", Postgres, false), &ConnectionConfig { name: "renamed".into(), read_only: true, ..src(Postgres) }, false), None, "only the target counts");
        // The schema couldn't be read: not run in the default one
        let mut nos = k("select * from t", Postgres, false);
        nos.more.schema = None;
        assert!(reopen_refused(&nos, &src(Postgres), false).unwrap().contains("couldn't read which schema it ran in"));
        // What a new connection runs first changed: the rows could say something else (a tenant variable, another time zone)
        let mut set = src(Postgres);
        set.options.startup_script = "set qurry.tenant = 'a'".into();
        let ka = Kept { cfg: set.clone(), ..k("select * from t", Postgres, false) };
        assert_eq!(reopen_refused(&ka, &ConnectionConfig { options: store::ConnOptions { startup_script: " set qurry.tenant = 'a'\n".into(), ..set.options.clone() }, ..set.clone() }, false), None, "only spaces around it");
        let setup = "The data source reconnected since this result was read (settings saved, disconnected or idle), and the data source's startup script, time zone or isolation level changed since. Run the query again to load more rows.";
        for opts in [
            store::ConnOptions { startup_script: "set qurry.tenant = 'b'".into(), ..set.options.clone() },
            store::ConnOptions { time_zone: Some("Asia/Seoul".into()), ..set.options.clone() },
            store::ConnOptions { isolation: Some("serializable".into()), ..set.options.clone() },
        ] {
            assert_eq!(reopen_refused(&ka, &ConnectionConfig { options: opts, ..set.clone() }, false).as_deref(), Some(setup));
        }
        for (sql, kind) in [
            ("select nextval('s'), g from generate_series(1, 1000) g", Postgres),
            ("select pg_sleep(1), * from t", Postgres),
            ("with d as (delete from t returning *) select * from d", Postgres),
            ("select * from t for update", Postgres),
            ("select pg_notify('c', 'x'), * from t", Postgres),
            ("select sleep(1), t.* from t", Mysql),
            ("select get_lock('x', 1), t.* from t", Mysql),
        ] {
            let e = refused(sql, kind, false, false).unwrap_or_else(|| panic!("{sql}"));
            assert!(e.contains("isn't a plain read") && e.contains("Run it again"), "{sql}: {e}");
        }
        for (sql, kind) in [("select * from t for share", Postgres), ("select * from t for key share", Postgres), ("select * from t lock in share mode", Mysql), ("select * from t for share", Mysql)] {
            let e = refused(sql, kind, false, false).unwrap_or_else(|| panic!("{sql}"));
            assert!(e.contains("it locks rows") && e.contains("Run it again"), "{sql}: {e}");
        }
    }

    /// Auto-disconnect: close pools and sessions after the idle time. keep-alive keeps the session alive.
    #[tokio::test]
    #[ignore]
    async fn housekeeping_keepalive_and_auto_disconnect() {
        let dir = std::env::temp_dir().join(format!("dbtool-hk-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let Some(crate::testenv::Server { mut cfg, .. }) = crate::testenv::pg() else { return };
        cfg.id = "hk".into();
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

    /// Load more after the data source reconnected (#80): saving its settings, Disconnect and the idle drop close the tab sessions
    /// (drop_pool), and fetch_more reopens the result on a new one after the rows already read. Real servers:
    /// QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres QURRY_TEST_MY=127.0.0.1:13306:root:dbtool:shop cargo test -p qurry-core fetch_more_after -- --ignored
    async fn fetch_more_after_reconnect(kind: crate::store::DbKind) {
        let Some(crate::testenv::Server { mut cfg, password }) = crate::testenv::server(kind) else { return };
        (cfg.id, cfg.name) = ("d".into(), "reconnect".into());
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        st.save_connection(cfg.clone(), password.clone(), None).await.unwrap();
        let q = |s: &str, tab: &str| st.run_query("d".into(), s.into(), Some(500), Some(tab.into()));
        let my = kind == crate::store::DbKind::Mysql;
        q(if my { "create database if not exists fetch_more_reconnect" } else { "create schema if not exists fetch_more_reconnect" }, "setup").await.unwrap();
        q("drop table if exists fetch_more_reconnect.t", "setup").await.unwrap();
        q("create table fetch_more_reconnect.t (n int primary key)", "setup").await.unwrap();
        let fill = match my {
            // No WITH RECURSIVE: the older-servers CI job runs this on MySQL 5.7
            true => "insert into fetch_more_reconnect.t select a.d * 100 + b.d * 10 + c.d + 1 from \
                     (select 0 d union all select 1 union all select 2 union all select 3 union all select 4 union all select 5 union all select 6 union all select 7 union all select 8 union all select 9 union all select 10 union all select 11) a, \
                     (select 0 d union all select 1 union all select 2 union all select 3 union all select 4 union all select 5 union all select 6 union all select 7 union all select 8 union all select 9) b, \
                     (select 0 d union all select 1 union all select 2 union all select 3 union all select 4 union all select 5 union all select 6 union all select 7 union all select 8 union all select 9) c",
            false => "insert into fetch_more_reconnect.t select g from generate_series(1, 1200) g",
        };
        q(fill, "setup").await.unwrap();
        st.close_session("setup".into()).await.unwrap();
        let first = |r: &QueryResult| r.rows[0][0].as_i64().or_else(|| r.rows[0][0].as_str().and_then(|s| s.parse().ok()));
        // The table only by its name: the console's schema (search_path · USE) is set on the new session too
        let schema = if my { "use fetch_more_reconnect" } else { "set search_path to fetch_more_reconnect, public" };
        let select = "select n from t order by n";
        let full = "select n from fetch_more_reconnect.t order by n";

        // Saving the settings: rows 1001 … after 500 + 500 were read
        q(schema, "a").await.unwrap();
        let r = q(select, "a").await.unwrap();
        assert_eq!((r.rows.len(), r.truncated), (500, true));
        let r = st.fetch_more("a".into(), Some(500)).await.unwrap();
        assert_eq!((first(&r), r.truncated), (Some(501), true));
        st.save_connection(cfg.clone(), None, None).await.unwrap();
        assert!(st.sessions.read().await.get("a").is_none(), "saving closed the tab session");
        let r = st.fetch_more("a".into(), Some(500)).await.unwrap();
        assert_eq!((r.rows.len(), first(&r), r.truncated), (200, Some(1001), false));
        st.disconnect("d".into()).await.unwrap();
        let e = st.fetch_more("a".into(), Some(500)).await.unwrap_err().to_string();
        assert_eq!(e, "Session closed", "a finished result isn't reopened");

        // Disconnect: rows 501 …, and the reopened result goes on after a second disconnect
        q(schema, "b").await.unwrap();
        q(select, "b").await.unwrap();
        st.disconnect("d".into()).await.unwrap();
        let r = st.fetch_more("b".into(), Some(500)).await.unwrap();
        assert_eq!((r.rows.len(), first(&r), r.truncated), (500, Some(501), true));
        st.disconnect("d".into()).await.unwrap();
        let r = st.fetch_more("b".into(), Some(500)).await.unwrap();
        assert_eq!((r.rows.len(), first(&r), r.truncated), (200, Some(1001), false));

        // A result read in a Manual-mode transaction: not reopened (its transaction ended with the old connection)
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        assert!(q(full, "m").await.unwrap().truncated);
        st.disconnect("d".into()).await.unwrap();
        let e = st.fetch_more("m".into(), Some(500)).await.unwrap_err().to_string();
        assert!(e.contains("transaction (Manual mode or BEGIN)") && e.contains("Run the query again"), "{e}");
        assert!(st.sessions.read().await.get("m").is_none(), "refused before opening a session");

        // A statement that isn't a plain read: not run again behind the user's back
        let sleep = if my { "select n, sleep(0) from fetch_more_reconnect.t order by n" } else { "select n, pg_sleep(0) from fetch_more_reconnect.t order by n" };
        assert!(q(sleep, "w").await.unwrap().truncated);
        st.disconnect("d".into()).await.unwrap();
        let e = st.fetch_more("w".into(), Some(500)).await.unwrap_err().to_string();
        assert!(e.contains("isn't a plain read") && e.contains("Run it again"), "{e}");
        assert!(st.sessions.read().await.get("w").is_none(), "refused before opening a session");

        // The source now points at another database: its rows aren't appended to this result (refused before connecting)
        assert!(q(full, "e").await.unwrap().truncated);
        st.save_connection(ConnectionConfig { database: "qurry_fetch_more_other".into(), ..cfg.clone() }, None, None).await.unwrap();
        let e = st.fetch_more("e".into(), Some(500)).await.unwrap_err().to_string();
        assert!(e.contains("server or database changed since") && e.contains("Run the query again"), "{e}");
        assert!(st.sessions.read().await.get("e").is_none() && st.pools.read().await.is_empty(), "nothing connected");
        let script = ConnectionConfig { options: store::ConnOptions { startup_script: "select 1".into(), ..cfg.options.clone() }, ..cfg.clone() };
        st.save_connection(script, None, None).await.unwrap();
        let e = st.fetch_more("e".into(), Some(500)).await.unwrap_err().to_string();
        assert!(e.contains("startup script, time zone or isolation level changed since"), "{e}");
        st.save_connection(cfg.clone(), None, None).await.unwrap();

        // A reopen that fails: trying again says why again (not "No more rows")
        q("create table fetch_more_reconnect.gone as select n from fetch_more_reconnect.t", "setup").await.unwrap();
        assert!(q("select n from fetch_more_reconnect.gone order by n", "g").await.unwrap().truncated);
        st.disconnect("d".into()).await.unwrap();
        q("drop table fetch_more_reconnect.gone", "setup").await.unwrap();
        for _ in 0..2 {
            let e = st.fetch_more("g".into(), Some(500)).await.unwrap_err().to_string();
            assert!(e.contains("reopening the result on the new connection failed"), "{e}");
        }

        // A closed tab forgets its result
        q(full, "c").await.unwrap();
        st.close_session("c".into()).await.unwrap();
        st.disconnect("d".into()).await.unwrap();
        assert_eq!(st.fetch_more("c".into(), Some(500)).await.unwrap_err().to_string(), "Session closed");

        q(if my { "drop database fetch_more_reconnect" } else { "drop schema fetch_more_reconnect cascade" }, "setup").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_more_after_reconnect_pg() {
        fetch_more_after_reconnect(crate::store::DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_more_after_reconnect_mysql() {
        fetch_more_after_reconnect(crate::store::DbKind::Mysql).await;
    }

    /// A load-more queued behind a run in the same tab was asked for the old result: it must not move the new result's cursor
    /// (the GUI drops its answer, so the new result would skip 500 rows). And two load-mores reopening one result at once go on
    /// one after the other. QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-core fetch_more_queued -- --ignored
    #[tokio::test]
    #[ignore]
    async fn fetch_more_queued_behind_a_run_or_a_reopen() {
        let Some(crate::testenv::Server { cfg, password }) = crate::testenv::pg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        st.save_connection(ConnectionConfig { id: "q".into(), name: "q".into(), ..cfg }, password, None).await.unwrap();
        let first = |r: &QueryResult| r.rows[0][0].as_i64();
        let run = |sql: &'static str| st.run_query("q".into(), sql.into(), Some(500), Some("t".into()));
        let later = |ms| tokio::time::sleep(std::time::Duration::from_millis(ms));
        assert_eq!(first(&run("select g from generate_series(1, 2000) g order by g").await.unwrap()), Some(1));
        // The run holds the session for 0.5 s; the load-more asked meanwhile waits for it
        let slow = "select g + 10000 from generate_series(1, 2000) g, (select pg_sleep(0.5)) s order by 1";
        let (ran, queued) = tokio::join!(run(slow), async { later(100).await; st.fetch_more("t".into(), Some(500)).await });
        assert_eq!(first(&ran.unwrap()), Some(10001));
        assert_eq!(queued.unwrap_err().to_string(), "Session closed");
        assert_eq!(first(&st.fetch_more("t".into(), Some(500)).await.unwrap()), Some(10501), "the new result's rows are contiguous");

        // Two load-mores after a reconnect: one reopens, the other goes on from its cursor
        run("select g from generate_series(1, 2000) g order by g").await.unwrap();
        st.disconnect("q".into()).await.unwrap();
        let (a, b) = tokio::join!(st.fetch_more("t".into(), Some(500)), st.fetch_more("t".into(), Some(500)));
        let mut got = vec![first(&a.unwrap()), first(&b.unwrap())];
        got.sort();
        assert_eq!(got, vec![Some(501), Some(1001)]);
        assert_eq!(first(&st.fetch_more("t".into(), Some(500)).await.unwrap()), Some(1501));
    }

    /// A load-more reopening its result and a new run in the same tab, both opening the tab's session at once (slow connects
    /// make them overlap), in both orders: they share one session, and the new result goes on with its own rows, never the old
    /// statement's. QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-core fetch_more_reopen_racing -- --ignored
    #[tokio::test]
    #[ignore]
    async fn fetch_more_reopen_racing_a_run_keeps_the_new_result() {
        let Some(crate::testenv::Server { cfg: pg, password }) = crate::testenv::pg() else { return };
        let upstream = format!("{}:{}", pg.host, pg.port);
        // Each connection starts forwarding after 300 ms
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut a, _)) = l.accept().await {
                let upstream = upstream.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    let mut b = tokio::net::TcpStream::connect(upstream).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut a, &mut b).await;
                });
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        st.save_connection(ConnectionConfig { id: "slow".into(), name: "slow".into(), port, ..pg }, password, None).await.unwrap();
        let first = |r: &QueryResult| r.rows[0][0].as_i64();
        let run = |sql: &'static str| st.run_query("slow".into(), sql.into(), Some(500), Some("t".into()));
        let old = "select g from generate_series(1, 1200) g order by g";
        let new = "select g + 10000 from generate_series(1, 1200) g order by 1";
        let later = |ms| tokio::time::sleep(std::time::Duration::from_millis(ms));
        for run_first in [false, true] {
            assert_eq!(first(&run(old).await.unwrap()), Some(1));
            st.disconnect("slow".into()).await.unwrap();
            let (more, ran) = match run_first {
                false => tokio::join!(st.fetch_more("t".into(), Some(500)), async { later(100).await; run(new).await }),
                true => {
                    let (r, f) = tokio::join!(run(new), async { later(100).await; st.fetch_more("t".into(), Some(500)).await });
                    (f, r)
                }
            };
            assert_eq!(first(&ran.unwrap()), Some(10001), "run first: {run_first}");
            // The old result's next rows, or refused because the new run took the session first: never the new statement's
            if let Ok(m) = more {
                assert_eq!(first(&m), Some(501), "run first: {run_first}");
            }
            let next = st.fetch_more("t".into(), Some(500)).await.unwrap();
            assert_eq!(first(&next), Some(10501), "run first: {run_first}: the new result goes on with its own rows");
            assert_eq!(st.sessions.read().await.len(), 1);
        }
    }
}
