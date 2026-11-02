//! Each console tab gets its own dedicated connection (session).
//! - SELECT-like statements fetch 500 rows at a time via a server cursor (DECLARE/FETCH) → "load more"
//! - The backend PID is known, so a running query can be cancelled with pg_cancel_backend
use crate::backend::{Backend, dispatch};
use crate::db::{pg_columns, pg_value, query_result, ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use futures::TryStreamExt;
use serde_json::Value;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnection, PgPool, PgRow};
use sqlx::{Column, Either, Executor, Postgres, Row, TypeInfo};
use std::time::Instant;

const CURSOR: &str = "dbtool_cur";
/// When switching from Manual → Auto is blocked
pub const PENDING_TX: &str = "There are uncommitted changes — commit or roll back first";

pub struct PgSession {
    conn: PoolConnection<Postgres>,
    pub pid: i32,
    cursor_open: bool,
    cursor_sql: String,
    columns: Vec<ColumnInfo>,
    /// Tx: Manual mode (DataGrip). Statements accumulate in a transaction until Commit / Roll back
    manual: bool,
    /// Whether a transaction is open in Manual mode
    pub in_tx: bool,
    /// Last time the cursor was used (closed when unused for long)
    cursor_used: Instant,
    /// Whether the cursor was closed for being unused (reopened on load more)
    idle_closed: bool,
    /// Rows read through the cursor for the current result (OFFSET when reopening)
    fetched: usize,
}

/// In Auto mode, a cursor whose result was not fully read is closed after this long unused.
/// While the cursor is open, the transaction is open too (idle in transaction) and blocks VACUUM/DDL
pub const CURSOR_IDLE: std::time::Duration = std::time::Duration::from_secs(60);

pub fn strip_comments_lead(sql: &str) -> String {
    let mut s = sql.trim_start();
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map(|x| x.1).unwrap_or("").trim_start();
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map(|x| x.1).unwrap_or("").trim_start();
        } else {
            return s.to_lowercase();
        }
    }
}

pub fn is_select_like(sql: &str) -> bool {
    let s = strip_comments_lead(sql);
    ["select", "with", "values", "table", "("].iter().any(|k| s.starts_with(k))
}

/// EXPLAIN (GENERIC_PLAN) of a statement with $1 … has no values to bind, which the extended protocol expects: it goes over the simple protocol.
/// One statement only (the simple protocol would run everything after a `;` too)
pub fn is_generic_plan(sql: &str) -> bool {
    let s = strip_comments_lead(sql);
    s.starts_with("explain") && s.contains("generic_plan") && !strip_trailing_semicolon(&s).contains(';')
}

fn strip_trailing_semicolon(sql: &str) -> &str {
    sql.trim_end().trim_end_matches(';').trim_end()
}

impl PgSession {
    pub async fn open(pool: &PgPool) -> AppResult<Self> {
        let mut conn = pool.acquire().await?;
        let pid: i32 = sqlx::query_scalar("select pg_backend_pid()").fetch_one(&mut *conn).await?;
        Ok(Self { conn, pid, cursor_open: false, cursor_sql: String::new(), columns: Vec::new(), manual: false, in_tx: false, cursor_used: Instant::now(), idle_closed: false, fetched: 0 })
    }

    /// keep-alive: so idle session connections are not dropped
    pub async fn ping(&mut self) {
        let _ = self.conn.execute("select 1").await;
    }

    #[allow(dead_code)]
    pub fn has_more(&self) -> bool {
        self.cursor_open
    }

    /// Whether a user-opened transaction exists (Manual mode, or BEGIN run directly in Auto). The cursor then only uses savepoints
    /// and never ends the user's transaction with COMMIT/ROLLBACK
    fn user_tx(&self) -> bool {
        self.manual || self.in_tx
    }

    pub async fn close_cursor(&mut self) {
        if self.cursor_open {
            // A cursor inside the user's transaction is only closed (committing would silently finalize the user's changes)
            let sql = if self.user_tx() { "CLOSE dbtool_cur" } else { "CLOSE dbtool_cur; COMMIT" };
            let _ = self.conn.execute(sql).await;
            self.cursor_open = false;
        }
    }

