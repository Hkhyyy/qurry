//! MySQL / MariaDB driver. Returns the same shapes as the PostgreSQL side (QueryResult, metadata, sessions).
//! - Queries without parameters go over the text protocol (COM_QUERY), so values arrive as text bytes
//! - MySQL has no schema level: database == schema
use crate::db::{int_value, query_result, ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use crate::meta::{
    ColumnMeta, CompletionTable, ConstraintMeta, FtsHit, FtsRequest, IndexEntry, IndexMeta, RelationInfo, RoutineInfo, SchemaInfo,
    SchemaObjects, TableDetail, TableStructure, ForeignKeyMeta, TriggerMeta,
};
use crate::session::{is_select_like, strip_comments_lead};
use futures::TryStreamExt;
use serde_json::Value;
use sqlx::mysql::{MySqlPool, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{Column, Either, Executor, MySql, Row, TypeInfo, ValueRef};
use std::time::Instant;

pub const SYSTEM_DBS: [&str; 4] = ["information_schema", "performance_schema", "mysql", "sys"];

pub fn ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}
/// MySQL string literal: backslash is also an escape character
pub fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

pub(crate) fn columns_of(row: &MySqlRow) -> Vec<ColumnInfo> {
    row.columns().iter().map(|c| ColumnInfo { name: c.name().to_string(), type_name: c.type_info().name().to_string() }).collect()
}

/// Text protocol value → JSON
pub(crate) fn value(row: &MySqlRow, i: usize) -> Value {
    let raw = match row.try_get_raw(i) {
        Ok(r) => r,
        Err(_) => return Value::Null,
    };
    if raw.is_null() {
        return Value::Null;
    }
    let ty = raw.type_info().name().to_uppercase();
    let bytes: &[u8] = match row.try_get_unchecked::<&[u8], _>(i) {
        Ok(b) => b,
        Err(_) => return Value::Null,
    };
    let binary = ty.contains("BLOB") || ty.contains("BINARY") || ty == "GEOMETRY";
    if binary {
        let hex: String = bytes.iter().take(64).map(|x| format!("{x:02X}")).collect();
        return Value::from(format!("0x{hex}{}", if bytes.len() > 64 { "…" } else { "" }));
    }
    let s = String::from_utf8_lossy(bytes);
    match ty.as_str() {
        "BOOLEAN" => Value::from(s == "1"),
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT" | "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED"
        | "INT UNSIGNED" | "YEAR" => s.parse::<i64>().map(int_value).unwrap_or_else(|_| Value::from(s.as_ref())),
        "BIGINT UNSIGNED" => match s.parse::<u64>() {
            Ok(v) if v <= (1u64 << 53) => Value::from(v),
            _ => Value::from(s.as_ref()),
        },
        "FLOAT" | "DOUBLE" => s.parse::<f64>().ok().filter(|f| f.is_finite()).map(Value::from).unwrap_or_else(|| Value::from(s.as_ref())),
        "BIT" => {
            // BIT(n) arrives as binary even over the text protocol
            let n = bytes.iter().fold(0u64, |a, b| (a << 8) | *b as u64);
            Value::from(n)
        }
        _ => Value::from(s.as_ref()),
    }
}

pub async fn run_on(conn: &mut sqlx::MySqlConnection, sql: &str, limit: usize) -> AppResult<QueryResult> {
    run_on_with(conn, sql, limit, false).await
}

/// Read-only path: run via the prepared protocol (the server accepts only one statement → appending statements with `;` doesn't work as a bypass).
/// Only statements that can't be prepared (some SHOW etc., error 1295) go over the text protocol — the guard check has already passed by then
pub async fn run_read_only_on(conn: &mut sqlx::MySqlConnection, sql: &str, limit: usize) -> AppResult<QueryResult> {
    match run_on_with(conn, sql, limit, true).await {
        Err(AppError::Db(sqlx::Error::Database(e))) if e.code().as_deref() == Some("HY000") && e.message().contains("prepared statement protocol") => run_on_with(conn, sql, limit, false).await,
        r => r,
    }
}

async fn run_on_with(conn: &mut sqlx::MySqlConnection, sql: &str, limit: usize, prepared: bool) -> AppResult<QueryResult> {
    let start = Instant::now();
    let mut columns = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut affected = 0u64;
    let mut truncated = false;
    let mut has_rs = false;
    {
        let mut stream = if prepared { conn.fetch_many(sqlx::query(sql)) } else { conn.fetch_many(sqlx::raw_sql(sql)) };
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Left(done) => affected += done.rows_affected(),
                Either::Right(row) => {
                    has_rs = true;
                    if columns.is_empty() {
                        columns = columns_of(&row);
                    }
                    if rows.len() >= limit {
                        truncated = true;
                        continue; // Must consume up to the next result/OK packet to reuse the connection
                    }
                    rows.push((0..row.len()).map(|i| value(&row, i)).collect());
                }
            }
        }
    }
    // Column info for a 0-row SELECT
    if !has_rs && is_select_like(sql) {
        if let Ok(d) = conn.describe(sql).await {
            if !d.columns().is_empty() {
                has_rs = true;
                columns = d.columns().iter().map(|c| ColumnInfo { name: c.name().to_string(), type_name: c.type_info().name().to_string() }).collect();
            }
        }
    }
    Ok(query_result(columns, rows, affected, truncated, start, has_rs))
}

/// Export on an already acquired connection. prepared = the prepared protocol that runs only one statement (read-only path)
pub async fn export_on(conn: &mut sqlx::MySqlConnection, sql: &str, out: &mut crate::transfer::Exporter, prepared: bool) -> AppResult<()> {
    let mut stream = if prepared { conn.fetch(sqlx::query(sql)) } else { conn.fetch(sqlx::raw_sql(sql)) };
    while let Some(row) = stream.try_next().await? {
        if !out.has_header() {
            let names: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
            out.header(&names).await?;
        }
        let vals: Vec<serde_json::Value> = (0..row.len()).map(|i| value(&row, i)).collect();
        out.row(&vals).await?;
    }
    Ok(())
}

/// Undo snapshot (a SELECT … FOR UPDATE from audit::UndoPlan) in the open transaction (text protocol: `value` reads text values);
/// a failing SELECT leaves the MySQL transaction as it was, except a deadlock (and a lock wait timeout with innodb_rollback_on_timeout), which rolls back all of it
pub(crate) async fn snapshot_on(conn: &mut sqlx::MySqlConnection, sql: &str) -> AppResult<QueryResult> {
    run_on(conn, sql, crate::audit::UNDO_MAX_ROWS + 1).await
}

