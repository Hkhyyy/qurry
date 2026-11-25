//! Qurry MCP server (stdio, JSON-RPC 2.0). Uses the same engine (qurry-core) and the same Data Sources as the GUI·TUI.
//!
//! Read-only by default: PG/MySQL run a single statement inside a READ ONLY transaction, Redis allows read commands only.
//! `--allow-write` adds the `execute` tool (Read-only and Production Data Sources still never take it).
//! MCP exposure is set per Data Source (Options › MCP: Off / Read / Read & write — store::McpAccess). Hidden sources are not listed and cannot be connected to.
mod mask;
mod tools;

use qurry_core::app::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

const SERVER_NAME: &str = "qurry";
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

struct Server {
    state: Arc<AppState>,
    allow_write: bool,
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
                    if server.allow_write { "Write access is enabled via the execute tool." } else { "The server is read-only: query runs one statement in a READ ONLY transaction (PostgreSQL, MySQL) or one read command (Redis)." }
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
    qurry_core::logfile::log_panics();
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
    use crate::tools::tests::{server_with, source};
    use crate::tools::MAX_ROWS;
    use qurry_core::explain::explain_sql;
    use qurry_core::store::DbKind;

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

    /// Real servers (one at a time): QURRY_TEST_PG=host:port:user:db (trust auth; pg_stat_statements preloaded with
    /// pg_stat_statements.track=all), cargo test -p qurry-mcp pg_dba_erd_explain -- --ignored
    #[tokio::test]
    #[ignore]
    async fn pg_dba_erd_explain() {
        let Some(pg) = qurry_core::testenv::var("QURRY_TEST_PG") else { return };
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
        let Some(my) = qurry_core::testenv::var("QURRY_TEST_MY") else { return };
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
}
