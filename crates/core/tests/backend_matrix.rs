//! Backend matrix: pins what each operation that goes through `Pool` returns on PostgreSQL, MySQL and Redis,
//! so a refactor of the dispatch shows up as a failing assertion instead of a changed screen.
//! Real-server tests are #[ignore]d and read the same env vars as CI:
//!   QURRY_TEST_PG=host:port:user:db · QURRY_TEST_MY=host:port:user:password:db · QURRY_TEST_REDIS=host:port (default 127.0.0.1:16379)
//! cargo test -p qurry-core --test backend_matrix -- --ignored
use qurry_core::app::AppState;
use qurry_core::db::{self, Pool};
use qurry_core::hints::HintReq;
use qurry_core::store::{ConnectionConfig, DbKind};
use qurry_core::transfer::{Dest, Format};
use serde_json::{Value, json};

const ID: &str = "m";

fn cfg(kind: &str, host: &str, port: &str, user: &str, database: &str) -> ConnectionConfig {
    serde_json::from_value(json!({
        "id": ID, "name": "matrix", "kind": kind, "host": host, "port": port.parse::<u16>().unwrap(), "user": user, "database": database,
        "options": { "confirmChanges": true },
    }))
    .unwrap()
}

/// QURRY_TEST_PG=host:port:user:db
fn pg_env() -> Option<ConnectionConfig> {
    let e = std::env::var("QURRY_TEST_PG").ok()?;
    let p: Vec<&str> = e.split(':').collect();
    Some(cfg("postgres", p[0], p[1], p[2], p[3]))
}

/// QURRY_TEST_MY=host:port:user:password:db → (source, password)
fn my_env() -> Option<(ConnectionConfig, String)> {
    let e = std::env::var("QURRY_TEST_MY").ok()?;
    let p: Vec<&str> = e.split(':').collect();
    Some((cfg("mysql", p[0], p[1], p[2], p[4]), p[3].to_string()))
}

/// QURRY_TEST_REDIS=host:port, on its own DB (the redisdb tests use 0 and 3–8)
fn redis_env(db: &str) -> ConnectionConfig {
    let addr = std::env::var("QURRY_TEST_REDIS").unwrap_or_else(|_| "127.0.0.1:16379".into());
    let (host, port) = addr.rsplit_once(':').expect("QURRY_TEST_REDIS=host:port");
    cfg("redis", host, port, "", db)
}

/// An AppState whose config folder is removed when the test drops it,
/// so the saved password and the key that decrypts it don't outlive the test
struct Matrix {
    st: AppState,
    // Declared after `st`: the folder goes after the state is dropped
    dir: tempfile::TempDir,
}

impl std::ops::Deref for Matrix {
    type Target = AppState;
    fn deref(&self) -> &AppState {
        &self.st
    }
}

/// A fresh config folder with one saved source (id "m"; Production audit on)
async fn state(c: &ConnectionConfig, password: Option<String>) -> Matrix {
    let dir = tempfile::Builder::new().prefix("qurry-matrix-").tempdir().unwrap();
    let st = qurry_core::open_state(Some(dir.path().to_path_buf())).unwrap();
    st.save_connection(c.clone(), password, None).await.unwrap();
    Matrix { st, dir }
}

async fn q(st: &AppState, sql: &str) -> db::QueryResult {
    st.run_query(ID.into(), sql.into(), None, None).await.unwrap_or_else(|e| panic!("{sql}: {e}"))
}

/// Run `sql` in console tab `tab_id`
async fn q_tab(st: &AppState, tab_id: &str, sql: &str) -> db::QueryResult {
    st.run_query(ID.into(), sql.into(), None, Some(tab_id.into())).await.unwrap_or_else(|e| panic!("{sql}: {e}"))
}

async fn export(st: &AppState, sql: &str, format: Format, table: Option<(&str, &str)>) -> String {
    let table = table.map(|(s, t)| (s.to_string(), t.to_string()));
    let r = st.export(ID.into(), sql.into(), format, Dest::Clipboard, table, format!("{format:?}")).await.unwrap();
    r.text.unwrap()
}

/// Search the given tables for "needle"
fn fts(tables: Vec<(&str, &str)>) -> qurry_core::meta::FtsRequest {
    let tables: Vec<(String, String)> = tables.into_iter().map(|(s, t)| (s.into(), t.into())).collect();
    serde_json::from_value(json!({ "text": "needle", "mode": "contains", "matchCase": false, "columns": "text", "tables": tables, "limit": 10 })).unwrap()
}

fn hint(schema: &str, table: &str, column: &str) -> HintReq {
    serde_json::from_value(json!({ "schema": schema, "table": table, "column": column })).unwrap()
}

fn err<T>(r: qurry_core::error::AppResult<T>) -> String {
    match r {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.to_string(),
    }
}

// ───────── Dispatch decided before any connection ─────────

/// A source that would refuse connections: these checks must answer without connecting
async fn offline(kind: &str) -> Matrix {
    let mut c = cfg(kind, "127.0.0.1", "1", "u", "d");
    c.id = kind.into();
    state(&c, None).await
}

#[tokio::test]
async fn config_folder_is_removed_with_the_state() {
    let st = state(&cfg("mysql", "127.0.0.1", "1", "u", "d"), Some("pw".into())).await;
    let dir = st.dir.path().to_path_buf();
    assert!(dir.join("secrets.json").exists() && dir.join("secret.key").exists(), "the password was saved");
    drop(st);
    assert!(!dir.exists(), "{dir:?} left behind");
}

#[tokio::test]
async fn compare_refuses_mixed_kinds_and_redis_offline() {
    let st = offline("postgres").await;
    let mut r = cfg("redis", "127.0.0.1", "1", "", "db0");
    r.id = "redis".into();
    st.save_connection(r, None, None).await.unwrap();
    let e = err(st.compare_schemas("postgres".into(), "public".into(), "redis".into(), "db0".into()).await);
    assert!(e.contains("same kind"), "{e}");
    let e = err(st.compare_data("redis".into(), "db0".into(), "a".into(), "redis".into(), "db0".into(), "a".into()).await);
    assert!(e.contains("same kind"), "{e}");
}

#[tokio::test]
async fn export_and_fake_fill_check_arguments_offline() {
    let st = offline("mysql").await;
    let e = err(st.export("mysql".into(), "select 1".into(), Format::Xlsx, Dest::Clipboard, None, "j".into()).await);
    assert!(e.contains("import only"), "{e}");
    let e = err(st.fake_fill("mysql".into(), "d".into(), "t".into(), 0).await);
    assert!(e.contains("Choose 1 to"), "{e}");
}

/// Redis has one schema per DB, named after it; the explorer never asks the server
#[test]
fn redis_schema_is_the_db_name() {
    let s = qurry_core::redisdb::list_schemas("db11");
    assert_eq!(s.iter().map(|s| (s.name.as_str(), s.is_system)).collect::<Vec<_>>(), [("db11", false)]);
    assert_eq!(qurry_core::redisdb::db_index("db11"), 11);
    assert_eq!(qurry_core::redisdb::db_index(""), 0);
}

/// What the audit log counts as a change, per kind
#[test]
fn audit_change_per_kind() {
    use qurry_core::audit::is_change;
    assert!(is_change("update t set a = 1", DbKind::Postgres));
    assert!(is_change("drop table t", DbKind::Mysql));
    assert!(!is_change("select 1", DbKind::Postgres));
    assert!(!is_change("set names utf8mb4", DbKind::Mysql));
    assert!(!is_change("begin", DbKind::Postgres));
    assert!(is_change("SET k v", DbKind::Redis), "SET is a write in Redis");
    assert!(!is_change("GET k", DbKind::Redis));
    assert!(!is_change("CONFIG GET maxmemory", DbKind::Redis));
}

/// SQL INSERT export quotes columns the way the target kind does
#[test]
fn sql_export_quotes_per_kind() {
    use qurry_core::transfer::Encoder;
    let line = |kind| {
        let mut e = Encoder::new(Format::Sql, kind, "t");
        e.header(&["a b".into()]);
        e.row(&[Value::from("it's")])
    };
    assert_eq!(line(DbKind::Postgres), "INSERT INTO t (\"a b\") VALUES ('it''s');\n");
    assert_eq!(line(DbKind::Mysql), "INSERT INTO t (`a b`) VALUES ('it''s');\n");
}

// ───────── PostgreSQL ─────────

#[tokio::test]
#[ignore]
async fn pg_connect_and_query() {
    let Some(c) = pg_env() else { return };
    let pool = db::connect(&c, None).await.unwrap();
    assert!(matches!(pool, Pool::Pg(_)));
    let v = pool.server_version().await.unwrap();
    assert!(v.starts_with("PostgreSQL "), "{v}");

    let r = pool.run_query("select g, 'v' || g as t from generate_series(1, 10) g", 3).await.unwrap();
    let cols: Vec<(&str, &str)> = r.columns.iter().map(|c| (c.name.as_str(), c.type_name.as_str())).collect();
    assert_eq!(cols, [("g", "INT4"), ("t", "TEXT")]);
    assert_eq!(r.rows, [[json!(1), json!("v1")], [json!(2), json!("v2")], [json!(3), json!("v3")]]);
    assert!(r.truncated && r.has_result_set && !r.in_transaction);
    let all = pool.run_query("select g from generate_series(1, 3) g", 3).await.unwrap();
    assert!(!all.truncated, "exactly limit rows is not truncated");
    let empty = pool.run_query("select 1 as a where false", 10).await.unwrap();
    assert!(empty.has_result_set && empty.rows.is_empty() && empty.columns.len() == 1);
    let ddl = pool.run_query("create temp table qm_t (a int)", 10).await.unwrap();
    assert!(!ddl.has_result_set && ddl.columns.is_empty());
    assert!(pool.run_query("select * from qm_no_such_table", 10).await.is_err());
}

#[tokio::test]
#[ignore]
async fn pg_session_fetch_cancel_tx() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    let tab = || Some("pg-tab".to_string());
    let r1 = st.run_query(ID.into(), "select g from generate_series(1, 1200) g".into(), Some(500), tab()).await.unwrap();
    assert_eq!((r1.rows.len(), r1.truncated, r1.rows[0][0].clone()), (500, true, json!(1)));
    let r2 = st.fetch_more("pg-tab".into(), Some(500)).await.unwrap();
    assert_eq!((r2.rows.len(), r2.truncated, r2.rows[0][0].clone()), (500, true, json!(501)));
    let r3 = st.fetch_more("pg-tab".into(), Some(500)).await.unwrap();
    assert_eq!((r3.rows.len(), r3.truncated), (200, false));
    assert!(st.fetch_more("pg-tab".into(), Some(500)).await.is_err(), "no more rows");

    // Cancel a running query from another task: the query fails, the session stays usable
    let st = std::sync::Arc::new(st);
    let st2 = st.clone();
    let cancel = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        st2.cancel_query("pg-tab".into()).await.unwrap()
    });
    let t0 = std::time::Instant::now();
    let slow = st.run_query(ID.into(), "select pg_sleep(10)".into(), None, tab()).await;
    assert!(cancel.await.unwrap());
    assert!(err(slow).contains("canceling statement"));
    assert!(t0.elapsed().as_secs() < 5);
    assert!(!st.cancel_query("pg-no-tab".into()).await.unwrap(), "unknown tab");

    // Manual: uncommitted rows are invisible to the pool, Roll back drops them, Commit keeps them
    q(&st, "drop table if exists qm_tx").await;
    q(&st, "create table qm_tx (id int primary key)").await;
    st.set_tx_mode(ID.into(), "pg-tab".into(), true).await.unwrap();
    let ins = st.run_query(ID.into(), "insert into qm_tx values (1)".into(), None, tab()).await.unwrap();
    assert!(ins.in_transaction && ins.rows_affected == 1);
    assert!(st.tx_state("pg-tab".into()).await);
    let count = |st: std::sync::Arc<Matrix>| async move { q(&st, "select count(*)::int from qm_tx").await.rows[0][0].clone() };
    assert_eq!(count(st.clone()).await, json!(0));
    st.rollback_tx("pg-tab".into()).await.unwrap();
    assert!(!st.tx_state("pg-tab".into()).await);
    st.run_query(ID.into(), "insert into qm_tx values (2)".into(), None, tab()).await.unwrap();
    st.commit_tx("pg-tab".into()).await.unwrap();
    assert_eq!(count(st.clone()).await, json!(1));
    st.set_tx_mode(ID.into(), "pg-tab".into(), false).await.unwrap();
    st.close_session("pg-tab".into()).await.unwrap();
    q(&st, "drop table qm_tx").await;
}

