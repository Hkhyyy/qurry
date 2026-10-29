//! Qurry driver: DuckDB (a single file, :memory: too). Has schemas (default main)
use duckdb::{Connection, types::Value as DV};
use qurry_driver::{Column, ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};

#[derive(Default)]
struct Duck {
    conn: Option<Connection>,
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn value(v: DV) -> Value {
    match v {
        DV::Null => Value::Null,
        DV::Boolean(b) => json!(b),
        DV::TinyInt(x) => json!(x),
        DV::SmallInt(x) => json!(x),
        DV::Int(x) => json!(x),
        DV::BigInt(x) => json!(x),
        DV::UTinyInt(x) => json!(x),
        DV::USmallInt(x) => json!(x),
        DV::UInt(x) => json!(x),
        DV::UBigInt(x) => json!(x),
        DV::Float(x) => json!(x),
        DV::Double(x) => json!(x),
        DV::HugeInt(x) => Value::String(x.to_string()),
        DV::Decimal(d) => Value::String(d.to_string()),
        DV::Text(s) => Value::String(s),
        DV::Blob(b) => Value::String(format!("0x{}", b.iter().map(|x| format!("{x:02X}")).collect::<String>())),
        DV::List(v) | DV::Array(v) => Value::Array(v.into_iter().map(value).collect()),
        other => Value::String(format!("{other:?}")),
    }
}

/// Whether the statement returns results (otherwise execute it for the affected row count)
fn returns_rows(sql: &str) -> bool {
    let w = sql.trim_start().split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("").to_lowercase();
    ["select", "with", "show", "describe", "pragma", "values", "table", "from", "explain", "summarize", "call"].contains(&w.as_str())
}

impl Duck {
    fn db(&self) -> Result<&Connection, String> {
        self.conn.as_ref().ok_or_else(|| "not connected".to_string())
    }

    fn query(&self, sql: &str, limit: usize) -> Result<QueryOut, String> {
        let db = self.db()?;
        if !returns_rows(sql) {
            let n = db.execute(sql, []).map_err(|e| e.to_string())?;
            return Ok(QueryOut { rows_affected: n as u64, ..Default::default() });
        }
        let mut st = db.prepare(sql).map_err(|e| e.to_string())?;
        let mut rows = st.query([]).map_err(|e| e.to_string())?;
        let stmt = rows.as_ref().ok_or("no statement")?;
        let n = stmt.column_count();
        let columns: Vec<Column> = (0..n).map(|i| Column { name: stmt.column_name(i).map(|s| s.to_string()).unwrap_or_default(), type_name: format!("{:?}", stmt.column_type(i)).to_uppercase() }).collect();
        let mut out = QueryOut { columns, has_result_set: true, ..Default::default() };
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            if out.rows.len() >= limit {
                out.truncated = true;
                break;
            }
            out.rows.push((0..n).map(|i| r.get::<_, DV>(i).map(value).unwrap_or(Value::Null)).collect());
        }
        Ok(out)
    }

    fn col(&self, sql: &str) -> Result<Vec<Vec<Value>>, String> {
        Ok(self.query(sql, 1_000_000)?.rows)
    }
}

