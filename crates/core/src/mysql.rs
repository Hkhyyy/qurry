//! MySQL / MariaDB 드라이버. PostgreSQL 쪽과 같은 모양(QueryResult, 메타데이터, 세션)을 돌려준다.
//! - 인자 없는 쿼리는 텍스트 프로토콜(COM_QUERY)로 가므로 값은 텍스트 바이트로 받는다
//! - MySQL 에는 스키마 계층이 없다: database == schema
use crate::db::{int_value, query_result, ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use crate::meta::{
    ColumnMeta, CompletionTable, ConstraintMeta, FtsHit, FtsRequest, IndexEntry, IndexMeta, RelationInfo, RoutineInfo, SchemaInfo,
    SchemaObjects, TableDetail,
};
use crate::session::{is_select_like, strip_comments_lead};
use crate::store::ConnectionConfig;
use futures::TryStreamExt;
use serde_json::Value;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{Column, Either, Executor, MySql, Row, TypeInfo, ValueRef};
use std::time::{Duration, Instant};

pub const SYSTEM_DBS: [&str; 4] = ["information_schema", "performance_schema", "mysql", "sys"];

pub fn ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}
/// MySQL 문자열 리터럴: 백슬래시도 이스케이프 문자다
pub fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

pub async fn connect(cfg: &ConnectionConfig, password: Option<&str>, host: &str, port: u16) -> AppResult<MySqlPool> {
    let mut opts = MySqlConnectOptions::new().host(host).port(port).username(&cfg.user).charset("utf8mb4");
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        opts = opts.password(p);
    }
    if !cfg.database.trim().is_empty() {
        opts = opts.database(cfg.database.trim());
    }
    if let Some(ssl) = cfg.active_ssl() {
        use sqlx::mysql::MySqlSslMode;
        opts = opts.ssl_mode(match ssl.mode() {
            "verify-full" => MySqlSslMode::VerifyIdentity,
            "verify-ca" => MySqlSslMode::VerifyCa,
            "require" => MySqlSslMode::Required,
            m => return Err(AppError::Msg(format!("Unknown SSL mode '{m}' (use require, verify-ca or verify-full)"))),
        });
        let home = |p: &str| p.strip_prefix("~/").map(|r| format!("{}/{r}", std::env::var("HOME").unwrap_or_default())).unwrap_or_else(|| p.to_string());
        if !ssl.ca_file.trim().is_empty() {
            opts = opts.ssl_ca(home(ssl.ca_file.trim()));
        }
        if !ssl.cert_file.trim().is_empty() {
            opts = opts.ssl_client_cert(home(ssl.cert_file.trim()));
        }
        if !ssl.key_file.trim().is_empty() {
            opts = opts.ssl_client_key(home(ssl.key_file.trim()));
        }
    }
    // 접속 직후 실행할 세션 설정 (Options 탭)
    let mut init: Vec<String> = Vec::new();
    const LEVELS: [&str; 4] = ["read uncommitted", "read committed", "repeatable read", "serializable"];
    if let Some(iso) = cfg.options.isolation.as_deref().filter(|i| LEVELS.contains(i)) {
        init.push(format!("SET SESSION TRANSACTION ISOLATION LEVEL {}", iso.to_uppercase()));
    }
    if cfg.read_only {
        init.push("SET SESSION TRANSACTION READ ONLY".into());
    }
    if let Some(tz) = cfg.options.time_zone.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        init.push(format!("SET time_zone = {}", lit(tz)));
    }
    let script = cfg.options.startup_script.trim().to_string();
    let pool_opts = || {
        let (init, script) = (init.clone(), script.clone());
        MySqlPoolOptions::new()
        // 콘솔·테이블 탭마다 연결을 하나씩 전용으로 쓴다. 연결은 필요할 때만 열리므로 상한만 넉넉히
        .max_connections(crate::db::POOL_MAX)
        .idle_timeout(crate::db::POOL_IDLE)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(move |conn, _| {
            let init = init.clone();
            let script = script.clone();
            Box::pin(async move {
                for s in &init {
                    conn.execute(s.as_str()).await?;
                }
                if !script.is_empty() {
                    conn.execute(sqlx::raw_sql(&script)).await?;
                }
                Ok(())
            })
        })
    };
    let explicit_ssl = cfg.active_ssl().is_some();
    let pool = match pool_opts().connect_with(opts.clone()).await {
        // SSL 을 정하지 않으면 TLS 를 먼저 해 본다 (Preferred). 예전 서버(MySQL 5.7 등)는 TLS 1.2 를 못 맞춰 핸드셰이크에서 끊긴다 → 평문으로 다시
        Err(e) if !explicit_ssl && is_tls_failure(&e) => pool_opts().connect_with(opts.ssl_mode(sqlx::mysql::MySqlSslMode::Disabled)).await?,
        r => r?,
    };
    Ok(pool)
}