async fn pg_fixture(st: &AppState, schema: &str) {
    for s in [
        format!("drop schema if exists {schema} cascade"),
        format!("create schema {schema}"),
        format!("create table {schema}.customers (id serial primary key, name text not null default 'anon', tier text)"),
        format!("comment on table {schema}.customers is 'People'"),
        format!("create index customers_tier on {schema}.customers (tier)"),
        format!("create table {schema}.orders (id int primary key, customer_id int not null references {schema}.customers (id), total numeric(10,2))"),
        format!("create view {schema}.big_orders as select id, total from {schema}.orders where total > 100"),
        format!("create function {schema}.twice(x int) returns int language sql as 'select x * 2'"),
        format!("insert into {schema}.customers (name, tier) values ('a', 'gold'), ('b', 'gold'), ('c', 'silver')"),
        format!("insert into {schema}.orders values (1, 1, 50), (2, 1, 150)"),
    ] {
        q(st, &s).await;
    }
}

#[tokio::test]
#[ignore]
async fn pg_metadata() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    pg_fixture(&st, "qm_meta").await;

    let dbs = st.list_databases(ID.into()).await.unwrap();
    assert!(dbs.contains(&"postgres".to_string()) && !dbs.iter().any(|d| d.starts_with("template")), "{dbs:?}");
    let schemas = st.list_schemas(ID.into()).await.unwrap();
    let flag = |n: &str| schemas.iter().find(|s| s.name == n).map(|s| s.is_system);
    assert_eq!((flag("qm_meta"), flag("public"), flag("pg_catalog"), flag("information_schema")), (Some(false), Some(false), Some(true), Some(true)));
    assert!(schemas.last().unwrap().is_system, "system schemas last");

    let o = st.list_objects(ID.into(), "qm_meta".into()).await.unwrap();
    let rels: Vec<(&str, &str)> = o.relations.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect();
    assert_eq!(rels, [("big_orders", "view"), ("customers", "table"), ("customers_id_seq", "sequence"), ("orders", "table")]);
    assert_eq!(o.relations[1].comment.as_deref(), Some("People"));
    let r = &o.routines[0];
    assert_eq!((o.routines.len(), r.name.as_str(), r.args.as_str(), r.returns.as_deref(), r.kind.as_str()), (1, "twice", "x integer", Some("integer"), "function"));

    let d = st.table_detail(ID.into(), "qm_meta".into(), "customers".into()).await.unwrap();
    let cols: Vec<(&str, &str, bool, bool)> = d.columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str(), c.nullable, c.is_pk)).collect();
    assert_eq!(cols, [("id", "integer", false, true), ("name", "text", false, false), ("tier", "text", true, false)]);
    assert_eq!(d.columns[1].default_value.as_deref(), Some("'anon'::text"));
    let idx: Vec<(&str, bool, bool)> = d.indexes.iter().map(|i| (i.name.as_str(), i.is_unique, i.is_primary)).collect();
    assert_eq!(idx, [("customers_pkey", true, true), ("customers_tier", false, false)]);
    let cons: Vec<(&str, &str, &str)> = d.constraints.iter().map(|c| (c.name.as_str(), c.kind.as_str(), c.definition.as_str())).collect();
    assert_eq!(cons, [("customers_pkey", "p", "PRIMARY KEY (id)")]);
    let fk = st.table_detail(ID.into(), "qm_meta".into(), "orders".into()).await.unwrap();
    assert!(fk.constraints.iter().any(|c| c.kind == "f" && c.definition == "FOREIGN KEY (customer_id) REFERENCES qm_meta.customers(id)"), "{:?}", fk.constraints.iter().map(|c| &c.definition).collect::<Vec<_>>());

    let ddl = st.object_ddl(ID.into(), "qm_meta".into(), "customers".into(), "table".into()).await.unwrap();
    assert!(ddl.starts_with("CREATE TABLE qm_meta.customers\n(\n    id integer DEFAULT nextval("), "{ddl}");
    assert!(ddl.contains("    name text DEFAULT 'anon'::text NOT NULL,\n") && ddl.contains("    CONSTRAINT customers_pkey PRIMARY KEY (id)\n);\n"), "{ddl}");
    assert!(ddl.contains("\nCREATE INDEX customers_tier ON qm_meta.customers USING btree (tier);\n") && ddl.contains("COMMENT ON TABLE qm_meta.customers IS 'People';"), "{ddl}");
    let view = st.object_ddl(ID.into(), "qm_meta".into(), "big_orders".into(), "view".into()).await.unwrap();
    assert!(view.starts_with("CREATE OR REPLACE VIEW qm_meta.big_orders AS\n") && view.ends_with(";\n"), "{view}");
    let seq = st.object_ddl(ID.into(), "qm_meta".into(), "customers_id_seq".into(), "sequence".into()).await.unwrap();
    assert!(seq.starts_with("CREATE SEQUENCE qm_meta.customers_id_seq\n    START WITH 1\n    INCREMENT BY 1\n"), "{seq}");
    let f = st.object_ddl(ID.into(), "qm_meta".into(), "twice".into(), "routine".into()).await.unwrap();
    assert!(f.starts_with("CREATE OR REPLACE FUNCTION qm_meta.twice(x integer)"), "{f}");

    let comp = st.completion_meta(ID.into()).await.unwrap();
    let t = comp.iter().find(|t| t.schema == "qm_meta" && t.name == "customers").expect("customers in completion");
    assert_eq!(t.columns, [("id".to_string(), "integer".to_string()), ("name".into(), "text".into()), ("tier".into(), "text".into())]);
    assert!(comp.iter().any(|t| t.schema == "qm_meta" && t.name == "big_orders"), "views too");
    assert!(!comp.iter().any(|t| t.schema == "pg_catalog"));

    let idx = st.object_index(ID.into()).await.unwrap();
    let mut mine: Vec<(&str, &str)> = idx.iter().filter(|e| e.schema == "qm_meta").map(|e| (e.name.as_str(), e.kind.as_str())).collect();
    mine.sort();
    assert_eq!(mine, [("big_orders", "view"), ("customers", "table"), ("customers_id_seq", "sequence"), ("orders", "table"), ("qm_meta", "schema"), ("twice", "function")]);
    q(&st, "drop schema qm_meta cascade").await;
}

