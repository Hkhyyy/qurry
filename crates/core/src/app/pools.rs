use super::*;

impl AppState {
    /// The data sources whose tunnel Qurry is waiting for or replacing (after the Mac woke, or the tunnel stopped answering), and
    /// what to show for each (qurry_shared::errors' words). GUI and TUI ask while a load waits on a source. Nothing is read when
    /// nothing reconnects
    pub fn reconnecting(&self) -> Vec<Reconnecting> {
        use qurry_shared::errors::{reconnect_cancelled, reconnect_failed, reconnect_slow, reconnecting};
        let ids: Vec<String> = self.reconnecting.lock().unwrap().keys().cloned().collect();
        ids.into_iter()
            .filter_map(|id| {
                let c = self.source(&id).ok()?;
                let ssm = c.active_ssm().is_some();
                Some(Reconnecting { message: reconnecting(&c.name, ssm), slow: reconnect_slow(ssm), failed: reconnect_failed(&c.name, ssm), cancelled: reconnect_cancelled(&c.name, ssm), id })
            })
            .collect()
    }

    /// Counts a wait for, or an open of, the source's tunnel as reconnecting until the guard drops (public for the apps' tests and
    /// screens, which show a source reconnecting)
    pub fn reconnecting_now(&self, base: &str) -> Running<'_> {
        count(&self.reconnecting, base)
    }
}

/// A data source whose tunnel Qurry waits for or replaces (AppState::reconnecting): what the apps say meanwhile (`message`, and
/// `slow` after RECONNECT_SLOW), the heading when the load waiting on it fails (`failed`), and what Cancel leaves (`cancelled`)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reconnecting {
    pub id: String,
    pub message: String,
    pub slow: String,
    pub failed: String,
    pub cancelled: String,
}

/// A tab's result with more rows to load (AppState::more)
#[derive(Clone)]
pub(super) struct Kept {
    pub(super) conn_id: String,
    /// The data source's settings when it ran: reopened only on the same server and database
    pub(super) cfg: ConnectionConfig,
    /// Which console run it came from (AppState::runs): a reopen that lost a race with a newer run in the tab stops
    pub(super) run: u64,
    pub(super) more: session::More,
    /// Its session had been changed (SessionEntry::changed): a new connection wouldn't have those settings
    pub(super) changed: bool,
}

/// Why a result whose session was closed isn't reopened on a new one (AppState::fetch_more): None if it may be.
/// now = the data source's settings now; manual = the tab is in Manual mode now (the new session would read it in a transaction of its own)
pub(super) fn reopen_refused(k: &Kept, now: &ConnectionConfig, manual: bool) -> Option<String> {
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
    } else if k.changed {
        "the session was changed since (SET, RESET, USE or a statement that isn't a read), so a new connection wouldn't match"
    } else {
        return None;
    };
    Some(format!("{RECONNECTED}, and {why}. Run the query again to load more rows."))
}

/// If the connection was lost: drop the tab session (so the dead connection is not reused; the next run reconnects),
/// and close the pool too if it does not respond (e.g. the SSH tunnel died). Never resend the same statement (the server may have already run it).
/// tx = how the console's open transaction ended (audit::ended_by), None if none was open
pub(super) async fn recover(state: &AppState, id: &str, tab: Option<&str>, tx: Option<&str>, e: AppError) -> AppError {
    if !is_conn_lost(&e) {
        return e;
    }
    if let Some(tab) = tab {
        state.sessions.write().await.remove(tab);
        // This error says what became of the transaction: a disconnect meanwhile needn't say it again (settle_lost)
        state.lost_tx.lock().unwrap().remove(tab);
    }
    let pool = state.pools.read().await.get(id).cloned();
    if let Some(p) = pool {
        let alive = tokio::time::timeout(std::time::Duration::from_secs(3), p.server_version()).await.is_ok_and(|r| r.is_ok());
        if !alive {
            // The Data Source's other consoles lost their connections too: the server rolled back their open transactions
            drop_pool_logged(state, split_target(id).0, crate::audit::BY_SERVER).await;
        }
    }
    crate::log!("[qurry] connection lost on {id}: {}", crate::logfile::error_kind(&e));
    let idle = idle_ended(&e);
    // Which way it went: a connect's timeout says so itself (timeout_error)
    let cfg = state.source(id).ok();
    let through = cfg.as_ref().filter(|_| !timed_out(&e)).and_then(way);
    // Through a tunnel, the driver's words ("expected to read 5 bytes, got 0 bytes at EOF") are about the local socket: the way says more
    let (e, through) = match (&e, through) {
        (AppError::Db(sqlx::Error::Io(_) | sqlx::Error::Protocol(_)), Some(w)) => (format!("The connection through {w} closed"), String::new()),
        (_, w) => (
            match &cfg {
                Some(c) => timeout_error(c, e, false).to_string(),
                None => e.to_string(),
            },
            w.map(|w| format!(" through {w}")).unwrap_or_default(),
        ),
    };
    use qurry_shared::errors::{COMMIT_UNKNOWN_ERR, TX_ROLLED_BACK, idle_tx_ended};
    AppError::Msg(match tx {
        // Before the COMMIT's own words: the server had ended the session before the COMMIT came, so nothing was committed
        Some(_) if idle.is_some() => format!("{e} — {}", idle_tx_ended(idle == Some(true))),
        Some(crate::audit::COMMIT_UNKNOWN) => format!("{e} — {COMMIT_UNKNOWN_ERR}"),
        Some(_) => format!("{e} — connection lost{through}; {TX_ROLLED_BACK}. Run it again to reconnect."),
        None => format!("{e} — connection lost{through}. Run it again to reconnect."),
    })
}

/// One data source's connects
#[derive(Default)]
pub(super) struct Connecting {
    /// Held while the shared tunnel opens, so callers arriving together start one SSH / SSM session. Then the time and message
    /// of its failure (only the tunnel's: a database's own connect error stays with its caller)
    pub(super) failed: tokio::sync::Mutex<Option<(std::time::Instant, String)>>,
    /// drop_pool counts up: a connect that started before keeps no pool or tunnel (its tunnel may be gone, its settings old)
    pub(super) generation: std::sync::atomic::AtomicU64,
    /// The generation whose tunnel was found to answer after a connect through it timed out, and when (drop_dead_tunnel): callers
    /// that timed out with it get that answer instead of checking again one after another
    answered: std::sync::Mutex<Option<(u64, std::time::Instant)>>,
    /// When the source's pools were last used or checked (settle), by the wall clock, and the wakes then (AppState::wakes)
    pub(super) used: std::sync::Mutex<Option<(std::time::SystemTime, u64)>>,
    /// Its last tunnel was closed because it stopped or stopped answering: the next one opened replaces it (AppState::reconnecting)
    dead: std::sync::atomic::AtomicBool,
    /// The generation the last close not for a dead tunnel counted up to (Disconnect, the idle drop, saved settings): a read
    /// through a pool from before it isn't run again on a new one (with_pool), which would connect the source again
    pub(super) disconnected: std::sync::atomic::AtomicU64,
    /// Held while a Redis source connects: its databases share the one connection (redis_db). Then the time and message of
    /// its failure, for the callers that waited meanwhile (redis_turn)
    redis_connect: tokio::sync::Mutex<Option<(std::time::Instant, String)>>,
}

impl Connecting {
    /// Whether a read through the pool of generation `g` may run again on a new pool: no disconnect came since
    fn may_rerun(&self, g: u64) -> bool {
        self.disconnected.load(SeqCst) <= g
    }

    /// Whether the source's pools and tunnel are checked before their next use (settle): the Mac slept since they were last used,
    /// or, through a tunnel, they sat unused for POOL_IDLE by the wall clock (sqlx's own POOL_IDLE counts on an Instant; a keep-alive
    /// that got through counts as a use: pinged). Otherwise this counts as a use
    fn stale(&self, wakes: u64, tunneled: bool) -> bool {
        let mut used = self.used.lock().unwrap();
        let stale = used.is_none_or(|(at, w)| w < wakes || (tunneled && idle_for(at, db::POOL_IDLE)));
        if !stale {
            *used = Some((std::time::SystemTime::now(), wakes));
        }
        stale
    }

    fn settled(&self, wakes: u64) {
        *self.used.lock().unwrap() = Some((std::time::SystemTime::now(), wakes));
    }

    /// The keep-alive got through the source's tunnel: a use, though not a check after a wake (its other idle connections may be
    /// from before the sleep)
    pub(super) fn pinged(&self) {
        if let Some((at, _)) = self.used.lock().unwrap().as_mut() {
            *at = std::time::SystemTime::now();
        }
    }
}

