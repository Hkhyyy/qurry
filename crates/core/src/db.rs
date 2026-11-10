//! DB connections and query execution. Only PostgreSQL is supported for now.
use crate::backend::{Backend, dispatch};
use crate::error::AppResult;
use crate::store::{ConnectionConfig, DbKind};
use futures::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::{Column, Either, Executor, Row, TypeInfo, ValueRef};
use std::time::{Duration, Instant};
use crate::zone::Zone;

#[derive(Clone)]
pub enum Pool {
    Pg(PgPool),
    My(sqlx::MySqlPool),
    /// (connection, DB index this pool points to)
    Redis(crate::redisdb::RedisConn, i64),
    /// Driver plugin (separate process)
    Plugin(std::sync::Arc<crate::plugin::PluginConn>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    pub truncated: bool,
    pub elapsed_ms: u64,
    pub has_result_set: bool,
    /// Tx: whether an uncommitted transaction is open in Manual mode
    #[serde(default)]
    pub in_transaction: bool,
    /// It ran, but something after it failed (e.g. its result couldn't be written to the Production log): show it, don't offer to run it again
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// A console SELECT's result: the table its rows can be edited in (`edit`), or why they can't (`readOnlyReason`).
    /// None: not checked (not a console run, no result set, or the check couldn't tell)
    #[serde(flatten)]
    pub editable: Option<crate::editable::Editable>,
}

pub(crate) fn query_result(columns: Vec<ColumnInfo>, rows: Vec<Vec<Value>>, rows_affected: u64, truncated: bool, start: Instant, has_result_set: bool) -> QueryResult {
    QueryResult { columns, rows, rows_affected, truncated, elapsed_ms: start.elapsed().as_millis() as u64, has_result_set, in_transaction: false, warning: None, editable: None }
}

/// Max connections per DB (number of tabs + metadata queries such as the tree and autocomplete)
pub const POOL_MAX: u32 = 32;

/// Whether it is a server on this machine or goes through an SSH / AWS SSM tunnel (encrypted to the tunnel's far end)
pub fn is_local(cfg: &ConnectionConfig) -> bool {
    cfg.tunneled() || is_loopback(&cfg.host)
}

pub fn is_loopback(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']').to_lowercase();
    // A real loopback address (127.0.0.0/8, ::1, also as ::ffff:127.x.x.x) — not a name that merely starts with 127.
    // 0.0.0.0 / :: as a destination is this machine too. A host starting with / is a Unix socket directory (PostgreSQL), which never leaves the machine
    let local = |ip: std::net::IpAddr| ip.is_loopback() || ip.is_unspecified();
    h == "localhost"
        || h.starts_with('/')
        || h.parse::<std::net::IpAddr>().is_ok_and(|ip| local(ip) || matches!(ip, std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some_and(|v4| local(v4.into()))))
}

/// The SSL modes Qurry knows; anything else is refused instead of being passed on to a driver or tool
pub fn check_ssl_mode(mode: &str) -> crate::error::AppResult<()> {
    match mode {
        "require" | "verify-ca" | "verify-full" => Ok(()),
        m => Err(crate::error::AppError::Msg(format!("Unknown SSL mode '{m}' (use require, verify-ca or verify-full)"))),
    }
}

/// Whether a connection with no SSL setting may connect without TLS: the actual endpoint is this machine (including tunnels), or the source allows it
pub fn plaintext_allowed(cfg: &ConnectionConfig, connect_host: &str) -> bool {
    cfg.options.allow_plaintext || is_local(cfg) || is_loopback(connect_host)
}

/// Close idle connections after 1 minute (so connections opened in bulk while reading the tree or search index do not linger on production servers). Connections held by tabs are not idle and stay
pub const POOL_IDLE: Duration = Duration::from_secs(60);

pub async fn connect(cfg: &ConnectionConfig, password: Option<&str>) -> AppResult<Pool> {
    connect_via(cfg, password, None).await
}

/// If `tunnel_port` is set, connect to the tunnel's local port.
pub async fn connect_via(cfg: &ConnectionConfig, password: Option<&str>, tunnel_port: Option<u16>) -> AppResult<Pool> {
    let (host, port) = match tunnel_port {
        Some(p) => ("127.0.0.1", p),
        None => (cfg.host.as_str(), cfg.port),
    };
    match cfg.kind {
        DbKind::Sqlite | DbKind::Mssql | DbKind::Clickhouse | DbKind::Duckdb | DbKind::Mongodb | DbKind::Oracle | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus => Ok(Pool::Plugin(crate::plugin::connect(cfg, password, tunnel_port).await?)),
        DbKind::Postgres => Ok(Pool::Pg(crate::backend::pg::connect(cfg, password, host, port).await?)),
        DbKind::Mysql => Ok(Pool::My(crate::backend::mysql::connect(cfg, password, host, port).await?)),
        DbKind::Redis => Ok(Pool::Redis(crate::redisdb::connect(cfg, password, host, port).await?, crate::redisdb::db_index(&cfg.database))),
    }
}

/// Warn when the server is older than the tested version (mostly works, but not verified)
pub fn version_note(kind: DbKind, version: &str) -> Option<String> {
    // The first digits in the version text (years like 2017 too)
    let nums: Vec<u32> = version.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    let first = *nums.first()?;
    let lower = version.to_lowercase();
    let (min, label): (u32, &str) = match kind {
        DbKind::Postgres => (12, "PostgreSQL 12"),
        DbKind::Mysql if lower.contains("mariadb") => (10, "MariaDB 10.5"),
        DbKind::Mysql => (5, "MySQL 5.7"),
        // "Microsoft SQL Server 2017 (RTM-CU…" → year
        DbKind::Mssql => (2017, "SQL Server 2017"),
        DbKind::Mongodb => (5, "MongoDB 5"),
        DbKind::Clickhouse => (22, "ClickHouse 22"),
        // "Oracle Database 19c …" · "… 23ai"
        DbKind::Oracle => (19, "Oracle 19c"),
        _ => return None,
    };
    let v = if kind == DbKind::Mssql { nums.iter().copied().find(|n| *n >= 2000).unwrap_or(first) } else { first };
    let old = v < min || (kind == DbKind::Mysql && !lower.contains("mariadb") && v == 5 && nums.get(1).is_some_and(|m| *m < 7)) || (kind == DbKind::Mysql && lower.contains("mariadb") && v == 10 && nums.get(1).is_some_and(|m| *m < 5));
    old.then(|| format!("⚠ This server is older than the versions Qurry is tested with ({label} and newer). Most things should work; some explorer details may not."))
}