/// MySQL error number (1213 = deadlock, …)
pub(crate) fn error_number(e: &AppError) -> Option<u16> {
    match e {
        AppError::Db(sqlx::Error::Database(d)) => d.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>().map(|m| m.number()),
        _ => None,
    }
}

// ───────── Console session ─────────

/// In Manual mode (autocommit=0), whether the statement opens the transaction: every one but SHOW / SET / USE
fn opens_tx(s: &str) -> bool {
    !s.starts_with("show") && !s.starts_with("set") && !s.starts_with("use")
}

/// Whether the console asks the server if its transaction is still open after the statement, whether it ran or failed: after every
/// statement but a SELECT and a row change (s = strip_comments_lead(sql)). MySQL commits the open transaction for DDL, LOCK / UNLOCK
/// TABLES, SET autocommit, SET PASSWORD, INSTALL PLUGIN … (some also when they then fail, not when they fail to parse), and a CALL or
/// DO can open one in Manual: no list of words gets them all right
fn asks_server(sql: &str, s: &str) -> bool {
    !is_select_like(sql) && !is_dml(s)
}

/// The leading word of `s` (strip_comments_lead), ending at any character that isn't a letter, digit or `_`, so a comment glued to
/// it (`call/**/p()`) doesn't hide it
fn first_word(s: &str) -> &str {
    s.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("")
}

/// The word after a leading BEGIN keyword (s = strip_comments_lead(sql)), comments skipped (`--`, `/* */`, `#`): "" for none.
/// None when `s` doesn't start with the word BEGIN
fn after_begin(s: &str) -> Option<&str> {
    let mut rest = s.strip_prefix("begin").filter(|r| !r.starts_with(|c: char| c.is_alphanumeric() || c == '_'))?.trim_start();
    loop {
        rest = if let Some(r) = rest.strip_prefix("--").or_else(|| rest.strip_prefix('#')) {
            r.split_once('\n').map_or("", |x| x.1)
        } else if let Some(r) = rest.strip_prefix("/*") {
            r.split_once("*/").map_or("", |x| x.1)
        } else {
            return Some(first_word(rest));
        }
        .trim_start();
    }
}

/// A MariaDB compound statement: at top level only BEGIN NOT ATOMIC … END (a labeled one is a syntax error there). It doesn't
/// commit the open transaction, and what it runs can commit or roll back
pub(crate) fn is_compound(s: &str) -> bool {
    after_begin(s) == Some("not")
}

/// A plain BEGIN (s = strip_comments_lead(sql)), whatever follows it (`work`, `;`, a trailing comment the splitter left in): it
/// commits the open transaction and opens one
fn is_begin(s: &str) -> bool {
    after_begin(s).is_some_and(|w| w != "not")
}

/// Whether `s` (strip_comments_lead) runs statements of its own that can COMMIT or ROLLBACK: a CALL, EXECUTE of a prepared
/// COMMIT / ROLLBACK, or a MariaDB BEGIN NOT ATOMIC … END. When the transaction ended with it, the server's answer can't tell
/// whether it committed or rolled back
fn ends_in_procedure(s: &str) -> bool {
    let first = first_word(s);
    ["call", "execute"].contains(&first) || is_compound(s)
}

/// Statements that change rows (s = strip_comments_lead(sql))
fn changes_rows(s: &str) -> bool {
    let first = first_word(s);
    ["insert", "update", "delete", "replace"].contains(&first) || s.starts_with("load data")
}

/// Statements that change or read rows and never commit the open transaction (s = strip_comments_lead(sql))
fn is_dml(s: &str) -> bool {
    let first = first_word(s);
    changes_rows(s) || ["show", "explain", "describe", "desc"].contains(&first)
}

/// Dedicated connection for a console tab. MySQL has no protocol-level cursor, so "load more" re-queries with LIMIT/OFFSET.
pub struct MySession {
    conn: PoolConnection<MySql>,
    pub id: i64,
    manual: bool,
    pub in_tx: bool,
    /// A change ran in the open transaction (audit::is_change, as the Production log has it): a transaction opened by BEGIN or
    /// guessed after a SELECT in Manual mode has none, so ending it saves nothing
    pub changed: bool,
    /// How the last statement ended the open transaction, when it did by itself (audit::IMPLICIT_COMMIT or audit::ENDED_UNKNOWN):
    /// not a COMMIT / ROLLBACK, a deadlock in a row change or a lost connection
    pub ended: Option<&'static str>,
    /// tx_open's answer can be trusted: it saw the transaction the session opened to check it (TiDB, Vitess accept the SET inside one)
    probe: bool,
    /// (SELECT statement, next OFFSET)
    more: Option<(String, usize)>,
}

impl MySession {
    pub async fn open(p: &MySqlPool) -> AppResult<Self> {
        let mut conn = p.acquire().await?;
        let id: u64 = sqlx::query_scalar("select connection_id()").fetch_one(&mut *conn).await?;
        let mut s = Self { conn, id: id as i64, manual: false, in_tx: false, changed: false, ended: None, probe: true, more: None };
        // Best effort: a MySQL-protocol server without transactions (ClickHouse, older StarRocks / Doris) still opens, without the probe
        let began = s.conn.execute("START TRANSACTION").await.is_ok();
        s.probe = began && s.tx_open().await != Some(false);
        if began {
            let _ = s.conn.execute("ROLLBACK").await;
        }
        Ok(s)
    }

    pub async fn ping(&mut self) {
        let _ = self.conn.execute("select 1").await;
    }

    pub fn close_cursor(&mut self) {
        self.more = None;
    }

    /// On tab close: roll back the open transaction and restore autocommit (so other pool users don't inherit it)
    pub async fn discard(&mut self) {
        self.more = None;
        if self.manual || self.in_tx {
            let _ = self.conn.execute("ROLLBACK").await;
            self.in_tx = false;
        }
        if self.manual {
            let _ = self.conn.execute("SET autocommit = 1").await;
            self.manual = false;
        }
    }

    /// Don't switch to Auto with uncommitted changes (Commit or Roll back first)
    pub async fn set_manual(&mut self, manual: bool) -> AppResult<()> {
        if !manual && self.in_tx {
            return Err(AppError::Msg(crate::session::PENDING_TX.into()));
        }
        self.more = None;
        self.conn.execute(if manual { "SET autocommit = 0" } else { "SET autocommit = 1" }).await?;
        self.manual = manual;
        Ok(())
    }

    pub async fn commit(&mut self) -> AppResult<()> {
        let r = self.conn.execute("COMMIT").await;
        // A COMMIT the server refused (e.g. a Group Replication certification failure) ends the transaction too: it was rolled back.
        // Not a lost connection: then it is unknown whether it committed
        if matches!(r, Ok(_) | Err(sqlx::Error::Database(_))) {
            self.in_tx = false;
        }
        r?;
        Ok(())
    }

