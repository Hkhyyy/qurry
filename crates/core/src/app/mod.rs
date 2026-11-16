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

/// A console whose session a disconnect closed while a transaction may have been open in it (AppState::lost_tx)
struct LostTx {
    /// What its next run, load-more, Commit or Roll back says instead of running (once)
    msg: String,
    /// Something ran in the session then, so whether a transaction was open is known only when that ends (settle_lost)
    busy: bool,
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

/// How long a Production console's transaction may sit open and unused before the console reminds about it (#106)
pub const TX_IDLE_REMIND: std::time::Duration = std::time::Duration::from_secs(5 * 60);

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

    /// The consoles of Production data sources whose transaction has been open and unused for TX_IDLE_REMIND: (tab id, what to
    /// tell them, qurry_shared::errors::tx_idle_reminder). Once per stretch: a statement, Commit or Roll back starts a new one.
    /// GUI and TUI ask on their housekeeping beat
    pub async fn tx_reminders(&self) -> Vec<(String, String)> {
        self.tx_reminders_after(TX_IDLE_REMIND).await
    }

    pub(crate) async fn tx_reminders_after(&self, after: std::time::Duration) -> Vec<(String, String)> {
        let open: Vec<(String, String, std::time::SystemTime, std::sync::Arc<tokio::sync::Mutex<Session>>)> =
            self.sessions.read().await.iter().map(|(t, e)| (t.clone(), e.conn_id.clone(), *e.used.lock().unwrap(), e.session.clone())).collect();
        let mut out = Vec::new();
        for (tab, conn, used, sess) in open {
            let Ok(cfg) = self.source(&conn) else { continue };
            // Busy (a statement runs in it): not idle
            let Some(changed) = sess.try_lock().ok().filter(|s| s.in_tx()).map(|s| s.tx_changed()) else { continue };
            // UNIX_EPOCH: a slow session marked to be checked again on its next use (tab_session), not one unused since 1970
            let checking = used == std::time::SystemTime::UNIX_EPOCH;
            if !cfg.options.confirm_changes || checking || !idle_for(used, after) || self.reminded.lock().unwrap().get(&tab) == Some(&used) {
                continue;
            }
            self.reminded.lock().unwrap().insert(tab.clone(), used);
            let minutes = used.elapsed().map_or(after.as_secs(), |d| d.as_secs()) / 60;
            // Keep-alive pings the console's session too, which restarts the server's clock: then the server doesn't end it
            let keep_alive = cfg.options.keep_alive_secs.filter(|k| *k > 0).map(|k| k as u64);
            let timeout = self.idle_tx_timeout(&conn).await.filter(|t| keep_alive.is_none_or(|k| k >= *t)).map(|s| (cfg.kind == store::DbKind::Mysql, s));
            out.push((tab, qurry_shared::errors::tx_idle_reminder(&cfg.name, minutes, changed, timeout)));
        }
        let live = self.sessions.read().await.keys().cloned().collect::<std::collections::HashSet<_>>();
        self.reminded.lock().unwrap().retain(|t, _| live.contains(t));
        out
    }

    /// The server's limit for a session sitting idle in a transaction, in seconds (None: none, or it couldn't be read): read on the
    /// pool, whose connections run the same startup script as the console's (a statement in the console's session would count as a
    /// use and restart PostgreSQL's clock)
    async fn idle_tx_timeout(&self, conn: &str) -> Option<u64> {
        let pool = self.pools.read().await.get(conn).cloned()?;
        let read = async {
            match &pool {
                Pool::Pg(p) => sqlx::query_scalar::<_, i64>("select setting::int8 / 1000 from pg_catalog.pg_settings where name = 'idle_in_transaction_session_timeout'").persistent(false).fetch_one(p).await.ok(),
                Pool::My(p) => sqlx::query_scalar::<_, i64>("select cast(@@session.wait_timeout as signed)").fetch_one(p).await.ok(),
                _ => None,
            }
        };
        let s = tokio::time::timeout(std::time::Duration::from_secs(3), read).await.ok().flatten()?;
        u64::try_from(s).ok().filter(|s| *s > 0)
    }
}