impl AppState {
    /// Created only for a source that exists (ensure_pool looks it up first) and removed when it is deleted
    pub(super) fn connecting(&self, base: &str) -> std::sync::Arc<Connecting> {
        self.connecting.lock().unwrap().entry(base.to_string()).or_default().clone()
    }
}

/// drop_pool, and close in the Production log the logged transactions of the consoles it closed (the server rolls them back): word =
/// how. A console whose transaction went with it is never left to run its next statement in a new session as if nothing happened
/// (autocommit, the rest of the transaction gone): that use says so once and sends nothing (tab_session)
pub(crate) async fn drop_pool_logged(state: &AppState, id: &str, word: &str) {
    let cfg = state.source(id).ok();
    let tabs = close_all(state, id, cfg.as_ref().map(|c| tx_lost_error(c, word)).as_deref(), word == crate::audit::BY_SERVER).await;
    let Some(cfg) = cfg else { return };
    for (t, conn) in tabs {
        if let Err(e) = state.audit.tx_closed(&conn, &cfg.name, &t, word) {
            crate::log!("[qurry] {e}");
        }
    }
}

/// Close all DB pools, sessions and tunnels of the Data Source. → the consoles whose sessions it closed (tab id, connection id)
pub async fn drop_pool(state: &AppState, id: &str) -> Vec<(String, String)> {
    close_all(state, id, None, false).await
}

/// drop_pool. lost = what a console whose session it closes with a transaction open says on its next use (LostTx): marked as the
/// session is taken out, so a statement that ends with the tunnel meanwhile and says what became of it (recover, settle_lost)
/// always comes after the mark. dead = for a tunnel that stopped or stopped answering (drop_pool_logged with BY_SERVER)
async fn close_all(state: &AppState, id: &str, lost: Option<&str>, dead: bool) -> Vec<(String, String)> {
    // No entry: no connect of it has started, so none to stop
    if let Some(c) = state.connecting.lock().unwrap().get(split_target(id).0) {
        // Before the count: a read that sees the new generation knows whether it may run again (with_pool)
        c.dead.store(dead, SeqCst);
        if !dead {
            c.disconnected.fetch_max(c.generation.load(SeqCst) + 1, SeqCst);
        }
        c.generation.fetch_add(1, SeqCst);
    }
    // The tunnel goes before the pools: a connect that saw this generation and kept its pool while it was still open (keep_pool)
    // has that pool closed below
    let mut tunnel = false;
    state.tunnels.write().await.retain(|k, _| {
        tunnel |= belongs(k, id);
        !belongs(k, id)
    });
    let mut tabs = Vec::new();
    state.sessions.write().await.retain(|t, e| {
        let gone = belongs(&e.conn_id, id);
        if gone {
            tabs.push((t.clone(), e.conn_id.clone()));
            // Busy (something runs in it): whether a transaction is open is known when that ends (settle_lost). Held by
            // housekeeping only: none is (Housekept)
            let tx = e.session.try_lock().ok().map(|s| s.in_tx()).or_else(|| e.housekept.load(SeqCst).then_some(false));
            if let Some(msg) = lost.filter(|_| tx != Some(false)) {
                state.lost_tx.lock().unwrap().insert(t.clone(), LostTx { msg: msg.to_string(), busy: tx.is_none() });
            }
        }
        !gone
    });
    let removed: Vec<Pool> = {
        let mut pools = state.pools.write().await;
        let keys: Vec<String> = pools.keys().filter(|k| belongs(k, id)).cloned().collect();
        keys.into_iter().filter_map(|k| pools.remove(&k)).collect()
    };
    // Closing waits for the connections in use: one whose statement hangs on a server that doesn't answer would hold up the
    // disconnect, and the connect lock while a dead tunnel is replaced (drop_dead_tunnel). The pools close together, within 3s in
    // all: each is closed to new uses right away, and a connection still in use is closed when it comes back. Through a tunnel,
    // its connections end with it (above)
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), futures::future::join_all(removed.iter().map(|p| p.close()))).await;
    if tunnel || !removed.is_empty() || !tabs.is_empty() {
        let why = if dead { " (it stopped answering)" } else { "" };
        crate::log!("[qurry] closed {id}{why}: {} pool(s), {} console session(s){}", removed.len(), tabs.len(), if tunnel { ", the tunnel" } else { "" });
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
    // A form's settings (Test Connection) may not be saved yet
    crate::logfile::hide(cfg);
    let tunnel = open_tunnel(cfg, ssh_secret).await?;
    let mark = tunnel.as_ref().and_then(|t| t.ssm_mark());
    let started = std::time::Instant::now();
    crate::log!("[qurry] connect {}", log_id(cfg));
    let r = db::connect_via(cfg, password, tunnel.as_ref().map(|t| t.local_port)).await;
    log_connect("connect", log_id(cfg), started, &r);
    match r {
        Ok(pool) => Ok((pool, tunnel)),
        Err(e) => Err(crate::ssm::connect_error(mark, &cfg.host, cfg.port, timeout_error(cfg, e, false)).await),
    }
}

/// The data source in a log line: its id only (a form's settings that aren't saved have none)
fn log_id(cfg: &ConnectionConfig) -> &str {
    if cfg.id.is_empty() { "(unsaved)" } else { &cfg.id }
}

/// How a connect to the database (not its tunnel) went, and how long it took
pub(super) fn log_connect<T>(what: &str, id: &str, started: std::time::Instant, r: &AppResult<T>) {
    let ms = started.elapsed().as_millis();
    match r {
        Ok(_) => crate::log!("[qurry] {what} {id} ok in {ms} ms"),
        Err(e) => crate::log!("[qurry] {what} {id} failed after {ms} ms: {e}"),
    }
}

async fn open_tunnel(cfg: &ConnectionConfig, ssh_secret: Option<&str>) -> AppResult<Option<crate::tunnel::Tunnel>> {
    let started = std::time::Instant::now();
    let r = match (cfg.active_ssh(), cfg.active_ssm()) {
        (Some(_), Some(_)) => return Err(AppError::Msg("Both an SSH and an AWS SSM tunnel are turned on — choose one (GUI: SSH/SSL tab · TUI: Tunnel).".into())),
        (Some(ssh), None) => crate::tunnel::open(ssh, ssh_secret, &cfg.host, cfg.port).await,
        (None, Some(ssm)) => crate::ssm::open(ssm, &cfg.host, cfg.port).await,
        (None, None) => return Ok(None),
    };
    // Not `way`: it names the tunnel's host or instance
    let (ms, kind) = (started.elapsed().as_millis(), if cfg.active_ssm().is_some() { "AWS SSM tunnel" } else { "SSH tunnel" });
    match &r {
        Ok(t) => crate::log!("[qurry] {kind} of {} open in {ms} ms on local port {}", log_id(cfg), t.local_port),
        Err(e) => crate::log!("[qurry] {kind} of {} failed after {ms} ms: {e}", log_id(cfg)),
    }
    r.map(Some)
}

/// The data source's tunnel: None without one, Some(false) once it has stopped (the aws CLI exited: idle timeout, network, session ended)
pub(super) async fn tunnel_alive(state: &AppState, base: &str) -> Option<bool> {
    state.tunnels.read().await.get(base).map(|t| t.alive())
}

/// Closing the data source's connections would lose work: a console of it is running a statement (its session is locked) or has a
/// transaction open, or an export, import, backup, restore or subset copy runs on it (AppState::job_running). A session only
/// housekeeping holds runs nothing of the user's (Housekept)
pub(super) async fn in_use(state: &AppState, base: &str) -> bool {
    state.running.lock().unwrap().get(base).is_some_and(|n| *n > 0)
        || state.sessions.read().await.values().any(|e| belongs(&e.conn_id, base) && e.session.try_lock().map(|g| g.in_tx()).unwrap_or_else(|_| !e.housekept.load(SeqCst)))
}

/// Something on a data source, counted while it runs: a long job (AppState::job_running), a wait for or an open of its tunnel
/// (AppState::reconnecting)
pub struct Running<'a>(&'a std::sync::Mutex<HashMap<String, usize>>, String);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        let mut all = self.0.lock().unwrap();
        if let Some(n) = all.get_mut(&self.1) {
            *n -= 1;
            if *n == 0 {
                all.remove(&self.1);
            }
        }
    }
}

