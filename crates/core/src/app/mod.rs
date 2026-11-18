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

mod pools;
pub use pools::*;
mod sessions;
pub use sessions::*;
mod sources;
pub use sources::*;
mod run;
pub use run::*;
mod tx;
pub use tx::*;

pub struct SessionEntry {
    pub conn_id: String,
    pub pid: i64,
    pub session: std::sync::Arc<tokio::sync::Mutex<Session>>,
    /// When a run or a load-more last took it or finished with it (tab_session). By the wall clock (idle_for)
    pub(crate) used: std::sync::Mutex<std::time::SystemTime>,
    /// Set while housekeeping holds the session's lock (closing its idle cursor, its keep-alive ping) and no user transaction is
    /// open in it: then a locked session isn't running anything of the user's, so it doesn't count as busy (in_use) or as maybe
    /// holding a transaction (close_all)
    pub(crate) housekept: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// A statement that changes the session (session::changes_session: SET, RESET, USE …) ran in it since it was opened (Kept::changed)
    pub(crate) changed: std::sync::atomic::AtomicBool,
}

/// The clocks when they were last read (AppState::wakes), and how many times the Mac was found to have slept so far
struct Clock {
    wall: std::time::SystemTime,
    mono: std::time::Instant,
    wakes: u64,
}

impl Default for Clock {
    fn default() -> Self {
        Self { wall: std::time::SystemTime::now(), mono: std::time::Instant::now(), wakes: 0 }
    }
}

/// How much further the wall clock must have moved than an Instant between two reads for the Mac to count as having slept.
/// Housekeeping reads both every 5s, so a sleep this long is never missed; a shorter one leaves tunnels as they were
const WAKE_GAP: std::time::Duration = std::time::Duration::from_secs(10);

/// Whether the Mac slept between two reads of the clocks: `wall` = how far the wall clock moved (None: it was set back), `mono` =
/// how far an Instant moved (it stops while the Mac sleeps). A clock set back is no sleep; one set forward by WAKE_GAP or more
/// counts as one (one check too many, never one too few)
fn slept(wall: Option<std::time::Duration>, mono: std::time::Duration) -> bool {
    wall.is_some_and(|w| w.saturating_sub(mono) >= WAKE_GAP)
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
    /// Data Source id → last keep-alive time (wall clock: idle_for)
    pub last_ping: std::sync::Mutex<HashMap<String, std::time::SystemTime>>,
    /// Tab id → its transaction was lost with its session in a disconnect (drop_pool_logged): the next use says so and sends nothing
    lost_tx: std::sync::Mutex<HashMap<String, LostTx>>,
    /// Data source id → how many exports, imports, backups, restores and subset copies run on it (AppState::job_running)
    running: std::sync::Mutex<HashMap<String, usize>>,
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
    /// The app's grids handle cut cells (QueryResult::cut, the whole value read with fetch_values), so its consoles' PostgreSQL
    /// and MySQL results read long values cut (store::grid_cut_chars): the GUI and the TUI. Off (a grid that would edit a cut text as the
    /// whole value and write it back; tests), they read whole values. An app turns it on in the same change that makes its grid
    /// handle cut cells, never before
    pub cut_grids: bool,
    /// Secrets key → password of a temporary data source (Store::temporary): kept in memory only, never in the secrets file
    temporary: std::sync::Mutex<HashMap<String, String>>,
    /// Tab id → the last use of its session that tx_reminders reminded about (once per stretch of the transaction sitting unused)
    reminded: std::sync::Mutex<HashMap<String, std::time::SystemTime>>,
    /// Notices that the Mac slept (AppState::wakes)
    clock: std::sync::Mutex<Clock>,
    /// The wakes housekeeping has checked the tunnels and pools after
    housekept: std::sync::atomic::AtomicU64,
    /// Data source id → how many waits for its tunnel to answer, or opens of a new one in place of one that stopped, are under
    /// way (AppState::reconnecting)
    reconnecting: std::sync::Mutex<HashMap<String, usize>>,
}

impl AppState {
    /// How many times the Mac was found to have slept: each call reads the wall clock and an Instant and compares how far each
    /// moved since the last call (slept). Read on every pool use and every housekeeping beat, so no sleep goes unnoticed
    pub(crate) fn wakes(&self) -> u64 {
        let mut c = self.clock.lock().unwrap();
        let (wall, mono) = (std::time::SystemTime::now(), std::time::Instant::now());
        if slept(wall.duration_since(c.wall).ok(), mono - c.mono) {
            c.wakes += 1;
        }
        (c.wall, c.mono) = (wall, mono);
        c.wakes
    }

