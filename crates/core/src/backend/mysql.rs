//! MySQL: the metadata queries, statements and the console session live in crate::mysql; the table tools' queries,
//! connecting (TLS rules), MCP reads in a READ ONLY transaction and the jobs (schema compare, migrations, import, undo columns) are here
use super::{Backend, Snapshots};
use crate::audit;
use crate::db::QueryResult;
use crate::ddl::{ColumnDef, ForeignKeyDef, IndexDef, TableDef, action, lit};
use crate::erd::{Columns, Erd, ErdColumn, ErdRelation, ErdTable};
use crate::error::{AppError, AppResult};
use crate::fake::{Col, Fk, TableShape, qualified, quoted_literals, type_args};
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, FtsHit, FtsRequest, IndexEntry, SchemaInfo, SchemaObjects, TableDetail, TableStructure};
use crate::migrate::{self, Model, norm, qualify_refs};
use crate::session::Session;
use crate::store::{ConnectionConfig, DbKind};
use crate::transfer::{Exporter, Tx};
use qurry_shared::errors::{DROPPED_TLS, NEEDS_TLS};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode};
use sqlx::{Executor, MySqlPool};
use std::io::ErrorKind;
use std::time::Duration;

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

    async fn table_structure(&self, schema: &str, table: &str) -> AppResult<TableStructure> {
        crate::mysql::table_structure(self, schema, table).await
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

    /// Foreign keys: MySQL 5.7 can't drop a referenced table even with the table referencing it in the same statement
    async fn drop_deps(&self, schemas: &[String]) -> AppResult<Vec<[String; 4]>> {
        let sql = format!(
            "SELECT DISTINCT CAST(TABLE_SCHEMA AS CHAR), CAST(TABLE_NAME AS CHAR), CAST(REFERENCED_TABLE_SCHEMA AS CHAR), CAST(REFERENCED_TABLE_NAME AS CHAR) \
             FROM information_schema.KEY_COLUMN_USAGE WHERE REFERENCED_TABLE_NAME IS NOT NULL AND TABLE_SCHEMA IN ({})",
            vec!["?"; schemas.len()].join(", ")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String)>(&sql);
        for s in schemas {
            q = q.bind(s.clone());
        }
        Ok(q.fetch_all(self).await?.into_iter().map(|(a, b, c, d)| [a, b, c, d]).collect())
    }

    // ───── Connection: db.rs · app.rs ─────

    async fn server_version(&self) -> AppResult<String> {
        let v: String = sqlx::query_scalar("select cast(version() as char)").fetch_one(self).await?;
        Ok(if v.to_lowercase().contains("mariadb") { format!("MariaDB {v}") } else { format!("MySQL {v}") })
    }

    async fn close(&self) {
        MySqlPool::close(self).await
    }

    async fn ping(&self) {
        let _ = sqlx::query("select 1").execute(self).await;
    }

    /// A source without SSL set connected in plaintext (reconnected in plaintext because TLS failed — also what happens when someone deliberately strips TLS)
    async fn plaintext_note(&self, cfg: &ConnectionConfig) -> Option<String> {
        if cfg.active_ssl().is_some() || crate::db::is_local(cfg) {
            return None;
        }
        let row: Option<(String, String)> = sqlx::query_as("SHOW SESSION STATUS LIKE 'Ssl_cipher'").fetch_optional(self).await.ok()?;
        row.filter(|(_, c)| c.is_empty()).map(|_| "⚠ Not encrypted: TLS didn't work with this server, so Qurry connected without it (the password and data travel in plain text). Enable SSL in the data source to require it.".to_string())
    }

    // ───── Running: db.rs · session.rs · app.rs ─────

    async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        let mut conn = self.acquire().await?;
        crate::mysql::run_on(&mut conn, sql, limit).await
    }

    async fn export(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        let mut conn = self.acquire().await?;
        crate::mysql::export_on(&mut conn, sql, out, false).await
    }

    /// The prepared protocol (the server accepts only one statement), in a READ ONLY transaction with a time limit
    async fn run_read_only(&self, sql: &str, limit: usize) -> AppResult<QueryResult> {
        let mut conn = self.acquire().await?;
        let r = match start_read_only(&mut conn).await {
            Ok(()) => crate::mysql::run_read_only_on(&mut conn, sql, limit).await,
            Err(e) => Err(e),
        };
        end_read_only(&mut conn).await;
        r
    }

    async fn export_read_only(&self, sql: &str, out: &mut Exporter) -> AppResult<()> {
        let mut conn = self.acquire().await?;
        let r = match start_read_only(&mut conn).await {
            Ok(()) => crate::mysql::export_on(&mut conn, sql, out, true).await,
            Err(e) => Err(e),
        };
        end_read_only(&mut conn).await;
        r
    }

    async fn open_session(&self) -> AppResult<Session> {
        Ok(Session::My(crate::mysql::MySession::open(self).await?))
    }

    async fn cancel(&self, id: i64) -> AppResult<bool> {
        sqlx::query(&format!("KILL QUERY {id}")).execute(self).await?;
        Ok(true)
    }

    /// One transaction. prepared: the prepared-statement protocol, which takes exactly one statement (MCP)
    async fn execute_batch(&self, statements: &[String], exact_one: bool, prepared: bool, snaps: &[Option<&str>]) -> AppResult<(u64, Snapshots)> {
        let mut tx = self.begin().await?;
        let mut affected = 0;
        let mut shots = Vec::new();
        for (i, st) in statements.iter().enumerate() {
            let shot = match snaps.get(i).copied().flatten() {
                Some(q) => Some(crate::mysql::snapshot_on(&mut tx, q).await),
                None => None,
            };
            // Cancelled · lock wait timeout · deadlock (the server may have rolled back the statements before): nothing is saved (dropping tx rolls back)
            if let Some(Err(e)) = &shot
                && crate::audit::stops_change(e)
            {
                return Err(crate::audit::stopped(e, false));
            }
            let r = if prepared { sqlx::query(st).execute(&mut *tx).await } else { tx.execute(sqlx::raw_sql(st)).await };
            let n = r.map_err(|e| AppError::Msg(format!("{e}\n→ {st}")))?.rows_affected();
            crate::session::check_exact_one(st, n, exact_one)?;
            shots.push(shot.map(|s| (s, n)));
            affected += n;
        }
        tx.commit().await?;
        Ok((affected, shots))
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

/// MCP reads and MCP CSV exports: a READ ONLY transaction with a 60-second limit. end_read_only always follows, even when this fails
async fn start_read_only(conn: &mut sqlx::MySqlConnection) -> AppResult<()> {
    conn.execute("SET SESSION max_execution_time = 60000").await?;
    conn.execute("START TRANSACTION READ ONLY").await?;
    Ok(())
}

/// Roll back and reset the time limit, so it does not stay on the pooled connection
async fn end_read_only(conn: &mut sqlx::MySqlConnection) {
    let _ = conn.execute("ROLLBACK").await;
    let _ = conn.execute("SET SESSION max_execution_time = 0").await;
}

/// Connect to `host`:`port`: the server, or the SSH tunnel's local port (db::connect_via)
pub(crate) async fn connect(cfg: &ConnectionConfig, password: Option<&str>, host: &str, port: u16) -> AppResult<MySqlPool> {
    let mut opts = MySqlConnectOptions::new().host(host).port(port).username(&cfg.user).charset("utf8mb4");
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        opts = opts.password(p);
    }
    if !cfg.database.trim().is_empty() {
        opts = opts.database(cfg.database.trim());
    }
    if let Some(ssl) = cfg.active_ssl() {
        opts = opts.ssl_mode(match ssl.mode() {
            "verify-full" => MySqlSslMode::VerifyIdentity,
            "verify-ca" => MySqlSslMode::VerifyCa,
            "require" => MySqlSslMode::Required,
            m => return Err(AppError::Msg(format!("Unknown SSL mode '{m}' (use require, verify-ca or verify-full)"))),
        });
        let home = |p: &str| p.strip_prefix("~/").map(|r| format!("{}/{r}", std::env::var("HOME").unwrap_or_default())).unwrap_or_else(|| p.to_string());
        if !ssl.ca_file.trim().is_empty() {
            opts = opts.ssl_ca(home(ssl.ca_file.trim()));
        }
        if !ssl.cert_file.trim().is_empty() {
            opts = opts.ssl_client_cert(home(ssl.cert_file.trim()));
        }
        if !ssl.key_file.trim().is_empty() {
            opts = opts.ssl_client_key(home(ssl.key_file.trim()));
        }
    }
    // Session settings to run right after connecting (Options tab)
    let mut init: Vec<String> = Vec::new();
    const LEVELS: [&str; 4] = ["read uncommitted", "read committed", "repeatable read", "serializable"];
    if let Some(iso) = cfg.options.isolation.as_deref().filter(|i| LEVELS.contains(i)) {
        init.push(format!("SET SESSION TRANSACTION ISOLATION LEVEL {}", iso.to_uppercase()));
    }
    if cfg.read_only {
        init.push("SET SESSION TRANSACTION READ ONLY".into());
    }
    if let Some(tz) = cfg.options.time_zone.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        init.push(format!("SET time_zone = {}", crate::mysql::lit(tz)));
    }
    let script = cfg.options.startup_script.trim().to_string();
    let pool_opts = || {
        let (init, script) = (init.clone(), script.clone());
        MySqlPoolOptions::new()
        // Each console·table tab gets its own dedicated connection. Connections open only when needed, so just keep the cap generous
        .max_connections(crate::db::POOL_MAX)
        .idle_timeout(crate::db::POOL_IDLE)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(move |conn, _| {
            let init = init.clone();
            let script = script.clone();
            Box::pin(async move {
                for s in &init {
                    conn.execute(s.as_str()).await?;
                }
                if !script.is_empty() {
                    conn.execute(sqlx::raw_sql(&script)).await?;
                }
                Ok(())
            })
        })
    };
    // Require TLS even when SSL isn't set (certificate not verified). Plaintext only for this machine · SSH tunnels, or when "Allow unencrypted" is on
    // (Old servers (MySQL 5.7 etc.) can't negotiate TLS 1.2. Someone stripping TLS in the middle looks the same, so we don't fall back to plaintext automatically)
    let explicit_ssl = cfg.active_ssl().is_some();
    let plain_ok = crate::db::plaintext_allowed(cfg, host);
    if !explicit_ssl && !plain_ok {
        opts = opts.ssl_mode(MySqlSslMode::Required);
    }
    let pool = match pool_opts().connect_with(opts.clone()).await {
        Err(e) => match on_connect_error(&e, explicit_ssl, plain_ok) {
            OnConnectError::RetryPlain => pool_opts().connect_with(opts.ssl_mode(MySqlSslMode::Disabled)).await?,
            OnConnectError::NeedsTls => return Err(AppError::Msg(format!("{NEEDS_TLS}\n({e})"))),
            OnConnectError::Dropped => return Err(AppError::Msg(format!("{DROPPED_TLS}\n({e})"))),
            OnConnectError::Fail => return Err(e.into()),
        },
        Ok(p) => p,
    };
    Ok(pool)
}