    /// In Auto mode, close cursors unused for long and end the transaction (Manual is left alone since it is the user's transaction)
    pub async fn close_idle_cursor(&mut self, idle: std::time::Duration) -> bool {
        if self.cursor_open && !self.user_tx() && self.cursor_used.elapsed() >= idle {
            self.close_cursor().await;
            self.idle_closed = true;
            return true;
        }
        false
    }

    /// On tab close: clean up the cursor and transaction and return a clean connection to the pool
    pub async fn discard(&mut self) {
        self.close_cursor().await;
        if self.manual || self.in_tx {
            let _ = self.conn.execute("ROLLBACK").await;
            self.in_tx = false;
        }
    }

    /// Undo a cursor-side failure: inside the user's transaction, only back to the savepoint (the transaction survives)
    async fn rollback(&mut self) {
        let sql = if self.user_tx() { "ROLLBACK TO SAVEPOINT dbtool_sp" } else { "ROLLBACK" };
        let _ = self.conn.execute(sql).await;
        self.cursor_open = false;
    }

    pub fn in_tx(&self) -> bool {
        self.in_tx
    }

    /// Switch Tx mode. Do not switch to Auto if there are uncommitted changes (Commit or Roll back first)
    pub async fn set_manual(&mut self, manual: bool) -> AppResult<()> {
        if !manual && self.in_tx {
            return Err(AppError::Msg(PENDING_TX.into()));
        }
        self.close_cursor().await;
        self.manual = manual;
        Ok(())
    }

    pub async fn commit(&mut self) -> AppResult<()> {
        self.close_cursor().await;
        let r = self.conn.execute("COMMIT").await;
        // A failed COMMIT (e.g. a deferred foreign key) ends the transaction too: PostgreSQL rolls it back
        self.in_tx = false;
        r?;
        Ok(())
    }

    pub async fn rollback_tx(&mut self) -> AppResult<()> {
        self.close_cursor().await;
        self.conn.execute("ROLLBACK").await?;
        self.in_tx = false;
        Ok(())
    }

    /// Sync the state when the user runs BEGIN / COMMIT / ROLLBACK directly
    fn track_tx_keywords(&mut self, sql: &str) {
        let s = strip_comments_lead(sql);
        if s.starts_with("begin") || s.starts_with("start transaction") {
            self.in_tx = true;
        } else if s.starts_with("commit") || s.starts_with("rollback") || s.starts_with("end") || s.starts_with("abort") {
            if !s.starts_with("rollback to") {
                self.in_tx = false;
            }
        }
    }

    pub async fn run(&mut self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        let mut r = self.run_inner(sql, limit).await;
        if let Ok(q) = &mut r {
            q.in_transaction = self.in_tx;
        }
        r
    }

    async fn run_inner(&mut self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        self.close_cursor().await;
        self.idle_closed = false;
        let start = Instant::now();
        if self.manual && !self.in_tx && !strip_comments_lead(sql).starts_with("begin") {
            self.conn.execute("BEGIN").await?;
            self.in_tx = true;
        }
        if is_select_like(sql) {
            match self.declare(strip_trailing_semicolon(sql)).await {
                Ok(_) => {
                    self.cursor_sql = sql.to_string();
                    self.columns.clear();
                    self.fetched = 0;
                    let mut r = self.fetch(limit).await?;
                    r.elapsed_ms = start.elapsed().as_millis() as u64;
                    return Ok(r);
                }
                Err(_) => {
                    // Statements DECLARE cannot take (e.g. WITH ... DELETE) → run normally. A real error will surface again below.
                    self.rollback().await;
                }
            }
        }
        let r = self.run_direct(sql, limit, start).await;
        self.track_tx_keywords(sql);
        r
    }

    /// Short transaction for the cursor. Inside a user transaction (Manual, or BEGIN run directly in Auto), protected by a savepoint.
    async fn declare(&mut self, query: &str) -> Result<(), sqlx::Error> {
        let declare = if self.user_tx() {
            format!("SAVEPOINT dbtool_sp; DECLARE {CURSOR} NO SCROLL CURSOR FOR {query}")
        } else {
            format!("BEGIN; DECLARE {CURSOR} NO SCROLL CURSOR FOR {query}")
        };
        self.conn.execute(declare.as_str()).await?;
        self.cursor_open = true;
        self.cursor_used = Instant::now();
        self.idle_closed = false;
        Ok(())
    }

