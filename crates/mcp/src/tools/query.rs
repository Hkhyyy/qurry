//! Running statements: query, export_csv, explain
use super::MAX_ROWS;
use crate::Server;
use qurry_core::db::QueryResult;
use qurry_core::explain::explain_sql;
use qurry_core::plan::PlanNode;
use qurry_core::store::DbKind;
use serde_json::{json, Value};

impl Server {
    pub(super) async fn query(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, ..) = self.target(a).await?;
        let limit = n("limit", 100).clamp(1, MAX_ROWS);
        let r = st.run_read_only(t, s("sql").ok_or("sql is required")?, limit).await.map_err(e)?;
        Ok(result_json(&r))
    }

    pub(super) async fn export_csv(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, ..) = self.target(a).await?;
        let q = s("sql").ok_or("sql is required")?;
        // Only in the designated folder, as a new file (never write to a path chosen by someone else)
        let path = qurry_core::export_path(&s("file_name").ok_or("file_name is required")?).map_err(e)?;
        let rows = st.export_csv_read_only(t, q, path.clone()).await.map_err(e)?;
        Ok(json!({ "rows": rows, "path": path.display().to_string() }))
    }

    /// The plan as before (PostgreSQL: the FORMAT TEXT lines, others: the rows). PostgreSQL also gets {"plan_analysis": …} from a separate
    /// EXPLAIN (FORMAT JSON) without ANALYZE, so with analyze the statement still runs once; left out if that run or its parse fails
    pub(super) async fn explain(&self, a: &Value) -> Result<Vec<Value>, String> {
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

pub(super) fn result_json(r: &QueryResult) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tests::source;
    use std::sync::Arc;

    /// A source behind AWS SSM, through the core tests' fake aws (crates/core/tests/fake_aws.py) on PATH. Its session stops
    /// forwarding while the CLI runs on (sleep, a network change, the AWS side): the next query opens the tunnel again and answers,
    /// without restarting the server.
    /// Run on its own (it changes PATH): QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-mcp query_recovers -- --ignored
    #[cfg(unix)]
    #[tokio::test]
    #[ignore]
    async fn query_recovers_from_a_dead_ssm_tunnel() {
        use std::os::unix::fs::PermissionsExt;
        let Some(pg) = qurry_core::testenv::var("QURRY_TEST_PG") else { return };
        let pg: Vec<&str> = pg.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-mcp-ssm-{}-{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let python = std::env::split_paths(&path).map(|d| d.join("python3")).find(|p| p.is_file()).expect("python3 on PATH");
        let fake = include_str!("../../../core/tests/fake_aws.py").replace("@MODE@", "ok").replacen("/usr/bin/env python3", &python.to_string_lossy(), 1);
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
}
