//! The MCP tools: their list (tools/list), and the dispatch of tools/call to one function per tool, grouped by file
//! (execute, only with --allow-write, is still answered here).
use crate::Server;
use qurry_core::app::ConnectionView;
use qurry_core::sql;
use qurry_core::store::{DbKind, McpAccess};
use serde_json::{json, Value};

mod dba;
mod query;
mod redis;
mod search;
mod sources;

use dba::report_ids;
use query::result_json;

pub(crate) const MAX_ROWS: usize = 1000;

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
    pub(crate) fn tools(&self) -> Vec<Value> {
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

    pub(crate) async fn call(&self, name: &str, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let e = |e: qurry_core::error::AppError| e.to_string();
        match name {
            "list_data_sources" => self.list_data_sources().await,
            "list_databases" => self.list_databases(a).await,
            "list_schemas" => self.list_schemas(a).await,
            "list_objects" => self.list_objects(a).await,
            "describe_table" => self.describe_table(a).await,
            "get_ddl" => self.get_ddl(a).await,
            "search_objects" => self.search_objects(a).await,
            "query" => self.query(a).await,
            "get_erd" => self.get_erd(a).await,
            "dba_report" => self.dba_report(a).await,
            "full_text_search" => self.full_text_search(a).await,
            "redis_scan" => self.redis_scan(a).await,
            "redis_get" => self.redis_get(a).await,
            "export_csv" => self.export_csv(a).await,
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
            "list_rag_indexes" => self.list_rag_indexes().await,
            "semantic_search" => self.semantic_search(a).await,
            other => Err(format!("Unknown tool: {other}")),
        }
    }

    /// A tool's answer as MCP content items: one for every tool, except explain on PostgreSQL (the plan, then its analysis)
    pub(crate) async fn contents(&self, name: &str, a: &Value) -> Result<Vec<Value>, String> {
        match name {
            "explain" => self.explain(a).await,
            _ => Ok(vec![self.call(name, a).await?]),
        }
    }
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

/// search_objects' databases of a source: the ones its tree shows (Schemas tab scope, system DBs hidden unless scoped), at most 8
fn search_dbs(cfg: &qurry_core::store::ConnectionConfig, all: Vec<String>, def: &str) -> Vec<String> {
    qurry_core::app::visible_dbs(cfg, all, def).into_iter().take(8).collect()
}

/// search_objects ranking: the TUI palette's score (qurry_shared::fuzzy); None = no match
fn fuzzy(q: &str, text: &str) -> Option<i64> {
    qurry_core::fuzzy::score(q, text, Default::default()).map(|s| s as i64)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Arc;

    pub(crate) fn source(id: &str, name: &str, mcp: Option<&str>, production: bool) -> ConnectionView {
        let mut config: qurry_core::store::ConnectionConfig =
            serde_json::from_value(json!({ "id": id, "name": name, "kind": "postgres", "host": "db.internal", "port": 5432, "user": "admin" })).unwrap();
        config.options.mcp = mcp.map(String::from);
        config.options.confirm_changes = production;
        ConnectionView { config, has_password: true, connected: false, temporary: false }
    }

    pub(crate) fn server_with(sources: &[ConnectionView]) -> (Server, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("qurry-mcp-test-{}-{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()));
        let state = qurry_core::open_state(Some(dir.clone())).unwrap();
        for s in sources {
            state.store.save(&s.config).unwrap();
        }
        (Server { state: Arc::new(state), allow_write: false }, dir)
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

    /// search_objects looks in the databases the app's tree shows: system DBs of every kind hidden, unless the Schemas tab scopes them
    #[test]
    fn search_skips_system_databases_unless_scoped() {
        let cfg = |kind: &str, scope: Option<&[&str]>| {
            let mut c = source("1", "S", Some("read"), false).config;
            c.kind = serde_json::from_value(json!(kind)).unwrap();
            c.database = "app".into();
            c.scope.databases = scope.map(|s| s.iter().map(|d| d.to_string()).collect());
            c
        };
        let v = |x: &[&str]| x.iter().map(|d| d.to_string()).collect::<Vec<_>>();
        for (kind, system) in [
            ("mssql", &["master", "tempdb", "model", "msdb"][..]),
            ("mongodb", &["admin", "config", "local"][..]),
            ("clickhouse", &["system", "information_schema", "INFORMATION_SCHEMA"][..]),
            ("postgres", &["template0", "template1", "rdsadmin"][..]),
            ("mysql", &["information_schema", "performance_schema", "mysql", "sys"][..]),
        ] {
            let all = [&["app", "sales"][..], system].concat();
            let c = cfg(kind, None);
            assert_eq!(search_dbs(&c, v(&all), &sql::default_db(&c)), v(&["app", "sales"]), "{kind}");
            // Scoped in the Schemas tab: searched
            let c = cfg(kind, Some(&[system[0], "app"]));
            assert_eq!(search_dbs(&c, v(&all), &sql::default_db(&c)), v(&["app", system[0]]), "{kind} scoped");
        }
        // At most 8
        let c = cfg("postgres", None);
        let many: Vec<String> = (0..12).map(|i| format!("db{i}")).collect();
        assert_eq!(search_dbs(&c, many.clone(), "app"), many[..8].to_vec());
    }
}