    async fn run_direct(&mut self, sql: &str, limit: usize, start: Instant) -> AppResult<QueryResult> {
        run_direct_on(&mut self.conn, sql, limit, start).await
    }

    /// A change with its undo snapshot right before it, in one transaction (Session::run_with_snapshot)
    async fn run_with_snapshot(&mut self, snap: &str, sql: &str, limit: usize) -> (AppResult<QueryResult>, AppResult<QueryResult>) {
        self.close_cursor().await;
        let own = !self.user_tx();
        let began = !self.in_tx;
        if began {
            if let Err(e) = self.conn.execute("BEGIN").await {
                return (Err(AppError::Msg(e.to_string())), Err(e.into()));
            }
            self.in_tx = true;
        }
        let shot = pg_snapshot(&mut self.conn, snap).await;
        // Cancelled · timed out while locking the rows: don't run the change (the savepoint kept the user's transaction as it was)
        if let Err(e) = &shot
            && crate::audit::stops_change(e)
        {
            if began {
                let _ = self.rollback_tx().await;
            }
            return (Err(AppError::Msg(e.to_string())), Err(crate::audit::stopped(e, false)));
        }
        let mut r = self.run(sql, limit).await;
        if own {
            if r.is_ok()
                && let Err(e) = self.commit().await
            {
                r = Err(e);
            }
            match &mut r {
                Ok(q) => q.in_transaction = false,
                // The change or its COMMIT failed: nothing of it stays, and no transaction is left open
                Err(_) => {
                    let _ = self.rollback_tx().await;
                    self.in_tx = false;
                }
            }
        }
        (shot, r)
    }

    /// Fetch the next `limit` rows from the open cursor.
    /// If the cursor was closed for being unused, reopen the same statement after the rows already read (OFFSET) (without ORDER BY, the DB decides the order)
    pub async fn fetch(&mut self, limit: usize) -> AppResult<QueryResult> {
        if !self.cursor_open && self.idle_closed {
            let q = format!("SELECT * FROM (\n{}\n) AS qurry_page OFFSET {}", strip_trailing_semicolon(&self.cursor_sql), self.fetched);
            if let Err(e) = self.declare(&q).await {
                self.rollback().await;
                self.idle_closed = false;
                return Err(AppError::Msg(format!("The result was closed after {} s without use, and reopening it failed ({e}). Run the query again to load more rows.", CURSOR_IDLE.as_secs())));
            }
        }
        if !self.cursor_open {
            return Err(AppError::Msg("No more rows".into()));
        }
        self.cursor_used = Instant::now();
        let start = Instant::now();
        let rows: Vec<PgRow> = match sqlx::query(&format!("FETCH {limit} FROM {CURSOR}")).persistent(false).fetch_all(&mut *self.conn).await {
            Ok(r) => r,
            Err(e) => {
                self.rollback().await;
                return Err(e.into());
            }
        };
        if self.columns.is_empty() {
            self.columns = match rows.first() {
                Some(r) => pg_columns(r),
                None => match (&mut *self.conn).describe(strip_trailing_semicolon(&self.cursor_sql)).await {
                    Ok(d) => d
                        .columns()
                        .iter()
                        .map(|c| ColumnInfo { name: c.name().to_string(), type_name: c.type_info().name().to_string() })
                        .collect(),
                    Err(_) => Vec::new(),
                },
            };
        }
        let done = rows.len() < limit;
        self.fetched += rows.len();
        let data: Vec<Vec<Value>> = rows.iter().map(|r| (0..r.len()).map(|i| pg_value(r, i)).collect()).collect();
        if done {
            self.close_cursor().await;
        }
        Ok(query_result(self.columns.clone(), data, 0, !done, start, true))
    }
}

/// Run a statement on the connection (no cursor)
async fn run_direct_on(conn: &mut PgConnection, sql: &str, limit: usize, start: Instant) -> AppResult<QueryResult> {
    let mut columns = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut affected = 0u64;
    let mut truncated = false;
    let mut has_rs = false;
    {
        #[allow(deprecated)]
        let mut stream = match is_generic_plan(sql) {
            true => sqlx::raw_sql(sql).fetch_many(&mut *conn),
            false => sqlx::query(sql).persistent(false).fetch_many(&mut *conn),
        };
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Left(done) => affected += done.rows_affected(),
                Either::Right(row) => {
                    has_rs = true;
                    if columns.is_empty() {
                        columns = pg_columns(&row);
                    }
                    if rows.len() >= limit {
                        truncated = true;
                        break;
                    }
                    rows.push((0..row.len()).map(|i| pg_value(&row, i)).collect());
                }
            }
        }
    }
    Ok(query_result(columns, rows, affected, truncated, start, has_rs))
}

