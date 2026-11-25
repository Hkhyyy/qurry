//! Database Admin reports and the ERD: dba_report, get_erd
use super::result_json;
use super::MAX_ROWS;
use crate::mask::mask_query_text;
use crate::Server;
use qurry_core::dba;
use qurry_core::erd::Erd;
use qurry_core::store::DbKind;
use serde_json::{json, Value};

impl Server {
    pub(super) async fn get_erd(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let schema_or = |kind: DbKind, db: &str| s("schema").unwrap_or_else(|| if kind == DbKind::Postgres { "public".into() } else { db.to_string() });
        let (t, kind, db, _) = self.target(a).await?;
        if kind == DbKind::Redis {
            return Err("An ERD is not available for Redis".into());
        }
        let schemas: Vec<String> = match a.get("schemas").and_then(|v| v.as_array()) {
            Some(v) => v.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            None => vec![schema_or(kind, &db)],
        };
        Ok(erd_json(st.erd(t, schemas).await.map_err(e)?, ERD_MAX_TABLES))
    }

    pub(super) async fn dba_report(&self, a: &Value) -> Result<Value, String> {
        let st = &self.state;
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(String::from);
        let e = |e: qurry_core::error::AppError| e.to_string();
        let (t, kind, ..) = self.target(a).await?;
        let report = s("report").ok_or("report is required")?;
        let id = qurry_core::app::split_target(&t).0.to_string();
        let c = self.sources().await?.into_iter().find(|c| c.config.id == id).ok_or("Data source not found")?.config;
        if !c.mcp_dba_report_allowed() {
            return Err(reports_not_allowed(&c.name, "Database Admin reports (sessions, client addresses, settings, roles)"));
        }
        let info = dba::reports(kind).into_iter().find(|r| r.id == report).ok_or_else(|| unknown_report(&report, kind))?;
        let mut r = st.dba_report(t, report).await.map_err(e)?;
        mask_query_text(&mut r, kind);
        if r.rows.len() > MAX_ROWS {
            r.rows.truncate(MAX_ROWS);
            r.truncated = true;
        }
        // Never the report's actions (Cancel Query, Terminate Session, generated SQL): MCP only reads
        Ok(json!({ "report": info.id, "title": info.title, "help": info.help, "result": result_json(&r) }))
    }
}

/// A Production source's admin reports (dba_report, Redis's SLOWLOG GET and CLIENT LIST through query) before a person allowed them
pub(super) fn reports_not_allowed(name: &str, what: &str) -> String {
    format!(
        "\"{name}\" is a Production data source: MCP reads its {what} only if a person allows it for this data source in Qurry (Data Source Properties › Options › \"AI tools can read admin reports\"; TUI: \"Admin reports\" in the data source form)"
    )
}

/// get_erd: at most this many tables
const ERD_MAX_TABLES: usize = 200;

/// Every report id of every kind, in the reports' order
pub(super) fn report_ids() -> Vec<&'static str> {
    let mut ids: Vec<&str> = dba::reports(DbKind::Postgres).iter().map(|r| r.id).collect();
    for r in dba::reports(DbKind::Mysql) {
        if !ids.contains(&r.id) {
            ids.push(r.id);
        }
    }
    ids
}

fn unknown_report(report: &str, kind: DbKind) -> String {
    let ids: Vec<&str> = dba::reports(kind).iter().map(|r| r.id).collect();
    if ids.is_empty() {
        return "Database Admin reports are available for PostgreSQL and MySQL data sources only".into();
    }
    format!("No report \"{report}\" for this data source. Reports: {}", ids.join(", "))
}

