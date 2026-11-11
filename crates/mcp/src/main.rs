//! Qurry MCP server (stdio, JSON-RPC 2.0). Uses the same engine (qurry-core) and the same Data Sources as the GUI·TUI.
//!
//! Read-only by default: PG/MySQL run a single statement inside a READ ONLY transaction, Redis allows read commands only.
//! `--allow-write` adds the `execute` tool (Read-only and Production Data Sources still never take it).
//! MCP exposure is set per Data Source (Options › MCP: Off / Read / Read & write — store::McpAccess). Hidden sources are not listed and cannot be connected to.
use qurry_core::app::{AppState, ConnectionView};
use qurry_core::db::QueryResult;
use qurry_core::dba;
use qurry_core::erd::Erd;
use qurry_core::explain::explain_sql;
use qurry_core::plan::PlanNode;
use qurry_core::sql;
use qurry_core::store::{DbKind, McpAccess};
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
                "Show the execution plan of a statement (PostgreSQL: EXPLAIN (FORMAT TEXT); MySQL, DuckDB, ClickHouse: EXPLAIN; SQLite: EXPLAIN QUERY PLAN; not available for SQL Server, Oracle, MongoDB, Redis and vector databases). PostgreSQL adds a second content item, JSON {\"plan_analysis\": …}: the plan tree with each node's share of the estimated cost and warnings (Seq Scan on a big table, sorts and hashes spilled to disk …), from a separate EXPLAIN (FORMAT JSON) that never runs the statement.",
                ds_props(json!({ "sql": { "type": "string" }, "analyze": { "type": "boolean", "description": "EXPLAIN ANALYZE (runs the query inside a read-only transaction; not for SQLite or ClickHouse)" } })),
                &["data_source", "sql"],
                true,
            ),
            tool(
                "get_erd",
                "Tables with their primary keys and the foreign keys between them (no row data), as JSON and as a Mermaid erDiagram. Tables in other schemas that a foreign key points to are included. At most 200 tables.",
                ds_props(json!({ "schemas": { "type": "array", "items": { "type": "string" }, "description": "default: PostgreSQL public, MySQL and others the database" } })),
                &["data_source"],
                true,
            ),
            tool(
                "dba_report",
                &format!(
                    "Run one of Qurry's fixed Database Admin reports (read-only; no SQL from you runs). Reports: {}. Not every database kind has every report (PostgreSQL and MySQL only). String and number literals in statement text are replaced with ?, and MySQL replication errors (Last_Error, Last_SQL_Error, Last_IO_Error) from their first quote to their last; for the full text, ask the user to open the report in Qurry. Production data sources answer only if a person allowed it for that data source in Qurry (list_data_sources: dba_report). Nothing is changed: run suggested fixes (CREATE / DROP INDEX, VACUUM …) by asking the user to do it in Qurry.",
                    report_ids().join(", ")
                ),
                ds_props(json!({ "report": { "type": "string", "description": "Report id, e.g. sessions, slow_queries, table_sizes" } })),
                &["data_source", "report"],
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
                "Change data, one statement per item, from an allow-list. SQL: one INSERT, or UPDATE / DELETE with a WHERE. MongoDB: insertOne / insertMany, and updateOne / updateMany / replaceOne / deleteOne / deleteMany with a non-empty filter. Vector DBs: adding points, and deleting points by id. Redis: reads, and writes to single keys without expiry options. PostgreSQL/MySQL: all items in one transaction (all or nothing); other kinds run them one by one with no rollback. Only for data sources whose MCP access is 'Read & write' (see list_data_sources). Anything else (DDL such as CREATE / ALTER / DROP, TRUNCATE, MERGE, scripts, deletes of whole keys or collections) is refused — ask the user to run it in Qurry.",
                ds_props(json!({ "statements": { "type": "array", "items": { "type": "string" } } })),
                &["data_source", "statements"],
                false,
            ));
        }
        t
    }

    /// Only Data Sources visible to MCP
    async fn sources(&self) -> Result<Vec<qurry_core::app::ConnectionView>, String> {
        let list = self.state.list_connections().await.map_err(|e| e.to_string())?;
        Ok(list.into_iter().filter(|c| c.config.mcp_access(self.allow_write) != McpAccess::Off).collect())
    }

    /// Name or id → (id, kind, default DB, MCP access)
    async fn resolve(&self, a: &Value) -> Result<(String, DbKind, String, McpAccess), String> {
        let want = a.get("data_source").and_then(|v| v.as_str()).ok_or("data_source is required")?;
        let all = self.state.list_connections().await.map_err(|e| e.to_string())?;
        let c = find_source(&all, want, self.allow_write)?;
        Ok((c.config.id.clone(), c.config.kind, sql::default_db(&c.config), c.config.mcp_access(self.allow_write)))
    }

    async fn target(&self, a: &Value) -> Result<(String, DbKind, String, McpAccess), String> {
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
                let list = self.sources().await?;
                Ok(Value::Array(
                    list.iter()
                        .map(|c| {
                            json!({
                                "id": c.config.id, "name": c.config.name, "kind": c.config.kind,
                                "host": qurry_core::store::without_userinfo(&c.config.host), "port": c.config.port, "user": c.config.user,
                                "default_database": sql::default_db(&c.config),
                                "read_only": c.config.read_only, "production": c.config.options.confirm_changes,
                                "mcp_access": if c.config.mcp_access(self.allow_write) == McpAccess::Write { "write" } else { "read" },
                                "dba_report": c.config.mcp_dba_report_allowed() && !dba::reports(c.config.kind).is_empty(),
                                "ssh_tunnel": c.config.active_ssh().map(|s| format!("{}@{}", s.user, s.host)),
                                "ssm_tunnel": c.config.active_ssm().map(|s| s.target.trim().to_string()),
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
                // DDL of a Redis key reads the whole value: use redis_get (up to the limit) for large keys
                if kind == DbKind::Redis {
                    let key = s("name").ok_or("name is required")?;
                    let d = st.redis_key(t.clone(), key, Some(1)).await.map_err(e)?;
                    let big = if d.ty == "string" { d.length > qurry_core::redisdb::STRING_PREVIEW_MAX } else { d.length > qurry_core::guard::REDIS_STRICT_MAX };
                    if big {
                        return Err(format!("The key is too big for its DDL ({} {}) — use redis_get with a limit", d.length, if d.ty == "string" { "bytes" } else { "elements" }));
                    }
                }
                Ok(Value::String(st.object_ddl(t, schema_or(kind, &db), s("name").ok_or("name is required")?, k).await.map_err(e)?))
            }
            "search_objects" => {
                let q = s("query").ok_or("query is required")?;
                let limit = n("limit", 30);
                let list = self.sources().await?;
                let only = if a.get("data_source").is_some() { Some(self.resolve(a).await?.0) } else { None };
                let mut hits: Vec<(i64, Value)> = vec![];
                let mut errors = vec![];
                for c in list.iter().filter(|c| only.as_ref().map(|o| o == &c.config.id).unwrap_or(true)) {
                    let def = sql::default_db(&c.config);
                    // DBs to search: the given DB, otherwise the DBs made visible in the app's Schemas tab (at most 8)
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
            "get_erd" => {
                let (t, kind, db, _) = self.target(a).await?;
                if kind == DbKind::Redis {
                    return Err("An ERD is not available for Redis".into());
                }
                let schemas: Vec<String> = match a.get("schemas").and_then(|v| v.as_array()) {
                    Some(v) => v.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                    None => vec![schema_or(kind, &db)],
                };
                Ok(erd_json(st.erd(t, schemas).await.map_err(e)?, ERD_MAX_TABLES))
            }
            "dba_report" => {
                let (t, kind, ..) = self.target(a).await?;
                let report = s("report").ok_or("report is required")?;
                let id = qurry_core::app::split_target(&t).0.to_string();
                let c = self.sources().await?.into_iter().find(|c| c.config.id == id).ok_or("Data source not found")?.config;
                if !c.mcp_dba_report_allowed() {
                    return Err(format!(
                        "\"{}\" is a Production data source: MCP reads its Database Admin reports (sessions, client addresses, settings, roles) only if a person allows it for this data source in Qurry (Data Source Properties › Options › \"AI tools can read admin reports\"; TUI: \"Admin reports\" in the data source form)",
                        c.name
                    ));
                }
                let info = dba::reports(kind).into_iter().find(|r| r.id == report).ok_or_else(|| unknown_report(&report, kind))?;
                let mut r = st.dba_report(t, report).await.map_err(e)?;
                mask_query_text(&mut r, kind);
                if r.rows.len() > MAX_ROWS {
                    r.rows.truncate(MAX_ROWS);
                    r.truncated = true;
                }
                // Never the report's actions (Cancel Query, Terminate Session, generated SQL): MCP only reads
                Ok(json!({ "report": info.id, "title": info.title, "help": info.help, "result": result_json(&r) }))
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
                // Only in the designated folder, as a new file (never write to a path chosen by someone else)
                let path = qurry_core::export_path(&s("file_name").ok_or("file_name is required")?).map_err(e)?;
                let rows = st.export_csv_read_only(t, q, path.clone()).await.map_err(e)?;
                Ok(json!({ "rows": rows, "path": path.display().to_string() }))
            }
            "execute" if self.allow_write => {
                let (t, kind, _, access) = self.target(a).await?;
                if access != McpAccess::Write {
                    let id = self.resolve(a).await?.0;
                    let (ro, prod) = self.sources().await?.iter().find(|c| c.config.id == id).map_or((false, false), |c| (c.config.read_only, c.config.options.confirm_changes));
                    return Err(match (ro, prod) {
                        (true, _) => "MCP never writes to a Read-only data source; change it in Qurry".into(),
                        (_, true) => "MCP never writes to a Production data source (the app asks a person to confirm changes there); change it in Qurry".into(),
                        _ => "This data source is read-only for MCP (Qurry: data source › Options › MCP: Read & write)".into(),
                    });
                }
                let stmts: Vec<String> = a.get("statements").and_then(|v| v.as_array()).ok_or("statements is required")?.iter().filter_map(|x| x.as_str().map(String::from)).collect();
                if stmts.is_empty() {
                    return Err("statements is empty".into());
                }
                // Allow-list (guard::mcp_execute_violation): INSERT, UPDATE / DELETE with WHERE and their MongoDB / vector DB / Redis equivalents
                for s in &stmts {
                    if let Some(why) = qurry_core::guard::mcp_execute_violation(s, kind) {
                        return Err(format!("{why}: {s}"));
                    }
                }
                // PostgreSQL / MySQL: each item as a prepared statement, so the server itself refuses more than one statement
                let r = st.execute_batch_prepared(t, stmts.clone()).await.map_err(e)?;
                let mut out = json!({ "statements": stmts.len(), "rows_affected": r.rows });
                // Committed, but the Production log couldn't record the result: say so instead of failing (a retry would run it twice)
                if let Some(w) = r.warning {
                    out["warning"] = w.into();
                }
                Ok(out)
            }
            "list_rag_indexes" => {
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
            "semantic_search" => {
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
            other => Err(format!("Unknown tool: {other}")),
        }
    }

    /// A tool's answer as MCP content items: one for every tool, except explain on PostgreSQL (the plan, then its analysis)
    async fn contents(&self, name: &str, a: &Value) -> Result<Vec<Value>, String> {
        match name {
            "explain" => self.explain(a).await,
            _ => Ok(vec![self.call(name, a).await?]),
        }
    }

    /// The plan as before (PostgreSQL: the FORMAT TEXT lines, others: the rows). PostgreSQL also gets {"plan_analysis": …} from a separate
    /// EXPLAIN (FORMAT JSON) without ANALYZE, so with analyze the statement still runs once; left out if that run or its parse fails
    async fn explain(&self, a: &Value) -> Result<Vec<Value>, String> {
        let st = &self.state;
        let (t, kind, ..) = self.target(a).await?;
        let q = a.get("sql").and_then(|v| v.as_str()).ok_or("sql is required")?;
        let q = q.trim();
        let analyze = a.get("analyze").and_then(|v| v.as_bool()).unwrap_or(false);
        // The consoles' statement (qurry_shared::explain), as TEXT: a kind without one says so and nothing runs
        let sqlq = explain_sql(q, kind, analyze, false)?;
        let r = st.run_read_only(t.clone(), sqlq, MAX_ROWS).await.map_err(|e| e.to_string())?;
        if kind != DbKind::Postgres {
            return Ok(vec![result_json(&r)]);
        }
        let text = Value::String(r.rows.iter().filter_map(|row| row.first().and_then(|v| v.as_str())).collect::<Vec<_>>().join("\n"));
        let plan = st.run_read_only(t, explain_sql(q, kind, false, true)?, 1).await.ok().and_then(|r| {
            let cell = r.rows.first()?.first()?.clone();
            qurry_core::plan::parse_pg(&cell.as_str().map(String::from).unwrap_or_else(|| cell.to_string())).ok()
        });
        Ok(match plan {
            Some(p) => vec![text, plan_analysis(&p)],
            None => vec![text],
        })
    }
}

/// get_erd: at most this many tables
const ERD_MAX_TABLES: usize = 200;

/// Every report id of every kind, in the reports' order
fn report_ids() -> Vec<&'static str> {
    let mut ids: Vec<&str> = dba::reports(DbKind::Postgres).iter().map(|r| r.id).collect();
    for r in dba::reports(DbKind::Mysql) {
        if !ids.contains(&r.id) {
            ids.push(r.id);
        }
    }
    ids
}

fn unknown_report(report: &str, kind: DbKind) -> String {
    let ids: Vec<&str> = dba::reports(kind).iter().map(|r| r.id).collect();
    if ids.is_empty() {
        return "Database Admin reports are available for PostgreSQL and MySQL data sources only".into();
    }
    format!("No report \"{report}\" for this data source. Reports: {}", ids.join(", "))
}

/// Statement text in a report (dba::QUERY_TEXT_COLUMNS) with its literals replaced by ?, and server messages (dba::ERROR_TEXT_COLUMNS)
/// masked from their first quote to their last: sessions, slow statements and replication errors can hold row values.
/// Names match in any case (SHOW REPLICA STATUS names its columns Last_SQL_Error …)
fn mask_query_text(r: &mut QueryResult, kind: DbKind) {
    let of = |list: &[&str]| -> Vec<usize> { r.columns.iter().enumerate().filter(|(_, c)| list.iter().any(|q| q.eq_ignore_ascii_case(&c.name))).map(|(i, _)| i).collect() };
    let (query, error) = (of(dba::QUERY_TEXT_COLUMNS), of(dba::ERROR_TEXT_COLUMNS));
    for row in &mut r.rows {
        for &i in &query {
            if let Some(Value::String(s)) = row.get_mut(i) {
                *s = sql::mask_literals(s, kind);
            }
        }
        for &i in &error {
            if let Some(Value::String(s)) = row.get_mut(i) {
                *s = mask_quoted(s);
            }
        }
    }
}

/// Everything from the first quote to the last as one ?: MySQL quotes values inside a quoted message without escaping
/// ('Duplicate entry 'x' for key 'k''), so no quote can be trusted to end a value. The words around it stay
fn mask_quoted(s: &str) -> String {
    let is_quote = |c: char| c == '\'' || c == '"';
    match (s.find(is_quote), s.rfind(is_quote)) {
        // A lone quote opens a value that runs to the end
        (Some(a), Some(b)) => format!("{}?{}", &s[..a], if a == b { "" } else { &s[b + 1..] }),
        _ => s.to_string(),
    }
}

/// get_erd's answer: the first `cap` tables (in AppState::erd's order), the foreign keys whose both ends are kept, and Mermaid of that
fn erd_json(mut e: Erd, cap: usize) -> Value {
    let total = e.tables.len();
    e.tables.truncate(cap);
    let kept = |schema: &str, name: &str| e.tables.iter().any(|t| t.schema == schema && t.name == name);
    let relations: Vec<_> = e.relations.iter().filter(|r| kept(&r.from_schema, &r.from_table) && kept(&r.to_schema, &r.to_table)).cloned().collect();
    e.relations = relations;
    let mut out = json!({
        "mermaid": e.mermaid(),
        "tables": e.tables.iter().map(|t| json!({ "schema": t.schema, "name": t.name, "primary_key": t.columns.iter().filter(|c| c.pk).map(|c| &c.name).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "foreign_keys": e.relations.iter().map(|r| json!({
            "name": r.name, "from_schema": r.from_schema, "from_table": r.from_table, "from_columns": r.from_columns,
            "to_schema": r.to_schema, "to_table": r.to_table, "to_columns": r.to_columns
        })).collect::<Vec<_>>(),
    });
    if total > cap {
        out["note"] = format!("Showing {cap} of {total} tables; pass fewer schemas").into();
    }
    out
}

/// explain's second content item. Shares are by time only when the JSON plan has actual times, which it never does here (no ANALYZE)
fn plan_analysis(p: &PlanNode) -> Value {
    fn node(n: &PlanNode) -> Value {
        let mut v = json!({ "op": n.op, "detail": n.detail, "rows": n.rows, "pct": (n.pct * 10.0).round() / 10.0, "self_cost": n.self_cost.map(|c| (c * 100.0).round() / 100.0) });
        if let Some(t) = n.self_time {
            v["self_time_ms"] = t.into();
        }
        if !n.warnings.is_empty() {
            v["warnings"] = json!(n.warnings);
        }
        if !n.children.is_empty() {
            v["children"] = n.children.iter().map(node).collect();
        }
        v
    }
    json!({ "plan_analysis": { "basis": if p.actual_time.is_some() { "time" } else { "cost" }, "plan": node(p) } })
}

/// The Data Source an MCP client asked for: id, name (any case) or a unique part of the name, among the ones shared with MCP.
/// A hidden source that matches by id or name gets its own error (the user turned it off — it isn't missing), and nothing else about it is shown
fn find_source<'a>(all: &'a [ConnectionView], want: &str, allow_write: bool) -> Result<&'a ConnectionView, String> {
    let shown = |c: &ConnectionView| c.config.mcp_access(allow_write) != McpAccess::Off;
    let list: Vec<&ConnectionView> = all.iter().filter(|c| shown(c)).collect();
    let found = list
        .iter()
        .find(|c| c.config.id == want)
        .or_else(|| list.iter().find(|c| c.config.name.eq_ignore_ascii_case(want)))
        .or_else(|| {
            let m: Vec<_> = list.iter().filter(|c| c.config.name.to_lowercase().contains(&want.to_lowercase())).collect();
            if m.len() == 1 { Some(m[0]) } else { None }
        });
    if let Some(c) = found {
        return Ok(c);
    }
    if all.iter().any(|c| !shown(c) && (c.config.id == want || c.config.name.eq_ignore_ascii_case(want))) {
        return Err(format!("Data source \"{want}\" is not shared with MCP — turn it on in Qurry: data source › Options › MCP (TUI: data source form)"));
    }
    Err(format!("Data source not found: {want}. Available: {}", list.iter().map(|c| c.config.name.as_str()).collect::<Vec<_>>().join(", ")))
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

/// search_objects ranking: the TUI palette's score (qurry_shared::fuzzy); None = no match
fn fuzzy(q: &str, text: &str) -> Option<i64> {
    qurry_core::fuzzy::score(q, text, Default::default()).map(|s| s as i64)
}

async fn handle(server: Arc<Server>, msg: Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let id = msg.get("id").cloned()?; // Notifications get no response
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
            Ok(match server.contents(&name, &args).await {
                Ok(items) => {
                    let text = |v: Value| match v {
                        Value::String(s) => s,
                        other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                    };
                    json!({ "content": items.into_iter().map(|v| json!({ "type": "text", "text": text(v) })).collect::<Vec<_>>(), "isError": false })
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
    // stdout is for the protocol only. Engine logs (stderr) only with --verbose
    qurry_core::QUIET.store(!args.iter().any(|a| a == "--verbose"), std::sync::atomic::Ordering::Relaxed);
    qurry_core::SANDBOX.store(true, std::sync::atomic::Ordering::Relaxed);
    qurry_core::logfile::init("mcp", dir.as_deref());
    let mut state = qurry_core::open_state(dir).map_err(|e| std::io::Error::other(e.to_string()))?;
    state.mcp = true;
    // AWS SSM tunnels run the aws CLI: stop it when the host stops this server or it panics
    qurry_core::ssm::kill_on_panic();
    tokio::spawn(qurry_core::ssm::kill_on_signals());
    let state = Arc::new(state);
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
        // Handle each request separately (so a long query doesn't block other requests)
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
    // Even if stdin closes, finish sending in-flight responses
    while tasks.join_next().await.is_some() {}
    qurry_core::ssm::kill_all();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, name: &str, mcp: Option<&str>, production: bool) -> ConnectionView {
        let mut config: qurry_core::store::ConnectionConfig =
            serde_json::from_value(json!({ "id": id, "name": name, "kind": "postgres", "host": "db.internal", "port": 5432, "user": "admin" })).unwrap();
        config.options.mcp = mcp.map(String::from);
        config.options.confirm_changes = production;
        ConnectionView { config, has_password: true, connected: false, temporary: false }
    }

    #[test]
    fn hidden_sources_say_they_are_not_shared() {
        let all = [source("1", "Dev", Some("read"), false), source("2", "Prod", None, true), source("3", "Analytics", Some("off"), false)];
        let find = |w: &str| find_source(&all, w, false).map(|c| c.config.id.clone());
        assert_eq!(find("dev").unwrap(), "1");
        assert_eq!(find("1").unwrap(), "1");
        // An old Production source (no MCP setting) and a source set to Off: by name (any case) or id
        for w in ["Prod", "prod", "2", "Analytics", "3"] {
            let e = find(w).unwrap_err();
            assert!(e.contains("is not shared with MCP") && e.contains("Options › MCP"), "{w}: {e}");
            assert!(!e.contains("db.internal") && !e.contains("admin"), "{e}");
        }
        // A part of a hidden name doesn't match it, and hidden sources aren't listed as available
        let e = find("Analy").unwrap_err();
        assert!(e.starts_with("Data source not found") && !e.contains("Analytics") && !e.contains("Prod"), "{e}");
    }

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

    /// A source behind AWS SSM, through the core tests' fake aws (crates/core/tests/fake_aws.py) on PATH. Its session stops
    /// forwarding while the CLI runs on (sleep, a network change, the AWS side): the next query opens the tunnel again and answers,
    /// without restarting the server.
    /// Run on its own (it changes PATH): QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-mcp query_recovers -- --ignored
    #[cfg(unix)]
    #[tokio::test]
    #[ignore]
    async fn query_recovers_from_a_dead_ssm_tunnel() {
        use std::os::unix::fs::PermissionsExt;
        let pg = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG=host:port:user:db");
        let pg: Vec<&str> = pg.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-mcp-ssm-{}-{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let python = std::env::split_paths(&path).map(|d| d.join("python3")).find(|p| p.is_file()).expect("python3 on PATH");
        let fake = include_str!("../../core/tests/fake_aws.py").replace("@MODE@", "ok").replacen("/usr/bin/env python3", &python.to_string_lossy(), 1);
        std::fs::write(bin.join("aws"), fake).unwrap();
        std::fs::set_permissions(bin.join("aws"), std::fs::Permissions::from_mode(0o755)).unwrap();
        unsafe { std::env::set_var("PATH", std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(&path))).unwrap()) };

        let mut state = qurry_core::open_state(Some(dir.join("config"))).unwrap();
        state.mcp = true;
        let mut c = source("ssm", "Behind SSM", Some("read"), false).config;
        (c.host, c.port, c.user, c.database) = (pg[0].into(), pg[1].parse().unwrap(), pg[2].into(), pg[3].into());
        c.ssm = Some(qurry_core::store::SsmConfig { enabled: true, profile: "dev".into(), region: "ap-northeast-2".into(), target: "i-0123456789abcdef0".into() });
        state.store.save(&c).unwrap();
        let server = Server { state: Arc::new(state), allow_write: false };
        let q = json!({ "data_source": "Behind SSM", "sql": "select 1 as one" });
        let starts = || std::fs::read_to_string(bin.join("starts")).unwrap().lines().count();
        assert_eq!(server.call("query", &q).await.expect("connects")["rows"][0][0], json!(1));
        assert_eq!(starts(), 1);

        // The session dies; the CLI and its plugin run on and accept connections (the fake's dead-<plugin pid>)
        let pids = std::fs::read_to_string(bin.join("pids")).unwrap();
        std::fs::write(bin.join(format!("dead-{}", pids.split_whitespace().nth(1).unwrap())), "").unwrap();
        let t = std::time::Instant::now();
        let r = server.call("query", &q).await.expect("the same server answers again");
        assert_eq!(r["rows"][0][0], json!(1));
        assert!(t.elapsed() < std::time::Duration::from_secs(14), "one timeout, a new session and a connect: {:?}", t.elapsed());
        assert_eq!(starts(), 2, "a new session");
        server.state.disconnect("ssm".into()).await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    fn server_with(sources: &[ConnectionView]) -> (Server, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("qurry-mcp-test-{}-{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()));
        let state = qurry_core::open_state(Some(dir.clone())).unwrap();
        for s in sources {
            state.store.save(&s.config).unwrap();
        }
        (Server { state: Arc::new(state), allow_write: false }, dir)
    }

    #[test]
    fn erd_keeps_the_first_tables_and_their_relations() {
        use qurry_core::erd::{ErdColumn, ErdRelation, ErdTable};
        let table = |i: usize| ErdTable { schema: "public".into(), name: format!("t{i:03}"), columns: vec![ErdColumn { name: "id".into(), data_type: "int".into(), pk: true, nullable: false }] };
        let fk = |from: usize, to: usize| ErdRelation {
            name: format!("fk_{from}_{to}"), from_schema: "public".into(), from_table: format!("t{from:03}"), from_columns: vec!["id".into()],
            to_schema: "public".into(), to_table: format!("t{to:03}"), to_columns: vec!["id".into()],
        };
        let big = Erd { tables: (0..250).map(table).collect(), relations: vec![fk(1, 0), fk(210, 0), fk(5, 220)] };
        let v = erd_json(big, 200);
        assert_eq!(v["tables"].as_array().unwrap().len(), 200);
        assert_eq!(v["tables"][199]["name"], "t199");
        assert_eq!(v["tables"][0]["primary_key"], json!(["id"]));
        assert_eq!(v["foreign_keys"], json!([{ "name": "fk_1_0", "from_schema": "public", "from_table": "t001", "from_columns": ["id"], "to_schema": "public", "to_table": "t000", "to_columns": ["id"] }]));
        assert_eq!(v["note"], "Showing 200 of 250 tables; pass fewer schemas");
        let m = v["mermaid"].as_str().unwrap();
        assert!(m.contains("t000 ||--o{ t001") && !m.contains("t210") && !m.contains("t220"), "{m}");
        let small = erd_json(Erd { tables: (0..3).map(table).collect(), relations: vec![fk(1, 0)] }, 200);
        assert!(small.get("note").is_none() && small["foreign_keys"].as_array().unwrap().len() == 1, "{small}");
    }

    /// Production without the per-source setting, and a source not shared with MCP, get no report; an unknown id lists the ones there are
    #[tokio::test]
    async fn dba_report_gates() {
        let mut allowed = source("4", "Prod allowed", Some("read"), true);
        allowed.config.options.mcp_dba_reports = true;
        let (server, dir) = server_with(&[source("1", "Dev", Some("read"), false), source("2", "Prod", Some("read"), true), source("3", "Hidden", Some("off"), false), allowed]);
        let report = |ds: &str, r: &str| json!({ "data_source": ds, "report": r });
        let e = server.call("dba_report", &report("Prod", "sessions")).await.unwrap_err();
        assert!(e.contains("Production") && e.contains("only if a person allows it for this data source in Qurry"), "{e}");
        let e = server.call("dba_report", &report("Hidden", "sessions")).await.unwrap_err();
        assert!(e.contains("is not shared with MCP") && !e.contains("db.internal"), "{e}");
        for ds in ["Dev", "Prod allowed"] {
            let e = server.call("dba_report", &report(ds, "drop_everything")).await.unwrap_err();
            assert!(e.starts_with("No report \"drop_everything\"") && e.contains("sessions, locks, slow_queries") && e.contains("vacuum_progress"), "{e}");
            // PostgreSQL has no fragmentation (a MySQL report)
            let e = server.call("dba_report", &report(ds, "fragmentation")).await.unwrap_err();
            assert!(e.starts_with("No report \"fragmentation\""), "{e}");
        }
        let list = server.call("list_data_sources", &json!({})).await.unwrap();
        let flag = |name: &str| list.as_array().unwrap().iter().find(|c| c["name"] == name).map(|c| c["dba_report"].clone());
        assert_eq!((flag("Dev"), flag("Prod"), flag("Prod allowed"), flag("Hidden")), (Some(json!(true)), Some(json!(false)), Some(json!(true)), None));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn new_tools_are_listed_read_only() {
        let (server, dir) = server_with(&[]);
        let r = handle(Arc::new(server), json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await.unwrap();
        let tools = r["result"]["tools"].as_array().unwrap();
        for name in ["get_erd", "dba_report", "explain"] {
            let t = tools.iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("{name} listed"));
            assert_eq!(t["annotations"]["readOnlyHint"], json!(true), "{name}");
        }
        let d = tools.iter().find(|t| t["name"] == "dba_report").unwrap()["description"].as_str().unwrap().to_string();
        for id in ["sessions", "locks", "slow_queries", "table_sizes", "table_bloat", "index_bloat", "fragmentation", "unused_indexes", "duplicate_indexes", "fk_without_index", "vacuum", "vacuum_progress", "wraparound", "replication", "server", "settings", "roles", "grants"] {
            assert!(d.contains(id), "{id}: {d}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn report_query_text_is_masked_by_column() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "TEXT".into() };
        let mut r = QueryResult {
            columns: vec![col("user"), col("blocking_query"), col("query")],
            rows: vec![vec![json!("app'1"), json!("update t set x = 'secret' where id = 42"), Value::Null], vec![json!("b"), json!("select $1"), json!("select E'a\\'b', 7")]],
            rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None,
        };
        mask_query_text(&mut r, DbKind::Postgres);
        assert_eq!(r.rows[0], vec![json!("app'1"), json!("update t set x = ? where id = ?"), Value::Null]);
        assert_eq!(r.rows[1], vec![json!("b"), json!("select $1"), json!("select ?, ?")]);
    }

    /// SHOW REPLICA STATUS: its columns come from the server in its own case, and the last errors quote the failed statement
    #[test]
    fn replica_status_errors_are_masked() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "TEXT".into() };
        let sql_error = "Error 'Duplicate entry 'kim@example.com' for key 'users.email'' on query. Default database: 'shop'. Query: 'INSERT INTO users (email) VALUES ('kim@example.com')'";
        let worker = "Worker 1 failed executing transaction 'ANONYMOUS'; Could not execute Write_rows event on table shop.users; Duplicate entry '4242' for key 'users.PRIMARY', Error_code: 1062";
        let mut r = QueryResult {
            columns: vec![col("Replica_IO_Running"), col("Last_Errno"), col("Last_Error"), col("Last_IO_Error"), col("Last_SQL_Error")],
            rows: vec![vec![json!("Yes"), json!(1062), json!(sql_error), json!(""), json!(worker)]],
            rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None,
        };
        mask_query_text(&mut r, DbKind::Mysql);
        assert_eq!(r.rows[0], vec![json!("Yes"), json!(1062), json!("Error ?"), json!(""), json!("Worker 1 failed executing transaction ?, Error_code: 1062")]);
        // One quote: masked to the end
        assert_eq!(mask_quoted("Access denied for user 'repl@10.0.0.6"), "Access denied for user ?");
        assert_eq!(mask_quoted("error reconnecting to source"), "error reconnecting to source");
    }

    #[test]
    fn plan_analysis_shape() {
        let p = qurry_core::plan::parse_pg(r#"[{"Plan": {"Node Type": "Hash Join", "Total Cost": 200, "Plan Rows": 1, "Join Type": "Inner", "Hash Cond": "(a.id = b.id)",
            "Plans": [{"Node Type": "Seq Scan", "Relation Name": "a", "Total Cost": 150, "Plan Rows": 50000}, {"Node Type": "Hash", "Total Cost": 30, "Plan Rows": 10}]}}]"#).unwrap();
        let v = plan_analysis(&p);
        let a = &v["plan_analysis"];
        assert_eq!(a["basis"], "cost");
        assert_eq!((a["plan"]["op"].clone(), a["plan"]["self_cost"].clone(), a["plan"]["pct"].clone()), (json!("Hash Join"), json!(20.0), json!(10.0)));
        let scan = &a["plan"]["children"][0];
        assert!(scan["warnings"][0].as_str().unwrap().contains("Seq Scan") && scan.get("children").is_none() && scan.get("self_time_ms").is_none(), "{scan}");
    }

    /// Real servers (one at a time): QURRY_TEST_PG=host:port:user:db (trust auth; pg_stat_statements preloaded with
    /// pg_stat_statements.track=all), cargo test -p qurry-mcp pg_dba_erd_explain -- --ignored
    #[tokio::test]
    #[ignore]
    async fn pg_dba_erd_explain() {
        let pg = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG=host:port:user:db");
        let pg: Vec<&str> = pg.split(':').collect();
        let mut c = source("pg", "Demo PG", Some("read"), false).config;
        (c.host, c.port, c.user, c.database) = (pg[0].into(), pg[1].parse().unwrap(), pg[2].into(), pg[3].into());
        let (server, dir) = server_with(&[]);
        server.state.save_connection(c, None, None).await.unwrap();
        let server = Arc::new(server);
        let st = server.state.clone();
        let run = |q: &str| st.run_query("pg".into(), q.into(), None, None);
        run("create extension if not exists pg_stat_statements").await.unwrap();
        run("drop schema if exists qurry_mcp cascade").await.unwrap();
        run("create schema qurry_mcp").await.unwrap();
        run("create table qurry_mcp.customers (id int primary key, name text)").await.unwrap();
        run("create table qurry_mcp.orders (id int primary key, customer_id int references qurry_mcp.customers(id), total numeric)").await.unwrap();
        run("insert into qurry_mcp.customers select g, 'c' || g from generate_series(1, 500) g").await.unwrap();
        run("select pg_stat_statements_reset()").await.unwrap();
        let call = |name: &'static str, a: Value| {
            let s = server.clone();
            async move { handle(s, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": a } })).await.unwrap()["result"].clone() }
        };
        let text = |r: &Value, i: usize| r["content"][i]["text"].as_str().unwrap_or_default().to_string();

        // Literals in a statement MCP ran show as ? in slow_queries
        let q = call("query", json!({ "data_source": "Demo PG", "sql": "select name from qurry_mcp.customers where name = 'secret-lit-7731' or id = 918273" })).await;
        assert_eq!(q["isError"], json!(false), "{q}");
        let r = call("dba_report", json!({ "data_source": "Demo PG", "report": "slow_queries" })).await;
        let t = text(&r, 0);
        assert!(t.contains("qurry_mcp.customers") && !t.contains("secret-lit-7731") && !t.contains("918273") && !t.contains("\"actions\""), "{t}");
        assert!(t.contains("\"title\": \"Slow Queries (Top 100)\""), "{t}");
        // A running session's text (another client's, as MCP refuses pg_sleep): literals masked
        let sleeper = {
            let st = st.clone();
            tokio::spawn(async move { st.run_query("pg".into(), "select pg_sleep(3), 'secret-lit-4410' as s, 55501 as n".into(), None, None).await.map_err(|e| e.to_string()) })
        };
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let t = text(&call("dba_report", json!({ "data_source": "Demo PG", "report": "sessions" })).await, 0);
        assert!(t.contains("select pg_sleep(?), ? as s, ? as n") && !t.contains("secret-lit-4410") && !t.contains("55501"), "{t}");
        sleeper.await.unwrap().unwrap();

        // ERD with its foreign key
        let erd = call("get_erd", json!({ "data_source": "Demo PG", "schemas": ["qurry_mcp"] })).await;
        let v: Value = serde_json::from_str(&text(&erd, 0)).unwrap();
        assert_eq!(v["foreign_keys"][0]["from_table"], "orders");
        assert!(v["mermaid"].as_str().unwrap().contains("customers ||--o{ orders"), "{v}");

        // explain: the text plan as before, then the analysis
        let sql = "select * from qurry_mcp.customers c join qurry_mcp.orders o on o.customer_id = c.id where c.id > 10";
        let r = call("explain", json!({ "data_source": "Demo PG", "sql": sql })).await;
        let before = st.run_read_only("pg".into(), format!("EXPLAIN (FORMAT TEXT) {sql}"), MAX_ROWS).await.unwrap();
        let before: Vec<&str> = before.rows.iter().filter_map(|row| row[0].as_str()).collect();
        assert_eq!(text(&r, 0), before.join("\n"));
        let a: Value = serde_json::from_str(&text(&r, 1)).unwrap();
        assert_eq!(a["plan_analysis"]["basis"], "cost");
        assert!(a["plan_analysis"]["plan"]["op"].is_string(), "{a}");
        assert_eq!(r["content"].as_array().unwrap().len(), 2);

        // analyze: the statement runs once (the text plan); the analysis is by cost. pg_stat_statements (track = all) counts a call for
        // the nested statement of a plain EXPLAIN too, which never runs it: its rows only grow with a run (count(*): one row each)
        let counted = "select count(*) from qurry_mcp.customers where id > 7";
        let ran = || async {
            let q = "select coalesce(sum(rows), 0)::int8 from pg_stat_statements where not toplevel and query like '%select count(*) from qurry_mcp.customers where id > $1'";
            st.run_query("pg".into(), q.into(), None, None).await.unwrap().rows[0][0].as_i64().unwrap()
        };
        let n0 = ran().await;
        let r = call("explain", json!({ "data_source": "Demo PG", "sql": counted, "analyze": true })).await;
        assert!(text(&r, 0).contains("actual time"), "{r}");
        assert_eq!(serde_json::from_str::<Value>(&text(&r, 1)).unwrap()["plan_analysis"]["basis"], "cost");
        assert_eq!(ran().await, n0 + 1, "executed once");

        // The consoles' statements (the GUI and the TUI send them through AppState::run_query): JSON plans that parse, with actual
        // rows under Analyze
        for analyze in [false, true] {
            let r = run(&explain_sql(&format!("{counted};"), DbKind::Postgres, analyze, true).unwrap()).await.unwrap();
            let cell = r.rows[0][0].clone();
            let p = qurry_core::plan::parse_pg(&cell.as_str().map(String::from).unwrap_or_else(|| cell.to_string())).unwrap();
            assert_eq!(p.actual_rows.is_some(), analyze, "{p:?}");
        }

        run("drop schema qurry_mcp cascade").await.unwrap();
        server.state.disconnect("pg".into()).await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// QURRY_TEST_MY=host:port:user:password:db, cargo test -p qurry-mcp mysql_dba_and_explain -- --ignored
    #[tokio::test]
    #[ignore]
    async fn mysql_dba_and_explain() {
        let my = std::env::var("QURRY_TEST_MY").expect("QURRY_TEST_MY=host:port:user:password:db");
        let my: Vec<&str> = my.split(':').collect();
        let mut c = source("my", "Demo MySQL", Some("read"), false).config;
        c.kind = DbKind::Mysql;
        (c.host, c.port, c.user, c.database) = (my[0].into(), my[1].parse().unwrap(), my[2].into(), my[4].into());
        let (server, dir) = server_with(&[]);
        server.state.save_connection(c, Some(my[3].into()), None).await.unwrap();
        let server = Arc::new(server);
        let call = |name: &'static str, a: Value| {
            let s = server.clone();
            async move { handle(s, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": a } })).await.unwrap()["result"].clone() }
        };
        let text = |r: &Value| r["content"][0]["text"].as_str().unwrap_or_default().to_string();
        // Another client's statement (MySQL keeps no digest of MCP's prepared ones)
        server.state.run_query("my".into(), "select 'secret-lit-3321' as s, 77123 as n".into(), None, None).await.unwrap();
        let t = text(&call("dba_report", json!({ "data_source": "Demo MySQL", "report": "slow_queries" })).await);
        assert!(t.contains("SELECT ? AS") && !t.contains("secret-lit-3321") && !t.contains("77123"), "{t}");
        let sleeper = {
            let st = server.state.clone();
            tokio::spawn(async move { st.run_query("my".into(), "select sleep(3), 'secret-lit-9090' as s, _utf8mb4'secret-lit-9191' as u".into(), None, None).await.map_err(|e| e.to_string()) })
        };
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let t = text(&call("dba_report", json!({ "data_source": "Demo MySQL", "report": "sessions" })).await);
        assert!(t.contains("select sleep(?), ? as s, ? as u") && !t.contains("secret-lit-9"), "{t}");
        sleeper.await.unwrap().unwrap();
        // explain: one content item, the rows as before
        let r = call("explain", json!({ "data_source": "Demo MySQL", "sql": "select 1" })).await;
        assert_eq!(r["content"].as_array().unwrap().len(), 1, "{r}");
        assert!(text(&r).contains("\"columns\""), "{r}");
        let r = call("explain", json!({ "data_source": "Demo MySQL", "sql": "select 1;", "analyze": true })).await;
        assert!(text(&r).contains("actual time"), "{r}");
        // The consoles' statements through AppState::run_query (the GUI's and the TUI's path)
        for analyze in [false, true] {
            let r = server.state.run_query("my".into(), explain_sql("select 1;", DbKind::Mysql, analyze, true).unwrap(), None, None).await.unwrap();
            assert!(r.has_result_set && !r.rows.is_empty(), "{r:?}");
        }
        server.state.disconnect("my".into()).await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Only the SQLite driver (cargo build --manifest-path drivers/sqlite/Cargo.toml --target-dir target), cargo test -p qurry-mcp
    /// sqlite_explain -- --ignored
    #[tokio::test]
    #[ignore]
    async fn sqlite_explain() {
        let (server, dir) = server_with(&[]);
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = source("lite", "Demo SQLite", Some("read"), false).config;
        (c.kind, c.host, c.port, c.user, c.database) = (DbKind::Sqlite, String::new(), 0, String::new(), dir.join("app.db").to_string_lossy().into());
        server.state.save_connection(c, None, None).await.unwrap();
        let server = Arc::new(server);
        let st = server.state.clone();
        st.run_query("lite".into(), "create table t (id integer primary key, name text)".into(), None, None).await.unwrap();
        let call = |name: &'static str, a: Value| {
            let s = server.clone();
            async move { handle(s, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": a } })).await.unwrap()["result"].clone() }
        };
        let r = call("explain", json!({ "data_source": "Demo SQLite", "sql": "select * from t where id = 1;" })).await;
        assert!(r["content"][0]["text"].as_str().unwrap().contains("SEARCH t USING INTEGER PRIMARY KEY"), "{r}");
        let r = call("explain", json!({ "data_source": "Demo SQLite", "sql": "select * from t", "analyze": true })).await;
        assert_eq!(r["isError"], json!(true), "{r}");
        assert_eq!(r["content"][0]["text"], "SQLite has no Explain that runs the statement: use Explain without Analyze");
        // The consoles' statement through AppState::run_query (the GUI's and the TUI's path)
        let r = st.run_query("lite".into(), explain_sql("select * from t where id = 1;", DbKind::Sqlite, false, true).unwrap(), None, None).await.unwrap();
        assert!(r.rows.iter().flatten().any(|v| v.as_str().is_some_and(|s| s.contains("SEARCH t"))), "{r:?}");
        st.disconnect("lite".into()).await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn uri_passwords_are_hidden() {
        assert_eq!(qurry_core::store::without_userinfo("mongodb+srv://app:s3cret@cluster0.x.net/?retryWrites=true"), "mongodb+srv://cluster0.x.net/?retryWrites=true");
        assert_eq!(qurry_core::store::without_userinfo("mongodb://a:b@h1:27017,h2:27017/db"), "mongodb://h1:27017,h2:27017/db");
        assert_eq!(qurry_core::store::without_userinfo("https://qdrant.example.com"), "https://qdrant.example.com");
        assert_eq!(qurry_core::store::without_userinfo("db.internal"), "db.internal");
        assert_eq!(qurry_core::store::without_userinfo("u:p@db.internal"), "db.internal");
        assert_eq!(qurry_core::store::without_userinfo("mongodb://app:p/a?s#s@h1:27017/db"), "mongodb://h1:27017/db");
    }
}
