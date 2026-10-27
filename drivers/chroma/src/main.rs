//! Qurry 드라이버: Chroma (REST v2, 기본 8000). DB = Chroma database (default_tenant 안), 컬렉션 = 테이블.
//! 콘솔은 그 DB 기준 경로 (컬렉션은 이름으로 써도 된다):
//!   GET /collections
//!   POST /collections/docs/query
//!   {query_embeddings: [[0.1, 0.2]], n_results: 5}
//! /api/... 로 시작하면 그대로 보낸다
use qurry_driver::rest::{Http, parse_request, table};
use qurry_driver::{ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Map, Value, json};

const TENANT: &str = "default_tenant";

#[derive(Default)]
struct Chroma {
    http: Option<Http>,
    db: String,
}

/// get/query 결과 (열마다 배열) → 행. query 는 질의마다 한 겹 더 싸여 있다
fn columnar(v: &Value, limit: usize) -> Option<QueryOut> {
    let ids = v.get("ids")?.as_array()?;
    let nested = ids.first().is_some_and(Value::is_array);
    let fields = [("ids", "id"), ("distances", "distance"), ("documents", "document"), ("metadatas", "metadata"), ("embeddings", "embedding"), ("uris", "uri")];
    let groups: Vec<usize> = if nested { (0..ids.len()).collect() } else { vec![0] };
    let mut rows = vec![];
    for q in groups {
        let at = |f: &str| -> Option<Vec<Value>> {
            let a = v.get(f)?.as_array()?;
            if nested { a.get(q)?.as_array().cloned() } else { Some(a.clone()) }
        };
        let n = at("ids").map(|a| a.len()).unwrap_or(0);
        for i in 0..n {
            let mut row = Map::new();
            if nested && ids.len() > 1 {
                row.insert("query".into(), json!(q));
            }
            for (f, name) in fields {
                if let Some(x) = at(f).and_then(|a| a.get(i).cloned()) {
                    row.insert(name.into(), x);
                }
            }
            rows.push(Value::Object(row));
        }
    }
    Some(table(&Value::Array(rows), &["metadata"], limit))
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
}

impl Chroma {
    fn prefix(&self) -> String {
        format!("/api/v2/tenants/{TENANT}/databases/{}", self.db)
    }

    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let (status, v) = self.http.as_ref().ok_or("not connected")?.send(method, path, body)?;
        if !(200..300).contains(&status) {
            return Err(match (v.get("error").and_then(Value::as_str), v.get("message").and_then(Value::as_str)) {
                (Some(e), Some(m)) => format!("{e}: {m}"),
                _ => format!("HTTP {status}: {v}"),
            });
        }
        Ok(v)
    }

    /// 컬렉션 이름 → id (API 는 /collections/{id}/... 를 받는다)
    fn coll_id(&self, name: &str) -> Result<String, String> {
        if is_uuid(name) {
            return Ok(name.into());
        }
        let v = self.call("GET", &format!("{}/collections/{name}", self.prefix()), None)?;
        v["id"].as_str().map(String::from).ok_or_else(|| format!("collection {name} not found"))
    }

    /// 콘솔 경로 → API 경로: DB 기준 경로를 붙이고, /collections/<이름>/<더> 의 이름을 id 로
    fn api_path(&self, path: &str) -> Result<String, String> {
        if path.starts_with("/api/") {
            return Ok(path.into());
        }
        let parts: Vec<&str> = path.trim_start_matches('/').splitn(3, '/').collect();
        Ok(match parts.as_slice() {
            ["collections", name, rest] => format!("{}/collections/{}/{rest}", self.prefix(), self.coll_id(name)?),
            _ => format!("{}{path}", self.prefix()),
        })
    }

    fn query(&self, text: &str, limit: usize) -> Result<QueryOut, String> {
        let t = text.trim();
        let lower = t.to_lowercase();
        // 탐색기: SELECT * FROM db.coll [LIMIT n] → get
        if lower.starts_with("select") {
            let after = lower.find(" from ").map(|i| &t[i + 6..]).ok_or("Chroma isn't SQL — use REST, e.g. POST /collections/<name>/query")?;
            let coll = after.split_whitespace().next().unwrap_or("").rsplit('.').next().unwrap_or("").trim_matches('"').to_string();
            let n = lower.split(" limit ").nth(1).and_then(|x| x.trim().parse::<usize>().ok()).unwrap_or(limit).min(limit);
            let v = self.call("POST", &format!("{}/collections/{}/get", self.prefix(), self.coll_id(&coll)?), Some(&json!({ "limit": n + 1, "include": ["documents", "metadatas", "embeddings"] })))?;
            return Ok(columnar(&v, n).unwrap_or_default());
        }
        let (m, p, body) = parse_request(t)?;
        let v = self.call(&m, &self.api_path(&p)?, body.as_ref())?;
        if let Some(out) = columnar(&v, limit) {
            return Ok(out);
        }
        let mut out = table(&v, &["metadata"], limit);
        // add/upsert/delete 는 빈 {} 를 돌려준다
        if m != "GET" && v.as_object().is_some_and(Map::is_empty) {
            out = QueryOut { has_result_set: false, rows_affected: 1, ..Default::default() };
        }
        Ok(out)
    }

    fn collections(&self) -> Result<Vec<Value>, String> {
        let v = self.call("GET", &format!("{}/collections?limit=10000", self.prefix()), None)?;
        let mut list = v.as_array().cloned().unwrap_or_default();
        list.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(list)
    }
}