#[derive(Debug, PartialEq)]
enum OnConnectError {
    /// Plain text is allowed: connect again without TLS
    RetryPlain,
    /// Tell the user about "Allow unencrypted connection"
    NeedsTls,
    /// The server dropped the connection: maybe an old server refusing TLS, maybe a proxy without a backend — a softer hint
    Dropped,
    Fail,
}

/// What to do when the first connect fails. A TLS failure, or the server just dropping the connection (some old servers do during the
/// handshake), counts only when SSL isn't set explicitly. sqlx doesn't say at which step a connection was dropped (a proxy without a
/// backend, or a drop after auth, look the same), so a drop gets a hint that doesn't claim TLS was the cause
fn on_connect_error(e: &sqlx::Error, explicit_ssl: bool, plain_ok: bool) -> OnConnectError {
    let dropped = matches!(e, sqlx::Error::Io(io) if matches!(io.kind(), ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted));
    if explicit_ssl || !(is_tls_failure(e) || dropped) {
        OnConnectError::Fail
    } else if plain_ok {
        OnConnectError::RetryPlain
    } else if dropped {
        OnConnectError::Dropped
    } else {
        OnConnectError::NeedsTls
    }
}

/// Connection failure from failing to negotiate TLS (not auth failures etc.)
fn is_tls_failure(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Tls(_) => true,
        sqlx::Error::Io(io) => is_rustls_error(io),
        _ => false,
    }
}

