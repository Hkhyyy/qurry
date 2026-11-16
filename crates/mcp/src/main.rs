//! Qurry MCP server (stdio, JSON-RPC 2.0). Uses the same engine (qurry-core) and the same Data Sources as the GUI·TUI.
//!
//! Read-only by default: PG/MySQL run a single statement inside a READ ONLY transaction, Redis allows read commands only.
//! `--allow-write` adds the `execute` tool (Read-only and Production Data Sources still never take it).
//! MCP exposure is set per Data Source (Options › MCP: Off / Read / Read & write — store::McpAccess). Hidden sources are not listed and cannot be connected to.
mod mask;
mod tools;

use qurry_core::app::AppState;
use qurry_core::db::QueryResult;
use qurry_core::dba;
use qurry_core::erd::Erd;
use qurry_core::plan::PlanNode;
use qurry_core::store::DbKind;
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