/// SSL 을 정하지 않은 소스가 평문으로 붙었으면 알릴 말 (TLS 가 안 돼 평문으로 다시 접속했을 때 — 누가 TLS 를 일부러 끊어도 이렇게 된다)
pub async fn plaintext_note(p: &MySqlPool, cfg: &ConnectionConfig) -> Option<String> {
    if cfg.active_ssl().is_some() {
        return None;
    }
    let row: Option<(String, String)> = sqlx::query_as("SHOW SESSION STATUS LIKE 'Ssl_cipher'").fetch_optional(p).await.ok()?;
    row.filter(|(_, c)| c.is_empty()).map(|_| "⚠ Not encrypted: TLS didn't work with this server, so Qurry connected without it (the password and data travel in plain text). Enable SSL in the data source to require it.".to_string())
}

/// TLS 를 맞추지 못한 접속 실패 (인증 실패 등은 아니다)
fn is_tls_failure(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Tls(_) => true,
        sqlx::Error::Io(io) => io.to_string().to_lowercase().contains("handshake") || io.to_string().contains("AlertReceived"),
        _ => false,
    }
}

pub async fn server_version(p: &MySqlPool) -> AppResult<String> {
    let v: String = sqlx::query_scalar("select cast(version() as char)").fetch_one(p).await?;
    Ok(if v.to_lowercase().contains("mariadb") { format!("MariaDB {v}") } else { format!("MySQL {v}") })
}

pub(crate) fn columns_of(row: &MySqlRow) -> Vec<ColumnInfo> {
    row.columns().iter().map(|c| ColumnInfo { name: c.name().to_string(), type_name: c.type_info().name().to_string() }).collect()
}

/// 텍스트 프로토콜 값 → JSON
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
            // BIT(n) 는 텍스트 프로토콜에서도 바이너리로 온다
            let n = bytes.iter().fold(0u64, |a, b| (a << 8) | *b as u64);
            Value::from(n)
        }
        _ => Value::from(s.as_ref()),
    }
}

/// 풀에서 쿼리 하나 실행
pub async fn run(p: &MySqlPool, sql: &str, limit: usize) -> AppResult<QueryResult> {
    let mut conn = p.acquire().await?;
    run_on(&mut conn, sql, limit).await
}

pub async fn run_on(conn: &mut sqlx::MySqlConnection, sql: &str, limit: usize) -> AppResult<QueryResult> {
    run_on_with(conn, sql, limit, false).await
}

/// 읽기 전용 경로: prepared 프로토콜로 실행 (서버가 한 문장만 받는다 → `;` 로 문장을 덧붙이는 우회가 통하지 않는다).
/// prepared 로 안 되는 문장(일부 SHOW 등, 오류 1295)만 텍스트 프로토콜로 — 이때도 guard 검사는 이미 지났다
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
                        continue; // 다음 결과/OK 패킷까지 소비해야 연결을 재사용할 수 있다
                    }
                    rows.push((0..row.len()).map(|i| value(&row, i)).collect());
                }
            }
        }
    }
    // 0행 SELECT 의 컬럼 정보
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

