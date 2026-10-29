//! Qurry 드라이버: ClickHouse (HTTP 인터페이스, 기본 8123 / TLS 8443). DB = 스키마 (MySQL 처럼 한 단계)
use qurry_driver::{Column, ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};

#[derive(Default)]
struct Ch {
    url: String,
    user: String,
    password: String,
    database: String,
    agent: Option<ureq::Agent>,
    /// Read-only Data Source
    read_only: bool,
    /// 지금 쿼리는 읽기만 (Read-only Data Source 또는 MCP 읽기): 서버도 readonly=2 로 막는다
    ro: bool,
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// 결과를 돌려주는 문장인지
fn returns_rows(sql: &str) -> bool {
    let w = sql.trim_start().split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("").to_lowercase();
    ["select", "with", "show", "describe", "desc", "explain", "exists"].contains(&w.as_str())
}

impl Ch {
    /// 쿼리 하나 (설정은 URL 로). 응답 본문과 요약 헤더
    fn post(&self, sql: &str, settings: &[(&str, String)]) -> Result<(String, Option<String>), String> {
        let agent = self.agent.as_ref().ok_or("not connected")?;
        let mut req = agent.post(&self.url).header("X-ClickHouse-User", &self.user).header("X-ClickHouse-Key", &self.password).query("database", &self.database);
        for (k, v) in settings {
            req = req.query(*k, v);
        }
        // readonly=2: 읽기만, 이 요청의 설정 (max_result_rows …) 은 바꿀 수 있다
        if self.ro {
            req = req.query("readonly", "2");
        }
        let mut res = req.config().http_status_as_error(false).build().send(sql).map_err(|e| e.to_string())?;
        let summary = res.headers().get("X-ClickHouse-Summary").and_then(|v| v.to_str().ok()).map(String::from);
        let ok_status = res.status().is_success();
        let body = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
        if !ok_status {
            return Err(body.trim().to_string());
        }
        Ok((body, summary))
    }

    fn query(&self, sql: &str, limit: usize) -> Result<QueryOut, String> {
        let sql = sql.trim().trim_end_matches(';');
        if !returns_rows(sql) {
            let (_, summary) = self.post(sql, &[])?;
            let n = summary.and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.get("written_rows").and_then(|x| x.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0);
            return Ok(QueryOut { rows_affected: n, ..Default::default() });
        }
        // 한 행 더 받아 잘렸는지 안다
        let (body, _) = self.post(&format!("{sql} FORMAT JSONCompact"), &[("max_result_rows", (limit + 1).to_string()), ("result_overflow_mode", "break".into())])?;
        let v: Value = serde_json::from_str(&body).map_err(|e| format!("bad reply: {e}"))?;
        let columns: Vec<Column> = v["meta"].as_array().map(|m| m.iter().map(|c| Column { name: c["name"].as_str().unwrap_or("").into(), type_name: c["type"].as_str().unwrap_or("").to_uppercase() }).collect()).unwrap_or_default();
        let mut rows: Vec<Vec<Value>> = v["data"].as_array().map(|d| d.iter().map(|r| r.as_array().cloned().unwrap_or_default()).collect()).unwrap_or_default();
        let truncated = rows.len() > limit;
        rows.truncate(limit);
        Ok(QueryOut { columns, rows, has_result_set: true, truncated, rows_affected: 0 })
    }

