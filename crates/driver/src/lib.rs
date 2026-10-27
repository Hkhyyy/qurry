//! Qurry 드라이버 플러그인 프로토콜. 드라이버는 따로 받는 실행 파일 (qurry-driver-<이름>) 이고, Qurry 가 띄워서 stdio 로 한 줄에 하나씩 JSON 을 주고받는다.
//!
//! 요청: `{"id": 1, "method": "query", "params": {...}}` → 응답: `{"id": 1, "result": ...}` 또는 `{"id": 1, "error": "..."}`
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

/// 프로토콜 버전 (Qurry 와 다르면 연결하지 않는다)
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
    /// SQLite·DuckDB: 파일 경로
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

/// 드라이버: 메서드 하나씩 받아 결과(JSON) 나 오류 글
pub trait Driver {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
}

/// stdin 에서 요청을 읽고 stdout 으로 응답한다 (stdin 이 닫히면 끝)
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

/// params 를 꺼낸다 (모자라면 오류 글)
pub fn params<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("bad params: {e}"))
}

/// 결과를 JSON 으로
pub fn ok<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}
