//! PostgreSQL: pg_catalog queries, connecting (TLS rules), MCP reads in a READ ONLY transaction, batches. The console session is crate::session::PgSession
use super::{Backend, Snapshots};
use crate::db::QueryResult;
use crate::ddl::{ColumnDef, ForeignKeyDef, IndexDef, TableDef, action};
use crate::erd::{Columns, Erd, ErdColumn, ErdRelation, ErdTable};
use crate::error::{AppError, AppResult};
use crate::fake::{Col, Fk, TableShape, quoted_literals, type_args};
use crate::hints::{Hint, HintReq};
use crate::meta::{ColumnMeta, CompletionTable, ConstraintMeta, FtsHit, FtsRequest, IndexEntry, IndexMeta, RelationInfo, RoutineInfo, SchemaInfo, SchemaObjects, TableDetail};
use crate::session::{PgSession, Session, check_exact_one, pg_snapshot};
use crate::store::{ConnectionConfig, DbKind};
use crate::transfer::Exporter;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Executor, PgPool};
use std::time::Duration;

const REL_OID: &str = "(select c.oid from pg_class c join pg_namespace n on n.oid = c.relnamespace where n.nspname = $1 and c.relname = $2)";

fn qi(s: &str) -> String {
    let simple = s.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if simple { s.to_string() } else { format!("\"{}\"", s.replace('"', "\"\"")) }
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Full-text search condition for one column
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

impl Backend for PgPool {
    fn label(&self) -> String {
        "PostgreSQL".into()
    }

    // ───── Metadata: meta.rs · hints.rs · erd.rs · ddl.rs · fake.rs ─────

    async fn list_databases(&self) -> AppResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "select datname::text from pg_database where datallowconn and not datistemplate order by datname",
        )
        .fetch_all(self)
        .await?;
        Ok(rows.into_iter().map(|(d,)| d).collect())
    }

    async fn list_schemas(&self) -> AppResult<Vec<SchemaInfo>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "select nspname::text from pg_namespace \
             where nspname not like 'pg_toast%' and nspname not like 'pg_temp_%' \
             order by (nspname in ('pg_catalog','information_schema')), nspname",
        )
        .fetch_all(self)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(name,)| SchemaInfo { is_system: name == "pg_catalog" || name == "information_schema", name })
            .collect())
    }

    async fn list_objects(&self, schema: &str) -> AppResult<SchemaObjects> {
        let rels: Vec<(String, String, Option<String>, Option<f32>)> = sqlx::query_as(
            "select c.relname::text, c.relkind::text, obj_description(c.oid, 'pg_class'), c.reltuples \
             from pg_class c join pg_namespace n on n.oid = c.relnamespace \
             where n.nspname = $1 and c.relkind in ('r','p','v','m','f','S') and not c.relispartition \
             order by c.relname",
        )
        .bind(schema)
        .fetch_all(self)
        .await?;
        let routines: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "select p.proname::text, pg_get_function_identity_arguments(p.oid), pg_get_function_result(p.oid), p.prokind::text \
             from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
             where n.nspname = $1 order by p.proname",
        )
        .bind(schema)
        .fetch_all(self)
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

    async fn table_detail(&self, schema: &str, table: &str) -> AppResult<TableDetail> {
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
        .fetch_all(self)
        .await?;
        let idx: Vec<(String, String, bool, bool)> = sqlx::query_as(&format!(
            "select i.relname::text, pg_get_indexdef(ix.indexrelid), ix.indisunique, ix.indisprimary \
             from pg_index ix join pg_class i on i.oid = ix.indexrelid \
             where ix.indrelid = {REL_OID} order by ix.indisprimary desc, i.relname"
        ))
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        let cons: Vec<(String, String, String)> = sqlx::query_as(&format!(
            "select conname::text, contype::text, pg_get_constraintdef(oid) from pg_constraint \
             where conrelid = {REL_OID} and contype in ('p','f','u','c') order by contype, conname"
        ))
        .bind(schema)
        .bind(table)
        .fetch_all(self)
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

    async fn object_ddl(&self, schema: &str, name: &str, kind: &str) -> AppResult<String> {
        let full = format!("{}.{}", qi(schema), qi(name));
        match kind {
            "view" | "matview" => {
                let (def,): (String,) = sqlx::query_as(&format!("select pg_get_viewdef({REL_OID}, true)"))
                    .bind(schema)
                    .bind(name)
                    .fetch_one(self)
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
                .fetch_one(self)
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
                .fetch_all(self)
                .await?;
                Ok(defs.into_iter().map(|(d,)| format!("{};\n", d.trim_end())).collect::<Vec<_>>().join("\n"))
            }
            _ => {
                let d = self.table_detail(schema, name).await?;
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
                    .fetch_optional(self)
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

    async fn completion_meta(&self) -> AppResult<Vec<CompletionTable>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "select n.nspname::text, c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod) \
             from pg_attribute a join pg_class c on c.oid = a.attrelid join pg_namespace n on n.oid = c.relnamespace \
             where c.relkind in ('r','p','v','m','f') and a.attnum > 0 and not a.attisdropped \
               and n.nspname not in ('pg_catalog','information_schema') and n.nspname not like 'pg_toast%' \
             order by n.nspname, c.relname, a.attnum limit 30000",
        )
        .fetch_all(self)
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

    async fn object_index(&self) -> AppResult<Vec<IndexEntry>> {
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
        let rows: Vec<(String, String, String)> = sqlx::query_as(&sql).fetch_all(self).await?;
        Ok(rows.into_iter().map(|(schema, name, kind)| IndexEntry { schema, name, kind }).collect())
    }

    async fn full_text_search(&self, r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
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
        .fetch_all(self)
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
            let mut tx = self.begin().await?;
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

    async fn column_hints(&self, req: &HintReq) -> AppResult<Vec<Hint>> {
        use crate::hints::{MAX_HINTS, keys_sql, pairs, pairs_sql, values_sql};
        use sqlx::Executor;
        let sql = if req.pairs {
            pairs_sql(req)
        } else if req.keys {
            keys_sql(DbKind::Postgres, req)
        } else {
            values_sql(DbKind::Postgres, req)
        };
        let mut tx = self.begin().await?;
        tx.execute("SET TRANSACTION READ ONLY").await?;
        tx.execute("SET LOCAL statement_timeout = '3s'").await?;
        let r = crate::db::run_on(&mut tx, &sql, MAX_HINTS).await;
        let _ = tx.rollback().await;
        Ok(pairs(&r?.rows))
    }

    async fn erd(&self, schemas: &[String]) -> AppResult<Erd> {
        let rels: Vec<(String, String, String, Vec<String>, String, String, Vec<String>)> = sqlx::query_as(
            "select con.conname::text, fn.nspname::text, fc.relname::text,
                    (select array_agg(a.attname::text order by k.ord) from unnest(con.conkey) with ordinality k(n, ord) join pg_attribute a on a.attrelid = con.conrelid and a.attnum = k.n),
                    tn.nspname::text, tc.relname::text,
                    (select array_agg(a.attname::text order by k.ord) from unnest(con.confkey) with ordinality k(n, ord) join pg_attribute a on a.attrelid = con.confrelid and a.attnum = k.n)
             from pg_constraint con
             join pg_class fc on fc.oid = con.conrelid join pg_namespace fn on fn.oid = fc.relnamespace
             join pg_class tc on tc.oid = con.confrelid join pg_namespace tn on tn.oid = tc.relnamespace
             where con.contype = 'f' and (fn.nspname = any($1) or tn.nspname = any($1))
             order by fn.nspname, fc.relname, con.conname",
        )
        .bind(schemas)
        .fetch_all(self)
        .await?;
        let mut wanted: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
            "select n.nspname::text, c.relname::text from pg_class c join pg_namespace n on n.oid = c.relnamespace
             where c.relkind in ('r', 'p') and n.nspname = any($1) and not c.relispartition order by n.nspname, c.relname",
        )
        .bind(schemas)
        .fetch_all(self)
        .await?;
        // Relationships pointing to another schema: include that table too
        for r in &rels {
            for k in [(r.1.clone(), r.2.clone()), (r.4.clone(), r.5.clone())] {
                if !wanted.contains(&k) {
                    wanted.push(k);
                }
            }
        }
        let (ns, ts): (Vec<String>, Vec<String>) = wanted.iter().cloned().unzip();
        let cols: Vec<(String, String, String, String, bool, bool)> = sqlx::query_as(
            "select n.nspname::text, c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod), not a.attnotnull,
                    exists(select 1 from pg_constraint pk where pk.conrelid = c.oid and pk.contype = 'p' and a.attnum = any(pk.conkey))
             from unnest($1::text[], $2::text[]) w(ns, tn)
             join pg_namespace n on n.nspname = w.ns join pg_class c on c.relnamespace = n.oid and c.relname = w.tn
             join pg_attribute a on a.attrelid = c.oid and a.attnum > 0 and not a.attisdropped
             order by n.nspname, c.relname, a.attnum",
        )
        .bind(&ns)
        .bind(&ts)
        .fetch_all(self)
        .await?;
        let mut by: Columns = Default::default();
        for (n, t, name, data_type, nullable, pk) in cols {
            by.entry((n, t)).or_default().push(ErdColumn { name, data_type, pk, nullable });
        }
        Ok(Erd {
            tables: wanted.into_iter().map(|(s, n)| ErdTable { columns: by.remove(&(s.clone(), n.clone())).unwrap_or_default(), schema: s, name: n }).collect(),
            relations: rels.into_iter().map(|r| ErdRelation { name: r.0, from_schema: r.1, from_table: r.2, from_columns: r.3, to_schema: r.4, to_table: r.5, to_columns: r.6 }).collect(),
        })
    }

    async fn table_def(&self, schema: &str, table: &str) -> AppResult<TableDef> {
        let mut t = TableDef { schema: schema.into(), name: table.into(), ..Default::default() };
        let cols: Vec<(String, String, bool, Option<String>, bool, Option<String>)> = sqlx::query_as(&format!(
            "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), NOT a.attnotnull, pg_get_expr(d.adbin, d.adrelid), a.attidentity <> '', col_description(a.attrelid, a.attnum) \
             FROM pg_attribute a LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE a.attrelid = {REL_OID} AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum"
        ))
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        if cols.is_empty() {
            return Err(AppError::Msg(format!("Couldn't find table {schema}.{table}")));
        }
        t.columns = cols.into_iter().map(|(name, data_type, nullable, default, auto_increment, comment)| ColumnDef { orig: Some(name.clone()), name, data_type, nullable, default, auto_increment, comment }).collect();
        t.comment = sqlx::query_scalar::<_, Option<String>>(&format!("SELECT obj_description({REL_OID}, 'pg_class')")).bind(schema).bind(table).fetch_one(self).await?;
        let pk: Option<(String, Vec<String>)> = sqlx::query_as(&format!(
            "SELECT c.conname::text, array(SELECT a.attname::text FROM unnest(c.conkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.n ORDER BY k.o) \
             FROM pg_constraint c WHERE c.conrelid = {REL_OID} AND c.contype = 'p'"
        ))
        .bind(schema)
        .bind(table)
        .fetch_optional(self)
        .await?;
        if let Some((name, cols)) = pk {
            t.primary_key_name = Some(name);
            t.primary_key = cols;
        }
        // Indexes (excluding PK). Ones used by UNIQUE constraints are constraint. Expression · predicate indexes only keep the definition text
        let idx: Vec<(String, bool, bool, Vec<String>, bool, String)> = sqlx::query_as(&format!(
            "SELECT i.relname::text, ix.indisunique, EXISTS (SELECT 1 FROM pg_constraint c WHERE c.conindid = ix.indexrelid AND c.contype = 'u'), \
                    array(SELECT a.attname::text FROM unnest(ix.indkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = ix.indrelid AND a.attnum = k.n ORDER BY k.o), \
                    ix.indexprs IS NOT NULL OR ix.indpred IS NOT NULL, pg_get_indexdef(ix.indexrelid) \
             FROM pg_index ix JOIN pg_class i ON i.oid = ix.indexrelid WHERE ix.indrelid = {REL_OID} AND NOT ix.indisprimary ORDER BY i.relname"
        ))
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        t.indexes = idx
            .into_iter()
            .map(|(name, unique, constraint, columns, complex, def)| IndexDef { orig: Some(name.clone()), name, columns: if complex { vec![] } else { columns }, unique, constraint, definition: complex.then_some(def) })
            .collect();
        let fks: Vec<(String, Vec<String>, String, String, Vec<String>, String, String)> = sqlx::query_as(&format!(
            "SELECT c.conname::text, \
               array(SELECT a.attname::text FROM unnest(c.conkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.n ORDER BY k.o), \
               rn.nspname::text, rc.relname::text, \
               array(SELECT a.attname::text FROM unnest(c.confkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.confrelid AND a.attnum = k.n ORDER BY k.o), \
               c.confdeltype::text, c.confupdtype::text \
             FROM pg_constraint c JOIN pg_class rc ON rc.oid = c.confrelid JOIN pg_namespace rn ON rn.oid = rc.relnamespace \
             WHERE c.conrelid = {REL_OID} AND c.contype = 'f' ORDER BY c.conname"
        ))
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        t.foreign_keys = fks
            .into_iter()
            .map(|(name, columns, ref_schema, ref_table, ref_columns, del, upd)| ForeignKeyDef { orig: Some(name.clone()), name, columns, ref_schema, ref_table, ref_columns, on_delete: action(&del), on_update: action(&upd) })
            .collect();
        Ok(t)
    }

    async fn table_shape(&self, schema: &str, table: &str) -> AppResult<TableShape> {
        let rel = crate::fake::qualified(DbKind::Postgres, schema, table);
        let cols: Vec<(String, String, String, bool, bool, Vec<String>)> = sqlx::query_as(
            "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), (CASE WHEN t.typtype = 'd' THEN bt.typname ELSE t.typname END)::text, \
                    NOT a.attnotnull, \
                    coalesce(a.attidentity <> '' OR a.attgenerated <> '' OR pg_get_expr(d.adbin, d.adrelid) LIKE 'nextval(%', false), \
                    coalesce((SELECT array_agg(e.enumlabel::text ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid = a.atttypid), '{}') \
             FROM pg_attribute a JOIN pg_type t ON t.oid = a.atttypid LEFT JOIN pg_type bt ON bt.oid = t.typbasetype \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE a.attrelid = $1::regclass AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
        )
        .bind(&rel)
        .fetch_all(self)
        .await?;
        let keys: Vec<(String, bool, i16)> = sqlx::query_as(
            "SELECT a.attname::text, i.indisprimary, i.indnatts FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = $1::regclass AND i.indisunique AND i.indpred IS NULL",
        )
        .bind(&rel)
        .fetch_all(self)
        .await?;
        let checks: Vec<(String, String)> = sqlx::query_as(
            "SELECT a.attname::text, pg_get_constraintdef(c.oid) FROM pg_constraint c JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = c.conkey[1] \
             WHERE c.conrelid = $1::regclass AND c.contype = 'c' AND array_length(c.conkey, 1) = 1",
        )
        .bind(&rel)
        .fetch_all(self)
        .await?;
        let fks: Vec<(String, Vec<String>, Vec<String>)> = sqlx::query_as(
            "SELECT c.confrelid::regclass::text, \
               array(SELECT a.attname::text FROM unnest(c.conkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.n ORDER BY k.o), \
               array(SELECT a.attname::text FROM unnest(c.confkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.confrelid AND a.attnum = k.n ORDER BY k.o) \
             FROM pg_constraint c WHERE c.conrelid = $1::regclass AND c.contype = 'f'",
        )
        .bind(&rel)
        .fetch_all(self)
        .await?;
        let cols = cols
            .into_iter()
            .map(|(name, data_type, base, nullable, auto, enums)| {
                let (len, scale) = type_args(&data_type);
                let pk = keys.iter().any(|(k, p, _)| *k == name && *p);
                let unique = keys.iter().any(|(k, p, n)| *k == name && !*p && *n == 1) || (pk && keys.iter().filter(|(_, p, _)| *p).count() == 1);
                let mut choices = enums;
                if choices.is_empty() {
                    choices = checks.iter().filter(|(c, _)| *c == name).flat_map(|(_, def)| if def.contains("ANY") || def.contains(" IN ") { quoted_literals(def) } else { vec![] }).collect();
                }
                Col { name, data_type, base, len, scale, nullable, auto, pk, unique, choices, unsigned: false }
            })
            .collect();
        Ok(TableShape { cols, fks: fks.into_iter().map(|(ref_table, cols, ref_cols)| Fk { cols, ref_table, ref_cols }).collect() })
    }

    // ───── Connection: db.rs · app.rs ─────

    async fn server_version(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>("select version()").fetch_one(self).await?)
    }

    async fn close(&self) {
        PgPool::close(self).await
    }

    async fn ping(&self) {
        let _ = sqlx::query("select 1").execute(self).await;
    }

    /// With no SSL setting, it falls back to plain text if the server lacks TLS (except this machine · tunnels)
    async fn plaintext_note(&self, cfg: &ConnectionConfig) -> Option<String> {
        if cfg.active_ssl().is_some() || crate::db::is_local(cfg) {
            return None;
        }
        let ssl: Option<bool> = sqlx::query_scalar("select ssl from pg_stat_ssl where pid = pg_backend_pid()").fetch_optional(self).await.ok().flatten();
        (ssl == Some(false)).then(|| "⚠ Not encrypted: the server didn't offer TLS, so the password and data travel in plain text. Enable SSL in the data source to require it.".to_string())
    }

    // ───── Running: db.rs · session.rs · app.rs ─────

    async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        crate::db::run_pg_on(&mut *self.acquire().await?, sql, limit).await
    }

    async fn export(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        let mut conn = self.acquire().await?;
        crate::db::export_pg_on(&mut conn, sql, out).await
    }

    async fn run_read_only(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        let mut tx = begin_read_only(self).await?;
        let r = crate::db::run_on(&mut tx, sql, limit).await;
        let _ = tx.rollback().await;
        r
    }

    async fn export_read_only(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        let mut tx = begin_read_only(self).await?;
        let r = crate::db::export_pg_on(&mut tx, sql, out).await;
        let _ = tx.rollback().await;
        r
    }

    async fn open_session(&self) -> AppResult<Session> {
        Ok(Session::Pg(PgSession::open(self).await?))
    }

    /// Cancel the backend's running query from another connection
    async fn cancel(&self, pid: i64) -> AppResult<bool> {
        Ok(sqlx::query_scalar("select pg_cancel_backend($1)").bind(pid as i32).fetch_one(self).await?)
    }

    /// One transaction: if any statement fails, everything rolls back
    async fn execute_batch(&self, statements: &[String], exact_one: bool, prepared: bool, snaps: &[Option<&str>]) -> AppResult<(u64, Snapshots)> {
        let mut tx = self.begin().await?;
        let mut affected = 0;
        let mut shots = Vec::new();
        for (i, st) in statements.iter().enumerate() {
            let shot = match snaps.get(i).copied().flatten() {
                Some(q) => Some(pg_snapshot(&mut tx, q).await),
                None => None,
            };
            // Cancelled · timed out while locking the rows: nothing is saved (dropping tx rolls back)
            if let Some(Err(e)) = &shot
                && crate::audit::stops_change(e)
            {
                return Err(crate::audit::stopped(e, false));
            }
            let r = if prepared { sqlx::query(st).execute(&mut *tx).await } else { tx.execute(st.as_str()).await };
            let n = r.map_err(|e| AppError::Msg(format!("{e}\n→ {st}")))?.rows_affected();
            check_exact_one(st, n, exact_one)?;
            shots.push(shot.map(|s| (s, n)));
            affected += n;
        }
        tx.commit().await?;
        Ok((affected, shots))
    }
}