/// sqlx hands rustls handshake errors over as InvalidData I/O errors; matched by rustls' Display texts (rustls::Error), e.g.
/// MySQL 5.7 limited to TLS 1.0/1.1: "received fatal alert: HandshakeFailure", MySQL 5.5: "received corrupt message of type UnknownProtocolVersion"
fn is_rustls_error(io: &std::io::Error) -> bool {
    const TEXTS: [&str; 6] = ["received fatal alert", "received corrupt message", "received unexpected message", "peer is incompatible", "peer misbehaved", "handshake"];
    let m = io.to_string().to_lowercase();
    io.kind() == ErrorKind::InvalidData && TEXTS.iter().any(|t| m.contains(t))
}

#[cfg(test)]
mod tests {
    /// Real errors from old servers (sqlx + rustls) count as TLS failures; auth and network errors don't
    #[test]
    fn rustls_handshake_errors_are_tls_failures() {
        use std::io::{Error, ErrorKind};
        for m in ["received fatal alert: HandshakeFailure", "received fatal alert: ProtocolVersion", "received corrupt message of type UnknownProtocolVersion",
            "received corrupt message of type InvalidContentType", "peer is incompatible: ServerTlsVersionIsDisabledByOurConfig", "peer misbehaved: IllegalTlsInnerPlaintext", "handshake not complete"]
        {
            assert!(super::is_tls_failure(&sqlx::Error::Io(Error::new(ErrorKind::InvalidData, m))), "{m}");
        }
        assert!(super::is_tls_failure(&sqlx::Error::Tls("server does not support TLS".into())));
        assert!(!super::is_tls_failure(&sqlx::Error::Io(Error::new(ErrorKind::ConnectionRefused, "Connection refused (os error 61)"))));
        assert!(!super::is_tls_failure(&sqlx::Error::Io(Error::new(ErrorKind::InvalidData, "invalid packet sequence"))));
        assert!(!super::is_tls_failure(&sqlx::Error::Protocol("Access denied for user 'u'".into())));
    }

