//! Each console tab gets its own dedicated connection (session).
//! - SELECT-like statements fetch 500 rows at a time via a server cursor (DECLARE/FETCH) → "load more"
//! - The backend PID is known, so a running query can be cancelled with pg_cancel_backend
use crate::backend::{Backend, dispatch};
use crate::db::{pg_columns, pg_origins, pg_values, query_result, ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use futures::TryStreamExt;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnection, PgPool, PgRow};
use sqlx::{Column, Either, Executor, Postgres, Row, TypeInfo};
use std::time::Instant;

const CURSOR: &str = "dbtool_cur";
/// When switching from Manual → Auto is blocked
pub const PENDING_TX: &str = "There are uncommitted changes — commit or roll back first";
/// Explain Analyse runs the statement between these and rolls it back (Session::explain_wrap): in a transaction of its own, or in a
/// savepoint of the one that is open
pub const EXPLAIN_BEGIN: [&str; 2] = ["BEGIN", "ROLLBACK"];
pub const EXPLAIN_SAVEPOINT: [&str; 2] = ["SAVEPOINT qurry_explain", "ROLLBACK TO SAVEPOINT qurry_explain"];
/// A result grid's edits in the console's transaction (Session::submit): set, undo when one fails, release
const SUBMIT_SAVEPOINT: [&str; 3] = ["SAVEPOINT qurry_submit", "ROLLBACK TO SAVEPOINT qurry_submit", "RELEASE SAVEPOINT qurry_submit"];
/// A COMMIT of a PostgreSQL transaction an earlier error aborted: the server rolled it back
const ABORTED_COMMIT: &str =
    "Not committed: an earlier statement in this transaction failed, so PostgreSQL rolled back the whole transaction and none of its changes were saved. Run the changes again (without the failing statement).";

pub struct PgSession {
    conn: PoolConnection<Postgres>,
    pub pid: i32,
    cursor_open: bool,
    cursor_sql: String,
    columns: Vec<ColumnInfo>,
    /// Where the cursor's columns come from (db::pg_origins), for editing the result (crate::editable). Empty for a result read without a cursor
    pub(crate) origins: Vec<Option<(u32, i16)>>,
    /// Tx: Manual mode (DataGrip). Statements accumulate in a transaction until Commit / Roll back
    manual: bool,
    /// Whether a transaction is open in Manual mode
    pub in_tx: bool,
    /// A change ran in the open transaction (audit::is_change, as MySession::changed)
    changed: bool,
    /// Production (Session::set_reads_outside_tx): in Manual mode a plain read before the transaction is open runs as in Auto
    reads_outside_tx: bool,
    /// Last time the cursor was used (closed when unused for long)
    cursor_used: Instant,
    /// Whether the cursor was closed for being unused (reopened on load more)
    idle_closed: bool,
    /// Rows read through the cursor for the current result (OFFSET when reopening)
    fetched: usize,
    /// The session's time zone for timestamptz (db::pg_values): read at the first such result, again after a statement that may
    /// change it (zone_may_change)
    zone: Option<crate::zone::Zone>,
}

/// Why loading more needs a new session: saving the source's settings, Disconnect and the idle drop close the tab sessions (app::drop_pool)
pub const RECONNECTED: &str = "The data source reconnected since this result was read (settings saved, disconnected or idle)";

