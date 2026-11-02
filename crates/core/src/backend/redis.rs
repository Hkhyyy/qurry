//! Redis: one schema per DB (named db<n>), keys as objects. The queries live in crate::redisdb
use super::{Backend, Snapshots};
use crate::db::QueryResult;
use crate::erd::Erd;
use crate::error::{AppError, AppResult};
use crate::fake::TableShape;
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, FtsHit, FtsRequest, IndexEntry, SchemaInfo, SchemaObjects, TableDetail};
use crate::redisdb::RedisConn;
use crate::session::Session;
use crate::transfer::Exporter;

/// The connection and the DB index the pool points to
pub(crate) struct Redis<'a> {
    pub conn: &'a RedisConn,
    pub db: i64,
}

impl Backend for Redis<'_> {
    fn label(&self) -> String {
        "Redis".into()
    }

    // ───── Metadata: meta.rs · hints.rs · erd.rs · ddl.rs · fake.rs ─────

    async fn list_databases(&self) -> AppResult<Vec<String>> {
        crate::redisdb::list_databases(self.conn, self.db).await
    }

    async fn list_schemas(&self) -> AppResult<Vec<SchemaInfo>> {
        Ok(crate::redisdb::list_schemas(&format!("db{}", self.db)))
    }

    async fn list_objects(&self, _schema: &str) -> AppResult<SchemaObjects> {
        crate::redisdb::list_objects(self.conn).await
    }

    /// Keys have no columns
    async fn table_detail(&self, _schema: &str, _table: &str) -> AppResult<TableDetail> {
        Ok(TableDetail { columns: vec![], indexes: vec![], constraints: vec![] })
    }

    async fn object_ddl(&self, _schema: &str, name: &str, _kind: &str) -> AppResult<String> {
        crate::redisdb::key_ddl(self.conn, name).await
    }

    async fn completion_meta(&self) -> AppResult<Vec<CompletionTable>> {
        Ok(vec![])
    }

    async fn object_index(&self) -> AppResult<Vec<IndexEntry>> {
        crate::redisdb::object_index(self.conn, &format!("db{}", self.db)).await
    }

    async fn full_text_search(&self, _r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
        Err(AppError::Msg("Full-text search is not available for Redis; use SCAN MATCH in the console".into()))
    }

    async fn column_hints(&self, _req: &HintReq) -> AppResult<Vec<Hint>> {
        Ok(vec![])
    }

    async fn erd(&self, _schemas: &[String]) -> AppResult<Erd> {
        Err(AppError::Msg("Redis has no tables".into()))
    }

    async fn table_shape(&self, _schema: &str, _table: &str) -> AppResult<TableShape> {
        Err(AppError::Msg("Redis has no tables".into()))
    }

    // ───── Connection: db.rs · app.rs ─────

    async fn server_version(&self) -> AppResult<String> {
        crate::redisdb::server_version(self.conn).await
    }

    async fn ping(&self) {
        crate::redisdb::ping(self.conn).await
    }

    // ───── Running: db.rs · session.rs · app.rs ─────

    async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        crate::redisdb::run(self.conn, sql, limit).await
    }

    async fn export(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        crate::redisdb::export(self.conn, sql, out).await
    }

    /// Within the size limits (redisdb::check_read). AppState::run_read_only has already refused anything but read commands (no CONFIG, KEYS)
    async fn run_read_only(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        crate::redisdb::check_read(self.conn, sql).await?;
        crate::redisdb::run(self.conn, sql, limit).await
    }

    async fn export_read_only(&self, _sql: &str, _out: &mut Exporter) -> AppResult<()> {
        Err(AppError::Msg("CSV export of Redis commands is not available in read-only mode".into()))
    }

    async fn open_session(&self) -> AppResult<Session> {
        Ok(Session::Redis(self.conn.clone()))
    }

    /// One command after another (no transaction)
    async fn execute_batch(&self, statements: &[String], _exact_one: bool, _prepared: bool, _snaps: &[Option<&str>]) -> AppResult<(u64, Snapshots)> {
        for st in statements {
            crate::redisdb::run(self.conn, st, 1).await?;
        }
        Ok((statements.len() as u64, vec![]))
    }
}