#[tokio::test]
#[ignore]
async fn pg_tools() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    pg_fixture(&st, "qm_tools").await;

    // Hints: most frequent first
    let h = st.column_hints(ID.into(), hint("qm_tools", "customers", "tier")).await.unwrap();
    assert_eq!(h.iter().map(|h| (h.value.as_str(), h.count)).collect::<Vec<_>>(), [("gold", 2), ("silver", 1)]);

    // ERD
    let e = st.erd(ID.into(), vec!["qm_tools".into()]).await.unwrap();
    assert_eq!(e.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["customers", "orders"], "tables only, no views");
    let r = &e.relations[0];
    assert_eq!((e.relations.len(), r.from_table.as_str(), r.from_columns.clone(), r.to_table.as_str(), r.to_columns.clone()), (1, "orders", vec!["customer_id".to_string()], "customers", vec!["id".to_string()]));
    assert!(e.tables[0].columns[0].pk && !e.tables[0].columns[2].pk);

    // Export
    let sql = "select id, name, tier from qm_tools.customers order by id limit 2";
    assert_eq!(export(&st, sql, Format::Csv, None).await, "id,name,tier\n1,a,gold\n2,b,gold\n");
    assert_eq!(export(&st, sql, Format::Json, None).await, "[\n  {\"id\":1,\"name\":\"a\",\"tier\":\"gold\"},\n  {\"id\":2,\"name\":\"b\",\"tier\":\"gold\"}\n]\n");
    assert_eq!(
        export(&st, sql, Format::Sql, Some(("qm_tools", "customers"))).await,
        "INSERT INTO qm_tools.customers (id, name, tier) VALUES (1, 'a', 'gold');\nINSERT INTO qm_tools.customers (id, name, tier) VALUES (2, 'b', 'gold');\n"
    );
    assert_eq!(export(&st, sql, Format::Markdown, None).await, "| id | name | tier |\n| --- | --- | --- |\n| 1 | a | gold |\n| 2 | b | gold |\n");

    // Test data: preview inserts nothing, fill inserts with valid foreign keys
    let p = st.fake_preview(ID.into(), "qm_tools".into(), "orders".into(), 3).await.unwrap();
    assert_eq!(p.plan.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "customer_id", "total"]);
    assert_eq!((p.result.rows.len(), p.result.columns.len()), (3, 3));
    assert_eq!(st.fake_fill(ID.into(), "qm_tools".into(), "orders".into(), 5).await.unwrap().rows, 5);
    assert_eq!(q(&st, "select count(*)::int from qm_tools.orders").await.rows[0][0], json!(7));

    // Schema compare: the target gets the missing table and column
    for s in ["drop schema if exists qm_dst cascade", "create schema qm_dst", "create table qm_dst.customers (id serial primary key, name text not null default 'anon')"] {
        q(&st, s).await;
    }
    let ch = st.compare_schemas(ID.into(), "qm_tools".into(), ID.into(), "qm_dst".into()).await.unwrap();
    let kinds: Vec<(&str, &str)> = ch.iter().map(|c| (c.kind.as_str(), c.object.as_str())).collect();
    assert!(kinds.contains(&("add_column", "customers.tier")) && kinds.contains(&("create_table", "orders")) && kinds.contains(&("create_view", "big_orders")), "{kinds:?}");
    assert!(ch.iter().all(|c| !c.destructive));

    // Audit (Production audit on): a change is logged with its undo SQL
    q(&st, "update qm_tools.customers set tier = 'bronze' where id = 3").await;
    let a = st.audit_log(Some(ID.into()), String::new(), 1).remove(0);
    assert_eq!((a.sql.as_str(), a.rows, a.error.is_none(), a.in_tx), ("update qm_tools.customers set tier = 'bronze' where id = 3", Some(1), true, false));
    let u = a.undo.as_deref().unwrap_or_default();
    assert!(u.contains("UPDATE qm_tools.customers AS qurry_t SET tier = qurry_old.tier") && u.contains(r#""tier": "silver""#), "{u}");
    q(&st, "drop schema qm_tools cascade").await;
    q(&st, "drop schema qm_dst cascade").await;
}

/// Migrations, import, export progress and the undo of a delete on a table with a generated column, in the source's own
/// database (the history table is per database), with the import into `schema`. Returns the export job's total, the progress bar's estimate
async fn jobs(st: &Matrix, schema: &str) -> u64 {
    let dir = st.dir.path().join("migrations");
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    std::fs::write(dir.join("V1__items.sql"), "create table items (id int primary key, name varchar(20), total decimal(10,2), doubled int generated always as (id * 2) stored);").unwrap();
    std::fs::write(dir.join("V2__seed.sql"), "insert into items (id, name, total) values (1, 'a', 1.50);").unwrap();
    std::fs::write(dir.join("V3__bad.sql"), "insert into items (id, name, total) values (2, 'b', 2);\ninsert into no_such_table values (1);").unwrap();
    let e = err(st.apply_migrations(ID.into(), d.clone()).await);
    assert!(e.starts_with("V3 (V3__bad.sql) failed and was rolled back: ") && e.contains("no_such_table"), "{e}");
    let status: Vec<(String, String)> = st.migration_status(ID.into(), d.clone()).await.unwrap().into_iter().map(|m| (m.version, m.status)).collect();
    assert_eq!(status, [("1".into(), "applied".into()), ("2".into(), "applied".into()), ("3".into(), "pending".into())]);
    assert_eq!(q(st, "select count(*) from items").await.rows[0][0], json!(1), "V3's insert was rolled back");
    std::fs::remove_file(dir.join("V3__bad.sql")).unwrap();
    assert_eq!(st.apply_migrations(ID.into(), d).await.unwrap(), Vec::<String>::new());

    // Import: one transaction, numbers sent as text (PG casts them to the types of the columns it finds in `schema`)
    let file = st.dir.path().join("items.csv");
    std::fs::write(&file, "id,name,total\n2,b,2.25\n3,c,3\n").unwrap();
    let spec = |table: &str| qurry_core::transfer::ImportSpec { path: file.to_string_lossy().into(), schema: schema.into(), table: table.into(), create: false, columns: vec![(0, "id".into()), (1, "name".into()), (2, "total".into())] };
    assert_eq!(st.import(ID.into(), spec("items"), "ji".into()).await.unwrap(), 2);
    let e = err(st.import(ID.into(), spec("items"), "ji".into()).await);
    assert!(e.starts_with("Rows 1–2: "), "{e}");
    assert_eq!(q(st, "select count(*) from items").await.rows[0][0], json!(3), "the duplicate batch inserted nothing");

    // Export: a job registered beforehand keeps the progress total
    let job = st.start_job("je");
    let csv = st.export(ID.into(), "select id, name, total, doubled from items order by id".into(), Format::Csv, Dest::Clipboard, None, "je".into()).await.unwrap();
    assert_eq!(csv.text.unwrap(), "id,name,total,doubled\n1,a,1.50,2\n2,b,2.25,4\n3,c,3.00,6\n");

    // Audit: the undo of a delete inserts the rows back without the generated column
    q(st, "delete from items where id = 3").await;
    let u = st.audit_log(Some(ID.into()), String::new(), 1).remove(0).undo.unwrap_or_default();
    let insert = u.lines().find(|l| l.starts_with("INSERT INTO")).unwrap_or_default();
    assert!(insert.contains("total") && !insert.contains("doubled"), "{u}");
    job.total.load(std::sync::atomic::Ordering::Relaxed)
}

#[tokio::test]
#[ignore]
async fn pg_jobs() {
    let Some(c) = pg_env() else { return };
    let boot = state(&c, None).await;
    q(&boot, "drop database if exists qm_jobs").await;
    q(&boot, "create database qm_jobs").await;
    let mut c = c;
    c.database = "qm_jobs".into();
    let st = state(&c, None).await;
    assert!(jobs(&st, "public").await > 0, "EXPLAIN gives the export a row estimate");

    // A deferred foreign key fails at COMMIT: reported like a failing statement, and the whole file is rolled back
    let dir = st.dir.path().join("migrations");
    std::fs::write(dir.join("V4__late.sql"), "create table parents (id int primary key);\ncreate table kids (id int primary key, parent int references parents deferrable initially deferred);\ninsert into kids values (1, 99);").unwrap();
    let e = err(st.apply_migrations(ID.into(), dir.to_string_lossy().into()).await);
    assert!(e.starts_with("V4 (V4__late.sql) failed and was rolled back: ") && e.contains("kids"), "{e}");
    assert_eq!(q(&st, "select to_regclass('kids') is null").await.rows[0][0], json!(true), "V4's tables were rolled back");
    drop(st);
    q(&boot, "drop database qm_jobs with (force)").await;
}

/// Full-text search reads in a READ ONLY transaction: a view whose function has a side effect
/// that a rollback can't undo (nextval) is refused, so the table gives no hit and the sequence doesn't move
#[tokio::test]
#[ignore]
async fn pg_full_text_search_is_read_only() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    for s in [
        "drop schema if exists qm_fts_ro cascade",
        "create schema qm_fts_ro",
        "create sequence qm_fts_ro.hits",
        "create table qm_fts_ro.t (id int, body text)",
        "insert into qm_fts_ro.t values (1, 'needle')",
        "create function qm_fts_ro.f() returns bigint language sql volatile as 'select nextval(''qm_fts_ro.hits'')'",
        "create view qm_fts_ro.v as select id, body, qm_fts_ro.f() as x from qm_fts_ro.t",
    ] {
        q(&st, s).await;
    }
    let hits = st.full_text_search(ID.into(), fts(vec![("qm_fts_ro", "v")])).await.unwrap();
    assert!(hits.is_empty(), "{hits:?}");
    assert_eq!(q(&st, "select is_called from qm_fts_ro.hits").await.rows[0][0], json!(false), "nextval ran");
    // A plain table is still found
    assert_eq!(st.full_text_search(ID.into(), fts(vec![("qm_fts_ro", "t")])).await.unwrap().len(), 1);
    q(&st, "drop schema qm_fts_ro cascade").await;
}

/// Full-text search on a database with standard_conforming_strings = off (a legacy setting): a \ in the search text can't end
/// the string early, so `\' or …` finds nothing and runs nothing (before, every row came back)
#[tokio::test]
#[ignore]
async fn pg_full_text_search_text_is_not_sql_with_standard_conforming_strings_off() {
    let Some(mut c) = pg_env() else { return };
    let boot = state(&c, None).await;
    q(&boot, "drop database if exists qm_guard_scs with (force)").await;
    q(&boot, "create database qm_guard_scs").await;
    q(&boot, "alter database qm_guard_scs set standard_conforming_strings = off").await;
    c.database = "qm_guard_scs".into();
    let st = state(&c, None).await;
    assert_eq!(q(&st, "show standard_conforming_strings").await.rows[0][0], json!("off"));
    q(&st, "create table t (id int, body text)").await;
    q(&st, "insert into t values (1, 'needle'), (2, 'hay')").await;
    for mode in ["matches", "like", "contains"] {
        let mut r = fts(vec![("public", "t")]);
        r.text = "\\' or inet_server_port() > 0 --".into();
        r.mode = mode.into();
        r.match_case = true;
        let hits = st.full_text_search(ID.into(), r).await.unwrap();
        assert!(hits.is_empty(), "{mode}: {hits:?}");
    }
    // A backslash in the text is still searched for as itself
    q(&st, "insert into t values (3, E'a\\\\b')").await;
    let mut r = fts(vec![("public", "t")]);
    r.text = "a\\b".into();
    r.mode = "matches".into();
    let hits = st.full_text_search(ID.into(), r).await.unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].result.rows.len(), 1);
    drop(st);
    q(&boot, "drop database qm_guard_scs with (force)").await;
}

/// Explain Analyse as the GUI and the TUI run it (BEGIN, the EXPLAIN, ROLLBACK on the tab's session) of a SELECT whose function
/// writes: the guard asks first, the rollback undoes the function's insert, and the same EXPLAIN without it (before) kept the row
async fn explain_analyze_of_a_writing_function(st: &AppState, kind: DbKind, call: &str, count: &str) {
    let tab = |sql: String| async move { st.run_query(ID.into(), sql.clone(), None, Some("ea".into())).await.unwrap_or_else(|e| panic!("{sql}: {e}")) };
    let explain = if kind == DbKind::Postgres { format!("EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) {call}") } else { format!("EXPLAIN ANALYZE {call}") };
    assert_eq!(qurry_core::guard::explain_analyze_ask(call, kind), None, "{call}: a function's name says nothing of what it does");
    tab("BEGIN".into()).await;
    tab(explain.clone()).await;
    tab("ROLLBACK".into()).await;
    assert_eq!(q(st, count).await.rows[0][0].to_string().trim_matches('"'), "0", "the rollback undid the function's insert");
    tab(explain).await;
    assert_eq!(q(st, count).await.rows[0][0].to_string().trim_matches('"'), "1", "without the transaction the insert stays");
    st.close_session("ea".into()).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_explain_analyze_rolls_back_what_a_selected_function_writes() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    for s in [
        "drop schema if exists qm_guard_ea cascade",
        "create schema qm_guard_ea",
        "create table qm_guard_ea.t (a int)",
        "create function qm_guard_ea.g() returns int language plpgsql volatile as $$ begin insert into qm_guard_ea.t values (77); return 1; end $$",
    ] {
        q(&st, s).await;
    }
    explain_analyze_of_a_writing_function(&st, DbKind::Postgres, "select qm_guard_ea.g()", "select count(*) from qm_guard_ea.t").await;
    q(&st, "drop schema qm_guard_ea cascade").await;
}

// ───────── MySQL ─────────

/// A fresh database `name` and a state whose source points at it
async fn my_state(name: &str) -> Option<Matrix> {
    let (c, pw) = my_env()?;
    let boot = state(&c, Some(pw.clone())).await;
    q(&boot, &format!("drop database if exists {name}")).await;
    q(&boot, &format!("create database {name}")).await;
    let mut c = c;
    c.database = name.into();
    Some(state(&c, Some(pw)).await)
}

#[tokio::test]
#[ignore]
async fn mysql_connect_and_query() {
    let Some((c, pw)) = my_env() else { return };
    let pool = db::connect(&c, Some(&pw)).await.unwrap();
    assert!(matches!(pool, Pool::My(_)));
    let v = pool.server_version().await.unwrap();
    assert!(v.starts_with("MySQL ") || v.contains("MariaDB"), "{v}");

    let r = pool.run_query("select 1 as g, 'v1' as t union all select 2, 'v2' union all select 3, 'v3' union all select 4, 'v4'", 3).await.unwrap();
    let cols: Vec<(&str, &str)> = r.columns.iter().map(|c| (c.name.as_str(), c.type_name.as_str())).collect();
    assert_eq!(cols, [("g", "BIGINT"), ("t", "VARCHAR")]);
    assert_eq!(r.rows, [[json!(1), json!("v1")], [json!(2), json!("v2")], [json!(3), json!("v3")]]);
    assert!(r.truncated && r.has_result_set && !r.in_transaction);
    let empty = pool.run_query("select 1 as a from dual where false", 10).await.unwrap();
    assert!(empty.has_result_set && empty.rows.is_empty() && empty.columns.len() == 1);
    let ddl = pool.run_query("create temporary table qm_t (a int)", 10).await.unwrap();
    assert!(!ddl.has_result_set && ddl.columns.is_empty());
    assert!(pool.run_query("select * from qm_no_such_table", 10).await.is_err());
}