impl Driver for Duck {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "duckdb", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": true })),
            "version" => ok(format!("DuckDB {}", self.col("SELECT version()")?.first().and_then(|r| r[0].as_str()).unwrap_or(""))),
            "connect" => {
                let c: ConnectParams = params(p)?;
                let path = if c.database.trim().is_empty() { ":memory:" } else { c.database.trim() };
                let read_only = c.options.get("readOnly").and_then(Value::as_bool).unwrap_or(false);
                let mut cfg = duckdb::Config::default().access_mode(if read_only { duckdb::AccessMode::ReadOnly } else { duckdb::AccessMode::ReadWrite }).map_err(|e| e.to_string())?;
                // MCP: disable local file · URL reads and extension install · load, and lock it so it can't be re-enabled
                if c.options.get("sandbox").and_then(Value::as_bool).unwrap_or(false) {
                    cfg = cfg.enable_external_access(false).and_then(|c| c.enable_autoload_extension(false)).and_then(|c| c.with("lock_configuration", "true")).map_err(|e| e.to_string())?;
                }
                self.conn = Some(Connection::open_with_flags(path, cfg).map_err(|e| format!("{path}: {e}"))?);
                ok(Value::Null)
            }
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            "databases" => ok(self.col("SELECT database_name FROM duckdb_databases() WHERE NOT internal ORDER BY 1")?.into_iter().map(|r| r[0].clone()).collect::<Vec<_>>()),
            "schemas" => {
                let rows = self.col("SELECT schema_name FROM information_schema.schemata WHERE catalog_name = current_database() ORDER BY 1")?;
                ok(rows.into_iter().map(|r| { let n = r[0].as_str().unwrap_or("").to_string(); json!({ "isSystem": n == "information_schema" || n == "pg_catalog", "name": n }) }).collect::<Vec<_>>())
            }
            "objects" => {
                let schema = p.get("schema").and_then(Value::as_str).unwrap_or("main");
                let rows = self.col(&format!("SELECT table_name, table_type FROM information_schema.tables WHERE table_catalog = current_database() AND table_schema = {} ORDER BY 1", lit(schema)))?;
                let relations: Vec<Value> = rows.iter().map(|r| json!({ "name": r[0], "kind": if r[1] == "VIEW" { "view" } else { "table" }, "comment": null, "rowEstimate": null })).collect();
                ok(json!({ "relations": relations, "routines": [] }))
            }
            "tableDetail" => {
                let schema = lit(p.get("schema").and_then(Value::as_str).unwrap_or("main"));
                let table = lit(p.get("table").and_then(Value::as_str).unwrap_or(""));
                let cons = self.col(&format!("SELECT constraint_type, constraint_text, constraint_column_names, coalesce(constraint_name, '') FROM duckdb_constraints() WHERE schema_name = {schema} AND table_name = {table}"))?;
                let pk: Vec<String> = cons.iter().filter(|c| c[0] == "PRIMARY KEY").flat_map(|c| c[2].as_array().cloned().unwrap_or_default()).filter_map(|v| v.as_str().map(String::from)).collect();
                let cols = self.col(&format!("SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = {schema} AND table_name = {table} ORDER BY ordinal_position"))?;
                let columns: Vec<Value> = cols.iter().map(|r| { let n = r[0].as_str().unwrap_or(""); json!({ "name": n, "dataType": r[1], "nullable": r[2] == "YES", "defaultValue": r[3], "comment": null, "isPk": pk.iter().any(|p| p == n) }) }).collect();
                let constraints: Vec<Value> = cons
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        let kind = match c[0].as_str()? {
                            "PRIMARY KEY" => "p",
                            "FOREIGN KEY" => "f",
                            "UNIQUE" => "u",
                            "CHECK" => "c",
                            _ => return None,
                        };
                        let name = c[3].as_str().filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| format!("{}_{kind}{i}", p.get("table").and_then(Value::as_str).unwrap_or("")));
                        Some(json!({ "name": name, "kind": kind, "definition": c[1] }))
                    })
                    .collect();
                let idx = self.col(&format!("SELECT index_name, is_unique, is_primary, coalesce(sql, '') FROM duckdb_indexes() WHERE schema_name = {schema} AND table_name = {table}"))?;
                let indexes: Vec<Value> = idx.iter().map(|r| json!({ "name": r[0], "isUnique": r[1], "isPrimary": r[2], "definition": r[3] })).collect();
                ok(json!({ "columns": columns, "indexes": indexes, "constraints": constraints }))
            }
            "ddl" => {
                let schema = lit(p.get("schema").and_then(Value::as_str).unwrap_or("main"));
                let name = lit(p.get("name").and_then(Value::as_str).unwrap_or(""));
                let rows = self.col(&format!("SELECT sql FROM duckdb_tables() WHERE schema_name = {schema} AND table_name = {name} UNION ALL SELECT sql FROM duckdb_views() WHERE schema_name = {schema} AND view_name = {name}"))?;
                ok(rows.into_iter().filter_map(|r| r[0].as_str().map(|s| format!("{}\n", s.trim()))).collect::<String>())
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Duck::default());
}
