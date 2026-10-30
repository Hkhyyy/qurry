//! Qurry driver: Milvus (REST v2, default 19530). DB = Milvus database, collection = table.
//! The console uses paths after /v2/vectordb (dbName is filled with the current DB):
//!   POST /entities/search
//!   {collectionName: 'docs', data: [[0.1, 0.2]], limit: 5, outputFields: ['*']}
use qurry_driver::rest::{Http, parse_request, table};
use qurry_driver::{ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};

#[derive(Default)]
struct Milvus {
    http: Option<Http>,
    db: String,
}

impl Milvus {
    /// path is after /v2/vectordb. If the body has no dbName, the current DB
    fn call(&self, path: &str, body: Value) -> Result<Value, String> {
        let mut body = if body.is_object() { body } else { json!({}) };
        if body.get("dbName").is_none() && !path.starts_with("/databases/") {
            body["dbName"] = json!(self.db);
        }
        let full = if path.starts_with("/v2/") { path.to_string() } else { format!("/v2/vectordb{path}") };
        let (status, v) = self.http.as_ref().ok_or("not connected")?.send("POST", &full, Some(&body))?;
        match v.get("code").and_then(Value::as_i64) {
            Some(0) => Ok(v),
            Some(_) => {
                let m = v["message"].as_str().unwrap_or("").to_string();
                Err(if m.contains("not loaded") { format!("{m} — load it first: POST /collections/load {{collectionName: '…'}}") } else { m })
            }
            None => Err(format!("HTTP {status}: {v}")),
        }
    }

    fn query(&self, text: &str, limit: usize) -> Result<QueryOut, String> {
        let t = text.trim();
        let lower = t.to_lowercase();
        // Explorer: SELECT * FROM db.coll [LIMIT n] → entities/query
        if lower.starts_with("select") {
            let after = lower.find(" from ").map(|i| &t[i + 6..]).ok_or("Milvus isn't SQL — use REST, e.g. POST /entities/search")?;
            let coll = after.split_whitespace().next().unwrap_or("").rsplit('.').next().unwrap_or("").trim_matches('"').to_string();
            let n = lower.split(" limit ").nth(1).and_then(|x| x.trim().parse::<usize>().ok()).unwrap_or(limit).min(limit).min(16383);
            let v = self.call("/entities/query", json!({ "collectionName": coll, "filter": "", "limit": n + 1, "outputFields": ["*"] }))?;
            return Ok(table(&v["data"], &[], n));
        }
        let (_m, p, body) = parse_request(t)?;
        let v = self.call(&p, body.unwrap_or(json!({})))?;
        let data = &v["data"];
        // insert/upsert/delete: how many
        if let Some(n) = ["insertCount", "upsertCount", "deleteCount"].iter().find_map(|k| data.get(*k).and_then(Value::as_u64)) {
            return Ok(QueryOut { has_result_set: false, rows_affected: n, ..Default::default() });
        }
        if data.as_object().is_some_and(|o| o.is_empty()) || data.is_null() {
            return Ok(QueryOut { has_result_set: false, ..Default::default() });
        }
        Ok(table(data, &[], limit))
    }

    fn collections(&self) -> Result<Vec<String>, String> {
        let v = self.call("/collections/list", json!({}))?;
        let mut names: Vec<String> = v["data"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(String::from)).collect();
        names.sort();
        Ok(names)
    }
}

/// Field type in describe: vectors include dimension and metric, e.g. FloatVector(768, COSINE)
fn field_type(f: &Value, indexes: &Value) -> String {
    let ty = f["type"].as_str().unwrap_or("");
    let dim = f["params"].as_array().into_iter().flatten().find(|p| p["key"] == "dim").and_then(|p| p["value"].as_str());
    let metric = indexes.as_array().into_iter().flatten().find(|i| i["fieldName"] == f["name"]).and_then(|i| i["metricType"].as_str());
    match (dim, metric) {
        (Some(d), Some(m)) => format!("{ty}({d}, {m})"),
        (Some(d), None) => format!("{ty}({d})"),
        _ => ty.to_string(),
    }
}

impl Driver for Milvus {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "milvus", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": false })),
            "connect" => {
                let c: ConnectParams = params(p)?;
                let mut http = Http::new(&c, 19530);
                // user:password if there's a user, otherwise the password as a token (API key)
                let pw = c.password.unwrap_or_default();
                let token = if c.user.is_empty() { pw } else { format!("{}:{pw}", c.user) };
                if !token.is_empty() {
                    http.headers.push(("authorization".into(), format!("Bearer {token}")));
                }
                self.http = Some(http);
                self.db = if c.database.trim().is_empty() { "default".into() } else { c.database.trim().into() };
                self.collections()?;
                ok(Value::Null)
            }
            // The server version isn't available over REST
            "version" => ok("Milvus (REST v2)"),
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            "databases" => ok(Milvus::call(self, "/databases/list", json!({}))?["data"].clone()),
            "schemas" => ok(json!([{ "name": self.db, "isSystem": false }])),
            "objects" => ok(json!({ "relations": self.collections()?.iter().map(|n| json!({ "name": n, "kind": "table", "comment": "collection", "rowEstimate": null })).collect::<Vec<_>>(), "routines": [] })),
            "tableDetail" => {
                let coll = p.get("table").and_then(Value::as_str).unwrap_or("");
                let d = Milvus::call(self, "/collections/describe", json!({ "collectionName": coll }))?["data"].clone();
                let columns: Vec<Value> = d["fields"].as_array().into_iter().flatten().map(|f| json!({ "name": f["name"], "dataType": field_type(f, &d["indexes"]), "nullable": f["nullable"].as_bool().unwrap_or(false), "defaultValue": null, "comment": f["description"].as_str().filter(|s| !s.is_empty()), "isPk": f["primaryKey"].as_bool().unwrap_or(false) })).collect();
                let indexes: Vec<Value> = d["indexes"].as_array().into_iter().flatten().map(|i| json!({ "name": i["indexName"], "definition": format!("{} on {} ({})", i["indexName"].as_str().unwrap_or(""), i["fieldName"].as_str().unwrap_or(""), i["metricType"].as_str().unwrap_or("")), "isUnique": false, "isPrimary": false })).collect();
                ok(json!({ "columns": columns, "indexes": indexes, "constraints": [] }))
            }
            "ddl" => {
                let coll = p.get("name").and_then(Value::as_str).unwrap_or("");
                let d = Milvus::call(self, "/collections/describe", json!({ "collectionName": coll }))?["data"].clone();
                let fields: Vec<Value> = d["fields"].as_array().into_iter().flatten().map(|f| {
                    let mut x = json!({ "fieldName": f["name"], "dataType": f["type"], "isPrimary": f["primaryKey"] });
                    if let Some(ps) = f["params"].as_array().filter(|a| !a.is_empty()) {
                        x["elementTypeParams"] = ps.iter().map(|p| (p["key"].as_str().unwrap_or("").to_string(), p["value"].clone())).collect::<serde_json::Map<_, _>>().into();
                    }
                    x
                }).collect();
                let idx: Vec<Value> = d["indexes"].as_array().into_iter().flatten().map(|i| json!({ "fieldName": i["fieldName"], "indexName": i["indexName"], "metricType": i["metricType"] })).collect();
                let body = json!({ "collectionName": coll, "schema": { "autoId": d["autoId"], "enableDynamicField": d["enableDynamicField"], "fields": fields }, "indexParams": idx });
                ok(format!("POST /collections/create\n{}\n", serde_json::to_string_pretty(&body).unwrap_or_default()))
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Milvus::default());
}