#[tokio::test]
#[ignore]
async fn mysql_session_fetch_cancel_tx() {
    let Some(st) = my_state("qm_session").await else { return };
    let tab = || Some("my-tab".to_string());
    q(&st, "create table nums (n int primary key)").await;
    // cte_max_recursion_depth is 1000: 1–1000, then 1001–1200
    q(&st, "insert into nums with recursive s(n) as (select 1 union all select n + 1 from s where n < 1000) select n from s").await;
    q(&st, "insert into nums select n + 1000 from nums where n <= 200").await;
    let r1 = st.run_query(ID.into(), "select n from nums order by n".into(), Some(500), tab()).await.unwrap();
    assert_eq!((r1.rows.len(), r1.truncated, r1.rows[0][0].clone()), (500, true, json!(1)));
    let r2 = st.fetch_more("my-tab".into(), Some(500)).await.unwrap();
    assert_eq!((r2.rows.len(), r2.truncated, r2.rows[0][0].clone()), (500, true, json!(501)));
    let r3 = st.fetch_more("my-tab".into(), Some(500)).await.unwrap();
    assert_eq!((r3.rows.len(), r3.truncated), (200, false));
    assert!(st.fetch_more("my-tab".into(), Some(500)).await.is_err(), "no more rows");

    // KILL QUERY interrupts SLEEP: it returns 1 early instead of failing
    let st = std::sync::Arc::new(st);
    let st2 = st.clone();
    let cancel = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        st2.cancel_query("my-tab".into()).await.unwrap()
    });
    let t0 = std::time::Instant::now();
    let slow = st.run_query(ID.into(), "select sleep(10)".into(), None, tab()).await.unwrap();
    assert!(cancel.await.unwrap());
    assert_eq!(slow.rows[0][0], json!(1));
    assert!(t0.elapsed().as_secs() < 5);

    q(&st, "create table tx (id int primary key)").await;
    st.set_tx_mode(ID.into(), "my-tab".into(), true).await.unwrap();
    let ins = st.run_query(ID.into(), "insert into tx values (1)".into(), None, tab()).await.unwrap();
    assert!(ins.in_transaction && ins.rows_affected == 1);
    assert!(st.tx_state("my-tab".into()).await);
    let count = |st: std::sync::Arc<Matrix>| async move { q(&st, "select count(*) from tx").await.rows[0][0].clone() };
    assert_eq!(count(st.clone()).await, json!(0));
    st.rollback_tx("my-tab".into()).await.unwrap();
    assert!(!st.tx_state("my-tab".into()).await);
    st.run_query(ID.into(), "insert into tx values (2)".into(), None, tab()).await.unwrap();
    st.commit_tx("my-tab".into()).await.unwrap();
    assert_eq!(count(st.clone()).await, json!(1));
    st.set_tx_mode(ID.into(), "my-tab".into(), false).await.unwrap();
    st.close_session("my-tab".into()).await.unwrap();
    q(&st, "drop database qm_session").await;
}

async fn my_fixture(st: &AppState, db: &str) {
    for s in [
        format!("create table {db}.customers (id int auto_increment primary key, name varchar(20) not null default 'anon', tier varchar(10), key customers_tier (tier)) comment 'People'"),
        format!("create table {db}.orders (id int primary key, customer_id int not null, total decimal(10,2), constraint orders_customer foreign key (customer_id) references {db}.customers (id))"),
        format!("create view {db}.big_orders as select id, total from {db}.orders where total > 100"),
        format!("create function {db}.twice(x int) returns int deterministic return x * 2"),
        format!("insert into {db}.customers (name, tier) values ('a', 'gold'), ('b', 'gold'), ('c', 'silver')"),
        format!("insert into {db}.orders values (1, 1, 50), (2, 1, 150)"),
    ] {
        q(st, &s).await;
    }
}

#[tokio::test]
#[ignore]
async fn mysql_metadata() {
    let Some(st) = my_state("qm_meta").await else { return };
    my_fixture(&st, "qm_meta").await;

    let dbs = st.list_databases(ID.into()).await.unwrap();
    assert!(dbs.contains(&"qm_meta".to_string()) && dbs.contains(&"mysql".to_string()), "{dbs:?}");
    let schemas = st.list_schemas(ID.into()).await.unwrap();
    assert_eq!(schemas.iter().map(|s| (s.name.as_str(), s.is_system)).collect::<Vec<_>>(), [("qm_meta", false)], "only the current database");

    let o = st.list_objects(ID.into(), "qm_meta".into()).await.unwrap();
    let rels: Vec<(&str, &str)> = o.relations.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect();
    assert_eq!(rels, [("big_orders", "view"), ("customers", "table"), ("orders", "table")]);
    assert_eq!(o.relations[1].comment.as_deref(), Some("People"));
    assert_eq!(o.routines.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect::<Vec<_>>(), [("twice", "function")]);

    let d = st.table_detail(ID.into(), "qm_meta".into(), "customers".into()).await.unwrap();
    let cols: Vec<(&str, &str, bool, bool)> = d.columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str(), c.nullable, c.is_pk)).collect();
    assert_eq!(cols, [("id", "int", false, true), ("name", "varchar(20)", false, false), ("tier", "varchar(10)", true, false)]);
    assert_eq!(d.columns[1].default_value.as_deref(), Some("anon"));
    let idx: Vec<(&str, bool, bool)> = d.indexes.iter().map(|i| (i.name.as_str(), i.is_unique, i.is_primary)).collect();
    assert_eq!(idx, [("customers_tier", false, false), ("PRIMARY", true, true)], "by name, case-insensitive");
    let fk = st.table_detail(ID.into(), "qm_meta".into(), "orders".into()).await.unwrap();
    assert!(fk.constraints.iter().any(|c| c.kind == "f" && c.name == "orders_customer"), "{:?}", fk.constraints.iter().map(|c| (&c.name, &c.kind)).collect::<Vec<_>>());

    let ddl = st.object_ddl(ID.into(), "qm_meta".into(), "customers".into(), "table".into()).await.unwrap();
    assert!(ddl.starts_with("CREATE TABLE `customers` (\n  `id` int NOT NULL AUTO_INCREMENT,"), "{ddl}");
    assert!(ddl.contains("COMMENT='People'"), "{ddl}");
    let view = st.object_ddl(ID.into(), "qm_meta".into(), "big_orders".into(), "view".into()).await.unwrap();
    assert!(view.contains("VIEW `big_orders` AS select"), "{view}");
    let f = st.object_ddl(ID.into(), "qm_meta".into(), "twice".into(), "routine".into()).await.unwrap();
    assert!(f.contains("FUNCTION `twice`(x int) RETURNS int"), "{f}");

    let comp = st.completion_meta(ID.into()).await.unwrap();
    let t = comp.iter().find(|t| t.schema == "qm_meta" && t.name == "customers").expect("customers in completion");
    assert_eq!(t.columns, [("id".to_string(), "int".to_string()), ("name".into(), "varchar(20)".into()), ("tier".into(), "varchar(10)".into())]);

    let idx = st.object_index(ID.into()).await.unwrap();
    let mut mine: Vec<(&str, &str)> = idx.iter().filter(|e| e.schema == "qm_meta").map(|e| (e.name.as_str(), e.kind.as_str())).collect();
    mine.sort();
    assert_eq!(mine, [("big_orders", "view"), ("customers", "table"), ("orders", "table"), ("qm_meta", "schema"), ("twice", "function")]);
    q(&st, "drop database qm_meta").await;
}

#[tokio::test]
#[ignore]
async fn mysql_tools() {
    let Some(st) = my_state("qm_tools").await else { return };
    my_fixture(&st, "qm_tools").await;

    let h = st.column_hints(ID.into(), hint("qm_tools", "customers", "tier")).await.unwrap();
    assert_eq!(h.iter().map(|h| (h.value.as_str(), h.count)).collect::<Vec<_>>(), [("gold", 2), ("silver", 1)]);

    let e = st.erd(ID.into(), vec!["qm_tools".into()]).await.unwrap();
    assert_eq!(e.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["customers", "orders"], "tables only, no views");
    let r = &e.relations[0];
    assert_eq!((e.relations.len(), r.from_table.as_str(), r.from_columns.clone(), r.to_table.as_str(), r.to_columns.clone()), (1, "orders", vec!["customer_id".to_string()], "customers", vec!["id".to_string()]));

    let sql = "select id, name, tier from qm_tools.customers order by id limit 2";
    assert_eq!(export(&st, sql, Format::Csv, None).await, "id,name,tier\n1,a,gold\n2,b,gold\n");
    assert_eq!(export(&st, sql, Format::Json, None).await, "[\n  {\"id\":1,\"name\":\"a\",\"tier\":\"gold\"},\n  {\"id\":2,\"name\":\"b\",\"tier\":\"gold\"}\n]\n");
    assert_eq!(
        export(&st, sql, Format::Sql, Some(("qm_tools", "customers"))).await,
        "INSERT INTO qm_tools.customers (id, name, tier) VALUES (1, 'a', 'gold');\nINSERT INTO qm_tools.customers (id, name, tier) VALUES (2, 'b', 'gold');\n",
        "plain identifiers stay unquoted"
    );
    assert_eq!(export(&st, sql, Format::Markdown, None).await, "| id | name | tier |\n| --- | --- | --- |\n| 1 | a | gold |\n| 2 | b | gold |\n");

    let p = st.fake_preview(ID.into(), "qm_tools".into(), "orders".into(), 3).await.unwrap();
    assert_eq!(p.plan.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "customer_id", "total"]);
    assert_eq!((p.result.rows.len(), p.result.columns.len()), (3, 3));
    assert_eq!(st.fake_fill(ID.into(), "qm_tools".into(), "orders".into(), 5).await.unwrap().rows, 5);
    assert_eq!(q(&st, "select count(*) from qm_tools.orders").await.rows[0][0], json!(7));

    q(&st, "drop database if exists qm_dst").await;
    q(&st, "create database qm_dst").await;
    q(&st, "create table qm_dst.customers (id int auto_increment primary key, name varchar(20) not null default 'anon')").await;
    let ch = st.compare_schemas(ID.into(), "qm_tools".into(), ID.into(), "qm_dst".into()).await.unwrap();
    let kinds: Vec<(&str, &str)> = ch.iter().map(|c| (c.kind.as_str(), c.object.as_str())).collect();
    assert!(kinds.contains(&("add_column", "customers.tier")) && kinds.contains(&("create_table", "orders")) && kinds.contains(&("create_view", "big_orders")), "{kinds:?}");
    assert!(ch.iter().all(|c| !c.destructive));

    q(&st, "update qm_tools.customers set tier = 'bronze' where id = 3").await;
    let a = st.audit_log(Some(ID.into()), String::new(), 1).remove(0);
    assert_eq!((a.sql.as_str(), a.rows, a.error.is_none(), a.in_tx), ("update qm_tools.customers set tier = 'bronze' where id = 3", Some(1), true, false));
    assert!(a.undo.as_deref().is_some_and(|u| u.contains("'silver'")), "{a:?}");
    q(&st, "drop database qm_tools").await;
    q(&st, "drop database qm_dst").await;
}

#[tokio::test]
#[ignore]
async fn mysql_jobs() {
    let Some(st) = my_state("qm_jobs").await else { return };
    assert_eq!(jobs(&st, "").await, 0, "no row estimate on MySQL");
    // Binary columns are snapshotted as HEX
    q(&st, "create table blobs (id int primary key, b varbinary(4))").await;
    q(&st, "insert into blobs values (1, x'00ff')").await;
    q(&st, "update blobs set b = x'01' where id = 1").await;
    let a = st.audit_log(Some(ID.into()), String::new(), 1).remove(0);
    assert!(a.undo.as_deref().is_some_and(|u| u.contains("00FF")), "{a:?}");
    q(&st, "drop database qm_jobs").await;
}

