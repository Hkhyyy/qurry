use super::*;

/// A console whose session a disconnect closed while a transaction may have been open in it (AppState::lost_tx)
pub(super) struct LostTx {
    /// What its next run, load-more, Commit or Roll back says instead of running (once)
    pub(super) msg: String,
    /// Something ran in the session then, so whether a transaction was open is known only when that ends (settle_lost)
    pub(super) busy: bool,
}

/// How long a Production console's transaction may sit open and unused before the console reminds about it (#106)
pub const TX_IDLE_REMIND: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl AppState {
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

/// What a console says when a disconnect closed its session with a transaction open in it (drop_pool_logged): word = how
pub(super) fn tx_lost_error(cfg: &ConnectionConfig, word: &str) -> String {
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

impl AppState {
    /// Production audit log (newest first)
    pub fn audit_log(&self, conn: Option<String>, filter: String, limit: usize) -> Vec<crate::audit::AuditEntry> {
        self.audit.list(conn.as_deref(), &filter, limit)
    }

    /// Log the console's Commit / Roll back to the audit log (only if the logged changes were uncommitted)
    pub(super) async fn audit_tx_end(&self, tab_id: &str, word: &str) -> AppResult<()> {
        let conn = self.sessions.read().await.get(tab_id).map(|e| e.conn_id.clone());
        match conn.map(|c| (self.source(&c), c)) {
            Some((Ok(cfg), c)) => self.audit.tx_closed(&c, &cfg.name, tab_id, word),
            _ => Ok(()),
        }
    }

    /// The console's transaction with logged changes ended without a COMMIT after `sql` ran: as the MySQL session says the statement
    /// ended it (`ended`: DDL, SET PASSWORD, BEGIN … committed it; a CALL, deadlock or lock wait timeout either way), else rolled back
    /// by the server (deadlock, failed COMMIT, lost connection), or a COMMIT cut off by a lost connection has an unknown outcome (audit::ended_by)
    pub(super) async fn audit_lost_tx(&self, id: &str, name: &str, tab: Option<&str>, sql: &str, r: &AppResult<QueryResult>, ended: Option<&'static str>) -> AppResult<()> {
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
mod tests {
    use super::*;
    use crate::app::tests::*;

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
