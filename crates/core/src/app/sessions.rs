use super::*;

/// Housekeeping's hold on a console's session (SessionEntry::housekept). Dropped before the session's lock is let go
pub(super) struct Housekept(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Housekept {
    /// Under the session's lock: only a session without a user transaction counts as idle while housekeeping holds it
    pub(super) fn hold(flag: &std::sync::Arc<std::sync::atomic::AtomicBool>, s: &Session) -> Option<Self> {
        (!s.in_tx()).then(|| {
            flag.store(true, SeqCst);
            Self(flag.clone())
        })
    }
}

impl Drop for Housekept {
    fn drop(&mut self) {
        self.0.store(false, SeqCst);
    }
}

/// Whether `d` has passed since `t` by the wall clock: an Instant stops while the Mac sleeps, and sleep is when tunnels die. A
/// clock set back counts as passed (one check too many, never one too few)
pub(crate) fn idle_for(t: std::time::SystemTime, d: std::time::Duration) -> bool {
    t.elapsed().map_or(true, |e| e >= d)
}

/// The run, load-more, Commit or Roll back that had the tab's session while a disconnect closed it (LostTx::busy) ended: only now
/// is it known whether a transaction was open in it. in_tx = the session's, under its lock
pub(super) fn settle_lost(state: &AppState, tab: &str, in_tx: bool) {
    let mut all = state.lost_tx.lock().unwrap();
    match all.get_mut(tab) {
        Some(l) if l.busy && in_tx => l.busy = false,
        Some(l) if l.busy => drop(all.remove(tab)),
        _ => {}
    }
}

/// How long a console's session through a tunnel may sit unused before its next use checks it first (tab_session)
pub(super) const SESSION_IDLE: std::time::Duration = std::time::Duration::from_secs(10);

/// How long that check and the keep-alive ping of a session wait for an answer
pub(super) const SESSION_PING: std::time::Duration = std::time::Duration::from_secs(5);

/// A console's session, as its next use finds it (tab_session)
pub(super) enum TabSession {
    Open(std::sync::Arc<tokio::sync::Mutex<Session>>, String),
    None,
    /// It didn't answer and was closed; no transaction was open in it. Some(generation) = its tunnel was closed too, so opening
    /// a new one was the call's one retry (with_pool_after)
    Lost(Option<u64>),
}

/// The tab's session for a run, a load-more, a Commit or a Roll back. Through a tunnel, one unused for SESSION_IDLE answers a
/// ping first (within SESSION_PING): after sleep or a network change the tunnel's session may carry nothing while its process
/// runs on, and a statement sent there would wait forever. One that doesn't answer is closed without being sent anything else,
/// and when the tunnel doesn't carry a new connection either, the tunnel and the source's pools go too (drop_dead_tunnel), so
/// the next session opens through a new tunnel. A transaction that was open in it is lost: that is an error
/// (lost_session_error), so the statement meant for it never runs in a new session instead. Unless the tunnel still carries a new
/// connection: then the session is slow, not gone, and it is kept with its transaction (slow_session_error). A session a disconnect
/// closed with a transaction open says so here first (LostTx)
pub(super) async fn tab_session(state: &AppState, tab: &str) -> AppResult<TabSession> {
    // A disconnect closed its session with a transaction open in it: said once, nothing sent (drop_pool_logged)
    if let Some(l) = state.lost_tx.lock().unwrap().remove(tab) {
        return Err(AppError::Msg(l.msg));
    }
    let found = state.sessions.read().await.get(tab).map(|e| {
        let mut used = e.used.lock().unwrap();
        let idle = idle_for(*used, SESSION_IDLE);
        *used = std::time::SystemTime::now();
        (e.session.clone(), e.conn_id.clone(), idle)
    });
    let Some((sess, conn_id, idle)) = found else { return Ok(TabSession::None) };
    if !idle {
        return Ok(TabSession::Open(sess, conn_id));
    }
    let cfg = match state.source(&conn_id) {
        Ok(c) if c.tunneled() => c,
        _ => return Ok(TabSession::Open(sess, conn_id)),
    };
    let base = split_target(&conn_id).0;
    // After the Mac woke or a long idle the tunnel is checked first (settle). When it carries nothing it is closed with the
    // source's sessions, this one too: a transaction open in it is lost and says so (LostTx), else the tab opens a new session
    if settle(state, base).await.is_some() {
        if let Some(l) = state.lost_tx.lock().unwrap().remove(tab) {
            return Err(AppError::Msg(l.msg));
        }
        return Ok(TabSession::None);
    }
    let c = state.connecting(base);
    // The tunnel the session went through (a disconnect counts it up and closes the session)
    let g = c.generation.load(SeqCst);
    // The tunnel was checked here already and carried nothing: it isn't checked again (drop_dead_tunnel)
    let mut tunnel_dead = false;
    let in_tx = {
        // Something runs in it (a load-more, a cancel's wait): it isn't idle
        let Ok(mut s) = sess.try_lock() else { return Ok(TabSession::Open(sess, conn_id)) };
        let slow = match tokio::time::timeout(SESSION_PING, s.check()).await {
            // Any answer counts, an error too (a transaction an error aborted)
            Ok(r) if r.as_ref().err().is_none_or(|e| !is_conn_lost(e)) => {
                drop(s);
                return Ok(TabSession::Open(sess, conn_id));
            }
            Ok(_) => false,
            Err(_) => true,
        };
        // A transaction isn't given up on one slow answer: while the tunnel carries a new connection, the session is slow (the
        // network coming back after sleep, a tunnel busy with a backup), not gone, and it is kept as it is. The check left
        // unanswered is read past by its next use (Session::check)
        if slow && s.in_tx() {
            let port = state.tunnels.read().await.get(base).map(|t| t.local_port);
            if let Some(port) = port {
                if crate::tunnel::answers(port, &cfg, SESSION_PING).await {
                    drop(s);
                    // Checked again on its next use, so a session that never comes back can't take a statement and hang
                    if let Some(e) = state.sessions.read().await.get(tab) {
                        *e.used.lock().unwrap() = std::time::SystemTime::UNIX_EPOCH;
                    }
                    // This error says it already: a mark a disconnect left meanwhile isn't said a second time
                    state.lost_tx.lock().unwrap().remove(tab);
                    return Err(slow_session_error(&cfg));
                }
                tunnel_dead = true;
            }
        }
        s.abandon();
        s.in_tx()
    };
    crate::log!("[qurry] the console session of {tab} on {conn_id} didn't answer within {}s; closing it", SESSION_PING.as_secs());
    if in_tx && let Err(e) = state.audit_tx_end(tab, crate::audit::BY_SERVER).await {
        crate::log!("[qurry] {e}");
    }
    {
        let mut all = state.sessions.write().await;
        if all.get(tab).is_some_and(|e| std::sync::Arc::ptr_eq(&e.session, &sess)) {
            all.remove(tab);
        }
    }
    drop(sess);
    let asked = std::time::Instant::now();
    let closed = {
        let _connecting = c.failed.lock().await;
        if !tunnel_dead {
            drop_dead_tunnel(state, &c, base, &cfg, g, asked).await.is_ok()
        } else {
            // Unless a disconnect closed it meanwhile
            if c.generation.load(SeqCst) == g {
                crate::log!("[qurry] the tunnel of {base} carries nothing any more; closing it (the next connect opens a new one)");
                drop_pool_logged(state, base, crate::audit::BY_SERVER).await;
            }
            true
        }
    };
    if in_tx {
        // The drop above may have marked this tab too; this error says it already
        state.lost_tx.lock().unwrap().remove(tab);
        return Err(lost_session_error(&cfg, closed));
    }
    Ok(TabSession::Lost(closed.then_some(g)))
}

/// The tab's run or load-more ended: its session counts as unused from now, not from when a long statement started (tab_session)
pub(super) async fn used_now(state: &AppState, tab: &str) {
    if let Some(e) = state.sessions.read().await.get(tab) {
        *e.used.lock().unwrap() = std::time::SystemTime::now();
    }
}

pub async fn ensure_session(state: &AppState, id: &str, tab_id: &str) -> AppResult<std::sync::Arc<tokio::sync::Mutex<Session>>> {
    let stale = match tab_session(state, tab_id).await? {
        TabSession::Open(s, c) if c == id => return Ok(s),
        TabSession::Lost(stale) => stale,
        _ => None,
    };
    new_session(state, id, tab_id, stale).await
}

/// A new session for the tab. stale: tab_session closed the tab's dead tunnel already
pub(super) async fn new_session(state: &AppState, id: &str, tab_id: &str, stale: Option<u64>) -> AppResult<std::sync::Arc<tokio::sync::Mutex<Session>>> {
    // If the same tab switched to another DB, clean up and return the previous session
    let old = {
        let mut all = state.sessions.write().await;
        match all.get(tab_id) {
            Some(e) if e.conn_id != id => all.remove(tab_id),
            _ => None,
        }
    };
    if let Some(e) = old {
        e.session.lock().await.discard().await;
    }
    // Opening one runs nothing of the user's: through a tunnel that stopped forwarding, it opens again on a new one
    let mut s = with_pool_after(state, id, stale, true, |p| async move { Session::open(&p).await }).await?;
    // Production: a read in Manual mode doesn't open the transaction (Session::set_reads_outside_tx)
    s.set_reads_outside_tx(state.source(id).is_ok_and(|c| c.options.confirm_changes));
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
    let entry = SessionEntry { conn_id: id.to_string(), pid: s.backend_id(), session: std::sync::Arc::new(tokio::sync::Mutex::new(s)), used: std::sync::Mutex::new(std::time::SystemTime::now()), housekept: Default::default(), changed: Default::default() };
    let arc = entry.session.clone();
    let replaced = all.insert(tab_id.to_string(), entry);
    drop(all);
    if let Some(e) = replaced {
        e.session.lock().await.discard().await;
    }
    Ok(arc)
}

/// A console session AppState::close_all_sessions closed: its tab, its data source's name, whether a transaction was rolled back
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedSession {
    pub tab_id: String,
    pub name: String,
    pub rolled_back: bool,
}

impl AppState {
    pub async fn cancel_query(&self, tab_id: String) -> AppResult<bool> {
        let (conn_id, pid) = match self.sessions.read().await.get(&tab_id) {
            Some(e) => (e.conn_id.clone(), e.pid),
            None => return Ok(false),
        };
        // A cancel opens a connection of its own and asks the server to stop the statement: it may run again. Through a tunnel that
        // carries nothing any more, the timeout closes it (drop_dead_tunnel), which ends the statement waiting there too
        with_pool(self, &conn_id, |p| async move { dispatch!(p, b => b.cancel(pid).await) }).await
    }

    pub async fn close_session(&self, tab_id: String) -> AppResult<()> {
        self.tab_manual.lock().unwrap().remove(&tab_id);
        self.lost_tx.lock().unwrap().remove(&tab_id);
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

    /// Every console session, closed: the GUI's window loaded its page again (a reload after an error), so the tabs that owned them
    /// are gone, and a session left open would keep its transaction and locks. An open transaction is rolled back (audited as
    /// closed); one a running statement holds counts as open. The closed ones in order, with their data source's name
    pub async fn close_all_sessions(&self) -> Vec<ClosedSession> {
        let all: Vec<_> = self.sessions.read().await.iter().map(|(t, e)| (t.clone(), e.conn_id.clone(), e.session.clone(), e.housekept.clone())).collect();
        let mut out = Vec::new();
        for (tab_id, conn_id, session, housekept) in all {
            let rolled_back = session.try_lock().map(|s| s.in_tx()).unwrap_or_else(|_| !housekept.load(SeqCst));
            drop(session);
            let _ = self.close_session(tab_id.clone()).await;
            let name = self.source(&conn_id).map(|c| c.name).unwrap_or(conn_id);
            out.push(ClosedSession { tab_id, name, rolled_back });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::*;

    /// A console set to Manual while its source can't be reached (a Production console opened then, #106): the session its next
    /// run opens starts in Manual, never in Auto committing at once
    #[tokio::test]
    async fn manual_mode_is_kept_when_the_session_cant_open() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        st.store.save(&silent_source("x", 1, None)).unwrap();
        assert!(st.set_tx_mode("x".into(), "t".into(), true).await.is_err());
        assert_eq!(st.tab_manual.lock().unwrap().get("t"), Some(&true));
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
        assert_eq!(housekeeping(&state).await, vec![("hk".to_string(), format!("Disconnected (idle): {}", cfg.name))]);
        assert!(state.pools.read().await.is_empty() && state.sessions.read().await.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The GUI's page loaded again (#181): every console session closes, the open transaction is rolled back (its row isn't
    /// there, its lock is gone) and said, a session without one is closed too
    #[tokio::test]
    #[ignore]
    async fn close_all_sessions_rolls_back_open_transactions() {
        let Some(crate::testenv::Server { cfg, password }) = crate::testenv::pg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        st.save_connection(ConnectionConfig { id: "d".into(), name: "dev".into(), ..cfg }, password, None).await.unwrap();
        let q = |sql: &str, tab: &str| st.run_query("d".into(), sql.into(), None, Some(tab.into()));
        q("create table if not exists qurry_close_all (a int primary key)", "setup").await.unwrap();
        q("truncate qurry_close_all", "setup").await.unwrap();
        st.close_session("setup".into()).await.unwrap();
        st.set_tx_mode("d".into(), "open".into(), true).await.unwrap();
        assert!(q("insert into qurry_close_all values (1)", "open").await.unwrap().in_transaction);
        q("select 1", "plain").await.unwrap();
        let mut closed = st.close_all_sessions().await;
        closed.sort_by(|a, b| a.tab_id.cmp(&b.tab_id));
        assert_eq!(
            closed,
            [ClosedSession { tab_id: "open".into(), name: "dev".into(), rolled_back: true }, ClosedSession { tab_id: "plain".into(), name: "dev".into(), rolled_back: false }]
        );
        assert!(st.sessions.read().await.is_empty());
        assert!(st.close_all_sessions().await.is_empty());
        // Rolled back: no row, and the key isn't locked (this would wait on the lock otherwise)
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), q("insert into qurry_close_all values (1) returning a", "after")).await.expect("not locked").unwrap();
        assert_eq!(r.rows.len(), 1);
        q("drop table qurry_close_all", "after").await.unwrap();
        st.close_session("after".into()).await.unwrap();
    }

    /// A console's session that only housekeeping holds (no user transaction open in it) isn't busy: closing its source loses
    /// nothing (in_use), and the console isn't told a transaction was lost (close_all). One with a transaction open still counts,
    /// whoever holds it. QURRY_TEST_PG=… cargo test -p qurry-core a_session_only_housekeeping_holds -- --ignored
    #[tokio::test]
    #[ignore]
    async fn a_session_only_housekeeping_holds_is_idle() {
        let Some(server) = crate::testenv::pg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let mut c = server.cfg.clone();
        c.id = String::new();
        let id = st.save_connection(c, server.password.clone(), None).await.unwrap().id;
        st.run_query(id.clone(), "select 1".into(), None, Some("idle".into())).await.unwrap();
        async fn entry(st: &AppState, tab: &str) -> (std::sync::Arc<tokio::sync::Mutex<Session>>, std::sync::Arc<std::sync::atomic::AtomicBool>) {
            let all = st.sessions.read().await;
            let e = all.get(tab).unwrap();
            (e.session.clone(), e.housekept.clone())
        }
        let (s, flag) = entry(&st, "idle").await;
        {
            let g = s.lock().await;
            let _held = Housekept::hold(&flag, &g).expect("no transaction open");
            assert!(!in_use(&st, &id).await, "housekeeping's hold");
            drop_pool_logged(&st, &id, crate::audit::BY_SERVER).await;
        }
        assert!(!st.lost_tx.lock().unwrap().contains_key("idle"), "not told a transaction was lost");
        st.run_query(id.clone(), "select 1".into(), None, Some("idle".into())).await.expect("runs in a new session");
        // A user's run holds it: busy, and whether a transaction was open is known when it ends
        let (s, flag) = entry(&st, "idle").await;
        {
            let _g = s.lock().await;
            assert!(!flag.load(SeqCst) && in_use(&st, &id).await);
        }
        // A transaction open: busy whoever holds it, and lost when it goes
        st.run_query(id.clone(), "begin".into(), None, Some("tx".into())).await.unwrap();
        let (s, flag) = entry(&st, "tx").await;
        {
            let g = s.lock().await;
            assert!(Housekept::hold(&flag, &g).is_none(), "housekeeping never counts it idle");
            assert!(in_use(&st, &id).await);
            drop_pool_logged(&st, &id, crate::audit::BY_SERVER).await;
        }
        let e = st.run_query(id.clone(), "select 1".into(), None, Some("tx".into())).await.expect_err("lost").to_string();
        assert!(e.contains("The transaction is lost"), "{e}");
        st.disconnect(id).await.unwrap();
    }
}