/// Full-text search reads in a READ ONLY transaction: a view whose stored function writes is refused (error 1792), not committed
#[tokio::test]
#[ignore]
async fn mysql_full_text_search_is_read_only() {
    let Some(st) = my_state("qm_fts_ro").await else { return };
    for s in [
        "create table qm_fts_ro.audit (id int auto_increment primary key)",
        "create table qm_fts_ro.t (id int, body text)",
        "insert into qm_fts_ro.t values (1, 'needle')",
        "create function qm_fts_ro.f() returns int deterministic modifies sql data begin insert into qm_fts_ro.audit values (); return 1; end",
        "create view qm_fts_ro.v as select id, body, qm_fts_ro.f() as x from qm_fts_ro.t",
    ] {
        q(&st, s).await;
    }
    let hits = st.full_text_search(ID.into(), fts(vec![("qm_fts_ro", "v")])).await.unwrap();
    assert!(hits.is_empty(), "{hits:?}");
    assert_eq!(q(&st, "select count(*) from qm_fts_ro.audit").await.rows[0][0], json!(0), "the function's INSERT was committed");
    // A plain table is still found
    assert_eq!(st.full_text_search(ID.into(), fts(vec![("qm_fts_ro", "t")])).await.unwrap().len(), 1);
    q(&st, "drop database qm_fts_ro").await;
}

#[tokio::test]
#[ignore]
async fn mysql_explain_analyze_rolls_back_what_a_selected_function_writes() {
    let Some(st) = my_state("qm_guard_ea").await else { return };
    q(&st, "create table t (a int) engine = innodb").await;
    q(&st, "create function g() returns int deterministic modifies sql data begin insert into t values (77); return 1; end").await;
    explain_analyze_of_a_writing_function(&st, DbKind::Mysql, "select g()", "select count(*) from t").await;
    q(&st, "drop database qm_guard_ea").await;
}

/// Explain Analyse of a SELECT inside a transaction the user opened, as the GUI and the TUI send it on a read-only Data Source:
/// the savepoint around it passes the read-only check, so the plan runs and the transaction stays open
async fn explain_analyze_in_a_read_only_transaction(mut c: ConnectionConfig, password: Option<String>, explain: &str) {
    c.read_only = true;
    let st = state(&c, password).await;
    let tab = |sql: &str| {
        let sql = sql.to_string();
        let st = &st;
        async move { st.run_query(ID.into(), sql.clone(), None, Some("ea-ro".into())).await.unwrap_or_else(|e| panic!("{sql}: {e}")) }
    };
    tab("BEGIN").await;
    tab("SAVEPOINT qurry_explain").await;
    assert!(!tab(explain).await.rows.is_empty(), "{explain}");
    assert!(tab("ROLLBACK TO SAVEPOINT qurry_explain").await.in_transaction);
    tab("ROLLBACK").await;
    st.close_session("ea-ro".into()).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_explain_analyze_in_a_transaction_on_a_read_only_source() {
    let Some(c) = pg_env() else { return };
    explain_analyze_in_a_read_only_transaction(c, None, "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select 1").await;
}

#[tokio::test]
#[ignore]
async fn mysql_explain_analyze_in_a_transaction_on_a_read_only_source() {
    let Some((c, pw)) = my_env() else { return };
    explain_analyze_in_a_read_only_transaction(c, Some(pw), "EXPLAIN ANALYZE select 1").await;
}

/// Explain Analyse after `setup` left a row of `t` (count `count`) uncommitted in the console tab, run as the GUI and the TUI run it
/// (AppState::explain_wrap around the statement): it uses a savepoint, so the row is still pending (seen in the tab, not on another
/// connection) and the user's `end` (ROLLBACK or COMMIT) removes or commits it. A BEGIN there would commit it on MySQL, and on
/// PostgreSQL its closing ROLLBACK would throw it away
async fn explain_analyze_keeps_pending_work(st: &AppState, tab_id: &str, setup: &[&str], explain: &str, count: &str, end: &str) {
    let tab = |sql: &str| {
        let sql = sql.to_string();
        async move { st.run_query(ID.into(), sql.clone(), None, Some(tab_id.into())).await.unwrap_or_else(|e| panic!("{sql}: {e}")) }
    };
    let n = |r: db::QueryResult| r.rows[0][0].to_string().trim_matches('"').to_string();
    for s in setup {
        tab(s).await;
    }
    let wrap = st.explain_wrap(ID.into(), tab_id.into()).await.unwrap();
    assert_eq!(wrap, qurry_core::session::EXPLAIN_SAVEPOINT, "{setup:?}");
    tab(wrap[0]).await;
    tab(explain).await;
    let back = tab(wrap[1]).await;
    assert!(back.in_transaction, "{setup:?}: the pending row shows as uncommitted");
    assert_eq!(n(tab(count).await), "1", "{setup:?}: the tab still sees its pending row");
    assert_eq!(n(q(st, count).await), "0", "{setup:?}: Explain Analyse committed the pending row");
    tab(end).await;
    assert_eq!(n(q(st, count).await), if end == "COMMIT" { "1" } else { "0" }, "{setup:?} {end}");
    st.close_session(tab_id.into()).await.unwrap();
}

/// Production, Manual mode, no transaction open yet: a SELECT whose function writes runs in a read-only transaction of its own
/// (Session::set_reads_outside_tx), so the server refuses the write; the error says to run BEGIN first, and nothing is written or
/// left open
async fn read_first_that_writes_is_refused(st: &AppState, tab: &str, sql: &str, count: &str) {
    let e = st.run_query(ID.into(), sql.into(), None, Some(tab.into())).await.unwrap_err().to_string();
    assert!(e.contains(qurry_shared::errors::READ_FIRST_WROTE), "{e}");
    assert!(!st.tx_state(tab.into()).await);
    assert_eq!(q(st, count).await.rows[0][0].to_string().trim_matches('"'), "0");
}

#[tokio::test]
#[ignore]
async fn mysql_explain_analyze_never_commits_pending_work() {
    let Some(st) = my_state("qm_explain_guard").await else { return };
    q(&st, "create table t (a int) engine = innodb").await;
    q(&st, "create procedure off() begin set autocommit = 0; end").await;
    q(&st, "create function w() returns int deterministic modifies sql data begin insert into t values (77); return 1; end").await;
    let (explain, count) = ("EXPLAIN ANALYZE select * from t", "select count(*) from t");
    // autocommit=0 typed by hand, turned off where Qurry can't see it (a procedure, a dump's /*! */ header), Manual mode, and a
    // Manual-mode SELECT whose function writes
    explain_analyze_keeps_pending_work(&st, "typed", &["SET autocommit = 0", "insert into t values (1)"], explain, count, "ROLLBACK").await;
    explain_analyze_keeps_pending_work(&st, "call", &["call off()", "insert into t values (1)"], explain, count, "ROLLBACK").await;
    explain_analyze_keeps_pending_work(&st, "dump", &["/*!40101 SET autocommit = 0 */", "insert into t values (1)"], explain, count, "ROLLBACK").await;
    // Autocommit on, a transaction opened where Qurry's tracker can't see it: the server is asked
    explain_analyze_keeps_pending_work(&st, "hidden", &["/*!40101 START TRANSACTION */", "insert into t values (1)"], explain, count, "ROLLBACK").await;
    explain_analyze_keeps_pending_work(&st, "hash", &["# note\nSTART TRANSACTION", "insert into t values (1)"], explain, count, "ROLLBACK").await;
    st.set_tx_mode(ID.into(), "manual".into(), true).await.unwrap();
    explain_analyze_keeps_pending_work(&st, "manual", &["insert into t values (1)"], explain, count, "ROLLBACK").await;
    st.set_tx_mode(ID.into(), "manual-f".into(), true).await.unwrap();
    read_first_that_writes_is_refused(&st, "manual-f", "select w()", count).await;
    explain_analyze_keeps_pending_work(&st, "manual-f", &["BEGIN", "select w()"], explain, count, "ROLLBACK").await;
    // COMMIT / ROLLBACK AND CHAIN open the next transaction at once: its row shows as pending, and the user's COMMIT keeps it
    for (tab_id, chain) in [("chain-c", "COMMIT AND CHAIN"), ("chain-r", "rollback work and chain")] {
        for s in ["BEGIN", chain] {
            q_tab(&st, tab_id, s).await;
        }
        assert!(q_tab(&st, tab_id, "insert into t values (1)").await.in_transaction, "{chain}");
        q_tab(&st, tab_id, "ROLLBACK").await;
        st.close_session(tab_id.into()).await.unwrap();
    }
    explain_analyze_keeps_pending_work(&st, "chain", &["BEGIN", "COMMIT AND CHAIN", "insert into t values (1)"], explain, count, "COMMIT").await;
    // Autocommit on and nothing open: a transaction of its own
    assert_eq!(st.explain_wrap(ID.into(), "auto".into()).await.unwrap(), qurry_core::session::EXPLAIN_BEGIN);
    st.close_session("auto".into()).await.unwrap();
    q(&st, "drop database qm_explain_guard").await;
}

#[tokio::test]
#[ignore]
async fn pg_explain_analyze_never_commits_pending_work() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    for s in ["drop schema if exists qm_explain_guard cascade", "create schema qm_explain_guard", "create table qm_explain_guard.t (a int)"] {
        q(&st, s).await;
    }
    let (explain, count) = ("EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select * from qm_explain_guard.t", "select count(*) from qm_explain_guard.t");
    explain_analyze_keeps_pending_work(&st, "begin", &["BEGIN", "insert into qm_explain_guard.t values (1)"], explain, count, "ROLLBACK").await;
    st.set_tx_mode(ID.into(), "manual".into(), true).await.unwrap();
    // COMMIT / ROLLBACK AND CHAIN open the next transaction at once: Explain Analyse keeps its row, the user's COMMIT commits it
    let ins = "insert into qm_explain_guard.t values (1)";
    explain_analyze_keeps_pending_work(&st, "chain-r", &["BEGIN", "/* x */ ROLLBACK WORK AND CHAIN", ins], explain, count, "ROLLBACK").await;
    explain_analyze_keeps_pending_work(&st, "chain-c", &["BEGIN", "COMMIT AND CHAIN", ins], explain, count, "COMMIT").await;
    q(&st, "delete from qm_explain_guard.t").await;
    explain_analyze_keeps_pending_work(&st, "manual", &["insert into qm_explain_guard.t values (1)"], explain, count, "ROLLBACK").await;
    // A Manual-mode SELECT whose function writes
    q(&st, "create function qm_explain_guard.w() returns int language sql as $$ insert into qm_explain_guard.t values (77); select 1 $$").await;
    st.set_tx_mode(ID.into(), "manual-f".into(), true).await.unwrap();
    read_first_that_writes_is_refused(&st, "manual-f", "select qm_explain_guard.w()", count).await;
    explain_analyze_keeps_pending_work(&st, "manual-f", &["BEGIN", "select qm_explain_guard.w()"], explain, count, "ROLLBACK").await;
    assert_eq!(st.explain_wrap(ID.into(), "auto".into()).await.unwrap(), qurry_core::session::EXPLAIN_BEGIN);
    st.close_session("auto".into()).await.unwrap();
    q(&st, "drop schema qm_explain_guard cascade").await;
}

// ───────── Redis ─────────

