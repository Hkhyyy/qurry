//! RAG: embed table text (OpenAI · Ollama), store it in a vector store (pgvector · Qdrant · Chroma · Milvus), and find rows close to a question.
//! Writes to the store usually go the same way as queries (run_query): nothing is written to a Read-only Data Source.
use crate::app::AppState;
use crate::db::{ColumnInfo, QueryResult, cell_text};
use crate::error::{AppError, AppResult};
use crate::sql::{literal, quote_ident};
use crate::store::DbKind;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

/// Batch size for embedding
const BATCH: usize = 64;
/// Text cap per row (within the embedding model's input limit)
const MAX_CHARS: usize = 4000;
/// Secret name holding the OpenAI API key
pub const OPENAI_KEY: &str = "rag:openai";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RagIndex {
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Source connection target (conn or conn::db) · schema · table · row id column · text columns to embed
    pub source: String,
    pub schema: String,
    pub table: String,
    pub id_column: String,
    pub text_columns: Vec<String>,
    /// openai | ollama
    pub provider: String,
    pub model: String,
    /// Empty = default URL (OpenAI-compatible server·another Ollama URL)
    #[serde(default)]
    pub base_url: String,
    /// Where to store: connection target · collection (for pgvector, [schema.]table)
    pub dest: String,
    pub collection: String,
    #[serde(default)]
    pub dim: Option<usize>,
    #[serde(default)]
    pub rows: u64,
    #[serde(default)]
    pub built_at: Option<String>,
}

pub fn default_model(provider: &str) -> &'static str {
    if provider == "openai" { "text-embedding-3-small" } else { "nomic-embed-text" }
}

/// Server URL: configured → env var (OPENAI_BASE_URL · OLLAMA_HOST, same names as each tool) → default
fn base_url(provider: &str, base: &str) -> String {
    let (env, def) = if provider == "openai" { ("OPENAI_BASE_URL", "https://api.openai.com/v1") } else { ("OLLAMA_HOST", "http://localhost:11434") };
    let b = Some(base.trim().to_string()).filter(|b| !b.is_empty()).or_else(|| std::env::var(env).ok().filter(|b| !b.trim().is_empty())).unwrap_or_else(|| def.into());
    // OLLAMA_HOST is sometimes written without a scheme, like "127.0.0.1:11434"
    let b = if b.starts_with("http://") || b.starts_with("https://") { b } else { format!("http://{b}") };
    b.trim_end_matches('/').to_string()
}

/// Whether the saved OpenAI key (or OPENAI_API_KEY) may be sent to this server: OpenAI or the server set via OPENAI_BASE_URL, and https (http on this machine is fine).
/// Not sent to "OpenAI-compatible" servers set per index (so a real key doesn't go in plaintext to local · LAN servers)
fn openai_key_allowed(base: &str) -> bool {
    let url = base_url("openai", base);
    let trusted = std::iter::once("https://api.openai.com/v1".to_string()).chain(std::env::var("OPENAI_BASE_URL").ok().map(|b| base_url("openai", &b)));
    let host = url.split("://").nth(1).unwrap_or("").split(['/', '?']).next().unwrap_or("");
    let host = host.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit())).map(|(h, _)| h).unwrap_or(host);
    let local = ["localhost", "127.0.0.1", "[::1]"].contains(&host);
    trusted.into_iter().any(|t| t == url) && (url.starts_with("https://") || local)
}

