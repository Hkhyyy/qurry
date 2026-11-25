//! Redis keys: redis_scan, redis_get
use crate::Server;
use qurry_core::redis_read::{key_arg, key_json};
use qurry_core::store::DbKind;
use serde_json::{json, Value};

impl Server {
    pub(super) async fn redis_scan(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, kind, ..) = self.target(a).await?;
        if kind != DbKind::Redis {
            return Err("Not a Redis data source".into());
        }
        let (cursor, keys) = st
            .redis_scan_read_only(t, s("cursor").unwrap_or_else(|| "0".into()), s("pattern").unwrap_or_else(|| "*".into()), n("count", 200).clamp(1, 5000), s("type"))
            .await
            .map_err(e)?;
        let keys: Vec<Value> = keys
            .iter()
            .map(|(k, ty, ttl)| {
                let mut row = key_json(k);
                row.insert("type".into(), json!(ty));
                row.insert("ttl".into(), json!(ttl));
                Value::Object(row)
            })
            .collect();
        Ok(json!({ "next_cursor": cursor, "keys": keys }))
    }

    pub(super) async fn redis_get(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let n = |k: &str, d: usize| a.get(k).and_then(|v| v.as_u64()).map(|x| x as usize).unwrap_or(d);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, kind, ..) = self.target(a).await?;
        if kind != DbKind::Redis {
            return Err("Not a Redis data source".into());
        }
        let key = key_arg(s("key").as_deref(), s("key_base64").as_deref()).map_err(e)?;
        Ok(json!(st.redis_key_read_only(t, key, n("limit", 200).clamp(1, 10000)).await.map_err(e)?))
    }
}