/// Undo snapshot inside the open transaction, under a savepoint: if it fails, the transaction carries on as it was (PostgreSQL would abort it)
pub(crate) async fn pg_snapshot(conn: &mut PgConnection, sql: &str) -> AppResult<QueryResult> {
    conn.execute("SAVEPOINT qurry_snap").await?;
    let mut r = run_direct_on(conn, sql, crate::audit::UNDO_MAX_ROWS + 1, Instant::now()).await;
    // A row lock needs the UPDATE privilege: a role with only SELECT and DELETE gets the snapshot without the lock (as before rows were locked)
    if let Err(AppError::Db(sqlx::Error::Database(e))) = &r
        && e.code().as_deref() == Some("42501")
        && let Some(plain) = crate::audit::pg_unlocked(sql)
    {
        conn.execute("ROLLBACK TO SAVEPOINT qurry_snap").await?;
        r = run_direct_on(conn, &plain, crate::audit::UNDO_MAX_ROWS + 1, Instant::now()).await;
    }
    conn.execute(if r.is_ok() { "RELEASE SAVEPOINT qurry_snap" } else { "ROLLBACK TO SAVEPOINT qurry_snap" }).await?;
    r
}

/// Console tab session (per DB kind)
pub enum Session {
    Pg(PgSession),
    My(crate::mysql::MySession),
    Redis(crate::redisdb::RedisConn),
    /// Driver plugin: all at once (up to limit), without server cursors or manual transactions
    Plugin(std::sync::Arc<crate::plugin::PluginConn>),
}