#[tokio::test]
#[ignore]
async fn redis_connect_and_query() {
    let c = redis_env("db12");
    let pool = db::connect(&c, None).await.unwrap();
    assert!(matches!(pool, Pool::Redis(_, 12)));
    let v = pool.server_version().await.unwrap();
    assert!(v.starts_with("Redis "), "{v}");
    pool.run_query("FLUSHDB", 10).await.unwrap();
    pool.run_query("RPUSH qm:list a b c d", 10).await.unwrap();
    let r = pool.run_query("LRANGE qm:list 0 -1", 3).await.unwrap();
    assert!(r.truncated && r.has_result_set, "{r:?}");
    assert_eq!(r.rows.len(), 3);
    let s = pool.run_query("SET qm:s hello", 10).await.unwrap();
    assert_eq!(s.rows, [[json!("OK")]]);
    assert_eq!(pool.run_query("GET qm:s", 10).await.unwrap().rows, [[json!("hello")]]);
    let e = err(pool.run_query("SELECT 2", 10).await);
    assert!(e.contains("database selector"), "{e}");
    assert!(pool.run_query("NOSUCHCOMMAND", 10).await.is_err());
    pool.run_query("FLUSHDB", 10).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn redis_metadata_and_tools() {
    let c = redis_env("db11");
    let st = state(&c, None).await;
    q(&st, "FLUSHDB").await;
    q(&st, "SET qm:s hello").await;
    q(&st, "HSET qm:h f1 v1").await;

    assert!(st.list_databases(ID.into()).await.unwrap().contains(&"db11".to_string()));
    let schemas = st.list_schemas(ID.into()).await.unwrap();
    assert_eq!(schemas.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["db11"]);
    let o = st.list_objects(ID.into(), "db11".into()).await.unwrap();
    let mut rels: Vec<(&str, &str, Option<&str>)> = o.relations.iter().map(|r| (r.name.as_str(), r.kind.as_str(), r.comment.as_deref())).collect();
    rels.sort();
    assert_eq!(rels, [("qm:h", "key", Some("hash")), ("qm:s", "key", Some("string"))]);
    assert!(o.routines.is_empty());
    let d = st.table_detail(ID.into(), "db11".into(), "qm:s".into()).await.unwrap();
    assert!(d.columns.is_empty() && d.indexes.is_empty() && d.constraints.is_empty());
    assert_eq!(st.object_ddl(ID.into(), "db11".into(), "qm:h".into(), "key".into()).await.unwrap(), "HSET qm:h f1 v1");
    assert!(st.completion_meta(ID.into()).await.unwrap().is_empty());
    let mut idx: Vec<(String, String, String)> = st.object_index(ID.into()).await.unwrap().into_iter().map(|e| (e.schema, e.name, e.kind)).collect();
    idx.sort();
    assert_eq!(idx, [("db11".into(), "qm:h".into(), "key".into()), ("db11".into(), "qm:s".into(), "key".into())]);
    assert!(st.column_hints(ID.into(), hint("db11", "qm:h", "f1")).await.unwrap().is_empty());

    // Table tools refuse Redis
    assert_eq!(err(st.erd(ID.into(), vec!["db11".into()]).await), "Redis has no tables");
    assert_eq!(err(st.fake_preview(ID.into(), "db11".into(), "qm:h".into(), 3).await), "Redis has no tables");
    assert_eq!(err(st.compare_schemas(ID.into(), "db11".into(), ID.into(), "db11".into()).await), "Redis has no schema to compare");

    let file = st.dir.path().join("keys.csv");
    std::fs::write(&file, "k\nx\n").unwrap();
    let spec = qurry_core::transfer::ImportSpec { path: file.to_string_lossy().into(), schema: String::new(), table: "t".into(), create: false, columns: vec![(0, "k".into())] };
    assert_eq!(err(st.import(ID.into(), spec, "ji".into()).await), "Import into Redis is not supported");

    // Export runs the command
    assert_eq!(export(&st, "HGETALL qm:h", Format::Csv, None).await, "field,value\nf1,v1\n");
    assert_eq!(export(&st, "HGETALL qm:h", Format::Json, None).await, "[\n  {\"field\":\"f1\",\"value\":\"v1\"}\n]\n");

    // Sessions: no cursors, no manual transactions, nothing to cancel
    let r = st.run_query(ID.into(), "GET qm:s".into(), None, Some("rd-tab".into())).await.unwrap();
    assert_eq!(r.rows, [[json!("hello")]]);
    assert_eq!(err(st.fetch_more("rd-tab".into(), None).await), "No more rows");
    assert!(err(st.set_tx_mode(ID.into(), "rd-tab".into(), true).await).contains("not supported for Redis"));
    assert!(!st.cancel_query("rd-tab".into()).await.unwrap());

    // Audit: writes are logged, without undo
    q(&st, "SET qm:s bye").await;
    let a = st.audit_log(Some(ID.into()), String::new(), 1).remove(0);
    assert_eq!((a.sql.as_str(), a.error.is_none(), a.undo.is_none()), ("SET qm:s bye", true, true), "{a:?}");
    q(&st, "FLUSHDB").await;
}

// ───────── Console results: can their rows be edited (#86) ─────────

/// Each statement's edit target (in table `s.<table>`, pk, the table column of each result column) or its read-only reason,
/// run in a console tab. `count` is the server's name for count(*)
async fn console_edit_cases(st: &AppState, s: &str, count: &str) {
    use qurry_core::editable::{ColumnEdit, EditTarget, Editable, JoinEdit, JoinTable};
    use qurry_shared::errors as e;
    let v = |x: &[&str]| x.iter().map(|c| c.to_string()).collect::<Vec<_>>();
    let edit = |table: &str, pk: &[&str], cols: &[&str]| Some(Editable::Table(EditTarget { schema: s.into(), table: table.into(), pk: v(pk), columns: v(cols) }));
    let ro = |why: String| Some(Editable::ReadOnly(why));
    let (to, cro) = (|t: usize, c: &str| ColumnEdit::To { table: t, column: c.into() }, |why: String| ColumnEdit::ReadOnly { read_only: why });
    let jt = |table: &str, pk: &[&str], pk_at: &[usize]| JoinTable { schema: s.into(), table: table.into(), pk: v(pk), pk_at: pk_at.to_vec() };
    let join = |tables: Vec<JoinTable>, columns: Vec<ColumnEdit>| Some(Editable::Join(JoinEdit { tables, columns }));
    let items_pk = e::edit_missing_pk(&format!("{s}.items"), &v(&["id"]));
    let cases = [
        (format!("select * from {s}.items"), edit("items", &["id"], &["id", "name", "qty"])),
        (format!("select name, id from {s}.items where qty > 0 order by id"), edit("items", &["id"], &["name", "id"])),
        (format!("select i.id as item_id, i.name n from {s}.items i limit 2"), edit("items", &["id"], &["id", "name"])),
        (format!("select * from {s}.pairs"), edit("pairs", &["a", "b"], &["a", "b", "note"])),
        // No rows: still editable (rows can be added)
        (format!("select * from {s}.items where qty < 0"), edit("items", &["id"], &["id", "name", "qty"])),
        // Joins (#102): per table, where that table's key is in the result
        (format!("select i.id, i.name from {s}.items i join {s}.pairs p on p.a = i.id"), join(vec![jt("items", &["id"], &[0])], vec![to(0, "id"), to(0, "name")])),
        // A self-join: one table, but the columns of two different rows; b's key isn't in the result
        (format!("select a.id, b.name from {s}.items a join {s}.items b on b.id = a.qty"), join(vec![jt("items", &["id"], &[0])], vec![to(0, "id"), cro(items_pk.clone())])),
        (
            format!("select * from {s}.items, {s}.pairs"),
            join(vec![jt("items", &["id"], &[0]), jt("pairs", &["a", "b"], &[3, 4])], vec![to(0, "id"), to(0, "name"), to(0, "qty"), to(1, "a"), to(1, "b"), to(1, "note")]),
        ),
        (format!("select count(*) from {s}.items"), ro(e::edit_computed(count))),
        (format!("select name, max(id) from {s}.items group by name"), ro(e::edit_combined("GROUP BY"))),
        (format!("select distinct id, name from {s}.items"), ro(e::edit_combined("DISTINCT"))),
        (format!("select id from {s}.items union select a from {s}.pairs"), ro(e::edit_combined("UNION"))),
        (format!("select id, upper(name) as name from {s}.items"), ro(e::edit_computed("name"))),
        (format!("select name, qty from {s}.items"), ro(items_pk.clone())),
        (format!("select a, note from {s}.pairs"), ro(e::edit_missing_pk(&format!("{s}.pairs"), &v(&["a", "b"])))),
        (format!("select * from {s}.v_items"), ro(e::edit_not_table(&format!("{s}.v_items"), "view"))),
        (format!("select id, id from {s}.items"), ro(e::edit_duplicate("id"))),
        (format!("select id, name, name as again from {s}.items"), ro(e::edit_duplicate("name"))),
        (format!("select * from {s}.nokey"), ro(e::edit_no_pk(&format!("{s}.nokey")))),
        (format!("select * from (select * from {s}.items) x"), ro(e::EDIT_NOT_PLAIN.into())),
        // A locking read: in Auto mode the cursor's transaction keeps the row locks the edits (another connection) would wait on
        (format!("select * from {s}.items for update"), ro(e::EDIT_LOCKS.into())),
        ("select 1 as id".to_string(), ro(e::EDIT_NO_TABLE.into())),
    ];
    for (sql, want) in cases {
        let r = q_tab(st, "edit-tab", &sql).await;
        assert_eq!(r.editable, want, "{sql}");
    }
    // Statements without a result set say nothing
    let r = q_tab(st, "edit-tab", &format!("update {s}.items set qty = qty where id = 1")).await;
    assert_eq!(r.editable, None);
    // The lookups don't touch the console's cursor: the rest of the rows still load
    let r = st.run_query(ID.into(), format!("select * from {s}.items order by id"), Some(1), Some("edit-tab".into())).await.unwrap();
    assert!(r.truncated && matches!(r.editable, Some(Editable::Table(_))), "{r:?}");
    let more = st.fetch_more("edit-tab".into(), Some(10)).await.unwrap();
    assert_eq!(more.rows.len(), 2);
    // ... nor its Manual transaction: the pending change is still there to roll back
    st.set_tx_mode(ID.into(), "edit-tab".into(), true).await.unwrap();
    q_tab(st, "edit-tab", &format!("update {s}.items set qty = 99 where id = 1")).await;
    let r = q_tab(st, "edit-tab", &format!("select id, qty from {s}.items where id = 1")).await;
    assert!(matches!(r.editable, Some(Editable::Table(_))) && r.in_transaction && r.rows[0][1] == json!(99), "{r:?}");
    st.rollback_tx("edit-tab".into()).await.unwrap();
    st.set_tx_mode(ID.into(), "edit-tab".into(), false).await.unwrap();
    st.close_session("edit-tab".into()).await.unwrap();
    assert_eq!(q(st, &format!("select qty from {s}.items where id = 1")).await.rows[0][0], json!(1));
    // Not a console (no tab), and MCP's read-only runs: not checked
    assert_eq!(q(st, &format!("select * from {s}.items")).await.editable, None);
    assert_eq!(st.run_read_only(ID.into(), format!("select * from {s}.items"), 10).await.unwrap().editable, None);
}

#[tokio::test]
#[ignore]
async fn pg_console_result_edit_target() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    let s = "qm_edit";
    for sql in [
        format!("drop schema if exists {s} cascade"),
        format!("create schema {s}"),
        format!("create table {s}.items (id int primary key, name text, qty int)"),
        format!("create table {s}.pairs (a int, b int, note text, primary key (a, b))"),
        format!("create table {s}.nokey (x int, y text)"),
        format!("create view {s}.v_items as select id, name from {s}.items"),
        format!("insert into {s}.items values (1, 'a', 1), (2, 'b', 2), (3, 'c', 3)"),
        format!("insert into {s}.pairs values (1, 1, 'x')"),
    ] {
        q(&st, &sql).await;
    }
    console_edit_cases(&st, s, "count").await;
    // PostgreSQL only: a system column, a cast, a materialized view, a partitioned table
    use qurry_core::editable::Editable;
    use qurry_shared::errors as e;
    for sql in [
        format!("create materialized view {s}.m_items as select * from {s}.items"),
        format!("create table {s}.parts (id int, at date, primary key (id, at)) partition by range (at)"),
        format!("create table {s}.parts_1 partition of {s}.parts for values from ('2020-01-01') to ('2030-01-01')"),
    ] {
        q(&st, &sql).await;
    }
    let cases = [
        (format!("select ctid, * from {s}.items"), Editable::ReadOnly(e::edit_computed("ctid"))),
        (format!("select id, name::varchar from {s}.items"), Editable::ReadOnly(e::edit_computed("name"))),
        (format!("select * from {s}.m_items"), Editable::ReadOnly(e::edit_not_table(&format!("{s}.m_items"), "materialized view"))),
    ];
    for (sql, want) in cases {
        assert_eq!(q_tab(&st, "edit-pg", &sql).await.editable, Some(want), "{sql}");
    }
    let r = q_tab(&st, "edit-pg", &format!("select * from {s}.parts")).await;
    assert!(matches!(&r.editable, Some(Editable::Table(t)) if t.table == "parts" && t.pk == ["id", "at"]), "{r:?}");
    // Mixed-case names keep their case; the search path finds the table
    q(&st, &format!("create table {s}.\"Mixed\" (\"Id\" int primary key, \"Note\" text)")).await;
    q_tab(&st, "edit-pg", &format!("set search_path = {s}")).await;
    let r = q_tab(&st, "edit-pg", "select \"Note\", \"Id\" from \"Mixed\"").await;
    assert!(matches!(&r.editable, Some(Editable::Table(t)) if t.schema == s && t.table == "Mixed" && t.pk == ["Id"] && t.columns == ["Note", "Id"]), "{r:?}");
    // A temporary table lives in the console's session: edits saved on another connection can't reach it
    q_tab(&st, "edit-pg", "create temporary table tmp_items (id int primary key, name text)").await;
    assert_eq!(q_tab(&st, "edit-pg", "select * from tmp_items").await.editable, Some(Editable::ReadOnly(e::edit_temporary("tmp_items"))));
    st.close_session("edit-pg".into()).await.unwrap();
    q(&st, &format!("drop schema {s} cascade")).await;
}

