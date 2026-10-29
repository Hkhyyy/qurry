//! Metadata queries for the schema tree and autocomplete (based on PostgreSQL pg_catalog).
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<(String, String)>,
}

pub async fn list_databases(pool: &Pool) -> AppResult<Vec<String>> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::list_databases(m).await,
        Pool::Redis(r, cur) => return crate::redisdb::list_databases(r, *cur).await,
        Pool::Plugin(c) => return c.call("databases", serde_json::Value::Null).await,
    };
    let rows: Vec<(String,)> = sqlx::query_as(
        "select datname::text from pg_database where datallowconn and not datistemplate order by datname",
    )
    .fetch_all(p)
    .await?;
    Ok(rows.into_iter().map(|(d,)| d).collect())
}

pub async fn list_schemas(pool: &Pool) -> AppResult<Vec<SchemaInfo>> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::list_schemas(m).await,
        Pool::Redis(_, cur) => return Ok(crate::redisdb::list_schemas(&format!("db{cur}"))),
        Pool::Plugin(c) => return c.call("schemas", serde_json::json!({})).await,
    };
    let rows: Vec<(String,)> = sqlx::query_as(
        "select nspname::text from pg_namespace \
         where nspname not like 'pg_toast%' and nspname not like 'pg_temp_%' \
         order by (nspname in ('pg_catalog','information_schema')), nspname",
    )
    .fetch_all(p)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name,)| SchemaInfo { is_system: name == "pg_catalog" || name == "information_schema", name })
        .collect())
}

pub async fn list_objects(pool: &Pool, schema: &str) -> AppResult<SchemaObjects> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::list_objects(m, schema).await,
        Pool::Redis(r, _) => return crate::redisdb::list_objects(r).await,
        Pool::Plugin(c) => return c.call("objects", serde_json::json!({ "schema": schema })).await,
    };
    let rels: Vec<(String, String, Option<String>, Option<f32>)> = sqlx::query_as(
        "select c.relname::text, c.relkind::text, obj_description(c.oid, 'pg_class'), c.reltuples \
         from pg_class c join pg_namespace n on n.oid = c.relnamespace \
         where n.nspname = $1 and c.relkind in ('r','p','v','m','f','S') and not c.relispartition \
         order by c.relname",
    )
    .bind(schema)
    .fetch_all(p)
    .await?;
    let routines: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "select p.proname::text, pg_get_function_identity_arguments(p.oid), pg_get_function_result(p.oid), p.prokind::text \
         from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
         where n.nspname = $1 order by p.proname",
    )
    .bind(schema)
    .fetch_all(p)
    .await?;
    Ok(SchemaObjects {
        relations: rels
            .into_iter()
            .map(|(name, k, comment, tuples)| RelationInfo {
                name,
                kind: match k.as_str() {
                    "r" => "table",
                    "p" => "partitioned",
                    "v" => "view",
                    "m" => "matview",
                    "f" => "foreign",
                    _ => "sequence",
                }
                .into(),
                comment,
                row_estimate: tuples.filter(|t| *t >= 0.0).map(|t| t as i64),
            })
            .collect(),
        routines: routines
            .into_iter()
            .map(|(name, args, returns, k)| RoutineInfo {
                name,
                args,
                returns,
                kind: match k.as_str() {
                    "p" => "procedure",
                    "a" => "aggregate",
                    "w" => "window",
                    _ => "function",
                }
                .into(),
            })
            .collect(),
    })
}

const REL_OID: &str = "(select c.oid from pg_class c join pg_namespace n on n.oid = c.relnamespace where n.nspname = $1 and c.relname = $2)";