impl Session {
    pub async fn open(pool: &crate::db::Pool) -> AppResult<Self> {
        dispatch!(pool, b => b.open_session().await)
    }
    /// Backend id for cancelling (PG pid / MySQL connection id)
    pub fn backend_id(&self) -> i64 {
        match self {
            Session::Pg(s) => s.pid as i64,
            Session::My(s) => s.id,
            Session::Redis(_) | Session::Plugin(_) => 0,
        }
    }
    pub async fn run(&mut self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        match self {
            Session::Pg(s) => s.run(sql, limit).await,
            Session::My(s) => s.run(sql, limit).await,
            Session::Redis(m) => crate::redisdb::run(m, sql, limit).await,
            Session::Plugin(c) => c.query(sql, limit).await,
        }
    }
    /// Run a change with its undo snapshot (audit::UndoPlan, a SELECT … FOR UPDATE) right before it in the same transaction, so nobody changes the rows in between.
    /// In the user's transaction (Manual · BEGIN) both join it; otherwise they get one of their own, committed right after the change (rolled back if it fails).
    /// A failing snapshot leaves the transaction as it was, and the change still runs (without undo) — unless it was cancelled, timed out or
    /// deadlocked (audit::stops_change): then the change doesn't run either. → (snapshot, change)
    pub async fn run_with_snapshot(&mut self, snap: &str, sql: &str, limit: usize) -> (AppResult<QueryResult>, AppResult<QueryResult>) {
        match self {
            Session::Pg(s) => s.run_with_snapshot(snap, sql, limit).await,
            Session::My(s) => s.run_with_snapshot(snap, sql, limit).await,
            _ => (Err(AppError::Msg("Undo is kept for PostgreSQL and MySQL only".into())), self.run(sql, limit).await),
        }
    }
    pub async fn fetch(&mut self, limit: usize) -> AppResult<QueryResult> {
        match self {
            Session::Pg(s) => s.fetch(limit).await,
            Session::My(s) => s.fetch(limit).await,
            Session::Redis(_) | Session::Plugin(_) => Err(AppError::Msg("No more rows".into())),
        }
    }
    pub async fn set_manual(&mut self, manual: bool) -> AppResult<()> {
        match self {
            Session::Pg(s) => s.set_manual(manual).await,
            Session::My(s) => s.set_manual(manual).await,
            Session::Redis(_) => Err(AppError::Msg("Manual transactions are not supported for Redis (use MULTI / EXEC)".into())),
            Session::Plugin(c) => Err(AppError::Msg(format!("Manual transactions are not supported by the {} driver yet (use BEGIN / COMMIT in the console)", c.driver))),
        }
    }
    pub async fn commit(&mut self) -> AppResult<()> {
        match self {
            Session::Pg(s) => s.commit().await,
            Session::My(s) => s.commit().await,
            Session::Redis(_) | Session::Plugin(_) => Ok(()),
        }
    }
    pub async fn rollback_tx(&mut self) -> AppResult<()> {
        match self {
            Session::Pg(s) => s.rollback_tx().await,
            Session::My(s) => s.rollback_tx().await,
            Session::Redis(_) | Session::Plugin(_) => Ok(()),
        }
    }
    pub async fn close_cursor(&mut self) {
        match self {
            Session::Pg(s) => s.close_cursor().await,
            Session::My(s) => s.close_cursor(),
            Session::Redis(_) | Session::Plugin(_) => {}
        }
    }
    /// Whether there are uncommitted changes
    pub fn in_tx(&self) -> bool {
        match self {
            Session::Pg(s) => s.in_tx(),
            Session::My(s) => s.in_tx,
            Session::Redis(_) | Session::Plugin(_) => false,
        }
    }
    /// On tab close: roll back the open transaction, reset session settings and return the connection to the pool
    pub async fn discard(&mut self) {
        match self {
            Session::Pg(s) => s.discard().await,
            Session::My(s) => s.discard().await,
            Session::Redis(_) | Session::Plugin(_) => {}
        }
    }
    pub async fn close_idle_cursor(&mut self, idle: std::time::Duration) -> bool {
        match self {
            Session::Pg(s) => s.close_idle_cursor(idle).await,
            _ => false,
        }
    }
    pub async fn ping(&mut self) {
        match self {
            Session::Pg(s) => s.ping().await,
            Session::My(s) => s.ping().await,
            Session::Redis(m) => crate::redisdb::ping(m).await,
            Session::Plugin(_) => {}
        }
    }
}