/// A result with more rows to load, kept outside its session so loading more can reopen it on a new one after the session was
/// closed (AppState::fetch_more)
#[derive(Clone, Debug)]
pub struct More {
    pub sql: String,
    /// Rows read so far (the OFFSET to reopen at)
    pub fetched: usize,
    /// Read in the user's transaction (Manual, or BEGIN in Auto): it ended with the old session, so the rows may not line up
    pub tx: bool,
    /// The console's schema it ran in: PostgreSQL search_path, MySQL database() ("" when none is selected). None: it couldn't be read
    pub schema: Option<String>,
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

/// Whether a COMMIT / ROLLBACK / END / ABORT (s = strip_comments_lead(sql)) ends with AND CHAIN, which opens a new transaction
/// right away: `commit and chain`, `rollback work and chain`, `end transaction /* x */ and chain` (not AND NO CHAIN)
pub(crate) fn chains(s: &str) -> bool {
    let mut rest = s;
    let mut words = Vec::new();
    while words.len() < 4 {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("--").or_else(|| rest.strip_prefix('#')) {
            rest = r.split_once('\n').map_or("", |x| x.1);
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r.split_once("*/").map_or("", |x| x.1);
        } else {
            let n = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
            if n == 0 {
                break;
            }
            words.push(&rest[..n]);
            rest = &rest[n..];
        }
    }
    let after = if matches!(words.get(1), Some(&"work" | &"transaction")) { 2 } else { 1 };
    matches!(words.get(after..), Some(["and", "chain", ..]))
}

/// Whether a statement may change the session's TimeZone: `SET [LOCAL] TIME ZONE`, `SET timezone`, `set_config('TimeZone', …)`,
/// `RESET`, `DISCARD ALL`, the end of a transaction (a SET LOCAL, or a SET it rolls back), a procedure or DO block. Words in a
/// comment or a string count too: the zone is then only read again
fn zone_may_change(sql: &str) -> bool {
    let s = strip_comments_lead(sql);
    let word = s.split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("");
    ["rollback", "commit", "end", "abort", "call", "do", "execute"].contains(&word)
        || ["zone", "set_config", "reset", "discard"].iter().any(|k| s.contains(k))
}

pub fn is_select_like(sql: &str) -> bool {
    let s = strip_comments_lead(sql);
    ["select", "with", "values", "table", "("].iter().any(|k| s.starts_with(k))
}

/// A read that takes no row locks (guard::is_read: no side-effect call either), and no transaction statement or SET: on Production
/// it doesn't open Manual mode's transaction (Session::set_reads_outside_tx)
pub(crate) fn plain_read(sql: &str, kind: crate::store::DbKind) -> bool {
    let s = strip_comments_lead(sql);
    let word = s.split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("");
    let tx = ["begin", "start", "commit", "rollback", "end", "abort", "savepoint", "release", "set", "xa"].contains(&word);
    !tx && crate::guard::is_read(sql, kind) && !crate::guard::locks_rows(sql, kind)
}

/// A read run outside the console's transaction (Session::set_reads_outside_tx) runs in a read-only one, so a function in it that
/// writes can't commit by itself: the server refuses it (PostgreSQL 25006, MySQL 1792), and the error says how to run it
pub(crate) fn read_first_refused(e: AppError) -> AppError {
    let code = match &e {
        AppError::Db(sqlx::Error::Database(d)) => d.code().map(|c| c.to_string()),
        _ => None,
    };
    if code.as_deref() == Some("25006") || crate::mysql::error_number(&e) == Some(1792) {
        AppError::Msg(format!("{e}\n{}", qurry_shared::errors::READ_FIRST_WROTE))
    } else {
        e
    }
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
        Ok(Self { conn, pid, cursor_open: false, cursor_sql: String::new(), columns: Vec::new(), origins: Vec::new(), manual: false, in_tx: false, changed: false, reads_outside_tx: false, cursor_used: Instant::now(), idle_closed: false, fetched: 0, zone: None })
    }

    /// keep-alive: so idle session connections are not dropped
    pub async fn ping(&mut self) {
        let _ = self.conn.execute("select 1").await;
    }

    #[allow(dead_code)]
    pub fn has_more(&self) -> bool {
        self.cursor_open
    }

    /// Whether a user-opened transaction exists (Manual mode once a statement opened it, or BEGIN run directly in Auto). The cursor
    /// then only uses savepoints and never ends the user's transaction with COMMIT/ROLLBACK. A read that runs before it (Production,
    /// reads_outside_tx) has a cursor transaction of its own, as in Auto
    fn user_tx(&self) -> bool {
        self.in_tx
    }

    pub async fn close_cursor(&mut self) {
        if self.cursor_open {
            // A cursor inside the user's transaction is only closed (committing would silently finalize the user's changes)
            let sql = if self.user_tx() { "CLOSE dbtool_cur" } else { "CLOSE dbtool_cur; COMMIT" };
            let _ = self.conn.execute(sql).await;
            self.cursor_open = false;
        }
    }