/// Text batch → vectors (blocking HTTP; call inside spawn_blocking)
fn embed_blocking(provider: &str, model: &str, base: &str, key: Option<&str>, texts: &[String]) -> AppResult<Vec<Vec<f32>>> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(300))).http_status_as_error(false).max_redirects(0).build().into();
    let (url, body) = match provider {
        "openai" => (format!("{}/embeddings", base_url(provider, base)), json!({ "model": model, "input": texts })),
        "ollama" => (format!("{}/api/embed", base_url(provider, base)), json!({ "model": model, "input": texts })),
        p => return Err(AppError::Msg(format!("Unknown embedding provider '{p}' (use openai or ollama)"))),
    };
    let mut req = agent.post(&url);
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        req = req.header("Authorization", &format!("Bearer {k}"));
    }
    let shown = crate::store::without_userinfo(&url);
    let mut res = req.send_json(body).map_err(|e| AppError::Msg(format!("{provider}: can't reach {shown}: {e}").replace(&url, &shown)))?;
    let status = res.status().as_u16();
    let v: Value = serde_json::from_str(&res.body_mut().read_to_string().unwrap_or_default()).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let msg = v.pointer("/error/message").or_else(|| v.get("error")).and_then(Value::as_str).map(String::from).unwrap_or_else(|| format!("HTTP {status}"));
        let hint = if provider == "ollama" && msg.contains("not found") { format!(" — run: ollama pull {model}") } else { String::new() };
        return Err(AppError::Msg(format!("{provider}: {msg}{hint}")));
    }
    let vecs: Vec<Vec<f32>> = match provider {
        // OpenAI preserves data[].index order
        "openai" => {
            let mut d: Vec<(u64, Vec<f32>)> = v["data"].as_array().into_iter().flatten().map(|x| (x["index"].as_u64().unwrap_or(0), floats(&x["embedding"]))).collect();
            d.sort_by_key(|x| x.0);
            d.into_iter().map(|x| x.1).collect()
        }
        _ => v["embeddings"].as_array().into_iter().flatten().map(floats).collect(),
    };
    if vecs.len() != texts.len() || vecs.iter().any(|x| x.is_empty()) {
        return Err(AppError::Msg(format!("{provider}: expected {} embeddings, got {}", texts.len(), vecs.len())));
    }
    Ok(vecs)
}

fn floats(v: &Value) -> Vec<f32> {
    v.as_array().into_iter().flatten().filter_map(Value::as_f64).map(|f| f as f32).collect()
}

/// Row → text to embed: just the value for one column, "column: value" lines for several
fn row_text(cols: &[String], vals: &[Value]) -> String {
    let parts: Vec<String> = cols.iter().zip(vals).filter(|(_, v)| !v.is_null()).map(|(c, v)| if cols.len() == 1 { cell_text(v) } else { format!("{c}: {}", cell_text(v)) }).collect();
    let t = parts.join("\n");
    t.chars().take(MAX_CHARS).collect()
}

fn vec_lit(v: &[f32]) -> String {
    format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))
}

/// pgvector table name: "schema.name" or "name" (public)
fn pg_table(collection: &str) -> String {
    let (s, t) = collection.split_once('.').unwrap_or(("public", collection));
    format!("{}.{}", quote_ident(DbKind::Postgres, s), quote_ident(DbKind::Postgres, t))
}

/// Qdrant point ids must be integers or UUIDs: always the same UUID for a source id
fn point_id(source_id: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, source_id.as_bytes()).to_string()
}

/// The first of these column names present in the result
fn col(r: &QueryResult, names: &[&str]) -> Option<usize> {
    names.iter().find_map(|n| r.columns.iter().position(|c| c.name == *n))
}

impl AppState {
    fn rag_path(&self) -> std::path::PathBuf {
        self.store.dir().join("rag.json")
    }