/// get_erd's answer: the first `cap` tables (in AppState::erd's order), the foreign keys whose both ends are kept, and Mermaid of that
fn erd_json(mut e: Erd, cap: usize) -> Value {
    let total = e.tables.len();
    e.tables.truncate(cap);
    let kept = |schema: &str, name: &str| e.tables.iter().any(|t| t.schema == schema && t.name == name);
    let relations: Vec<_> = e.relations.iter().filter(|r| kept(&r.from_schema, &r.from_table) && kept(&r.to_schema, &r.to_table)).cloned().collect();
    e.relations = relations;
    let mut out = json!({
        "mermaid": e.mermaid(),
        "tables": e.tables.iter().map(|t| json!({ "schema": t.schema, "name": t.name, "primary_key": t.columns.iter().filter(|c| c.pk).map(|c| &c.name).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "foreign_keys": e.relations.iter().map(|r| json!({
            "name": r.name, "from_schema": r.from_schema, "from_table": r.from_table, "from_columns": r.from_columns,
            "to_schema": r.to_schema, "to_table": r.to_table, "to_columns": r.to_columns
        })).collect::<Vec<_>>(),
    });
    if total > cap {
        out["note"] = format!("Showing {cap} of {total} tables; pass fewer schemas").into();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tests::{server_with, source};

    #[test]
    fn erd_keeps_the_first_tables_and_their_relations() {
        use qurry_core::erd::{ErdColumn, ErdRelation, ErdTable};
        let table = |i: usize| ErdTable { schema: "public".into(), name: format!("t{i:03}"), columns: vec![ErdColumn { name: "id".into(), data_type: "int".into(), pk: true, nullable: false }] };
        let fk = |from: usize, to: usize| ErdRelation {
            name: format!("fk_{from}_{to}"), from_schema: "public".into(), from_table: format!("t{from:03}"), from_columns: vec!["id".into()],
            to_schema: "public".into(), to_table: format!("t{to:03}"), to_columns: vec!["id".into()],
        };
        let big = Erd { tables: (0..250).map(table).collect(), relations: vec![fk(1, 0), fk(210, 0), fk(5, 220)] };
        let v = erd_json(big, 200);
        assert_eq!(v["tables"].as_array().unwrap().len(), 200);
        assert_eq!(v["tables"][199]["name"], "t199");
        assert_eq!(v["tables"][0]["primary_key"], json!(["id"]));
        assert_eq!(v["foreign_keys"], json!([{ "name": "fk_1_0", "from_schema": "public", "from_table": "t001", "from_columns": ["id"], "to_schema": "public", "to_table": "t000", "to_columns": ["id"] }]));
        assert_eq!(v["note"], "Showing 200 of 250 tables; pass fewer schemas");
        let m = v["mermaid"].as_str().unwrap();
        assert!(m.contains("t000 ||--o{ t001") && !m.contains("t210") && !m.contains("t220"), "{m}");
        let small = erd_json(Erd { tables: (0..3).map(table).collect(), relations: vec![fk(1, 0)] }, 200);
        assert!(small.get("note").is_none() && small["foreign_keys"].as_array().unwrap().len() == 1, "{small}");
    }

    /// Production without the per-source setting, and a source not shared with MCP, get no report; an unknown id lists the ones there are
    #[tokio::test]
    async fn dba_report_gates() {
        let mut allowed = source("4", "Prod allowed", Some("read"), true);
        allowed.config.options.mcp_dba_reports = true;
        let (server, dir) = server_with(&[source("1", "Dev", Some("read"), false), source("2", "Prod", Some("read"), true), source("3", "Hidden", Some("off"), false), allowed]);
        let report = |ds: &str, r: &str| json!({ "data_source": ds, "report": r });
        let e = server.call("dba_report", &report("Prod", "sessions")).await.unwrap_err();
        assert!(e.contains("Production") && e.contains("only if a person allows it for this data source in Qurry"), "{e}");
        let e = server.call("dba_report", &report("Hidden", "sessions")).await.unwrap_err();
        assert!(e.contains("is not shared with MCP") && !e.contains("db.internal"), "{e}");
        for ds in ["Dev", "Prod allowed"] {
            let e = server.call("dba_report", &report(ds, "drop_everything")).await.unwrap_err();
            assert!(e.starts_with("No report \"drop_everything\"") && e.contains("sessions, locks, slow_queries") && e.contains("vacuum_progress"), "{e}");
            // PostgreSQL has no fragmentation (a MySQL report)
            let e = server.call("dba_report", &report(ds, "fragmentation")).await.unwrap_err();
            assert!(e.starts_with("No report \"fragmentation\""), "{e}");
        }
        let list = server.call("list_data_sources", &json!({})).await.unwrap();
        let flag = |name: &str| list.as_array().unwrap().iter().find(|c| c["name"] == name).map(|c| c["dba_report"].clone());
        assert_eq!((flag("Dev"), flag("Prod"), flag("Prod allowed"), flag("Hidden")), (Some(json!(true)), Some(json!(false)), Some(json!(true)), None));
        let _ = std::fs::remove_dir_all(dir);
    }
}