    pub async fn rollback_tx(&mut self) -> AppResult<()> {
        self.conn.execute("ROLLBACK").await?;
        self.in_tx = false;
        Ok(())
    }

    /// Whether InnoDB rolled back the whole transaction on this error: a deadlock, and a lock wait timeout with innodb_rollback_on_timeout
    async fn tx_rolled_back(&mut self, e: &AppError) -> bool {
        match error_number(e) {
            Some(1213) => true,
            Some(1205) => sqlx::query_scalar::<_, i64>("SELECT @@innodb_rollback_on_timeout").fetch_one(&mut *self.conn).await.is_ok_and(|v| v != 0),
            _ => false,
        }
    }

    /// Whether the server has a transaction open on this connection (asks_server), changing nothing: MySQL refuses to set
    /// session_track_gtids inside one (1766), and set to its own value it is a no-op. (Not the isolation level: setting it outside a
    /// transaction would undo a pending SET TRANSACTION ISOLATION LEVEL.) MariaDB has no session_track_gtids (1193) but has
    /// @@in_transaction. No privilege needed. None = no answer (MySQL 5.6, or a server that gets it wrong: `probe`)
    async fn tx_open(&mut self) -> Option<bool> {
        if !self.probe {
            return None;
        }
        match self.conn.execute("SET @@SESSION.session_track_gtids = @@SESSION.session_track_gtids").await.map_err(AppError::from) {
            Ok(_) => Some(false),
            Err(e) if error_number(&e) == Some(1766) => Some(true),
            Err(e) if error_number(&e) == Some(1193) => sqlx::query_scalar::<_, u64>("SELECT @@in_transaction").fetch_one(&mut *self.conn).await.ok().map(|n| n != 0),
            Err(_) => None,
        }
    }

    /// Whether a BEGIN now could commit pending work: a transaction is open (Qurry saw it open, or the server says so: a START
    /// TRANSACTION inside a /*! … */ comment or after a # comment), or the server has autocommit off (then one may be open though
    /// Qurry didn't see it: autocommit turned off in a procedure, a /*! … */ comment or the server's default). One the server
    /// reports is followed from then on, so its changes show as pending; an autocommit=0 session is followed as Manual (asked even
    /// with one open), so closing the tab turns autocommit back on before the connection goes back to the pool
    pub async fn tx_may_be_open(&mut self) -> AppResult<bool> {
        self.in_tx = self.in_tx || self.tx_open().await == Some(true);
        let on: i64 = match sqlx::query_scalar("SELECT @@autocommit").fetch_one(&mut *self.conn).await {
            Ok(on) => on,
            Err(_) if self.in_tx => return Ok(true),
            Err(e) => return Err(AppError::Msg(format!("Explain Analyse didn't run the statement: it couldn't ask the server whether autocommit is on ({e}). Nothing was run or committed; try again, or Commit or Roll back first"))),
        };
        self.manual |= on == 0;
        Ok(self.in_tx || on == 0)
    }

    /// Whether to ask the server if the transaction is still open after `sql` (s = strip_comments_lead(sql)) ran or failed: one
    /// rule for both. In Auto with none open, only a CALL, EXECUTE or BEGIN NOT ATOMIC can leave one open
    fn asks(&self, sql: &str, s: &str) -> bool {
        (self.manual || self.in_tx || ends_in_procedure(s)) && asks_server(sql, s)
    }

    /// After `s` (strip_comments_lead) ran or failed with `err`: follow the server's answer whether the transaction is still open.
    /// Without one it stays as it was (or opens in Manual), so Qurry never claims changes are committed without knowing. If the
    /// statement ended it (not a COMMIT / ROLLBACK), `ended` says how: committed, unless it can also have rolled it back (a CALL or
    /// EXECUTE; a deadlock or lock wait timeout: a DDL waiting for its metadata lock times out after the commit, a SET with a
    /// SELECT … FOR UPDATE rolls it back)
    async fn follow_server(&mut self, s: &str, err: Option<&AppError>) {
        let was = self.in_tx;
        let opened = self.manual && opens_tx(s) && err.is_none_or(|e| error_number(e) != Some(1064));
        self.in_tx = self.tx_open().await.unwrap_or(was || opened);
        if was && !self.in_tx && !s.starts_with("commit") && !s.starts_with("rollback") {
            let either = ends_in_procedure(s) || err.is_some_and(|e| matches!(error_number(e), Some(1205 | 1213)));
            self.ended = Some(if either { crate::audit::ENDED_UNKNOWN } else { crate::audit::IMPLICIT_COMMIT });
        }
    }

    pub async fn run(&mut self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        self.more = None;
        self.ended = None;
        if !self.in_tx {
            self.changed = false;
        }
        let mut r = match run_on(&mut self.conn, sql, limit).await {
            Ok(r) => r,
            Err(e) => {
                let s = strip_comments_lead(sql);
                if self.asks(sql, &s) && error_number(&e).is_some() && !crate::app::is_conn_lost(&e) {
                    // Same rule as for one that ran: a DDL or SET PASSWORD that committed before failing, a COMMIT the server refused
                    // (rolled back) ended it, one that failed to parse (COMMIT TRANSACTION: 1064) didn't, a CALL may have opened one
                    self.follow_server(&s, Some(&e)).await;
                } else if self.tx_rolled_back(&e).await {
                    self.in_tx = false;
                } else if self.manual && opens_tx(&s) && !s.starts_with("rollback") && error_number(&e) != Some(1064) {
                    // With autocommit=0 a statement that fails (but parsed) still leaves the transaction open: the row locks it took
                    // before a lock wait timeout, what a procedure changed before it failed
                    self.in_tx = true;
                }
                return Err(e);
            }
        };
        let s = strip_comments_lead(sql);
        if s.starts_with("set") && s.contains("autocommit") {
            // A typed SET autocommit: follow the server's mode (whether it committed is asked below)
            if let Ok(on) = sqlx::query_scalar::<_, i64>("SELECT @@autocommit").fetch_one(&mut *self.conn).await {
                self.manual = on == 0;
            }
        }
        if is_begin(&s) || s.starts_with("start transaction") {
            // It committed the one that was open
            if self.in_tx {
                self.ended = Some(crate::audit::IMPLICIT_COMMIT);
                self.changed = false;
            }
            self.in_tx = true;
        } else if s.starts_with("commit") || (s.starts_with("rollback") && !s.starts_with("rollback to")) {
            // AND CHAIN opens the next transaction at once (also when none was open)
            self.in_tx = crate::session::chains(&s);
            self.changed = false;
        } else if self.asks(sql, &s) {
            self.follow_server(&s, None).await;
        } else if self.manual && opens_tx(&s) {
            // With autocommit=0, any statement opens a transaction, not only a change: WITH … DELETE, SELECT … FOR UPDATE (its row locks),
            // SELECT f() where f writes. Like PostgreSQL's Manual mode, it stays open until Commit / Roll back
            self.in_tx = true;
        }
        self.changed |= self.in_tx && crate::audit::is_change(sql, crate::store::DbKind::Mysql);
        if r.truncated && is_select_like(sql) {
            self.more = Some((sql.trim().trim_end_matches(';').trim().to_string(), limit));
        } else {
            r.truncated = false;
        }
        r.in_transaction = self.in_tx;
        Ok(r)
    }