pub async fn table_detail(pool: &Pool, schema: &str, table: &str) -> AppResult<TableDetail> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::table_detail(m, schema, table).await,
        Pool::Redis(..) => return Ok(TableDetail { columns: vec![], indexes: vec![], constraints: vec![] }),
        Pool::Plugin(c) => return c.call("tableDetail", serde_json::json!({ "schema": schema, "table": table })).await,
    };
    let cols: Vec<(String, String, bool, Option<String>, Option<String>, bool)> = sqlx::query_as(&format!(
        "select a.attname::text, format_type(a.atttypid, a.atttypmod), not a.attnotnull, \
                pg_get_expr(d.adbin, d.adrelid), col_description(a.attrelid, a.attnum), \
                coalesce(a.attnum = any(pk.conkey), false) \
         from pg_attribute a \
         left join pg_attrdef d on d.adrelid = a.attrelid and d.adnum = a.attnum \
         left join pg_constraint pk on pk.conrelid = a.attrelid and pk.contype = 'p' \
         where a.attrelid = {REL_OID} and a.attnum > 0 and not a.attisdropped \
         order by a.attnum"
    ))
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let idx: Vec<(String, String, bool, bool)> = sqlx::query_as(&format!(
        "select i.relname::text, pg_get_indexdef(ix.indexrelid), ix.indisunique, ix.indisprimary \
         from pg_index ix join pg_class i on i.oid = ix.indexrelid \
         where ix.indrelid = {REL_OID} order by ix.indisprimary desc, i.relname"
    ))
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    let cons: Vec<(String, String, String)> = sqlx::query_as(&format!(
        "select conname::text, contype::text, pg_get_constraintdef(oid) from pg_constraint \
         where conrelid = {REL_OID} and contype in ('p','f','u','c') order by contype, conname"
    ))
    .bind(schema)
    .bind(table)
    .fetch_all(p)
    .await?;
    Ok(TableDetail {
        columns: cols
            .into_iter()
            .map(|(name, data_type, nullable, default_value, comment, is_pk)| ColumnMeta {
                name,
                data_type,
                nullable,
                default_value,
                comment,
                is_pk,
            })
            .collect(),
        indexes: idx
            .into_iter()
            .map(|(name, definition, is_unique, is_primary)| IndexMeta { name, definition, is_unique, is_primary })
            .collect(),
        constraints: cons.into_iter().map(|(name, kind, definition)| ConstraintMeta { name, kind, definition }).collect(),
    })
}

fn qi(s: &str) -> String {
    let simple = s.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if simple { s.to_string() } else { format!("\"{}\"", s.replace('"', "\"\"")) }
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Builds object DDL like DataGrip's "Go to DDL". kind: table|partitioned|foreign|view|matview|sequence|routine
pub async fn object_ddl(pool: &Pool, schema: &str, name: &str, kind: &str) -> AppResult<String> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::object_ddl(m, schema, name, kind).await,
        Pool::Redis(r, _) => return crate::redisdb::key_ddl(r, name).await,
        Pool::Plugin(c) => return c.call("ddl", serde_json::json!({ "schema": schema, "name": name, "kind": kind })).await,
    };
    let full = format!("{}.{}", qi(schema), qi(name));
    match kind {
        "view" | "matview" => {
            let (def,): (String,) = sqlx::query_as(&format!("select pg_get_viewdef({REL_OID}, true)"))
                .bind(schema)
                .bind(name)
                .fetch_one(p)
                .await?;
            let head = if kind == "view" { "CREATE OR REPLACE VIEW" } else { "CREATE MATERIALIZED VIEW" };
            Ok(format!("{head} {full} AS\n{}\n", def.trim_end().trim_end_matches(';').to_string() + ";"))
        }
        "sequence" => {
            let row: (i64, i64, i64, i64, bool, i64) = sqlx::query_as(
                "select start_value, increment_by, min_value, max_value, cycle, cache_size from pg_sequences where schemaname = $1 and sequencename = $2",
            )
            .bind(schema)
            .bind(name)
            .fetch_one(p)
            .await?;
            Ok(format!(
                "CREATE SEQUENCE {full}\n    START WITH {}\n    INCREMENT BY {}\n    MINVALUE {}\n    MAXVALUE {}\n    CACHE {}{};\n",
                row.0, row.1, row.2, row.3, row.5, if row.4 { "\n    CYCLE" } else { "" }
            ))
        }
        "routine" => {
            let defs: Vec<(String,)> = sqlx::query_as(
                "select pg_get_functiondef(p.oid) from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
                 where n.nspname = $1 and p.proname = $2 and p.prokind in ('f','p') order by p.oid",
            )
            .bind(schema)
            .bind(name)
            .fetch_all(p)
            .await?;
            Ok(defs.into_iter().map(|(d,)| format!("{};\n", d.trim_end())).collect::<Vec<_>>().join("\n"))
        }
        _ => {
            let d = table_detail(pool, schema, name).await?;
            let mut lines: Vec<String> = d
                .columns
                .iter()
                .map(|c| {
                    let mut l = format!("    {} {}", qi(&c.name), c.data_type);
                    if let Some(def) = &c.default_value {
                        l += &format!(" DEFAULT {def}");
                    }
                    if !c.nullable {
                        l += " NOT NULL";
                    }
                    l
                })
                .collect();
            for c in &d.constraints {
                lines.push(format!("    CONSTRAINT {} {}", qi(&c.name), c.definition));
            }
            let mut out = format!("CREATE TABLE {full}\n(\n{}\n);\n", lines.join(",\n"));
            // Remaining indexes, excluding ones created by constraints
            let constraint_names: Vec<&str> = d.constraints.iter().map(|c| c.name.as_str()).collect();
            for i in d.indexes.iter().filter(|i| !constraint_names.contains(&i.name.as_str())) {
                out += &format!("\n{};\n", i.definition);
            }
            let comment: Option<(Option<String>,)> = sqlx::query_as(&format!("select obj_description({REL_OID}, 'pg_class')"))
                .bind(schema)
                .bind(name)
                .fetch_optional(p)
                .await?;
            if let Some((Some(c),)) = comment {
                out += &format!("\nCOMMENT ON TABLE {full} IS {};\n", lit(&c));
            }
            for c in d.columns.iter().filter(|c| c.comment.is_some()) {
                out += &format!("COMMENT ON COLUMN {full}.{} IS {};\n", qi(&c.name), lit(c.comment.as_ref().unwrap()));
            }
            Ok(out)
        }
    }
}

