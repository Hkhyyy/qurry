//! Qurry 드라이버: MongoDB. DB → 컬렉션 (테이블처럼). 콘솔은 셸 문법 또는 명령 JSON:
//!   db.users.find({age: {$gt: 30}}, {name: 1}).sort({name: 1}).limit(10)
//!   db.users.aggregate([{$group: {_id: "$city", n: {$sum: 1}}}])
//!   db.users.countDocuments({}) · insertOne({...}) · insertMany([...]) · updateOne(f, u) · updateMany · deleteOne · deleteMany
//!   {"find": "users", "filter": {...}}  (그 밖의 명령도 그대로 runCommand)
//! 탐색기에서 컬렉션을 열 때의 SELECT * FROM db.coll [LIMIT n] 은 find 로 바꾼다
use mongodb::bson::{Bson, Document, doc};
use mongodb::sync::{Client, Database};
use qurry_driver::{Column, ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};

#[derive(Default)]
struct Mongo {
    client: Option<Client>,
    db: String,
}

/// 셸 확장 (ObjectId("…"), ISODate("…"), NumberLong(…)) → 확장 JSON, 그 뒤 JSON5 (따옴표 없는 키·작은따옴표)
fn parse_args(s: &str) -> Result<Vec<Bson>, String> {
    let re_oid = regex::Regex::new(r#"ObjectId\(\s*["']([0-9a-fA-F]{24})["']\s*\)"#).unwrap();
    let re_date = regex::Regex::new(r#"(?:ISODate|new Date)\(\s*["']([^"']+)["']\s*\)"#).unwrap();
    let re_long = regex::Regex::new(r#"Number(?:Long|Int)\(\s*["']?(-?\d+)["']?\s*\)"#).unwrap();
    let s = re_oid.replace_all(s, r#"{"$$oid": "$1"}"#);
    let s = re_date.replace_all(&s, r#"{"$$date": "$1"}"#);
    let s = re_long.replace_all(&s, "$1");
    let v: Value = json5::from_str(&format!("[{s}]")).map_err(|e| format!("can't read the arguments: {e}"))?;
    v.as_array().unwrap().iter().map(|x| Bson::try_from(x.clone()).map_err(|e| e.to_string())).collect()
}

fn as_doc(b: Option<&Bson>) -> Document {
    match b {
        Some(Bson::Document(d)) => d.clone(),
        _ => Document::new(),
    }
}

/// bson → 화면 값 (ObjectId·날짜는 글로, 중첩은 JSON 그대로)
fn value(b: &Bson) -> Value {
    match b {
        Bson::ObjectId(o) => Value::String(o.to_hex()),
        Bson::DateTime(d) => Value::String(d.try_to_rfc3339_string().unwrap_or_default()),
        Bson::Int32(i) => json!(i),
        Bson::Int64(i) => json!(i),
        Bson::Double(f) => json!(f),
        Bson::String(s) => Value::String(s.clone()),
        Bson::Boolean(b) => json!(b),
        Bson::Null | Bson::Undefined => Value::Null,
        Bson::Decimal128(d) => Value::String(d.to_string()),
        other => other.clone().into_relaxed_extjson(),
    }
}

/// 문서들 → 표 (처음 나온 순서로 키를 모아 열로)
fn table(docs: Vec<Document>, truncated: bool) -> QueryOut {
    let mut keys: Vec<String> = vec![];
    for d in &docs {
        for k in d.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    let rows = docs.iter().map(|d| keys.iter().map(|k| d.get(k).map(value).unwrap_or(Value::Null)).collect()).collect();
    QueryOut { columns: keys.into_iter().map(|name| Column { name, type_name: String::new() }).collect(), rows, truncated, has_result_set: true, rows_affected: 0 }
}

impl Mongo {
    fn db(&self) -> Result<Database, String> {
        Ok(self.client.as_ref().ok_or("not connected")?.database(&self.db))
    }

    fn find(&self, coll: &str, filter: Document, proj: Option<Document>, sort: Option<Document>, skip: u64, limit: usize) -> Result<QueryOut, String> {
        let c = self.db()?.collection::<Document>(coll);
        let mut f = c.find(filter).limit(limit as i64 + 1).skip(skip);
        if let Some(p) = proj {
            f = f.projection(p);
        }
        if let Some(s) = sort {
            f = f.sort(s);
        }
        let mut docs: Vec<Document> = f.run().map_err(|e| e.to_string())?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
        let truncated = docs.len() > limit;
        docs.truncate(limit);
        Ok(table(docs, truncated))
    }

    fn query(&self, text: &str, limit: usize) -> Result<QueryOut, String> {
        let t = text.trim().trim_end_matches(';').trim();
        // 탐색기: SELECT * FROM db.coll [LIMIT n]
        let sel = regex::Regex::new(r#"(?is)^select\s+\*\s+from\s+(?:"?([^".\s]+)"?\.)?"?([^"\s;]+)"?\s*(?:limit\s+(\d+))?$"#).unwrap();
        if let Some(c) = sel.captures(t) {
            let n = c.get(3).and_then(|x| x.as_str().parse().ok()).unwrap_or(limit).min(limit);
            return self.find(&c[2], Document::new(), None, None, 0, n);
        }
        if t.to_lowercase().starts_with("select") {
            return Err("MongoDB isn't SQL — use db.collection.find({...}) (see the console help)".into());
        }
        if t.starts_with('{') {
            let cmd = parse_args(t)?.into_iter().next().and_then(|b| b.as_document().cloned()).ok_or("expected a command document")?;
            let r = self.db()?.run_command(cmd).run().map_err(|e| e.to_string())?;
            // find / aggregate 명령이면 cursor.firstBatch 를 표로
            if let Ok(c) = r.get_document("cursor") {
                let batch: Vec<Document> = c.get_array("firstBatch").map(|a| a.iter().filter_map(|b| b.as_document().cloned()).collect()).unwrap_or_default();
                return Ok(table(batch, false));
            }
            return Ok(table(vec![r], false));
        }
        if t == "db.getCollectionNames()" || t == "show collections" {
            let names = self.db()?.list_collection_names().run().map_err(|e| e.to_string())?;
            return Ok(table(names.into_iter().map(|n| doc! { "name": n }).collect(), false));
        }
        // db.<coll>.<method>(args)[.sort(..)][.limit(n)][.skip(n)]
        let re = regex::Regex::new(r#"(?s)^db\.(?:getCollection\(\s*["']([^"']+)["']\s*\)|([A-Za-z_][\w.-]*?))\.(\w+)\((.*)\)$"#).unwrap();
        let caps = re.captures(t).ok_or("expected db.collection.method(...) or a command {...}")?;
        let coll = caps.get(1).or(caps.get(2)).unwrap().as_str().to_string();
        let mut method = caps[3].to_string();
        let mut rest = caps[4].to_string();
        // 끝에 붙은 .sort() .limit() .skip() 을 떼어 낸다
        let (mut sort, mut lim, mut skip) = (None, None, 0u64);
        let tail = regex::Regex::new(r#"(?s)^(.*)\)\.(sort|limit|skip)\((.*)$"#).unwrap();
        while let Some(c) = tail.captures(&rest.clone()) {
            let arg = c[3].trim().to_string();
            match &c[2] {
                "sort" => sort = parse_args(&arg)?.into_iter().next().and_then(|b| b.as_document().cloned()),
                "limit" => lim = arg.parse::<usize>().ok(),
                _ => skip = arg.parse().unwrap_or(0),
            }
            rest = c[1].to_string();
        }
        if method == "find" && rest.contains(").") {
            method = "find".into();
        }
        let args = if rest.trim().is_empty() { vec![] } else { parse_args(&rest)? };
        let c = self.db()?.collection::<Document>(&coll);
        let err = |e: mongodb::error::Error| e.to_string();
        let changed = |n: u64| Ok(QueryOut { rows_affected: n, ..Default::default() });
        match method.as_str() {
            "find" => self.find(&coll, as_doc(args.first()), args.get(1).and_then(|b| b.as_document().cloned()), sort, skip, lim.unwrap_or(limit).min(limit)),
            "findOne" => self.find(&coll, as_doc(args.first()), args.get(1).and_then(|b| b.as_document().cloned()), None, 0, 1),
            "aggregate" => {
                let pipeline: Vec<Document> = match args.first() {
                    Some(Bson::Array(a)) => a.iter().filter_map(|b| b.as_document().cloned()).collect(),
                    _ => return Err("aggregate expects an array of stages".into()),
                };
                let mut docs: Vec<Document> = c.aggregate(pipeline).run().map_err(err)?.take(limit + 1).collect::<Result<_, _>>().map_err(err)?;
                let truncated = docs.len() > limit;
                docs.truncate(limit);
                Ok(table(docs, truncated))
            }
            "countDocuments" | "count" => Ok(table(vec![doc! { "count": c.count_documents(as_doc(args.first())).run().map_err(err)? as i64 }], false)),
            "distinct" => {
                let field = args.first().and_then(|b| b.as_str()).ok_or("distinct expects a field name")?;
                Ok(table(c.distinct(field, as_doc(args.get(1))).run().map_err(err)?.into_iter().map(|v| doc! { field: v }).collect(), false))
            }
            "insertOne" => {
                c.insert_one(as_doc(args.first())).run().map_err(err)?;
                changed(1)
            }
            "insertMany" => match args.first() {
                Some(Bson::Array(a)) => changed(c.insert_many(a.iter().filter_map(|b| b.as_document().cloned()).collect::<Vec<_>>()).run().map_err(err)?.inserted_ids.len() as u64),
                _ => Err("insertMany expects an array of documents".into()),
            },
            "updateOne" => changed(c.update_one(as_doc(args.first()), as_doc(args.get(1))).run().map_err(err)?.modified_count),
            "updateMany" => changed(c.update_many(as_doc(args.first()), as_doc(args.get(1))).run().map_err(err)?.modified_count),
            "replaceOne" => changed(c.replace_one(as_doc(args.first()), as_doc(args.get(1))).run().map_err(err)?.modified_count),
            "deleteOne" => changed(c.delete_one(as_doc(args.first())).run().map_err(err)?.deleted_count),
            "deleteMany" => changed(c.delete_many(as_doc(args.first())).run().map_err(err)?.deleted_count),
            "drop" => {
                c.drop().run().map_err(err)?;
                changed(0)
            }
            "createIndex" => Ok(table(vec![doc! { "name": c.create_index(mongodb::IndexModel::builder().keys(as_doc(args.first())).build()).run().map_err(err)?.index_name }], false)),
            m => Err(format!("{m} isn't supported here — run it as a command: {{\"{m}\": \"{coll}\", …}}")),
        }
    }
}

impl Driver for Mongo {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "mongodb", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": false })),
            "connect" => {
                let c: ConnectParams = params(p)?;
                // host 에 mongodb:// 나 mongodb+srv:// URI 를 넣어도 된다
                let uri = if c.host.starts_with("mongodb") {
                    c.host.clone()
                } else {
                    let auth = if c.user.is_empty() { String::new() } else { format!("{}:{}@", enc(&c.user), enc(c.password.as_deref().unwrap_or(""))) };
                    let tls = if c.options.get("ssl").is_some_and(|s| !s.is_null()) { "&tls=true" } else { "" };
                    format!("mongodb://{auth}{}:{}/?serverSelectionTimeoutMS=10000&appName=qurry{tls}", c.host, if c.port == 0 { 27017 } else { c.port })
                };
                let client = Client::with_uri_str(&uri).map_err(|e| e.to_string())?;
                self.db = if c.database.trim().is_empty() { "test".into() } else { c.database.trim().into() };
                client.database("admin").run_command(doc! { "ping": 1 }).run().map_err(|e| e.to_string())?;
                self.client = Some(client);
                ok(Value::Null)
            }
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            "databases" => ok(self.client.as_ref().ok_or("not connected")?.list_database_names().run().map_err(|e| e.to_string())?),
            "schemas" => ok(json!([{ "name": self.db, "isSystem": false }])),
            "objects" => {
                let names = self.db()?.list_collection_names().run().map_err(|e| e.to_string())?;
                let mut names: Vec<String> = names.into_iter().filter(|n| !n.starts_with("system.")).collect();
                names.sort();
                ok(json!({ "relations": names.iter().map(|n| json!({ "name": n, "kind": "table", "comment": null, "rowEstimate": null })).collect::<Vec<_>>(), "routines": [] }))
            }
            "tableDetail" => {
                let coll = p.get("table").and_then(Value::as_str).unwrap_or("");
                let c = self.db()?.collection::<Document>(coll);
                // 문서 100개를 보고 필드와 타입을 짐작한다
                let docs: Vec<Document> = c.find(Document::new()).limit(100).run().map_err(|e| e.to_string())?.filter_map(Result::ok).collect();
                let mut fields: Vec<(String, String)> = vec![];
                for d in &docs {
                    for (k, v) in d {
                        let ty = format!("{:?}", v.element_type()).to_lowercase();
                        match fields.iter_mut().find(|f| f.0 == *k) {
                            Some(f) if !f.1.split(" | ").any(|t| t == ty) => f.1 = format!("{} | {ty}", f.1),
                            None => fields.push((k.clone(), ty)),
                            _ => {}
                        }
                    }
                }
                let columns: Vec<Value> = fields.iter().map(|(k, t)| json!({ "name": k, "dataType": t, "nullable": k != "_id", "defaultValue": null, "comment": null, "isPk": k == "_id" })).collect();
                let idx: Vec<Value> = c.list_indexes().run().map_err(|e| e.to_string())?.filter_map(Result::ok).map(|i| {
                    let name = i.options.as_ref().and_then(|o| o.name.clone()).unwrap_or_default();
                    json!({ "name": name, "definition": format!("db.{coll}.createIndex({})", Bson::Document(i.keys.clone()).into_relaxed_extjson()), "isUnique": i.options.as_ref().and_then(|o| o.unique).unwrap_or(false), "isPrimary": name == "_id_" })
                }).collect();
                ok(json!({ "columns": columns, "indexes": idx, "constraints": [] }))
            }
            "ddl" => {
                let coll = p.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let d = self.call("tableDetail", json!({ "table": coll }))?;
                let mut out = format!("db.createCollection(\"{coll}\")\n");
                for i in d["indexes"].as_array().into_iter().flatten().filter(|i| i["isPrimary"] != json!(true)) {
                    out += &format!("{}\n", i["definition"].as_str().unwrap_or(""));
                }
                ok(out)
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

/// URI 의 사용자·비밀번호 (특수 문자를 %XX 로)
fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn main() {
    serve(Mongo::default());
}