    /// A change with its undo snapshot right before it, in one transaction (Session::run_with_snapshot)
    pub async fn run_with_snapshot(&mut self, snap: &str, sql: &str, limit: usize) -> (AppResult<QueryResult>, AppResult<QueryResult>) {
        self.more = None;
        if !self.in_tx {
            self.changed = false;
        }
        let own = !self.manual && !self.in_tx;
        let had_tx = self.in_tx;
        if own {
            if let Err(e) = self.conn.execute("BEGIN").await {
                return (Err(AppError::Msg(e.to_string())), Err(e.into()));
            }
            self.in_tx = true;
        }
        let shot = snapshot_on(&mut self.conn, snap).await;
        // Cancelled · lock wait timeout · deadlock while locking the rows: don't run the change
        if let Err(e) = &shot
            && crate::audit::stops_change(e)
        {
            // No transaction was open before: end the one the snapshot ran in (Qurry's own, or the one autocommit=0 opened in Manual,
            // which holds nothing else since in Manual every statement but SHOW / SET / USE sets in_tx, also when it fails)
            // with the row locks it took before it stopped
            let gone = if !had_tx {
                let _ = self.rollback_tx().await;
                true
            } else {
                self.tx_rolled_back(e).await
            };
            if gone {
                self.in_tx = false;
            }
            return (Err(AppError::Msg(e.to_string())), Err(crate::audit::stopped(e, had_tx && gone)));
        }
        // Manual: the snapshot's locking read already opened the transaction (its locks are held until Commit / Roll back)
        if shot.is_ok() {
            self.in_tx = true;
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

    pub async fn fetch(&mut self, limit: usize) -> AppResult<QueryResult> {
        let Some((sql, offset)) = self.more.clone() else { return Err(AppError::Msg("No more rows".into())) };
        let paged = format!("SELECT * FROM ({sql}) AS dbtool_page LIMIT {limit} OFFSET {offset}");
        let mut r = run_on(&mut self.conn, &paged, limit).await?;
        r.truncated = r.rows.len() >= limit;
        self.more = if r.truncated { Some((sql, offset + limit)) } else { None };
        r.in_transaction = self.in_tx;
        Ok(r)
    }
}

// ───────── Metadata (information_schema) ─────────
// MySQL 8 information_schema columns sometimes come back as binary strings, so everything is CAST(... AS CHAR)

pub async fn list_databases(p: &MySqlPool) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar("select cast(schema_name as char) from information_schema.schemata order by 1").fetch_all(p).await?)
}

pub async fn list_schemas(p: &MySqlPool) -> AppResult<Vec<SchemaInfo>> {
    let db: Option<String> = sqlx::query_scalar("select cast(database() as char)").fetch_one(p).await?;
    Ok(db.map(|name| SchemaInfo { is_system: SYSTEM_DBS.contains(&name.as_str()), name }).into_iter().collect())
}

pub async fn list_objects(p: &MySqlPool, schema: &str) -> AppResult<SchemaObjects> {
    let rels: Vec<(String, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "select cast(table_name as char), cast(table_type as char), cast(nullif(table_comment, '') as char), cast(table_rows as signed) \
         from information_schema.tables where table_schema = ? order by table_name",
    )
    .bind(schema)
    .fetch_all(p)
    .await?;
    let relations = rels
        .into_iter()
        .map(|(name, ty, comment, rows)| RelationInfo { name, kind: if ty.contains("VIEW") { "view".into() } else { "table".into() }, comment, row_estimate: rows })
        .collect();
    let routines: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "select cast(r.routine_name as char), cast(r.routine_type as char), cast(r.dtd_identifier as char), \
           (select cast(group_concat(concat(coalesce(p.parameter_mode, ''), ' ', p.parameter_name, ' ', p.dtd_identifier) order by p.ordinal_position separator ', ') as char) \
              from information_schema.parameters p where p.specific_schema = r.routine_schema and p.specific_name = r.specific_name and p.ordinal_position > 0) \
         from information_schema.routines r where r.routine_schema = ? order by r.routine_name",
    )
    .bind(schema)
    .fetch_all(p)
    .await?;
    let routines = routines
        .into_iter()
        .map(|(name, ty, returns, args)| RoutineInfo {
            name,
            args: args.unwrap_or_default().split(", ").map(str::trim).collect::<Vec<_>>().join(", "),
            returns,
            kind: ty.to_lowercase(),
        })
        .collect();
    Ok(SchemaObjects { relations, routines })
}