/// Driver: schema → table → column (up to 300 tables)
async fn plugin_completion(pool: &Pool) -> AppResult<Vec<CompletionTable>> {
    let mut out = vec![];
    for s in list_schemas(pool).await? {
        for r in list_objects(pool, &s.name).await?.relations {
            if out.len() >= 300 {
                return Ok(out);
            }
            let cols = table_detail(pool, &s.name, &r.name).await.map(|d| d.columns.into_iter().map(|c| (c.name, c.data_type)).collect()).unwrap_or_default();
            out.push(CompletionTable { schema: s.name.clone(), name: r.name, columns: cols });
        }
    }
    Ok(out)
}

/// For autocomplete: tables/views and columns in user schemas (up to 30,000 columns)
pub async fn completion_meta(pool: &Pool) -> AppResult<Vec<CompletionTable>> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::completion_meta(m).await,
        Pool::Redis(..) => return Ok(vec![]),
        Pool::Plugin(_) => return plugin_completion(pool).await,
    };
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "select n.nspname::text, c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod) \
         from pg_attribute a join pg_class c on c.oid = a.attrelid join pg_namespace n on n.oid = c.relnamespace \
         where c.relkind in ('r','p','v','m','f') and a.attnum > 0 and not a.attisdropped \
           and n.nspname not in ('pg_catalog','information_schema') and n.nspname not like 'pg_toast%' \
         order by n.nspname, c.relname, a.attnum limit 30000",
    )
    .fetch_all(p)
    .await?;
    let mut out: Vec<CompletionTable> = Vec::new();
    for (schema, name, col, ty) in rows {
        match out.last_mut() {
            Some(t) if t.schema == schema && t.name == name => t.columns.push((col, ty)),
            _ => out.push(CompletionTable { schema, name, columns: vec![(col, ty)] }),
        }
    }
    Ok(out)
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
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::object_index(m).await,
        Pool::Redis(r, cur) => return crate::redisdb::object_index(r, &format!("db{cur}")).await,
        Pool::Plugin(_) => {
            let mut out = vec![];
            for s in list_schemas(pool).await? {
                out.push(IndexEntry { schema: s.name.clone(), name: s.name.clone(), kind: "schema".into() });
                for r in list_objects(pool, &s.name).await?.relations {
                    out.push(IndexEntry { schema: s.name.clone(), name: r.name, kind: r.kind });
                }
            }
            return Ok(out);
        }
    };
    const USER_NS: &str = "n.nspname not in ('pg_catalog','information_schema') and n.nspname not like 'pg_toast%' and n.nspname not like 'pg_temp%'";
    let sql = format!(
        "select n.nspname::text, n.nspname::text, 'schema' from pg_namespace n where {USER_NS} \
         union all \
         select n.nspname::text, c.relname::text, case c.relkind when 'v' then 'view' when 'm' then 'matview' when 'S' then 'sequence' when 'f' then 'foreign' else 'table' end \
           from pg_class c join pg_namespace n on n.oid = c.relnamespace where c.relkind in ('r','p','v','m','S','f') and not c.relispartition and {USER_NS} \
         union all \
         select n.nspname::text, p.proname::text, case p.prokind when 'p' then 'procedure' else 'function' end \
           from pg_proc p join pg_namespace n on n.oid = p.pronamespace where p.prokind in ('f','p') and {USER_NS} \
         limit 50000"
    );
    let rows: Vec<(String, String, String)> = sqlx::query_as(&sql).fetch_all(p).await?;
    Ok(rows.into_iter().map(|(schema, name, kind)| IndexEntry { schema, name, kind }).collect())
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

fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Condition for one column
fn fts_cond(col: &str, numeric: bool, r: &FtsRequest) -> String {
    let c = ident(col);
    let esc = |t: &str| t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let t = &r.text;
    if numeric {
        return format!("{c}::text = {}", lit(t.trim()));
    }
    let op = if r.match_case { "like" } else { "ilike" };
    match r.mode.as_str() {
        "matches" if r.match_case => format!("{c}::text = {}", lit(t)),
        "matches" => format!("lower({c}::text) = lower({})", lit(t)),
        "starts" => format!("{c}::text {op} {}", lit(&format!("{}%", esc(t)))),
        "ends" => format!("{c}::text {op} {}", lit(&format!("%{}", esc(t)))),
        "like" => format!("{c}::text {op} {}", lit(t)),
        _ => format!("{c}::text {op} {}", lit(&format!("%{}%", esc(t)))),
    }
}

pub async fn full_text_search(pool: &Pool, r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
    let p = match pool {
        Pool::Pg(p) => p,
        Pool::My(m) => return crate::mysql::full_text_search(m, r).await,
        Pool::Redis(..) => return Err(crate::error::AppError::Msg("Full-text search is not available for Redis; use SCAN MATCH in the console".into())),
        Pool::Plugin(c) => return Err(crate::error::AppError::Msg(format!("Full-text search is not available for the {} driver yet", c.driver))),
    };
    // Target tables · columns and type class (S=string, N=number)
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "select n.nspname::text, c.relname::text, a.attname::text, t.typcategory::text \
         from pg_attribute a join pg_class c on c.oid = a.attrelid join pg_namespace n on n.oid = c.relnamespace \
         join pg_type t on t.oid = a.atttypid \
         where c.relkind in ('r','p','v','m') and not c.relispartition and a.attnum > 0 and not a.attisdropped \
           and (n.nspname = any($1) or (n.nspname || '.' || c.relname) = any($2)) \
         order by n.nspname, c.relname, a.attnum",
    )
    .bind(&r.schemas)
    .bind(r.tables.iter().map(|(s, t)| format!("{s}.{t}")).collect::<Vec<_>>())
    .fetch_all(p)
    .await?;
    let numeric_text = r.text.trim().parse::<f64>().is_ok();
    let mut tables: Vec<(String, String, Vec<String>)> = Vec::new();
    for (schema, table, col, cat) in rows {
        let numeric = cat == "N";
        let take = match r.columns.as_str() {
            "numeric" => numeric && numeric_text,
            "all" => !numeric || numeric_text,
            _ => cat == "S",
        };
        let cond = if take { Some(fts_cond(&col, numeric, r)) } else { None };
        match tables.last_mut() {
            Some((s, t, conds)) if *s == schema && *t == table => conds.extend(cond),
            _ => tables.push((schema, table, cond.into_iter().collect())),
        }
    }
    let mut hits = Vec::new();
    for (schema, table, conds) in tables.into_iter().filter(|t| !t.2.is_empty()) {
        let where_clause = conds.join(" OR ");
        let sql = format!("select * from {}.{} where {where_clause} limit {}", ident(&schema), ident(&table), r.limit.max(1));
        // Per-statement timeout so one slow table doesn't stall everything
        let mut tx = p.begin().await?;
        sqlx::query("set local statement_timeout = '15s'").execute(&mut *tx).await?;
        let res = crate::db::run_on(&mut *tx, &sql, r.limit.max(1)).await;
        let _ = tx.rollback().await;
        match res {
            Ok(result) if !result.rows.is_empty() => hits.push(FtsHit { schema, table, where_clause, result }),
            Ok(_) => {}
            Err(e) => crate::log!("[qurry] fts {schema}.{table}: {e}"),
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ConnectionConfig, DbKind};

    #[tokio::test]
    #[ignore]
    async fn pg_meta() {
        let cfg = ConnectionConfig {
            id: "t".into(),
            name: "t".into(),
            kind: DbKind::Postgres,
            host: "localhost".into(),
            port: 5432,
            user: std::env::var("USER").unwrap(),
            database: String::new(),
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
