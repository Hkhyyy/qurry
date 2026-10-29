//! Qurry driver: SQL Server (tiberius, TDS). DB → schema (dbo …) → table
use qurry_driver::{Column, ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, FromSql};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

struct Ms {
    rt: tokio::runtime::Runtime,
    client: Option<Client<Compat<TcpStream>>>,
    /// Read-only Data Source
    read_only: bool,
}

fn lit(s: &str) -> String {
    format!("N'{}'", s.replace('\'', "''"))
}

fn q(s: &str) -> String {
    format!("[{}]", s.replace(']', "]]"))
}

fn value(d: &ColumnData<'static>) -> Value {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    match d {
        ColumnData::U8(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::I16(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::I32(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::I64(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::F32(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::F64(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::Bit(v) => v.map(|x| json!(x)).unwrap_or(Value::Null),
        ColumnData::String(v) => v.as_ref().map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        ColumnData::Guid(v) => v.map(|g| Value::String(g.to_string())).unwrap_or(Value::Null),
        ColumnData::Numeric(v) => v.map(|n| Value::String(n.to_string())).unwrap_or(Value::Null),
        ColumnData::Binary(v) => v.as_ref().map(|b| Value::String(format!("0x{}", b.iter().map(|x| format!("{x:02X}")).collect::<String>()))).unwrap_or(Value::Null),
        ColumnData::Xml(v) => v.as_ref().map(|x| Value::String(x.to_string())).unwrap_or(Value::Null),
        ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => NaiveDateTime::from_sql(d).ok().flatten().map(|t| Value::String(t.to_string())).unwrap_or(Value::Null),
        ColumnData::Date(_) => NaiveDate::from_sql(d).ok().flatten().map(|t| Value::String(t.to_string())).unwrap_or(Value::Null),
        ColumnData::Time(_) => NaiveTime::from_sql(d).ok().flatten().map(|t| Value::String(t.to_string())).unwrap_or(Value::Null),
        ColumnData::DateTimeOffset(_) => chrono::DateTime::<chrono::FixedOffset>::from_sql(d).ok().flatten().map(|t| Value::String(t.to_string())).unwrap_or(Value::Null),
    }
}

/// Whether the statement returns an affected row count (otherwise read the results)
fn is_write(sql: &str) -> bool {
    let w = sql.trim_start().split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("").to_lowercase();
    ["insert", "update", "delete", "merge", "create", "alter", "drop", "truncate", "grant", "revoke"].contains(&w.as_str())
}

impl Ms {
    fn query(&mut self, sql: &str, limit: usize) -> Result<QueryOut, String> {
        let client = self.client.as_mut().ok_or("not connected")?;
        self.rt.block_on(async {
            if is_write(sql) {
                let r = client.execute(sql, &[]).await.map_err(|e| e.to_string())?;
                return Ok(QueryOut { rows_affected: r.total(), ..Default::default() });
            }
            let mut stream = client.simple_query(sql).await.map_err(|e| e.to_string())?;
            let columns: Vec<Column> = stream.columns().await.map_err(|e| e.to_string())?.map(|cs| cs.iter().map(|c| Column { name: c.name().to_string(), type_name: format!("{:?}", c.column_type()).to_uppercase() }).collect()).unwrap_or_default();
            let mut out = QueryOut { has_result_set: !columns.is_empty(), columns, ..Default::default() };
            let rows = stream.into_first_result().await.map_err(|e| e.to_string())?;
            for r in rows {
                if out.rows.len() >= limit {
                    out.truncated = true;
                    break;
                }
                out.rows.push(r.into_iter().map(|d| value(&d)).collect());
            }
            Ok(out)
        })
    }

    fn exec(&mut self, sql: &str) -> Result<(), String> {
        let client = self.client.as_mut().ok_or("not connected")?;
        self.rt.block_on(async { client.simple_query(sql).await.map_err(|e| e.to_string())?.into_results().await.map(|_| ()).map_err(|e| e.to_string()) })
    }

    fn rows(&mut self, sql: &str) -> Result<Vec<Vec<Value>>, String> {
        Ok(self.query(sql, 1_000_000)?.rows)
    }
}

impl Driver for Ms {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "mssql", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": true })),
            "version" => {
                let v = self.rows("SELECT @@VERSION")?;
                ok(v.first().and_then(|r| r[0].as_str()).and_then(|s| s.lines().next()).unwrap_or("").trim().to_string())
            }
            "connect" => {
                let c: ConnectParams = params(p)?;
                let mut cfg = Config::new();
                cfg.host(&c.host);
                cfg.port(if c.port == 0 { 1433 } else { c.port });
                cfg.authentication(AuthMethod::sql_server(&c.user, c.password.as_deref().unwrap_or("")));
                if !c.database.trim().is_empty() {
                    cfg.database(c.database.trim());
                }
                cfg.application_name("qurry");
                // Verify certificates only with SSL "verify" (self-signed is common for in-house servers)
                match c.options.get("ssl").and_then(Value::as_str) {
                    Some(m) if m.starts_with("verify") => cfg.encryption(EncryptionLevel::Required),
                    Some(_) => {
                        cfg.encryption(EncryptionLevel::Required);
                        cfg.trust_cert();
                    }
                    None => cfg.trust_cert(),
                }
                let client = self.rt.block_on(async {
                    let tcp = tokio::time::timeout(std::time::Duration::from_secs(10), TcpStream::connect(cfg.get_addr())).await.map_err(|_| "connection timed out".to_string())?.map_err(|e| e.to_string())?;
                    tcp.set_nodelay(true).ok();
                    Client::connect(cfg, tcp.compat_write()).await.map_err(|e| e.to_string())
                })?;
                self.client = Some(client);
                self.read_only = c.options.get("readOnly").and_then(Value::as_bool).unwrap_or(false);
                ok(Value::Null)
            }
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("").to_string();
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                // Read-only (Read-only Data Source · MCP): run inside a transaction and always roll back (so writes · DDL the guard missed don't persist)
                if self.read_only || p.get("readOnly").and_then(Value::as_bool).unwrap_or(false) {
                    self.exec("SET XACT_ABORT ON; BEGIN TRANSACTION")?;
                    let r = self.query(&sql, limit);
                    let back = self.exec("IF @@TRANCOUNT > 0 ROLLBACK; SET XACT_ABORT OFF");
                    return ok(r.and_then(|o| back.map(|_| o))?);
                }
                ok(self.query(&sql, limit)?)
            }
            "databases" => ok(self.rows("SELECT name FROM sys.databases WHERE state = 0 ORDER BY name")?.into_iter().map(|r| r[0].clone()).collect::<Vec<_>>()),
            "schemas" => {
                let rows = self.rows("SELECT name FROM sys.schemas ORDER BY name")?;
                ok(rows
                    .into_iter()
                    .map(|r| {
                        let n = r[0].as_str().unwrap_or("").to_string();
                        json!({ "isSystem": n == "sys" || n == "INFORMATION_SCHEMA" || n == "guest" || n.starts_with("db_"), "name": n })
                    })
                    .collect::<Vec<_>>())
            }
            "objects" => {
                let s = lit(p.get("schema").and_then(Value::as_str).unwrap_or("dbo"));
                let rows = self.rows(&format!("SELECT TABLE_NAME, TABLE_TYPE FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = {s} ORDER BY TABLE_NAME"))?;
                let relations: Vec<Value> = rows.iter().map(|r| json!({ "name": r[0], "kind": if r[1] == "VIEW" { "view" } else { "table" }, "comment": null, "rowEstimate": null })).collect();
                let routines = self.rows(&format!("SELECT ROUTINE_NAME, ROUTINE_TYPE FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = {s} ORDER BY ROUTINE_NAME"))?;
                let routines: Vec<Value> = routines.iter().map(|r| json!({ "name": r[0], "args": "", "returns": null, "kind": if r[1] == "PROCEDURE" { "procedure" } else { "function" } })).collect();
                ok(json!({ "relations": relations, "routines": routines }))
            }
            "tableDetail" => {
                let (s, t) = (p.get("schema").and_then(Value::as_str).unwrap_or("dbo").to_string(), p.get("table").and_then(Value::as_str).unwrap_or("").to_string());
                let obj = lit(&format!("{}.{}", q(&s), q(&t)));
                let pk: Vec<String> = self
                    .rows(&format!("SELECT c.name FROM sys.indexes i JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id WHERE i.object_id = OBJECT_ID({obj}) AND i.is_primary_key = 1 ORDER BY ic.key_ordinal"))?
                    .into_iter()
                    .filter_map(|r| r[0].as_str().map(String::from))
                    .collect();
                let cols = self.rows(&format!(
                    "SELECT COLUMN_NAME, DATA_TYPE + CASE WHEN CHARACTER_MAXIMUM_LENGTH = -1 THEN '(max)' WHEN CHARACTER_MAXIMUM_LENGTH IS NOT NULL THEN '(' + CAST(CHARACTER_MAXIMUM_LENGTH AS varchar) + ')' WHEN DATA_TYPE IN ('decimal', 'numeric') THEN '(' + CAST(NUMERIC_PRECISION AS varchar) + ',' + CAST(NUMERIC_SCALE AS varchar) + ')' ELSE '' END, IS_NULLABLE, COLUMN_DEFAULT
                     FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
                    lit(&s),
                    lit(&t)
                ))?;
                let columns: Vec<Value> = cols.iter().map(|r| json!({ "name": r[0], "dataType": r[1], "nullable": r[2] == "YES", "defaultValue": r[3], "comment": null, "isPk": pk.iter().any(|x| Some(x.as_str()) == r[0].as_str()) })).collect();
                let mut constraints: Vec<Value> = vec![];
                if !pk.is_empty() {
                    let name = self.rows(&format!("SELECT name FROM sys.key_constraints WHERE parent_object_id = OBJECT_ID({obj}) AND type = 'PK'"))?;
                    constraints.push(json!({ "name": name.first().and_then(|r| r[0].as_str()).unwrap_or("PK"), "kind": "p", "definition": format!("PRIMARY KEY ({})", pk.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ")) }));
                }
                let fks = self.rows(&format!(
                    "SELECT fk.name, pc.name, OBJECT_SCHEMA_NAME(fk.referenced_object_id), OBJECT_NAME(fk.referenced_object_id), rc.name FROM sys.foreign_keys fk
                     JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id
                     JOIN sys.columns pc ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id
                     JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id
                     WHERE fk.parent_object_id = OBJECT_ID({obj}) ORDER BY fk.name, fkc.constraint_column_id"
                ))?;
                let mut groups: Vec<(String, Vec<String>, String, Vec<String>)> = vec![];
                for r in fks {
                    let (name, from, to_t, to) = (r[0].as_str().unwrap_or("").to_string(), r[1].as_str().unwrap_or("").to_string(), format!("{}.{}", q(r[2].as_str().unwrap_or("")), q(r[3].as_str().unwrap_or(""))), r[4].as_str().unwrap_or("").to_string());
                    match groups.last_mut() {
                        Some(g) if g.0 == name => {
                            g.1.push(from);
                            g.3.push(to);
                        }
                        _ => groups.push((name, vec![from], to_t, vec![to])),
                    }
                }
                for (name, from, to_t, to) in groups {
                    constraints.push(json!({ "name": name, "kind": "f", "definition": format!("FOREIGN KEY ({}) REFERENCES {to_t} ({})", from.iter().map(|c| q(c)).collect::<Vec<_>>().join(", "), to.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ")) }));
                }
                let idx = self.rows(&format!(
                    // STRING_AGG is 2017+: FOR XML PATH so older servers work too
                    "SELECT i.name, i.is_unique, i.is_primary_key, STUFF((SELECT ', ' + c.name FROM sys.index_columns ic JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
                       WHERE ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0 ORDER BY ic.key_ordinal FOR XML PATH('')), 1, 2, '')
                     FROM sys.indexes i WHERE i.object_id = OBJECT_ID({obj}) AND i.name IS NOT NULL"
                ))?;
                let indexes: Vec<Value> = idx
                    .iter()
                    .map(|r| json!({ "name": r[0], "isUnique": r[1], "isPrimary": r[2], "definition": format!("CREATE {}INDEX {} ON {}.{} ({})", if r[1] == json!(true) { "UNIQUE " } else { "" }, r[0].as_str().unwrap_or(""), q(&s), q(&t), r[3].as_str().unwrap_or("")) }))
                    .collect();
                ok(json!({ "columns": columns, "indexes": indexes, "constraints": constraints }))
            }
            "ddl" => {
                let (s, n) = (p.get("schema").and_then(Value::as_str).unwrap_or("dbo").to_string(), p.get("name").and_then(Value::as_str).unwrap_or("").to_string());
                let obj = lit(&format!("{}.{}", q(&s), q(&n)));
                // Views·routines have definitions. Tables are built from columns·keys
                let def = self.rows(&format!("SELECT OBJECT_DEFINITION(OBJECT_ID({obj}))"))?;
                if let Some(d) = def.first().and_then(|r| r[0].as_str()) {
                    return ok(format!("{}\n", d.trim()));
                }
                let d = self.call("tableDetail", json!({ "schema": s, "table": n }))?;
                let mut lines: Vec<String> = d["columns"].as_array().into_iter().flatten().map(|c| format!("    {} {}{}{}", q(c["name"].as_str().unwrap_or("")), c["dataType"].as_str().unwrap_or(""), c["defaultValue"].as_str().map(|v| format!(" DEFAULT {v}")).unwrap_or_default(), if c["nullable"] == json!(false) { " NOT NULL" } else { "" })).collect();
                for c in d["constraints"].as_array().into_iter().flatten() {
                    lines.push(format!("    CONSTRAINT {} {}", q(c["name"].as_str().unwrap_or("")), c["definition"].as_str().unwrap_or("")));
                }
                ok(format!("CREATE TABLE {}.{} (\n{}\n);\n", q(&s), q(&n), lines.join(",\n")))
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio");
    serve(Ms { rt, client: None, read_only: false });
}