    /// In Auto mode, close cursors unused for long and end the transaction (Manual is left alone since it is the user's transaction).
    /// Within 5s, like the keep-alive ping: through a tunnel that stopped forwarding it would wait forever, holding the session
    /// and housekeeping (keep-alive and auto-disconnect of every source)
    pub async fn close_idle_cursor(&mut self, idle: std::time::Duration) -> bool {
        if self.cursor_open && !self.user_tx() && self.cursor_used.elapsed() >= idle {
            if tokio::time::timeout(std::time::Duration::from_secs(5), self.close_cursor()).await.is_err() {
                return false;
            }
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
        self.refuse_aborted_commit().await?;
        let r = self.conn.execute("COMMIT").await;
        // A failed COMMIT (e.g. a deferred foreign key) ends the transaction too: PostgreSQL rolls it back
        self.in_tx = false;
        // A SET LOCAL TIME ZONE (or a rolled-back SET) ends with it
        self.zone = None;
        r?;
        Ok(())
    }

    pub async fn rollback_tx(&mut self) -> AppResult<()> {
        self.close_cursor().await;
        self.zone = None;
        self.conn.execute("ROLLBACK").await?;
        self.in_tx = false;
        Ok(())
    }

    /// PostgreSQL answers COMMIT of a transaction an error aborted with ROLLBACK, not an error: end it so and say so (an error,
    /// so the console and the Production log don't read it as committed)
    async fn refuse_aborted_commit(&mut self) -> AppResult<()> {
        if !self.in_tx {
            return Ok(());
        }
        match self.conn.execute("SELECT 1").await {
            Err(sqlx::Error::Database(d)) if d.code().as_deref() == Some("25P02") => {
                self.conn.execute("ROLLBACK").await?;
                self.in_tx = false;
                Err(AppError::Msg(ABORTED_COMMIT.into()))
            }
            // The session is gone (the server ended it after it sat idle in the transaction, a lost connection): the COMMIT isn't
            // sent, and the error says why (app::recover)
            Err(e) => {
                let e = AppError::Db(e);
                if crate::app::is_conn_lost(&e) { Err(e) } else { Ok(()) }
            }
            Ok(_) => Ok(()),
        }
    }

    /// Sync the state when the user runs BEGIN / COMMIT / ROLLBACK directly (`ok`: it ran without error). One that ran with
    /// AND CHAIN opens the next transaction at once; a failed COMMIT AND CHAIN leaves none open
    fn track_tx_keywords(&mut self, sql: &str, ok: bool) {
        let s = strip_comments_lead(sql);
        if s.starts_with("begin") || s.starts_with("start transaction") {
            self.in_tx = true;
        } else if ["commit", "rollback", "end", "abort"].iter().any(|k| s.starts_with(k)) && !s.starts_with("rollback to") {
            self.in_tx = ok && chains(&s);
            self.changed = false;
        }
    }

    pub async fn run(&mut self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        if !self.in_tx {
            self.changed = false;
        }
        // Production: a plain read doesn't open the Manual transaction (Session::set_reads_outside_tx)
        let read_first = self.reading_first() && plain_read(sql, crate::store::DbKind::Postgres);
        // Before (its own rows, e.g. `select set_config('TimeZone', …), now()`, read it anew) and after (a SET has no rows)
        let rezone = zone_may_change(sql);
        if rezone {
            self.zone = None;
        }
        let mut r = self.run_inner(sql, limit, read_first).await;
        if rezone {
            self.zone = None;
        }
        if let Ok(q) = &mut r {
            q.in_transaction = self.in_tx;
            self.changed |= self.in_tx && crate::audit::is_change(sql, crate::store::DbKind::Postgres);
        }
        if read_first { r.map_err(read_first_refused) } else { r }
    }

    /// Manual mode on Production with no transaction open yet: a plain read runs in a read-only transaction of its own
    /// (Session::set_reads_outside_tx)
    fn reading_first(&self) -> bool {
        self.manual && !self.in_tx && self.reads_outside_tx
    }

    async fn run_inner(&mut self, sql: &str, limit: usize, read_first: bool) -> AppResult<QueryResult> {
        self.close_cursor().await;
        self.idle_closed = false;
        self.origins.clear();
        let start = Instant::now();
        if self.manual && !self.in_tx && !read_first && !strip_comments_lead(sql).starts_with("begin") {
            self.conn.execute("BEGIN").await?;
            self.in_tx = true;
        }
        if crate::audit::tx_end(sql) == Some("COMMIT") {
            self.refuse_aborted_commit().await?;
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
                Err(e) => {
                    // The session is gone (the server ended it, idle in the transaction): its error says why, the retry below wouldn't
                    let e = AppError::Db(e);
                    if crate::app::is_conn_lost(&e) {
                        return Err(e);
                    }
                    // Statements DECLARE cannot take (e.g. WITH ... DELETE) → run normally. A real error will surface again below.
                    self.rollback().await;
                }
            }
        }
        if read_first {
            // A read DECLARE can't take (several statements, SHOW, EXPLAIN): read-only too, ended with it
            self.conn.execute("BEGIN READ ONLY").await?;
            let r = self.run_direct(sql, limit, start).await;
            let _ = self.conn.execute(if r.is_ok() { "COMMIT" } else { "ROLLBACK" }).await;
            return r;
        }
        let r = self.run_direct(sql, limit, start).await;
        self.track_tx_keywords(sql, r.is_ok());
        r
    }