pub async fn export(p: &MySqlPool, sql: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
    let mut conn = p.acquire().await?;
    export_on(&mut conn, sql, out, false).await
}

/// 이미 잡은 연결에서 내보낸다. prepared = 한 문장만 실행되는 prepared 프로토콜 (읽기 전용 경로)
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

pub async fn execute_batch(p: &MySqlPool, statements: &[String], exact_one: bool) -> AppResult<u64> {
    let mut tx = p.begin().await?;
    let mut affected = 0;
    for st in statements {
        let n = tx.execute(sqlx::raw_sql(st)).await.map_err(|e| AppError::Msg(format!("{e}\n→ {st}")))?.rows_affected();
        crate::session::check_exact_one(st, n, exact_one)?;
        affected += n;
    }
    tx.commit().await?;
    Ok(affected)
}

pub async fn cancel(p: &MySqlPool, id: i64) -> AppResult<bool> {
    sqlx::query(&format!("KILL QUERY {id}")).execute(p).await?;
    Ok(true)
}

// ───────── 콘솔 세션 ─────────

/// 콘솔 탭 전용 연결. MySQL 에는 프로토콜 수준 커서가 없으므로 "더 불러오기"는 LIMIT/OFFSET 으로 다시 조회한다.
pub struct MySession {
    conn: PoolConnection<MySql>,
    pub id: i64,
    manual: bool,
    pub in_tx: bool,
    /// (SELECT 문, 다음 OFFSET)
    more: Option<(String, usize)>,
}

impl MySession {
    pub async fn open(p: &MySqlPool) -> AppResult<Self> {
        let mut conn = p.acquire().await?;
        let id: u64 = sqlx::query_scalar("select connection_id()").fetch_one(&mut *conn).await?;
        Ok(Self { conn, id: id as i64, manual: false, in_tx: false, more: None })
    }

    pub async fn ping(&mut self) {
        let _ = self.conn.execute("select 1").await;
    }

    pub fn close_cursor(&mut self) {
        self.more = None;
    }

    /// 탭을 닫을 때: 열린 트랜잭션을 되돌리고 autocommit 을 원래대로 (풀의 다른 사용자가 물려받지 않게)
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

