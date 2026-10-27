//! Qurry 드라이버: Oracle. 실행할 때 Oracle Instant Client 가 필요하다 (ODPI-C 가 libclntsh 를 연다).
//! database = 서비스 이름 (FREEPDB1, ORCLPDB1 …), 스키마 = 사용자. 자동 커밋
use oracle::sql_type::OracleType;
use oracle::{Connection, InitParams};
use qurry_driver::{Column, ConnectParams, Driver, QueryOut, ok, params, serve};
use serde_json::{Value, json};

#[derive(Default)]
struct Ora {
    conn: Option<Connection>,
    service: String,
}

fn returns_rows(sql: &str) -> bool {
    let w = sql.trim_start().split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("").to_lowercase();
    ["select", "with"].contains(&w.as_str())
}

fn numeric(t: &OracleType) -> bool {
    matches!(t, OracleType::Number(..) | OracleType::Float(_) | OracleType::BinaryFloat | OracleType::BinaryDouble | OracleType::Int64 | OracleType::UInt64)
}

/// Instant Client 위치: 설정 → ORACLE_CLIENT_LIB_DIR → OCI_LIB_DIR → (없으면 시스템 기본 경로)
fn init_client(dir: Option<&str>) -> Result<(), String> {
    let dir = dir.map(String::from).or_else(|| std::env::var("ORACLE_CLIENT_LIB_DIR").ok()).or_else(|| std::env::var("OCI_LIB_DIR").ok());
    if InitParams::is_initialized() {
        return Ok(());
    }
    let mut p = InitParams::new();
    if let Some(d) = dir.filter(|d| !d.is_empty()) {
        p.oracle_client_lib_dir(d).map_err(|e| e.to_string())?;
    }
    p.init().map(|_| ()).map_err(|e| {
        format!("Oracle Instant Client was not found ({e}). Install it from https://www.oracle.com/database/technologies/instant-client/downloads.html and set its folder in the data source (Options › Oracle client) or ORACLE_CLIENT_LIB_DIR.")
    })
}

impl Ora {
    fn db(&self) -> Result<&Connection, String> {
        self.conn.as_ref().ok_or_else(|| "not connected".to_string())
    }

    fn query(&self, sql: &str, limit: usize) -> Result<QueryOut, String> {
        let db = self.db()?;
        // SQL 문장 끝 ; 는 Oracle 이 받지 않는다 (PL/SQL 블록은 END; 까지 그대로)
        let t = sql.trim();
        let upper = t.to_uppercase();
        let plsql = upper.starts_with("BEGIN") || upper.starts_with("DECLARE") || (upper.starts_with("CREATE") && ["PROCEDURE", "FUNCTION", "PACKAGE", "TRIGGER", "TYPE BODY"].iter().any(|k| upper.contains(k)));
        let sql = if plsql { t } else { t.trim_end_matches(';') };
        if !returns_rows(sql) {
            let st = db.execute(sql, &[]).map_err(|e| e.to_string())?;
            return Ok(QueryOut { rows_affected: st.row_count().unwrap_or(0), ..Default::default() });
        }
        let rows = db.query(sql, &[]).map_err(|e| e.to_string())?;
        let info = rows.column_info().to_vec();
        let mut out = QueryOut { columns: info.iter().map(|c| Column { name: c.name().to_string(), type_name: c.oracle_type().to_string().to_uppercase() }).collect(), has_result_set: true, ..Default::default() };
        for r in rows {
            if out.rows.len() >= limit {
                out.truncated = true;
                break;
            }
            let r = r.map_err(|e| e.to_string())?;
            out.rows.push(
                r.sql_values()
                    .iter()
                    .zip(&info)
                    .map(|(v, c)| match v.get::<Option<String>>() {
                        Ok(Some(s)) if numeric(c.oracle_type()) => s.parse::<i64>().map(|i| json!(i)).unwrap_or(Value::String(s)),
                        Ok(Some(s)) => Value::String(s),
                        _ => Value::Null,
                    })
                    .collect(),
            );
        }
        Ok(out)
    }