    /// Short transaction for the cursor. Inside a user transaction (Manual, or BEGIN run directly in Auto), protected by a savepoint;
    /// a read before Manual mode's transaction opens (reading_first) can't write
    async fn declare(&mut self, query: &str) -> Result<(), sqlx::Error> {
        let declare = if self.user_tx() {
            format!("SAVEPOINT dbtool_sp; DECLARE {CURSOR} NO SCROLL CURSOR FOR {query}")
        } else if self.reading_first() {
            format!("BEGIN READ ONLY; DECLARE {CURSOR} NO SCROLL CURSOR FOR {query}")
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
        run_direct_on(&mut self.conn, sql, limit, start, &mut self.zone).await
    }

    /// A change with its undo snapshot right before it, in one transaction (Session::run_with_snapshot)
    async fn run_with_snapshot(&mut self, snap: &str, sql: &str, limit: usize) -> (AppResult<QueryResult>, AppResult<QueryResult>) {
        self.close_cursor().await;
        let own = !self.manual && !self.in_tx;
        let began = !self.in_tx;
        if began {
            if let Err(e) = self.conn.execute("BEGIN").await {
                return (Err(AppError::Msg(e.to_string())), Err(e.into()));
            }
            (self.in_tx, self.changed) = (true, false);
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

    /// The cursor's statement again, after the rows already read (without ORDER BY, the DB decides the order)
    fn after_fetched(&self) -> String {
        format!("SELECT * FROM (\n{}\n) AS qurry_page OFFSET {}", strip_trailing_semicolon(&self.cursor_sql), self.fetched)
    }

    /// The result that has more rows to load (More::schema is left to Session::more)
    fn more(&self) -> Option<More> {
        (self.cursor_open || self.idle_closed).then(|| More { sql: self.cursor_sql.clone(), fetched: self.fetched, tx: self.user_tx(), schema: None })
    }

    /// Open `m` on this new session after the rows its old session read, and fetch the next `limit` rows (Session::reopen)
    async fn reopen(&mut self, m: &More, limit: usize) -> AppResult<QueryResult> {
        if let Some(path) = &m.schema {
            sqlx::query("select pg_catalog.set_config('search_path', $1, false)").bind(path).persistent(false).execute(&mut *self.conn).await?;
        }
        (self.cursor_sql, self.fetched) = (m.sql.clone(), m.fetched);
        self.columns.clear();
        if let Err(e) = self.declare(&self.after_fetched()).await {
            self.rollback().await;
            return Err(e.into());
        }
        self.fetch(limit).await
    }

    /// Fetch the next `limit` rows from the open cursor.
    /// If the cursor was closed for being unused, reopen the same statement after the rows already read (OFFSET)
    pub async fn fetch(&mut self, limit: usize) -> AppResult<QueryResult> {
        if !self.cursor_open
            && self.idle_closed
            && let Err(e) = self.declare(&self.after_fetched()).await
        {
            self.rollback().await;
            self.idle_closed = false;
            return Err(AppError::Msg(format!("The result was closed after {} s without use, and reopening it failed ({e}). Run the query again to load more rows.", CURSOR_IDLE.as_secs())));
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
                return Err(if self.reading_first() { read_first_refused(e.into()) } else { e.into() });
            }
        };
        if self.columns.is_empty() {
            (self.columns, self.origins) = match rows.first() {
                Some(r) => (pg_columns(r), pg_origins(r.columns())),
                None => match (&mut *self.conn).describe(strip_trailing_semicolon(&self.cursor_sql)).await {
                    Ok(d) => (
                        d.columns().iter().map(|c| ColumnInfo { name: c.name().to_string(), type_name: c.type_info().name().to_string() }).collect(),
                        pg_origins(d.columns()),
                    ),
                    Err(_) => (Vec::new(), Vec::new()),
                },
            };
        }
        let done = rows.len() < limit;
        self.fetched += rows.len();
        let data = pg_values(&mut self.conn, &self.columns, &rows, &mut self.zone).await;
        if done {
            self.close_cursor().await;
        }
        Ok(query_result(self.columns.clone(), data, 0, !done, start, true))
    }
}

/// Run a statement on the connection (no cursor). `zone`: the session's time zone, read here when unknown (db::pg_values)
async fn run_direct_on(conn: &mut PgConnection, sql: &str, limit: usize, start: Instant, zone: &mut Option<crate::zone::Zone>) -> AppResult<QueryResult> {
    let mut columns = Vec::new();
    let mut rows: Vec<PgRow> = Vec::new();
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
                    rows.push(row);
                }
            }
        }
    }
    let rows = pg_values(conn, &columns, &rows, zone).await;
    Ok(query_result(columns, rows, affected, truncated, start, has_rs))
}

