//! Driver plugins: each operation is a method call to the driver process (protocol: crates/driver)
use super::Backend;
use crate::error::AppResult;
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, IndexEntry, SchemaInfo, SchemaObjects, TableDetail};
use crate::plugin::PluginConn;
use serde_json::{Value, json};

impl Backend for PluginConn {
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
}
