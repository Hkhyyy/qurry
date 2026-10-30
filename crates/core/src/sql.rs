//! SQL utilities per DB kind (shared by TUI·MCP). Quoting, the lexer and statement splitting live in qurry_shared::sql
//! (re-exported here); only the data source helpers are core's own.
use crate::store::{ConnectionConfig, DbKind};

pub use qurry_shared::sql::*;

/// Connection target id: "<data source id>::<database>"
pub fn target_id(conn: &str, db: Option<&str>) -> String {
    match db.filter(|d| !d.is_empty()) {
        Some(d) => format!("{conn}::{d}"),
        None => conn.to_string(),
    }
}

/// Default DB name for the tree·console
pub fn default_db(cfg: &ConnectionConfig) -> String {
    match cfg.kind {
        DbKind::Postgres => if cfg.database.trim().is_empty() { "postgres".into() } else { cfg.database.trim().into() },
        DbKind::Mysql => cfg.database.trim().into(),
        DbKind::Redis => format!("db{}", crate::redisdb::db_index(&cfg.database)),
        // File DBs use main, other drivers use the configured DB name as is
        k if k.is_file() => "main".into(),
        // Vector DB: the server's default name if empty
        DbKind::Qdrant if cfg.database.trim().is_empty() => "qdrant".into(),
        DbKind::Chroma if cfg.database.trim().is_empty() => "default_database".into(),
        DbKind::Milvus if cfg.database.trim().is_empty() => "default".into(),
        _ => cfg.database.trim().into(),
    }
}

