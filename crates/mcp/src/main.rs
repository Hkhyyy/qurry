//! Qurry MCP 서버 (stdio, JSON-RPC 2.0). GUI·TUI 와 같은 엔진(qurry-core)·같은 Data Source 를 쓴다.
//!
//! 기본은 읽기 전용: PG/MySQL 은 READ ONLY 트랜잭션 안에서 한 문장만, Redis 는 읽기 명령만.
//! `--allow-write` 로 켜면 `execute` 도구가 생긴다 (Data Source 의 Read-only 설정은 그래도 지킨다).
use qurry_core::app::AppState;
use qurry_core::db::QueryResult;
use qurry_core::sql;
use qurry_core::store::DbKind;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

const SERVER_NAME: &str = "qurry";
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_ROWS: usize = 1000;

struct Server {
    state: Arc<AppState>,
    allow_write: bool,
}

fn tool(name: &str, desc: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "description": desc,
        "inputSchema": { "type": "object", "properties": props, "required": required },
        "annotations": { "readOnlyHint": read_only, "openWorldHint": false }
    })
}

fn ds_props(extra: Value) -> Value {
    let mut p = json!({
        "data_source": { "type": "string", "description": "Data source name or id (see list_data_sources)" },
        "database": { "type": "string", "description": "Database name (default: the data source's default database; Redis: db0, db1, …)" }
    });
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    p
}

impl Server {
    fn tools(&self) -> Vec<Value> {
        let mut t = vec![
            tool("list_data_sources", "List configured data sources (PostgreSQL, MySQL, Redis and installed drivers: SQL Server, Oracle, SQLite, DuckDB, ClickHouse, MongoDB, Qdrant, Chroma, Milvus) shared with the Qurry app.", json!({}), &[], true),
            tool("list_databases", "List databases of a data source.", ds_props(json!({})), &["data_source"], true),
            tool("list_schemas", "List schemas in a database (MySQL/Redis: the database itself).", ds_props(json!({})), &["data_source"], true),
            tool(
                "list_objects",
                "List tables, views, materialized views, sequences and routines in a schema (Redis: keys with their types).",
                ds_props(json!({ "schema": { "type": "string", "description": "Schema name (PostgreSQL default: public)" } })),
                &["data_source"],
                true,
            ),
            tool(
                "describe_table",
                "Columns (type, nullability, default, comment, primary key), indexes and constraints of a table.",
                ds_props(json!({ "schema": { "type": "string" }, "table": { "type": "string" } })),
                &["data_source", "table"],
                true,
            ),
            tool(
                "get_ddl",
                "DDL of a table, view, materialized view, sequence or routine (Redis: commands that recreate the key).",
                ds_props(json!({ "schema": { "type": "string" }, "name": { "type": "string" }, "kind": { "type": "string", "enum": ["table", "view", "matview", "sequence", "routine", "key"], "description": "default: table" } })),
                &["data_source", "name"],
                true,
            ),
            tool(
                "search_objects",
                "Fuzzy search database objects by name (Go to Database Object). Searches the default database of each data source, or the given one.",
                json!({
                    "query": { "type": "string", "description": "Name or fragment, e.g. 'usr_pay' or 'public.orders'" },
                    "data_source": { "type": "string", "description": "Limit to one data source (default: all)" },
                    "database": { "type": "string" },
                    "limit": { "type": "integer", "description": "default 30" }
                }),
                &["query"],
                true,
            ),
            tool(
                "query",
                "Run a read-only query and return rows. PostgreSQL/MySQL: a single SELECT/WITH/SHOW/EXPLAIN statement executed in a READ ONLY transaction. Redis: a read command line (e.g. 'HGETALL user:1', 'SCAN 0 MATCH user:* COUNT 100').",
                ds_props(json!({ "sql": { "type": "string" }, "limit": { "type": "integer", "description": "max rows, default 100, max 1000" } })),
                &["data_source", "sql"],
                true,
            ),
            tool(
                "explain",
                "Show the execution plan of a statement (PostgreSQL: EXPLAIN (FORMAT TEXT); MySQL: EXPLAIN).",
                ds_props(json!({ "sql": { "type": "string" }, "analyze": { "type": "boolean", "description": "EXPLAIN ANALYZE (runs the query inside a read-only transaction)" } })),
                &["data_source", "sql"],
                true,
            ),
            tool(
                "full_text_search",
                "Find rows containing a value across all tables of the given schemas (DataGrip Full-text Search).",
                ds_props(json!({
                    "text": { "type": "string" },
                    "schemas": { "type": "array", "items": { "type": "string" }, "description": "default: all user schemas" },
                    "mode": { "type": "string", "enum": ["contains", "starts", "ends", "matches", "like"], "description": "default contains" },
                    "match_case": { "type": "boolean" },
                    "columns": { "type": "string", "enum": ["text", "numeric", "all"], "description": "default text" },
                    "limit_per_table": { "type": "integer", "description": "default 10" }
                })),
                &["data_source", "text"],
                true,
            ),
            tool(
                "redis_scan",
                "Scan Redis keys with a pattern and get their type and TTL (ms, -1 = no expiry).",
                ds_props(json!({ "pattern": { "type": "string", "description": "default *" }, "type": { "type": "string", "enum": ["string", "hash", "list", "set", "zset", "stream"] }, "count": { "type": "integer", "description": "default 200" }, "cursor": { "type": "string" } })),
                &["data_source"],
                true,
            ),
            tool(
                "redis_get",
                "Get a Redis key's type, TTL, size and value (hash fields, list items, set/zset members, stream entries).",
                ds_props(json!({ "key": { "type": "string" }, "limit": { "type": "integer", "description": "max elements, default 200" } })),
                &["data_source", "key"],
                true,
            ),
            tool(
                "export_csv",
                "Run a read-only query and save the full result as a CSV file (UTF-8 with BOM, opens in Excel) in ~/Downloads/qurry-exports. Existing files are never overwritten.",
                ds_props(json!({ "sql": { "type": "string" }, "file_name": { "type": "string", "description": "Plain file name such as orders.csv (no folders)" } })),
                &["data_source", "sql", "file_name"],
                false,
            ),
            tool(
                "list_rag_indexes",
                "List RAG indexes made in the Qurry app: a table whose text columns were embedded into pgvector or a vector DB, searchable with semantic_search.",
                json!({}),
                &[],
                true,
            ),
            tool(
                "semantic_search",
                "Find the table rows closest in meaning to a question (RAG). Returns score (1 = same meaning) and the source rows, best first. The question is embedded with the index's provider (Ollama locally, or OpenAI).",
                json!({
                    "index": { "type": "string", "description": "Index name or id (see list_rag_indexes)" },
                    "query": { "type": "string", "description": "The question in plain words" },
                    "limit": { "type": "integer", "description": "default 10, max 50" }
                }),
                &["index", "query"],
                true,
            ),
        ];
        if self.allow_write {
            t.push(tool(
                "execute",
                "Execute statements that modify data or schema (INSERT/UPDATE/DELETE/DDL; Redis write commands). Statements run in one transaction (all or nothing). Rejected for read-only data sources.",
                ds_props(json!({ "statements": { "type": "array", "items": { "type": "string" } } })),
                &["data_source", "statements"],
                false,
            ));
        }
        t
    }

