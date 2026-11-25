//! MCP's own Redis reads, where they differ from the app's: within MCP's byte budget, values cut and binary as base64
//! (crate::redis_read)
use super::*;
use crate::redisdb::{self, KeyDetail, Reader};

impl AppState {
    /// redis_get: a key's type, TTL, size and its first `limit` elements, fewer when they would pass the byte budget. The key as
    /// bytes (it may not be UTF-8)
    pub async fn redis_key_read_only(&self, id: String, key: Vec<u8>, limit: usize) -> AppResult<KeyDetail> {
        redisdb::key_detail_with(&redis_pool(self, &id).await?, &key, limit, Reader::Mcp).await
    }

    /// get_ddl of a key: it is read whole, so only when that read passes the read checks (elements and bytes)
    pub async fn redis_key_ddl_read_only(&self, id: String, key: String) -> AppResult<String> {
        let m = redis_pool(self, &id).await?;
        crate::redis_read::check_whole_key(&m, key.as_bytes()).await.map_err(|e| AppError::Msg(format!("The DDL reads the whole key: {e}")))?;
        redisdb::key_ddl(&m, &key).await
    }

    /// search_objects on Redis: key names containing `query` in any case, from a bounded SCAN MATCH; and whether it scanned the
    /// whole database
    pub async fn redis_search_keys(&self, id: String, query: String, max: usize) -> AppResult<(Vec<Vec<u8>>, bool)> {
        crate::redis_read::search_keys(&redis_pool(self, &id).await?, &query, max).await
    }

    /// redis_scan: about `count` keys with their type and TTL, the keys as bytes (redis_read::key_json shows them)
    pub async fn redis_scan_read_only(&self, id: String, cursor: String, pattern: String, count: usize, type_filter: Option<String>) -> AppResult<(String, Vec<(Vec<u8>, String, i64)>)> {
        redisdb::scan_keys_page(&redis_pool(self, &id).await?, &cursor, &pattern, count, type_filter.as_deref()).await
    }
}
