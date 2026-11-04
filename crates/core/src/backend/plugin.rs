//! Driver plugins: each operation is a method call to the driver process (protocol: crates/driver)
use super::{Backend, Snapshots};
use crate::db::QueryResult;
use crate::error::{AppError, AppResult};
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, IndexEntry, SchemaInfo, SchemaObjects, TableDetail};
use crate::plugin::PluginConn;
use crate::session::Session;
use crate::transfer::Exporter;
use serde_json::{Value, json};
use std::sync::Arc;

/// Max rows of an MCP CSV export
const EXPORT_MAX_ROWS: usize = 1_000_000;

impl Backend for Arc<PluginConn> {
    fn label(&self) -> String {
        format!("the {} driver", self.driver)
    }

    // ───── Metadata: meta.rs · hints.rs · erd.rs · ddl.rs · fake.rs ─────

    async fn list_databases(&self) -> AppResult<Vec<String>> {
        self.call("databases", Value::Null).await
    }

    async fn list_schemas(&self) -> AppResult<Vec<SchemaInfo>> {
        self.call("schemas", json!({})).await
    }

    async fn list_objects(&self, schema: &str) -> AppResult<SchemaObjects> {
        self.call("objects", json!({ "schema": schema })).await
    }

    async fn table_detail(&self, schema: &str, table: &str) -> AppResult<TableDetail> {
        self.call("tableDetail", json!({ "schema": schema, "table": table })).await
    }

    async fn object_ddl(&self, schema: &str, name: &str, kind: &str) -> AppResult<String> {
        self.call("ddl", json!({ "schema": schema, "name": name, "kind": kind })).await
    }

    /// schema → table → column (up to 300 tables)
    async fn completion_meta(&self) -> AppResult<Vec<CompletionTable>> {
        let mut out = vec![];
        for s in self.list_schemas().await? {
            for r in self.list_objects(&s.name).await?.relations {
                if out.len() >= 300 {
                    return Ok(out);
                }
                let cols = self.table_detail(&s.name, &r.name).await.map(|d| d.columns.into_iter().map(|c| (c.name, c.data_type)).collect()).unwrap_or_default();
                out.push(CompletionTable { schema: s.name.clone(), name: r.name, columns: cols });
            }
        }
        Ok(out)
    }

    async fn object_index(&self) -> AppResult<Vec<IndexEntry>> {
        let mut out = vec![];
        for s in self.list_schemas().await? {
            out.push(IndexEntry { schema: s.name.clone(), name: s.name.clone(), kind: "schema".into() });
            for r in self.list_objects(&s.name).await?.relations {
                out.push(IndexEntry { schema: s.name.clone(), name: r.name, kind: r.kind });
            }
        }
        Ok(out)
    }

    /// Suggestions are optional in the console: none rather than an error
    async fn column_hints(&self, _req: &HintReq) -> AppResult<Vec<Hint>> {
        Ok(vec![])
    }

    // ───── Connection: db.rs · app.rs ─────

    async fn server_version(&self) -> AppResult<String> {
        self.call("version", Value::Null).await
    }

    // ───── Running: db.rs · session.rs · app.rs ─────

    async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        self.query(sql, limit).await
    }

    async fn export(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        let r = self.query(sql, 10_000_000).await?;
        write_all(&r, out).await
    }

    /// No server-side READ ONLY transaction — only what passes the guard check (one statement, read commands)
    async fn run_read_only(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        self.query_opts(sql, limit, true).await
    }

    async fn export_read_only(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        let r = tokio::time::timeout(std::time::Duration::from_secs(60), self.query_opts(sql, EXPORT_MAX_ROWS, true))
            .await
            .map_err(|_| AppError::Msg("The export took over 60 s — narrow the query".into()))??;
        write_all(&r, out).await
    }

    /// All at once (up to limit), without server cursors or manual transactions
    async fn open_session(&self) -> AppResult<Session> {
        Ok(Session::Plugin(self.clone()))
    }

    /// One by one (drivers know nothing about transactions — only exact_one is enforced)
    async fn execute_batch(&self, statements: &[String], exact_one: bool, _prepared: bool, _snaps: &[Option<&str>]) -> AppResult<(u64, Snapshots)> {
        let mut affected = 0;
        for st in statements {
            let n = self.query(st, 1).await?.rows_affected;
            crate::session::check_exact_one(st, n, exact_one)?;
            affected += n;
        }
        Ok((affected, vec![]))
    }
}

/// A driver's whole result to the export
async fn write_all(r: &QueryResult, out: &mut Exporter) -> AppResult<()> {
    out.header(&r.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()).await?;
    for row in &r.rows {
        out.row(row).await?;
    }
    Ok(())
}