/// If a table edit's UPDATE/DELETE hits 0 or multiple rows (changed by someone else or duplicate keys), roll back everything
pub fn check_exact_one(st: &str, n: u64, exact_one: bool) -> AppResult<()> {
    let s = strip_comments_lead(st);
    if exact_one && (s.starts_with("update") || s.starts_with("delete")) && n != 1 {
        return Err(AppError::Msg(format!(
            "Expected to change exactly 1 row but changed {n} — nothing was saved (the row may have been changed or deleted by someone else; reload and try again)\n→ {st}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect, Pool};

    /// MCP execute path: the server refuses two statements in one prepared item (nothing runs)
    #[tokio::test]
    #[ignore]
    async fn prepared_batch_refuses_multiple_statements() {
        let Some(p) = pool().await else { return };
        sqlx::raw_sql("drop table if exists qurry_prep; create table qurry_prep (a int)").execute(&p).await.unwrap();
        let e = p.execute_batch(&["insert into qurry_prep values (1); drop table qurry_prep".into()], false, true, &[]).await.unwrap_err().to_string();
        assert!(e.contains("multiple commands"), "{e}");
        assert_eq!(p.execute_batch(&["insert into qurry_prep values (1)".into()], false, true, &[]).await.unwrap().0, 1);
        sqlx::raw_sql("drop table qurry_prep").execute(&p).await.unwrap();
    }

    /// QURRY_TEST_PG=host:port:user:db[:password] (crate::testenv)
    async fn pool() -> Option<PgPool> {
        let s = crate::testenv::pg()?;
        let Pool::Pg(p) = connect(&s.cfg, s.password.as_deref()).await.unwrap() else { unreachable!() };
        Some(p)
    }

    /// In Auto mode, a cursor (SELECT) does not commit a transaction the user opened (a single SELECT used to finalize an UPDATE)
    #[tokio::test]
    #[ignore]
    async fn auto_mode_user_transaction_is_not_committed_by_select() {
        let Some(p) = pool().await else { return };
        sqlx::query("create table if not exists public.dbtool_tx_t(id int primary key, v text)").execute(&p).await.unwrap();
        sqlx::query("delete from public.dbtool_tx_t").execute(&p).await.unwrap();
        sqlx::query("insert into public.dbtool_tx_t values (1, 'a')").execute(&p).await.unwrap();
        let seen = || async { sqlx::query_scalar::<_, String>("select v from public.dbtool_tx_t where id = 1").fetch_one(&p).await.unwrap() };
        let mut s = PgSession::open(&p).await.unwrap();
        s.run("begin", 500).await.unwrap();
        s.run("update public.dbtool_tx_t set v = 'changed' where id = 1", 500).await.unwrap();
        // A fully read SELECT, a multi-page SELECT and a failing SELECT all leave the transaction alone
        let r = s.run("select * from public.dbtool_tx_t", 500).await.unwrap();
        assert!(r.in_transaction);
        s.run("select g from generate_series(1, 1200) g", 500).await.unwrap();
        assert!(s.run("select * from no_such_table_xyz", 500).await.is_err());
        assert_eq!(seen().await, "a", "nothing committed yet");
        assert!(s.set_manual(true).await.is_ok() && s.set_manual(false).await.is_err(), "pending changes block switching to Auto");
        s.run("rollback", 500).await.unwrap();
        assert_eq!(seen().await, "a", "rolled back");
        // With uncommitted changes in Manual, switching to Auto is not allowed
        s.set_manual(true).await.unwrap();
        s.run("update public.dbtool_tx_t set v = 'm' where id = 1", 500).await.unwrap();
        assert!(s.set_manual(false).await.is_err());
        s.rollback_tx().await.unwrap();
        s.set_manual(false).await.unwrap();
        assert_eq!(seen().await, "a");
        // Saving table edits: UPDATE/DELETE hitting 0 or multiple rows rolls back everything
        let none = p.execute_batch(&["insert into public.dbtool_tx_t values (2, 'b')".into(), "update public.dbtool_tx_t set v = 'x' where id = 99".into()], true, false, &[]).await;
        assert!(none.is_err());
        let many = p.execute_batch(&["update public.dbtool_tx_t set v = 'x' where id > 0".into()], true, false, &[]).await;
        assert!(many.is_ok(), "one row matches");
        sqlx::query("insert into public.dbtool_tx_t values (3, 'c')").execute(&p).await.unwrap();
        assert!(p.execute_batch(&["delete from public.dbtool_tx_t where id > 0".into()], true, false, &[]).await.is_err());
        let n: i64 = sqlx::query_scalar("select count(*) from public.dbtool_tx_t").fetch_one(&p).await.unwrap();
        assert_eq!(n, 2, "row 2 insert was rolled back, rows 1 and 3 remain");
        sqlx::query("drop table public.dbtool_tx_t").execute(&p).await.unwrap();
    }

    #[test]
    fn select_like() {
        assert!(is_select_like("  -- c\n /* x */ SELECT 1"));
        assert!(is_select_like("with a as (select 1) select * from a"));
        assert!(!is_select_like("update t set x = 1"));
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_after_idle_close() {
        let Some(p) = pool().await else { return };
        let mut s = PgSession::open(&p).await.unwrap();
        let r1 = s.run("select g from generate_series(1,1200) g order by g -- tail comment", 500).await.unwrap();
        assert_eq!((r1.rows.len(), r1.truncated), (500, true));
        // After it is closed for being unused (the transaction ends too)
        assert!(s.close_idle_cursor(std::time::Duration::ZERO).await);
        let state: String = sqlx::query_scalar("select state from pg_stat_activity where pid = $1").bind(s.pid).fetch_one(&p).await.unwrap();
        assert_eq!(state, "idle");
        // Loading more continues from the row after the ones read
        let r2 = s.fetch(500).await.unwrap();
        assert_eq!((r2.rows.len(), r2.truncated, r2.columns.len()), (500, true, 1));
        assert_eq!(r2.rows[0][0], Value::from(501));
        assert!(s.close_idle_cursor(std::time::Duration::ZERO).await);
        let r3 = s.fetch(500).await.unwrap();
        assert_eq!((r3.rows.len(), r3.truncated), (200, false));
        assert_eq!(r3.rows[0][0], Value::from(1001));
        assert!(s.fetch(500).await.is_err(), "no more rows");
        // If another statement ran in between, the old result is not reopened
        s.run("select g from generate_series(1,1200) g", 500).await.unwrap();
        assert!(s.close_idle_cursor(std::time::Duration::ZERO).await);
        s.run("set application_name = 'x'", 500).await.unwrap();
        assert!(s.fetch(500).await.is_err());
    }

    #[tokio::test]
    #[ignore]
    async fn cursor_paging_cancel_batch() {
        let Some(p) = pool().await else { return };
        let mut s = PgSession::open(&p).await.unwrap();

        // Cursor paging: 1200 rows → 500 + 500 + 200
        let r1 = s.run("select g, 'v'||g as t, g::numeric/3 as n from generate_series(1,1200) g;", 500).await.unwrap();
        assert_eq!((r1.rows.len(), r1.truncated), (500, true));
        assert_eq!(r1.rows[0][0], Value::from(1));
        println!("first row via cursor: {:?}", r1.rows[0]);
        let r2 = s.fetch(500).await.unwrap();
        assert_eq!((r2.rows.len(), r2.truncated), (500, true));
        assert_eq!(r2.rows[0][0], Value::from(501));
        let r3 = s.fetch(500).await.unwrap();
        assert_eq!((r3.rows.len(), r3.truncated), (200, false));
        assert!(!s.has_more());

        // A 0-row SELECT must still return columns
        let e = s.run("select relname from pg_class where false", 500).await.unwrap();
        assert_eq!((e.columns.len(), e.rows.len()), (1, 0));

        // Session persists: SET applies to the next query too
        s.run("set application_name = 'dbtool_sess'", 500).await.unwrap();
        let a = s.run("show application_name", 500).await.unwrap();
        assert_eq!(a.rows[0][0], Value::from("dbtool_sess"));

        // DML / DDL run normally
        s.run("create temp table t_sess(id int primary key, name text)", 500).await.unwrap();
        let ins = s.run("insert into t_sess values (1,'a'),(2,'b')", 500).await.unwrap();
        assert_eq!(ins.rows_affected, 2);

        // The session must stay usable after an error
        assert!(s.run("select * from no_such_tbl", 500).await.is_err());
        assert_eq!(s.run("select 1", 500).await.unwrap().rows[0][0], Value::from(1));

        // Cancel: run pg_sleep in another task → cancel
        let pid = s.pid;
        let p2 = p.clone();
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            p2.cancel(pid as i64).await.unwrap()
        });
        let t0 = Instant::now();
        let res = s.run("select pg_sleep(10)", 500).await;
        assert!(canceller.await.unwrap());
        assert!(res.is_err() && t0.elapsed().as_secs() < 5, "cancel failed: {res:?}");
        println!("cancelled after {:?}: {}", t0.elapsed(), res.unwrap_err());
        assert_eq!(s.run("select 2", 500).await.unwrap().rows[0][0], Value::from(2));

        // Tx: Manual — invisible to other connections before commit, gone after Roll back
        sqlx::query("create table if not exists public.dbtool_tx_t(id int primary key)").execute(&p).await.unwrap();
        sqlx::query("truncate public.dbtool_tx_t").execute(&p).await.unwrap();
        s.set_manual(true).await.unwrap();
        let ins = s.run("insert into public.dbtool_tx_t values (1)", 500).await.unwrap();
        assert!(ins.in_transaction);
        let seen: i64 = sqlx::query_scalar("select count(*) from public.dbtool_tx_t").fetch_one(&p).await.unwrap();
        assert_eq!(seen, 0, "uncommitted row must not be visible");
        // Inside a Manual transaction, SELECT (cursor) sees its own changes too
        let mine = s.run("select count(*) from public.dbtool_tx_t", 500).await.unwrap();
        assert_eq!(mine.rows[0][0], Value::from(1));
        s.rollback_tx().await.unwrap();
        let after_rb = s.run("select count(*) from public.dbtool_tx_t", 500).await.unwrap();
        assert_eq!(after_rb.rows[0][0], Value::from(0));
        s.run("insert into public.dbtool_tx_t values (2)", 500).await.unwrap();
        s.commit().await.unwrap();
        let committed: i64 = sqlx::query_scalar("select count(*) from public.dbtool_tx_t").fetch_one(&p).await.unwrap();
        assert_eq!(committed, 1);
        // Statements DECLARE cannot take do not break the manual transaction either
        let w = s.run("with x as (delete from public.dbtool_tx_t where id = 2 returning id) select count(*) from x", 500).await.unwrap();
        assert_eq!(w.rows[0][0], Value::from(1));
        s.rollback_tx().await.unwrap();
        s.set_manual(false).await.unwrap();
        sqlx::query("drop table public.dbtool_tx_t").execute(&p).await.unwrap();

        // Batch edit: roll back everything on failure
        sqlx::query("create table if not exists public.dbtool_batch_t(id int primary key, name text)").execute(&p).await.unwrap();
        sqlx::query("truncate public.dbtool_batch_t").execute(&p).await.unwrap();
        let ok = p.execute_batch(&["insert into public.dbtool_batch_t values (1,'a')".into(), "update public.dbtool_batch_t set name='b' where id=1".into()], true, false, &[]).await.unwrap();
        assert_eq!(ok.0, 2);
        let bad = p.execute_batch(&["insert into public.dbtool_batch_t values (2,'c')".into(), "insert into public.dbtool_batch_t values (1,'dup')".into()], true, false, &[]).await;
        assert!(bad.is_err());
        let n: i64 = sqlx::query_scalar("select count(*) from public.dbtool_batch_t").fetch_one(&p).await.unwrap();
        assert_eq!(n, 1);
        sqlx::query("drop table public.dbtool_batch_t").execute(&p).await.unwrap();
    }

    /// A failing undo snapshot leaves the transaction usable: the change still runs and commits (Auto), or joins the user's transaction (Manual)
    async fn failed_snapshot_keeps_the_change(pool: Pool, schema: &str, drop: &str) {
        let mut s = Session::open(&pool).await.unwrap();
        let t = "audit_hardening_s.qurry_snap_fail";
        for q in [schema, &format!("drop table if exists {t}"), &format!("create table {t} (id int primary key, v int)"), &format!("insert into {t} values (1, 1)")] {
            s.run(q, 1).await.unwrap();
        }
        let v = |r: QueryResult| r.rows[0][0].to_string().trim_matches('"').to_string();
        let bad = "select * from qurry_no_such_table for update";
        let (shot, r) = s.run_with_snapshot(bad, &format!("update {t} set v = 2 where id = 1"), 10).await;
        assert!(shot.is_err());
        assert_eq!(r.unwrap().rows_affected, 1);
        assert!(!s.in_tx());
        s.set_manual(true).await.unwrap();
        let (shot, r) = s.run_with_snapshot(bad, &format!("update {t} set v = 3 where id = 1"), 10).await;
        assert!(shot.is_err());
        assert!(r.unwrap().in_transaction);
        assert!(s.in_tx());
        s.rollback_tx().await.unwrap();
        s.set_manual(false).await.unwrap();
        assert_eq!(v(s.run(&format!("select v from {t}"), 1).await.unwrap()), "2");
        // A working snapshot sees the row as it is in the transaction, which commits after the change
        let (shot, r) = s.run_with_snapshot(&format!("select v from {t} where id = 1 for update"), &format!("update {t} set v = 4 where id = 1"), 10).await;
        assert_eq!((v(shot.unwrap()), r.unwrap().in_transaction, s.in_tx()), ("2".into(), false, false));
        assert_eq!(v(s.run(&format!("select v from {t}"), 1).await.unwrap()), "4");
        s.run(drop, 1).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_failed_snapshot_keeps_the_change() {
        let Some(p) = pool().await else { return };
        failed_snapshot_keeps_the_change(Pool::Pg(p), "create schema if not exists audit_hardening_s", "drop schema audit_hardening_s cascade").await;
    }

    /// QURRY_TEST_MY=host:port:user:password:db
    #[tokio::test]
    #[ignore]
    async fn mysql_failed_snapshot_keeps_the_change() {
        let Some(s) = crate::testenv::my() else { return };
        failed_snapshot_keeps_the_change(connect(&s.cfg, s.password.as_deref()).await.unwrap(), "create database if not exists audit_hardening_s", "drop database audit_hardening_s").await;
    }
}
