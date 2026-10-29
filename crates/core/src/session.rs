//! Each console tab gets its own dedicated connection (session).
//! - SELECT-like statements fetch 500 rows at a time via a server cursor (DECLARE/FETCH) → "load more"
//! - The backend PID is known, so a running query can be cancelled with pg_cancel_backend
use crate::db::{pg_columns, pg_value, query_result, ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use futures::TryStreamExt;
use serde_json::Value;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgRow};
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
        self.conn.execute("COMMIT").await?;
        self.in_tx = false;
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
        let mut columns = Vec::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut affected = 0u64;
        let mut truncated = false;
        let mut has_rs = false;
        {
            #[allow(deprecated)]
            let mut stream = sqlx::query(sql).persistent(false).fetch_many(&mut *self.conn);
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
        Ok(match pool {
            crate::db::Pool::Pg(p) => Session::Pg(PgSession::open(p).await?),
            crate::db::Pool::My(p) => Session::My(crate::mysql::MySession::open(p).await?),
            crate::db::Pool::Redis(m, _) => Session::Redis(m.clone()),
            crate::db::Pool::Plugin(c) => Session::Plugin(c.clone()),
        })
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

/// Cancel the backend's running query from another connection.
pub async fn cancel(pool: &PgPool, pid: i32) -> AppResult<bool> {
    Ok(sqlx::query_scalar("select pg_cancel_backend($1)").bind(pid).fetch_one(pool).await?)
}

/// Apply data edits in one transaction. If any fails, roll back everything.
/// Statements in one transaction (roll back everything if any fails). exact_one = table edits: each UPDATE/DELETE must hit exactly 1 row
pub async fn execute_batch(pool: &PgPool, statements: &[String], exact_one: bool) -> AppResult<u64> {
    let mut tx = pool.begin().await?;
    let mut affected = 0;
    for st in statements {
        let n = tx.execute(st.as_str()).await.map_err(|e| AppError::Msg(format!("{e}\n→ {st}")))?.rows_affected();
        check_exact_one(st, n, exact_one)?;
        affected += n;
    }
    tx.commit().await?;
    Ok(affected)
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
    use crate::store::{ConnectionConfig, DbKind};

    /// Test DB: local PostgreSQL by default. Override with QURRY_TEST_PG=host:port:user:db (e.g. a container)
    async fn pool() -> PgPool {
        let env = std::env::var("QURRY_TEST_PG").ok();
        let part = |i: usize, d: String| env.as_ref().and_then(|e| e.split(':').nth(i).map(String::from)).unwrap_or(d);
        let cfg = ConnectionConfig {
            id: "t".into(), name: "t".into(), kind: DbKind::Postgres, host: part(0, "localhost".into()), port: part(1, "5432".into()).parse().unwrap(),
            user: part(2, std::env::var("USER").unwrap()), database: part(3, String::new()), color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        let Pool::Pg(p) = connect(&cfg, None).await.unwrap() else { unreachable!() };
        p
    }

    /// In Auto mode, a cursor (SELECT) does not commit a transaction the user opened (a single SELECT used to finalize an UPDATE)
    #[tokio::test]
    #[ignore]
    async fn auto_mode_user_transaction_is_not_committed_by_select() {
        let p = pool().await;
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
        let none = execute_batch(&p, &["insert into public.dbtool_tx_t values (2, 'b')".into(), "update public.dbtool_tx_t set v = 'x' where id = 99".into()], true).await;
        assert!(none.is_err());
        let many = execute_batch(&p, &["update public.dbtool_tx_t set v = 'x' where id > 0".into()], true).await;
        assert!(many.is_ok(), "one row matches");
        sqlx::query("insert into public.dbtool_tx_t values (3, 'c')").execute(&p).await.unwrap();
        assert!(execute_batch(&p, &["delete from public.dbtool_tx_t where id > 0".into()], true).await.is_err());
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
        let p = pool().await;
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
        let p = pool().await;
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
            cancel(&p2, pid).await.unwrap()
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
        let ok = execute_batch(&p, &["insert into public.dbtool_batch_t values (1,'a')".into(), "update public.dbtool_batch_t set name='b' where id=1".into()], true).await.unwrap();
        assert_eq!(ok, 2);
        let bad = execute_batch(&p, &["insert into public.dbtool_batch_t values (2,'c')".into(), "insert into public.dbtool_batch_t values (1,'dup')".into()], true).await;
        assert!(bad.is_err());
        let n: i64 = sqlx::query_scalar("select count(*) from public.dbtool_batch_t").fetch_one(&p).await.unwrap();
        assert_eq!(n, 1);
        sqlx::query("drop table public.dbtool_batch_t").execute(&p).await.unwrap();
    }
}