#[cfg(test)]
mod version_tests {
    use super::*;
    #[test]
    fn notes_old_servers_only() {
        assert!(version_note(DbKind::Postgres, "PostgreSQL 11.9 on x86_64").is_some());
        assert!(version_note(DbKind::Postgres, "PostgreSQL 16.4").is_none());
        assert!(version_note(DbKind::Mysql, "MySQL 5.6.51").is_some());
        assert!(version_note(DbKind::Mysql, "MySQL 5.7.44").is_none());
        assert!(version_note(DbKind::Mysql, "MariaDB 10.4.1").is_some());
        assert!(version_note(DbKind::Mysql, "MariaDB 11.4.2").is_none());
        assert!(version_note(DbKind::Mssql, "Microsoft SQL Server 2016 (SP3) - 13.0.6300.2").is_some());
        assert!(version_note(DbKind::Mssql, "Microsoft SQL Server 2022 (RTM-CU14) - 16.0.4135.4").is_none());
        assert!(version_note(DbKind::Oracle, "Oracle Database 11g Enterprise Edition Release 11.2.0.4.0").is_some());
        assert!(version_note(DbKind::Oracle, "Oracle Database 23ai Free Release 23.0.0.0.0").is_none());
        assert!(version_note(DbKind::Mongodb, "MongoDB 4.4.29").is_some());
        assert!(version_note(DbKind::Sqlite, "SQLite 3.46.0").is_none());
    }
}

impl Pool {
    pub async fn server_version(&self) -> AppResult<String> {
        dispatch!(self, b => b.server_version().await)
    }

    pub async fn close(&self) {
        dispatch!(self, b => Backend::close(b).await)
    }

    /// Rerun the query and export the full result (out decides the format and target)
    pub async fn export(&self, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
        dispatch!(self, b => b.export(sql, out).await)
    }

    /// To a CSV file (with BOM). Row count
    pub async fn export_csv(&self, sql: &str, path: &std::path::Path) -> AppResult<u64> {
        let mut out = crate::transfer::Exporter::csv_file(path).await?;
        self.export(sql, &mut out).await?;
        Ok(out.finish().await?.rows)
    }

    /// Run one query and return up to `limit` rows.
    pub async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        dispatch!(self, b => b.run_query(sql, limit).await)
    }

    /// keep-alive. Cut off at 5 seconds so one unresponsive server does not stall the cleanup routine. → whether it ended in time
    pub async fn ping(&self) -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(5), async { dispatch!(self, b => b.ping().await) }).await.is_ok()
    }
}

