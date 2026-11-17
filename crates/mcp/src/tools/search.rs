//! Finding rows: full_text_search, list_rag_indexes, semantic_search
use super::result_json;
use crate::Server;
use qurry_core::store::DbKind;
use serde_json::{json, Value};

impl Server {
    pub(super) async fn full_text_search(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, kind, db, _) = self.target(a).await?;
        let schemas: Vec<String> = match a.get("schemas").and_then(|v| v.as_array()) {
            Some(v) => v.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            None if kind == DbKind::Postgres => st.list_schemas(t.clone()).await.map_err(e)?.into_iter().filter(|s| !s.is_system).map(|s| s.name).collect(),
            None => vec![db],
        };
        let req = qurry_core::meta::FtsRequest {
            text: s("text").ok_or("text is required")?,
            mode: s("mode").unwrap_or_else(|| "contains".into()),
            match_case: a.get("match_case").and_then(|v| v.as_bool()).unwrap_or(false),
            columns: s("columns").unwrap_or_else(|| "text".into()),
            schemas,
            tables: vec![],
            limit: n("limit_per_table", 10).clamp(1, 200),
        };
        let hits = st.full_text_search(t, req).await.map_err(e)?;
        Ok(json!(hits.iter().map(|h| json!({ "schema": h.schema, "table": h.table, "where": h.where_clause, "rows": result_json(&h.result) })).collect::<Vec<_>>()))
    }

    pub(super) async fn list_rag_indexes(&self) -> Result<Value, String> {
        let st = &self.state;
        let e = |e: qurry_core::error::AppError| e.to_string();
        let names: std::collections::HashMap<String, String> = self.sources().await?.into_iter().map(|c| (c.config.id, c.config.name)).collect();
        let shown = |t: &str| names.contains_key(qurry_core::app::split_target(t).0);
        let name_of = |t: &str| {
            let (id, db) = qurry_core::app::split_target(t);
            let n = names.get(id).cloned().unwrap_or_else(|| id.to_string());
            db.map(|d| format!("{n} / {d}")).unwrap_or(n)
        };
        Ok(json!(st.rag_indexes().map_err(e)?.iter().filter(|x| shown(&x.source) && shown(&x.dest)).map(|x| json!({
            "name": x.name, "id": x.id, "source": name_of(&x.source), "table": format!("{}.{}", x.schema, x.table),
            "text_columns": x.text_columns, "store": name_of(&x.dest), "collection": x.collection,
            "embeddings": format!("{} {}", x.provider, x.model), "rows": x.rows, "built_at": x.built_at
        })).collect::<Vec<_>>()))
    }

    pub(super) async fn semantic_search(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let index = s("index").ok_or("index is required")?;
        let query = s("query").ok_or("query is required")?;
        // Both the source table and the store must be visible to MCP (the store contains text from source rows)
        let ids: std::collections::HashSet<String> = self.sources().await?.into_iter().map(|c| c.config.id).collect();
        let shown = |t: &str| ids.contains(qurry_core::app::split_target(t).0);
        // Resolve once and search exactly the index that was checked (by id)
        let x = st.rag_get(&index).map_err(e)?;
        if !(shown(&x.source) && shown(&x.dest)) {
            return Err(format!("RAG index '{index}' not found"));
        }
        Ok(result_json(&st.rag_search(x.id, query, n("limit", 10).min(50)).await.map_err(e)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tests::source;
    use std::sync::Arc;

    /// "Docs" (on a shared source) and "docs" (on a hidden one): the index that is checked is the one that runs
    #[tokio::test]
    async fn semantic_search_checks_the_index_it_runs() {
        let dir = std::env::temp_dir().join(format!("qurry-mcp-rag-{}-{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()));
        let state = qurry_core::open_state(Some(dir.clone())).unwrap();
        state.store.save(&source("shown", "Shown", Some("read"), false).config).unwrap();
        state.store.save(&source("hidden", "Hidden", Some("off"), false).config).unwrap();
        let idx = |id: &str, name: &str, src: &str| json!({ "id": id, "name": name, "source": src, "schema": "public", "table": "t", "idColumn": "id",
            "textColumns": ["body"], "provider": "ollama", "model": "m", "dest": src, "collection": "c" });
        std::fs::write(dir.join("rag.json"), json!([idx("i1", "Docs", "shown"), idx("i2", "docs", "hidden")]).to_string()).unwrap();
        let server = Server { state: Arc::new(state), allow_write: false };
        let search = |index: &str| json!({ "index": index, "query": "q" });
        // The visible one gets past the check (and stops at "not built yet"); the hidden one is not found, by name or id
        let e = server.call("semantic_search", &search("Docs")).await.unwrap_err();
        assert!(e.contains("Build the index \"Docs\" first"), "{e}");
        for w in ["docs", "i2"] {
            let e = server.call("semantic_search", &search(w)).await.unwrap_err();
            assert!(e.contains("not found"), "{w}: {e}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