impl AppState {
    /// Counts a long job (export, import, backup, restore, subset copy) on the source of `id` until the guard drops: while one runs,
    /// the source isn't disconnected when idle, and its tunnel gets the longer check before it is taken for dead (drop_dead_tunnel)
    pub(crate) fn job_running(&self, id: &str) -> Running<'_> {
        count(&self.running, split_target(id).0)
    }
}

/// Counts one more on `base` until the guard drops (Running)
fn count<'a>(all: &'a std::sync::Mutex<HashMap<String, usize>>, base: &str) -> Running<'a> {
    *all.lock().unwrap().entry(base.to_string()).or_default() += 1;
    Running(all, base.to_string())
}

/// The local port of the data source's open tunnel
pub async fn tunnel_port(state: &AppState, id: &str) -> Option<u16> {
    state.tunnels.read().await.get(split_target(id).0).map(|t| t.local_port)
}

/// Why housekeeping disconnected a data source, as GUI and TUI say it (qurry_shared::errors::disconnected)
pub(super) fn disconnected(cfg: &ConnectionConfig, why: qurry_shared::errors::Disconnected) -> String {
    let tunnel = if cfg.active_ssm().is_some() { "AWS SSM tunnel" } else { "SSH tunnel" };
    qurry_shared::errors::disconnected(&cfg.name, tunnel, why)
}

pub fn touch(state: &AppState, id: &str) {
    let base = split_target(id).0.to_string();
    state.last_used.lock().unwrap().insert(base, std::time::Instant::now());
}

/// The pool of a data source's database: the open one, or a new connect (through the source's tunnel when it has one). A connect
/// through the tunnel that gets no answer opens the tunnel again and tries once more (open_pool)
pub async fn ensure_pool(state: &AppState, id: &str) -> AppResult<Pool> {
    open_pool(state, id, None).await
}

/// ensure_pool. A connect through the tunnel that times out may have gone through a session that stopped forwarding while the
/// tunnel's process runs on (sleep, a network change, the session ended on the AWS side or by the SSH server): the tunnel is
/// opened again (shared_tunnel) for one more try, whose timeout is the error. stale = the generation whose tunnel timed out
/// already (with_pool): no more tries then
async fn open_pool(state: &AppState, id: &str, mut stale: Option<u64>) -> AppResult<Pool> {
    // MCP server: never connect to hidden Data Sources (sources without a password · pools already open too)
    if state.mcp
        && let Some(c) = state.store.get(split_target(id).0)?.filter(|c| c.mcp_access(false) == store::McpAccess::Off)
    {
        return Err(AppError::Msg(format!("Data source \"{}\" is not shared with MCP", c.name)));
    }
    touch(state, id);
    let base = split_target(id).0;
    // After the Mac woke or a long idle: the tunnel and the pools' idle connections are checked first. It may close them. Whenever
    // the source has anything open, not only this database's pool: a database opened first after a wake goes through the same tunnel
    let open = state.pools.read().await.keys().any(|k| belongs(k, base));
    if open || state.tunnels.read().await.contains_key(base) {
        settle(state, base).await;
    }
    let pool = state.pools.read().await.get(id).cloned();
    let none_open = pool.is_none();
    // Not the pool that just timed out (it is closed when its tunnel is opened again)
    let timed_out_here = stale.is_some() && state.connecting.lock().unwrap().get(base).map(|c| c.generation.load(SeqCst)) == stale;
    if let Some(p) = pool
        && !timed_out_here
        && tunnel_alive(state, base).await != Some(false)
    {
        return Ok(p);
    }
    let redis = state.source(id)?.kind == store::DbKind::Redis;
    if redis && split_target(id).1.is_some() {
        return redis_db(state, id, stale).await;
    }
    let c = state.connecting(base);
    let mut one = match redis {
        true => match redis_turn(state, &c, id, none_open).await? {
            Ok(turn) => Some(turn),
            Err(pool) => return Ok(pool),
        },
        false => None,
    };
    let cfg = target(state, id)?;
    let asked = std::time::SystemTime::now();
    let mut password = state.login_password(&cfg).await?;
    let ssh_secret = if cfg.active_ssh().is_some() { state.saved_secret(&store::ssh_secret_key(base), cfg.ssh_secret_saved)? } else { None };
    loop {
        let (port, g) = if cfg.tunneled() {
            let (port, g) = shared_tunnel(state, &c, base, &cfg, ssh_secret.as_deref(), stale).await?;
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
        let started = std::time::Instant::now();
        crate::log!("[qurry] pool open {id}{}", port.map(|p| format!(" through local port {p}")).unwrap_or_default());
        let r = db::connect_via(&cfg, password.as_deref(), port).await;
        log_connect("pool open", id, started, &r);
        match r {
            Ok(pool) => return keep_pool(state, &c, g, id, port, pool).await,
            Err(e) if port.is_some() && stale.is_none() && timed_out(&e) => stale = Some(g),
            // A password command's output kept from an earlier login (Password reuse) that the server no longer takes: the token
            // expired while no pool was open (Disconnect, Auto-disconnect, a wake). It runs again and this connects once more
            Err(e) if login_refused(&e) && state.command_password_older(&cfg, asked) => password = Some(state.command_password(&cfg, true).await?),
            Err(e) => {
                let e = crate::ssm::connect_error(mark, &cfg.host, cfg.port, timeout_error(&cfg, e, stale.is_some())).await;
                if let Some(failed) = one.as_deref_mut() {
                    *failed = Some((std::time::Instant::now(), e.to_string()));
                }
                return Err(e);
            }
        }
    }
}

/// The turn of a Redis source's connect, held until it ends; then the time and message of its failure (Connecting::redis_connect)
type RedisTurn<'a> = tokio::sync::MutexGuard<'a, Option<(std::time::Instant, String)>>;

/// Redis: callers that come together (the browser, the terminal, MCP, each on a database of the source) share one connect. →
/// the turn to connect, or the pool another caller opened while this one waited (none_open: there was none before). One that
/// waited while the connect failed gets its error, instead of trying again after it
async fn redis_turn<'a>(state: &AppState, c: &'a Connecting, id: &str, none_open: bool) -> AppResult<Result<RedisTurn<'a>, Pool>> {
    let waited = std::time::Instant::now();
    let turn = c.redis_connect.lock().await;
    if let Some((at, e)) = turn.as_ref()
        && *at >= waited
    {
        return Err(AppError::Msg(e.clone()));
    }
    match state.pools.read().await.get(id).cloned().filter(|_| none_open) {
        Some(p) => Ok(Err(p)),
        None => Ok(Ok(turn)),
    }
}

/// A database of a Redis source: the source's one connection (open_pool), pinned to the database (RedisConn::on_db). SSH / SSM
/// tunnels carry badly many connections opened at once (#221), and a connection per database made the browser, the terminal
/// and MCP open one each
async fn redis_db(state: &AppState, id: &str, stale: Option<u64>) -> AppResult<Pool> {
    let (base, db) = split_target(id);
    let Pool::Redis(m, _) = Box::pin(open_pool(state, base, stale)).await? else {
        return Err(AppError::Msg("Not a Redis data source".into()));
    };
    // As for a connect: a disconnect from here on keeps no pool (keep_pool)
    let c = state.connecting(base);
    let g = c.generation.load(SeqCst);
    let port = tunnel_port(state, base).await;
    if !state.pools.read().await.contains_key(base) {
        return Err(AppError::Msg(DISCONNECTED_WHILE_CONNECTING.into()));
    }
    let n = crate::redisdb::db_index(db.unwrap_or_default());
    let pool = Pool::Redis(m.on_db(n).await?, n);
    keep_pool(state, &c, g, id, port, pool).await
}

/// The settings a connect to `id` (a source, or one of its databases) uses
fn target(state: &AppState, id: &str) -> AppResult<ConnectionConfig> {
    let mut cfg = state.source(id)?;
    // For file DBs (SQLite, DuckDB) database is the file path: do not overwrite it with the DB name (main)
    if let Some(db) = split_target(id).1.filter(|_| !cfg.kind.is_file()) {
        cfg.database = db.to_string();
    }
    Ok(cfg)
}

/// Runs `f` on the pool of `id` (ensure_pool). When a pool that was already open times out getting a connection through the
/// source's tunnel (nothing reached the server), the tunnel may have stopped forwarding while its process runs on: it is opened
/// again and `f` runs once more on a new connect. So it does, once, when the pool is closed under it because its tunnel was found
/// dead (closed_under) or its tunnel stops while it runs (unless_stopped), but not after a disconnect (Disconnect, the idle drop):
/// that would connect the source again. Only for an `f` that may run again: a read, or opening a connection
pub(crate) async fn with_pool<T, Fut: std::future::Future<Output = AppResult<T>>>(state: &AppState, id: &str, f: impl Fn(Pool) -> Fut) -> AppResult<T> {
    with_pool_after(state, id, None, true, f).await
}