impl Driver for Chroma {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "chroma", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": false })),
            "connect" => {
                let c: ConnectParams = params(p)?;
                let mut http = Http::new(&c, 8000);
                if let Some(k) = c.password.filter(|k| !k.is_empty()) {
                    http.headers.push(("x-chroma-token".into(), k.clone()));
                    http.headers.push(("authorization".into(), format!("Bearer {k}")));
                }
                self.http = Some(http);
                self.db = if c.database.trim().is_empty() { "default_database".into() } else { c.database.trim().into() };
                self.collections()?;
                ok(Value::Null)
            }
            "version" => ok(format!("Chroma {}", Chroma::call(self, "GET", "/api/v2/version", None)?.as_str().unwrap_or(""))),
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            "databases" => {
                let v = Chroma::call(self, "GET", &format!("/api/v2/tenants/{TENANT}/databases"), None)?;
                ok(v.as_array().into_iter().flatten().filter_map(|d| d["name"].as_str()).collect::<Vec<_>>())
            }
            "schemas" => ok(json!([{ "name": self.db, "isSystem": false }])),
            "objects" => ok(json!({ "relations": self.collections()?.iter().map(|c| json!({ "name": c["name"], "kind": "table", "comment": "collection", "rowEstimate": null })).collect::<Vec<_>>(), "routines": [] })),
            "tableDetail" => {
                let name = p.get("table").and_then(Value::as_str).unwrap_or("");
                let info = Chroma::call(self, "GET", &format!("{}/collections/{name}", self.prefix()), None)?;
                let id = info["id"].as_str().unwrap_or("");
                let space = info.pointer("/configuration_json/hnsw/space").and_then(Value::as_str).unwrap_or("l2");
                let dim = info["dimension"].as_u64().map(|d| d.to_string()).unwrap_or_else(|| "?".into());
                let col = |n: &str, t: &str, pk: bool| json!({ "name": n, "dataType": t, "nullable": !pk, "defaultValue": null, "comment": null, "isPk": pk });
                let mut columns = vec![col("id", "string", true), col("document", "text", false), col("embedding", &format!("vector({dim}, {space})"), false)];
                // metadata 필드: 50개를 보고 짐작
                let got = Chroma::call(self, "POST", &format!("{}/collections/{id}/get", self.prefix()), Some(&json!({ "limit": 50, "include": ["metadatas"] })))?;
                let mut seen: Vec<String> = vec![];
                for m in got["metadatas"].as_array().into_iter().flatten() {
                    for (k, v) in m.as_object().into_iter().flatten() {
                        if !seen.contains(k) {
                            seen.push(k.clone());
                            let ty = match v { Value::String(_) => "string", Value::Number(_) => "number", Value::Bool(_) => "bool", _ => "json" };
                            columns.push(col(&format!("metadata.{k}"), ty, false));
                        }
                    }
                }
                ok(json!({ "columns": columns, "indexes": [], "constraints": [] }))
            }
            "ddl" => {
                let name = p.get("name").and_then(Value::as_str).unwrap_or("");
                let info = Chroma::call(self, "GET", &format!("{}/collections/{name}", self.prefix()), None)?;
                let body = json!({ "name": name, "metadata": info["metadata"], "configuration": { "hnsw": { "space": info.pointer("/configuration_json/hnsw/space") } } });
                ok(format!("POST /collections\n{}\n", serde_json::to_string_pretty(&body).unwrap_or_default()))
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Chroma::default());
}
