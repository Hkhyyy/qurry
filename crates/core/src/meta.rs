//! Metadata for the schema tree and autocomplete. Each kind's queries live in crate::backend.
use crate::backend::{Backend, dispatch};
use crate::db::Pool;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    pub name: String,
    pub is_system: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationInfo {
    pub name: String,
    /// table | partitioned | view | matview | foreign | sequence
    pub kind: String,
    pub comment: Option<String>,
    pub row_estimate: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInfo {
    pub name: String,
    pub args: String,
    pub returns: Option<String>,
    /// function | procedure | aggregate | window
    pub kind: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObjects {
    pub relations: Vec<RelationInfo>,
    pub routines: Vec<RoutineInfo>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_pk: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexMeta {
    pub name: String,
    pub definition: String,
    pub is_unique: bool,
    pub is_primary: bool,
    /// Key columns (or expressions) in index order; empty when the source doesn't say
    #[serde(default)]
    pub columns: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintMeta {
    pub name: String,
    /// p(primary) | f(foreign) | u(unique) | c(check)
    pub kind: String,
    pub definition: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDetail {
    pub columns: Vec<ColumnMeta>,
    pub indexes: Vec<IndexMeta>,
    pub constraints: Vec<ConstraintMeta>,
}

/// One foreign key, either direction: `schema.table(columns)` references `ref_schema.ref_table(ref_columns)`
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyMeta {
    pub name: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    /// NO ACTION | RESTRICT | CASCADE | SET NULL | SET DEFAULT
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerMeta {
    pub name: String,
    /// BEFORE | AFTER | INSTEAD OF
    pub timing: String,
    /// INSERT · UPDATE · DELETE · TRUNCATE joined with " OR "
    pub events: String,
    pub definition: String,
}

/// The DDL tab's Structure view of a table: table_detail plus the sections it doesn't carry.
/// A section that is None isn't available for that kind of data source (the view says so)
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    #[serde(flatten)]
    pub detail: TableDetail,
    pub foreign_keys: Option<Vec<ForeignKeyMeta>>,
    pub referenced_by: Option<Vec<ForeignKeyMeta>>,
    pub triggers: Option<Vec<TriggerMeta>>,
    /// (name, value): engine, owner, tablespace, partition key…
    pub options: Option<Vec<(String, String)>>,
    pub comment: Option<String>,
    /// Referenced by and Triggers list only what this account has privileges on (MySQL), so empty doesn't mean there are none
    #[serde(default)]
    pub only_visible: bool,
}

impl From<TableDetail> for TableStructure {
    fn from(detail: TableDetail) -> Self {
        TableStructure { detail, foreign_keys: None, referenced_by: None, triggers: None, options: None, comment: None, only_visible: false }
    }
}

/// Autocomplete metadata row: defined in qurry_shared::complete (the GUI's WASM and the TUI complete from it)
pub use qurry_shared::complete::CompletionTable;

pub async fn list_databases(pool: &Pool) -> AppResult<Vec<String>> {
    dispatch!(pool, b => b.list_databases().await)
}

pub async fn list_schemas(pool: &Pool) -> AppResult<Vec<SchemaInfo>> {
    dispatch!(pool, b => b.list_schemas().await)
}

pub async fn list_objects(pool: &Pool, schema: &str) -> AppResult<SchemaObjects> {
    dispatch!(pool, b => b.list_objects(schema).await)
}

pub async fn table_detail(pool: &Pool, schema: &str, table: &str) -> AppResult<TableDetail> {
    dispatch!(pool, b => b.table_detail(schema, table).await)
}

pub async fn table_structure(pool: &Pool, schema: &str, table: &str) -> AppResult<TableStructure> {
    dispatch!(pool, b => b.table_structure(schema, table).await)
}

/// Builds object DDL like DataGrip's "Go to DDL". kind: table|partitioned|foreign|view|matview|sequence|routine
pub async fn object_ddl(pool: &Pool, schema: &str, name: &str, kind: &str) -> AppResult<String> {
    dispatch!(pool, b => b.object_ddl(schema, name, kind).await)
}

/// For autocomplete: tables/views and columns in user schemas (up to 30,000 columns)
pub async fn completion_meta(pool: &Pool) -> AppResult<Vec<CompletionTable>> {
    dispatch!(pool, b => b.completion_meta().await)
}

/// Object list for Go to Database Object / Search Everywhere
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub schema: String,
    pub name: String,
    /// schema | table | view | matview | sequence | foreign | function | procedure
    pub kind: String,
}

pub async fn object_index(pool: &Pool) -> AppResult<Vec<IndexEntry>> {
    dispatch!(pool, b => b.object_index().await)
}

/// Full-text Search (Find in database)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtsRequest {
    pub text: String,
    /// contains | starts | ends | matches | like
    pub mode: String,
    pub match_case: bool,
    /// text | numeric | all
    pub columns: String,
    /// Schemas to search (when tables is empty)
    #[serde(default)]
    pub schemas: Vec<String>,
    /// Only specific tables as "schema.table"
    #[serde(default)]
    pub tables: Vec<(String, String)>,
    pub limit: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FtsHit {
    pub schema: String,
    pub table: String,
    /// WHERE clause that can go straight into the table editor
    pub where_clause: String,
    pub result: crate::db::QueryResult,
}

pub async fn full_text_search(pool: &Pool, r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
    dispatch!(pool, b => b.full_text_search(r).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    /// Real DB: QURRY_TEST_PG=host:port:user:db cargo test -p qurry-core meta -- --ignored
    async fn pg_meta() {
        let Some(s) = crate::testenv::pg() else { return };
        let pool = crate::db::connect(&s.cfg, s.password.as_deref()).await.unwrap();
        let Pool::Pg(p) = &pool else { unreachable!() };
        sqlx::query("create schema if not exists dbtool_meta_test").execute(p).await.unwrap();
        sqlx::query("create table if not exists dbtool_meta_test.parent(id bigserial primary key, name text not null default 'x')").execute(p).await.unwrap();
        sqlx::query("create table if not exists dbtool_meta_test.child(id int primary key, parent_id bigint references dbtool_meta_test.parent(id), note varchar(20))").execute(p).await.unwrap();
        sqlx::query("comment on table dbtool_meta_test.parent is '부모'").execute(p).await.unwrap();

        // Go to Database Object index
        let idx = object_index(&pool).await.unwrap();
        assert!(idx.iter().any(|e| e.schema == "dbtool_meta_test" && e.name == "parent" && e.kind == "table"));
        assert!(idx.iter().any(|e| e.name == "dbtool_meta_test" && e.kind == "schema"));
        // Full-text Search
        sqlx::query("insert into dbtool_meta_test.parent(id, name) values (424242, 'Hello_World 50%') on conflict do nothing").execute(p).await.unwrap();
        let mk = |text: &str, mode: &str, case: bool, cols: &str| FtsRequest {
            text: text.into(), mode: mode.into(), match_case: case, columns: cols.into(),
            schemas: vec!["dbtool_meta_test".into()], tables: vec![], limit: 10,
        };
        let h = full_text_search(&pool, &mk("world", "contains", false, "text")).await.unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].table, "parent");
        assert!(full_text_search(&pool, &mk("world", "contains", true, "text")).await.unwrap().is_empty(), "match case");
        assert_eq!(full_text_search(&pool, &mk("50%", "ends", false, "text")).await.unwrap().len(), 1, "% escaped");
        assert!(full_text_search(&pool, &mk("Hello_", "ends", false, "text")).await.unwrap().is_empty());
        assert_eq!(full_text_search(&pool, &mk("424242", "matches", false, "numeric")).await.unwrap().len(), 1);
        let w = &full_text_search(&pool, &mk("hello", "starts", false, "all")).await.unwrap()[0].where_clause;
        let n: i64 = sqlx::query_scalar(&format!("select count(*) from dbtool_meta_test.parent where {w}")).fetch_one(p).await.unwrap();
        assert_eq!(n, 1, "where clause reusable: {w}");

        let dbs = list_databases(&pool).await.unwrap();
        assert!(dbs.contains(&"postgres".to_string()));
        let schemas = list_schemas(&pool).await.unwrap();
        assert!(schemas.iter().any(|s| s.name == "dbtool_meta_test" && !s.is_system));
        let objs = list_objects(&pool, "dbtool_meta_test").await.unwrap();
        let names: Vec<_> = objs.relations.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect();
        println!("{names:?}");
        assert!(names.contains(&("parent", "table")) && names.contains(&("child", "table")) && names.contains(&("parent_id_seq", "sequence")));
        assert_eq!(objs.relations.iter().find(|r| r.name == "parent").unwrap().comment.as_deref(), Some("부모"));
        let d = table_detail(&pool, "dbtool_meta_test", "child").await.unwrap();
        let cols: Vec<_> = d.columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str(), c.is_pk)).collect();
        println!("{cols:?}\n{:?}", d.constraints.iter().map(|c| &c.definition).collect::<Vec<_>>());
        assert_eq!(cols, vec![("id", "integer", true), ("parent_id", "bigint", false), ("note", "character varying(20)", false)]);
        assert!(d.constraints.iter().any(|c| c.kind == "f" && c.definition.contains("REFERENCES dbtool_meta_test.parent(id)")));
        assert!(d.indexes.iter().any(|i| i.is_primary));
        sqlx::query("create or replace view dbtool_meta_test.v_parent as select id, name from dbtool_meta_test.parent where id > 0").execute(p).await.unwrap();
        sqlx::query("create index if not exists child_note_idx on dbtool_meta_test.child(note)").execute(p).await.unwrap();
        sqlx::query("comment on column dbtool_meta_test.child.note is 'memo''s'").execute(p).await.unwrap();
        sqlx::query("create or replace function dbtool_meta_test.add1(x int) returns int language sql as 'select x + 1'").execute(p).await.unwrap();
        let tddl = object_ddl(&pool, "dbtool_meta_test", "child", "table").await.unwrap();
        println!("{tddl}");
        assert!(tddl.contains("CREATE TABLE dbtool_meta_test.child"));
        assert!(tddl.contains("id integer NOT NULL"));
        assert!(tddl.contains("CONSTRAINT child_pkey PRIMARY KEY (id)"));
        assert!(tddl.contains("REFERENCES dbtool_meta_test.parent(id)"));
        assert!(tddl.contains("CREATE INDEX child_note_idx"));
        assert!(tddl.contains("IS 'memo''s'"));
        let pddl = object_ddl(&pool, "dbtool_meta_test", "parent", "table").await.unwrap();
        assert!(pddl.contains("DEFAULT nextval(") && pddl.contains("COMMENT ON TABLE dbtool_meta_test.parent IS '부모'"));
        let vddl = object_ddl(&pool, "dbtool_meta_test", "v_parent", "view").await.unwrap();
        println!("{vddl}");
        assert!(vddl.starts_with("CREATE OR REPLACE VIEW dbtool_meta_test.v_parent AS"));
        let sddl = object_ddl(&pool, "dbtool_meta_test", "parent_id_seq", "sequence").await.unwrap();
        assert!(sddl.contains("CREATE SEQUENCE dbtool_meta_test.parent_id_seq"));
        let fddl = object_ddl(&pool, "dbtool_meta_test", "add1", "routine").await.unwrap();
        assert!(fddl.contains("CREATE OR REPLACE FUNCTION dbtool_meta_test.add1"));
        let comp = completion_meta(&pool).await.unwrap();
        assert!(comp.iter().any(|t| t.schema == "dbtool_meta_test" && t.name == "child" && t.columns.len() == 3));
        sqlx::query("drop schema dbtool_meta_test cascade").execute(p).await.unwrap();
    }

    /// Real DB: QURRY_TEST_PG=… cargo test -p qurry-core table_structure -- --ignored
    #[tokio::test]
    #[ignore]
    async fn pg_table_structure() {
        let Some(s) = crate::testenv::pg() else { return };
        let pool = crate::db::connect(&s.cfg, s.password.as_deref()).await.unwrap();
        let Pool::Pg(p) = &pool else { unreachable!() };
        sqlx::raw_sql(
            "drop schema if exists qurry_ddl_structure cascade; create schema qurry_ddl_structure; set search_path = qurry_ddl_structure;
             create table parent(id int primary key, code text unique) with (fillfactor = 70);
             create table child(id int primary key, parent_id int references parent(id) on delete cascade, note text, boss int references child(id));
             create index child_note_id on child(note, lower(note), id);
             comment on table child is 'Child rows';
             create function touch() returns trigger language plpgsql as 'begin return new; end';
             create trigger child_touch before insert or update on child for each row execute function touch();",
        )
        .execute(p)
        .await
        .unwrap();
        let s = table_structure(&pool, "qurry_ddl_structure", "child").await.unwrap();
        // Outgoing: to parent and to itself
        let fks = s.foreign_keys.as_ref().unwrap();
        let fk = fks.iter().find(|f| f.ref_table == "parent").unwrap();
        assert_eq!((fk.table.as_str(), fk.columns.clone(), fk.ref_columns.clone(), fk.on_delete.as_str(), fk.on_update.as_str()), ("child", vec!["parent_id".to_string()], vec!["id".to_string()], "CASCADE", "NO ACTION"));
        assert!(fks.iter().any(|f| f.ref_table == "child" && f.columns == ["boss"]));
        // Incoming: only the self-reference points at child
        let inc = s.referenced_by.as_ref().unwrap();
        assert_eq!(inc.iter().map(|f| (f.table.as_str(), f.columns.join(","))).collect::<Vec<_>>(), [("child", "boss".to_string())]);
        let pin = table_structure(&pool, "qurry_ddl_structure", "parent").await.unwrap();
        let inc = pin.referenced_by.unwrap();
        assert_eq!(inc.iter().map(|f| (f.schema.as_str(), f.table.as_str(), f.columns.join(","), f.ref_columns.join(","))).collect::<Vec<_>>(), [("qurry_ddl_structure", "child", "parent_id".into(), "id".into())]);
        assert!(pin.foreign_keys.unwrap().is_empty());
        // Index columns in order, expressions included
        let ix = s.detail.indexes.iter().find(|i| i.name == "child_note_id").unwrap();
        assert_eq!(ix.columns, ["note", "lower(note)", "id"]);
        // Triggers
        let tr = s.triggers.as_ref().unwrap();
        assert_eq!(tr.iter().map(|t| (t.name.as_str(), t.timing.as_str(), t.events.as_str())).collect::<Vec<_>>(), [("child_touch", "BEFORE", "INSERT OR UPDATE")]);
        assert!(tr[0].definition.contains("EXECUTE FUNCTION"), "{}", tr[0].definition);
        // Options and comment
        assert_eq!(s.comment.as_deref(), Some("Child rows"));
        let opts = pin.options.unwrap();
        assert!(opts.iter().any(|(k, v)| k == "Storage parameters" && v == "fillfactor=70"), "{opts:?}");
        assert!(opts.iter().any(|(k, _)| k == "Owner"), "{opts:?}");
        // A table in the default tablespace is stored in the database's, which isn't always pg_default
        let (db_space,): (String,) = sqlx::query_as("select spcname::text from pg_tablespace where oid = (select dattablespace from pg_database where datname = current_database())").fetch_one(p).await.unwrap();
        assert!(opts.contains(&("Tablespace".to_string(), db_space)), "{opts:?}");
        sqlx::query("drop schema qurry_ddl_structure cascade").execute(p).await.unwrap();
    }

    /// Real DB: QURRY_TEST_MY=… cargo test -p qurry-core table_structure -- --ignored
    #[tokio::test]
    #[ignore]
    async fn my_table_structure() {
        let Some(s) = crate::testenv::my() else { return };
        let pool = crate::db::connect(&s.cfg, s.password.as_deref()).await.unwrap();
        let (s_cfg, s_pw) = (s.cfg.clone(), s.password.clone().unwrap_or_default());
        let Pool::My(p) = &pool else { unreachable!() };
        for q in [
            "drop database if exists qurry_ddl_structure",
            "create database qurry_ddl_structure",
            "create table qurry_ddl_structure.parent(id int primary key auto_increment, code varchar(20) unique) engine=InnoDB comment='Parents'",
            "insert into qurry_ddl_structure.parent(code) values ('a'), ('b')",
            "create table qurry_ddl_structure.child(id int primary key, parent_id int, note varchar(40), boss int, \
               index child_note_id (note, id), index mx (note, (upper(note)), id), index fx ((lower(note))), foreign key fk_parent (parent_id) references qurry_ddl_structure.parent(id) on delete cascade, \
               constraint fk_boss foreign key (boss) references qurry_ddl_structure.child(id))",
            "create trigger qurry_ddl_structure.child_touch before insert on qurry_ddl_structure.child for each row set new.note = coalesce(new.note, '-')",
        ] {
            sqlx::raw_sql(q).execute(p).await.unwrap_or_else(|e| panic!("{q}: {e}"));
        }
        let s = table_structure(&pool, "qurry_ddl_structure", "child").await.unwrap();
        let fks = s.foreign_keys.as_ref().unwrap();
        let fk = fks.iter().find(|f| f.ref_table == "parent").unwrap();
        assert_eq!((fk.columns.clone(), fk.ref_columns.clone(), fk.on_delete.as_str()), (vec!["parent_id".to_string()], vec!["id".to_string()], "CASCADE"));
        assert!(fks.iter().any(|f| f.name == "fk_boss" && f.ref_table == "child"));
        assert_eq!(s.referenced_by.as_ref().unwrap().iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["fk_boss"]);
        let ix = s.detail.indexes.iter().find(|i| i.name == "child_note_id").unwrap();
        assert_eq!(ix.columns, ["note", "id"]);
        // Functional key parts come as their expression; an index of expressions only doesn't fail the whole call
        let ix = |n: &str| s.detail.indexes.iter().find(|i| i.name == n).unwrap();
        assert_eq!(ix("mx").columns, ["note", "(upper(`note`))", "id"]);
        assert_eq!(ix("mx").definition, "INDEX `mx` (note, (upper(`note`)), id)");
        assert_eq!(ix("fx").columns, ["(lower(`note`))"]);
        let tr = s.triggers.as_ref().unwrap();
        assert_eq!(tr.iter().map(|t| (t.name.as_str(), t.timing.as_str(), t.events.as_str())).collect::<Vec<_>>(), [("child_touch", "BEFORE", "INSERT")]);
        assert!(tr[0].definition.contains("coalesce"), "{}", tr[0].definition);
        let pin = table_structure(&pool, "qurry_ddl_structure", "parent").await.unwrap();
        assert_eq!(pin.referenced_by.unwrap().iter().map(|f| (f.table.as_str(), f.columns.join(","))).collect::<Vec<_>>(), [("child", "parent_id".to_string())]);
        assert_eq!(pin.comment.as_deref(), Some("Parents"));
        let opts = pin.options.unwrap();
        assert!(opts.iter().any(|(k, v)| k == "Engine" && v == "InnoDB"), "{opts:?}");
        // information_schema's cached AUTO_INCREMENT would lag SHOW CREATE TABLE, so it isn't shown
        assert!(!opts.iter().any(|(k, _)| k.contains("increment")), "{opts:?}");
        // A server whose sql_mode has NO_BACKSLASH_ESCAPES: multi-column keys still split into columns
        let opts = sqlx::mysql::MySqlConnectOptions::new().host(&s_cfg.host).port(s_cfg.port).username(&s_cfg.user).password(&s_pw).database("qurry_ddl_structure");
        let raw = sqlx::mysql::MySqlPoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
        // (one connection, so the session setting holds for the calls below)
        sqlx::raw_sql("set session sql_mode = concat(@@sql_mode, ',NO_BACKSLASH_ESCAPES')").execute(&raw).await.unwrap();
        let s = crate::mysql::table_structure(&raw, "qurry_ddl_structure", "child").await.unwrap();
        assert_eq!(s.detail.indexes.iter().find(|i| i.name == "child_note_id").unwrap().columns, ["note", "id"]);
        assert!(s.foreign_keys.unwrap().iter().all(|f| f.columns.len() == 1));
        sqlx::raw_sql("drop database qurry_ddl_structure").execute(p).await.unwrap();
    }

    /// Real DB: an account with SELECT on one database only doesn't see the cascading key from another database nor the
    /// trigger (MySQL lists only what it has privileges on), so the structure says its lists are only what's visible
    #[tokio::test]
    #[ignore]
    async fn my_table_structure_limited_account() {
        let Some(s) = crate::testenv::my() else { return };
        let pool = crate::db::connect(&s.cfg, s.password.as_deref()).await.unwrap();
        let Pool::My(p) = &pool else { unreachable!() };
        let setup = [
            "drop user if exists 'qurry_ddlsv_ro'@'%'",
            "drop database if exists qurry_ddlsv_a",
            "drop database if exists qurry_ddlsv_b",
            "create database qurry_ddlsv_a",
            "create database qurry_ddlsv_b",
            "create table qurry_ddlsv_a.parent(id int primary key)",
            "create table qurry_ddlsv_b.child(id int primary key, pid int, foreign key (pid) references qurry_ddlsv_a.parent(id) on delete cascade)",
            "create trigger qurry_ddlsv_a.parent_del before delete on qurry_ddlsv_a.parent for each row set @n = 1",
            "create user 'qurry_ddlsv_ro'@'%' identified by 'ro_pw_1'",
            "grant select on qurry_ddlsv_a.* to 'qurry_ddlsv_ro'@'%'",
        ];
        for q in setup {
            sqlx::raw_sql(q).execute(p).await.unwrap_or_else(|e| panic!("{q}: {e}"));
        }
        let full = table_structure(&pool, "qurry_ddlsv_a", "parent").await.unwrap();
        assert_eq!((full.referenced_by.as_ref().unwrap().len(), full.triggers.as_ref().unwrap().len()), (1, 1));
        let mut cfg = s.cfg.clone();
        cfg.user = "qurry_ddlsv_ro".into();
        cfg.database = "qurry_ddlsv_a".into();
        let ro = crate::db::connect(&cfg, Some("ro_pw_1")).await.unwrap();
        let seen = table_structure(&ro, "qurry_ddlsv_a", "parent").await;
        for q in ["drop user 'qurry_ddlsv_ro'@'%'", "drop database qurry_ddlsv_b", "drop database qurry_ddlsv_a"] {
            sqlx::raw_sql(q).execute(p).await.unwrap();
        }
        let seen = seen.unwrap();
        assert!(seen.referenced_by.unwrap().is_empty() && seen.triggers.unwrap().is_empty());
        assert!(seen.only_visible && full.only_visible);
    }
}
