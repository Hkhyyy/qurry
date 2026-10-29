//! Qurry driver plugin protocol. A driver is a separately downloaded executable (qurry-driver-<name>) that Qurry launches, exchanging one JSON per line over stdio.
//!
//! Request: `{"id": 1, "method": "query", "params": {...}}` → response: `{"id": 1, "result": ...}` or `{"id": 1, "error": "..."}`
//!
//! | method | params | result |
//! |---|---|---|
//! | hello | – | `{name, version, protocol, sql}` |
//! | connect | `{host, port, user, password, database, options}` | `null` |
//! | query | `{sql, limit}` | `{columns: [{name, typeName}], rows, rowsAffected, truncated, hasResultSet}` |
//! | databases | – | `["db", …]` |
//! | schemas | `{database}` | `[{name, isSystem}]` |
//! | objects | `{schema}` | `{relations: [{name, kind, comment, rowEstimate}], routines: []}` |
//! | tableDetail | `{schema, table}` | `{columns: [{name, dataType, nullable, defaultValue, comment, isPk}], indexes: [{name, definition, isUnique, isPrimary}], constraints: [{name, kind, definition}]}` |
//! | ddl | `{schema, name, kind}` | `"CREATE …"` |
#[cfg(feature = "rest")]
pub mod rest;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};

/// Protocol version (no connection if it differs from Qurry's)
pub const PROTOCOL: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectParams {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: Option<String>,
    /// SQLite, DuckDB: file path
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOut {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    pub truncated: bool,
    pub has_result_set: bool,
}

/// Driver: takes one method at a time and returns a result (JSON) or an error message
pub trait Driver {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
}

/// Read requests from stdin and respond on stdout (ends when stdin closes)
pub fn serve(mut d: impl Driver) {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(r) => match d.call(&r.method, r.params) {
                Ok(v) => serde_json::json!({ "id": r.id, "result": v }),
                Err(e) => serde_json::json!({ "id": r.id, "error": e }),
            },
            Err(e) => serde_json::json!({ "id": 0, "error": format!("bad request: {e}") }),
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

/// Extract params (error message if missing)
pub fn params<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("bad params: {e}"))
}

/// Result as JSON
pub fn ok<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}