/// Run one statement on an already acquired connection (transaction). With 0 rows the columns come from describe
/// `pool`: where to read the session's time zone for timestamptz when the result is cut off at `limit` (a connection of the same
/// pool has the same zone, and reading it on `conn` would first wait for the rest of the result); None reads it on `conn` (a
/// transaction that ends right after reads the rest anyway)
pub(crate) async fn run_pg_on(conn: &mut sqlx::PgConnection, sql: &str, limit: usize, pool: Option<&PgPool>) -> AppResult<QueryResult> {
    let start = Instant::now();
    let mut columns = Vec::new();
    let mut rows: Vec<PgRow> = Vec::new();
    let mut rows_affected = 0u64;
    let mut truncated = false;
    let mut has_result_set = false;
    {
        #[allow(deprecated)]
        let mut stream = match crate::session::is_generic_plan(sql) {
            true => sqlx::raw_sql(sql).fetch_many(&mut *conn),
            false => sqlx::query(sql).persistent(false).fetch_many(&mut *conn),
        };
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Left(done) => rows_affected += done.rows_affected(),
                Either::Right(row) => {
                    has_result_set = true;
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
    let mut zone = None;
    if let Some(p) = pool.filter(|_| truncated && columns.iter().any(|c| zoned(&c.type_name))) {
        zone = pool_zone(p).await;
    }
    let rows = pg_values(conn, &columns, &rows, &mut zone).await;
    // With 0 rows the column info is unknown, so fill it in with describe.
    if !has_result_set {
        if let Ok(desc) = conn.describe(sql).await {
            if !desc.columns().is_empty() {
                has_result_set = true;
                columns = desc
                    .columns()
                    .iter()
                    .map(|c| ColumnInfo {
                        name: c.name().to_string(),
                        type_name: c.type_info().name().to_string(),
                    })
                    .collect();
            }
        }
    }
    Ok(QueryResult {
        columns,
        rows,
        rows_affected,
        truncated,
        elapsed_ms: start.elapsed().as_millis() as u64,
        has_result_set,
        in_transaction: false,
        warning: None,
        editable: None,
    })
}

/// Run a SELECT on an already acquired connection (transaction) into a QueryResult (for Full-text Search)
pub async fn run_on(conn: &mut sqlx::PgConnection, sql: &str, limit: usize) -> AppResult<QueryResult> {
    let start = Instant::now();
    let mut columns = Vec::new();
    let mut rows: Vec<PgRow> = Vec::new();
    let mut truncated = false;
    {
        let mut stream = sqlx::query(sql).persistent(false).fetch(&mut *conn);
        while let Some(row) = stream.try_next().await? {
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
    let rows = pg_values(conn, &columns, &rows, &mut None).await;
    Ok(QueryResult { columns, rows, rows_affected: 0, truncated, elapsed_ms: start.elapsed().as_millis() as u64, has_result_set: true, in_transaction: false, warning: None, editable: None })
}

/// Export on an already acquired connection of `pool` (possibly a read-only transaction). The time zone for timestamptz is read
/// on another connection of the pool (same after_connect settings), when the first row has such a column: `conn` is busy streaming
pub async fn export_pg_on(conn: &mut sqlx::PgConnection, pool: &PgPool, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
    let mut zone = Zone::UTC;
    let mut stream = sqlx::query(sql).persistent(false).fetch(&mut *conn);
    while let Some(row) = stream.try_next().await? {
        if !out.has_header() {
            let names: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
            out.header(&names).await?;
            if row.columns().iter().any(|c| zoned(c.type_info().name())) {
                zone = pool_zone(pool).await.unwrap_or(Zone::UTC);
            }
        }
        let vals: Vec<Value> = (0..row.len()).map(|i| pg_value(&row, i, &zone)).collect();
        out.row(&vals).await?;
    }
    drop(stream);
    if !out.has_header() {
        if let Ok(desc) = (&mut *conn).describe(sql).await {
            let names: Vec<String> = desc.columns().iter().map(|c| c.name().to_string()).collect();
            out.header(&names).await?;
        }
    }
    Ok(())
}

pub(crate) fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Where each column comes from: (table OID, attribute number), None for a computed one
pub(crate) fn pg_origins(cols: &[sqlx::postgres::PgColumn]) -> Vec<Option<(u32, i16)>> {
    cols.iter().map(|c| c.relation_id().zip(c.relation_attribute_no()).map(|(o, a)| (o.0, a))).collect()
}

pub(crate) fn pg_columns(row: &PgRow) -> Vec<ColumnInfo> {
    row.columns()
        .iter()
        .map(|c| ColumnInfo {
            name: c.name().to_string(),
            type_name: c.type_info().name().to_string(),
        })
        .collect()
}

/// Large integers not exactly representable as JS numbers are sent as strings.
pub(crate) fn int_value(v: i64) -> Value {
    if v.unsigned_abs() <= (1u64 << 53) {
        Value::from(v)
    } else {
        Value::from(v.to_string())
    }
}

/// Whether a column's text needs the session's time zone (pg_zone)
fn zoned(ty: &str) -> bool {
    matches!(ty, "TIMESTAMPTZ" | "TIMESTAMPTZ[]")
}

/// The session's time zone: its TimeZone setting, and the offset the server says it has now (for a setting the tz database here
/// doesn't name, such as `+09:00`). None when it couldn't be read
pub(crate) async fn pg_zone(conn: &mut sqlx::PgConnection) -> Option<Zone> {
    let (name, now): (String, i32) = sqlx::query_as("select pg_catalog.current_setting('TimeZone'), extract(timezone from pg_catalog.now())::int4")
        .persistent(false)
        .fetch_one(conn)
        .await
        .ok()?;
    Some(Zone::new(&name, now))
}

/// The time zone of the pool's sessions, read on a connection of its own
async fn pool_zone(pool: &PgPool) -> Option<Zone> {
    pg_zone(&mut *pool.acquire().await.ok()?).await
}

/// Rows read on `conn` → values. The session's time zone is read (once, into `zone`) only when a column is timestamptz, after
/// the rows: the connection is free then
pub(crate) async fn pg_values(conn: &mut sqlx::PgConnection, columns: &[ColumnInfo], rows: &[PgRow], zone: &mut Option<Zone>) -> Vec<Vec<Value>> {
    if zone.is_none() && !rows.is_empty() && columns.iter().any(|c| zoned(&c.type_name)) {
        *zone = pg_zone(conn).await;
    }
    let z = zone.as_ref().unwrap_or(&Zone::UTC);
    rows.iter().map(|r| (0..r.len()).map(|i| pg_value(r, i, z)).collect()).collect()
}

pub(crate) fn pg_value(row: &PgRow, i: usize, zone: &Zone) -> Value {
    use chrono::NaiveTime;
    let raw = match row.try_get_raw(i) {
        Ok(r) => r,
        Err(_) => return Value::Null,
    };
    if raw.is_null() {
        return Value::Null;
    }
    let ty = raw.type_info().name().to_string();
    if matches!(raw.format(), sqlx::postgres::PgValueFormat::Text) {
        return text_value(&ty, raw.as_str().unwrap_or_default());
    }
    macro_rules! get {
        ($t:ty) => {
            row.try_get::<$t, _>(i).ok()
        };
    }
    let v: Option<Value> = match ty.as_str() {
        "BOOL" => get!(bool).map(Value::from),
        "INT2" => get!(i16).map(Value::from),
        "INT4" => get!(i32).map(Value::from),
        "INT8" => get!(i64).map(int_value),
        "OID" => get!(sqlx::postgres::types::Oid).map(|o| Value::from(o.0)),
        "FLOAT4" => get!(f32).map(|f| Value::from(f as f64)),
        "FLOAT8" => get!(f64).map(Value::from),
        "NUMERIC" => raw
            .as_bytes()
            .ok()
            .and_then(decode_pg_numeric)
            .map(Value::from),
        "UUID" => get!(uuid::Uuid).map(|u| Value::from(u.to_string())),
        "JSON" | "JSONB" => get!(Value).map(|j| Value::from(j.to_string())),
        "TIMESTAMP" | "TIMESTAMPTZ" | "DATE" => raw.as_bytes().ok().and_then(|b| pg_datetime(&ty, b, zone)).map(Value::from),
        "TIMESTAMP[]" | "TIMESTAMPTZ[]" | "DATE[]" => {
            let elem = ty.trim_end_matches("[]");
            raw.as_bytes().ok().and_then(|b| pg_array(b, &|e| pg_datetime(elem, e, zone))).map(Value::from)
        }
        "TIME" => get!(NaiveTime).map(|t| Value::from(t.to_string())),
        "BYTEA" => get!(Vec<u8>).map(|b| {
            let hex: String = b.iter().take(64).map(|x| format!("{x:02X}")).collect();
            Value::from(format!("0x{hex}{}", if b.len() > 64 { "…" } else { "" }))
        }),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "CITEXT" | "XML" => {
            get!(String).map(Value::from)
        }
        _ => None,
    };
    v.unwrap_or_else(|| {
        let bytes = raw.as_bytes().ok();
        bytes.and_then(|b| decode_pgvector(&ty, b)).map(Value::from).unwrap_or_else(|| fallback(bytes, &ty))
    })
}

/// PostgreSQL timestamp / timestamptz / date (binary) → text. Decoded here, not with sqlx's chrono types, which panic on
/// infinity (i64::MAX / i64::MIN microseconds, i32::MAX / i32::MIN days) and past chrono's range. Shows infinity / -infinity
/// as psql does; None for anything chrono can't hold. timestamptz shows in the session's time zone (`zone`) with its offset, as psql
fn pg_datetime(ty: &str, b: &[u8], zone: &Zone) -> Option<String> {
    use chrono::{NaiveDate, NaiveTime, TimeDelta};
    let epoch = NaiveDate::from_ymd_opt(2000, 1, 1)?;
    match ty {
        "TIMESTAMP" | "TIMESTAMPTZ" => match i64::from_be_bytes(b.try_into().ok()?) {
            i64::MAX => Some("infinity".into()),
            i64::MIN => Some("-infinity".into()),
            us => {
                let t = epoch.and_time(NaiveTime::MIN).checked_add_signed(TimeDelta::microseconds(us))?;
                match ty {
                    "TIMESTAMP" => Some(t.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
                    _ => zone.format(t),
                }
            }
        },
        "DATE" => match i32::from_be_bytes(b.try_into().ok()?) {
            i32::MAX => Some("infinity".into()),
            i32::MIN => Some("-infinity".into()),
            days => Some(epoch.checked_add_signed(TimeDelta::try_days(days.into())?)?.to_string()),
        },
        _ => None,
    }
}

/// PostgreSQL array of dates / times (binary) → the same text as PG's output, e.g. {infinity,NULL,"2024-01-02 00:00:00"}, each element through `elem`.
/// Format: ndim(i32) has_null(i32) elem_oid(u32) [len(i32) lbound(i32)] × ndim, then each element as len(i32, -1 = NULL) + bytes
fn pg_array(b: &[u8], elem: &dyn Fn(&[u8]) -> Option<String>) -> Option<String> {
    let i32_at = |i: usize| -> Option<i32> { Some(i32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
    let ndim = usize::try_from(i32_at(0)?).ok()?;
    if ndim == 0 {
        return Some("{}".into());
    }
    let dims: Vec<usize> = (0..ndim).map(|d| i32_at(12 + d * 8).and_then(|n| usize::try_from(n).ok())).collect::<Option<_>>()?;
    let mut pos = 12 + ndim * 8;
    let mut items = Vec::new();
    for _ in 0..dims.iter().try_fold(1usize, |a, &n| a.checked_mul(n))? {
        let len = i32_at(pos)?;
        pos += 4;
        if len < 0 {
            items.push("NULL".to_string());
            continue;
        }
        let s = elem(b.get(pos..pos + len as usize)?)?;
        pos += len as usize;
        items.push(if s.contains(' ') { format!("\"{s}\"") } else { s });
    }
    // Group the flat elements by each dimension, innermost first
    for &n in dims.iter().rev() {
        items = items.chunks(n.max(1)).map(|c| format!("{{{}}}", c.join(","))).collect();
    }
    items.pop()
}

/// pgvector binary → the same text as PG's text output: vector/halfvec "[1,0.5]", sparsevec "{1:0.5,3:1}/5"
/// vector: dim(u16) unused(u16) f32[dim] · halfvec: same header + f16[dim] · sparsevec: dim(i32) nnz(i32) unused(i32) idx(i32)[nnz] f32[nnz]
fn decode_pgvector(ty: &str, b: &[u8]) -> Option<String> {
    let u16_at = |i: usize| -> Option<u16> { Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?])) };
    let u32_at = |i: usize| -> Option<u32> { Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
    let f = |x: f32| x.to_string();
    match ty.to_lowercase().as_str() {
        "vector" => {
            let dim = u16_at(0)? as usize;
            let v: Option<Vec<String>> = (0..dim).map(|k| u32_at(4 + k * 4).map(|x| f(f32::from_bits(x)))).collect();
            Some(format!("[{}]", v?.join(",")))
        }
        "halfvec" => {
            let dim = u16_at(0)? as usize;
            let v: Option<Vec<String>> = (0..dim).map(|k| u16_at(4 + k * 2).map(|h| f(f16_to_f32(h)))).collect();
            Some(format!("[{}]", v?.join(",")))
        }
        "sparsevec" => {
            let (dim, nnz) = (u32_at(0)?, u32_at(4)? as usize);
            let idx: Option<Vec<u32>> = (0..nnz).map(|k| u32_at(12 + k * 4)).collect();
            let vals: Option<Vec<f32>> = (0..nnz).map(|k| u32_at(12 + nnz * 4 + k * 4).map(f32::from_bits)).collect();
            let parts: Vec<String> = idx?.iter().zip(vals?).map(|(i, v)| format!("{}:{}", i + 1, f(v))).collect();
            Some(format!("{{{}}}/{dim}", parts.join(",")))
        }
        _ => None,
    }
}

/// IEEE 754 half precision → f32
fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let frac = (h & 0x3ff) as f32;
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => if frac == 0.0 { sign * f32::INFINITY } else { f32::NAN },
        e => sign * (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}

/// Value received via the simple query protocol (text format)
fn text_value(ty: &str, s: &str) -> Value {
    match ty {
        "BOOL" => Value::from(s == "t"),
        "INT2" | "INT4" => s.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::from(s)),
        "INT8" => s.parse::<i64>().map(int_value).unwrap_or_else(|_| Value::from(s)),
        "FLOAT4" | "FLOAT8" => s.parse::<f64>().ok().filter(|f| f.is_finite()).map(Value::from).unwrap_or_else(|| Value::from(s)),
        "BYTEA" => Value::from(s.replacen("\\x", "0x", 1).to_uppercase().replacen("0X", "0x", 1)),
        _ => Value::from(s),
    }
}

/// PostgreSQL NUMERIC binary → the same string as PG's text output (dscale preserved).
/// Format: ndigits(i16) weight(i16) sign(u16) dscale(u16) digits[base 10000 i16]
fn decode_pg_numeric(b: &[u8]) -> Option<String> {
    let rd = |i: usize| -> Option<i16> { Some(i16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?])) };
    let ndigits = rd(0)? as usize;
    let weight = rd(2)? as i32;
    let sign = rd(4)? as u16;
    let dscale = rd(6)? as usize;
    match sign {
        0xC000 => return Some("NaN".into()),
        0xD000 => return Some("Infinity".into()),
        0xF000 => return Some("-Infinity".into()),
        _ => {}
    }
    let digits: Vec<i16> = (0..ndigits).map(|k| rd(8 + k * 2)).collect::<Option<_>>()?;
    let digit = |pos: i32| -> i16 {
        if pos >= 0 && (pos as usize) < digits.len() {
            digits[pos as usize]
        } else {
            0
        }
    };
    let mut int_part = String::new();
    if weight < 0 {
        int_part.push('0');
    } else {
        for k in 0..=weight {
            let d = digit(k);
            if k == 0 {
                int_part.push_str(&d.to_string());
            } else {
                int_part.push_str(&format!("{d:04}"));
            }
        }
    }
    let mut frac = String::new();
    let mut k = weight + 1;
    while frac.len() < dscale {
        frac.push_str(&format!("{:04}", digit(k)));
        k += 1;
    }
    frac.truncate(dscale);
    let mut out = String::new();
    if sign == 0x4000 {
        out.push('-');
    }
    out.push_str(&int_part);
    if dscale > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

/// Unknown types: shown as is if UTF-8 bytes, otherwise only the type name.
fn fallback(bytes: Option<&[u8]>, ty: &str) -> Value {
    match bytes.and_then(|b| std::str::from_utf8(b).ok()) {
        Some(s) if !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') => {
            Value::from(s.to_string())
        }
        _ => Value::from(format!("<{ty}>")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Remote servers with no SSL setting do not allow plain text (only this machine · SSH tunnels · explicit allow)
    #[test]
    fn plaintext_only_where_nothing_can_intercept() {
        let mut c: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "mysql", "host": "db.example", "port": 3306, "user": "u" })).unwrap();
        assert!(!plaintext_allowed(&c, "db.example"));
        assert!(plaintext_allowed(&c, "127.0.0.1"), "through a tunnel the client connects to 127.0.0.1");
        c.options.allow_plaintext = true;
        assert!(plaintext_allowed(&c, "db.example"));
        c.options.allow_plaintext = false;
        for h in ["localhost", "127.0.0.1", "::1", "[::1]", "0.0.0.0", "::ffff:127.0.0.1", "[::ffff:127.0.0.1]", "/var/run/postgresql", "/tmp"] {
            c.host = h.into();
            assert!(is_local(&c), "{h}");
        }
        for h in ["127.evil.example", "127.0.0.1.example", "db.example", "::ffff:10.0.0.1"] {
            c.host = h.into();
            assert!(!is_local(&c), "{h}");
        }
        // An AWS SSM tunnel is encrypted to the instance, like SSH
        c.ssm = Some(crate::store::SsmConfig { enabled: true, target: "i-0123456789abcdef0".into(), ..Default::default() });
        assert!(is_local(&c));
    }


    /// A remote PostgreSQL without SSL settings must set up TLS: the local test server has no TLS, reached through an address that isn't loopback by name
    /// ("127.1" resolves to 127.0.0.1 but isn't an IP literal, so Qurry treats it as remote; ::ffff:127.0.0.1 now counts as this machine)
    /// QURRY_TEST_PG=host:port:user:db (the server must have no TLS)
    #[tokio::test]
    #[ignore]
    async fn pg_remote_requires_tls() {
        let Some(crate::testenv::Server { mut cfg, .. }) = crate::testenv::pg() else { return };
        cfg.host = "127.1".into();
        assert!(!is_loopback(&cfg.host) && is_loopback("::ffff:127.0.0.1"));
        opt_in_then_connect(cfg, "pw").await;
    }

    /// QURRY_TEST_MY_NOTLS=host:port:user:password:db — a MySQL started without TLS (the shared one has TLS)
    #[tokio::test]
    #[ignore]
    async fn mysql_remote_requires_tls() {
        let s = crate::testenv::my_from("QURRY_TEST_MY_NOTLS").expect("QURRY_TEST_MY_NOTLS");
        opt_in_then_connect(ConnectionConfig { host: "127.1".into(), ..s.cfg }, s.password.as_deref().unwrap_or_default()).await;
    }

    /// The TLS error is one the GUI offers "Allow unencrypted" for; after AppState::allow_plaintext the saved password connects
    async fn opt_in_then_connect(cfg: ConnectionConfig, password: &str) {
        let dir = std::env::temp_dir().join(format!("qurry-tlsopt-{}", uuid::Uuid::new_v4()));
        let st = crate::app::AppState::new(crate::store::Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let id = st.save_connection(cfg, Some(password.into()), None).await.unwrap().id;
        let e = st.connect(id.clone()).await.expect_err("connected without TLS").to_string();
        assert!(qurry_shared::errors::tls_opt_in(&e), "{e}");
        let c = st.allow_plaintext(id.clone()).await.unwrap();
        assert!(c.options.allow_plaintext && c.password_saved);
        let v = st.connect(id).await.expect("allowed plain text connects with the saved password");
        assert!(v.contains("Not encrypted"), "{v}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    #[ignore]
    async fn pg_connection_options_applied() {
        let Some(crate::testenv::Server { mut cfg, .. }) = crate::testenv::pg() else { return };
        cfg.options.time_zone = Some("Asia/Seoul".into());
        cfg.options.isolation = Some("serializable".into());
        cfg.options.startup_script = "set application_name = 'dbtool_startup'; set search_path = pg_catalog".into();
        let Pool::Pg(p) = connect(&cfg, None).await.expect("connect") else { unreachable!() };
        let row: (String, String, String, String) = sqlx::query_as(
            "select current_setting('TimeZone'), current_setting('default_transaction_isolation'), current_setting('application_name'), current_setting('search_path')",
        )
        .fetch_one(&p)
        .await
        .unwrap();
        assert_eq!(row, ("Asia/Seoul".into(), "serializable".into(), "dbtool_startup".into(), "pg_catalog".into()));
    }

    #[tokio::test]
    #[ignore]
    async fn pg_read_only_rejects_writes() {
        let Some(s) = crate::testenv::pg() else { return };
        let cfg = ConnectionConfig { read_only: true, ..s.cfg };
        let Pool::Pg(p) = connect(&cfg, None).await.expect("connect") else { unreachable!() };
        let ro: String = sqlx::query_scalar("show default_transaction_read_only").fetch_one(&p).await.unwrap();
        assert_eq!(ro, "on");
        let err = sqlx::query("create temp table ro_probe(x int)").execute(&p).await;
        assert!(err.is_err(), "write must be rejected");
        assert!(format!("{}", err.unwrap_err()).contains("read-only"));
    }

    /// infinity sentinels show as text; values past chrono's range give None (shown as <TYPE>), never a panic (#84)
    #[test]
    fn pg_datetime_infinity_and_range() {
        let ts = |v: i64| v.to_be_bytes().to_vec();
        let d = |v: i32| v.to_be_bytes().to_vec();
        let utc = Zone::UTC;
        for ty in ["TIMESTAMP", "TIMESTAMPTZ"] {
            assert_eq!(pg_datetime(ty, &ts(i64::MAX), &utc).as_deref(), Some("infinity"));
            assert_eq!(pg_datetime(ty, &ts(i64::MIN), &utc).as_deref(), Some("-infinity"));
            assert_eq!(pg_datetime(ty, &ts(i64::MAX - 1), &utc), None);
        }
        assert_eq!(pg_datetime("TIMESTAMP", &ts(86_400_000_000 + 1_500_000), &utc).as_deref(), Some("2000-01-02 00:00:01.500"));
        assert_eq!(pg_datetime("DATE", &d(i32::MAX), &utc).as_deref(), Some("infinity"));
        assert_eq!(pg_datetime("DATE", &d(i32::MIN), &utc).as_deref(), Some("-infinity"));
        assert_eq!(pg_datetime("DATE", &d(-1), &utc).as_deref(), Some("1999-12-31"));
        assert_eq!(pg_datetime("DATE", &d(i32::MAX - 1), &utc), None);
        assert_eq!(pg_datetime("DATE", &[0, 1], &utc), None, "short value");
        // date[] [[infinity, NULL], [2000-01-01, -infinity]]: ndim, has_null, elem oid, (len, lbound) per dimension, elements
        let mut b = [2, 1, 1082, 2, 1, 2, 1].iter().flat_map(|v: &i32| v.to_be_bytes()).collect::<Vec<u8>>();
        for e in [Some(i32::MAX), None, Some(0), Some(i32::MIN)] {
            match e {
                Some(v) => b.extend([4i32.to_be_bytes(), v.to_be_bytes()].concat()),
                None => b.extend((-1i32).to_be_bytes()),
            }
        }
        let date = |e: &[u8]| pg_datetime("DATE", e, &utc);
        assert_eq!(pg_array(&b, &date).as_deref(), Some("{{infinity,NULL},{2000-01-01,-infinity}}"));
        assert_eq!(pg_array(&[0; 12], &date).as_deref(), Some("{}"), "empty array");
        assert_eq!(pg_array(&b[..b.len() - 2], &date), None, "cut short");
        let huge = [3, 0, 1082, i32::MAX, 1, i32::MAX, 1, i32::MAX, 1].iter().flat_map(|v: &i32| v.to_be_bytes()).collect::<Vec<u8>>();
        assert_eq!(pg_array(&huge, &date), None, "dimensions whose product overflows");
    }

    /// timestamptz is shown in the session's zone with its offset (not this computer's), timestamp as stored (#133)
    #[test]
    fn pg_timestamptz_in_the_session_zone() {
        // 2026-11-09 04:00:00.25 UTC, in microseconds from 2000-01-01
        let us = (chrono::NaiveDate::from_ymd_opt(2026, 11, 9).unwrap().and_hms_opt(4, 0, 0).unwrap() - chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap())
            .num_microseconds()
            .unwrap()
            + 250_000;
        let b = us.to_be_bytes();
        assert_eq!(pg_datetime("TIMESTAMPTZ", &b, &Zone::UTC).as_deref(), Some("2026-11-09 04:00:00.250 +00:00"));
        assert_eq!(pg_datetime("TIMESTAMPTZ", &b, &Zone::Fixed(9 * 3600)).as_deref(), Some("2026-11-09 13:00:00.250 +09:00"));
        assert_eq!(pg_datetime("TIMESTAMP", &b, &Zone::Fixed(9 * 3600)).as_deref(), Some("2026-11-09 04:00:00.250"), "timestamp has no zone");
        // Past chrono's range only once the offset is added: None (shown as <TIMESTAMPTZ>), never a panic
        let last = (chrono::NaiveDateTime::MAX - chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()).num_microseconds().unwrap();
        assert_eq!(pg_datetime("TIMESTAMPTZ", &last.to_be_bytes(), &Zone::Fixed(3600)), None);
        assert!(pg_datetime("TIMESTAMPTZ", &last.to_be_bytes(), &Zone::Fixed(-3600)).is_some());
    }

    /// The session's zone on each path: a data source without the Time zone option (sqlx starts every session in UTC), Asia/Seoul
    /// and `+09:00` through the option, Europe/Berlin across its DST change, arrays, infinity, the MCP read-only path and CSV export (#133)
    #[tokio::test]
    #[ignore]
    async fn pg_timestamptz_session_zone() {
        let Some(s) = crate::testenv::pg() else { return };
        let at = |tz: Option<&str>| {
            let mut c = s.cfg.clone();
            c.options.time_zone = tz.map(String::from);
            c
        };
        let q = "select '2026-11-09 04:00:00+00'::timestamptz, '2026-11-09 04:00:00'::timestamp";
        for (tz, want) in [(None, "2026-11-09 04:00:00 +00:00"), (Some("UTC"), "2026-11-09 04:00:00 +00:00"), (Some("Asia/Seoul"), "2026-11-09 13:00:00 +09:00"), (Some("+09:00"), "2026-11-09 13:00:00 +09:00"), (Some("-05:30"), "2026-11-08 22:30:00 -05:30")] {
            let pool = connect(&at(tz), s.password.as_deref()).await.expect("connect");
            let r = pool.run_query(q, 10).await.unwrap();
            assert_eq!(r.rows[0], vec![Value::from(want), Value::from("2026-11-09 04:00:00")], "{tz:?}");
            // The server's own text agrees on the offset (psql shows the same moment)
            let own = pool.run_query("select to_char('2026-11-09 04:00:00+00'::timestamptz, 'YYYY-MM-DD HH24:MI:SS TZH:TZM')", 1).await.unwrap();
            assert_eq!(own.rows[0][0], Value::from(want), "{tz:?}");
        }
        let pool = connect(&at(Some("Europe/Berlin")), s.password.as_deref()).await.expect("connect");
        let dst = "select '2026-10-25 00:30:00+00'::timestamptz a, '2026-10-25 01:30:00+00'::timestamptz b,                    array['2026-10-25 00:30:00+00', null, '2026-10-25 01:30:00+00', 'infinity', '-infinity']::timestamptz[] c,                    'infinity'::timestamptz d";
        let summer = "2026-10-25 02:30:00 +02:00";
        let winter = "2026-10-25 02:30:00 +01:00";
        let array = format!("{{\"{summer}\",NULL,\"{winter}\",infinity,-infinity}}");
        let want = vec![Value::from(summer), Value::from(winter), Value::from(array.as_str()), Value::from("infinity")];
        assert_eq!(pool.run_query(dst, 10).await.unwrap().rows[0], want);
        let Pool::Pg(p) = &pool else { unreachable!() };
        assert_eq!(crate::backend::Backend::run_read_only(p, dst, 10).await.unwrap().rows[0], want, "MCP's read-only path");
        let path = std::env::temp_dir().join(format!("qurry_tz_{}.csv", std::process::id()));
        pool.export_csv(dst, &path).await.unwrap();
        let csv = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(csv.lines().nth(1), Some(format!("{summer},{winter},\"{{\"\"{summer}\"\",NULL,\"\"{winter}\"\",infinity,-infinity}}\",infinity").as_str()), "{csv}");
    }

    /// `SET TIME ZONE` in a console applies to that console's later results; SET LOCAL ends with its transaction, and a value read
    /// in the statement that changes the zone already shows in the new one (#133)
    #[tokio::test]
    #[ignore]
    async fn pg_set_time_zone_in_console() {
        let Some(s) = crate::testenv::pg() else { return };
        let pool = connect(&s.cfg, s.password.as_deref()).await.expect("connect");
        let mut c = crate::session::Session::open(&pool).await.unwrap();
        let ts = "select '2026-11-09 04:00:00+00'::timestamptz";
        let mut shown = async |sql: &str| c.run(sql, 10).await.unwrap().rows.first().map(|r| r[0].clone());
        assert_eq!(shown(ts).await, Some(Value::from("2026-11-09 04:00:00 +00:00")));
        shown("set time zone 'Asia/Seoul'").await;
        assert_eq!(shown(ts).await, Some(Value::from("2026-11-09 13:00:00 +09:00")));
        // '+09:00' typed in a console is PostgreSQL's POSIX reading, 9 hours west: shown as the server shows it
        shown("SET TIME ZONE '+09:00'").await;
        assert_eq!(shown(ts).await, Some(Value::from("2026-11-08 19:00:00 -09:00")));
        shown("set timezone to 'Asia/Seoul'").await;
        shown("begin").await;
        shown("set local time zone 'Europe/Berlin'").await;
        assert_eq!(shown(ts).await, Some(Value::from("2026-11-09 05:00:00 +01:00")));
        shown("rollback").await;
        assert_eq!(shown(ts).await, Some(Value::from("2026-11-09 13:00:00 +09:00")));
        assert_eq!(shown("select '2026-11-09 04:00:00+00'::timestamptz, set_config('TimeZone', 'UTC', false)").await, Some(Value::from("2026-11-09 04:00:00 +00:00")));
        assert_eq!(shown("select array['2026-11-09 04:00:00+00']::timestamptz[]").await, Some(Value::from("{\"2026-11-09 04:00:00 +00:00\"}")));
        shown("reset time zone").await;
        assert_eq!(shown(ts).await, Some(Value::from("2026-11-09 04:00:00 +00:00")));
        c.discard().await;
    }

    /// The Time zone option is never SQL: a name is bound, also for a role with standard_conforming_strings = off (where `\'` doesn't
    /// end a string, so quoting by doubling quotes could be escaped)
    #[tokio::test]
    #[ignore]
    async fn pg_time_zone_option_is_not_sql() {
        let Some(s) = crate::testenv::pg() else { return };
        let admin = connect(&s.cfg, s.password.as_deref()).await.expect("connect");
        let pw = s.password.clone().unwrap_or_else(|| "qurry_tz_pw".into());
        for q in ["drop table if exists public.qurry_tz_pwn".to_string(), "drop role if exists qurry_tz_scs".into(), format!("create role qurry_tz_scs login password '{pw}'"), "grant create on schema public to qurry_tz_scs".into(), "alter role qurry_tz_scs set standard_conforming_strings = off".into()] {
            admin.run_query(&q, 1).await.unwrap();
        }
        let mut cfg = s.cfg.clone();
        cfg.user = "qurry_tz_scs".into();
        cfg.options.time_zone = Some(r"Asia/Seoul\'; create table public.qurry_tz_pwn(); --".into());
        let r = match connect(&cfg, Some(&pw)).await {
            Ok(p) => p.run_query("select 1", 1).await.map(|_| ()),
            Err(e) => Err(e),
        };
        assert!(r.is_err(), "not a time zone");
        cfg.options.time_zone = Some("Asia/Seoul".into());
        let p = connect(&cfg, Some(&pw)).await.unwrap();
        let row = p.run_query("select current_setting('TimeZone'), current_setting('standard_conforming_strings')", 1).await.unwrap().rows;
        assert_eq!(row[0], vec![Value::from("Asia/Seoul"), Value::from("off")]);
        p.close().await;
        let made = admin.run_query("select to_regclass('public.qurry_tz_pwn') is not null", 1).await.unwrap().rows;
        admin.run_query("revoke create on schema public from qurry_tz_scs", 1).await.unwrap();
        admin.run_query("drop role qurry_tz_scs", 1).await.unwrap();
        assert_eq!(made[0][0], Value::from(false), "the option's text ran as SQL");
    }

    /// A result cut off at the row limit doesn't wait for the rest of it to read the time zone: the pool reads it on another
    /// connection, a console before the statement (#133). Loose: compared with the same rows as numbers
    #[tokio::test]
    #[ignore]
    async fn pg_cut_off_timestamptz_returns_at_once() {
        let Some(s) = crate::testenv::pg() else { return };
        let pool = connect(&s.cfg, s.password.as_deref()).await.expect("connect");
        let rows = |e: &str| format!("select {e} from generate_series(1, 3000000) g");
        let (num, tz) = (rows("g::int8"), rows("'2026-01-01'::timestamptz + g * interval '1 second'"));
        let timed = async |r: AppResult<QueryResult>| {
            let r = r.unwrap();
            assert!(r.truncated && r.rows.len() == 500);
            r.elapsed_ms
        };
        // Each on a pool of its own, closed after: a cut-off result is read to its end when its connection goes back to the pool,
        // which would slow the next query in this test's runtime
        let once = async |q: &str| {
            let p = connect(&s.cfg, s.password.as_deref()).await.expect("connect");
            let t = Instant::now();
            timed(p.run_query(q, 500).await).await;
            let d = t.elapsed();
            p.close().await;
            d
        };
        let base = once(&num).await;
        let zoned = once(&tz).await;
        assert!(zoned < base * 2 + Duration::from_millis(100), "pool: {zoned:?} vs {base:?} without timestamptz");
        // A console's statement DECLARE can't take (a data-modifying WITH) runs without a cursor
        let mut c = crate::session::Session::open(&pool).await.unwrap();
        c.run("create temp table qurry_none (x int)", 1).await.unwrap();
        let direct = |q: &str| format!("with d as (delete from qurry_none returning x) {q}");
        let t = Instant::now();
        assert!(c.run(&direct(&num), 500).await.unwrap().truncated);
        let base = t.elapsed();
        c.run("select 1", 1).await.unwrap();
        let t = Instant::now();
        let r = c.run(&direct(&tz), 500).await.unwrap();
        let zoned = t.elapsed();
        assert!(r.truncated && r.rows[0][0] == "2026-01-01 00:00:01 +00:00", "{:?}", r.rows[0]);
        assert!(zoned < base * 2 + Duration::from_millis(100), "console: {zoned:?} vs {base:?} without timestamptz");
        c.discard().await;
    }

    /// PostgreSQL's infinity / -infinity in timestamp, timestamptz and date (also in arrays, next to NULLs) show as text; sqlx's chrono decode panicked on them (#84)
    #[tokio::test]
    #[ignore]
    async fn pg_infinity_dates() {
        let Some(s) = crate::testenv::pg() else { return };
        let pool = connect(&s.cfg, s.password.as_deref()).await.expect("connect");
        let r = pool
            .run_query(
                "select 'infinity'::timestamp, '-infinity'::timestamp, 'infinity'::timestamptz, '-infinity'::timestamptz, \
                 'infinity'::date, '-infinity'::date, null::timestamptz, '2024-01-02'::date",
                10,
            )
            .await
            .unwrap();
        let text = |v: &[&str]| v.iter().map(|s| Value::from(*s)).collect::<Vec<_>>();
        assert_eq!(r.rows[0][..6], text(&["infinity", "-infinity", "infinity", "-infinity", "infinity", "-infinity"]));
        assert_eq!(r.rows[0][6], Value::Null);
        assert_eq!(r.rows[0][7], Value::from("2024-01-02"));
        let a = pool
            .run_query(
                "select array['infinity', null, '-infinity', '2024-01-02 03:04:05']::timestamp[], array['-infinity', null]::timestamptz[], \
                 array['infinity', '2024-01-02', null]::date[], null::date[]",
                10,
            )
            .await
            .unwrap();
        assert_eq!(a.rows[0][..3], text(&["{infinity,NULL,-infinity,\"2024-01-02 03:04:05\"}", "{-infinity,NULL}", "{infinity,2024-01-02,NULL}"]));
        assert_eq!(a.rows[0][3], Value::Null);
    }

    #[tokio::test]
    #[ignore]
    async fn pg_end_to_end() {
        let Some(s) = crate::testenv::pg() else { return };
        let pool = connect(&s.cfg, s.password.as_deref()).await.expect("connect");
        assert!(
            pool.server_version()
                .await
                .unwrap()
                .starts_with("PostgreSQL")
        );

        let r = pool
            .run_query(
                "select 1 as a, 'x'::text b, now() c, null::int d, 12345678901234567::int8 e, \
                 12345678901234567890.12345::numeric f, '{\"k\":1}'::jsonb g, md5('x')::uuid h, \
                 '\\xdeadbeef'::bytea i, array[1,2] j",
                500,
            )
            .await
            .unwrap();
        println!("{:?}", r.rows[0]);
        assert_eq!(r.columns.len(), 10);
        assert_eq!(r.rows[0][0], Value::from(1));
        assert_eq!(r.rows[0][3], Value::Null);
        assert_eq!(r.rows[0][4], Value::from("12345678901234567"));
        assert_eq!(r.rows[0][5], Value::from("12345678901234567890.12345"));
        assert_eq!(r.rows[0][6], Value::from("{\"k\":1}"));
        assert_eq!(r.rows[0][8], Value::from("0xDEADBEEF"));

        let n = pool
            .run_query("select 0::numeric, -1.50::numeric(10,2), 0.0001::numeric, 10000::numeric, 'NaN'::numeric, 123.4567::numeric(8,3)", 10)
            .await
            .unwrap();
        assert_eq!(
            n.rows[0],
            vec![
                Value::from("0"),
                Value::from("-1.50"),
                Value::from("0.0001"),
                Value::from("10000"),
                Value::from("NaN"),
                Value::from("123.457")
            ]
        );

        let big = pool
            .run_query(
                "select g, md5(g::text) from generate_series(1, 100000) g",
                500,
            )
            .await
            .unwrap();
        assert_eq!(big.rows.len(), 500);
        assert!(big.truncated);
        println!("100k rows, first 500 fetched in {} ms", big.elapsed_ms);

        let empty = pool
            .run_query("select relname, relkind from pg_class where false", 500)
            .await
            .unwrap();
        assert!(empty.has_result_set);
        assert_eq!(empty.columns.len(), 2);

        let ddl = pool
            .run_query("create temp table if not exists t_dbtool(x int)", 500)
            .await
            .unwrap();
        assert!(!ddl.has_result_set);

        let path = std::env::temp_dir().join("dbtool_test.csv");
        let n = pool
            .export_csv("select g as id, '한글, \"따옴표\"' as txt, null::int as n from generate_series(1, 1000) g", &path)
            .await
            .unwrap();
        assert_eq!(n, 1000);
        let csv = std::fs::read_to_string(&path).unwrap();
        let mut lines = csv.lines();
        assert_eq!(lines.next().unwrap(), "\u{FEFF}id,txt,n");
        assert_eq!(lines.next().unwrap(), "1,\"한글, \"\"따옴표\"\"\",");

        let err = pool.run_query("select * from no_such_table_xyz", 500).await;
        assert!(err.is_err());
        println!("error text: {}", err.unwrap_err());
    }
}