    pub fn rag_indexes(&self) -> AppResult<Vec<RagIndex>> {
        match std::fs::read_to_string(self.rag_path()) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(_) => Ok(vec![]),
        }
    }

    fn rag_write(&self, list: &[RagIndex]) -> AppResult<()> {
        std::fs::write(self.rag_path(), serde_json::to_string_pretty(list)?)?;
        Ok(())
    }

    fn rag_get(&self, id: &str) -> AppResult<RagIndex> {
        self.rag_indexes()?.into_iter().find(|x| x.id == id || x.name == id).ok_or_else(|| AppError::Msg(format!("RAG index '{id}' not found")))
    }

    /// Create or update (new if id is empty). Changing the source·model·destination requires a rebuild (rows set to 0)
    pub fn rag_save(&self, mut idx: RagIndex) -> AppResult<RagIndex> {
        if idx.name.trim().is_empty() || idx.text_columns.is_empty() || idx.id_column.is_empty() || idx.collection.trim().is_empty() {
            return Err(AppError::Msg("Name, ID column, text columns and collection are required".into()));
        }
        if idx.model.trim().is_empty() {
            idx.model = default_model(&idx.provider).into();
        }
        let mut list = self.rag_indexes()?;
        if idx.id.is_empty() {
            idx.id = uuid::Uuid::new_v4().to_string();
        }
        if let Some(old) = list.iter_mut().find(|x| x.id == idx.id) {
            let same = old.source == idx.source && old.schema == idx.schema && old.table == idx.table && old.model == idx.model && old.provider == idx.provider && old.dest == idx.dest && old.collection == idx.collection;
            if same {
                (idx.dim, idx.rows, idx.built_at) = (old.dim, old.rows, old.built_at.clone());
            } else {
                (idx.dim, idx.rows, idx.built_at) = (None, 0, None);
            }
            *old = idx.clone();
        } else {
            list.push(idx.clone());
        }
        self.rag_write(&list)?;
        Ok(idx)
    }

    /// Delete only the definition (vectors in the store remain)
    pub fn rag_delete(&self, id: &str) -> AppResult<()> {
        let list: Vec<RagIndex> = self.rag_indexes()?.into_iter().filter(|x| x.id != id).collect();
        self.rag_write(&list)
    }

    pub fn rag_has_key(&self) -> bool {
        self.secret(OPENAI_KEY).ok().flatten().is_some_and(|k| !k.is_empty())
    }

    pub fn rag_set_key(&self, key: Option<String>) -> AppResult<()> {
        self.set_secret(OPENAI_KEY, key.as_deref().filter(|k| !k.is_empty()))
    }

    async fn embed(&self, idx: &RagIndex, texts: Vec<String>) -> AppResult<Vec<Vec<f32>>> {
        let key = if idx.provider == "openai" {
            // Saved key, else OPENAI_API_KEY (OpenAI-compatible local servers work without a key)
            if !openai_key_allowed(&idx.base_url) {
                // Other OpenAI-compatible servers: no key (local servers usually have none)
                None
            } else {
                let k = self.secret(OPENAI_KEY)?.filter(|k| !k.is_empty()).or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()));
                if k.is_none() && idx.base_url.trim().is_empty() && std::env::var("OPENAI_BASE_URL").is_err() {
                    return Err(AppError::Msg("Set your OpenAI API key first".into()));
                }
                k
            }
        } else {
            None
        };
        let (p, m, b) = (idx.provider.clone(), idx.model.clone(), idx.base_url.clone());
        tokio::task::spawn_blocking(move || embed_blocking(&p, &m, &b, key.as_deref(), &texts)).await.map_err(|e| AppError::Msg(e.to_string()))?
    }

    fn dest_kind(&self, idx: &RagIndex) -> AppResult<DbKind> {
        let k = self.source(&idx.dest)?.kind;
        match k {
            DbKind::Postgres | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus => Ok(k),
            _ => Err(AppError::Msg("Store vectors in PostgreSQL (pgvector), Qdrant, Chroma or Milvus".into())),
        }
    }

    /// Create the collection (table) in the store if missing
    async fn rag_prepare(&self, idx: &RagIndex, kind: DbKind, dim: usize) -> AppResult<()> {
        let q = |s: String| self.run_query(idx.dest.clone(), s, Some(10), None);
        let c = &idx.collection;
        match kind {
            DbKind::Postgres => {
                q("CREATE EXTENSION IF NOT EXISTS vector".into()).await.map_err(|e| AppError::Msg(format!("pgvector isn't available on this server ({e})")))?;
                q(format!("CREATE TABLE IF NOT EXISTS {} (source_id text PRIMARY KEY, content text, embedding vector({dim}))", pg_table(c))).await?;
            }
            DbKind::Qdrant => {
                if q(format!("GET /collections/{c}")).await.is_err() {
                    q(format!("PUT /collections/{c}\n{}", json!({ "vectors": { "size": dim, "distance": "Cosine" } }))).await?;
                }
            }
            DbKind::Chroma => {
                q(format!("POST /collections\n{}", json!({ "name": c, "get_or_create": true, "configuration": { "hnsw": { "space": "cosine" } } }))).await?;
            }
            DbKind::Milvus => {
                let has = q(format!("POST /collections/has\n{}", json!({ "collectionName": c }))).await?;
                let exists = col(&has, &["has"]).and_then(|i| has.rows.first().map(|r| r[i] == json!(true))).unwrap_or(false);
                if !exists {
                    let schema = json!({ "autoId": false, "enableDynamicField": false, "fields": [
                        { "fieldName": "source_id", "dataType": "VarChar", "isPrimary": true, "elementTypeParams": { "max_length": 512 } },
                        { "fieldName": "content", "dataType": "VarChar", "elementTypeParams": { "max_length": 65535 } },
                        { "fieldName": "embedding", "dataType": "FloatVector", "elementTypeParams": { "dim": dim } },
                    ] });
                    q(format!("POST /collections/create\n{}", json!({ "collectionName": c, "schema": schema, "indexParams": [{ "fieldName": "embedding", "indexName": "embedding", "metricType": "COSINE", "indexType": "AUTOINDEX" }] }))).await?;
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    async fn rag_upsert(&self, idx: &RagIndex, kind: DbKind, items: &[(String, String, Vec<f32>)]) -> AppResult<()> {
        let c = &idx.collection;
        let sql = match kind {
            DbKind::Postgres => {
                let values: Vec<String> = items.iter().map(|(id, t, v)| format!("({}, {}, '{}')", literal(DbKind::Postgres, Some(id)), literal(DbKind::Postgres, Some(t)), vec_lit(v))).collect();
                format!("INSERT INTO {} (source_id, content, embedding) VALUES {} ON CONFLICT (source_id) DO UPDATE SET content = excluded.content, embedding = excluded.embedding", pg_table(c), values.join(", "))
            }
            DbKind::Qdrant => format!("PUT /collections/{c}/points?wait=true\n{}", json!({ "points": items.iter().map(|(id, t, v)| json!({ "id": point_id(id), "vector": v, "payload": { "source_id": id, "content": t } })).collect::<Vec<_>>() })),
            DbKind::Chroma => format!("POST /collections/{c}/upsert\n{}", json!({ "ids": items.iter().map(|x| &x.0).collect::<Vec<_>>(), "embeddings": items.iter().map(|x| &x.2).collect::<Vec<_>>(), "documents": items.iter().map(|x| &x.1).collect::<Vec<_>>() })),
            DbKind::Milvus => format!("POST /entities/upsert\n{}", json!({ "collectionName": c, "data": items.iter().map(|(id, t, v)| json!({ "source_id": id, "content": t, "embedding": v })).collect::<Vec<_>>() })),
            _ => unreachable!(),
        };
        self.run_query(idx.dest.clone(), sql, Some(10), None).await?;
        Ok(())
    }

    /// Re-embed the source table from scratch (same ids are overwritten). Progress: job rows/total. Returns rows inserted
    pub async fn rag_build(&self, id: String, job_id: String) -> AppResult<u64> {
        let job = self.job(&job_id).unwrap_or_else(|| self.start_job(&job_id));
        let r = async {
            let idx = self.rag_get(&id)?;
            let src_kind = self.source(&idx.source)?.kind;
            if matches!(src_kind, DbKind::Redis | DbKind::Mongodb) || src_kind.is_vector() {
                return Err(AppError::Msg("Pick a SQL table as the source".into()));
            }
            let dest_kind = self.dest_kind(&idx)?;
            let q = |s: &str| quote_ident(src_kind, s);
            let table = format!("{}.{}", q(&idx.schema), q(&idx.table));
            let count = self.run_query(idx.source.clone(), format!("SELECT count(*) FROM {table}"), Some(1), None).await?;
            job.total.store(count.rows.first().and_then(|r| r.first()).map(cell_text).and_then(|s| s.parse().ok()).unwrap_or(0), Ordering::Relaxed);
            let cols: Vec<String> = std::iter::once(&idx.id_column).chain(&idx.text_columns).map(|c| q(c)).collect();
            let (mut last, mut done, mut dim) = (None::<String>, 0u64, None::<usize>);
            loop {
                // Keep reading in id order (OFFSET gets slower further in). Row count via run_query's limit
                let after = last.as_ref().map(|l| format!(" WHERE {} > {}", q(&idx.id_column), literal(src_kind, Some(l)))).unwrap_or_default();
                let page = self.run_query(idx.source.clone(), format!("SELECT {} FROM {table}{after} ORDER BY {}", cols.join(", "), q(&idx.id_column)), Some(BATCH), None).await?;
                if page.rows.is_empty() {
                    break;
                }
                last = page.rows.last().and_then(|r| r.first()).map(cell_text);
                let rows: Vec<(String, String)> = page.rows.iter().filter(|r| !r[0].is_null()).map(|r| (cell_text(&r[0]), row_text(&idx.text_columns, &r[1..]))).filter(|(_, t)| !t.trim().is_empty()).collect();
                if !rows.is_empty() {
                    let vecs = self.embed(&idx, rows.iter().map(|x| x.1.clone()).collect()).await?;
                    if dim.is_none() {
                        let d = vecs[0].len();
                        self.rag_prepare(&idx, dest_kind, d).await?;
                        dim = Some(d);
                    }
                    let items: Vec<(String, String, Vec<f32>)> = rows.into_iter().zip(vecs).map(|((i, t), v)| (i, t, v)).collect();
                    self.rag_upsert(&idx, dest_kind, &items).await?;
                    done += items.len() as u64;
                }
                job.rows.store(done, Ordering::Relaxed);
                job.done.fetch_add(page.rows.len() as u64, Ordering::Relaxed);
                if job.cancelled() {
                    return Err(AppError::Msg(crate::transfer::CANCELLED.into()));
                }
                if page.rows.len() < BATCH {
                    break;
                }
            }
            // pgvector: distance index (HNSW supports up to 2000 dimensions — beyond that, no index)
            if dest_kind == DbKind::Postgres && dim.is_some() {
                let t = pg_table(&idx.collection);
                self.run_query(idx.dest.clone(), format!("CREATE INDEX IF NOT EXISTS {} ON {t} USING hnsw (embedding vector_cosine_ops)", quote_ident(DbKind::Postgres, &format!("{}_embedding_idx", idx.collection.rsplit('.').next().unwrap_or("rag")))), Some(1), None).await.ok();
            }
            // Build results (rag_save keeps these values when saving user edits)
            let mut list = self.rag_indexes()?;
            if let Some(x) = list.iter_mut().find(|x| x.id == idx.id) {
                x.dim = dim.or(x.dim);
                x.rows = done;
                x.built_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
            }
            self.rag_write(&list)?;
            Ok(done)
        }
        .await;
        self.end_job(&job_id);
        r
    }

    /// Rows close to the question: score (1 = same meaning) + the source row's columns. If the source can't be read, score · id · content
    pub async fn rag_search(&self, id: String, query: String, limit: usize) -> AppResult<QueryResult> {
        let started = std::time::Instant::now();
        let idx = self.rag_get(&id)?;
        if idx.rows == 0 && idx.dim.is_none() {
            return Err(AppError::Msg(format!("Build the index \"{}\" first", idx.name)));
        }
        let kind = self.dest_kind(&idx)?;
        let v = self.embed(&idx, vec![query]).await?.remove(0);
        let c = &idx.collection;
        let limit = limit.clamp(1, 200);
        let sql = match kind {
            DbKind::Postgres => format!("SELECT source_id, content, 1 - (embedding <=> '{0}') AS score FROM {1} ORDER BY embedding <=> '{0}' LIMIT {limit}", vec_lit(&v), pg_table(c)),
            DbKind::Qdrant => format!("POST /collections/{c}/points/search\n{}", json!({ "vector": v, "limit": limit, "with_payload": true })),
            DbKind::Chroma => format!("POST /collections/{c}/query\n{}", json!({ "query_embeddings": [v], "n_results": limit, "include": ["documents", "distances"] })),
            DbKind::Milvus => format!("POST /entities/search\n{}", json!({ "collectionName": c, "data": [v], "annsField": "embedding", "limit": limit, "outputFields": ["source_id", "content"] })),
            _ => unreachable!(),
        };
        let r = self.run_query(idx.dest.clone(), sql, Some(limit), None).await?;
        let (ic, tc) = (col(&r, &["source_id", "payload.source_id", "id"]), col(&r, &["content", "payload.content", "document"]));
        let sc = col(&r, &["score", "distance"]);
        // Chroma returns distance (0 = identical), the others similarity
        let chroma = kind == DbKind::Chroma;
        let hits: Vec<(String, f64, Value)> = r
            .rows
            .iter()
            .map(|row| {
                let s = sc.and_then(|i| row[i].as_f64()).unwrap_or(0.0);
                (ic.map(|i| cell_text(&row[i])).unwrap_or_default(), if chroma { 1.0 - s } else { s }, tc.map(|i| row[i].clone()).unwrap_or(Value::Null))
            })
            .collect();
        let score = |s: f64| json!((s * 10000.0).round() / 10000.0);
        let col_info = |n: &str, t: &str| ColumnInfo { name: n.into(), type_name: t.into() };
        // Attach the source rows
        let src_kind = self.source(&idx.source)?.kind;
        let q = |s: &str| quote_ident(src_kind, s);
        let src = if hits.is_empty() {
            None
        } else {
            let ids: Vec<String> = hits.iter().map(|h| literal(src_kind, Some(&h.0))).collect();
            self.run_query(idx.source.clone(), format!("SELECT * FROM {}.{} WHERE {} IN ({})", q(&idx.schema), q(&idx.table), q(&idx.id_column), ids.join(", ")), Some(limit), None).await.ok()
        };
        let mut out = QueryResult { columns: vec![], rows: vec![], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false };
        match src.as_ref().and_then(|s| s.columns.iter().position(|c| c.name.eq_ignore_ascii_case(&idx.id_column)).map(|p| (s, p))) {
            Some((s, pos)) => {
                out.columns = std::iter::once(col_info("score", "FLOAT8")).chain(s.columns.iter().cloned()).collect();
                for (hid, sv, _) in &hits {
                    if let Some(row) = s.rows.iter().find(|row| cell_text(&row[pos]) == *hid) {
                        out.rows.push(std::iter::once(score(*sv)).chain(row.iter().cloned()).collect());
                    }
                }
            }
            None => {
                out.columns = vec![col_info("score", "FLOAT8"), col_info("id", "TEXT"), col_info("content", "TEXT")];
                out.rows = hits.into_iter().map(|(i, s, t)| vec![score(s), json!(i), t]).collect();
            }
        }
        out.elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_key_only_goes_to_openai() {
        if std::env::var("OPENAI_BASE_URL").is_ok() {
            return;
        }
        assert!(openai_key_allowed(""));
        assert!(openai_key_allowed("https://api.openai.com/v1/"));
        assert!(!openai_key_allowed("http://192.168.0.5:8000/v1"));
        assert!(!openai_key_allowed("192.168.0.5:8000/v1"));
        assert!(!openai_key_allowed("https://api.openai.com.evil.example/v1"));
        assert!(!openai_key_allowed("http://localhost:1234/v1"), "a local OpenAI-compatible server gets no key unless OPENAI_BASE_URL says so");
    }

    #[test]
    fn row_text_and_ids() {
        let cols = vec!["title".to_string(), "body".to_string()];
        assert_eq!(row_text(&cols, &[json!("Apple"), json!("red fruit")]), "title: Apple\nbody: red fruit");
        assert_eq!(row_text(&cols[..1], &[json!("Apple")]), "Apple");
        assert_eq!(row_text(&cols, &[json!("Apple"), Value::Null]), "title: Apple");
        assert_eq!(point_id("42"), point_id("42"));
        assert_ne!(point_id("42"), point_id("43"));
        assert_eq!(pg_table("docs_vec"), "public.docs_vec");
        assert_eq!(pg_table("rag.Docs"), "rag.\"Docs\"");
        assert_eq!(base_url("ollama", "http://gpu:11434/"), "http://gpu:11434");
        assert_eq!(base_url("openai", "https://example.com/v1"), "https://example.com/v1");
    }

    /// Fake embedding server: OpenAI(/embeddings)·Ollama(/api/embed) formats. Vector = counts of letters a~h
    async fn fake_embedder() -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else { return };
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 65536];
                    let (head_end, len) = loop {
                        let n = s.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                            let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                            break (p + 4, len);
                        }
                    };
                    while buf.len() < head_end + len {
                        let n = s.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                    let body: Value = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);
                    let vecs: Vec<Vec<f32>> = body["input"].as_array().into_iter().flatten().map(|t| {
                        let t = t.as_str().unwrap_or("").to_lowercase();
                        "abcdefgh".chars().map(|c| t.matches(c).count() as f32 + 0.01).collect()
                    }).collect();
                    let out = if head.contains("/api/embed") { json!({ "embeddings": vecs }) } else { json!({ "data": vecs.iter().enumerate().map(|(i, v)| json!({ "index": i, "embedding": v })).collect::<Vec<_>>() }) };
                    let b = out.to_string();
                    let _ = s.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}", b.len()).as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    /// PG table → build and search in (pgvector · Qdrant · Chroma · Milvus if available). With a fake embedding server (both OpenAI·Ollama formats)
    #[tokio::test]
    #[ignore]
    async fn rag_build_and_search_everywhere() {
        let Ok(pg) = std::env::var("QURRY_TEST_PG") else { return };
        let p: Vec<&str> = pg.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-rag-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir.clone())).unwrap();
        let save = |id: &str, kind: &str, host: &str, port: u16, user: &str, db: &str| {
            let cfg: crate::store::ConnectionConfig = serde_json::from_value(json!({ "id": id, "name": id, "kind": kind, "host": host, "port": port, "user": user, "database": db })).unwrap();
            st.store.save(&cfg).unwrap();
        };
        save("pg", "postgres", p[0], p[1].parse().unwrap(), p[2], p[3]);
        let q = |s: &str| st.run_query("pg".into(), s.into(), None, None);
        if q("create extension if not exists vector").await.is_err() {
            return;
        }
        q("drop table if exists qurry_rag_docs").await.ok();
        q("create table qurry_rag_docs (id int primary key, title text, body text)").await.unwrap();
        // Insert enough rows to cross the batch (64) boundary
        q("insert into qurry_rag_docs select g, 'filler ' || g, 'zzz' from generate_series(1, 70) g").await.unwrap();
        q("insert into qurry_rag_docs values (100, 'abba', 'aaaa bbbb'), (101, 'hedge', 'eeee hhhh'), (102, 'cafe', 'cccc ffff')").await.unwrap();
        let base = fake_embedder().await;
        let mut dests = vec![("pg".to_string(), "qurry_rag_vec".to_string())];
        for (env, kind, db) in [("QURRY_TEST_QDRANT", "qdrant", "qdrant"), ("QURRY_TEST_CHROMA", "chroma", "default_database"), ("QURRY_TEST_MILVUS", "milvus", "default")] {
            if let Ok(e) = std::env::var(env) {
                let v: Vec<&str> = e.split(':').collect();
                if crate::plugin::driver_path(kind).is_some() {
                    save(kind, kind, v[0], v[1].parse().unwrap(), "", db);
                    dests.push((kind.to_string(), "qurry_rag_vec".into()));
                }
            }
        }
        q("drop table if exists qurry_rag_vec").await.ok();
        for (i, (dest, coll)) in dests.iter().enumerate() {
            // Delete leftover collections
            match dest.as_str() {
                "qdrant" | "chroma" => { st.run_query(dest.clone(), format!("DELETE /collections/{coll}"), None, None).await.ok(); }
                "milvus" => { st.run_query(dest.clone(), format!("POST /collections/drop\n{{collectionName: '{coll}'}}"), None, None).await.ok(); }
                _ => {}
            }
            let provider = if i % 2 == 0 { "ollama" } else { "openai" };
            let idx = st.rag_save(RagIndex { name: format!("docs-{dest}"), source: "pg".into(), schema: "public".into(), table: "qurry_rag_docs".into(), id_column: "id".into(), text_columns: vec!["title".into(), "body".into()], provider: provider.into(), model: "fake".into(), base_url: base.clone(), dest: dest.clone(), collection: coll.clone(), ..Default::default() }).unwrap();
            let n = st.rag_build(idx.id.clone(), format!("rag-{dest}")).await.unwrap_or_else(|e| panic!("{dest}: {e}"));
            assert_eq!(n, 73, "{dest}");
            let saved = st.rag_indexes().unwrap().into_iter().find(|x| x.id == idx.id).unwrap();
            assert_eq!((saved.rows, saved.dim), (73, Some(8)));
            let r = st.rag_search(idx.id.clone(), "hhh eee".into(), 3).await.unwrap_or_else(|e| panic!("{dest}: {e}"));
            let title = r.columns.iter().position(|c| c.name == "title").unwrap_or_else(|| panic!("{dest}: {:?}", r.columns.iter().map(|c| &c.name).collect::<Vec<_>>()));
            assert_eq!(r.rows[0][title], json!("hedge"), "{dest}: {:?}", r.rows);
            assert!(r.rows[0][0].as_f64().unwrap() > 0.9, "{dest}: score {:?}", r.rows[0][0]);
            // Rebuilding overwrites the same ids too
            assert_eq!(st.rag_build(idx.id.clone(), "again".into()).await.unwrap(), 73);
        }
        assert_eq!(q("select count(*) from qurry_rag_vec").await.unwrap().rows[0][0], json!(73));
        q("drop table qurry_rag_vec").await.ok();
        q("drop table qurry_rag_docs").await.ok();
        std::fs::remove_dir_all(dir).ok();
    }
}