    fn rows(&self, sql: &str) -> Result<Vec<Vec<Value>>, String> {
        Ok(self.query(sql, 1_000_000)?.rows)
    }
}

impl Driver for Ch {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "clickhouse", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": true })),
            "version" => ok(format!("ClickHouse {}", self.rows("SELECT version()")?.first().and_then(|r| r[0].as_str()).unwrap_or(""))),
            "connect" => {
                let c: ConnectParams = params(p)?;
                let tls = c.options.get("ssl").is_some_and(|s| !s.is_null());
                let port = if c.port == 0 { if tls { 8443 } else { 8123 } } else { c.port };
                self.url = format!("{}://{}:{port}/", if tls { "https" } else { "http" }, c.host);
                self.user = if c.user.is_empty() { "default".into() } else { c.user };
                self.password = c.password.unwrap_or_default();
                self.database = if c.database.trim().is_empty() { "default".into() } else { c.database.trim().into() };
                // 리다이렉트는 따라가지 않는다 (X-ClickHouse-Key 가 다른 서버로 가지 않게)
                self.agent = Some(ureq::Agent::config_builder().timeout_connect(Some(std::time::Duration::from_secs(10))).max_redirects(0).build().into());
                self.read_only = c.options.get("readOnly").and_then(Value::as_bool).unwrap_or(false);
                self.ro = self.read_only;
                self.query("SELECT 1", 1)?;
                ok(Value::Null)
            }
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                self.ro = self.read_only || p.get("readOnly").and_then(Value::as_bool).unwrap_or(false);
                let r = self.query(sql, limit);
                self.ro = self.read_only;
                ok(r?)
            }
            "databases" => ok(self.rows("SELECT name FROM system.databases ORDER BY name")?.into_iter().map(|r| r[0].clone()).collect::<Vec<_>>()),
            // DB 가 곧 스키마
            "schemas" => ok(json!([{ "name": self.database, "isSystem": false }])),
            "objects" => {
                let db = p.get("schema").and_then(Value::as_str).unwrap_or(&self.database).to_string();
                let rows = self.rows(&format!("SELECT name, engine, comment, total_rows FROM system.tables WHERE database = {} ORDER BY name", lit(&db)))?;
                let relations: Vec<Value> = rows
                    .iter()
                    .map(|r| json!({ "name": r[0], "kind": if r[1].as_str().unwrap_or("").ends_with("View") { "view" } else { "table" }, "comment": r[2].as_str().filter(|c| !c.is_empty()), "rowEstimate": r[3].as_str().and_then(|n| n.parse::<i64>().ok()).or(r[3].as_i64()) }))
                    .collect();
                ok(json!({ "relations": relations, "routines": [] }))
            }
            "tableDetail" => {
                let db = lit(p.get("schema").and_then(Value::as_str).unwrap_or(&self.database));
                let t = lit(p.get("table").and_then(Value::as_str).unwrap_or(""));
                let rows = self.rows(&format!("SELECT name, type, default_expression, is_in_primary_key, comment FROM system.columns WHERE database = {db} AND table = {t} ORDER BY position"))?;
                let pk: Vec<String> = rows.iter().filter(|r| r[3] == json!(1) || r[3] == json!("1")).map(|r| r[0].as_str().unwrap_or("").to_string()).collect();
                let columns: Vec<Value> = rows
                    .iter()
                    .map(|r| json!({ "name": r[0], "dataType": r[1], "nullable": r[1].as_str().unwrap_or("").starts_with("Nullable("), "defaultValue": r[2].as_str().filter(|d| !d.is_empty()), "comment": r[4].as_str().filter(|c| !c.is_empty()), "isPk": r[3] == json!(1) || r[3] == json!("1") }))
                    .collect();
                let constraints = if pk.is_empty() { vec![] } else { vec![json!({ "name": "primary_key", "kind": "p", "definition": format!("PRIMARY KEY ({})", pk.join(", ")) })] };
                ok(json!({ "columns": columns, "indexes": [], "constraints": constraints }))
            }
            "ddl" => {
                let db = p.get("schema").and_then(Value::as_str).unwrap_or(&self.database).replace('`', "");
                let name = p.get("name").and_then(Value::as_str).unwrap_or("").replace('`', "");
                let r = self.rows(&format!("SHOW CREATE TABLE `{db}`.`{name}`"))?;
                ok(r.first().and_then(|x| x.first()).and_then(Value::as_str).map(|s| format!("{s};\n")).unwrap_or_default())
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Ch::default());
}
