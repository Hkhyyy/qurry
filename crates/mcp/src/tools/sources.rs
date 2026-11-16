//! Data sources and their objects: list_data_sources, list_databases, list_schemas, list_objects, describe_table, get_ddl, search_objects
use super::{fuzzy, search_dbs};
use crate::Server;
use qurry_core::dba;
use qurry_core::sql;
use qurry_core::store::{DbKind, McpAccess};
use serde_json::{json, Value};

impl Server {
    pub(super) async fn list_data_sources(&self) -> Result<Value, String> {
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

    pub(super) async fn list_databases(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (id, ..) = self.resolve(a).await?;
        Ok(json!(st.list_databases(id).await.map_err(e)?))
    }

    pub(super) async fn list_schemas(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, ..) = self.target(a).await?;
        let v = st.list_schemas(t).await.map_err(e)?;
        Ok(json!(v.iter().map(|s| json!({ "name": s.name, "system": s.is_system })).collect::<Vec<_>>()))
    }

    pub(super) async fn list_objects(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let schema_or = |kind: DbKind, db: &str| s("schema").unwrap_or_else(|| if kind == DbKind::Postgres { "public".into() } else { db.to_string() });
        let (t, kind, db, _) = self.target(a).await?;
        let o = st.list_objects(t, schema_or(kind, &db)).await.map_err(e)?;
        Ok(json!({ "relations": o.relations, "routines": o.routines }))
    }

    pub(super) async fn describe_table(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let schema_or = |kind: DbKind, db: &str| s("schema").unwrap_or_else(|| if kind == DbKind::Postgres { "public".into() } else { db.to_string() });
        let (t, kind, db, _) = self.target(a).await?;
        let d = st.table_detail(t, schema_or(kind, &db), s("table").ok_or("table is required")?).await.map_err(e)?;
        Ok(json!(d))
    }

    pub(super) async fn get_ddl(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let schema_or = |kind: DbKind, db: &str| s("schema").unwrap_or_else(|| if kind == DbKind::Postgres { "public".into() } else { db.to_string() });
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

    pub(super) async fn search_objects(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
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
                        let v = search_dbs(&c.config, all, &def);
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
        hits.sort_by_key(|h| std::cmp::Reverse(h.0));
        Ok(json!({ "matches": hits.into_iter().take(limit).map(|h| h.1).collect::<Vec<_>>(), "errors": errors }))
    }
}

#[cfg(test)]
mod tests {
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