    /// 이름 또는 id → (id, 종류, 기본 DB, read-only)
    async fn resolve(&self, a: &Value) -> Result<(String, DbKind, String, bool), String> {
        let want = a.get("data_source").and_then(|v| v.as_str()).ok_or("data_source is required")?;
        let list = self.state.list_connections().await.map_err(|e| e.to_string())?;
        let c = list
            .iter()
            .find(|c| c.config.id == want)
            .or_else(|| list.iter().find(|c| c.config.name.eq_ignore_ascii_case(want)))
            .or_else(|| {
                let m: Vec<_> = list.iter().filter(|c| c.config.name.to_lowercase().contains(&want.to_lowercase())).collect();
                if m.len() == 1 { Some(m[0]) } else { None }
            })
            .ok_or_else(|| format!("Data source not found: {want}. Available: {}", list.iter().map(|c| c.config.name.as_str()).collect::<Vec<_>>().join(", ")))?;
        Ok((c.config.id.clone(), c.config.kind, sql::default_db(&c.config), c.config.read_only))
    }

    async fn target(&self, a: &Value) -> Result<(String, DbKind, String, bool), String> {
        let (id, kind, def, ro) = self.resolve(a).await?;
        let db = a.get("database").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from).unwrap_or(def);
        Ok((sql::target_id(&id, Some(&db)), kind, db, ro))
    }

    async fn call(&self, name: &str, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let schema_or = |kind: DbKind, db: &str| s("schema").unwrap_or_else(|| if kind == DbKind::Postgres { "public".into() } else { db.to_string() });
        match name {
            "list_data_sources" => {
                let list = st.list_connections().await.map_err(e)?;
                Ok(Value::Array(
                    list.iter()
                        .map(|c| {
                            json!({
                                "id": c.config.id, "name": c.config.name, "kind": c.config.kind,
                                "host": c.config.host, "port": c.config.port, "user": c.config.user,
                                "default_database": sql::default_db(&c.config),
                                "read_only": c.config.read_only, "ssh_tunnel": c.config.active_ssh().map(|s| format!("{}@{}", s.user, s.host)),
                                "connected": c.connected
                            })
                        })
                        .collect(),
                ))
            }
            "list_databases" => {
                let (id, ..) = self.resolve(a).await?;
                Ok(json!(st.list_databases(id).await.map_err(e)?))
            }
            "list_schemas" => {
                let (t, ..) = self.target(a).await?;
                let v = st.list_schemas(t).await.map_err(e)?;
                Ok(json!(v.iter().map(|s| json!({ "name": s.name, "system": s.is_system })).collect::<Vec<_>>()))
            }
            "list_objects" => {
                let (t, kind, db, _) = self.target(a).await?;
                let o = st.list_objects(t, schema_or(kind, &db)).await.map_err(e)?;
                Ok(json!({ "relations": o.relations, "routines": o.routines }))
            }
            "describe_table" => {
                let (t, kind, db, _) = self.target(a).await?;
                let d = st.table_detail(t, schema_or(kind, &db), s("table").ok_or("table is required")?).await.map_err(e)?;
                Ok(json!(d))
            }
            "get_ddl" => {
                let (t, kind, db, _) = self.target(a).await?;
                let k = s("kind").unwrap_or_else(|| if kind == DbKind::Redis { "key".into() } else { "table".into() });
                Ok(Value::String(st.object_ddl(t, schema_or(kind, &db), s("name").ok_or("name is required")?, k).await.map_err(e)?))
            }
            "search_objects" => {
                let q = s("query").ok_or("query is required")?;
                let limit = n("limit", 30);
                let list = st.list_connections().await.map_err(e)?;
                let only = if a.get("data_source").is_some() { Some(self.resolve(a).await?.0) } else { None };
                let mut hits: Vec<(i64, Value)> = vec![];
                let mut errors = vec![];
                for c in list.iter().filter(|c| only.as_ref().map(|o| o == &c.config.id).unwrap_or(true)) {
                    let def = sql::default_db(&c.config);
                    // 찾을 DB: 지정한 DB, 아니면 앱의 Schemas 탭에서 보이게 한 DB 들 (최대 8개)
                    let dbs: Vec<String> = match s("database").filter(|_| only.is_some()) {
                        Some(d) => vec![d],
                        None if c.config.kind == DbKind::Redis || c.config.options.single_db => vec![def.clone()],
                        None => match st.list_databases(c.config.id.clone()).await {
                            Ok(all) => {
                                let v: Vec<String> = all.into_iter().filter(|d| c.config.scope.db_visible(d, &def)).filter(|d| !["template0", "template1", "rdsadmin", "information_schema", "performance_schema", "mysql", "sys"].contains(&d.as_str())).take(8).collect();
                                if v.is_empty() { vec![def.clone()] } else { v }
                            }
                            Err(err) => {
                                errors.push(format!("{}: {err}", c.config.name));
                                continue;
                            }
                        },
                    };
                    for db in dbs {
                    match st.object_index(sql::target_id(&c.config.id, Some(&db))).await {
                        Ok(es) => {
                            for x in es {
                                let full = format!("{}.{}", x.schema, x.name);
                                let sc = fuzzy(&q, &x.name).max(if q.contains('.') { fuzzy(&q, &full) } else { None });
                                if let Some(sc) = sc {
                                    hits.push((sc, json!({ "data_source": c.config.name, "database": db, "schema": x.schema, "name": x.name, "kind": x.kind })));
                                }
                            }
                        }
                        Err(err) => errors.push(format!("{}/{db}: {err}", c.config.name)),
                    }
                    }
                }
                hits.sort_by(|a, b| b.0.cmp(&a.0));
                Ok(json!({ "matches": hits.into_iter().take(limit).map(|h| h.1).collect::<Vec<_>>(), "errors": errors }))
            }
            "query" => {
                let (t, ..) = self.target(a).await?;
                let limit = n("limit", 100).clamp(1, MAX_ROWS);
                let r = st.run_read_only(t, s("sql").ok_or("sql is required")?, limit).await.map_err(e)?;
                Ok(result_json(&r))
            }
            "explain" => {
                let (t, kind, ..) = self.target(a).await?;
                let q = s("sql").ok_or("sql is required")?;
                let q = q.trim().trim_end_matches(';');
                let analyze = a.get("analyze").and_then(|v| v.as_bool()).unwrap_or(false);
                let sqlq = match kind {
                    DbKind::Postgres => format!("EXPLAIN ({}FORMAT TEXT) {q}", if analyze { "ANALYZE, BUFFERS, " } else { "" }),
                    DbKind::Mysql => format!("EXPLAIN {}{q}", if analyze { "ANALYZE " } else { "" }),
                    DbKind::Redis => return Err("EXPLAIN is not available for Redis".into()),
                    _ => format!("EXPLAIN {q}"),
                };
                let r = st.run_read_only(t, sqlq, MAX_ROWS).await.map_err(e)?;
                if kind == DbKind::Postgres {
                    Ok(Value::String(r.rows.iter().filter_map(|row| row.first().and_then(|v| v.as_str())).collect::<Vec<_>>().join("\n")))
                } else {
                    Ok(result_json(&r))
                }
            }
            "full_text_search" => {
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
            "redis_scan" => {
                let (t, kind, ..) = self.target(a).await?;
                if kind != DbKind::Redis {
                    return Err("Not a Redis data source".into());
                }
                let p = st
                    .redis_scan(t, s("cursor").unwrap_or_else(|| "0".into()), s("pattern").unwrap_or_else(|| "*".into()), n("count", 200).clamp(1, 5000), s("type"))
                    .await
                    .map_err(e)?;
                Ok(json!({ "next_cursor": p.cursor, "keys": p.keys }))
            }
            "redis_get" => {
                let (t, kind, ..) = self.target(a).await?;
                if kind != DbKind::Redis {
                    return Err("Not a Redis data source".into());
                }
                Ok(json!(st.redis_key(t, s("key").ok_or("key is required")?, Some(n("limit", 200).clamp(1, 10000))).await.map_err(e)?))
            }
            "export_csv" => {
                let (t, ..) = self.target(a).await?;
                let q = s("sql").ok_or("sql is required")?;
                // 정해진 폴더에만, 새 파일로 (남이 정한 경로에 쓰지 않는다)
                let path = qurry_core::export_path(&s("file_name").ok_or("file_name is required")?).map_err(e)?;
                let rows = st.export_csv_read_only(t, q, path.clone()).await.map_err(e)?;
                Ok(json!({ "rows": rows, "path": path.display().to_string() }))
            }
            "execute" if self.allow_write => {
                let (t, _, _, ro) = self.target(a).await?;
                if ro {
                    return Err("This data source is marked read-only in Qurry".into());
                }
                let stmts: Vec<String> = a.get("statements").and_then(|v| v.as_array()).ok_or("statements is required")?.iter().filter_map(|x| x.as_str().map(String::from)).collect();
                if stmts.is_empty() {
                    return Err("statements is empty".into());
                }
                let n = st.execute_batch(t, stmts.clone(), false).await.map_err(e)?;
                Ok(json!({ "statements": stmts.len(), "rows_affected": n }))
            }
            "list_rag_indexes" => {
                let names: std::collections::HashMap<String, String> = st.list_connections().await.map_err(e)?.into_iter().map(|c| (c.config.id, c.config.name)).collect();
                let name_of = |t: &str| {
                    let (id, db) = qurry_core::app::split_target(t);
                    let n = names.get(id).cloned().unwrap_or_else(|| id.to_string());
                    db.map(|d| format!("{n} / {d}")).unwrap_or(n)
                };
                Ok(json!(st.rag_indexes().map_err(e)?.iter().map(|x| json!({
                    "name": x.name, "id": x.id, "source": name_of(&x.source), "table": format!("{}.{}", x.schema, x.table),
                    "text_columns": x.text_columns, "store": name_of(&x.dest), "collection": x.collection,
                    "embeddings": format!("{} {}", x.provider, x.model), "rows": x.rows, "built_at": x.built_at
                })).collect::<Vec<_>>()))
            }
            "semantic_search" => {
                let index = s("index").ok_or("index is required")?;
                let query = s("query").ok_or("query is required")?;
                Ok(result_json(&st.rag_search(index, query, n("limit", 10).min(50)).await.map_err(e)?))
            }
            other => Err(format!("Unknown tool: {other}")),
        }
    }
}

