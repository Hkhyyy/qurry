//! DB connections and query execution. Only PostgreSQL is supported for now.
use crate::error::AppResult;
use crate::store::{ConnectionConfig, DbKind};
use futures::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgRow};
use sqlx::{Column, Either, Executor, Row, TypeInfo, ValueRef};
use std::time::{Duration, Instant};

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
}

pub(crate) fn query_result(columns: Vec<ColumnInfo>, rows: Vec<Vec<Value>>, rows_affected: u64, truncated: bool, start: Instant, has_result_set: bool) -> QueryResult {
    QueryResult { columns, rows, rows_affected, truncated, elapsed_ms: start.elapsed().as_millis() as u64, has_result_set, in_transaction: false }
}

/// Max connections per DB (number of tabs + metadata queries such as the tree and autocomplete)
pub const POOL_MAX: u32 = 32;

/// Whether it is a server on this machine or goes through an SSH tunnel (no network in between to intercept)
pub fn is_local(cfg: &ConnectionConfig) -> bool {
    cfg.active_ssh().is_some() || is_loopback(&cfg.host)
}

pub fn is_loopback(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']').to_lowercase();
    // A real loopback address (127.0.0.0/8, ::1) — not a name that merely starts with 127.
    h == "localhost" || h.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
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

/// Message to show when PostgreSQL connected without TLS (with no SSL setting, it falls back to plain text if the server lacks TLS. Except this machine · tunnels)
pub async fn pg_plaintext_note(p: &sqlx::PgPool, cfg: &ConnectionConfig) -> Option<String> {
    if cfg.active_ssl().is_some() || is_local(cfg) {
        return None;
    }
    let ssl: Option<bool> = sqlx::query_scalar("select ssl from pg_stat_ssl where pid = pg_backend_pid()").fetch_optional(p).await.ok().flatten();
    (ssl == Some(false)).then(|| "⚠ Not encrypted: the server didn't offer TLS, so the password and data travel in plain text. Enable SSL in the data source to require it.".to_string())
}
/// Close idle connections after 1 minute (so connections opened in bulk while reading the tree or search index do not linger on production servers). Connections held by tabs are not idle and stay
pub const POOL_IDLE: Duration = Duration::from_secs(60);

pub async fn connect(cfg: &ConnectionConfig, password: Option<&str>) -> AppResult<Pool> {
    connect_via(cfg, password, None).await
}