/// Undo snapshot inside the open transaction, under a savepoint: if it fails, the transaction carries on as it was (PostgreSQL would abort it)
pub(crate) async fn pg_snapshot(conn: &mut PgConnection, sql: &str) -> AppResult<QueryResult> {
    conn.execute("SAVEPOINT qurry_snap").await?;
    let mut r = run_direct_on(conn, sql, crate::audit::UNDO_MAX_ROWS + 1, Instant::now(), &mut None).await;
    // A row lock needs the UPDATE privilege: a role with only SELECT and DELETE gets the snapshot without the lock (as before rows were locked)
    if let Err(AppError::Db(sqlx::Error::Database(e))) = &r
        && e.code().as_deref() == Some("42501")
        && let Some(plain) = crate::audit::pg_unlocked(sql)
    {
        conn.execute("ROLLBACK TO SAVEPOINT qurry_snap").await?;
        r = run_direct_on(conn, &plain, crate::audit::UNDO_MAX_ROWS + 1, Instant::now(), &mut None).await;
    }
    end_snapshot(conn, r.is_ok()).await?;
    r
}

/// Close the snapshot's savepoint: keep what it read (ok) or undo it. A RELEASE that fails (a cancel landing on it) leaves the
/// transaction aborted: roll back to the savepoint, so the user's transaction goes on as it was
async fn end_snapshot(conn: &mut PgConnection, ok: bool) -> AppResult<()> {
    if !ok {
        conn.execute("ROLLBACK TO SAVEPOINT qurry_snap").await?;
    } else if let Err(e) = conn.execute("RELEASE SAVEPOINT qurry_snap").await {
        let _ = conn.execute("ROLLBACK TO SAVEPOINT qurry_snap").await;
        return Err(e.into());
    }
    Ok(())
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
    /// A result grid's edits in the console's transaction (Manual mode, or BEGIN in Auto: #106): under a savepoint, so that when
    /// one fails or doesn't change exactly one row (check_exact_one) only they are undone, and the transaction goes on as it was.
    /// They stay pending until Commit / Roll back. snaps = each statement's undo snapshot (run_with_snapshot).
    /// → rows changed, and each snapshot with the rows its statement changed
    pub(crate) async fn submit(&mut self, statements: &[String], snaps: &[Option<&str>]) -> (AppResult<u64>, crate::backend::Snapshots) {
        if let Err(e) = self.run(SUBMIT_SAVEPOINT[0], 1).await {
            return (Err(e), vec![]);
        }
        let (mut n, mut shots) = (0, Vec::new());
        for (i, st) in statements.iter().enumerate() {
            let (shot, r) = match snaps.get(i).copied().flatten() {
                Some(q) => {
                    let (shot, r) = self.run_with_snapshot(q, st, 1).await;
                    (Some(shot), r)
                }
                None => (None, self.run(st, 1).await),
            };
            // A lost connection keeps its error as it is: app::recover reads it
            let r = r.map_err(|e| if crate::app::is_conn_lost(&e) { e } else { AppError::Msg(format!("{e}\n→ {st}")) });
            match r.and_then(|q| check_exact_one(st, q.rows_affected, true).map(|_| q.rows_affected)) {
                Ok(k) => {
                    n += k;
                    shots.push(shot.map(|s| (s, k)));
                }
                Err(e) => {
                    // Undo these edits only; unless the transaction ended with the failure (a deadlock, a lost connection)
                    if self.in_tx() && !crate::app::is_conn_lost(&e) {
                        for back in &SUBMIT_SAVEPOINT[1..] {
                            let _ = self.run(back, 1).await;
                        }
                    }
                    return (Err(e), vec![]);
                }
            }
        }
        match self.run(SUBMIT_SAVEPOINT[2], 1).await {
            Ok(_) => (Ok(n), shots),
            Err(e) => (Err(e), vec![]),
        }
    }
    pub async fn fetch(&mut self, limit: usize) -> AppResult<QueryResult> {
        match self {
            Session::Pg(s) => s.fetch(limit).await,
            Session::My(s) => s.fetch(limit).await,
            Session::Redis(_) | Session::Plugin(_) => Err(AppError::Msg("No more rows".into())),
        }
    }
    /// The current result if it has more rows to load. schema: also read the console's schema (search_path · database()), which
    /// costs a round trip, so only once per result
    pub async fn more(&mut self, schema: bool) -> Option<More> {
        let mut m = match self {
            Session::Pg(s) => s.more(),
            Session::My(s) => s.more(),
            Session::Redis(_) | Session::Plugin(_) => None,
        }?;
        if schema && !m.tx {
            m.schema = match self {
                Session::Pg(s) => sqlx::query_scalar("select current_setting('search_path')").persistent(false).fetch_one(&mut *s.conn).await.ok(),
                Session::My(s) => s.database().await,
                _ => None,
            };
        }
        Some(m)
    }
    /// Open a result whose session was closed (Session::more of it) on this new session, after the rows already read
    pub async fn reopen(&mut self, m: &More, limit: usize) -> AppResult<QueryResult> {
        let r = match self {
            Session::Pg(s) => s.reopen(m, limit).await,
            Session::My(s) => s.reopen(m, limit).await,
            Session::Redis(_) | Session::Plugin(_) => Err(AppError::Msg("No more rows".into())),
        };
        r.map_err(|e| AppError::Msg(format!("{RECONNECTED}, and reopening the result on the new connection failed ({e}). Run the query again to load more rows.")))
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
    /// How Explain Analyse wraps the statement it really runs: in a savepoint when a transaction is open (or MySQL autocommit is
    /// off: a BEGIN would commit what is pending there), else in a transaction of its own. GUI and TUI both ask this
    pub async fn explain_wrap(&mut self) -> AppResult<[&'static str; 2]> {
        let open = match self {
            Session::My(s) => s.tx_may_be_open().await?,
            s => s.in_tx(),
        };
        Ok(if open { EXPLAIN_SAVEPOINT } else { EXPLAIN_BEGIN })
    }
    /// Whether there are uncommitted changes
    pub fn in_tx(&self) -> bool {
        match self {
            Session::Pg(s) => s.in_tx(),
            Session::My(s) => s.in_tx,
            Session::Redis(_) | Session::Plugin(_) => false,
        }
    }
    /// Whether a change ran in the open transaction (MySession::changed, PgSession::changed)
    pub fn tx_changed(&self) -> bool {
        match self {
            Session::Pg(s) => s.in_tx && s.changed,
            Session::My(s) => s.in_tx && s.changed,
            Session::Redis(_) | Session::Plugin(_) => false,
        }
    }
    /// Production (#106 review): in Manual mode, a plain read (plain_read) run while no transaction is open runs as in Auto, in a
    /// transaction of its own that ends with it, and the transaction opens at the first other statement. So reading a production
    /// server holds no snapshot or table locks (an ALTER TABLE would wait behind them, and every query behind it), and nothing
    /// shows as pending; a change still stays pending until Commit / Roll back. A BEGIN typed first opens it as before
    pub fn set_reads_outside_tx(&mut self, on: bool) {
        match self {
            Session::Pg(s) => s.reads_outside_tx = on,
            Session::My(s) => s.reads_outside_tx = on,
            Session::Redis(_) | Session::Plugin(_) => {}
        }
    }
    /// How the last statement ended the open MySQL transaction by itself (MySession::ended)
    pub fn tx_ended(&self) -> Option<&'static str> {
        match self {
            Session::My(s) => s.ended,
            _ => None,
        }
    }
    /// On tab close: roll back the open transaction, reset session settings and return the connection to the pool. Within 5s: on
    /// a server or tunnel that doesn't answer it would wait forever (closing the tab, switching its database). Then the connection
    /// is closed instead, and the server rolls the transaction back itself
    pub async fn discard(&mut self) {
        let done = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            match self {
                Session::Pg(s) => s.discard().await,
                Session::My(s) => s.discard().await,
                Session::Redis(_) | Session::Plugin(_) => {}
            }
        })
        .await;
        if done.is_err() {
            self.abandon();
        }
    }
    /// Its connection is closed when the session is dropped, not given back to the pool: it didn't answer, and a statement
    /// cut off by a time limit may have left it halfway through a message
    pub fn abandon(&mut self) {
        match self {
            Session::Pg(s) => s.conn.close_on_drop(),
            Session::My(s) => s.abandon(),
            Session::Redis(_) | Session::Plugin(_) => {}
        }
    }
    /// A round trip that runs nothing (PostgreSQL: the protocol's ping; MySQL: DO 0): whether the connection still answers. Callers
    /// set the time limit; when it runs out the session stays usable, since its next use reads past the answer left unread
    pub async fn check(&mut self) -> AppResult<()> {
        match self {
            Session::Pg(s) => Ok(sqlx::Connection::ping(&mut *s.conn).await?),
            Session::My(s) => s.check().await,
            Session::Redis(m) => crate::redisdb::check(m).await,
            Session::Plugin(_) => Ok(()),
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
    use serde_json::Value;
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
        sqlx::query("create table if not exists public.dbtool_tx_auto_t(id int primary key, v text)").execute(&p).await.unwrap();
        sqlx::query("delete from public.dbtool_tx_auto_t").execute(&p).await.unwrap();
        sqlx::query("insert into public.dbtool_tx_auto_t values (1, 'a')").execute(&p).await.unwrap();
        let seen = || async { sqlx::query_scalar::<_, String>("select v from public.dbtool_tx_auto_t where id = 1").fetch_one(&p).await.unwrap() };
        let mut s = PgSession::open(&p).await.unwrap();
        s.run("begin", 500).await.unwrap();
        s.run("update public.dbtool_tx_auto_t set v = 'changed' where id = 1", 500).await.unwrap();
        // A fully read SELECT, a multi-page SELECT and a failing SELECT all leave the transaction alone
        let r = s.run("select * from public.dbtool_tx_auto_t", 500).await.unwrap();
        assert!(r.in_transaction);
        s.run("select g from generate_series(1, 1200) g", 500).await.unwrap();
        assert!(s.run("select * from no_such_table_xyz", 500).await.is_err());
        assert_eq!(seen().await, "a", "nothing committed yet");
        assert!(s.set_manual(true).await.is_ok() && s.set_manual(false).await.is_err(), "pending changes block switching to Auto");
        s.run("rollback", 500).await.unwrap();
        assert_eq!(seen().await, "a", "rolled back");
        // With uncommitted changes in Manual, switching to Auto is not allowed
        s.set_manual(true).await.unwrap();
        s.run("update public.dbtool_tx_auto_t set v = 'm' where id = 1", 500).await.unwrap();
        assert!(s.set_manual(false).await.is_err());
        s.rollback_tx().await.unwrap();
        s.set_manual(false).await.unwrap();
        assert_eq!(seen().await, "a");
        // Saving table edits: UPDATE/DELETE hitting 0 or multiple rows rolls back everything
        let none = p.execute_batch(&["insert into public.dbtool_tx_auto_t values (2, 'b')".into(), "update public.dbtool_tx_auto_t set v = 'x' where id = 99".into()], true, false, &[]).await;
        assert!(none.is_err());
        let many = p.execute_batch(&["update public.dbtool_tx_auto_t set v = 'x' where id > 0".into()], true, false, &[]).await;
        assert!(many.is_ok(), "one row matches");
        sqlx::query("insert into public.dbtool_tx_auto_t values (3, 'c')").execute(&p).await.unwrap();
        assert!(p.execute_batch(&["delete from public.dbtool_tx_auto_t where id > 0".into()], true, false, &[]).await.is_err());
        let n: i64 = sqlx::query_scalar("select count(*) from public.dbtool_tx_auto_t").fetch_one(&p).await.unwrap();
        assert_eq!(n, 2, "row 2 insert was rolled back, rows 1 and 3 remain");
        sqlx::query("drop table public.dbtool_tx_auto_t").execute(&p).await.unwrap();
    }

    #[test]
    fn select_like() {
        assert!(is_select_like("  -- c\n /* x */ SELECT 1"));
        assert!(is_select_like("with a as (select 1) select * from a"));
        assert!(!is_select_like("update t set x = 1"));
    }

    /// COMMIT / ROLLBACK … AND CHAIN keeps a transaction open (PostgreSQL and MySQL); AND NO CHAIN and the plain ones don't
    #[test]
    fn and_chain_keeps_the_transaction() {
        let c = |q: &str| chains(&strip_comments_lead(q));
        for q in [
            "COMMIT AND CHAIN", "rollback and chain;", "commit work and chain", "ROLLBACK TRANSACTION AND CHAIN", "end and chain", "abort transaction and chain",
            "-- x\n/* y */ commit and chain", "commit /* x */ and -- y\n chain", "commit\n\tand\nchain", "commit # x\n and chain",
        ] {
            assert!(c(q), "{q}");
        }
        for q in [
            "commit", "rollback;", "commit work", "end transaction", "abort", "commit and no chain", "rollback work and no chain", "commit and", "commit chain",
            "commit and chained", "commit; and chain", "commit /* and chain */", "-- and chain\ncommit", "commit prepared 'x'", "rollback to savepoint s",
        ] {
            assert!(!c(q), "{q}");
        }
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
        sqlx::query("create table if not exists public.dbtool_tx_manual_t(id int primary key)").execute(&p).await.unwrap();
        sqlx::query("truncate public.dbtool_tx_manual_t").execute(&p).await.unwrap();
        s.set_manual(true).await.unwrap();
        let ins = s.run("insert into public.dbtool_tx_manual_t values (1)", 500).await.unwrap();
        assert!(ins.in_transaction);
        let seen: i64 = sqlx::query_scalar("select count(*) from public.dbtool_tx_manual_t").fetch_one(&p).await.unwrap();
        assert_eq!(seen, 0, "uncommitted row must not be visible");
        // Inside a Manual transaction, SELECT (cursor) sees its own changes too
        let mine = s.run("select count(*) from public.dbtool_tx_manual_t", 500).await.unwrap();
        assert_eq!(mine.rows[0][0], Value::from(1));
        s.rollback_tx().await.unwrap();
        let after_rb = s.run("select count(*) from public.dbtool_tx_manual_t", 500).await.unwrap();
        assert_eq!(after_rb.rows[0][0], Value::from(0));
        s.run("insert into public.dbtool_tx_manual_t values (2)", 500).await.unwrap();
        s.commit().await.unwrap();
        let committed: i64 = sqlx::query_scalar("select count(*) from public.dbtool_tx_manual_t").fetch_one(&p).await.unwrap();
        assert_eq!(committed, 1);
        // Statements DECLARE cannot take do not break the manual transaction either
        let w = s.run("with x as (delete from public.dbtool_tx_manual_t where id = 2 returning id) select count(*) from x", 500).await.unwrap();
        assert_eq!(w.rows[0][0], Value::from(1));
        s.rollback_tx().await.unwrap();
        s.set_manual(false).await.unwrap();
        sqlx::query("drop table public.dbtool_tx_manual_t").execute(&p).await.unwrap();

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

    /// A RELEASE of the snapshot's savepoint that fails (a cancel landing on it) rolls back to the savepoint, so the user's
    /// transaction isn't left aborted: it goes on with what it changed before
    #[tokio::test]
    #[ignore]
    async fn pg_failed_release_keeps_the_transaction() {
        let Some(p) = pool().await else { return };
        let mut c = p.acquire().await.unwrap();
        c.execute("BEGIN; CREATE TEMP TABLE qurry_release (v int); INSERT INTO qurry_release VALUES (1); SAVEPOINT qurry_snap").await.unwrap();
        // The transaction is aborted, as a cancel landing on RELEASE leaves it
        assert!(c.execute("SELECT 1 / 0").await.is_err());
        assert!(end_snapshot(&mut c, true).await.is_err());
        let v: i32 = sqlx::query_scalar("SELECT v FROM qurry_release").fetch_one(&mut *c).await.unwrap();
        assert_eq!(v, 1);
        c.execute("ROLLBACK").await.unwrap();
    }

    /// QURRY_TEST_MY=host:port:user:password:db
    #[tokio::test]
    #[ignore]
    async fn mysql_failed_snapshot_keeps_the_change() {
        let Some(s) = crate::testenv::my() else { return };
        failed_snapshot_keeps_the_change(connect(&s.cfg, s.password.as_deref()).await.unwrap(), "create database if not exists audit_hardening_s", "drop database audit_hardening_s").await;
    }
}