/// with_pool. stale = the generation whose dead tunnel this call closed already (tab_session): opening a new one was its one
/// retry, so `f` runs once and its timeout is the error. read = `f` may run again after it reached the server; otherwise (a
/// statement that may change data) only a connect that timed out, when nothing reached the server, is tried again
pub(super) async fn with_pool_after<T, Fut: std::future::Future<Output = AppResult<T>>>(state: &AppState, id: &str, stale: Option<u64>, read: bool, f: impl Fn(Pool) -> Fut) -> AppResult<T> {
    if let Some(g) = stale {
        let cfg = state.source(id)?;
        return f(open_pool(state, id, Some(g)).await?).await.map_err(|e| timeout_error(&cfg, e, true));
    }
    let base = split_target(id).0;
    // The generation the open pool belongs to (drop_pool closes the pools when it counts up)
    let open = state.pools.read().await.contains_key(id);
    let open = open.then(|| state.connecting.lock().unwrap().get(base).map(|c| c.generation.load(SeqCst))).flatten();
    let again = async || {
        let cfg = state.source(id)?;
        f(ensure_pool(state, id).await?).await.map_err(|e| timeout_error(&cfg, e, true))
    };
    // Read before the pool: a close that comes while ensure_pool hands it out counts the generation past this, so it is noticed
    // (closed_under, unless_stopped) and the read runs once more. The source exists for Connecting (AppState::connecting)
    let c = if read {
        state.source(id)?;
        let c = state.connecting(base);
        let g = c.generation.load(SeqCst);
        Some((c, g))
    } else {
        None
    };
    let pool = ensure_pool(state, id).await?;
    let r = match c {
        Some((c, g)) => {
            // The tunnel is watched from the generation of the pool in hand: ensure_pool may have closed a dead tunnel and opened
            // this pool on a new one (settle, shared_tunnel), which is no close under `f`. A close before, of a pool it handed out
            // anyway, makes `f` fail on the closed pool (closed_under, from the generation read before)
            let now = c.generation.load(SeqCst);
            match unless_stopped(state, &c, base, now, f(pool)).await {
                None => return again().await,
                Some(Err(e)) if closed_under(&e, &c, g, tunnel_alive(state, base).await == Some(false)) => return again().await,
                Some(r) => r,
            }
        }
        None => f(pool).await,
    };
    let e = match r {
        Err(e) if timed_out(&e) => e,
        Err(e) if login_refused(&e) => return refill(state, id, e, &f).await,
        r => return r,
    };
    let cfg = state.source(id)?;
    match open.filter(|_| cfg.tunneled()) {
        Some(g) => f(open_pool(state, id, Some(g)).await?).await.map_err(|e| timeout_error(&cfg, e, true)),
        None => Err(timeout_error(&cfg, e, false)),
    }
}

/// The server refused a new connection's login: PostgreSQL SQLSTATE class 28, MySQL 1045. Nothing ran on it
fn login_refused(e: &AppError) -> bool {
    match crate::mysql::error_number(e) {
        Some(n) => n == 1045,
        None => matches!(e, AppError::Db(sqlx::Error::Database(d)) if d.code().is_some_and(|c| c.starts_with("28"))),
    }
}

/// A use of a PostgreSQL / MySQL pool failed because the server refused a new connection's login (with_pool_after): with a
/// password command, the token the pool opened with may have expired (an RDS IAM token lasts 15 minutes), and sqlx connects with
/// the options the pool holds (no hook asks for a new password). The command runs again, the pool takes its output and `f` runs
/// once more: the refused login ran no statement, so this is safe for writes too. Without a command it is the error
async fn refill<T, Fut: std::future::Future<Output = AppResult<T>>>(state: &AppState, id: &str, e: AppError, f: impl Fn(Pool) -> Fut) -> AppResult<T> {
    let cfg = target(state, id)?;
    let Some(pool) = state.pools.read().await.get(id).cloned().filter(|_| !cfg.password_command.trim().is_empty()) else { return Err(e) };
    let password = state.command_password(&cfg, true).await?;
    match &pool {
        Pool::Pg(p) => p.set_connect_options((*p.connect_options()).clone().password(&password)),
        Pool::My(p) => p.set_connect_options((*p.connect_options()).clone().password(&password)),
        _ => return Err(e),
    }
    crate::log!("[qurry] pool of {id} took a new password from its command");
    f(pool).await
}

/// ensure_pool, for work that changes data or runs long: through a tunnel, the pool answers a round trip first (with_pool), so a
/// tunnel that stopped forwarding is replaced before anything runs. So it does with a password command, so a login the server
/// refuses (an expired token) gets a new password first (refill). Other errors of that round trip are left to the work itself
pub async fn checked_pool(state: &AppState, id: &str) -> AppResult<Pool> {
    let cfg = state.source(id)?;
    if !cfg.tunneled() && cfg.password_command.trim().is_empty() {
        return ensure_pool(state, id).await;
    }
    with_pool(state, id, |p| async move {
        match p.server_version().await {
            Err(e) if timed_out(&e) || login_refused(&e) => Err(e),
            _ => Ok(p),
        }
    })
    .await
}

/// A connect that got no answer within the pools' 10s (acquire_timeout): it didn't get as far as a statement
fn timed_out(e: &AppError) -> bool {
    matches!(e, AppError::Db(sqlx::Error::PoolTimedOut))
}

/// A read failed because its pool was closed under it (with_pool): it waited for a connection or used one when another caller
/// found the tunnel dead and closed the source, which counted the generation up from `before`, or the tunnel it went through
/// stopped (`stopped`: the SSH server ended the session, the aws CLI exited). The pool then says PoolClosed, a connection through
/// the closed tunnel an I/O error (connection reset). Not after a disconnect (Connecting::may_rerun)
pub(super) fn closed_under(e: &AppError, c: &Connecting, before: u64, stopped: bool) -> bool {
    matches!(e, AppError::Db(sqlx::Error::PoolClosed | sqlx::Error::Io(_))) && c.may_rerun(before) && (c.generation.load(SeqCst) != before || stopped)
}

/// How often a read through a tunnel looks whether the tunnel stopped meanwhile (unless_stopped)
const TUNNEL_WATCH: std::time::Duration = std::time::Duration::from_millis(200);

/// Runs `work`, a read on the source's pool of generation `g`, unless the source's tunnel stops first (its process exited) or
/// another caller finds the tunnel dead and closes the source: None then. Through a tunnel whose port nobody holds any more, sqlx
/// would try to connect again and again until its 10s acquire_timeout (it takes ConnectionRefused for a server starting up).
/// After a disconnect it runs on and ends with what the closed pool says. Without a tunnel it just runs
async fn unless_stopped<T>(state: &AppState, c: &Connecting, base: &str, g: u64, work: impl std::future::Future<Output = T>) -> Option<T> {
    if tunnel_alive(state, base).await != Some(true) {
        return Some(work.await);
    }
    let stopped = async {
        loop {
            tokio::time::sleep(TUNNEL_WATCH).await;
            if c.may_rerun(g) && (c.generation.load(SeqCst) != g || tunnel_alive(state, base).await == Some(false)) {
                break;
            }
        }
    };
    tokio::select! {
        biased;
        r = work => Some(r),
        () = stopped => None,
    }
}

/// How long a tunnel whose process runs gets to carry a new connection again before its first use after the Mac woke or a long
/// idle (settle). A judgement, not a measured resume time: after a wake the network (Wi-Fi rejoining, DHCP) and the SSM
/// plugin's or SSH session's connection usually come back within a few seconds, and nothing here measured a real aws CLI, plugin
/// or SSH session resuming. The fake aws (crates/core/tests/fake_aws.py) only shows the mechanism: a CLI and plugin the test
/// stops for 3s answer one round trip after it resumes them. A tunnel that is really dead costs this plus a new tunnel, instead
/// of the 10s acquire_timeout and a check (about 12s)
pub(crate) const TUNNEL_GRACE: std::time::Duration = std::time::Duration::from_secs(4);

/// How long a tunnel check waits while closing the tunnel would lose work (in_use): a busy tunnel is slower to carry a new connection
const BUSY_CHECK: std::time::Duration = std::time::Duration::from_secs(5);