fn result_json(r: &QueryResult) -> Value {
    if !r.has_result_set {
        return json!({ "rows_affected": r.rows_affected });
    }
    json!({
        "columns": r.columns.iter().map(|c| json!({ "name": c.name, "type": c.type_name })).collect::<Vec<_>>(),
        "rows": r.rows,
        "row_count": r.rows.len(),
        "truncated": r.truncated,
        "elapsed_ms": r.elapsed_ms
    })
}

fn fuzzy(q: &str, text: &str) -> Option<i64> {
    let (q, t) = (q.to_lowercase(), text.to_lowercase());
    if q.is_empty() {
        return Some(0);
    }
    if t == q {
        return Some(1000);
    }
    if t.starts_with(&q) {
        return Some(800 - t.len() as i64);
    }
    if let Some(i) = t.find(&q) {
        return Some(500 - i as i64 - t.len() as i64 / 10);
    }
    let mut it = t.chars();
    let mut score = 200i64;
    for qc in q.chars().filter(|c| !c.is_whitespace()) {
        let mut skipped = 0;
        loop {
            match it.next() {
                Some(c) if c == qc => break,
                Some(_) => skipped += 1,
                None => return None,
            }
        }
        score -= skipped.min(10);
    }
    Some(score - t.len() as i64 / 10)
}