    /// The cut a console's PostgreSQL cursor or MySQL page reads long values at (Session::set_cut): store::grid_cut_chars where the grids know
    /// cut cells (cut_grids), else 0. A read of settings.json: taken before the session's lock
    pub(crate) fn console_cut(&self) -> usize {
        if self.cut_grids { self.store.grid_cut_chars() } else { 0 }
    }
}

impl AppState {
    pub fn new(store: Store, secret_file: crate::secrets::SecretFile) -> Self {
        let history = crate::history::History::new(&store.dir());
        let audit = crate::audit::Audit::new(&store.dir());
        Self { history, audit, store, pools: RwLock::new(HashMap::new()), sessions: RwLock::new(HashMap::new()), tunnels: RwLock::new(HashMap::new()), connecting: Default::default(), secret_file, last_used: Default::default(), last_ping: Default::default(), lost_tx: Default::default(), running: Default::default(), tab_manual: Default::default(), more: Default::default(), runs: Default::default(), jobs: Default::default(), mcp: false, cut_grids: false, temporary: Default::default(), reminded: Default::default(), clock: Default::default(), housekept: Default::default(), reconnecting: Default::default() }
    }
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

/// The server ended the session because it sat idle in a transaction: PostgreSQL's idle_in_transaction_session_timeout (25P03),
/// MySQL's wait_timeout (4031, 8.0.24 and later). Some(true) = MySQL. A statement sent then never ran (a COMMIT committed nothing)
pub(crate) fn idle_ended(e: &AppError) -> Option<bool> {
    match e {
        AppError::Db(sqlx::Error::Database(d)) if d.code().as_deref() == Some("25P03") => Some(false),
        _ if crate::mysql::error_number(e) == Some(4031) => Some(true),
        _ => None,
    }
}

/// One pass: keep-alive queries / drop idle connections. → the disconnected data sources: (id, why, with its name)
pub async fn housekeeping(state: &AppState) -> Vec<(String, String)> {
    use qurry_shared::errors::Disconnected;
    use std::time::{Duration, Instant};
    let open_bases = || async {
        let pools = state.pools.read().await;
        let mut b: Vec<String> = pools.keys().map(|k| split_target(k).0.to_string()).collect();
        b.sort();
        b.dedup();
        b
    };
    let mut dropped = Vec::new();
    // The Mac woke: every source's tunnel and pools are checked at once (settle), before anything below waits on them
    let wakes = state.wakes();
    if state.housekept.swap(wakes, SeqCst) < wakes {
        let bases = open_bases().await;
        let found = futures::future::join_all(bases.iter().map(|b| settle(state, b))).await;
        for (base, why) in bases.into_iter().zip(found) {
            if let Some(why) = why {
                let why = state.source(&base).map_or_else(|_| format!("Disconnected: {base}"), |c| disconnected(&c, why));
                dropped.push((base, why));
            }
        }
    }
    let bases = open_bases().await;
    // Close cursors left unread for a long time (so open transactions do not block VACUUM/DDL): all sessions at once, each within
    // 5s. Skip sessions that are running, and those with a user transaction open
    let sessions: Vec<_> = state.sessions.read().await.iter().map(|(t, e)| (t.clone(), e.session.clone(), e.housekept.clone())).collect();
    futures::future::join_all(sessions.iter().map(|(tab, s, flag)| async move {
        let Ok(mut g) = s.try_lock() else { return };
        let Some(_held) = Housekept::hold(flag, &g) else { return };
        if g.close_idle_cursor(session::CURSOR_IDLE).await.is_none() {
            // Its connection didn't answer (a tunnel that carries nothing any more): given up after this one try, so neither the
            // next beat nor a run waiting for the session waits another 5s on it. The tab's next use opens a new session, and
            // loading more reopens the result there (fetch_more)
            crate::log!("[qurry] closing the idle cursor of {tab} didn't answer within 5s; closing its session");
            g.abandon();
            let mut all = state.sessions.write().await;
            if all.get(tab).is_some_and(|e| std::sync::Arc::ptr_eq(&e.session, s)) {
                all.remove(tab);
            }
        }
    }))
    .await;
    // Not held below: closing a pool waits for its sessions' connections to come back
    drop(sessions);
    for base in bases {
        // A stopped tunnel's port is free for anyone on this machine: don't let the pools (keep-alive) connect to it again
        let cfg = state.store.get(&base).ok().flatten();
        if tunnel_alive(state, &base).await == Some(false) {
            drop_pool_logged(state, &base, crate::audit::BY_SERVER).await;
            let why = cfg.as_ref().map_or_else(|| format!("Disconnected: {base}"), |c| disconnected(c, Disconnected::TunnelStopped));
            dropped.push((base, why));
            continue;
        }
        let Some(cfg) = cfg else { continue };
        let now = Instant::now();
        if let Some(secs) = cfg.options.auto_disconnect_secs.filter(|s| *s > 0) {
            let last = *state.last_used.lock().unwrap().entry(base.clone()).or_insert(now);
            // Do not disconnect if any session has a running query or uncommitted changes (disconnecting would lose the changes)
            if !in_use(state, &base).await && now.duration_since(last) >= Duration::from_secs(secs as u64) {
                crate::log!("[qurry] {base} unused for {secs}s; disconnecting it (auto-disconnect)");
                drop_pool_logged(state, &base, crate::audit::DISCONNECTED).await;
                state.last_used.lock().unwrap().remove(&base);
                dropped.push((base, disconnected(&cfg, Disconnected::Idle)));
                continue;
            }
        }
        if let Some(secs) = cfg.options.keep_alive_secs.filter(|s| *s > 0) {
            let due = {
                let mut m = state.last_ping.lock().unwrap();
                let last = *m.entry(base.clone()).or_insert_with(std::time::SystemTime::now);
                if idle_for(last, Duration::from_secs(secs as u64)) {
                    m.insert(base.clone(), std::time::SystemTime::now());
                    true
                } else {
                    false
                }
            };
            if due {
                let c = state.connecting(&base);
                let g = c.generation.load(SeqCst);
                let pools: Vec<Pool> = state.pools.read().await.iter().filter(|(k, _)| belongs(k, &base)).map(|(_, p)| p.clone()).collect();
                let sessions: Vec<_> = state.sessions.read().await.values().filter(|e| belongs(&e.conn_id, &base)).map(|e| (e.session.clone(), e.housekept.clone())).collect();
                // Together, each within 5s: one that doesn't answer doesn't hold up the others
                let pinged = futures::future::join_all(pools.iter().map(|p| p.ping())).await;
                let pinged_sessions = futures::future::join_all(sessions.iter().map(|(s, flag)| async move {
                    match s.try_lock() {
                        Ok(mut g) => {
                            let _held = Housekept::hold(flag, &g);
                            tokio::time::timeout(SESSION_PING, g.ping()).await.is_ok()
                        }
                        Err(_) => true,
                    }
                }))
                .await;
                // Not held while the tunnel is closed: closing a pool waits for its connections to come back
                drop((pools, sessions));
                let failed = pinged.iter().chain(&pinged_sessions).filter(|ok| !**ok).count();
                if failed > 0 {
                    crate::log!("[qurry] keep-alive of {base}: {failed} of {} connection(s) didn't answer", pinged.len() + pinged_sessions.len());
                }
                if pinged.iter().chain(&pinged_sessions).all(|ok| *ok) && c.generation.load(SeqCst) == g {
                    c.pinged();
                }
                // Through a tunnel that carries nothing any more (sleep, a network change), it is closed now (drop_dead_tunnel): a
                // statement waiting through it ends, and the next use opens a new one
                if cfg.tunneled() && pinged.iter().chain(&pinged_sessions).any(|ok| !ok) {
                    let asked = Instant::now();
                    let _connecting = c.failed.lock().await;
                    if c.generation.load(SeqCst) == g && drop_dead_tunnel(state, &c, &base, &cfg, g, asked).await.is_ok() {
                        dropped.push((base, disconnected(&cfg, Disconnected::TunnelSilent)));
                    }
                }
            }
        }
    }
    for (id, why) in &dropped {
        crate::log!("[qurry] {why} ({id})");
    }
    dropped
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The tab's session as if it sat unused for longer than SESSION_IDLE: its next use checks it first (tab_session)
    pub(crate) async fn age_session(st: &AppState, tab: &str) {
        let all = st.sessions.read().await;
        let mut used = all.get(tab).expect("a console session").used.lock().unwrap();
        *used -= SESSION_IDLE + std::time::Duration::from_secs(1);
    }

    /// A server that accepts and never answers (a DB or SSH host behind a firewall that drops packets)
    pub(crate) async fn silent_server() -> u16 {
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

    pub(super) fn silent_source(id: &str, port: u16, ssh: Option<serde_json::Value>) -> ConnectionConfig {
        let mut c = serde_json::json!({ "id": id, "name": id, "kind": "postgres", "host": "127.0.0.1", "port": port, "user": "u", "database": "d" });
        if let Some(ssh) = ssh {
            c["ssh"] = ssh;
        }
        serde_json::from_value(c).unwrap()
    }

    /// A connect's timeout says which way it went: through the AWS SSM tunnel (instance), the SSH tunnel (server) or direct
    #[test]
    fn a_timeout_says_which_way_it_went() {
        let timeout = || AppError::Db(sqlx::Error::PoolTimedOut);
        let e = timeout_error(&silent_source("d", 5432, None), timeout(), false).to_string();
        assert!(e.starts_with("Can't connect to 127.0.0.1:5432 — the server didn't answer within 10s") && e.contains("(VPN, firewall)") && !e.contains("tunnel"), "{e}");
        let ssh = silent_source("s", 5432, Some(serde_json::json!({ "enabled": true, "host": "bastion.example.com", "port": 2222, "user": "me" })));
        let e = timeout_error(&ssh, timeout(), true).to_string();
        assert!(e.contains("through the SSH tunnel (bastion.example.com:2222)") && e.contains("also after Qurry opened the tunnel again") && e.contains("the SSH server can reach it") && !e.contains("VPN"), "{e}");
        let mut ssm = silent_source("m", 5432, None);
        ssm.ssm = Some(store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: " i-0123456789abcdef0 ".into() });
        let e = timeout_error(&ssm, timeout(), false).to_string();
        assert!(e.contains("through the AWS SSM tunnel (instance i-0123456789abcdef0)") && e.contains("the instance can reach it") && !e.contains("also after") && !e.contains("VPN"), "{e}");
        // Any other error stays as it is
        assert_eq!(timeout_error(&ssm, AppError::Msg("password authentication failed".into()), true).to_string(), "password authentication failed");
        // A console's session that didn't answer with a transaction open: lost, nothing sent, and recover() leaves the text alone
        for closed in [true, false] {
            let e = lost_session_error(&ssm, closed);
            let m = e.to_string();
            assert!(m.contains("through the AWS SSM tunnel (instance i-0123456789abcdef0)") && m.contains("The transaction is lost") && m.contains("nothing was sent"), "{m}");
            assert_eq!(m.contains("closed both"), closed, "{m}");
            assert!(!is_conn_lost(&e), "{m}");
        }
        assert!(lost_session_error(&ssh, false).to_string().contains("through the SSH tunnel (bastion.example.com:2222)"));
    }

    /// A session's idle time is wall-clock time: an Instant stops while the Mac sleeps, so a console unused overnight would skip
    /// its check. A clock set back counts as idle (one check too many)
    #[test]
    fn idle_time_counts_by_the_wall_clock() {
        use std::time::{Duration, SystemTime};
        let now = SystemTime::now();
        assert!(!idle_for(now, SESSION_IDLE));
        assert!(idle_for(now - SESSION_IDLE - Duration::from_secs(1), SESSION_IDLE));
        assert!(idle_for(now - Duration::from_secs(8 * 3600), SESSION_IDLE), "overnight");
        assert!(idle_for(now + Duration::from_secs(60), SESSION_IDLE), "the clock was set back");
    }

    /// The Mac as if it slept `by` since the clocks were last read: the wall clock moved on, an Instant didn't (AppState::wakes)
    pub(crate) fn sleep_mac(st: &AppState, by: std::time::Duration) {
        st.clock.lock().unwrap().wall -= by;
    }

    /// The source's pools as if unused for `by` longer by the wall clock (Connecting::stale)
    pub(crate) fn idle_source(st: &AppState, base: &str, by: std::time::Duration) {
        if let Some((at, _)) = st.connecting(base).used.lock().unwrap().as_mut() {
            *at -= by;
        }
    }
}