/// Before the first use of a source's pools after the Mac woke, or after they sat unused through a tunnel for POOL_IDLE by the
/// wall clock (Connecting::stale): its tunnel may carry nothing while everything looks open (the aws CLI or the SSH accept loop runs on), and
/// sqlx would hand out idle connections from before the sleep (its POOL_IDLE counts on an Instant), waiting out the 10s
/// acquire_timeout on a dead one. So the tunnel is checked first: one whose process stopped is closed at once, and one that runs
/// gets TUNNEL_GRACE to carry a new connection (resumes) and is closed if it doesn't. A tunnel that answers is kept, and the
/// pools' idle connections are closed (Pool::drop_idle) so the next use opens new ones. Under the connect lock: callers that
/// arrive together share one check, and a connect waits for it. Every source is checked at once on a wake (housekeeping). → why
/// the source was closed, if it was (the next use opens a new tunnel)
pub(super) async fn settle(state: &AppState, base: &str) -> Option<qurry_shared::errors::Disconnected> {
    use qurry_shared::errors::Disconnected;
    let c = state.connecting(base);
    let tunneled = state.tunnels.read().await.contains_key(base);
    if !c.stale(state.wakes(), tunneled) {
        return None;
    }
    let _connecting = c.failed.lock().await;
    // Checked meanwhile by a caller that waited for the same lock
    let wakes = state.wakes();
    if !c.stale(wakes, tunneled) {
        return None;
    }
    let g = c.generation.load(SeqCst);
    let cfg = state.source(base).ok()?;
    let why = match tunnel_alive(state, base).await {
        None => None,
        Some(true) if resumes(state, base, &cfg).await => None,
        // Its process exited meanwhile, or before
        _ if tunnel_alive(state, base).await == Some(false) => Some(Disconnected::TunnelStopped),
        _ => Some(Disconnected::TunnelSilent),
    };
    c.settled(wakes);
    match why {
        // Unless a disconnect closed it meanwhile
        Some(w) if c.generation.load(SeqCst) == g => {
            let what = if w == Disconnected::TunnelStopped { "has stopped".to_string() } else { format!("didn't carry a new connection within {}s", TUNNEL_GRACE.as_secs()) };
            crate::log!("[qurry] the tunnel of {base} {what} after the Mac woke or sat unused; closing it (the next connect opens a new one)");
            drop_pool_logged(state, base, crate::audit::BY_SERVER).await;
            why
        }
        Some(_) => None,
        None => {
            for (_, p) in state.pools.read().await.iter().filter(|(k, _)| belongs(k, base)) {
                p.drop_idle();
            }
            None
        }
    }
}

/// How long the first try of a tunnel check gets before the apps say the source reconnects (resumes): a tunnel that works answers
/// well within it, so a healthy one never shows
const FIRST_PROBE: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether the source's running tunnel carries a new connection to the database (tunnel::answers) within TUNNEL_GRACE (BUSY_CHECK
/// when closing it would lose work), trying again while it refuses: the plugin may be resuming. False at once when its process stops.
/// Counted as reconnecting (AppState::reconnecting) only once the first try got nothing within FIRST_PROBE (it goes on meanwhile)
async fn resumes(state: &AppState, base: &str, cfg: &ConnectionConfig) -> bool {
    let within = if in_use(state, base).await { BUSY_CHECK } else { TUNNEL_GRACE };
    let end = std::time::Instant::now() + within;
    let mut shown = None;
    loop {
        let Some(port) = state.tunnels.read().await.get(base).filter(|t| t.alive()).map(|t| t.local_port) else { return false };
        let left = end.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return false;
        }
        let probe = crate::tunnel::answers(port, cfg, left);
        tokio::pin!(probe);
        // The first try runs on past FIRST_PROBE (a slow tunnel's connection isn't thrown away); from then on it shows
        let ok = match shown {
            Some(_) => probe.await,
            None => tokio::select! {
                ok = &mut probe => ok,
                () = tokio::time::sleep(left.min(FIRST_PROBE)) => {
                    shown = Some(state.reconnecting_now(base));
                    probe.await
                }
            },
        };
        if ok {
            return true;
        }
        shown.get_or_insert_with(|| state.reconnecting_now(base));
        tokio::time::sleep(left.min(std::time::Duration::from_millis(200))).await;
    }
}

/// A connect's timeout, saying which way it went: through the AWS SSM tunnel (its instance), the SSH tunnel (its server) or
/// straight to the server (sqlx only says "pool timed out while waiting for an open connection"). reopened = through a new tunnel too
pub(super) fn timeout_error(cfg: &ConnectionConfig, e: AppError, reopened: bool) -> AppError {
    if !timed_out(&e) {
        return e;
    }
    let (host, port) = (&cfg.host, cfg.port);
    let again = if reopened { ", also after Qurry opened the tunnel again" } else { "" };
    let reach = match cfg.active_ssm() {
        Some(_) => "the instance can reach it (its security group; Host as seen from the instance)",
        None => "the SSH server can reach it",
    };
    AppError::Msg(match way(cfg) {
        Some(w) => format!("Can't connect to {host}:{port} through {w} — the database didn't answer within 10s{again}. Check that it's up and that {reach}, then run it again."),
        None => format!("Can't connect to {host}:{port} — the server didn't answer within 10s. Check that it's up and that this machine can reach it (VPN, firewall), then run it again."),
    })
}

/// The way a data source's connections go: through the AWS SSM tunnel (its instance) or the SSH tunnel (its server). None = straight
pub(super) fn way(cfg: &ConnectionConfig) -> Option<String> {
    match (cfg.active_ssm(), cfg.active_ssh()) {
        (Some(s), _) => Some(format!("the AWS SSM tunnel (instance {})", s.target.trim())),
        (None, Some(s)) => Some(format!("the SSH tunnel ({}:{})", s.host, s.port)),
        (None, None) => None,
    }
}

/// A console's session through a tunnel didn't answer before a run (tab_session) while a transaction was open in it. closed =
/// the tunnel carried nothing either and was closed too
pub(super) fn lost_session_error(cfg: &ConnectionConfig, closed: bool) -> AppError {
    let w = way(cfg).unwrap_or_else(|| "the tunnel".into());
    let what = if closed { "nor did a new connection through the tunnel, so Qurry closed both (the next run opens a new tunnel)" } else { "so Qurry closed it" };
    AppError::Msg(format!("The console's connection through {w} sat unused and didn't answer when Qurry checked it before this ran, {what}. {}", qurry_shared::errors::TX_LOST_NOTHING_SENT))
}

/// A console's session through a tunnel didn't answer its check in time with a transaction open, while the tunnel carries a new
/// connection (tab_session): the session and its transaction are kept
pub(super) fn slow_session_error(cfg: &ConnectionConfig) -> AppError {
    let w = way(cfg).unwrap_or_else(|| "the tunnel".into());
    AppError::Msg(format!(
        "The console's connection through {w} sat unused and didn't answer within {}s when Qurry checked it before this ran, though the tunnel answers, so Qurry kept it with the transaction open in it. Nothing was sent. Run it again (Qurry checks the session again first), or Roll back.",
        SESSION_PING.as_secs()
    ))
}

/// A connect through a tunnel that works timed out (shared_tunnel): the tunnel is kept, and the error says what else holds it up
fn busy_error(cfg: &ConnectionConfig) -> AppError {
    let way = way(cfg).unwrap_or_else(|| "the tunnel".into());
    AppError::Msg(format!(
        "Can't get a connection to {}:{} within 10s, though {way} answers: all {} connections of this data source may be in use (open consoles and tables, exports, imports), or the database is slow to accept new ones, or its startup script (Options) fails or runs long. Close the tabs you don't need or check the startup script, then run it again.",
        cfg.host,
        cfg.port,
        db::POOL_MAX
    ))
}