#[tokio::test]
#[ignore]
async fn mysql_console_result_edit_target() {
    let s = "qm_edit";
    let Some(st) = my_state(s).await else { return };
    for sql in [
        format!("create table {s}.items (id int primary key, name varchar(20), qty int)"),
        format!("create table {s}.pairs (a int, b int, note varchar(20), primary key (a, b))"),
        format!("create table {s}.nokey (x int, y varchar(20))"),
        format!("create view {s}.v_items as select id, name from {s}.items"),
        format!("insert into {s}.items values (1, 'a', 1), (2, 'b', 2), (3, 'c', 3)"),
        format!("insert into {s}.pairs values (1, 1, 'x')"),
    ] {
        q(&st, &sql).await;
    }
    console_edit_cases(&st, s, "count(*)").await;
    // MySQL only: the console's database (no schema in the statement), names in other case, a bare value word, a temporary table
    use qurry_core::editable::Editable;
    use qurry_shared::errors as e;
    let r = q_tab(&st, "edit-my", "select NAME, Id from items").await;
    assert!(matches!(&r.editable, Some(Editable::Table(t)) if t.schema == s && t.table == "items" && t.columns == ["name", "id"]), "{r:?}");
    let r = q_tab(&st, "edit-my", "select id, current_date as d from items").await;
    assert_eq!(r.editable, Some(Editable::ReadOnly(e::edit_computed("d"))));
    q_tab(&st, "edit-my", "create temporary table tmp (id int primary key)").await;
    assert_eq!(q_tab(&st, "edit-my", "select * from tmp").await.editable, Some(Editable::ReadOnly(e::edit_temporary(&format!("{s}.tmp")))));
    // A temporary table shadowing a base table of the same name: the console reads the temporary one, while edits (saved on
    // another connection) would change the base table
    q_tab(&st, "edit-my", "create temporary table items (id int primary key, name varchar(20), qty int)").await;
    q_tab(&st, "edit-my", "insert into items values (1, 'temp', 9)").await;
    let r = q_tab(&st, "edit-my", "select * from items").await;
    assert_eq!(r.rows[0][1], json!("temp"));
    assert_eq!(r.editable, Some(Editable::ReadOnly(e::edit_temporary(&format!("{s}.items")))));
    assert_eq!(q_tab(&st, "edit-my", &format!("select * from {s}.items")).await.editable, Some(Editable::ReadOnly(e::edit_temporary(&format!("{s}.items")))));
    st.close_session("edit-my".into()).await.unwrap();
    q(&st, &format!("drop database {s}")).await;
}

// ───────── Console results from joins, per table (#102); grid edits in Production's Manual transaction (#106) ─────────

/// The tables of #102's example (voc joined to users), a self-referencing tree and a table without a key, in `s`
async fn join_edit_setup(st: &AppState, s: &str) {
    for sql in [
        format!("create table {s}.voc (idx int primary key, user_idx int, status varchar(20))"),
        format!("create table {s}.users (idx int primary key, role varchar(20), status varchar(20))"),
        format!("create table {s}.tree (id int primary key, parent int, name varchar(20))"),
        format!("create table {s}.nokey (idx int, note varchar(20))"),
        format!("insert into {s}.voc values (1, 10, 'open'), (2, 99, 'new')"),
        format!("insert into {s}.users values (10, 'member', 'active'), (11, 'admin', 'active')"),
        format!("insert into {s}.tree values (1, null, 'root'), (2, 1, 'child')"),
        format!("insert into {s}.nokey values (1, 'n')"),
    ] {
        q(st, &sql).await;
    }
}

/// The UPDATE the GUI and the TUI build for one edited cell of a join result (ResultGrid buildStatements, TUI App::submit)
fn join_update(r: &db::QueryResult, kind: DbKind, row: usize, c: usize, value: &str) -> Result<String, String> {
    use qurry_core::editable::Editable;
    let Some(Editable::Join(j)) = &r.editable else { panic!("{:?}", r.editable) };
    let row = &r.rows[row];
    let (t, col) = j.cell(c, |i| row[i].is_null())?;
    let q = |x: &str| qurry_shared::sql::quote_ident(kind, x);
    let lit = |v: &Value| match v {
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        v => v.to_string(),
    };
    let key = t.pk.iter().zip(&t.pk_at).map(|(k, &i)| format!("{} = {}", q(k), lit(&row[i]))).collect::<Vec<_>>().join(" AND ");
    Ok(format!("UPDATE {}.{} SET {} = '{value}' WHERE {key}", q(&t.schema), q(&t.table), q(col)))
}

async fn one_value(st: &AppState, sql: &str) -> Value {
    q(st, sql).await.rows[0][0].clone()
}

async fn console_join_edit_cases(st: &AppState, s: &str, kind: DbKind) {
    use qurry_core::editable::{ColumnEdit, Editable, JoinEdit, JoinTable};
    use qurry_shared::errors as e;
    let v = |x: &[&str]| x.iter().map(|c| c.to_string()).collect::<Vec<_>>();
    let to = |t: usize, c: &str| ColumnEdit::To { table: t, column: c.into() };
    let ro = |why: String| ColumnEdit::ReadOnly { read_only: why };
    let jt = |table: &str, pk: &str, at: usize| JoinTable { schema: s.into(), table: table.into(), pk: v(&[pk]), pk_at: vec![at] };
    let join = |r: &db::QueryResult| match &r.editable {
        Some(Editable::Join(j)) => j.clone(),
        other => panic!("{other:?}"),
    };
    let tab = "join-edit";

    // #102's example: a left join, both keys in the result; idx and status twice, each by its table
    let left = format!("select * from {s}.voc as vo left join {s}.users as pu on vo.user_idx = pu.idx order by vo.idx");
    let r = q_tab(st, tab, &left).await;
    assert_eq!(
        join(&r),
        JoinEdit { tables: vec![jt("voc", "idx", 0), jt("users", "idx", 3)], columns: vec![to(0, "idx"), to(0, "user_idx"), to(0, "status"), to(1, "idx"), to(1, "role"), to(1, "status")] }
    );
    // The outer side where no user matched (voc 2 points at user 99): read-only there
    assert_eq!(join_update(&r, kind, 1, 4, "x"), Err(e::edit_no_row(&format!("{s}.users"))));
    assert!(join_update(&r, kind, 1, 2, "x").is_ok());
    // An edit of users.role changes exactly that one users row (Auto mode: committed at once)
    let up = join_update(&r, kind, 0, 4, "admin").unwrap();
    assert!(up.contains("users") && up.ends_with(&format!("WHERE {} = 10", qurry_shared::sql::quote_ident(kind, "idx"))), "{up}");
    assert_eq!(st.submit_edits(ID.into(), Some(tab.into()), vec![up]).await.unwrap().rows, 1);
    assert_eq!(one_value(st, &format!("select role from {s}.users where idx = 10")).await, json!("admin"));
    assert_eq!(one_value(st, &format!("select role from {s}.users where idx = 11")).await, json!("admin"));
    assert_eq!(one_value(st, &format!("select status from {s}.voc where idx = 1")).await, json!("open"));
    // The same name in the other table: voc.status, not users.status
    let up = join_update(&r, kind, 0, 2, "closed").unwrap();
    st.submit_edits(ID.into(), Some(tab.into()), vec![up]).await.unwrap();
    assert_eq!(one_value(st, &format!("select status from {s}.voc where idx = 1")).await, json!("closed"));
    assert_eq!(one_value(st, &format!("select status from {s}.users where idx = 10")).await, json!("active"));
    q(st, &format!("update {s}.users set role = 'member' where idx = 10")).await;

    // An inner join, aliases swapping the names; the key of each table found by origin, not by name
    let r = q_tab(st, tab, &format!("select pu.role as status, pu.status as role, pu.idx, vo.idx as user_idx from {s}.voc vo join {s}.users pu on pu.idx = vo.user_idx")).await;
    assert_eq!(join(&r), JoinEdit { tables: vec![jt("voc", "idx", 3), jt("users", "idx", 2)], columns: vec![to(1, "role"), to(1, "status"), to(1, "idx"), to(0, "idx")] });
    let up = join_update(&r, kind, 0, 1, "gone").unwrap();
    assert!(up.contains(&format!("SET {} = 'gone'", qurry_shared::sql::quote_ident(kind, "status"))), "{up}");

    // A key missing: that table's columns say why; none at all: read-only
    let missing = |t: &str| e::edit_missing_pk(&format!("{s}.{t}"), &v(&["idx"]));
    let r = q_tab(st, tab, &format!("select vo.idx, vo.status, pu.role from {s}.voc vo join {s}.users pu on pu.idx = vo.user_idx")).await;
    assert_eq!(join(&r), JoinEdit { tables: vec![jt("voc", "idx", 0)], columns: vec![to(0, "idx"), to(0, "status"), ro(missing("users"))] });
    let r = q_tab(st, tab, &format!("select vo.status, pu.role from {s}.voc vo join {s}.users pu on pu.idx = vo.user_idx")).await;
    assert_eq!(r.editable, Some(Editable::ReadOnly(missing("voc"))));
    // A table without a key, a computed column
    let r = q_tab(st, tab, &format!("select v.idx, n.note, upper(v.status) as up from {s}.voc v join {s}.nokey n on n.idx = v.idx")).await;
    assert_eq!(join(&r).columns, [to(0, "idx"), ro(e::edit_no_pk(&format!("{s}.nokey"))), ro(e::edit_computed("up"))]);

    // A self-join: each occurrence by its own key, the parent's name changes only the parent row
    let r = q_tab(st, tab, &format!("select c.id, c.name, p.id as pid, p.name as pname from {s}.tree c join {s}.tree p on c.parent = p.id")).await;
    assert_eq!(join(&r), JoinEdit { tables: vec![jt("tree", "id", 0), jt("tree", "id", 2)], columns: vec![to(0, "id"), to(0, "name"), to(1, "id"), to(1, "name")] });
    let up = join_update(&r, kind, 0, 3, "top").unwrap();
    st.submit_edits(ID.into(), Some(tab.into()), vec![up]).await.unwrap();
    assert_eq!(q(st, &format!("select name from {s}.tree order by id")).await.rows, [[json!("top")], [json!("child")]]);
    // `*` over a self-join: by place
    let r = q_tab(st, tab, &format!("select * from {s}.tree c join {s}.tree p on c.parent = p.id")).await;
    assert_eq!(join(&r).tables.iter().map(|t| t.pk_at.clone()).collect::<Vec<_>>(), [vec![0], vec![3]]);
    // ... but not with USING, which merges the key: which occurrence a column is can't be told
    let r = q_tab(st, tab, &format!("select * from {s}.tree a join {s}.tree b using (id)")).await;
    let why = match kind {
        DbKind::Mysql => e::EDIT_STAR_USING.to_string(),
        _ => e::edit_self_join(&format!("{s}.tree")),
    };
    assert_eq!(r.editable, Some(Editable::ReadOnly(why)));
    st.close_session(tab.into()).await.unwrap();
}