impl AppState {
    pub fn new(store: Store, secret_file: crate::secrets::SecretFile) -> Self {
        let history = crate::history::History::new(&store.dir());
        let audit = crate::audit::Audit::new(&store.dir());
        Self { history, audit, store, pools: RwLock::new(HashMap::new()), sessions: RwLock::new(HashMap::new()), tunnels: RwLock::new(HashMap::new()), connecting: Default::default(), secret_file, last_used: Default::default(), last_ping: Default::default(), lost_tx: Default::default(), running: Default::default(), tab_manual: Default::default(), more: Default::default(), runs: Default::default(), jobs: Default::default(), mcp: false, temporary: Default::default(), reminded: Default::default(), clock: Default::default(), housekept: Default::default(), reconnecting: Default::default() }
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

/// What a console says when a disconnect closed its session with a transaction open in it (drop_pool_logged): word = how
fn tx_lost_error(cfg: &ConnectionConfig, word: &str) -> String {
    let what = match word {
        crate::audit::DISCONNECTED => format!("\"{}\" was disconnected (Disconnect, or its settings were saved) while a transaction was open in this console", cfg.name),
        _ => format!(
            "Qurry closed this console's connection to \"{}\"{} because the data source stopped answering, while a transaction was open in it",
            cfg.name,
            way(cfg).map(|w| format!(" through {w}")).unwrap_or_default()
        ),
    };
    format!("{what}. {}", qurry_shared::errors::TX_LOST_NOTHING_SENT)
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
impl AppState {
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

    /// Tx: Auto / Manual
    pub async fn set_tx_mode(&self, id: String, tab_id: String, manual: bool) -> AppResult<()> {
        // Manual is kept first: when the session can't be opened now (a Production console opened while its source doesn't
        // answer), the one its next run opens starts in Manual too, as the tab shows, never in Auto committing at once. Not for a
        // source whose sessions refuse Manual (Redis, driver plugins): every session the tab opens would refuse it again, and no
        // statement would run
        if manual && self.source(&id)?.supports_manual() {
            self.tab_manual.lock().unwrap().insert(tab_id.clone(), true);
        }
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

    /// Production: the COMMIT of logged changes goes into the log first; if it can't be written, the transaction stays open.
    /// → a warning when it committed but its result couldn't be logged
    pub async fn commit_tx(&self, tab_id: String) -> AppResult<Option<String>> {
        let TabSession::Open(sess, conn) = tab_session(self, &tab_id).await? else { return Err(AppError::Msg("Session closed".into())) };
        let cfg = self.source(&conn).ok();
        let started = match &cfg {
            Some(c) => self.audit.before(&conn, &c.name, c.kind, Some(&tab_id), "COMMIT")?,
            None => None,
        };
        let r = {
            let mut s = sess.lock().await;
            let r = s.commit().await.map(|_| db::query_result(vec![], vec![], 0, false, std::time::Instant::now(), false));
            settle_lost(self, &tab_id, s.in_tx());
            r
        };
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
        let TabSession::Open(sess, _) = tab_session(self, &tab_id).await? else { return Err(AppError::Msg("Session closed".into())) };
        {
            let mut s = sess.lock().await;
            let r = s.rollback_tx().await;
            settle_lost(self, &tab_id, s.in_tx());
            r?;
        }
        Ok(self.audit_tx_end(&tab_id, "ROLLBACK").await.err().map(|e| e.to_string()))
    }
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

    /// The texts of a console's transaction kept (a slow session through a tunnel that answers) or lost in a disconnect: they name
    /// the source and the way, say nothing was sent, and aren't taken for a lost connection (recover leaves them as they are)
    #[test]
    fn a_console_says_what_became_of_its_transaction() {
        let mut ssm = silent_source("m", 5432, None);
        ssm.name = "prod".into();
        ssm.ssm = Some(store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: "i-0123456789abcdef0".into() });
        let kept = slow_session_error(&ssm);
        let m = kept.to_string();
        assert!(m.contains("through the AWS SSM tunnel (instance i-0123456789abcdef0)") && m.contains("within 5s") && m.contains("kept it with the transaction open") && m.contains("Nothing was sent"), "{m}");
        assert!(!is_conn_lost(&kept), "{m}");
        let lost = tx_lost_error(&ssm, crate::audit::BY_SERVER);
        assert!(lost.starts_with("Qurry closed this console's connection to \"prod\" through the AWS SSM tunnel (instance i-0123456789abcdef0) because") && lost.contains("nothing was sent"), "{lost}");
        let direct = tx_lost_error(&silent_source("d", 5432, None), crate::audit::BY_SERVER);
        assert!(direct.starts_with("Qurry closed this console's connection to \"d\" because"), "{direct}");
        let gone = tx_lost_error(&ssm, crate::audit::DISCONNECTED);
        assert!(gone.starts_with("\"prod\" was disconnected") && gone.contains("The transaction is lost"), "{gone}");
        for m in [lost, gone] {
            assert!(!is_conn_lost(&AppError::Msg(m.clone())), "{m}");
        }
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

    /// A Production console's transaction left open and unused is reminded about once per stretch, with the server's limit for
    /// it (#106); a source that isn't Production, a console without a transaction and one that ended it aren't
    #[tokio::test]
    #[ignore]
    async fn idle_production_transaction_is_reminded_once() {
        let Some(crate::testenv::Server { cfg, password }) = crate::testenv::pg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let mut prod = ConnectionConfig { id: "p".into(), name: "prod".into(), ..cfg.clone() };
        prod.options.confirm_changes = true;
        prod.options.startup_script = "set idle_in_transaction_session_timeout = '10min'".into();
        // The same with keep-alive, whose ping restarts the server's clock: the server doesn't end it then
        let mut kept = ConnectionConfig { id: "k".into(), name: "kept".into(), ..prod.clone() };
        kept.options.keep_alive_secs = Some(60);
        st.save_connection(prod, password.clone(), None).await.unwrap();
        st.save_connection(kept, password.clone(), None).await.unwrap();
        st.save_connection(ConnectionConfig { id: "d".into(), name: "dev".into(), ..cfg }, password, None).await.unwrap();
        let now = std::time::Duration::ZERO;
        let change = "create temp table qurry_reminded (a int)";
        // A read in a Production console's Manual mode opens no transaction: nothing to remind about
        st.set_tx_mode("p".into(), "tx".into(), true).await.unwrap();
        let r = st.run_query("p".into(), "select 1".into(), None, Some("tx".into())).await.unwrap();
        assert!(!r.in_transaction && !st.tx_state("tx".into()).await);
        assert!(st.tx_reminders_after(now).await.is_empty());
        // A change opens it; a typed BEGIN in Auto with only a read in it, and keep-alive's source, too; a Dev source's isn't reminded
        for (src, tab, sql) in [("p", "tx", change), ("d", "dev", change), ("k", "kept", change), ("p", "auto", "begin")] {
            if tab != "auto" {
                st.set_tx_mode(src.into(), tab.into(), true).await.unwrap();
            }
            st.run_query(src.into(), sql.into(), None, Some(tab.into())).await.unwrap();
        }
        st.run_query("p".into(), "select 1".into(), None, Some("auto".into())).await.unwrap();
        let mut r = st.tx_reminders_after(now).await;
        r.sort();
        assert_eq!(r.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["auto", "kept", "tx"], "{r:?}");
        let says = |tab: &str| r.iter().find(|(t, _)| t == tab).unwrap().1.clone();
        let m = says("tx");
        assert!(m.contains("\"prod\"") && m.contains("rows it changed stay locked") && m.contains("10 min idle (idle_in_transaction_session_timeout)"), "{m}");
        let read = says("auto");
        assert!(read.contains("nothing was changed") && !read.contains("rows it changed"), "{read}");
        assert!(says("kept").contains("\"kept\"") && !says("kept").contains("The server ends it"), "{}", says("kept"));
        assert!(st.tx_reminders_after(now).await.is_empty(), "once");
        // A statement starts a new stretch; Roll back ends it
        st.run_query("p".into(), "select 2".into(), None, Some("tx".into())).await.unwrap();
        assert_eq!(st.tx_reminders_after(now).await.len(), 1);
        st.rollback_tx("tx".into()).await.unwrap();
        assert!(st.tx_reminders_after(now).await.is_empty());
        // Not before the time is up
        st.run_query("p".into(), change.into(), None, Some("tx".into())).await.unwrap();
        assert!(st.tx_reminders().await.is_empty());
        // A slow session marked to be checked again on its next use (tab_session) isn't one unused since 1970
        *st.sessions.read().await.get("tx").unwrap().used.lock().unwrap() = std::time::SystemTime::UNIX_EPOCH;
        assert!(st.tx_reminders_after(now).await.is_empty());
        for t in ["tx", "dev", "kept", "auto"] {
            st.close_session(t.into()).await.unwrap();
        }
    }
}