/// Under the connect lock (Connecting::failed): a connect or a console's session through the tunnel of generation `g` got no
/// answer. When a new connection through the tunnel doesn't get the database's first byte either (tunnel::answers), the tunnel
/// is dead: it, the source's pools and its sessions are closed (drop_pool; a statement still waiting through it ends), so the
/// next connect opens a new one. A tunnel that answers is busy, not dead (all the pool's connections in use, a database slow to
/// accept, a startup script that fails or runs long), and closing it would end what runs through it (an import, a restore, a
/// console's transaction). So it takes two signs: the database itself didn't answer first, and then the check gets nothing
/// within 2s, or 5s while closing would lose work (in_use: a console's transaction or statement, an export, import, backup …). Callers that waited with the
/// same generation share the outcome. → Err(busy_error) when the tunnel answers
pub(super) async fn drop_dead_tunnel(state: &AppState, c: &Connecting, base: &str, cfg: &ConnectionConfig, g: u64, asked: std::time::Instant) -> AppResult<()> {
    // Closed already: by a caller that timed out with this one, or a disconnect
    if c.generation.load(SeqCst) != g {
        return Ok(());
    }
    if c.answered.lock().unwrap().is_some_and(|(at_g, at)| at_g == g && at >= asked) {
        return Err(busy_error(cfg));
    }
    let port = state.tunnels.read().await.get(base).map(|t| t.local_port);
    let within = if in_use(state, base).await { BUSY_CHECK } else { std::time::Duration::from_secs(2) };
    if let Some(port) = port
        && crate::tunnel::answers(port, cfg, within).await
    {
        *c.answered.lock().unwrap() = Some((g, std::time::Instant::now()));
        return Err(busy_error(cfg));
    }
    crate::log!("[qurry] the tunnel of {base} carries nothing any more; closing it (the next connect opens a new one)");
    drop_pool_logged(state, base, crate::audit::BY_SERVER).await;
    Ok(())
}

/// The local port of the source's tunnel, and the generation it belongs to. The first caller opens it: one SSH / SSM session
/// per source, not per database. Callers that waited while it failed get its error instead of trying again one after
/// another (each try can take the whole timeout). stale = a connect through the tunnel of that generation timed out (open_pool):
/// when the tunnel doesn't carry a new connection either (tunnel::answers), it and the pools through it are closed and a new one
/// opened, unless another caller that timed out did so already
async fn shared_tunnel(state: &AppState, c: &Connecting, base: &str, cfg: &ConnectionConfig, ssh_secret: Option<&str>, stale: Option<u64>) -> AppResult<(u16, u64)> {
    let asked = std::time::Instant::now();
    let mut failed = c.failed.lock().await;
    if let Some(g) = stale {
        drop_dead_tunnel(state, c, base, cfg, g, asked).await?;
    }
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
    // In place of one that stopped or stopped answering: the apps say it reconnects
    let shown = c.dead.load(SeqCst).then(|| state.reconnecting_now(base));
    let t = match open_tunnel(cfg, ssh_secret).await.and_then(|t| t.ok_or_else(|| AppError::Msg("No tunnel is turned on".into()))) {
        Ok(t) => t,
        Err(e) => {
            // The apps said it reconnects: it couldn't (they tell this from the load's own errors by these words)
            let e = match shown {
                Some(_) => AppError::Msg(qurry_shared::errors::reconnect_failed_error(&cfg.name, cfg.active_ssm().is_some(), &e.to_string())),
                None => e,
            };
            *failed = Some((std::time::Instant::now(), e.to_string()));
            return Err(e);
        }
    };
    if shown.is_some() {
        crate::log!("[qurry] the tunnel of {base} reconnected in {} ms", asked.elapsed().as_millis());
    }
    drop(shown);
    c.dead.store(false, SeqCst);
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

/// A run or load-more that waited for its tab's session while Qurry gave the session up (Session::abandoned: housekeeping's close
/// of its idle cursor got no answer). The tab's next use opens a new session
pub(super) const SESSION_ABANDONED: &str =
    "Nothing was sent: this tab's connection didn't answer while this waited for it (after sleep or a network change), so Qurry closed it. Try again: Qurry opens a new one.";

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
            // Fresh: no check before its next use (settle)
            c.settled(state.wakes());
            return Ok(pool);
        }
    }
    pool.close().await;
    Err(AppError::Msg(DISCONNECTED_WHILE_CONNECTING.into()))
}

