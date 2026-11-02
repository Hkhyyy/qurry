//! MySQL: the metadata queries live in crate::mysql; the table tools' queries are here
use super::Backend;
use crate::audit;
use crate::migrate::{self, Model, norm, qualify_refs};
use crate::transfer::Tx;
use crate::ddl::{ColumnDef, ForeignKeyDef, IndexDef, TableDef, action, lit};
use crate::erd::{Columns, Erd, ErdColumn, ErdRelation, ErdTable};
use crate::error::{AppError, AppResult};
use crate::fake::{Col, Fk, TableShape, qualified, quoted_literals, type_args};
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, FtsHit, FtsRequest, IndexEntry, SchemaInfo, SchemaObjects, TableDetail};
use crate::store::DbKind;
use sqlx::MySqlPool;

impl Backend for MySqlPool {
    fn label(&self) -> String {
        "MySQL".into()
    }

    // ───── Metadata: meta.rs · hints.rs · erd.rs · ddl.rs · fake.rs ─────

    async fn list_databases(&self) -> AppResult<Vec<String>> {
        crate::mysql::list_databases(self).await
    }

    async fn list_schemas(&self) -> AppResult<Vec<SchemaInfo>> {
        crate::mysql::list_schemas(self).await
    }

    async fn list_objects(&self, schema: &str) -> AppResult<SchemaObjects> {
        crate::mysql::list_objects(self, schema).await
    }

    async fn table_detail(&self, schema: &str, table: &str) -> AppResult<TableDetail> {
        crate::mysql::table_detail(self, schema, table).await
    }

    async fn object_ddl(&self, schema: &str, name: &str, kind: &str) -> AppResult<String> {
        crate::mysql::object_ddl(self, schema, name, kind).await
    }

    async fn completion_meta(&self) -> AppResult<Vec<CompletionTable>> {
        crate::mysql::completion_meta(self).await
    }

    async fn object_index(&self) -> AppResult<Vec<IndexEntry>> {
        crate::mysql::object_index(self).await
    }

    async fn full_text_search(&self, r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
        crate::mysql::full_text_search(self, r).await
    }

    async fn column_hints(&self, req: &HintReq) -> AppResult<Vec<Hint>> {
        use crate::hints::{KEY_SAMPLE, MAX_HINTS, count_keys, keys_sql, pairs, values_sql};
        use sqlx::Executor;
        // MySQL has JSON_CONTAINS separately, so there are no @> conditions
        if req.pairs {
            return Ok(vec![]);
        }
        let mut conn = self.acquire().await?;
        conn.execute("START TRANSACTION READ ONLY").await?;
        let r = if req.keys {
            crate::mysql::run_on(&mut conn, &keys_sql(DbKind::Mysql, req), KEY_SAMPLE).await.map(|r| count_keys(&r.rows))
        } else {
            crate::mysql::run_on(&mut conn, &values_sql(DbKind::Mysql, req), MAX_HINTS).await.map(|r| pairs(&r.rows))
        };
        let _ = conn.execute("ROLLBACK").await;
        r
    }

