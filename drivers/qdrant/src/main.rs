//! Qurry driver: Qdrant (REST, default 6333). Collection = table. The console speaks raw REST:
//!   GET /collections
//!   POST /collections/items/points/search
//!   {"vector": [0.1, 0.2, 0.3], "limit": 5, "with_payload": true}
//! SELECT * FROM x.coll from opening a collection in the explorer becomes a scroll
use qurry_driver::rest::{Http, parse_request, table};
use qurry_driver::{ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};

#[derive(Default)]
struct Qdrant {
    http: Option<Http>,
}

impl Qdrant {
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let (status, v) = self.http.as_ref().ok_or("not connected")?.send(method, path, body)?;
        if !(200..300).contains(&status) {
            return Err(v.pointer("/status/error").and_then(Value::as_str).map(String::from).unwrap_or_else(|| format!("HTTP {status}: {v}")));
        }
        Ok(v)
    }

    fn query(&self, text: &str, limit: usize) -> Result<QueryOut, String> {
        let t = text.trim();
        let lower = t.to_lowercase();
        // Explorer: SELECT * FROM x.coll [LIMIT n] → scroll
        if lower.starts_with("select") {
            let coll = lower.split(" from ").nth(1).ok_or("Qdrant isn't SQL — use REST, e.g. POST /collections/<name>/points/search")?;
            let coll = t[t.len() - coll.len()..].split_whitespace().next().unwrap_or("").rsplit('.').next().unwrap_or("").trim_matches('"').to_string();
            let n = lower.split(" limit ").nth(1).and_then(|x| x.trim().parse::<usize>().ok()).unwrap_or(limit).min(limit);
            let v = self.call("POST", &format!("/collections/{coll}/points/scroll"), Some(&json!({ "limit": n + 1, "with_payload": true, "with_vector": true })))?;
            return Ok(table(v.pointer("/result/points").unwrap_or(&Value::Null), &["payload"], n));
        }
        let (m, p, body) = parse_request(t)?;
        let v = self.call(&m, &p, body.as_ref())?;
        // Result shape: result is an array, or result.points / result.collections …
        let r = v.get("result").unwrap_or(&v);
        let main = match r {
            Value::Object(o) => o.values().find(|x| x.is_array()).unwrap_or(r),
            _ => r,
        };
        let mut out = table(main, &["payload"], limit);
        if m != "GET" && !matches!(main, Value::Array(_)) {
            out.rows_affected = 1;
        }
        Ok(out)
    }

    fn collections(&self) -> Result<Vec<String>, String> {
        let v = self.call("GET", "/collections", None)?;
        let mut names: Vec<String> = v.pointer("/result/collections").and_then(Value::as_array).into_iter().flatten().filter_map(|c| c["name"].as_str().map(String::from)).collect();
        names.sort();
        Ok(names)
    }
}

impl Driver for Qdrant {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "qdrant", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": false })),
            "connect" => {
                let c: ConnectParams = params(p)?;
                let mut http = Http::new(&c, 6333);
                if let Some(k) = c.password.filter(|k| !k.is_empty()) {
                    http.headers.push(("api-key".into(), k));
                }
                self.http = Some(http);
                self.collections()?;
                ok(Value::Null)
            }
            "version" => ok(format!("Qdrant {}", Qdrant::call(self, "GET", "/", None)?["version"].as_str().unwrap_or(""))),
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            "databases" => ok(vec!["qdrant"]),
            "schemas" => ok(json!([{ "name": "qdrant", "isSystem": false }])),
            "objects" => ok(json!({ "relations": self.collections()?.iter().map(|n| json!({ "name": n, "kind": "table", "comment": "collection", "rowEstimate": null })).collect::<Vec<_>>(), "routines": [] })),
            "tableDetail" => {
                let coll = p.get("table").and_then(Value::as_str).unwrap_or("");
                let info = Qdrant::call(self, "GET", &format!("/collections/{coll}"), None)?;
                let vecs = info.pointer("/result/config/params/vectors").cloned().unwrap_or(Value::Null);
                let mut columns = vec![json!({ "name": "id", "dataType": "point id", "nullable": false, "defaultValue": null, "comment": null, "isPk": true })];
                // One unnamed vector {size, distance} or several named ones
                match &vecs {
                    Value::Object(o) if o.contains_key("size") => columns.push(json!({ "name": "vector", "dataType": format!("vector({}, {})", o["size"], o["distance"].as_str().unwrap_or("")), "nullable": false, "defaultValue": null, "comment": null, "isPk": false })),
                    Value::Object(o) => {
                        for (n, c) in o {
                            columns.push(json!({ "name": format!("vector.{n}"), "dataType": format!("vector({}, {})", c["size"], c["distance"].as_str().unwrap_or("")), "nullable": true, "defaultValue": null, "comment": null, "isPk": false }));
                        }
                    }
                    _ => {}
                }
                // payload fields: guessed from 50 points
                let pts = Qdrant::call(self, "POST", &format!("/collections/{coll}/points/scroll"), Some(&json!({ "limit": 50, "with_payload": true, "with_vector": false })))?;
                let mut seen: Vec<(String, String)> = vec![];
                for pt in pts.pointer("/result/points").and_then(Value::as_array).into_iter().flatten() {
                    for (k, v) in pt["payload"].as_object().into_iter().flatten() {
                        let ty = match v { Value::String(_) => "keyword", Value::Number(_) => "number", Value::Bool(_) => "bool", Value::Array(_) => "array", Value::Object(_) => "object", Value::Null => "null" };
                        if !seen.iter().any(|s| s.0 == *k) {
                            seen.push((k.clone(), ty.to_string()));
                        }
                    }
                }
                for (k, t) in seen {
                    columns.push(json!({ "name": format!("payload.{k}"), "dataType": t, "nullable": true, "defaultValue": null, "comment": null, "isPk": false }));
                }
                let idx: Vec<Value> = info.pointer("/result/payload_schema").and_then(Value::as_object).into_iter().flatten().map(|(k, v)| json!({ "name": k, "definition": format!("payload index {k}: {}", v["data_type"].as_str().unwrap_or("")), "isUnique": false, "isPrimary": false })).collect();
                ok(json!({ "columns": columns, "indexes": idx, "constraints": [] }))
            }
            "ddl" => {
                let coll = p.get("name").and_then(Value::as_str).unwrap_or("");
                let info = Qdrant::call(self, "GET", &format!("/collections/{coll}"), None)?;
                let params = info.pointer("/result/config/params").cloned().unwrap_or(Value::Null);
                ok(format!("PUT /collections/{coll}\n{}\n", serde_json::to_string_pretty(&json!({ "vectors": params["vectors"] })).unwrap_or_default()))
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Qdrant::default());
}