/// The transaction MCP reads and MCP CSV exports run in (the caller rolls it back): READ ONLY, 60 seconds (so queries sent by others
/// do not hold the server for long), string parsing that matches guard (even if the server or role sets standard_conforming_strings = off)
async fn begin_read_only(pool: &PgPool) -> AppResult<sqlx::Transaction<'static, sqlx::Postgres>> {
    let mut tx = pool.begin().await?;
    tx.execute("SET TRANSACTION READ ONLY").await?;
    tx.execute("SET LOCAL statement_timeout = '60s'").await?;
    tx.execute("SET LOCAL standard_conforming_strings = on").await?;
    Ok(tx)
}

/// Connect to `host`:`port`: the server, or the SSH tunnel's local port (db::connect_via)
pub(crate) async fn connect(cfg: &ConnectionConfig, password: Option<&str>, host: &str, port: u16) -> AppResult<PgPool> {
    let mut opts = PgConnectOptions::new()
        .host(host)
        .port(port)
        .username(&cfg.user)
        .application_name("qurry");
    // Like DataGrip, an empty value connects to the postgres DB (the PG default, a DB named after the user, often does not exist)
    let database = if cfg.database.trim().is_empty() { "postgres" } else { cfg.database.trim() };
    opts = opts.database(database);
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        opts = opts.password(p);
    }
    if cfg.read_only {
        // Do not leave the parsing of \ in strings to server settings (match the rules guard reads with)
        opts = opts.options([("default_transaction_read_only", "on"), ("standard_conforming_strings", "on")]);
    }
    // Without SSL settings a remote server must still set up TLS (certificate not checked); plain text only for this computer,
    // an SSH tunnel, or when the data source allows it (Prefer would quietly fall back to plain text if TLS were cut)
    if cfg.active_ssl().is_none() && !crate::db::plaintext_allowed(cfg, host) {
        opts = opts.ssl_mode(PgSslMode::Require);
    }
    // SSL from the SSH/SSL tab
    if let Some(ssl) = cfg.active_ssl() {
        opts = opts.ssl_mode(match ssl.mode() {
            "verify-full" => PgSslMode::VerifyFull,
            "verify-ca" => PgSslMode::VerifyCa,
            "require" => PgSslMode::Require,
            // Do not pass unknown values on as unverified require
            m => return Err(AppError::Msg(format!("Unknown SSL mode '{m}' (use require, verify-ca or verify-full)"))),
        });
        let home = |p: &str| p.strip_prefix("~/").map(|r| format!("{}/{r}", std::env::var("HOME").unwrap_or_default())).unwrap_or_else(|| p.to_string());
        if !ssl.ca_file.trim().is_empty() {
            opts = opts.ssl_root_cert(home(ssl.ca_file.trim()));
        }
        if !ssl.cert_file.trim().is_empty() {
            opts = opts.ssl_client_cert(home(ssl.cert_file.trim()));
        }
        if !ssl.key_file.trim().is_empty() {
            opts = opts.ssl_client_key(home(ssl.key_file.trim()));
        }
    }
    const LEVELS: [&str; 4] = ["read uncommitted", "read committed", "repeatable read", "serializable"];
    if let Some(iso) = cfg.options.isolation.as_deref().filter(|i| LEVELS.contains(i)) {
        opts = opts.options([("default_transaction_isolation", iso)]);
    }
    // sqlx sends TimeZone=UTC as a startup parameter, so change it with SET right after connecting
    let tz = cfg.options.time_zone.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(|t| t.replace('\'', "''"));
    let script = cfg.options.startup_script.trim().to_string();
    PgPoolOptions::new()
        // Each console/table tab uses one dedicated connection. Connections open only when needed, so just keep the cap generous
        .max_connections(crate::db::POOL_MAX)
        .idle_timeout(crate::db::POOL_IDLE)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(move |conn, _meta| {
            let script = script.clone();
            let tz = tz.clone();
            Box::pin(async move {
                if let Some(tz) = tz {
                    conn.execute(format!("set time zone '{tz}'").as_str()).await?;
                }
                if !script.is_empty() {
                    conn.execute(sqlx::raw_sql(&script)).await?;
                }
                Ok(())
            })
        })
        .connect_with(opts)
        .await
        .map_err(|e| match &e {
            // Required TLS that the server can't set up: say what to do instead of a bare TLS error
            sqlx::Error::Tls(_) if cfg.active_ssl().is_none() => AppError::Msg(format!("{}\n({e})", qurry_shared::errors::needs_tls_pg())),
            _ => e.into(),
        })
}