    async fn erd(&self, schemas: &[String]) -> AppResult<Erd> {
        if schemas.is_empty() {
            return Ok(Erd::default());
        }
        let marks = vec!["?"; schemas.len()].join(", ");
        let rel_sql = format!(
            "SELECT CAST(CONSTRAINT_NAME AS CHAR), CAST(TABLE_SCHEMA AS CHAR), CAST(TABLE_NAME AS CHAR), CAST(COLUMN_NAME AS CHAR),
                    CAST(REFERENCED_TABLE_SCHEMA AS CHAR), CAST(REFERENCED_TABLE_NAME AS CHAR), CAST(REFERENCED_COLUMN_NAME AS CHAR)
             FROM information_schema.KEY_COLUMN_USAGE WHERE REFERENCED_TABLE_NAME IS NOT NULL AND (TABLE_SCHEMA IN ({marks}) OR REFERENCED_TABLE_SCHEMA IN ({marks}))
             ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION"
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(&rel_sql);
        for s in schemas.iter().chain(schemas.iter()) {
            q = q.bind(s.clone());
        }
        let rows = q.fetch_all(self).await?;
        let mut relations: Vec<ErdRelation> = vec![];
        for (name, fs, ft, fc, ts, tt, tc) in rows {
            match relations.last_mut() {
                Some(r) if r.name == name && r.from_schema == fs && r.from_table == ft => {
                    r.from_columns.push(fc);
                    r.to_columns.push(tc);
                }
                _ => relations.push(ErdRelation { name, from_schema: fs, from_table: ft, from_columns: vec![fc], to_schema: ts, to_table: tt, to_columns: vec![tc] }),
            }
        }
        let t_sql = format!("SELECT CAST(TABLE_SCHEMA AS CHAR), CAST(TABLE_NAME AS CHAR) FROM information_schema.TABLES WHERE TABLE_TYPE = 'BASE TABLE' AND TABLE_SCHEMA IN ({marks}) ORDER BY TABLE_SCHEMA, TABLE_NAME");
        let mut q = sqlx::query_as::<_, (String, String)>(&t_sql);
        for s in schemas {
            q = q.bind(s.clone());
        }
        let mut wanted = q.fetch_all(self).await?;
        for r in &relations {
            for k in [(r.from_schema.clone(), r.from_table.clone()), (r.to_schema.clone(), r.to_table.clone())] {
                if !wanted.contains(&k) {
                    wanted.push(k);
                }
            }
        }
        let all: Vec<String> = wanted.iter().map(|w| w.0.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let c_sql = format!(
            "SELECT CAST(TABLE_SCHEMA AS CHAR), CAST(TABLE_NAME AS CHAR), CAST(COLUMN_NAME AS CHAR), CAST(COLUMN_TYPE AS CHAR), CAST(IS_NULLABLE = 'YES' AS SIGNED), CAST(COLUMN_KEY = 'PRI' AS SIGNED) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA IN ({}) ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION",
            vec!["?"; all.len()].join(", ")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String, i64, i64)>(&c_sql);
        for s in &all {
            q = q.bind(s.clone());
        }
        let mut by: Columns = Default::default();
        for (s, t, name, data_type, nullable, pk) in q.fetch_all(self).await? {
            by.entry((s, t)).or_default().push(ErdColumn { name, data_type, pk: pk != 0, nullable: nullable != 0 });
        }
        Ok(Erd { tables: wanted.into_iter().map(|(s, n)| ErdTable { columns: by.remove(&(s.clone(), n.clone())).unwrap_or_default(), schema: s, name: n }).collect(), relations })
    }

    async fn table_def(&self, schema: &str, table: &str) -> AppResult<TableDef> {
        let mut t = TableDef { schema: schema.into(), name: table.into(), ..Default::default() };
        let cols: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT CAST(COLUMN_NAME AS CHAR), CAST(COLUMN_TYPE AS CHAR), CAST(IS_NULLABLE AS CHAR), CAST(COLUMN_DEFAULT AS CHAR), CAST(EXTRA AS CHAR), CAST(COLUMN_COMMENT AS CHAR) \
             FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        if cols.is_empty() {
            return Err(AppError::Msg(format!("Couldn't find table {schema}.{table}")));
        }
        t.columns = cols
            .into_iter()
            .map(|(name, data_type, nullable, default, extra, comment)| {
                let extra = extra.to_lowercase();
                // Defaults in information_schema are unquoted: wrap as text unless it is an expression
                let numeric = ["int", "decimal", "float", "double", "bit", "year"].iter().any(|n| data_type.to_lowercase().starts_with(n));
                let expr = extra.contains("default_generated") || default.as_deref().is_some_and(|d| d.to_uppercase().starts_with("CURRENT_TIMESTAMP"));
                let default = default.map(|d| if numeric || expr { d } else { lit(DbKind::Mysql, &d) });
                ColumnDef { orig: Some(name.clone()), name, data_type, nullable: nullable == "YES", default, auto_increment: extra.contains("auto_increment"), comment: Some(comment).filter(|c| !c.is_empty()) }
            })
            .collect();
        t.comment = sqlx::query_scalar::<_, Option<String>>("SELECT CAST(TABLE_COMMENT AS CHAR) FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?")
            .bind(schema)
            .bind(table)
            .fetch_one(self)
            .await?
            .filter(|c| !c.is_empty());
        let stats: Vec<(String, i64, Option<String>, i64)> = sqlx::query_as(
            "SELECT CAST(INDEX_NAME AS CHAR), CAST(NON_UNIQUE AS SIGNED), CAST(COLUMN_NAME AS CHAR), CAST(SEQ_IN_INDEX AS SIGNED) FROM information_schema.STATISTICS \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY INDEX_NAME, SEQ_IN_INDEX",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        for (name, non_unique, col, _) in stats {
            if name == "PRIMARY" {
                t.primary_key.extend(col);
                t.primary_key_name = Some(name);
                continue;
            }
            match t.indexes.iter_mut().find(|i| i.name == name) {
                Some(i) => match col {
                    Some(c) => i.columns.push(c),
                    None => i.definition = Some(format!("{name} (expression)")),
                },
                None => t.indexes.push(IndexDef { orig: Some(name.clone()), definition: col.is_none().then(|| format!("{name} (expression)")), columns: col.into_iter().collect(), name, unique: non_unique == 0, constraint: false }),
            }
        }
        let fks: Vec<(String, String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT CAST(k.CONSTRAINT_NAME AS CHAR), CAST(k.COLUMN_NAME AS CHAR), CAST(k.REFERENCED_TABLE_SCHEMA AS CHAR), CAST(k.REFERENCED_TABLE_NAME AS CHAR), \
                    CAST(k.REFERENCED_COLUMN_NAME AS CHAR), CAST(r.DELETE_RULE AS CHAR), CAST(r.UPDATE_RULE AS CHAR) \
             FROM information_schema.KEY_COLUMN_USAGE k JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
               ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME AND r.TABLE_NAME = k.TABLE_NAME \
             WHERE k.TABLE_SCHEMA = ? AND k.TABLE_NAME = ? AND k.REFERENCED_TABLE_NAME IS NOT NULL ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        for (name, col, rs, rt, rc, del, upd) in fks {
            match t.foreign_keys.iter_mut().find(|f| f.name == name) {
                Some(f) => {
                    f.columns.push(col);
                    f.ref_columns.push(rc);
                }
                None => t.foreign_keys.push(ForeignKeyDef { orig: Some(name.clone()), name, columns: vec![col], ref_schema: rs, ref_table: rt, ref_columns: vec![rc], on_delete: action(&del), on_update: action(&upd) }),
            }
        }
        // Indexes used by FKs (made by MySQL under the FK name) are handled with the FK
        let fk_names: Vec<String> = t.foreign_keys.iter().map(|f| f.name.clone()).collect();
        t.indexes.retain(|i| !fk_names.contains(&i.name));
        Ok(t)
    }

    async fn table_shape(&self, schema: &str, table: &str) -> AppResult<TableShape> {
        let cols: Vec<(String, String, String, i64, i64, String)> = sqlx::query_as(
            "SELECT CAST(COLUMN_NAME AS CHAR), CAST(COLUMN_TYPE AS CHAR), CAST(DATA_TYPE AS CHAR), CAST(IS_NULLABLE = 'YES' AS SIGNED), \
                    CAST(EXTRA LIKE '%auto_increment%' OR EXTRA LIKE '%GENERATED%' AS SIGNED), CAST(COLUMN_KEY AS CHAR) \
             FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = COALESCE(NULLIF(?, ''), DATABASE()) AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        let uniq: Vec<(String,)> = sqlx::query_as(
            "SELECT CAST(MAX(COLUMN_NAME) AS CHAR) FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = COALESCE(NULLIF(?, ''), DATABASE()) AND TABLE_NAME = ? \
             AND NON_UNIQUE = 0 AND INDEX_NAME <> 'PRIMARY' GROUP BY INDEX_NAME HAVING COUNT(*) = 1",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        let fk_rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT CAST(CONSTRAINT_NAME AS CHAR), CAST(COLUMN_NAME AS CHAR), CAST(REFERENCED_TABLE_SCHEMA AS CHAR), CAST(REFERENCED_TABLE_NAME AS CHAR), CAST(REFERENCED_COLUMN_NAME AS CHAR) \
             FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = COALESCE(NULLIF(?, ''), DATABASE()) AND TABLE_NAME = ? AND REFERENCED_TABLE_NAME IS NOT NULL \
             ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(self)
        .await?;
        let mut fks: Vec<(String, Fk)> = vec![];
        for (cname, col, rs, rt, rc) in fk_rows {
            match fks.iter_mut().find(|(n, _)| *n == cname) {
                Some((_, f)) => {
                    f.cols.push(col);
                    f.ref_cols.push(rc);
                }
                None => fks.push((cname, Fk { cols: vec![col], ref_table: qualified(DbKind::Mysql, &rs, &rt), ref_cols: vec![rc] })),
            }
        }
        let pks = cols.iter().filter(|c| c.5 == "PRI").count();
        let cols = cols
            .into_iter()
            .map(|(name, ctype, base, nullable, auto, key)| {
                let lower = ctype.to_lowercase();
                let (len, scale) = type_args(&lower);
                let base = base.to_lowercase();
                let choices = if base == "enum" || base == "set" { quoted_literals(&ctype) } else { vec![] };
                let pk = key == "PRI";
                Col { unique: uniq.iter().any(|(u,)| *u == name) || (pk && pks == 1), name, data_type: ctype, base, len, scale, nullable: nullable != 0, auto: auto != 0, pk, choices, unsigned: lower.contains("unsigned") }
            })
            .collect();
        Ok(TableShape { cols, fks: fks.into_iter().map(|x| x.1).collect() })
    }

    // ───── Jobs: migrate.rs · transfer.rs · audit.rs ─────

    async fn schema_model(&self, db: &str) -> AppResult<Model> {
        use sqlx::Row;
        let objs: Vec<(String, String)> = sqlx::query_as("select cast(table_name as char), cast(table_type as char) from information_schema.tables where table_schema = ? order by 1")
            .bind(db)
            .fetch_all(self)
            .await?;
        let mut m = Model::default();
        let q = |s: &str| crate::sql::quote_ident(DbKind::Mysql, s);
        for (t, ty) in objs {
            if ty == "VIEW" {
                let def: Option<String> = sqlx::query_scalar("select cast(view_definition as char) from information_schema.views where table_schema = ? and table_name = ?").bind(db).bind(&t).fetch_one(self).await?;
                m.views.insert(t, norm(def.unwrap_or_default().trim(), db, DbKind::Mysql));
                continue;
            }
            let row = sqlx::query(&format!("SHOW CREATE TABLE {}.{}", q(db), q(&t))).fetch_one(self).await?;
            let ddl: String = row.try_get::<String, _>(1).or_else(|_| row.try_get::<Vec<u8>, _>(1).map(|b| String::from_utf8_lossy(&b).into_owned()))?;
            let mut tm = migrate::TableModel::default();
            // CREATE TABLE `t` ( … ) ENGINE=… : lines inside the parentheses
            for l in ddl.lines().skip(1) {
                let l = l.trim().trim_end_matches(',');
                if l.starts_with(')') {
                    break;
                }
                let name_of = |s: &str| s.split('`').nth(1).unwrap_or("").to_string();
                if l.starts_with('`') {
                    tm.columns.push((name_of(l), migrate::Col { line: l.to_string(), ..Default::default() }));
                } else if l.starts_with("PRIMARY KEY") {
                    tm.keys.insert("PRIMARY".into(), l.to_string());
                } else if l.starts_with("CONSTRAINT") && l.contains("FOREIGN KEY") {
                    tm.fks.insert(name_of(l), qualify_refs(l, db).replace(&format!("`{db}`."), ""));
                } else if !l.is_empty() {
                    tm.keys.insert(name_of(l), l.to_string());
                }
            }
            m.tables.insert(t, tm);
        }
        Ok(m)
    }

    /// DDL commits implicitly (only data statements are rolled back together)
    async fn apply_migration(&self, stmts: &[&str], fail: impl Fn(String) -> AppError) -> AppResult<()> {
        super::run_in_tx(self, stmts, fail).await
    }

    async fn import_tx(&self) -> AppResult<Tx> {
        Ok(Tx::My(self.begin().await?))
    }

    async fn table_columns(&self, table: &str) -> AppResult<Vec<audit::Col>> {
        let parts = audit::my_name_parts(table);
        let (schema, name) = match parts.as_slice() {
            [s, n] => (Some(s.clone()), n.clone()),
            _ => (None, parts.last().cloned().unwrap_or_default()),
        };
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT CAST(COLUMN_NAME AS CHAR), CAST(DATA_TYPE AS CHAR), CAST(COLUMN_KEY = 'PRI' AS SIGNED), CAST(EXTRA LIKE '%GENERATED%' AS SIGNED) \
             FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(name)
        .fetch_all(self)
        .await?;
        let bin = |t: &str| t.contains("blob") || t.contains("binary") || t == "bit" || ["geometry", "point", "linestring", "polygon", "multipoint", "multilinestring", "multipolygon", "geometrycollection"].contains(&t);
        Ok(rows.into_iter().map(|(name, t, pk, generated)| audit::Col { name, pk: pk != 0, generated: generated != 0, binary: bin(&t.to_lowercase()) }).collect())
    }
}
