//! Qurry 드라이버: SQLite (파일 하나). database = 파일 경로, 스키마 = main (ATTACH 한 DB 도)
use qurry_driver::{Column, ConnectParams, Driver, QueryOut, ok, params, serve};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Value, json};

#[derive(Default)]
struct Sqlite {
    conn: Option<Connection>,
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn value(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("0x{}", b.iter().map(|x| format!("{x:02X}")).collect::<String>())),
    }
}

impl Sqlite {
    fn db(&self) -> Result<&Connection, String> {
        self.conn.as_ref().ok_or_else(|| "not connected".to_string())
    }

    fn query(&self, sql: &str, limit: usize) -> Result<QueryOut, String> {
        let db = self.db()?;
        let mut st = db.prepare(sql).map_err(|e| e.to_string())?;
        if st.column_count() == 0 {
            let n = st.execute([]).map_err(|e| e.to_string())?;
            return Ok(QueryOut { rows_affected: n as u64, ..Default::default() });
        }
        let columns: Vec<Column> = st.columns().iter().map(|c| Column { name: c.name().to_string(), type_name: c.decl_type().unwrap_or("").to_uppercase() }).collect();
        let n = columns.len();
        let mut rows = st.query([]).map_err(|e| e.to_string())?;
        let mut out = QueryOut { columns, has_result_set: true, ..Default::default() };
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            if out.rows.len() >= limit {
                out.truncated = true;
                break;
            }
            out.rows.push((0..n).map(|i| r.get_ref(i).map(value).unwrap_or(Value::Null)).collect());
        }
        Ok(out)
    }

    /// 한 열짜리 결과의 글들
    fn strings(&self, sql: &str) -> Result<Vec<String>, String> {
        Ok(self.query(sql, 100_000)?.rows.into_iter().map(|r| r.first().and_then(|v| v.as_str()).unwrap_or("").to_string()).collect())
    }
}

impl Driver for Sqlite {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "sqlite", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": true })),
            "connect" => {
                let c: ConnectParams = params(p)?;
                let read_only = c.options.get("readOnly").and_then(Value::as_bool).unwrap_or(false);
                let flags = if read_only { OpenFlags::SQLITE_OPEN_READ_ONLY } else { OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE };
                let conn = Connection::open_with_flags(&c.database, flags | OpenFlags::SQLITE_OPEN_URI).map_err(|e| format!("{}: {e}", c.database))?;
                conn.execute_batch("PRAGMA foreign_keys = ON").map_err(|e| e.to_string())?;
                self.conn = Some(conn);
                ok(Value::Null)
            }
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            // SQLite: DB = 스키마 (main, temp 는 빼고 ATTACH 한 것)
            "databases" => ok(self.strings("SELECT name FROM pragma_database_list WHERE name <> 'temp'")?),
            "schemas" => {
                let db = p.get("database").and_then(Value::as_str).unwrap_or("main").to_string();
                ok(json!([{ "name": if db.is_empty() { "main".to_string() } else { db }, "isSystem": false }]))
            }
            "objects" => {
                let schema = p.get("schema").and_then(Value::as_str).unwrap_or("main");
                let r = self.query(&format!("SELECT name, type FROM {}.sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name", quote(schema)), 100_000)?;
                let relations: Vec<Value> = r.rows.iter().map(|x| json!({ "name": x[0], "kind": x[1], "comment": null, "rowEstimate": null })).collect();
                ok(json!({ "relations": relations, "routines": [] }))
            }
            "tableDetail" => {
                let schema = p.get("schema").and_then(Value::as_str).unwrap_or("main");
                let table = p.get("table").and_then(Value::as_str).unwrap_or("");
                let (s, t) = (quote(schema), table.replace('\'', "''"));
                let cols = self.query(&format!("SELECT name, type, \"notnull\", dflt_value, pk FROM {s}.pragma_table_info('{t}')"), 10_000)?;
                let mut pk: Vec<(i64, String)> = vec![];
                let columns: Vec<Value> = cols
                    .rows
                    .iter()
                    .map(|r| {
                        let k = r[4].as_i64().unwrap_or(0);
                        if k > 0 {
                            pk.push((k, r[0].as_str().unwrap_or("").to_string()));
                        }
                        json!({ "name": r[0], "dataType": r[1], "nullable": r[2].as_i64() == Some(0), "defaultValue": r[3], "comment": null, "isPk": k > 0 })
                    })
                    .collect();
                pk.sort();
                let idx = self.query(&format!("SELECT il.name, il.\"unique\", il.origin, m.sql FROM {s}.pragma_index_list('{t}') il LEFT JOIN {s}.sqlite_master m ON m.name = il.name"), 10_000)?;
                let indexes: Vec<Value> = idx
                    .rows
                    .iter()
                    .map(|r| json!({ "name": r[0], "definition": r[3].as_str().map(String::from).unwrap_or_else(|| format!("{} (automatic)", r[0].as_str().unwrap_or(""))), "isUnique": r[1].as_i64() == Some(1), "isPrimary": r[2] == "pk" }))
                    .collect();
                let fk = self.query(&format!("SELECT id, \"table\", \"from\", \"to\" FROM {s}.pragma_foreign_key_list('{t}') ORDER BY id, seq"), 10_000)?;
                let mut constraints: Vec<Value> = vec![];
                if !pk.is_empty() {
                    constraints.push(json!({ "name": format!("{table}_pkey"), "kind": "p", "definition": format!("PRIMARY KEY ({})", pk.iter().map(|x| quote(&x.1)).collect::<Vec<_>>().join(", ")) }));
                }
                let mut groups: Vec<(i64, String, Vec<String>, Vec<String>)> = vec![];
                for r in &fk.rows {
                    let id = r[0].as_i64().unwrap_or(0);
                    let (to_t, from, to) = (r[1].as_str().unwrap_or("").to_string(), r[2].as_str().unwrap_or("").to_string(), r[3].as_str().unwrap_or("").to_string());
                    match groups.last_mut() {
                        Some(g) if g.0 == id => {
                            g.2.push(from);
                            g.3.push(to);
                        }
                        _ => groups.push((id, to_t, vec![from], vec![to])),
                    }
                }
                for (id, to_t, from, to) in groups {
                    constraints.push(json!({ "name": format!("{table}_fk{id}"), "kind": "f", "definition": format!("FOREIGN KEY ({}) REFERENCES {}({})", from.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", "), quote(&to_t), to.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ")) }));
                }
                ok(json!({ "columns": columns, "indexes": indexes, "constraints": constraints }))
            }
            "ddl" => {
                let schema = p.get("schema").and_then(Value::as_str).unwrap_or("main");
                let name = p.get("name").and_then(Value::as_str).unwrap_or("").replace('\'', "''");
                let sql = self.strings(&format!("SELECT sql FROM {}.sqlite_master WHERE name = '{name}' AND sql IS NOT NULL", quote(schema)))?;
                ok(sql.into_iter().map(|s| format!("{s};\n")).collect::<String>())
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Sqlite::default());
}