/// If `tunnel_port` is set, connect to the SSH tunnel's local port.
pub async fn connect_via(cfg: &ConnectionConfig, password: Option<&str>, tunnel_port: Option<u16>) -> AppResult<Pool> {
    match cfg.kind {
        DbKind::Sqlite | DbKind::Mssql | DbKind::Clickhouse | DbKind::Duckdb | DbKind::Mongodb | DbKind::Oracle | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus => Ok(Pool::Plugin(crate::plugin::connect(cfg, password, tunnel_port).await?)),
        DbKind::Postgres => {
            let (host, port) = match tunnel_port {
                Some(p) => ("127.0.0.1", p),
                None => (cfg.host.as_str(), cfg.port),
            };
            let mut opts = PgConnectOptions::new()
                .host(host)
                .port(port)
                .username(&cfg.user)
                .application_name("qurry");
            // Like DataGrip, an empty value connects to the postgres DB (the PG default, a DB named after the user, often does not exist)
            let database = if cfg.database.trim().is_empty() { "postgres" } else { cfg.database.trim() };
            opts = opts.database(database);
            if let Some(p) = password.filter(|p| !p.is_empty()) {
                opts = opts.password(p);
            }
            if cfg.read_only {
                // Do not leave the parsing of \ in strings to server settings (match the rules guard reads with)
                opts = opts.options([("default_transaction_read_only", "on"), ("standard_conforming_strings", "on")]);
            }
            // Without SSL settings a remote server must still set up TLS (certificate not checked); plain text only for this computer,
            // an SSH tunnel, or when the data source allows it (Prefer would quietly fall back to plain text if TLS were cut)
            if cfg.active_ssl().is_none() && !plaintext_allowed(cfg, host) {
                opts = opts.ssl_mode(sqlx::postgres::PgSslMode::Require);
            }
            // SSL from the SSH/SSL tab
            if let Some(ssl) = cfg.active_ssl() {
                use sqlx::postgres::PgSslMode;
                opts = opts.ssl_mode(match ssl.mode() {
                    "verify-full" => PgSslMode::VerifyFull,
                    "verify-ca" => PgSslMode::VerifyCa,
                    "require" => PgSslMode::Require,
                    // Do not pass unknown values on as unverified require
                    m => return Err(crate::error::AppError::Msg(format!("Unknown SSL mode '{m}' (use require, verify-ca or verify-full)"))),
                });
                let home = |p: &str| p.strip_prefix("~/").map(|r| format!("{}/{r}", std::env::var("HOME").unwrap_or_default())).unwrap_or_else(|| p.to_string());
                if !ssl.ca_file.trim().is_empty() {
                    opts = opts.ssl_root_cert(home(ssl.ca_file.trim()));
                }
                if !ssl.cert_file.trim().is_empty() {
                    opts = opts.ssl_client_cert(home(ssl.cert_file.trim()));
                }
                if !ssl.key_file.trim().is_empty() {
                    opts = opts.ssl_client_key(home(ssl.key_file.trim()));
                }
            }
            const LEVELS: [&str; 4] = ["read uncommitted", "read committed", "repeatable read", "serializable"];
            if let Some(iso) = cfg.options.isolation.as_deref().filter(|i| LEVELS.contains(i)) {
                opts = opts.options([("default_transaction_isolation", iso)]);
            }
            // sqlx sends TimeZone=UTC as a startup parameter, so change it with SET right after connecting
            let tz = cfg.options.time_zone.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(|t| t.replace('\'', "''"));
            let script = cfg.options.startup_script.trim().to_string();
            let pool = PgPoolOptions::new()
                // Each console/table tab uses one dedicated connection. Connections open only when needed, so just keep the cap generous
                .max_connections(crate::db::POOL_MAX)
                .idle_timeout(crate::db::POOL_IDLE)
                .acquire_timeout(Duration::from_secs(10))
                .after_connect(move |conn, _meta| {
                    let script = script.clone();
                    let tz = tz.clone();
                    Box::pin(async move {
                        use sqlx::Executor;
                        if let Some(tz) = tz {
                            conn.execute(format!("set time zone '{tz}'").as_str()).await?;
                        }
                        if !script.is_empty() {
                            conn.execute(sqlx::raw_sql(&script)).await?;
                        }
                        Ok(())
                    })
                })
                .connect_with(opts)
                .await
                .map_err(|e| match &e {
                    // Required TLS that the server can't set up: say what to do instead of a bare TLS error
                    sqlx::Error::Tls(_) if cfg.active_ssl().is_none() => crate::error::AppError::Msg(format!("{}\n({e})", crate::mysql::NEEDS_TLS.replace(" without TLS 1.2", ""))),
                    _ => e.into(),
                })?;
            Ok(Pool::Pg(pool))
        }
        DbKind::Mysql => {
            let (host, port) = match tunnel_port {
                Some(p) => ("127.0.0.1", p),
                None => (cfg.host.as_str(), cfg.port),
            };
            Ok(Pool::My(crate::mysql::connect(cfg, password, host, port).await?))
        }
        DbKind::Redis => {
            let (host, port) = match tunnel_port {
                Some(p) => ("127.0.0.1", p),
                None => (cfg.host.as_str(), cfg.port),
            };
            let m = crate::redisdb::connect(cfg, password, host, port).await?;
            Ok(Pool::Redis(m, crate::redisdb::db_index(&cfg.database)))
        }
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
        match self {
            Pool::Pg(p) => Ok(sqlx::query_scalar::<_, String>("select version()").fetch_one(p).await?),
            Pool::My(p) => crate::mysql::server_version(p).await,
            Pool::Redis(m, _) => crate::redisdb::server_version(m).await,
            Pool::Plugin(c) => c.call("version", Value::Null).await,
        }
    }

    pub async fn close(&self) {
        match self {
            Pool::Pg(p) => p.close().await,
            Pool::My(p) => p.close().await,
            Pool::Redis(..) | Pool::Plugin(_) => {}
        }
    }

    /// Rerun the query and export the full result (out decides the format and target)
    pub async fn export(&self, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
        match self {
            Pool::Pg(p) => export_pg(p, sql, out).await,
            Pool::My(p) => crate::mysql::export(p, sql, out).await,
            Pool::Redis(m, _) => crate::redisdb::export(m, sql, out).await,
            Pool::Plugin(c) => {
                let r = c.query(sql, 10_000_000).await?;
                out.header(&r.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()).await?;
                for row in &r.rows {
                    out.row(row).await?;
                }
                Ok(())
            }
        }
    }

    /// To a CSV file (with BOM). Row count
    pub async fn export_csv(&self, sql: &str, path: &std::path::Path) -> AppResult<u64> {
        let mut out = crate::transfer::Exporter::csv_file(path).await?;
        self.export(sql, &mut out).await?;
        Ok(out.finish().await?.rows)
    }

    /// Run one query and return up to `limit` rows.
    pub async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        match self {
            Pool::Pg(p) => run_pg(p, sql, limit).await,
            Pool::My(p) => crate::mysql::run(p, sql, limit).await,
            Pool::Redis(m, _) => crate::redisdb::run(m, sql, limit).await,
            Pool::Plugin(c) => c.query(sql, limit).await,
        }
    }

    /// keep-alive
    /// keep-alive. Cut off at 5 seconds so one unresponsive server does not stall the cleanup routine
    pub async fn ping(&self) {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.ping_inner()).await;
    }

    async fn ping_inner(&self) {
        match self {
            Pool::Pg(p) => {
                let _ = sqlx::query("select 1").execute(p).await;
            }
            Pool::My(p) => {
                let _ = sqlx::query("select 1").execute(p).await;
            }
            Pool::Redis(m, _) => crate::redisdb::ping(m).await,
            Pool::Plugin(_) => {}
        }
    }
}

