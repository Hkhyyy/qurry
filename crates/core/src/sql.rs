//! SQL utilities per DB kind (shared by TUI·MCP). Quoting, the lexer and statement splitting live in qurry_shared::sql
//! (re-exported here); only the data source helpers are core's own.
use crate::store::ConnectionConfig;

pub use qurry_shared::sql::*;

/// Connection target id: "<data source id>::<database>"
pub fn target_id(conn: &str, db: Option<&str>) -> String {
    match db.filter(|d| !d.is_empty()) {
        Some(d) => format!("{conn}::{d}"),
        None => conn.to_string(),
    }
}

/// Default DB name for the tree·console (qurry_shared::kind::default_db)
pub fn default_db(cfg: &ConnectionConfig) -> String {
    qurry_shared::kind::default_db(cfg.kind, &cfg.database)
}