/// Grid edits in a Production console's Manual transaction (#106): they stay pending (the Production log has them, with undo)
/// until Commit / Roll back; a failing batch undoes only itself. Also the ask-free guard of a read-only source
async fn production_submit_in_manual_tx(st: &AppState, s: &str, kind: DbKind, read_only: &AppState) {
    let tab = "prod-tx";
    let role = |idx: i32| format!("select role from {s}.users where idx = {idx}");
    let src = st.store.get(ID).unwrap().unwrap();
    assert!(src.options.confirm_changes && src.starts_manual(), "Production starts consoles in Manual by default");
    let mut off = src.clone();
    off.options.production_manual = Some(false);
    assert!(!off.starts_manual());
    st.set_tx_mode(ID.into(), tab.into(), true).await.unwrap();
    let r = q_tab(st, tab, &format!("select vo.idx, pu.idx, pu.role from {s}.voc vo join {s}.users pu on pu.idx = vo.user_idx")).await;
    // A read opens no transaction on Production: the server sees nothing open and no lock on the table (an ALTER TABLE doesn't wait)
    assert!(!r.in_transaction && !st.tx_state(tab.into()).await, "a read leaves nothing pending");
    let pid = st.sessions.read().await.get(tab).unwrap().pid;
    let held = match kind {
        DbKind::Postgres => format!(
            "select (select count(*) from pg_stat_activity where pid = {pid} and state like 'idle in transaction%') + (select count(*) from pg_locks where pid = {pid} and relation = '{s}.users'::regclass)"
        ),
        _ => format!(
            "select (select count(*) from information_schema.innodb_trx where trx_mysql_thread_id = {pid}) + (select count(*) from performance_schema.metadata_locks m join performance_schema.threads t on t.thread_id = m.owner_thread_id where t.processlist_id = {pid} and m.object_schema = '{s}' and m.object_name = 'users')"
        ),
    };
    assert_eq!(one_value(st, &held).await.to_string(), "0", "the read holds nothing on the server");
    let up = join_update(&r, kind, 0, 2, "owner").unwrap();
    assert_eq!(st.submit_edits(ID.into(), Some(tab.into()), vec![up]).await.unwrap().rows, 1);
    assert!(st.tx_state(tab.into()).await, "still pending");
    assert_eq!(one_value(st, &role(10)).await, json!("member"), "not committed: another connection sees the old value");
    assert_eq!(q_tab(st, tab, &role(10)).await.rows[0][0], json!("owner"));
    let logged = st.audit_log(Some(ID.into()), "owner".into(), 1).remove(0);
    assert!(logged.in_tx && logged.undo.is_some() && logged.rows == Some(1), "{logged:?}");
    // A batch with one statement that changes two rows: none of it stays, the earlier edit still does
    let bad = vec![format!("update {s}.users set role = 'x' where idx = 11"), format!("update {s}.users set status = 'z' where status = 'active'")];
    let err = st.submit_edits(ID.into(), Some(tab.into()), bad).await.unwrap_err().to_string();
    assert!(err.contains("exactly 1 row but changed 2"), "{err}");
    assert!(st.tx_state(tab.into()).await);
    assert_eq!(q_tab(st, tab, &role(11)).await.rows[0][0], json!("admin"));
    assert_eq!(q_tab(st, tab, &role(10)).await.rows[0][0], json!("owner"));
    // Roll back: gone, and logged
    st.rollback_tx(tab.into()).await.unwrap();
    assert_eq!(one_value(st, &role(10)).await, json!("member"));
    assert_eq!(st.audit_log(Some(ID.into()), String::new(), 1)[0].sql, "ROLLBACK");
    // Commit: saved
    st.submit_edits(ID.into(), Some(tab.into()), vec![format!("update {s}.users set role = 'owner' where idx = 10")]).await.unwrap();
    st.commit_tx(tab.into()).await.unwrap();
    assert_eq!(one_value(st, &role(10)).await, json!("owner"));
    q(st, &format!("update {s}.users set role = 'member' where idx = 10")).await;
    st.close_session(tab.into()).await.unwrap();

    // A read-only source refuses the edits whether the console is in Auto or Manual mode, before anything is sent
    for manual in [false, true] {
        read_only.set_tx_mode(ID.into(), tab.into(), manual).await.unwrap();
        let err = read_only.submit_edits(ID.into(), Some(tab.into()), vec![format!("update {s}.users set role = 'x' where idx = 10")]).await.unwrap_err().to_string();
        assert!(err.contains("is read-only"), "{err}");
    }
    read_only.close_session(tab.into()).await.unwrap();
    assert_eq!(one_value(st, &role(10)).await, json!("member"));
}

/// The server ends a console's session that sat idle in its transaction (PostgreSQL idle_in_transaction_session_timeout, MySQL
/// wait_timeout, set for this session only): the next statement or Commit says so with the shared words, and that nothing was
/// committed; the Production log closes the transaction as rolled back by the server, not as a COMMIT of unknown outcome
async fn idle_transaction_ended_by_the_server(st: &AppState, s: &str, kind: DbKind) {
    use qurry_shared::errors as e;
    let mysql = kind == DbKind::Mysql;
    let tab = "idle-tx";
    st.set_tx_mode(ID.into(), tab.into(), true).await.unwrap();
    for end in ["statement", "commit"] {
        q_tab(st, tab, if mysql { "set session wait_timeout = 2" } else { "set idle_in_transaction_session_timeout = '1s'" }).await;
        q_tab(st, tab, &format!("update {s}.users set role = 'idle' where idx = 11")).await;
        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
        let err = match end {
            "commit" => st.commit_tx(tab.into()).await.map(|_| ()),
            _ => st.run_query(ID.into(), "select 1".into(), None, Some(tab.into())).await.map(|_| ()),
        }
        .unwrap_err()
        .to_string();
        assert!(err.contains(&e::idle_tx_ended(mysql)) && !err.contains(e::COMMIT_UNKNOWN_ERR), "{end}: {err}");
        assert_eq!(st.audit_log(Some(ID.into()), String::new(), 1)[0].sql, "ROLLBACK (by the server)", "{end}");
        assert_eq!(one_value(st, &format!("select role from {s}.users where idx = 11")).await, json!("admin"));
    }
    st.close_session(tab.into()).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_console_join_edit_and_production_tx() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    let s = "qm_join_edit";
    q(&st, &format!("drop schema if exists {s} cascade")).await;
    q(&st, &format!("create schema {s}")).await;
    join_edit_setup(&st, s).await;
    console_join_edit_cases(&st, s, DbKind::Postgres).await;
    let read_only = state(&ConnectionConfig { read_only: true, ..c.clone() }, None).await;
    production_submit_in_manual_tx(&st, s, DbKind::Postgres, &read_only).await;
    idle_transaction_ended_by_the_server(&st, s, DbKind::Postgres).await;
    q(&st, &format!("drop schema {s} cascade")).await;
}

#[tokio::test]
#[ignore]
async fn mysql_console_join_edit_and_production_tx() {
    let s = "qm_join_edit";
    let Some(st) = my_state(s).await else { return };
    let Some((c, pw)) = my_env() else { return };
    join_edit_setup(&st, s).await;
    console_join_edit_cases(&st, s, DbKind::Mysql).await;
    let read_only = state(&ConnectionConfig { read_only: true, database: s.into(), ..c }, Some(pw)).await;
    production_submit_in_manual_tx(&st, s, DbKind::Mysql, &read_only).await;
    idle_transaction_ended_by_the_server(&st, s, DbKind::Mysql).await;
    q(&st, &format!("drop database {s}")).await;
}

/// Completion metadata carries foreign keys (FK-aware joins) and user functions with their signatures (signature help)
#[tokio::test]
#[ignore]
async fn pg_completion_fks_and_functions() {
    let Some(c) = pg_env() else { return };
    let st = state(&c, None).await;
    pg_fixture(&st, "qc_completion").await;
    let comp = st.completion_meta(ID.into()).await.unwrap();
    let orders = comp.iter().find(|t| t.schema == "qc_completion" && t.name == "orders").expect("orders");
    assert_eq!(orders.fks, [qurry_shared::complete::ForeignKey { columns: vec!["customer_id".into()], ref_schema: "qc_completion".into(), ref_table: "customers".into(), ref_columns: vec!["id".into()] }]);
    let f = comp.iter().find(|t| t.schema == "qc_completion" && t.name == "twice").expect("twice");
    assert_eq!((f.columns.len(), f.routine.clone()), (0, Some(qurry_shared::complete::Routine { args: "x integer".into(), returns: "integer".into() })));
    q(&st, "drop schema qc_completion cascade").await;
}

#[tokio::test]
#[ignore]
async fn mysql_completion_fks_and_functions() {
    let Some(st) = my_state("qc_completion").await else { return };
    my_fixture(&st, "qc_completion").await;
    let comp = st.completion_meta(ID.into()).await.unwrap();
    let orders = comp.iter().find(|t| t.schema == "qc_completion" && t.name == "orders").expect("orders");
    assert_eq!(orders.fks, [qurry_shared::complete::ForeignKey { columns: vec!["customer_id".into()], ref_schema: "qc_completion".into(), ref_table: "customers".into(), ref_columns: vec!["id".into()] }]);
    let f = comp.iter().find(|t| t.schema == "qc_completion" && t.name == "twice").expect("twice");
    assert_eq!((f.columns.len(), f.routine.clone()), (0, Some(qurry_shared::complete::Routine { args: "x int".into(), returns: "int".into() })));
    q(&st, "drop database qc_completion").await;
}