async fn run_pg(pool: &PgPool, sql: &str, limit: usize) -> AppResult<QueryResult> {
    let start = Instant::now();
    let mut columns = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut rows_affected = 0u64;
    let mut truncated = false;
    let mut has_result_set = false;
    {
        #[allow(deprecated)]
        let mut stream = sqlx::query(sql).persistent(false).fetch_many(pool);
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
                    rows.push((0..row.len()).map(|i| pg_value(&row, i)).collect());
                }
            }
        }
    }
    // With 0 rows the column info is unknown, so fill it in with describe.
    if !has_result_set {
        if let Ok(desc) = pool.describe(sql).await {
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
    })
}

/// Run a SELECT on an already acquired connection (transaction) into a QueryResult (for Full-text Search)
pub async fn run_on(conn: &mut sqlx::PgConnection, sql: &str, limit: usize) -> AppResult<QueryResult> {
    let start = Instant::now();
    let mut columns = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
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
            rows.push((0..row.len()).map(|i| pg_value(&row, i)).collect());
        }
    }
    Ok(QueryResult { columns, rows, rows_affected: 0, truncated, elapsed_ms: start.elapsed().as_millis() as u64, has_result_set: true, in_transaction: false })
}

async fn export_pg(pool: &PgPool, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
    let mut conn = pool.acquire().await?;
    export_pg_on(&mut conn, sql, out).await
}

/// Export on an already acquired connection (possibly a read-only transaction)
pub async fn export_pg_on(conn: &mut sqlx::PgConnection, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
    let mut stream = sqlx::query(sql).persistent(false).fetch(&mut *conn);
    while let Some(row) = stream.try_next().await? {
        if !out.has_header() {
            let names: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
            out.header(&names).await?;
        }
        let vals: Vec<Value> = (0..row.len()).map(|i| pg_value(&row, i)).collect();
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

pub(crate) fn pg_value(row: &PgRow, i: usize) -> Value {
    use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
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
        "TIMESTAMP" => {
            get!(NaiveDateTime).map(|t| Value::from(t.format("%Y-%m-%d %H:%M:%S%.f").to_string()))
        }
        "TIMESTAMPTZ" => get!(DateTime<Utc>).map(|t| {
            Value::from(
                t.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S%.f %:z")
                    .to_string(),
            )
        }),
        "DATE" => get!(NaiveDate).map(|d| Value::from(d.to_string())),
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
        for h in ["localhost", "127.0.0.1", "::1", "[::1]"] {
            c.host = h.into();
            assert!(is_local(&c), "{h}");
        }
        for h in ["127.evil.example", "127.0.0.1.example", "db.example"] {
            c.host = h.into();
            assert!(!is_local(&c), "{h}");
        }
    }

    /// Tests against a local PostgreSQL. Run with `cargo test -- --ignored`.
    fn local_cfg() -> ConnectionConfig {
        ConnectionConfig {
            id: "test".into(),
            name: "local".into(),
            kind: DbKind::Postgres,
            host: std::env::var("QURRY_TEST_PG_HOST").unwrap_or_else(|_| "localhost".into()),
            port: 5432,
            user: std::env::var("QURRY_TEST_PG_USER")
                .unwrap_or_else(|_| std::env::var("USER").unwrap()),
            database: std::env::var("QURRY_TEST_PG_DB").unwrap_or_else(|_| "postgres".into()),
            color: None,
            ssh: None,
            password_saved: false,
            ssh_secret_saved: false,
            scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        }
    }

    /// A remote PostgreSQL without SSL settings must set up TLS: the local test server has no TLS, reached through an address that isn't loopback by name
    #[tokio::test]
    #[ignore]
    async fn pg_remote_requires_tls() {
        let mut cfg = local_cfg();
        cfg.host = "::ffff:127.0.0.1".into();
        let r = connect(&cfg, None).await;
        let Err(e) = r else { panic!("connected without TLS") };
        let e = e.to_string();
        assert!(e.contains("didn't set up an encrypted") || e.contains("TLS") || e.contains("SSL"), "{e}");
        cfg.options.allow_plaintext = true;
        assert!(connect(&cfg, None).await.is_ok(), "allowed plain text connects");
    }

    #[tokio::test]
    #[ignore]
    async fn pg_connection_options_applied() {
        let mut cfg = local_cfg();
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
        let cfg = ConnectionConfig { read_only: true, ..local_cfg() };
        let Pool::Pg(p) = connect(&cfg, None).await.expect("connect") else { unreachable!() };
        let ro: String = sqlx::query_scalar("show default_transaction_read_only").fetch_one(&p).await.unwrap();
        assert_eq!(ro, "on");
        let err = sqlx::query("create temp table ro_probe(x int)").execute(&p).await;
        assert!(err.is_err(), "write must be rejected");
        assert!(format!("{}", err.unwrap_err()).contains("read-only"));
    }

    #[tokio::test]
    #[ignore]
    async fn pg_end_to_end() {
        let pool = connect(&local_cfg(), None).await.expect("connect");
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