    fn rows(&self, sql: &str) -> Result<Vec<Vec<Value>>, String> {
        Ok(self.query(sql, 1_000_000)?.rows)
    }
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

impl Driver for Ora {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "hello" => ok(json!({ "name": "oracle", "version": env!("CARGO_PKG_VERSION"), "protocol": qurry_driver::PROTOCOL, "sql": true })),
            "connect" => {
                let c: ConnectParams = params(p)?;
                init_client(c.options.get("oracleClient").and_then(Value::as_str))?;
                // host 에 이미 완성된 연결 글(TNS·EZConnect)이 있으면 그대로
                let cs = if c.host.contains('/') || c.host.contains('(') { c.host.clone() } else { format!("//{}:{}/{}", c.host, if c.port == 0 { 1521 } else { c.port }, c.database.trim()) };
                let mut conn = Connection::connect(&c.user, c.password.as_deref().unwrap_or(""), &cs).map_err(|e| e.to_string())?;
                conn.set_autocommit(true);
                self.service = c.database.trim().to_string();
                self.conn = Some(conn);
                ok(Value::Null)
            }
            "query" => {
                let sql = p.get("sql").and_then(Value::as_str).unwrap_or("");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                ok(self.query(sql, limit)?)
            }
            // Oracle 은 접속한 서비스 하나
            "databases" => ok(vec![self.service.clone()]),
            "schemas" => {
                let rows = self.rows("SELECT username, oracle_maintained FROM all_users ORDER BY username")?;
                ok(rows.into_iter().map(|r| json!({ "name": r[0], "isSystem": r[1] == "Y" })).collect::<Vec<_>>())
            }
            "objects" => {
                let owner = lit(p.get("schema").and_then(Value::as_str).unwrap_or(""));
                let rows = self.rows(&format!("SELECT object_name, object_type FROM all_objects WHERE owner = {owner} AND object_type IN ('TABLE', 'VIEW', 'MATERIALIZED VIEW', 'SEQUENCE', 'FUNCTION', 'PROCEDURE') AND object_name NOT LIKE 'BIN$%' ORDER BY object_name"))?;
                let mut relations = vec![];
                let mut routines = vec![];
                for r in rows {
                    match r[1].as_str().unwrap_or("") {
                        "TABLE" => relations.push(json!({ "name": r[0], "kind": "table", "comment": null, "rowEstimate": null })),
                        "VIEW" => relations.push(json!({ "name": r[0], "kind": "view", "comment": null, "rowEstimate": null })),
                        "MATERIALIZED VIEW" => relations.push(json!({ "name": r[0], "kind": "matview", "comment": null, "rowEstimate": null })),
                        "SEQUENCE" => relations.push(json!({ "name": r[0], "kind": "sequence", "comment": null, "rowEstimate": null })),
                        k => routines.push(json!({ "name": r[0], "args": "", "returns": null, "kind": k.to_lowercase() })),
                    }
                }
                ok(json!({ "relations": relations, "routines": routines }))
            }
            "tableDetail" => {
                let (owner, t) = (lit(p.get("schema").and_then(Value::as_str).unwrap_or("")), lit(p.get("table").and_then(Value::as_str).unwrap_or("")));
                let cons = self.rows(&format!(
                    "SELECT c.constraint_name, c.constraint_type, LISTAGG(cc.column_name, ', ') WITHIN GROUP (ORDER BY cc.position), MAX(r.owner || '.' || r.table_name),
                            MAX((SELECT LISTAGG(rc.column_name, ', ') WITHIN GROUP (ORDER BY rc.position) FROM all_cons_columns rc WHERE rc.owner = c.r_owner AND rc.constraint_name = c.r_constraint_name))
                     FROM all_constraints c JOIN all_cons_columns cc ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name
                     LEFT JOIN all_constraints r ON r.owner = c.r_owner AND r.constraint_name = c.r_constraint_name
                     WHERE c.owner = {owner} AND c.table_name = {t} AND c.constraint_type IN ('P', 'R', 'U')
                     GROUP BY c.constraint_name, c.constraint_type"
                ))?;
                let pk: Vec<String> = cons.iter().filter(|c| c[1] == "P").flat_map(|c| c[2].as_str().unwrap_or("").split(", ").map(String::from).collect::<Vec<_>>()).collect();
                let cols = self.rows(&format!(
                    "SELECT column_name, data_type || CASE WHEN data_type IN ('VARCHAR2', 'NVARCHAR2', 'CHAR', 'RAW') THEN '(' || char_length || ')' WHEN data_type = 'NUMBER' AND data_precision IS NOT NULL THEN '(' || data_precision || ',' || data_scale || ')' ELSE '' END,
                            nullable, data_default FROM all_tab_columns WHERE owner = {owner} AND table_name = {t} ORDER BY column_id"
                ))?;
                let columns: Vec<Value> = cols.iter().map(|r| json!({ "name": r[0], "dataType": r[1], "nullable": r[2] == "Y", "defaultValue": r[3].as_str().map(|d| d.trim()), "comment": null, "isPk": pk.iter().any(|x| Some(x.as_str()) == r[0].as_str()) })).collect();
                let constraints: Vec<Value> = cons
                    .iter()
                    .map(|c| match c[1].as_str().unwrap_or("") {
                        "P" => json!({ "name": c[0], "kind": "p", "definition": format!("PRIMARY KEY ({})", c[2].as_str().unwrap_or("")) }),
                        "U" => json!({ "name": c[0], "kind": "u", "definition": format!("UNIQUE ({})", c[2].as_str().unwrap_or("")) }),
                        _ => json!({ "name": c[0], "kind": "f", "definition": format!("FOREIGN KEY ({}) REFERENCES {} ({})", c[2].as_str().unwrap_or(""), c[3].as_str().unwrap_or(""), c[4].as_str().unwrap_or("")) }),
                    })
                    .collect();
                let idx = self.rows(&format!("SELECT i.index_name, i.uniqueness, LISTAGG(ic.column_name, ', ') WITHIN GROUP (ORDER BY ic.column_position) FROM all_indexes i JOIN all_ind_columns ic ON ic.index_owner = i.owner AND ic.index_name = i.index_name WHERE i.table_owner = {owner} AND i.table_name = {t} GROUP BY i.index_name, i.uniqueness"))?;
                let indexes: Vec<Value> = idx.iter().map(|r| json!({ "name": r[0], "isUnique": r[1] == "UNIQUE", "isPrimary": cons.iter().any(|c| c[0] == r[0] && c[1] == "P"), "definition": format!("CREATE {}INDEX {} ON {} ({})", if r[1] == "UNIQUE" { "UNIQUE " } else { "" }, r[0].as_str().unwrap_or(""), p.get("table").and_then(Value::as_str).unwrap_or(""), r[2].as_str().unwrap_or("")) })).collect();
                ok(json!({ "columns": columns, "indexes": indexes, "constraints": constraints }))
            }
            "ddl" => {
                let owner = lit(p.get("schema").and_then(Value::as_str).unwrap_or(""));
                let name = lit(p.get("name").and_then(Value::as_str).unwrap_or(""));
                let ty = self.rows(&format!("SELECT object_type FROM all_objects WHERE owner = {owner} AND object_name = {name} AND object_type NOT IN ('INDEX', 'LOB') AND ROWNUM = 1"))?;
                let ty = ty.first().and_then(|r| r[0].as_str()).unwrap_or("TABLE").replace(' ', "_");
                let r = self.rows(&format!("SELECT DBMS_METADATA.GET_DDL({}, {name}, {owner}) FROM dual", lit(&ty)))?;
                ok(r.first().and_then(|x| x[0].as_str()).map(|s| format!("{};\n", s.trim())).unwrap_or_default())
            }
            m => Err(format!("unknown method: {m}")),
        }
    }
}

fn main() {
    serve(Ora::default());
}