pub async fn table_detail(p: &MySqlPool, schema: &str, table: &str) -> AppResult<TableDetail> {
    let cols: Vec<(String, String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "select cast(column_name as char), cast(column_type as char), cast(is_nullable as char), cast(column_default as char), \
           cast(nullif(column_comment, '') as char), cast(column_key as char) \
         from information_schema.columns where table_schema = ? and table_name = ? order by ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let columns = cols
        .into_iter()
        .map(|(name, data_type, nullable, default_value, comment, key)| ColumnMeta { name, data_type, nullable: nullable == "YES", default_value, comment, is_pk: key == "PRI" })
        .collect();
    let idx: Vec<(String, i64, String)> = sqlx::query_as(
        "select cast(index_name as char), cast(non_unique as signed), cast(group_concat(column_name order by seq_in_index separator '\\t') as char) \
         from information_schema.statistics where table_schema = ? and table_name = ? group by index_name, non_unique order by index_name",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let indexes = idx
        .into_iter()
        .map(|(name, non_unique, cols)| {
            let primary = name == "PRIMARY";
            let columns: Vec<String> = cols.split('\t').map(String::from).collect();
            IndexMeta {
                definition: format!("{} {} ({})", if primary { "PRIMARY KEY" } else if non_unique == 0 { "UNIQUE INDEX" } else { "INDEX" }, ident(&name), columns.join(", ")),
                is_unique: non_unique == 0,
                is_primary: primary,
                name,
                columns,
            }
        })
        .collect();
    let cons: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "select cast(tc.constraint_name as char), cast(tc.constraint_type as char), \
           (select cast(concat('FOREIGN KEY (', group_concat(k.column_name order by k.ordinal_position separator ', '), ') REFERENCES ', \
                   max(k.referenced_table_name), ' (', group_concat(k.referenced_column_name order by k.ordinal_position separator ', '), ')') as char) \
              from information_schema.key_column_usage k \
             where k.constraint_schema = tc.constraint_schema and k.constraint_name = tc.constraint_name and k.table_name = tc.table_name and k.referenced_table_name is not null) \
         from information_schema.table_constraints tc where tc.table_schema = ? and tc.table_name = ? order by tc.constraint_name",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let constraints = cons
        .into_iter()
        .filter_map(|(name, ty, fk)| {
            let kind = match ty.as_str() {
                "PRIMARY KEY" => "p",
                "FOREIGN KEY" => "f",
                "UNIQUE" => "u",
                "CHECK" => "c",
                _ => return None,
            };
            Some(ConstraintMeta { definition: fk.unwrap_or_else(|| ty.clone()), kind: kind.into(), name })
        })
        .collect();
    Ok(TableDetail { columns, indexes, constraints })
}

/// The Structure view: table_detail plus foreign keys both ways, triggers and table options
pub async fn table_structure(p: &MySqlPool, schema: &str, table: &str) -> AppResult<TableStructure> {
    let detail = table_detail(p, schema, table).await?;
    // Foreign keys of this table and those pointing at it (a self-reference is both)
    type Fk = (String, String, String, String, String, String, String, String, String);
    let fks: Vec<Fk> = sqlx::query_as(
        "select cast(k.constraint_name as char), cast(k.table_schema as char), cast(k.table_name as char), \
           cast(group_concat(k.column_name order by k.ordinal_position separator '\\t') as char), \
           cast(k.referenced_table_schema as char), cast(k.referenced_table_name as char), \
           cast(group_concat(k.referenced_column_name order by k.ordinal_position separator '\\t') as char), \
           cast(r.update_rule as char), cast(r.delete_rule as char) \
         from information_schema.key_column_usage k \
         join information_schema.referential_constraints r on r.constraint_schema = k.constraint_schema and r.constraint_name = k.constraint_name and r.table_name = k.table_name \
         where k.referenced_table_name is not null and ((k.table_schema = ? and k.table_name = ?) or (k.referenced_table_schema = ? and k.referenced_table_name = ?)) \
         group by k.constraint_schema, k.constraint_name, k.table_schema, k.table_name, k.referenced_table_schema, k.referenced_table_name, r.update_rule, r.delete_rule \
         order by 2, 3, 1",
    )
    .bind(schema)
    .bind(table)
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let split = |s: String| s.split('\t').map(String::from).collect::<Vec<_>>();
    let (mut foreign_keys, mut referenced_by) = (vec![], vec![]);
    for (name, schema_, table_, columns, ref_schema, ref_table, ref_columns, on_update, on_delete) in fks {
        let fk = ForeignKeyMeta { name, schema: schema_, table: table_, columns: split(columns), ref_schema, ref_table, ref_columns: split(ref_columns), on_update, on_delete };
        if (fk.ref_schema.as_str(), fk.ref_table.as_str()) == (schema, table) {
            referenced_by.push(fk.clone());
        }
        if (fk.schema.as_str(), fk.table.as_str()) == (schema, table) {
            foreign_keys.push(fk);
        }
    }
    let trg: Vec<(String, String, String, String)> = sqlx::query_as(
        "select cast(trigger_name as char), cast(action_timing as char), cast(event_manipulation as char), cast(action_statement as char) \
         from information_schema.triggers where event_object_schema = ? and event_object_table = ? order by action_timing, event_manipulation, action_order",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let triggers = trg
        .into_iter()
        .map(|(name, timing, events, body)| TriggerMeta {
            definition: format!("CREATE TRIGGER {} {timing} {events} ON {} FOR EACH ROW {body}", ident(&name), ident(table)),
            name,
            timing,
            events,
        })
        .collect();
    type S = Option<String>;
    let (engine, row_format, collation, charset, auto_inc, create_opts, comment): (S, S, S, S, S, S, S) = sqlx::query_as(
        "select cast(t.engine as char), cast(t.row_format as char), cast(t.table_collation as char), cast(c.character_set_name as char), \
           cast(t.auto_increment as char), cast(nullif(t.create_options, '') as char), cast(nullif(t.table_comment, '') as char) \
         from information_schema.tables t \
         left join information_schema.collation_character_set_applicability c on c.collation_name = t.table_collation \
         where t.table_schema = ? and t.table_name = ?",
    )
    .bind(schema)
    .bind(table)
    .fetch_one(p)
    .await?;
    let options = [("Engine", engine), ("Charset", charset), ("Collation", collation), ("Row format", row_format), ("Auto increment", auto_inc), ("Options", create_opts)]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k.to_string(), v)))
        .collect();
    Ok(TableStructure { detail, foreign_keys: Some(foreign_keys), referenced_by: Some(referenced_by), triggers: Some(triggers), options: Some(options), comment })
}

pub async fn object_ddl(p: &MySqlPool, schema: &str, name: &str, kind: &str) -> AppResult<String> {
    let fq = format!("{}.{}", ident(schema), ident(name));
    let mut conn = p.acquire().await?;
    let (sql, col) = match kind {
        "view" => (format!("SHOW CREATE VIEW {fq}"), 1),
        "routine" | "function" | "procedure" => {
            let ty: Option<String> = sqlx::query_scalar("select cast(routine_type as char) from information_schema.routines where routine_schema = ? and routine_name = ? limit 1")
                .bind(schema)
                .bind(name)
                .fetch_optional(&mut *conn)
                .await?;
            (format!("SHOW CREATE {} {fq}", ty.unwrap_or_else(|| "FUNCTION".into())), 2)
        }
        _ => (format!("SHOW CREATE TABLE {fq}"), 1),
    };
    let r = run_on(&mut conn, &sql, 1).await?;
    let ddl = r.rows.first().and_then(|row| row.get(col)).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    Ok(format!("{ddl};"))
}

