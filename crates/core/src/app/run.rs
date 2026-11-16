use super::*;

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

/// Max size of an MCP CSV export
const EXPORT_MAX_BYTES: u64 = 512 << 20;

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
        let sql = &sql;
        with_pool(self, &id, |p| async move { dispatch!(p, b => b.run_read_only(sql, limit).await) }).await
    }

    /// Read-only query result to CSV (MCP). Same rules as run_read_only for checking and running (the full result also runs in a READ ONLY transaction)
    pub async fn export_csv_read_only(&self, id: String, sql: String, path: std::path::PathBuf) -> AppResult<u64> {
        let kind = self.kind_of(&id)?;
        if let Some(why) = crate::guard::read_only_violation(&sql, kind, true) {
            return Err(AppError::Msg(why));
        }
        let pool = checked_pool(self, &id).await?;
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
        // On the pool, nothing reached the server when getting a connection timed out: through a dead tunnel it goes on through a
        // new one. Never after it reached the server: it may change data (an INSERT, DDL, a Redis command), and would run twice
        if tab.is_none() && !undo {
            return (with_pool_after(self, id, None, false, |p| async move { p.run_query(sql, limit).await }).await, None, TxSeen::default());
        }
        // The session first: opening it, or checking one that sat unused (tab_session), replaces a tunnel that stopped forwarding,
        // and the pool is then the new one
        let sess = match tab {
            Some(t) => match ensure_session(self, id, t).await {
                Ok(s) => Some(s),
                Err(e) => return (Err(e), None, TxSeen::default()),
            },
            None => None,
        };
        // Undo reads the table's columns first: on a pool that answered (checked_pool), so that read can't wait on a dead tunnel
        let pool = match if undo { checked_pool(self, id).await } else { ensure_pool(self, id).await } {
            Ok(p) => p,
            Err(e) => return (Err(e), None, TxSeen::default()),
        };
        let plan = match undo {
            true => Some(crate::audit::plan(&pool, sql, cfg.kind).await),
            false => None,
        };
        let (mut guard, mut temp);
        let s: &mut Session = match &sess {
            Some(s) => {
                guard = s.lock().await;
                if guard.abandoned() {
                    return (Err(AppError::Msg(SESSION_ABANDONED.into())), None, TxSeen::default());
                }
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
                // Whether this or an earlier statement changed the session: a startup script runs before, not through here
                let changed = self.sessions.read().await.get(t).filter(|e| sess.as_ref().is_some_and(|s| std::sync::Arc::ptr_eq(&e.session, s))).is_some_and(|e| {
                    if session::changes_session(sql, cfg.kind) {
                        e.changed.store(true, SeqCst);
                    }
                    e.changed.load(SeqCst)
                });
                {
                    let mut all = self.more.lock().unwrap();
                    match more {
                        Some(more) => all.insert(t.to_string(), Kept { conn_id: id.to_string(), cfg: cfg.clone(), run, more, changed }),
                        None => all.remove(t),
                    };
                }
                used_now(self, t).await;
                // Last, right before the session is let go: a disconnect meanwhile marks it busy (close_all)
                settle_lost(self, t, s.in_tx());
            }
            _ => s.discard().await,
        }
        (r, undo, TxSeen { open, changed, ended })
    }

    /// Load the next rows of the tab's result. When its session was closed (drop_pool: settings saved, Disconnect, the idle drop),
    /// reopen the statement on a new session after the rows already read, unless that could give rows that don't line up or
    /// run something again (reopen_refused)
    pub async fn fetch_more(&self, tab_id: String, limit: Option<usize>) -> AppResult<QueryResult> {
        let limit = limit.unwrap_or(500);
        let closed = || AppError::Msg("Session closed".into());
        // The result this load-more is for, read before it waits for anything: a run queued ahead of it has a result of its own
        let issued = self.more.lock().unwrap().get(&tab_id).cloned();
        let (entry, stale) = match tab_session(self, &tab_id).await? {
            TabSession::Open(s, c) => (Some((s, c)), None),
            TabSession::Lost(stale) => (None, stale),
            TabSession::None => (None, None),
        };
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
                (new_session(self, &k.conn_id, &tab_id, stale).await?, k.conn_id.clone(), Some(k))
            }
        };
        touch(self, &conn_id);
        let (r, in_tx) = {
            let mut s = sess.lock().await;
            if s.abandoned() {
                return Err(AppError::Msg(SESSION_ABANDONED.into()));
            }
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
            settle_lost(self, &tab_id, s.in_tx());
            used_now(self, &tab_id).await;
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

    /// Explain Analyse (PostgreSQL · MySQL): the statements to run before and after it in the console's session, so it is rolled back
    /// without committing what is pending (Session::explain_wrap)
    pub async fn explain_wrap(&self, id: String, tab_id: String) -> AppResult<[&'static str; 2]> {
        let sess = ensure_session(self, &id, &tab_id).await?;
        let mut s = sess.lock().await;
        s.explain_wrap().await
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
        let r = self.run_batch(&id, cfg.kind, &statements, exact_one, prepared, watch, None, &mut undos).await;
        let logged = match watch {
            true => self.audit.after_batch(started, r.as_ref().err().map(|e| e.to_string()), exact_one, undos, None),
            false => Ok(()),
        };
        match r {
            Err(e) => Err(recover(self, &id, None, None, e).await),
            // Committed: a log failure now is a warning, so nobody saves it again
            Ok(rows) => Ok(BatchResult { rows, warning: logged.err().map(|e| e.to_string()) }),
        }
    }

    /// A result grid's edits, of a table tab and a console result alike: each UPDATE / DELETE must change exactly one row, or none
    /// of them is saved. tab = the console the result is in: in Manual mode or with a transaction open, they run in its transaction
    /// and stay pending until Commit / Roll back (#106; Session::submit); otherwise, and for a table tab, in a transaction of
    /// their own (execute_batch). The read-only check and the Production log are the same either way. Statements that ran go into the
    /// query history, as a console's runs do (GUI and TUI alike)
    pub async fn submit_edits(&self, id: String, tab_id: Option<String>, statements: Vec<String>) -> AppResult<BatchResult> {
        let r = self.submit_edits_now(id.clone(), tab_id, statements.clone()).await;
        if r.is_ok() {
            self.record_edits(&id, &statements);
        }
        r
    }

    /// Submit's statements in the history, under the database they ran in (the source's default for a target without one, as a
    /// console's runs are). Nothing of a temporary source (qurry-tui URL) is kept
    fn record_edits(&self, id: &str, statements: &[String]) {
        let (conn, db) = split_target(id);
        if self.store.is_temporary(conn) {
            return;
        }
        let db = db.map(String::from).or_else(|| self.store.get(conn).ok().flatten().map(|c| crate::sql::default_db(&c)).filter(|d| !d.is_empty()));
        if let Err(e) = self.history.add_all(conn, db.as_deref(), statements) {
            crate::log!("[qurry] history: {e}");
        }
    }

    async fn submit_edits_now(&self, id: String, tab_id: Option<String>, statements: Vec<String>) -> AppResult<BatchResult> {
        let manual = tab_id.as_ref().is_some_and(|t| self.tab_manual.lock().unwrap().get(t).copied().unwrap_or(false));
        let tab = match tab_id {
            Some(t) if manual || self.tx_state(t.clone()).await => t,
            _ => return self.execute_batch(id, statements, true).await,
        };
        for st in &statements {
            self.check_read_only(&id, st)?;
        }
        let cfg = self.source(&id)?;
        let watch = cfg.options.confirm_changes;
        let started = match watch {
            true => self.audit.before_batch(&id, &cfg.name, cfg.kind, &statements)?,
            false => vec![],
        };
        let mut undos = Vec::new();
        let r = self.run_batch(&id, cfg.kind, &statements, true, false, watch, Some(&tab), &mut undos).await;
        // In the transaction: the log has them as not committed yet, and logs its Commit / Roll back
        let logged = match watch {
            true => self.audit.after_batch(started, r.as_ref().err().map(|e| e.to_string()), true, undos, Some(&tab)),
            false => Ok(()),
        };
        match r {
            Err(e) => {
                // The transaction may have ended with the failure: a lost connection, a deadlock (MySQL rolls the whole one back)
                let open = self.tx_state(tab.clone()).await;
                let ended = crate::audit::ended_by("", &e, open);
                if let Some(w) = ended.filter(|_| watch)
                    && let Err(le) = self.audit.tx_closed(&id, &cfg.name, &tab, w)
                {
                    crate::log!("[qurry] {le}");
                }
                if ended.is_some() && !is_conn_lost(&e) {
                    return Err(AppError::Msg(format!("{e} — {} (the changes made in it before are gone too)", qurry_shared::errors::TX_ROLLED_BACK)));
                }
                Err(recover(self, &id, Some(&tab), ended, e).await)
            }
            Ok(rows) => Ok(BatchResult { rows, warning: logged.err().map(|e| e.to_string()) }),
        }
    }

    /// undo = also keep undo SQL for each statement (into `undos`): the snapshots run in the batch's transaction, right before their
    /// statements. tab = run them in that console's transaction (submit_edits)
    #[allow(clippy::too_many_arguments)]
    async fn run_batch(&self, id: &str, kind: store::DbKind, statements: &[String], exact_one: bool, prepared: bool, undo: bool, tab: Option<&str>, undos: &mut Vec<Option<Result<String, String>>>) -> AppResult<u64> {
        // The session first: checking one that sat unused (tab_session) replaces a tunnel that stopped forwarding
        let sess = match tab {
            Some(t) => Some(ensure_session(self, id, t).await?),
            None => None,
        };
        let pool = checked_pool(self, id).await?;
        let mut plans = Vec::new();
        for st in statements {
            plans.push(match undo && crate::audit::is_change(st, kind) {
                true => Some(crate::audit::plan(&pool, st, kind).await),
                false => None,
            });
        }
        let snaps: Vec<Option<&str>> = plans.iter().map(|p| p.as_ref().and_then(|p| p.as_ref().ok()).map(|p| p.snapshot.as_str())).collect();
        let (n, mut shots) = match (&sess, tab) {
            (Some(sess), Some(t)) => {
                let mut s = sess.lock().await;
                let (r, shots) = s.submit(statements, &snaps).await;
                // Its statements closed the result's cursor: there are no more rows to load
                self.more.lock().unwrap().remove(t);
                used_now(self, t).await;
                settle_lost(self, t, s.in_tx());
                (r?, shots)
            }
            _ => dispatch!(pool, b => b.execute_batch(statements, exact_one, prepared, &snaps).await)?,
        };
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
        let (pool, r) = with_pool(self, &id, |p| async move { crate::meta::object_index(&p).await.map(|r| (p, r)) }).await?;
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
        let req = &req;
        with_pool(self, &id, |p| async move { crate::meta::full_text_search(&p, req).await }).await
    }

    pub async fn redis_scan(&self, id: String, cursor: String, pattern: String, count: usize, type_filter: Option<String>) -> AppResult<crate::redisdb::ScanPage> {
        crate::redisdb::scan_page(&redis_pool(self, &id).await?, &cursor, &pattern, count, type_filter.as_deref()).await
    }

    /// The Redis tab's database picker and the explorer's database rows (key counts), and whether CONFIG works (checked on connect)
    pub async fn redis_overview(&self, id: String) -> AppResult<crate::redisdb::ServerOverview> {
        match ensure_pool(self, &id).await? {
            Pool::Redis(m, db) => crate::redisdb::overview(&m, db).await,
            _ => Err(AppError::Msg("Not a Redis data source".into())),
        }
    }

    /// Terminal: key names starting with `prefix`, from a bounded SCAN
    pub async fn redis_key_names(&self, id: String, prefix: String, max: usize) -> AppResult<Vec<String>> {
        crate::redisdb::key_names(&redis_pool(self, &id).await?, &prefix, max.min(200)).await
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

    pub async fn export_csv(&self, id: String, sql: String, path: String) -> AppResult<u64> {
        self.check_read_only(&id, &sql)?;
        let pool = checked_pool(self, &id).await?;
        pool.export_csv(&sql, std::path::Path::new(&path)).await
    }

    pub async fn list_databases(&self, id: String) -> AppResult<Vec<String>> {
        crate::log!("[qurry] list_databases {id}");
        let r = with_pool(self, &id, |p| async move { crate::meta::list_databases(&p).await }).await;
        if let Err(e) = &r {
            crate::log!("[qurry] list_databases {id} failed: {}", crate::logfile::error_kind(e));
        }
        r
    }

    pub async fn list_schemas(&self, id: String) -> AppResult<Vec<crate::meta::SchemaInfo>> {
        crate::log!("[qurry] list_schemas {id}");
        let r = with_pool(self, &id, |p| async move { crate::meta::list_schemas(&p).await }).await;
        match &r {
            Ok(v) => crate::log!("[qurry] list_schemas → {} schemas", v.len()),
            Err(e) => crate::log!("[qurry] list_schemas {id} failed: {}", crate::logfile::error_kind(e)),
        }
        r
    }

    pub async fn list_objects(&self, id: String, schema: String) -> AppResult<crate::meta::SchemaObjects> {
        let schema = &schema;
        with_pool(self, &id, |p| async move { crate::meta::list_objects(&p, schema).await }).await
    }

    pub async fn table_detail(&self, id: String, schema: String, table: String) -> AppResult<crate::meta::TableDetail> {
        let (schema, table) = (&schema, &table);
        with_pool(self, &id, |p| async move { crate::meta::table_detail(&p, schema, table).await }).await
    }

    pub async fn table_structure(&self, id: String, schema: String, table: String) -> AppResult<crate::meta::TableStructure> {
        let (schema, table) = (&schema, &table);
        with_pool(self, &id, |p| async move { crate::meta::table_structure(&p, schema, table).await }).await
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
        let (schema, name, kind) = (&schema, &name, &kind);
        with_pool(self, &id, |p| async move { crate::meta::object_ddl(&p, schema, name, kind).await }).await
    }

    pub async fn completion_meta(&self, id: String) -> AppResult<Vec<crate::meta::CompletionTable>> {
        let (pool, r) = with_pool(self, &id, |p| async move { crate::meta::completion_meta(&p).await.map(|r| (p, r)) }).await?;
        if self.cached(&pool, &id) {
            crate::search_cache::save_columns(&self.store.dir(), &id, &r);
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::*;

    /// Submit's statements go into the history under the database they ran in (a target without one: the source's default, as a
    /// console's runs); a temporary source's are never kept
    #[test]
    fn submitted_edits_are_recorded_except_a_temporary_sources() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        st.store.save(&silent_source("x", 1, None)).unwrap();
        st.record_edits("x", &["update t set a = 1 where id = 1".into()]);
        st.record_edits("x::other", &["delete from t where id = 2".into()]);
        let v = st.history.list(Some("x"), "", 10);
        assert_eq!(v.iter().map(|i| (i.sql.as_str(), i.database.as_deref())).collect::<Vec<_>>(), [("delete from t where id = 2", Some("other")), ("update t set a = 1 where id = 1", Some("d"))]);
        let tmp = st.open_temporary(crate::dsn::Temporary { config: silent_source("t", 1, None), password: None, notes: vec![] });
        st.record_edits(&tmp.id, &["update t set a = 1 where id = 1".into()]);
        st.record_edits(&format!("{}::d", tmp.id), &["update t set a = 1 where id = 1".into()]);
        assert_eq!(st.history.list(None, "", 10).len(), 2);
    }

    /// with_pool runs a read once more only when its pool was closed under it: PoolClosed or an I/O error after another caller
    /// closed the source for a dead tunnel, or when its tunnel stopped. Not after a disconnect, nor for its own pool's errors, a
    /// timeout (it has its own retry) or any other error
    #[test]
    fn a_read_is_run_again_only_when_its_pool_was_closed_under_it() {
        let io = || AppError::Db(sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionReset)));
        let closed = || AppError::Db(sqlx::Error::PoolClosed);
        // At generation `now`, the last disconnect counted up to `disconnected`
        let at = |now: u64, disconnected: u64| {
            let c = Connecting::default();
            c.generation.store(now, SeqCst);
            c.disconnected.store(disconnected, SeqCst);
            c
        };
        assert!(closed_under(&closed(), &at(2, 0), 1, false), "closed for a dead tunnel");
        assert!(closed_under(&io(), &at(2, 1), 1, false));
        assert!(closed_under(&io(), &at(1, 0), 1, true), "the SSH server ended the session");
        assert!(!closed_under(&closed(), &at(1, 0), 1, false), "nobody else closed it");
        assert!(!closed_under(&io(), &at(1, 0), 1, false));
        assert!(!closed_under(&closed(), &at(2, 2), 1, false), "a disconnect closed it");
        assert!(!closed_under(&io(), &at(3, 2), 1, true), "a disconnect, then a tunnel that stopped");
        assert!(!closed_under(&AppError::Db(sqlx::Error::PoolTimedOut), &at(2, 0), 1, true));
        assert!(!closed_under(&AppError::Db(sqlx::Error::RowNotFound), &at(2, 0), 1, true));
        assert!(!closed_under(&AppError::Msg("syntax error at or near \"fro\"".into()), &at(2, 0), 1, true));
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
                st.more.lock().unwrap().insert("more".into(), Kept { conn_id: id.clone(), cfg: st.source(&id).unwrap(), run: 0, more, changed: false });
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
        let k = |sql: &str, kind, tx: bool| Kept { conn_id: "s".into(), cfg: src(kind), run: 0, more: session::More { sql: sql.into(), fetched: 500, tx, schema: Some("public".into()) }, changed: false };
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
        // The session was changed (SET, RESET, USE …) before or by it: a new connection lacks that, so it says to run it again
        let set = Kept { changed: true, ..k("select * from t", Postgres, false) };
        assert_eq!(
            reopen_refused(&set, &src(Postgres), false).as_deref(),
            Some("The data source reconnected since this result was read (settings saved, disconnected or idle), and the session was changed since (SET, RESET, USE or a statement that isn't a read), so a new connection wouldn't match. Run the query again to load more rows.")
        );
        // A statement of its own that isn't a plain read says so first
        let e = reopen_refused(&Kept { changed: true, ..k("select pg_sleep(1), * from t", Postgres, false) }, &src(Postgres), false).unwrap();
        assert!(e.contains("isn't a plain read"), "{e}");
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
        // the console's own lost-transaction notice (#110) comes first, once
        let e = st.fetch_more("m".into(), Some(500)).await.unwrap_err().to_string();
        assert!(e.contains("The transaction is lost") && e.contains("nothing was sent"), "{e}");
        assert!(st.sessions.read().await.get("m").is_none(), "refused before opening a session");

        // The session was changed since it opened (SET, RESET, a MySQL `select @v := …`, a temp table): a new connection wouldn't
        // have it, so it asks to run the query again (picking the schema above isn't such a change: reopen sets it again)
        let changes: &[&str] = match my {
            true => &["set @qurry_reconnect = 1", "select @qurry_reconnect := 5", "create temporary table qurry_reconnect_t (a int)"],
            false => &["set statement_timeout = 0", "reset all", "select set_config('qurry.reconnect', 'x', false)", "create temp table qurry_reconnect_t (a int)"],
        };
        for (i, c) in changes.iter().enumerate() {
            let tab = format!("s{i}");
            q(c, &tab).await.unwrap();
            assert!(q(full, &tab).await.unwrap().truncated);
            st.disconnect("d".into()).await.unwrap();
            let e = st.fetch_more(tab.clone(), Some(500)).await.unwrap_err().to_string();
            assert!(e.contains("the session was changed since (SET, RESET, USE or a statement that isn't a read)") && e.ends_with("Run the query again to load more rows."), "{c}: {e}");
            assert!(st.sessions.read().await.get(&tab).is_none(), "{c}: refused before opening a session");
            // Run again: a new session without the change, and its result goes on after a reconnect
            assert!(q(full, &tab).await.unwrap().truncated);
            st.disconnect("d".into()).await.unwrap();
            let r = st.fetch_more(tab.clone(), Some(500)).await.unwrap();
            assert_eq!(first(&r), Some(501), "{c}");
        }

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

    /// A failed statement that isn't the first in the console: the server's spot (PostgreSQL's character position, MySQL's line and
    /// near '…') lands on the right text of the console through the statement's offset. Real servers:
    /// QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres QURRY_TEST_MY=127.0.0.1:13306:root:dbtool:shop cargo test -p qurry-core error_position_maps -- --ignored
    async fn error_position_maps_into_the_console(kind: crate::store::DbKind) {
        let Some(crate::testenv::Server { mut cfg, password }) = crate::testenv::server(kind) else { return };
        (cfg.id, cfg.name) = ("d".into(), "subquery-run-errors".into());
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        st.save_connection(cfg, password, None).await.unwrap();
        let text = "select 1;\n\n-- 한글 before it: PostgreSQL counts characters\nselect n, '한글' as k\nfrom (select 1 as n) t\nwhere n = 1 and n = = 1;";
        let smallest = crate::exec::ExecSettings { inside: crate::exec::Inside::Smallest, ..Default::default() };
        let crate::exec::Plan::Run(v) = crate::exec::plan(text, text.find("where").unwrap(), kind, smallest) else { panic!("one statement") };
        let e = st.run_query("d".into(), v[0].sql.clone(), None, Some("t".into())).await.unwrap_err().to_string();
        let at = qurry_shared::errors::error_position(&e, &v[0].sql).unwrap_or_else(|| panic!("no position in {e:?}"));
        assert_eq!(v[0].start + at, text.rfind("= 1").unwrap(), "{e}");
        // A short spot that also comes earlier on the line (MySQL: near 't')
        let e = st.run_query("d".into(), "select 1 fro t".into(), None, Some("t".into())).await.unwrap_err().to_string();
        assert_eq!(qurry_shared::errors::error_position(&e, "select 1 fro t"), Some("select 1 fro ".len()), "{e}");
    }

    #[tokio::test]
    #[ignore]
    async fn error_position_maps_pg() {
        error_position_maps_into_the_console(crate::store::DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn error_position_maps_mysql() {
        error_position_maps_into_the_console(crate::store::DbKind::Mysql).await;
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

        // Two load-mores after a reconnect: one reopens, the other goes on from its cursor. On a new session: pg_sleep() above isn't
        // a plain read, so a result of the old one isn't reopened (session::changes_session)
        st.close_session("t".into()).await.unwrap();
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