    /// 커밋 안 한 변경이 있으면 Auto 로 바꾸지 않는다 (먼저 Commit 또는 Roll back)
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
        self.conn.execute("COMMIT").await?;
        self.in_tx = false;
        Ok(())
    }

    pub async fn rollback_tx(&mut self) -> AppResult<()> {
        self.conn.execute("ROLLBACK").await?;
        self.in_tx = false;
        Ok(())
    }

    pub async fn run(&mut self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        self.more = None;
        let mut r = run_on(&mut self.conn, sql, limit).await?;
        let s = strip_comments_lead(sql);
        if s.starts_with("begin") || s.starts_with("start transaction") {
            self.in_tx = true;
        } else if s.starts_with("commit") || (s.starts_with("rollback") && !s.starts_with("rollback to")) {
            self.in_tx = false;
        } else if self.manual && !is_select_like(sql) && !s.starts_with("show") && !s.starts_with("set") && !s.starts_with("use") {
            // autocommit=0 에서는 첫 변경 문장이 트랜잭션을 연다
            self.in_tx = true;
        }
        if r.truncated && is_select_like(sql) {
            self.more = Some((sql.trim().trim_end_matches(';').trim().to_string(), limit));
        } else {
            r.truncated = false;
        }
        r.in_transaction = self.in_tx;
        Ok(r)
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

// ───────── 메타데이터 (information_schema) ─────────
// MySQL 8 의 information_schema 컬럼은 바이너리 문자열로 오는 경우가 있어 전부 CAST(... AS CHAR) 한다

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
        "select cast(index_name as char), cast(non_unique as signed), cast(group_concat(column_name order by seq_in_index separator ', ') as char) \
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
            IndexMeta {
                definition: format!("{} {} ({cols})", if primary { "PRIMARY KEY" } else if non_unique == 0 { "UNIQUE INDEX" } else { "INDEX" }, ident(&name)),
                is_unique: non_unique == 0,
                is_primary: primary,
                name,
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
        match run_on(&mut conn, &sql, r.limit.max(1)).await {
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
    use crate::db::Pool;
    use crate::store::DbKind;

    /// docker run -d --name dbtool-mysql-test -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop -p 13306:3306 mysql:8.4
    fn cfg() -> ConnectionConfig {
        ConnectionConfig {
            id: "my".into(), name: "my".into(), kind: DbKind::Mysql, host: "127.0.0.1".into(), port: 13306, user: "root".into(),
            database: "shop".into(), color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(),
            read_only: false, options: Default::default(), ssl: None, redis: None,
        }
    }

    /// SSL 을 정하지 않은 소스가 평문으로 붙었으면 알린다 (TLS 로 붙었으면 말하지 않는다)
    #[tokio::test]
    #[ignore]
    async fn mysql_plaintext_is_reported() {
        let Pool::My(tls) = crate::db::connect(&cfg(), Some("dbtool")).await.expect("connect") else { unreachable!() };
        assert_eq!(plaintext_note(&tls, &cfg()).await, None, "the test server speaks TLS");
        let opts = sqlx::mysql::MySqlConnectOptions::new().host("127.0.0.1").port(13306).username("root").password("dbtool").ssl_mode(sqlx::mysql::MySqlSslMode::Disabled);
        let plain = sqlx::mysql::MySqlPoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
        assert!(plaintext_note(&plain, &cfg()).await.is_some_and(|n| n.contains("Not encrypted")));
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_end_to_end() {
        let pool = crate::db::connect(&cfg(), Some("dbtool")).await.expect("connect");
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
        // 값 변환
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
        // 0행 SELECT 도 컬럼이 나온다
        let z = pool.run_query("select id, customer from orders where 1 = 0", 10).await.unwrap();
        assert!(z.has_result_set && z.columns.len() == 2 && z.rows.is_empty());
        // DML
        let u = pool.run_query("update orders set memo = 'x' where customer = 'bob'", 10).await.unwrap();
        assert_eq!(u.rows_affected, 1);
        // 메타데이터
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
        // 세션: 더 불러오기, Manual Tx, 취소 id
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
        // 다른 연결에서는 아직 안 보인다
        let other = pool.run_query("select count(*) from orders where customer = 'dave'", 1).await.unwrap();
        assert_eq!(other.rows[0][0], 0);
        s.rollback_tx().await.unwrap();
        let after = s.run("select count(*) from orders where customer = 'dave'", 1).await.unwrap();
        assert_eq!(after.rows[0][0], 0);
        assert!(!after.in_transaction);
        s.set_manual(false).await.unwrap();
        // 편집 배치
        crate::mysql::execute_batch(p, &["update orders set memo = 'y' where id = 1".into()], true).await.unwrap();
        // Read-only
        let mut ro = cfg();
        ro.read_only = true;
        ro.options.time_zone = Some("+09:00".into());
        let rp = crate::db::connect(&ro, Some("dbtool")).await.unwrap();
        let tz = rp.run_query("select @@session.time_zone", 1).await.unwrap();
        assert_eq!(tz.rows[0][0], "+09:00");
        let e = rp.run_query("update orders set memo = 'z' where id = 1", 1).await;
        assert!(e.is_err() && format!("{}", e.unwrap_err()).to_lowercase().contains("read only"));
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_ssl() {
        let mut c = cfg();
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "require".into(), ..Default::default() });
        let pool = crate::db::connect(&c, Some("dbtool")).await.expect("ssl require");
        let r = pool.run_query("show session status like 'Ssl_version'", 5).await.unwrap();
        assert!(r.rows[0][1].as_str().unwrap().starts_with("TLS"), "{:?}", r.rows);
        // 자체 서명 인증서라 검증 모드는 실패해야 한다
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "verify-full".into(), ..Default::default() });
        assert!(crate::db::connect(&c, Some("dbtool")).await.is_err());
    }
}