pub async fn completion_meta(p: &MySqlPool) -> AppResult<Vec<CompletionTable>> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "select cast(table_schema as char), cast(table_name as char), cast(column_name as char), cast(column_type as char) \
         from information_schema.columns where table_schema = database() order by table_name, ordinal_position limit 30000",
    )
    .fetch_all(p)
    .await?;
    let mut out: Vec<CompletionTable> = Vec::new();
    for (schema, name, col, ty) in rows {
        match out.last_mut() {
            Some(t) if t.schema == schema && t.name == name => t.columns.push((col, ty)),
            _ => out.push(CompletionTable { schema, name, columns: vec![(col, ty)] }),
        }
    }
    Ok(out)
}

pub async fn object_index(p: &MySqlPool) -> AppResult<Vec<IndexEntry>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "select cast(database() as char), cast(database() as char), cast('schema' as char) from dual where database() is not null \
         union all \
         select cast(table_schema as char), cast(table_name as char), cast(if(table_type like '%VIEW%', 'view', 'table') as char) from information_schema.tables where table_schema = database() \
         union all \
         select cast(routine_schema as char), cast(routine_name as char), cast(lower(routine_type) as char) from information_schema.routines where routine_schema = database()",
    )
    .fetch_all(p)
    .await?;
    Ok(rows.into_iter().map(|(schema, name, kind)| IndexEntry { schema, name, kind }).collect())
}