    /// A dropped connection can't be told apart from a proxy with no backend or a drop after auth: it gets a softer hint
    /// (old MySQL without TLS 1.2, or check the host, port and proxy) — not "didn't send the password"
    #[test]
    fn dropped_handshake_gets_the_tls_hint() {
        use super::OnConnectError::*;
        use std::io::{Error, ErrorKind};
        let io = |k: ErrorKind, m: &str| sqlx::Error::Io(Error::new(k, m));
        for k in [ErrorKind::UnexpectedEof, ErrorKind::ConnectionReset, ErrorKind::ConnectionAborted] {
            let e = io(k, "unexpected end of file");
            assert_eq!(super::on_connect_error(&e, false, false), Dropped, "{k:?}");
            assert_eq!(super::on_connect_error(&e, false, true), RetryPlain, "{k:?}");
            // SSL set explicitly: the raw error
            assert_eq!(super::on_connect_error(&e, true, false), Fail, "{k:?}");
        }
        assert!(!super::DROPPED_TLS.contains("didn't send the password") && super::DROPPED_TLS.contains("proxy") && super::DROPPED_TLS.contains("Allow unencrypted"));
        let tls = io(ErrorKind::InvalidData, "received fatal alert: HandshakeFailure");
        assert_eq!(super::on_connect_error(&tls, false, false), NeedsTls);
        assert_eq!(super::on_connect_error(&tls, false, true), RetryPlain);
        assert_eq!(super::on_connect_error(&tls, true, false), Fail);
        for e in [io(ErrorKind::ConnectionRefused, "Connection refused (os error 61)"), sqlx::Error::Protocol("Access denied for user 'u'".into())] {
            assert_eq!(super::on_connect_error(&e, false, false), Fail, "{e}");
            assert_eq!(super::on_connect_error(&e, false, true), Fail, "{e}");
        }
    }
}