async fn handle(server: Arc<Server>, msg: Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let id = msg.get("id").cloned()?; // 알림(notification)에는 응답하지 않는다
    let result: Result<Value, (i64, String)> = match method.as_str() {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOLS[0]);
            let ver = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
            Ok(json!({
                "protocolVersion": ver,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                "instructions": format!(
                    "Qurry: database tools for the data sources configured in the Qurry app (PostgreSQL, MySQL, Redis and driver databases; SSH tunnels supported). \
                     Start with list_data_sources, then list_objects / describe_table / search_objects, and query for data. \
                     To find rows by meaning, use list_rag_indexes and semantic_search. {}",
                    if server.allow_write { "Write access is enabled via the execute tool." } else { "The server is read-only: query runs one statement in a READ ONLY transaction." }
                )
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": server.tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match server.call(&name, &args).await {
                Ok(v) => {
                    let text = match v {
                        Value::String(s) => s,
                        other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                    };
                    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
                }
                Err(err) => json!({ "content": [{ "type": "text", "text": err }], "isError": true }),
            })
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err((-32601, format!("Method not found: {other}"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, m)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": m } }),
    })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("qurry-mcp [--allow-write] [--config-dir DIR] [--verbose]\n\nMCP server (stdio) exposing the Qurry data sources. Read-only unless --allow-write.\n\nClaude Code:  claude mcp add qurry -- {}", args[0]);
        return Ok(());
    }
    let dir = args.iter().position(|a| a == "--config-dir").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    // stdout 은 프로토콜 전용. 엔진 로그(stderr)는 --verbose 일 때만
    qurry_core::QUIET.store(!args.iter().any(|a| a == "--verbose"), std::sync::atomic::Ordering::Relaxed);
    let state = Arc::new(qurry_core::open_state(dir).map_err(|e| std::io::Error::other(e.to_string()))?);
    let server = Arc::new(Server { state: state.clone(), allow_write: args.iter().any(|a| a == "--allow-write") });
    // keep-alive / auto-disconnect
    {
        let st = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                qurry_core::app::housekeeping(&st).await;
            }
        });
    }
    let out = Arc::new(Mutex::new(tokio::io::stdout()));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut tasks = tokio::task::JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let server = server.clone();
        let out = out.clone();
        // 요청마다 따로 처리 (긴 쿼리가 다른 요청을 막지 않게)
        tasks.spawn(async move {
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(Value::Array(batch)) => {
                    let mut v = vec![];
                    for m in batch {
                        if let Some(r) = handle(server.clone(), m).await {
                            v.push(r);
                        }
                    }
                    if v.is_empty() { None } else { Some(Value::Array(v)) }
                }
                Ok(m) => handle(server, m).await,
                Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("Parse error: {e}") } })),
            };
            if let Some(r) = reply {
                let mut o = out.lock().await;
                let _ = o.write_all(format!("{r}\n").as_bytes()).await;
                let _ = o.flush().await;
            }
        });
    }
    // stdin 이 닫혀도 진행 중인 응답은 마저 보낸다
    while tasks.join_next().await.is_some() {}
    Ok(())
}