pub async fn full_text_search(p: &MySqlPool, r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
    let schemas = if r.schemas.is_empty() && r.tables.is_empty() { vec![] } else { r.schemas.clone() };
    let mut cond_rel = Vec::new();
    if !schemas.is_empty() {
        cond_rel.push(format!("c.table_schema in ({})", schemas.iter().map(|s| lit(s)).collect::<Vec<_>>().join(",")));
    }
    for (s, t) in &r.tables {
        cond_rel.push(format!("(c.table_schema = {} and c.table_name = {})", lit(s), lit(t)));
    }
    if cond_rel.is_empty() {
        return Ok(vec![]);
    }
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(&format!(
        "select cast(c.table_schema as char), cast(c.table_name as char), cast(c.column_name as char), cast(c.data_type as char) \
         from information_schema.columns c join information_schema.tables t on t.table_schema = c.table_schema and t.table_name = c.table_name \
         where ({}) order by c.table_schema, c.table_name, c.ordinal_position",
        cond_rel.join(" or ")
    ))
    .fetch_all(p)
    .await?;
    const TEXT: [&str; 9] = ["char", "varchar", "tinytext", "text", "mediumtext", "longtext", "enum", "set", "json"];
    const NUM: [&str; 10] = ["tinyint", "smallint", "mediumint", "int", "bigint", "decimal", "float", "double", "bit", "year"];
    let numeric_text = r.text.trim().parse::<f64>().is_ok();
    let esc = |t: &str| t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let cond = |col: &str, numeric: bool| -> String {
        let c = format!("cast({} as char)", ident(col));
        let t = &r.text;
        if numeric {
            return format!("{c} = {}", lit(t.trim()));
        }
        let (lhs, rhs_wrap): (String, fn(String) -> String) =
            if r.match_case { (format!("{c} collate utf8mb4_bin"), |x| x) } else { (format!("lower({c})"), |x| format!("lower({x})")) };
        match r.mode.as_str() {
            "matches" => format!("{lhs} = {}", rhs_wrap(lit(t))),
            "starts" => format!("{lhs} like {}", rhs_wrap(lit(&format!("{}%", esc(t))))),
            "ends" => format!("{lhs} like {}", rhs_wrap(lit(&format!("%{}", esc(t))))),
            "like" => format!("{lhs} like {}", rhs_wrap(lit(t))),
            _ => format!("{lhs} like {}", rhs_wrap(lit(&format!("%{}%", esc(t))))),
        }
    };
    let mut tables: Vec<(String, String, Vec<String>)> = Vec::new();
    for (schema, table, col, ty) in rows {
        let ty = ty.to_lowercase();
        let numeric = NUM.contains(&ty.as_str());
        let take = match r.columns.as_str() {
            "numeric" => numeric && numeric_text,
            "all" => !numeric || numeric_text,
            _ => TEXT.contains(&ty.as_str()),
        };
        let c = if take { Some(cond(&col, numeric)) } else { None };
        match tables.last_mut() {
            Some((s, t, cs)) if *s == schema && *t == table => cs.extend(c),
            _ => tables.push((schema, table, c.into_iter().collect())),
        }
    }
    let mut conn = p.acquire().await?;
    let _ = conn.execute("SET SESSION max_execution_time = 15000").await;
    let mut hits = Vec::new();
    for (schema, table, conds) in tables.into_iter().filter(|t| !t.2.is_empty()) {
        let where_clause = conds.join(" OR ");
        let sql = format!("select * from {}.{} where {where_clause} limit {}", ident(&schema), ident(&table), r.limit.max(1));
        // READ ONLY and rolled back: a view or function with side effects (an INSERT in a stored function) is refused, not committed
        let res = match conn.execute("START TRANSACTION READ ONLY").await {
            Ok(_) => run_on(&mut conn, &sql, r.limit.max(1)).await,
            Err(e) => Err(e.into()),
        };
        let _ = conn.execute("ROLLBACK").await;
        match res {
            Ok(result) if !result.rows.is_empty() => hits.push(FtsHit { schema, table, where_clause, result }),
            Ok(_) => {}
            Err(e) => crate::log!("[qurry] fts {schema}.{table}: {e}"),
        }
    }
    let _ = conn.execute("SET SESSION max_execution_time = 0").await;
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Backend;
    use crate::db::Pool;
    use crate::store::ConnectionConfig;

    /// The console asks the server after any statement but a SELECT and a row change (they never commit the open transaction),
    /// whether it ran or failed
    #[test]
    fn asks_the_server_after_all_but_dml() {
        let a = |q: &str| asks_server(q, &strip_comments_lead(q));
        for q in ["UPDATE t SET a = 1", "-- x\ninsert into t values (1)", "delete from t", "replace into t values (1)", "load data infile 'x' into table t", "show tables", "explain select 1", "desc t", "select 1", "with x as (select 1) select * from x", "update/**/t set a = 1"] {
            assert!(!a(q), "{q}");
        }
        for q in [
            "set password = 'x'", "alter user u identified by 'x'", "install plugin p soname 'p.so'", "unlock tables", "lock tables t write", "set autocommit = 1",
            "set transaction isolation level read uncommitted", "call p()", "do 0", "load index into cache t", "start replica", "lock instance for backup", "updatex",
            "create temporary table x (a int)", "analyze update t set v = 1", "commit", "commit transaction",
        ] {
            assert!(a(q), "{q}");
        }
    }

    /// A CALL or EXECUTE can end the transaction either way (a procedure's ROLLBACK, a prepared COMMIT); other statements commit it
    #[test]
    fn call_and_execute_end_in_procedure() {
        for q in ["call p()", "CALL s.p", "-- x\n call p()", "execute stmt", "EXECUTE s USING @a", "call/**/p()", "execute/**/s"] {
            assert!(ends_in_procedure(&strip_comments_lead(q)), "{q}");
        }
        for q in ["create table t (a int)", "set password = 'x'", "callx", "do 0", "deallocate prepare s", "prepare s from 'commit'", "begin", "BEGIN WORK;", "begin ;"] {
            assert!(!ends_in_procedure(&strip_comments_lead(q)), "{q}");
        }
        for q in ["BEGIN NOT ATOMIC SELECT 1; END", "begin/*x*/not atomic select 1; end", "begin # x\n not atomic select 1; end"] {
            assert!(ends_in_procedure(&strip_comments_lead(q)), "{q}");
        }
    }

    /// Only a plain BEGIN commits the open transaction and opens one; a MariaDB compound statement asks the server
    #[test]
    fn plain_begin_only() {
        for q in ["begin", "BEGIN;", "-- x\nbegin work", "begin  work ;", "begin -- x", "BEGIN /* x */", "begin /* x */ work", "begin work -- go", "begin; -- note", "begin\n# x", "begin/* x */"] {
            assert!(is_begin(&strip_comments_lead(q)), "{q}");
            assert!(!ends_in_procedure(&strip_comments_lead(q)), "{q}");
        }
        for q in ["begin not atomic select 1; end", "BEGIN NOT ATOMIC\n  UPDATE t SET v = 1;\nEND", "begin /* x */ not atomic select 1; end", "begin/*x*/not atomic select 1; end", "begin # x\n not atomic select 1; end", "beginx", "begin_x"] {
            assert!(!is_begin(&strip_comments_lead(q)), "{q}");
        }
    }

    /// QURRY_TEST_MY=host:port:user:password:db (crate::testenv): the data source and its password
    fn server() -> Option<(ConnectionConfig, String)> {
        let crate::testenv::Server { mut cfg, password } = crate::testenv::my()?;
        (cfg.id, cfg.name) = ("my".into(), "my".into());
        Some((cfg, password.unwrap_or_default()))
    }

    /// Notify when a source without SSL set connected in plaintext (silent when connected over TLS)
    #[tokio::test]
    #[ignore]
    async fn mysql_plaintext_is_reported() {
        let Some((cfg, pw)) = server() else { return };
        // Treat it as a remote server (this machine · tunnels aren't notified)
        let remote = ConnectionConfig { host: "db.example".into(), ..cfg.clone() };
        // Default connection: silent if TLS works; notify if it fell back to plaintext (MySQL 5.7 without TLS 1.2)
        let Pool::My(auto) = crate::db::connect(&cfg, Some(pw.as_str())).await.expect("connect") else { unreachable!() };
        let (_, cipher): (String, String) = sqlx::query_as("SHOW SESSION STATUS LIKE 'Ssl_cipher'").fetch_one(&auto).await.unwrap();
        assert_eq!(auto.plaintext_note(&remote).await.is_some(), cipher.is_empty());
        let opts = sqlx::mysql::MySqlConnectOptions::new().host(&cfg.host).port(cfg.port).username(&cfg.user).password(&pw).ssl_mode(sqlx::mysql::MySqlSslMode::Disabled);
        let plain = sqlx::mysql::MySqlPoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
        assert!(plain.plaintext_note(&remote).await.is_some_and(|n| n.contains("Not encrypted")));
        assert_eq!(plain.plaintext_note(&cfg).await, None, "this computer: nothing to intercept");
    }

    /// MCP execute path: the prepared-statement protocol takes exactly one statement
    #[tokio::test]
    #[ignore]
    async fn mysql_prepared_batch_refuses_multiple_statements() {
        let Some((cfg, pw)) = server() else { return };
        let Pool::My(p) = crate::db::connect(&cfg, Some(pw.as_str())).await.expect("connect") else { unreachable!() };
        sqlx::raw_sql("drop table if exists qurry_prep; create table qurry_prep (a int)").execute(&p).await.unwrap();
        assert!(p.execute_batch(&["insert into qurry_prep values (1); drop table qurry_prep".into()], false, true, &[]).await.is_err());
        assert_eq!(p.execute_batch(&["insert into qurry_prep values (1)".into()], false, true, &[]).await.unwrap().0, 1);
        sqlx::raw_sql("drop table qurry_prep").execute(&p).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_end_to_end() {
        let Some((cfg, pw)) = server() else { return };
        let pool = crate::db::connect(&cfg, Some(pw.as_str())).await.expect("connect");
        assert!(pool.server_version().await.unwrap().starts_with("MySQL"));
        let Pool::My(p) = &pool else { unreachable!() };
        for s in [
            "drop table if exists order_item",
            "drop table if exists orders",
            "create table orders(id bigint unsigned primary key auto_increment, customer varchar(50) not null comment '고객', amount decimal(12,2), paid bit(1) default b'0', created datetime(3) default current_timestamp(3), memo text, img blob, unique key uq_customer(customer)) default charset=utf8mb4 comment '주문'",
            "create table order_item(id int primary key, order_id bigint unsigned, qty smallint, constraint fk_order foreign key (order_id) references orders(id))",
            "insert into orders(customer, amount, paid, memo, img) values ('alice', 12345.67, b'1', 'Hello_World 50%', x'DEADBEEF'), ('bob', null, b'0', null, null), ('carol', 1, b'0', '한글 메모', null)",
            "drop procedure if exists add_qty",
            "create procedure add_qty(in p_id int, in p_n int) begin update order_item set qty = qty + p_n where id = p_id; end",
            "create or replace view big_orders as select * from orders where amount > 100",
        ] {
            sqlx::raw_sql(s).execute(p).await.unwrap_or_else(|e| panic!("{s}: {e}"));
        }
        // Value conversion
        let r = pool.run_query("select id, customer, amount, paid, created, memo, img, 9007199254740993 as big, 1.5e0 as f from orders order by id", 500).await.unwrap();
        println!("{:?}\n{:?}", r.columns, r.rows[0]);
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0][1], "alice");
        assert_eq!(r.rows[0][2], "12345.67");
        assert_eq!(r.rows[0][3], 1);
        assert_eq!(r.rows[0][6], "0xDEADBEEF");
        assert_eq!(r.rows[0][7], "9007199254740993");
        assert_eq!(r.rows[2][5], "한글 메모");
        assert!(r.rows[1][2].is_null());
        // A 0-row SELECT returns columns too
        let z = pool.run_query("select id, customer from orders where 1 = 0", 10).await.unwrap();
        assert!(z.has_result_set && z.columns.len() == 2 && z.rows.is_empty());
        // DML
        let u = pool.run_query("update orders set memo = 'x' where customer = 'bob'", 10).await.unwrap();
        assert_eq!(u.rows_affected, 1);
        // Metadata
        let dbs = crate::meta::list_databases(&pool).await.unwrap();
        assert!(dbs.contains(&"shop".to_string()));
        let sch = crate::meta::list_schemas(&pool).await.unwrap();
        assert_eq!(sch[0].name, "shop");
        let objs = crate::meta::list_objects(&pool, "shop").await.unwrap();
        let rels: Vec<_> = objs.relations.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect();
        assert!(rels.contains(&("orders", "table")) && rels.contains(&("big_orders", "view")), "{rels:?}");
        assert!(objs.routines.iter().any(|r| r.name == "add_qty" && r.kind == "procedure" && r.args.contains("p_id")), "{:?}", objs.routines.iter().map(|r| (&r.name, &r.args)).collect::<Vec<_>>());
        let d = crate::meta::table_detail(&pool, "shop", "order_item").await.unwrap();
        assert!(d.columns.iter().any(|c| c.name == "id" && c.is_pk));
        assert!(d.constraints.iter().any(|c| c.kind == "f" && c.definition.contains("REFERENCES orders")), "{:?}", d.constraints.iter().map(|c| &c.definition).collect::<Vec<_>>());
        let d2 = crate::meta::table_detail(&pool, "shop", "orders").await.unwrap();
        assert!(d2.indexes.iter().any(|i| i.name == "uq_customer" && i.is_unique));
        assert_eq!(d2.columns.iter().find(|c| c.name == "customer").unwrap().comment.as_deref(), Some("고객"));
        let ddl = crate::meta::object_ddl(&pool, "shop", "orders", "table").await.unwrap();
        assert!(ddl.starts_with("CREATE TABLE `orders`"), "{ddl}");
        let pddl = crate::meta::object_ddl(&pool, "shop", "add_qty", "routine").await.unwrap();
        assert!(pddl.contains("PROCEDURE `add_qty`"), "{pddl}");
        let vddl = crate::meta::object_ddl(&pool, "shop", "big_orders", "view").await.unwrap();
        assert!(vddl.contains("VIEW `big_orders`"), "{vddl}");
        let comp = crate::meta::completion_meta(&pool).await.unwrap();
        assert!(comp.iter().any(|t| t.name == "orders" && t.columns.iter().any(|c| c.0 == "customer")));
        let idx = crate::meta::object_index(&pool).await.unwrap();
        assert!(idx.iter().any(|e| e.name == "add_qty" && e.kind == "procedure") && idx.iter().any(|e| e.kind == "schema"));
        // Full-text Search
        let mk = |text: &str, mode: &str, case: bool, cols: &str| crate::meta::FtsRequest {
            text: text.into(), mode: mode.into(), match_case: case, columns: cols.into(), schemas: vec!["shop".into()], tables: vec![], limit: 10,
        };
        let h = crate::meta::full_text_search(&pool, &mk("world", "contains", false, "text")).await.unwrap();
        let tabs: Vec<_> = h.iter().map(|x| x.table.as_str()).collect();
        assert_eq!(tabs, ["big_orders", "orders"], "view big_orders also contains alice");
        assert!(crate::meta::full_text_search(&pool, &mk("world", "contains", true, "text")).await.unwrap().is_empty());
        assert_eq!(crate::meta::full_text_search(&pool, &mk("50%", "ends", false, "text")).await.unwrap().len(), 2);
        assert!(crate::meta::full_text_search(&pool, &mk("Hello_", "ends", false, "text")).await.unwrap().is_empty());
        assert_eq!(crate::meta::full_text_search(&pool, &mk("한글", "starts", false, "text")).await.unwrap().len(), 1);
        let w = &h[1].where_clause;
        let n = pool.run_query(&format!("select count(*) from orders where {w}"), 1).await.unwrap();
        assert_eq!(n.rows[0][0], 1);
        // Session: load more, Manual Tx, cancel id
        let mut s = crate::session::Session::open(&pool).await.unwrap();
        assert!(s.backend_id() > 0);
        let first = s.run("select * from orders order by id", 2).await.unwrap();
        assert!(first.truncated && first.rows.len() == 2);
        let more = s.fetch(2).await.unwrap();
        assert_eq!(more.rows.len(), 1);
        assert!(!more.truncated);
        s.set_manual(true).await.unwrap();
        let ins = s.run("insert into orders(customer) values ('dave')", 10).await.unwrap();
        assert!(ins.in_transaction);
        // Not visible from another connection yet
        let other = pool.run_query("select count(*) from orders where customer = 'dave'", 1).await.unwrap();
        assert_eq!(other.rows[0][0], 0);
        s.rollback_tx().await.unwrap();
        let after = s.run("select count(*) from orders where customer = 'dave'", 1).await.unwrap();
        assert_eq!(after.rows[0][0], 0);
        // With autocommit=0 the SELECT opens a transaction too (as in PostgreSQL's Manual mode)
        assert!(after.in_transaction);
        assert!(s.set_manual(false).await.is_err());
        s.rollback_tx().await.unwrap();
        s.set_manual(false).await.unwrap();
        // Edit batch
        p.execute_batch(&["update orders set memo = 'y' where id = 1".into()], true, false, &[]).await.unwrap();
        // Read-only
        let mut ro = cfg.clone();
        ro.read_only = true;
        ro.options.time_zone = Some("+09:00".into());
        let rp = crate::db::connect(&ro, Some(pw.as_str())).await.unwrap();
        let tz = rp.run_query("select @@session.time_zone", 1).await.unwrap();
        assert_eq!(tz.rows[0][0], "+09:00");
        let e = rp.run_query("update orders set memo = 'z' where id = 1", 1).await;
        assert!(e.is_err() && format!("{}", e.unwrap_err()).to_lowercase().contains("read only"));
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_ssl() {
        let Some((cfg, pw)) = server() else { return };
        let mut c = cfg;
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "require".into(), ..Default::default() });
        let pool = crate::db::connect(&c, Some(pw.as_str())).await.expect("ssl require");
        let r = pool.run_query("show session status like 'Ssl_version'", 5).await.unwrap();
        assert!(r.rows[0][1].as_str().unwrap().starts_with("TLS"), "{:?}", r.rows);
        // Self-signed certificate, so verify mode must fail
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "verify-full".into(), ..Default::default() });
        assert!(crate::db::connect(&c, Some(pw.as_str())).await.is_err());
    }
}
