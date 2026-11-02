//! Redis: one schema per DB (named db<n>), keys as objects. The queries live in crate::redisdb
use super::Backend;
use crate::erd::Erd;
use crate::error::{AppError, AppResult};
use crate::fake::TableShape;
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, FtsHit, FtsRequest, IndexEntry, SchemaInfo, SchemaObjects, TableDetail};
use crate::migrate::Model;
use crate::redisdb::RedisConn;
use crate::transfer::Tx;

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

    // ───── Jobs: migrate.rs · transfer.rs · audit.rs ─────

    async fn schema_model(&self, _schema: &str) -> AppResult<Model> {
        Err(AppError::Msg("Redis has no schema to compare".into()))
    }

    async fn apply_migration(&self, _stmts: &[&str], _fail: impl Fn(String) -> AppError) -> AppResult<()> {
        Err(AppError::Msg("Redis has no migrations".into()))
    }

    async fn import_tx(&self) -> AppResult<Tx> {
        Err(AppError::Msg("Import into Redis is not supported".into()))
    }
}