pub async fn redis_pool(state: &AppState, id: &str) -> AppResult<crate::redisdb::RedisConn> {
    match ensure_pool(state, id).await? {
        Pool::Redis(m, _) => Ok(m),
        _ => Err(AppError::Msg("Not a Redis data source".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::*;

    /// The tree, autocomplete and restored tabs open a source and its databases together. When the server doesn't answer they
    /// all fail after one timeout, not one after another (4 callers used to take 4 × 10s)
    #[tokio::test]
    async fn callers_of_an_unreachable_source_wait_one_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        st.store.save(&silent_source("x", silent_server().await, None)).unwrap();
        let start = std::time::Instant::now();
        let r = futures::future::join_all(["x", "x::db1", "x", "x::db3"].map(|id| ensure_pool(&st, id))).await;
        let direct = |e: &str| e.contains("Can't connect to 127.0.0.1:") && e.contains("the server didn't answer within 10s") && e.contains("this machine can reach it") && !e.contains("tunnel");
        assert!(r.iter().all(|r| r.as_ref().err().is_some_and(|e| direct(&e.to_string()))), "{:?}", r.iter().map(|r| r.as_ref().err().map(|e| e.to_string())).collect::<Vec<_>>());
        assert!(start.elapsed() < std::time::Duration::from_secs(15), "{:?}", start.elapsed());
    }

    /// A source is checked before its next use after a wake, and through a tunnel after POOL_IDLE unused by the wall clock, but
    /// not when a keep-alive got through meanwhile; a source without a tunnel only after a wake (sqlx's POOL_IDLE does the rest)
    #[test]
    fn a_source_is_checked_after_a_wake_or_a_long_idle_through_a_tunnel() {
        let c = Connecting::default();
        assert!(c.stale(0, false), "never used");
        c.settled(0);
        assert!(!c.stale(0, true));
        assert!(c.stale(1, false), "the Mac slept");
        c.settled(1);
        let idle = |c: &Connecting| c.used.lock().unwrap().as_mut().unwrap().0 -= db::POOL_IDLE;
        idle(&c);
        assert!(!c.stale(1, false), "no tunnel");
        idle(&c);
        assert!(c.stale(1, true), "through a tunnel");
        c.settled(1);
        idle(&c);
        c.pinged();
        assert!(!c.stale(1, true), "the keep-alive got through");
        assert!(c.stale(2, true), "but a wake still is checked");
    }

    /// A wake is the wall clock running ahead of an Instant between two reads by WAKE_GAP or more; a clock set back is none
    #[test]
    fn a_wake_is_the_wall_clock_running_ahead_of_an_instant() {
        let s = std::time::Duration::from_secs;
        assert!(slept(Some(s(70)), s(5)), "a minute's sleep between two beats");
        assert!(slept(Some(s(8 * 3600 + 5)), s(5)), "overnight");
        assert!(!slept(Some(s(5)), s(5)), "awake");
        assert!(!slept(Some(s(14)), s(5)), "under WAKE_GAP: a nap the tunnels outlive");
        assert!(!slept(None, s(5)), "the clock was set back");
        assert!(!slept(Some(s(1)), s(30)), "the clock was set back by less than the time between the reads");
    }

    /// AppState::wakes counts each sleep once, by the reads around it; a clock set back counts none
    #[test]
    fn wakes_counts_each_sleep_once() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let w = st.wakes();
        assert_eq!(st.wakes(), w);
        sleep_mac(&st, std::time::Duration::from_secs(60));
        assert_eq!(st.wakes(), w + 1);
        assert_eq!(st.wakes(), w + 1, "once");
        st.clock.lock().unwrap().wall += std::time::Duration::from_secs(3600);
        assert_eq!(st.wakes(), w + 1, "set back an hour");
    }

    /// While Qurry waits for or replaces a source's tunnel, reconnecting() lists it with qurry_shared::errors::reconnecting's
    /// words for its kind of tunnel; nothing once that ends
    #[tokio::test]
    async fn reconnecting_says_which_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut ssm = silent_source("s", 1, None);
        ssm.name = "prod".into();
        ssm.ssm = Some(crate::store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: "i-0123".into() });
        st.store.save(&ssm).unwrap();
        let mut ssh = silent_source("h", 1, Some(serde_json::json!({ "enabled": true, "host": "bastion", "port": 22, "user": "me", "auth": "password", "keyPath": "" })));
        ssh.name = "stage".into();
        st.store.save(&ssh).unwrap();
        assert!(st.reconnecting().is_empty());
        let a = st.reconnecting_now("s");
        let b = st.reconnecting_now("h");
        let both = st.reconnecting_now("h");
        let mut now: Vec<(String, String)> = st.reconnecting().into_iter().map(|r| (r.id, r.message)).collect();
        now.sort();
        assert_eq!(now, vec![("h".to_string(), qurry_shared::errors::reconnecting("stage", false)), ("s".to_string(), qurry_shared::errors::reconnecting("prod", true))]);
        drop((a, b));
        let left = st.reconnecting();
        assert_eq!(left.iter().map(|r| (r.id.as_str(), r.message.as_str())).collect::<Vec<_>>(), vec![("h", "Reconnecting stage through SSH")], "one still under way");
        assert_eq!((left[0].failed.as_str(), left[0].slow.contains("SSH session")), ("Couldn't reconnect stage through SSH", true));
        drop(both);
        assert!(st.reconnecting().is_empty() && st.reconnecting.lock().unwrap().is_empty());
    }

    /// `work` and whether reconnecting() listed anything while it ran (looked at every 20 ms)
    async fn shown_while<T>(st: &AppState, work: impl std::future::Future<Output = T>) -> (T, bool) {
        let done = std::sync::atomic::AtomicBool::new(false);
        let (r, seen) = tokio::join!(
            async {
                let r = work.await;
                done.store(true, SeqCst);
                r
            },
            async {
                let mut seen = false;
                while !done.load(SeqCst) {
                    seen |= !st.reconnecting().is_empty();
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                seen
            }
        );
        (r, seen)
    }

    /// The check of a tunnel after a wake (settle) says the source reconnects only once its first try got no answer: a tunnel that
    /// answers at once never shows, one that doesn't does until it is closed. The keep-alive's check (drop_dead_tunnel) never shows
    #[cfg(unix)]
    #[tokio::test]
    async fn only_a_tunnel_that_doesnt_answer_shows_as_reconnecting() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut c = silent_source("r", 5432, None);
        c.ssm = Some(store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: "i-0123456789abcdef0".into() });
        st.store.save(&c).unwrap();
        let open = async |port: u16| {
            let (fake, t) = crate::ssm::tests::fake_tunnel_to(port).await;
            st.tunnels.write().await.insert("r".into(), t);
            sleep_mac(&st, std::time::Duration::from_secs(3600));
            fake
        };
        let fake = open(slow_pg_server().await).await;
        let (why, seen) = shown_while(&st, settle(&st, "r")).await;
        assert!(why.is_none() && !seen, "answers at once");
        assert!(st.tunnels.read().await.contains_key("r"));
        drop_pool(&st, "r").await;
        crate::ssm::tests::gone(fake.path()).await;

        let fake = open(silent_server().await).await;
        let (why, seen) = shown_while(&st, settle(&st, "r")).await;
        assert!(why == Some(qurry_shared::errors::Disconnected::TunnelSilent) && seen, "no answer within the grace");
        crate::ssm::tests::gone(fake.path()).await;

        let fake = open(silent_server().await).await;
        let c = st.connecting("r");
        let g = c.generation.load(SeqCst);
        let (r, seen) = shown_while(&st, drop_dead_tunnel(&st, &c, "r", &st.source("r").unwrap(), g, std::time::Instant::now())).await;
        assert!(r.is_ok() && !seen, "closed, not reconnecting");
        assert!(st.reconnecting().is_empty());
        crate::ssm::tests::gone(fake.path()).await;
    }

    /// A connect whose tunnel couldn't be opened again in place of one that stopped (while reconnecting() lists the source) says it
    /// couldn't reconnect, before the reason: the apps tell it from the load's own errors by that. A first open that fails doesn't
    #[tokio::test]
    async fn a_tunnel_that_cant_be_opened_again_says_it_couldnt_reconnect() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let c = silent_source("x", 5432, Some(serde_json::json!({ "enabled": true, "host": "127.0.0.1", "port": closed, "user": "me", "auth": "password", "keyPath": "" })));
        st.store.save(&c).unwrap();
        let first = ensure_pool(&st, "x").await.err().expect("no SSH server").to_string();
        assert!(!first.starts_with("Couldn't reconnect"), "{first}");
        // The tunnel stopped (as the keep-alive or the check after a wake closes it): the next open is a reconnect
        st.connecting("x").dead.store(true, SeqCst);
        let again = ensure_pool(&st, "x").await.err().expect("no SSH server").to_string();
        assert_eq!(again, format!("{}: {first}", qurry_shared::errors::reconnect_failed("x", false)));
        assert!(st.reconnecting().is_empty());
    }

    /// A tunnel that works but takes longer than FIRST_PROBE to bring the database's first byte (here 1.3s): its first probe is
    /// kept for the whole grace, so the check ends on that one connection after about 1.3s, not on a second probe after 1s + 1.3s
    #[cfg(unix)]
    #[tokio::test]
    async fn a_slow_first_answer_is_not_probed_again() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut c = silent_source("p", 5432, None);
        c.ssm = Some(store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: "i-0123456789abcdef0".into() });
        st.store.save(&c).unwrap();
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let n = accepted.clone();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                n.fetch_add(1, SeqCst);
                tokio::spawn(async move {
                    let mut b = [0u8; 8];
                    if s.read_exact(&mut b).await.is_ok() {
                        tokio::time::sleep(std::time::Duration::from_millis(1300)).await;
                        let _ = s.write_all(b"N").await;
                    }
                    let _ = s.read_to_end(&mut Vec::new()).await;
                });
            }
        });
        let (fake, t) = crate::ssm::tests::fake_tunnel_to(port).await;
        st.tunnels.write().await.insert("p".into(), t);
        // Opening the fake tunnel may have connected through it already
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let before = accepted.load(SeqCst);
        sleep_mac(&st, std::time::Duration::from_secs(3600));
        let started = std::time::Instant::now();
        let why = settle(&st, "p").await;
        let took = started.elapsed();
        assert!(why.is_none(), "kept");
        assert_eq!(accepted.load(SeqCst), before + 1, "one probe");
        assert!(took < std::time::Duration::from_millis(2000), "{took:?}");
        assert!(st.reconnecting().is_empty());
        drop_pool(&st, "p").await;
        crate::ssm::tests::gone(fake.path()).await;
    }

    /// with_pool reads the generation before ensure_pool: a close while ensure_pool hands out the pool (here: while it waits for
    /// the check after a wake) is noticed. Closed for a dead tunnel, the read runs again; after a Disconnect it ends with the error
    #[tokio::test]
    async fn a_close_while_the_pool_is_handed_out_is_noticed() {
        let lazy = || Pool::Pg(sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new().host("127.0.0.1").port(9)));
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        st.store.save(&silent_source("w", 9, None)).unwrap();
        for dead in [true, false] {
            let c = st.connecting("w");
            st.pools.write().await.insert("w".into(), lazy());
            // Unused since the wake: ensure_pool checks the source first, under the connect lock this test holds
            *c.used.lock().unwrap() = None;
            let held = c.failed.lock().await;
            let calls = std::sync::atomic::AtomicUsize::new(0);
            let read = with_pool(&st, "w", |_| {
                let n = calls.fetch_add(1, SeqCst);
                async move { if n == 0 { Err(AppError::Db(sqlx::Error::PoolClosed)) } else { Ok(n) } }
            });
            let (r, ()) = tokio::join!(read, async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if dead {
                    drop_pool_logged(&st, "w", crate::audit::BY_SERVER).await;
                } else {
                    drop_pool(&st, "w").await;
                }
                // Another caller's new pool, which ensure_pool hands out once the check is done
                st.pools.write().await.insert("w".into(), lazy());
                drop(held);
            });
            if dead {
                assert_eq!(r.unwrap(), 1, "run again");
            } else {
                assert!(matches!(r, Err(AppError::Db(sqlx::Error::PoolClosed))), "not after a Disconnect");
                assert_eq!(calls.load(SeqCst), 1);
            }
            drop_pool(&st, "w").await;
        }
    }

    /// A read through a tunnel that ensure_pool opened anew after it closed the dead one (here: the check after a wake waits for the
    /// connect lock this test holds, meanwhile the source is closed for a dead tunnel and a new tunnel and pool open) runs once:
    /// the new pool was not closed under it
    #[cfg(unix)]
    #[tokio::test]
    async fn a_read_on_the_pool_opened_after_a_dead_tunnel_runs_once() {
        let lazy = || Pool::Pg(sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new().host("127.0.0.1").port(9)));
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        st.store.save(&silent_source("w", 9, None)).unwrap();
        let c = st.connecting("w");
        let (old, t) = crate::ssm::tests::fake_tunnel().await;
        st.tunnels.write().await.insert("w".into(), t);
        st.pools.write().await.insert("w".into(), lazy());
        *c.used.lock().unwrap() = None;
        let held = c.failed.lock().await;
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let read = with_pool(&st, "w", |_| {
            calls.fetch_add(1, SeqCst);
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                Ok(())
            }
        });
        let (r, new) = tokio::join!(read, async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            drop_pool_logged(&st, "w", crate::audit::BY_SERVER).await;
            let (new, t) = crate::ssm::tests::fake_tunnel().await;
            st.tunnels.write().await.insert("w".into(), t);
            st.pools.write().await.insert("w".into(), lazy());
            // As keep_pool does for a new pool
            c.settled(st.wakes());
            drop(held);
            new
        });
        assert!(r.is_ok());
        assert_eq!(calls.load(SeqCst), 1, "not cut off and run again");
        drop_pool(&st, "w").await;
        crate::ssm::tests::gone(old.path()).await;
        crate::ssm::tests::gone(new.path()).await;
    }

    /// An export, import, backup, restore or subset copy keeps its source in use while it runs (counted per source, not per
    /// database), so the idle disconnect skips it and its tunnel gets the longer check
    #[tokio::test]
    async fn a_running_job_keeps_its_source_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        assert!(!in_use(&st, "x").await);
        let a = st.job_running("x::db1");
        let b = st.job_running("x");
        assert!(in_use(&st, "x").await && !in_use(&st, "y").await);
        drop(a);
        assert!(in_use(&st, "x").await, "one still runs");
        drop(b);
        assert!(!in_use(&st, "x").await && st.running.lock().unwrap().is_empty());
    }

    /// A tunnel whose CLI runs on while its port doesn't answer (the session died: sleep, a network change, the AWS side): the
    /// connects that time out through it close it and open a new one, once for the callers together, and they all get what that
    /// one did (here: the instance id is missing, so nothing is started). With a working new tunnel: cargo test -p qurry-core dead_ssm_tunnel -- --ignored
    #[cfg(unix)]
    #[tokio::test]
    async fn a_tunnel_that_stops_answering_is_opened_again() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut c = silent_source("t", 5432, None);
        c.ssm = Some(store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: String::new() });
        st.store.save(&c).unwrap();
        // The open tunnel: its CLI and plugin run, its port accepts, nothing answers
        let (fake, t) = crate::ssm::tests::fake_tunnel_to(silent_server().await).await;
        // In use just now, no wake: no check before the connect (settle)
        st.connecting("t").settled(st.wakes());
        st.tunnels.write().await.insert("t".into(), t);
        let start = std::time::Instant::now();
        let r = futures::future::join_all(["t", "t::db1", "t"].map(|id| ensure_pool(&st, id))).await;
        let took = start.elapsed();
        // The instance id is checked before the CLI runs (where there is one)
        let errors: Vec<String> = r.iter().map(|r| r.as_ref().err().map(|e| e.to_string()).unwrap_or_default()).collect();
        assert!(errors.iter().all(|e| *e == errors[0]) && (errors[0].contains("Enter the instance id") || errors[0].contains("AWS CLI not found")), "{errors:?}");
        assert!(took >= std::time::Duration::from_secs(10) && took < std::time::Duration::from_secs(15), "one timeout, then one try to open it: {took:?}");
        assert!(st.tunnels.read().await.is_empty());
        crate::ssm::tests::gone(fake.path()).await;
    }

    /// A PostgreSQL that answers an SSLRequest ("N") and then nothing: a database that is slow to accept new connections
    async fn slow_pg_server() -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 8];
                    if s.read_exact(&mut b).await.is_ok() && b == [0, 0, 0, 8, 4, 210, 22, 47] {
                        let _ = s.write_all(b"N").await;
                    }
                    let _ = s.read_to_end(&mut Vec::new()).await;
                });
            }
        });
        port
    }

    /// A connect that times out through a tunnel that still carries new connections (the pool is full, the database or its
    /// startup script is slow) keeps the tunnel and what runs through it, and fails after one timeout saying so
    #[cfg(unix)]
    #[tokio::test]
    async fn a_tunnel_that_still_answers_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut c = silent_source("k", 5432, None);
        c.ssm = Some(store::SsmConfig { enabled: true, profile: String::new(), region: String::new(), target: "i-0123456789abcdef0".into() });
        st.store.save(&c).unwrap();
        let (fake, t) = crate::ssm::tests::fake_tunnel_to(slow_pg_server().await).await;
        let port = t.local_port;
        st.connecting("k");
        st.tunnels.write().await.insert("k".into(), t);
        let start = std::time::Instant::now();
        let r = futures::future::join_all(["k", "k::db1"].map(|id| ensure_pool(&st, id))).await;
        let took = start.elapsed();
        for r in r {
            let e = r.err().expect("times out").to_string();
            assert!(e.contains("within 10s, though the AWS SSM tunnel (instance i-0123456789abcdef0) answers") && e.contains("all 32 connections") && e.contains("startup script"), "{e}");
        }
        assert!(took < std::time::Duration::from_secs(13), "one timeout, no second try: {took:?}");
        assert_eq!(st.tunnels.read().await.get("k").map(|t| (t.local_port, t.alive())), Some((port, true)), "the same tunnel");
        assert_eq!(std::fs::read_to_string(fake.path().join("starts")).unwrap().lines().count(), 1, "no new session");
        drop_pool(&st, "k").await;
        crate::ssm::tests::gone(fake.path()).await;
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
        let Some(pg) = crate::testenv::pg().map(|s| s.cfg) else { return };
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

    /// with_pool runs a read once more, on a new pool, when another caller closed its pool under it for a dead tunnel, and only
    /// once; not when the same error comes without that or after a disconnect (which leaves the source closed), nor for other
    /// errors. PostgreSQL and MySQL:
    /// QURRY_TEST_PG=… QURRY_TEST_MY=… cargo test -p qurry-core a_read_whose_pool_closes -- --ignored
    async fn a_read_whose_pool_closes_under_it_runs_again(kind: crate::store::DbKind) {
        use std::sync::atomic::AtomicUsize;
        let Some(server) = crate::testenv::server(kind) else { return };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let mut c = server.cfg.clone();
        c.id = String::new();
        let id = st.save_connection(c, server.password.clone(), None).await.unwrap().id;
        ensure_pool(&st, &id).await.unwrap();
        let (st, id, runs) = (&st, id.as_str(), &AtomicUsize::new(0));
        // A read whose first `fails` runs fail with `e`, closing the source first when `close`: as another caller that found the
        // tunnel dead would (BY_SERVER), or a Disconnect
        let read = |fails: usize, close: Option<&'static str>, e: fn() -> AppError| {
            runs.store(0, SeqCst);
            move |p: Pool| async move {
                if runs.fetch_add(1, SeqCst) < fails {
                    if let Some(word) = close {
                        drop_pool_logged(st, id, word).await;
                    }
                    return Err(e());
                }
                p.server_version().await
            }
        };
        let pool_closed: fn() -> AppError = || AppError::Db(sqlx::Error::PoolClosed);
        let reset: fn() -> AppError = || AppError::Db(sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionReset)));
        let (dead, disconnect) = (Some(crate::audit::BY_SERVER), Some(crate::audit::DISCONNECTED));
        for e in [pool_closed, reset] {
            assert!(with_pool(st, id, read(1, dead, e)).await.is_ok(), "on a new pool");
            assert_eq!(runs.load(SeqCst), 2);
            assert!(with_pool(st, id, read(2, dead, e)).await.is_err());
            assert_eq!(runs.load(SeqCst), 2, "once only");
            assert!(with_pool(st, id, read(1, None, e)).await.is_err());
            assert_eq!(runs.load(SeqCst), 1, "nobody closed it");
            assert!(with_pool(st, id, read(1, disconnect, e)).await.is_err());
            assert_eq!(runs.load(SeqCst), 1, "disconnected");
            assert!(st.pools.read().await.is_empty(), "the disconnect stays");
            ensure_pool(st, id).await.unwrap();
        }
        assert!(with_pool(st, id, read(1, dead, || AppError::Msg("syntax error".into()))).await.is_err());
        assert_eq!(runs.load(SeqCst), 1, "not for other errors");
        st.disconnect(id.into()).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn a_read_whose_pool_closes_under_it_runs_again_pg() {
        a_read_whose_pool_closes_under_it_runs_again(crate::store::DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn a_read_whose_pool_closes_under_it_runs_again_my() {
        a_read_whose_pool_closes_under_it_runs_again(crate::store::DbKind::Mysql).await;
    }
}
