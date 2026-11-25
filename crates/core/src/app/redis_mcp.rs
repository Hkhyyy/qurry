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
}
