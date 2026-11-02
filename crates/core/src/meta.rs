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
    use crate::store::{ConnectionConfig, DbKind};

    #[tokio::test]
    #[ignore]
    /// Real DB: QURRY_TEST_PG=host:port:user:db cargo test -p qurry-core meta -- --ignored
    async fn pg_meta() {
        let Ok(env) = std::env::var("QURRY_TEST_PG") else { return };
        let s: Vec<&str> = env.split(':').collect();
        let cfg = ConnectionConfig {
            id: "t".into(),
            name: "t".into(),
            kind: DbKind::Postgres,
            host: s[0].into(),
            port: s[1].parse().unwrap(),
            user: s[2].into(),
            database: s[3].into(),
            color: None,
            ssh: None,
            password_saved: false,
            ssh_secret_saved: false,
            scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        let pool = crate::db::connect(&cfg, None).await.unwrap();
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
}
