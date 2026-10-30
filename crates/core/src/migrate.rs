//! Schema compare: DDL that makes the target match the source. Order: create tables (without foreign keys) → columns → indexes·constraints → foreign keys → views → drops (last, marked dangerous)
use crate::app::{AppState, ensure_pool};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use std::collections::BTreeMap;

/// One change
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// create_table · add_column · alter_column · create_index · add_constraint · create_view · drop_…
    pub kind: String,
    /// Target (table, table.column …)
    pub object: String,
    pub sql: String,
    /// Deletes data·objects (not selected by default)
    pub destructive: bool,
}

/// Schema shape used for comparing. Columns·indexes·constraints are name → definition text (same text means same)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub tables: BTreeMap<String, TableModel>,
    pub views: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Col {
    pub ty: String,
    pub not_null: bool,
    pub default: Option<String>,
    /// MySQL: that line from SHOW CREATE TABLE (used as is for comparing·creating)
    pub line: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableModel {
    /// In order (name, column)
    pub columns: Vec<(String, Col)>,
    /// Non-foreign-key constraints (PK·UNIQUE·CHECK) and indexes: name → creating text
    pub keys: BTreeMap<String, String>,
    /// Foreign keys: name → definition
    pub fks: BTreeMap<String, String>,
}

/// Strip its own schema name (to compare two schemas as the same text): "s".x · s.x · `s`.x
fn norm(def: &str, schema: &str, kind: DbKind) -> String {
    let q = crate::sql::quote_ident(kind, schema);
    let mut out = def.replace(&format!("{q}."), "");
    if kind == DbKind::Mysql {
        out = out.replace(&format!("`{schema}`."), "");
    } else {
        out = out.replace(&format!("\"{schema}\"."), "").replace(&format!("{schema}."), "");
    }
    out
}

/// Source definition onto the target schema
fn remap(def: &str, src: &str, dst: &str, kind: DbKind) -> String {
    if src == dst {
        return def.to_string();
    }
    let (qs, qd) = (crate::sql::quote_ident(kind, src), crate::sql::quote_ident(kind, dst));
    let mut out = def.replace(&format!("{qs}."), &format!("{qd}."));
    if kind == DbKind::Mysql {
        out = out.replace(&format!("`{src}`."), &format!("`{dst}`."));
    } else {
        out = out.replace(&format!("\"{src}\"."), &format!("{qd}.")).replace(&format!("{src}."), &format!("{qd}."));
    }
    out
}

/// MySQL: add the DB name to REFERENCES `t` pointing at the same DB (so it's right even when run while connected to another DB)
fn qualify_refs(line: &str, db: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find("REFERENCES `") {
        out.push_str(&rest[..i + "REFERENCES ".len()]);
        let after = &rest[i + "REFERENCES ".len()..];
        let close = after[1..].find('`').map(|j| j + 2).unwrap_or(after.len());
        if !after[close..].starts_with('.') {
            out.push_str(&format!("`{db}`."));
        }
        out.push_str(&after[..close]);
        rest = &after[close..];
    }
    out.push_str(rest);
    out
}

async fn pg_model(p: &sqlx::PgPool, pool: &Pool, schema: &str) -> AppResult<Model> {
    let tables: Vec<(String,)> = sqlx::query_as("select c.relname::text from pg_class c join pg_namespace n on n.oid = c.relnamespace where n.nspname = $1 and c.relkind in ('r', 'p') and not c.relispartition order by 1")
        .bind(schema)
        .fetch_all(p)
        .await?;
    let mut m = Model::default();
    for (t,) in tables {
        let d = crate::meta::table_detail(pool, schema, &t).await?;
        let cons: Vec<&str> = d.constraints.iter().map(|c| c.name.as_str()).collect();
        let mut tm = TableModel { columns: d.columns.iter().map(|c| (c.name.clone(), Col { ty: c.data_type.clone(), not_null: !c.nullable, default: c.default_value.as_deref().map(|x| norm(x, schema, DbKind::Postgres)), line: String::new() })).collect(), ..Default::default() };
        for c in &d.constraints {
            let def = norm(&c.definition, schema, DbKind::Postgres);
            if c.kind == "f" {
                tm.fks.insert(c.name.clone(), def);
            } else {
                tm.keys.insert(c.name.clone(), format!("CONSTRAINT {} {def}", crate::sql::quote_ident(DbKind::Postgres, &c.name)));
            }
        }
        for i in d.indexes.iter().filter(|i| !cons.contains(&i.name.as_str())) {
            tm.keys.insert(i.name.clone(), norm(&i.definition, schema, DbKind::Postgres));
        }
        m.tables.insert(t, tm);
    }
    let views: Vec<(String, String)> = sqlx::query_as("select c.relname::text, pg_get_viewdef(c.oid, true) from pg_class c join pg_namespace n on n.oid = c.relnamespace where n.nspname = $1 and c.relkind = 'v' order by 1")
        .bind(schema)
        .fetch_all(p)
        .await?;
    for (v, def) in views {
        m.views.insert(v, norm(def.trim().trim_end_matches(';'), schema, DbKind::Postgres));
    }
    Ok(m)
}

async fn my_model(p: &sqlx::MySqlPool, db: &str) -> AppResult<Model> {
    use sqlx::Row;
    let objs: Vec<(String, String)> = sqlx::query_as("select cast(table_name as char), cast(table_type as char) from information_schema.tables where table_schema = ? order by 1")
        .bind(db)
        .fetch_all(p)
        .await?;
    let mut m = Model::default();
    let q = |s: &str| crate::sql::quote_ident(DbKind::Mysql, s);
    for (t, ty) in objs {
        if ty == "VIEW" {
            let def: Option<String> = sqlx::query_scalar("select cast(view_definition as char) from information_schema.views where table_schema = ? and table_name = ?").bind(db).bind(&t).fetch_one(p).await?;
            m.views.insert(t, norm(def.unwrap_or_default().trim(), db, DbKind::Mysql));
            continue;
        }
        let row = sqlx::query(&format!("SHOW CREATE TABLE {}.{}", q(db), q(&t))).fetch_one(p).await?;
        let ddl: String = row.try_get::<String, _>(1).or_else(|_| row.try_get::<Vec<u8>, _>(1).map(|b| String::from_utf8_lossy(&b).into_owned()))?;
        let mut tm = TableModel::default();
        // CREATE TABLE `t` ( … ) ENGINE=… : lines inside the parentheses
        for l in ddl.lines().skip(1) {
            let l = l.trim().trim_end_matches(',');
            if l.starts_with(')') {
                break;
            }
            let name_of = |s: &str| s.split('`').nth(1).unwrap_or("").to_string();
            if l.starts_with('`') {
                tm.columns.push((name_of(l), Col { line: l.to_string(), ..Default::default() }));
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

fn change(kind: &str, object: String, sql: String) -> Change {
    Change { destructive: kind.starts_with("drop"), kind: kind.into(), object, sql }
}

/// DDL that makes the target (dst) match the source (src)
pub fn diff(src: &Model, dst: &Model, kind: DbKind, src_schema: &str, dst_schema: &str) -> Vec<Change> {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    let tbl = |t: &str| format!("{}.{}", q(dst_schema), q(t));
    let map = |d: &str| {
        // Re-attach the target schema to the schema name stripped for comparing (MySQL foreign keys via qualify_refs)
        if kind == DbKind::Mysql { qualify_refs(d, dst_schema) } else { remap(d, src_schema, dst_schema, kind) }
    };
    let pg = kind == DbKind::Postgres;
    let col_sql = |c: &Col| if pg { format!("{}{}{}", c.ty, c.default.as_ref().map(|d| format!(" DEFAULT {d}")).unwrap_or_default(), if c.not_null { " NOT NULL" } else { "" }) } else { c.line.clone() };
    // PG: definition with the schema stripped for comparing → attach the target schema (sequences in indexes·defaults)
    let pg_qualify = |d: &str| -> String {
        if !pg {
            return d.to_string();
        }
        // CREATE INDEX i ON t USING … → ON "dst".t
        let d = match d.find(" ON ") {
            Some(i) if d.starts_with("CREATE") => {
                let after = &d[i + 4..];
                let after = after.strip_prefix("ONLY ").map(|x| format!("ONLY {}.{x}", q(dst_schema))).unwrap_or_else(|| format!("{}.{after}", q(dst_schema)));
                format!("{} ON {after}", &d[..i])
            }
            _ => d.to_string(),
        };
        // nextval('seq'::regclass) · REFERENCES t( → target schema
        let d = d.replace("nextval('", &format!("nextval('{}.", q(dst_schema)));
        match d.find("REFERENCES ") {
            Some(i) => format!("{}REFERENCES {}.{}", &d[..i], q(dst_schema), &d[i + 11..]),
            None => d,
        }
    };

    let mut create = vec![];
    let mut cols = vec![];
    let mut keys = vec![];
    let mut fks = vec![];
    let mut views = vec![];
    let mut drops = vec![];

    for (t, s) in &src.tables {
        match dst.tables.get(t) {
            None => {
                let mut pre = String::new();
                let mut lines: Vec<String> = vec![];
                for (n, c) in &s.columns {
                    let mut c = c.clone();
                    if pg {
                        if let Some(seq) = c.default.as_deref().and_then(|d| d.strip_prefix("nextval('")).and_then(|d| d.split('\'').next()) {
                            pre += &format!("CREATE SEQUENCE IF NOT EXISTS {}.{seq};\n", q(dst_schema));
                        }
                        c.default = c.default.as_deref().map(&pg_qualify);
                        lines.push(format!("    {} {}", q(n), col_sql(&c)));
                    } else {
                        lines.push(format!("    {}", c.line));
                    }
                }
                // Constraints (PK·UNIQUE·CHECK) go with the table, indexes separately
                for k in s.keys.values() {
                    if !pg || k.starts_with("CONSTRAINT") {
                        lines.push(format!("    {k}"));
                    }
                }
                create.push(change("create_table", t.clone(), format!("{pre}CREATE TABLE {} (\n{}\n);", tbl(t), lines.join(",\n"))));
                if pg {
                    for (n, k) in s.keys.iter().filter(|(_, k)| !k.starts_with("CONSTRAINT")) {
                        keys.push(change("create_index", format!("{t}.{n}"), format!("{};", pg_qualify(k))));
                    }
                }
                for (n, f) in &s.fks {
                    let sql = if pg { format!("ALTER TABLE {} ADD CONSTRAINT {} {};", tbl(t), q(n), pg_qualify(f)) } else { format!("ALTER TABLE {} ADD {};", tbl(t), map(f)) };
                    fks.push(change("add_constraint", format!("{t}.{n}"), sql));
                }
            }
            Some(d) => {
                for (n, c) in &s.columns {
                    match d.columns.iter().find(|x| x.0 == *n).map(|x| &x.1) {
                        None => {
                            let mut c = c.clone();
                            c.default = c.default.as_deref().map(&pg_qualify);
                            cols.push(change("add_column", format!("{t}.{n}"), format!("ALTER TABLE {} ADD COLUMN {} {};", tbl(t), q(n), if pg { col_sql(&c) } else { c.line.trim_start_matches(|ch| ch != ' ').trim().to_string() })));
                        }
                        Some(dc) if dc != c => {
                            let sql = if pg {
                                let mut acts = vec![];
                                if dc.ty != c.ty {
                                    acts.push(format!("ALTER COLUMN {} TYPE {} USING {}::{}", q(n), c.ty, q(n), c.ty));
                                }
                                if dc.not_null != c.not_null {
                                    acts.push(format!("ALTER COLUMN {} {} NOT NULL", q(n), if c.not_null { "SET" } else { "DROP" }));
                                }
                                if dc.default != c.default {
                                    acts.push(match &c.default {
                                        Some(v) => format!("ALTER COLUMN {} SET DEFAULT {}", q(n), pg_qualify(v)),
                                        None => format!("ALTER COLUMN {} DROP DEFAULT", q(n)),
                                    });
                                }
                                format!("ALTER TABLE {} {};", tbl(t), acts.join(", "))
                            } else {
                                format!("ALTER TABLE {} MODIFY COLUMN {};", tbl(t), c.line)
                            };
                            cols.push(change("alter_column", format!("{t}.{n}"), sql));
                        }
                        _ => {}
                    }
                }
                for (n, _) in d.columns.iter().filter(|x| !s.columns.iter().any(|y| y.0 == x.0)) {
                    drops.push(change("drop_column", format!("{t}.{n}"), format!("ALTER TABLE {} DROP COLUMN {};", tbl(t), q(n))));
                }
                // Keys·indexes
                let drop_key = |n: &str, k: &str| -> String {
                    match (pg, k.starts_with("CONSTRAINT"), n == "PRIMARY") {
                        (true, true, _) => format!("ALTER TABLE {} DROP CONSTRAINT {};", tbl(t), q(n)),
                        (true, false, _) => format!("DROP INDEX {}.{};", q(dst_schema), q(n)),
                        (false, _, true) => format!("ALTER TABLE {} DROP PRIMARY KEY;", tbl(t)),
                        (false, true, _) => format!("ALTER TABLE {} DROP CONSTRAINT {};", tbl(t), q(n)),
                        (false, false, _) => format!("ALTER TABLE {} DROP INDEX {};", tbl(t), q(n)),
                    }
                };
                let add_key = |k: &str| -> String {
                    match (pg, k.starts_with("CONSTRAINT")) {
                        (true, true) => format!("ALTER TABLE {} ADD {k};", tbl(t)),
                        (true, false) => format!("{};", pg_qualify(k)),
                        (false, _) => format!("ALTER TABLE {} ADD {k};", tbl(t)),
                    }
                };
                // Split indexes (KEY·INDEX) and constraints (CONSTRAINT …) and name them
                let is_index = |k: &str| !k.starts_with("CONSTRAINT") && (pg || !k.starts_with("PRIMARY"));
                for (n, k) in &s.keys {
                    match d.keys.get(n) {
                        None => keys.push(change(if is_index(k) { "create_index" } else { "add_constraint" }, format!("{t}.{n}"), add_key(k))),
                        Some(dk) if dk != k => keys.push(change("replace_key", format!("{t}.{n}"), format!("{}\n{}", drop_key(n, dk), add_key(k)))),
                        _ => {}
                    }
                }
                for (n, k) in d.keys.iter().filter(|(n, _)| !s.keys.contains_key(*n)) {
                    drops.push(change(if is_index(k) { "drop_index" } else { "drop_constraint" }, format!("{t}.{n}"), drop_key(n, k)));
                }
                let drop_fk = |n: &str| if pg { format!("ALTER TABLE {} DROP CONSTRAINT {};", tbl(t), q(n)) } else { format!("ALTER TABLE {} DROP FOREIGN KEY {};", tbl(t), q(n)) };
                let add_fk = |n: &str, f: &str| if pg { format!("ALTER TABLE {} ADD CONSTRAINT {} {};", tbl(t), q(n), pg_qualify(f)) } else { format!("ALTER TABLE {} ADD {};", tbl(t), map(f)) };
                for (n, f) in &s.fks {
                    match d.fks.get(n) {
                        None => fks.push(change("add_constraint", format!("{t}.{n}"), add_fk(n, f))),
                        Some(df) if df != f => fks.push(change("replace_constraint", format!("{t}.{n}"), format!("{}\n{}", drop_fk(n), add_fk(n, f)))),
                        _ => {}
                    }
                }
                for (n, _) in d.fks.iter().filter(|(n, _)| !s.fks.contains_key(*n)) {
                    drops.insert(0, change("drop_constraint", format!("{t}.{n}"), drop_fk(n)));
                }
            }
        }
    }
    for t in dst.tables.keys().filter(|t| !src.tables.contains_key(*t)) {
        drops.push(change("drop_table", t.clone(), format!("DROP TABLE {};", tbl(t))));
    }
    for (v, def) in &src.views {
        if dst.views.get(v) != Some(def) {
            let body = remap_view(def, dst_schema, kind);
            views.push(change("create_view", v.clone(), format!("CREATE OR REPLACE VIEW {} AS\n{};", tbl(v), body)));
        }
    }
    for v in dst.views.keys().filter(|v| !src.views.contains_key(*v)) {
        drops.insert(0, change("drop_view", v.clone(), format!("DROP VIEW {};", tbl(v))));
    }
    create.into_iter().chain(cols).chain(keys).chain(fks).chain(views).chain(drops).collect()
}

/// Attach the target schema to table names in the view definition (schema-stripped text): FROM t / JOIN t → FROM dst.t
fn remap_view(def: &str, dst: &str, kind: DbKind) -> String {
    let q = crate::sql::quote_ident(kind, dst);
    let mut out = String::new();
    let toks: Vec<&str> = def.split_inclusive(char::is_whitespace).collect();
    let mut prev = String::new();
    for t in toks {
        let word = t.trim();
        let lower = prev.to_lowercase();
        if (lower == "from" || lower == "join") && !word.is_empty() && !word.starts_with('(') && !word.contains('.') {
            out.push_str(&format!("{q}.{t}"));
        } else {
            out.push_str(t);
        }
        if !word.is_empty() {
            prev = word.to_string();
        }
    }
    out
}

impl AppState {
    /// Compare two schemas (of the same DB kind). DDL that makes the target match the source
    pub async fn compare_schemas(&self, source_id: String, source_schema: String, target_id: String, target_schema: String) -> AppResult<Vec<Change>> {
        let (sk, tk) = (self.kind_of(&source_id)?, self.kind_of(&target_id)?);
        if sk != tk {
            return Err(AppError::Msg("Compare two data sources of the same kind (PostgreSQL with PostgreSQL, MySQL with MySQL)".into()));
        }
        let (sp, tp) = (ensure_pool(self, &source_id).await?, ensure_pool(self, &target_id).await?);
        let model = |pool: Pool, schema: String| async move {
            match &pool {
                Pool::Pg(p) => pg_model(p, &pool, &schema).await,
                Pool::My(p) => my_model(p, &schema).await,
                Pool::Redis(..) => Err(AppError::Msg("Redis has no schema to compare".into())),
                Pool::Plugin(c) => Err(AppError::Msg(format!("Schema compare is not available for the {} driver yet", c.driver))),
            }
        };
        let (s, t) = (model(sp, source_schema.clone()).await?, model(tp, target_schema.clone()).await?);
        Ok(diff(&s, &t, sk, &source_schema, &target_schema))
    }
}

/// Data compare result: row counts matched by primary key, and SQL that makes the target match the source
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDiff {
    pub same: u64,
    pub only_source: u64,
    pub only_target: u64,
    pub different: u64,
    /// INSERT (source only) · UPDATE (different values) · DELETE (target only, last)
    pub sql: String,
    /// Too many rows, so only the first part was compared
    pub truncated: bool,
}

/// Max rows to compare (each side)
pub const MAX_COMPARE_ROWS: usize = 200_000;

impl AppState {
    /// Compare the data of two tables by primary key (common columns only)
    pub async fn compare_data(&self, source_id: String, source_schema: String, source_table: String, target_id: String, target_schema: String, target_table: String) -> AppResult<DataDiff> {
        let (kind, tk) = (self.kind_of(&source_id)?, self.kind_of(&target_id)?);
        if kind != tk || kind == DbKind::Redis {
            return Err(AppError::Msg("Compare tables of the same kind of database (PostgreSQL or MySQL)".into()));
        }
        let (sp, tp) = (ensure_pool(self, &source_id).await?, ensure_pool(self, &target_id).await?);
        let sd = crate::meta::table_detail(&sp, &source_schema, &source_table).await?;
        let td = crate::meta::table_detail(&tp, &target_schema, &target_table).await?;
        let pk: Vec<String> = sd.columns.iter().filter(|c| c.is_pk).map(|c| c.name.clone()).collect();
        if pk.is_empty() {
            return Err(AppError::Msg(format!("{source_table} has no primary key — data compare matches rows by the primary key")));
        }
        let tpk: Vec<String> = td.columns.iter().filter(|c| c.is_pk).map(|c| c.name.clone()).collect();
        if tpk != pk {
            return Err(AppError::Msg(format!("The primary keys differ ({} vs {})", pk.join(", "), tpk.join(", "))));
        }
        // Only columns present on both sides
        let cols: Vec<String> = sd.columns.iter().map(|c| c.name.clone()).filter(|n| td.columns.iter().any(|t| t.name == *n)).collect();
        let q = |s: &str| crate::sql::quote_ident(kind, s);
        let list = cols.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
        let fetch = |pool: Pool, schema: String, table: String| {
            let sql = format!("SELECT {list} FROM {}.{}", q(&schema), q(&table));
            async move { pool.run_query(&sql, MAX_COMPARE_ROWS).await }
        };
        let (a, b) = (fetch(sp, source_schema, source_table).await?, fetch(tp, target_schema.clone(), target_table.clone()).await?);
        let pk_idx: Vec<usize> = pk.iter().map(|p| cols.iter().position(|c| c == p).unwrap()).collect();
        let key = |r: &Vec<serde_json::Value>| pk_idx.iter().map(|&i| r[i].to_string()).collect::<Vec<_>>().join("\u{1}");
        let target_rows: std::collections::HashMap<String, &Vec<serde_json::Value>> = b.rows.iter().map(|r| (key(r), r)).collect();
        let lit = |v: &serde_json::Value| crate::transfer::sql_literal(kind, v);
        let tbl = format!("{}.{}", q(&target_schema), q(&target_table));
        let where_of = |r: &Vec<serde_json::Value>| pk_idx.iter().map(|&i| format!("{} = {}", q(&cols[i]), lit(&r[i]))).collect::<Vec<_>>().join(" AND ");
        let mut d = DataDiff { truncated: a.truncated || b.truncated, ..Default::default() };
        let (mut ins, mut upd, mut del) = (vec![], vec![], vec![]);
        let mut seen = std::collections::HashSet::new();
        for r in &a.rows {
            let k = key(r);
            seen.insert(k.clone());
            match target_rows.get(&k) {
                None => {
                    d.only_source += 1;
                    ins.push(format!("INSERT INTO {tbl} ({list}) VALUES ({});", r.iter().map(lit).collect::<Vec<_>>().join(", ")));
                }
                Some(t) if *t != r => {
                    d.different += 1;
                    let sets: Vec<String> = (0..cols.len()).filter(|&i| t[i] != r[i]).map(|i| format!("{} = {}", q(&cols[i]), lit(&r[i]))).collect();
                    upd.push(format!("UPDATE {tbl} SET {} WHERE {};", sets.join(", "), where_of(r)));
                }
                _ => d.same += 1,
            }
        }
        for r in b.rows.iter().filter(|r| !seen.contains(&key(r))) {
            d.only_target += 1;
            del.push(format!("DELETE FROM {tbl} WHERE {};", where_of(r)));
        }
        let mut sql = vec![];
        if !ins.is_empty() {
            sql.push(format!("-- {} row(s) only in the source\n{}", ins.len(), ins.join("\n")));
        }
        if !upd.is_empty() {
            sql.push(format!("-- {} row(s) with different values\n{}", upd.len(), upd.join("\n")));
        }
        if !del.is_empty() {
            sql.push(format!("-- {} row(s) only in the target (deletes them)\n{}", del.len(), del.join("\n")));
        }
        d.sql = sql.join("\n\n");
        Ok(d)
    }
}

// ───────── Migration files (Flyway style: V1__init.sql, V2__add_index.sql …) ─────────

/// Applied-history table (in the connected DB's default schema)
pub const HISTORY_TABLE: &str = "qurry_schema_history";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    pub version: String,
    pub description: String,
    pub file: String,
    pub checksum: String,
    /// pending · applied · changed (file changed after applying) · missing (recorded but the file is gone)
    pub status: String,
    pub applied_at: Option<String>,
}

/// V1__init.sql → ("1", "init"). V1_2__x / V1.2__x → "1.2"
fn parse_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix('V').or_else(|| name.strip_prefix('v'))?;
    let (ver, desc) = rest.split_once("__")?;
    let desc = desc.strip_suffix(".sql").or_else(|| desc.strip_suffix(".SQL"))?;
    let ver = ver.replace('_', ".");
    (!ver.is_empty() && ver.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))).then(|| (ver, desc.replace('_', " ")))
}

fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Value for detecting content changes (FNV-1a, CRLF newlines as LF)
fn checksum(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.replace("\r\n", "\n").bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Migration files in the folder (in version order): (version, description, file name, content)
fn scan_dir(dir: &str) -> AppResult<Vec<(String, String, String, String)>> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some((v, d)) = parse_name(&name) {
            out.push((v, d, name, std::fs::read_to_string(e.path())?));
        }
    }
    out.sort_by_key(|x| version_key(&x.0));
    Ok(out)
}

impl AppState {
    /// Applied history: version → (checksum, applied time). Empty if the history table doesn't exist
    async fn history_of(&self, pool: &Pool) -> std::collections::HashMap<String, (String, String)> {
        let q = format!("SELECT version, checksum, CAST(applied_at AS CHAR(40)) FROM {HISTORY_TABLE}");
        let q = if matches!(pool, Pool::Pg(_)) { q.replace("CHAR(40)", "text") } else { q };
        match pool.run_query(&q, 100_000).await {
            Ok(r) => r.rows.into_iter().map(|r| (crate::db::cell_text(&r[0]), (crate::db::cell_text(&r[1]), crate::db::cell_text(&r[2])))).collect(),
            Err(_) => Default::default(),
        }
    }

    /// Migrations in the folder and their applied state
    pub async fn migration_status(&self, id: String, dir: String) -> AppResult<Vec<Migration>> {
        let pool = ensure_pool(self, &id).await?;
        let mut applied = self.history_of(&pool).await;
        let mut out: Vec<Migration> = scan_dir(&dir)?
            .into_iter()
            .map(|(version, description, file, text)| {
                let checksum = checksum(&text);
                let (status, applied_at) = match applied.remove(&version) {
                    Some((c, at)) if c == checksum => ("applied", Some(at)),
                    Some((_, at)) => ("changed", Some(at)),
                    None => ("pending", None),
                };
                Migration { version, description, file, checksum, status: status.into(), applied_at }
            })
            .collect();
        // Recorded but the file is gone
        for (version, (checksum, at)) in applied {
            out.push(Migration { version, description: String::new(), file: String::new(), checksum, status: "missing".into(), applied_at: Some(at) });
        }
        out.sort_by_key(|m| version_key(&m.version));
        Ok(out)
    }

    /// Apply the not-yet-applied ones in version order (a transaction per file, stops at the first failure). The applied versions
    pub async fn apply_migrations(&self, id: String, dir: String) -> AppResult<Vec<String>> {
        let cfg = self.source(&id)?;
        if cfg.read_only {
            return Err(AppError::Msg(format!("Data source \"{}\" is read-only", cfg.name)));
        }
        let kind = cfg.kind;
        let status = self.migration_status(id.clone(), dir.clone()).await?;
        if let Some(m) = status.iter().find(|m| m.status == "changed") {
            return Err(AppError::Msg(format!("V{} ({}) was changed after it was applied — put the file back, or add the change as a new version", m.version, m.file)));
        }
        let pool = ensure_pool(self, &id).await?;
        let create = match kind {
            DbKind::Mysql => format!("CREATE TABLE IF NOT EXISTS {HISTORY_TABLE} (version VARCHAR(50) PRIMARY KEY, description VARCHAR(200) NOT NULL, checksum VARCHAR(32) NOT NULL, applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)"),
            _ => format!("CREATE TABLE IF NOT EXISTS {HISTORY_TABLE} (version TEXT PRIMARY KEY, description TEXT NOT NULL, checksum TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())"),
        };
        pool.run_query(&create, 1).await?;
        let files = scan_dir(&dir)?;
        let mut done = vec![];
        for m in status.iter().filter(|m| m.status == "pending") {
            let text = &files.iter().find(|f| f.0 == m.version).unwrap().3;
            let stmts: Vec<String> = crate::sql::split_statements(text, kind).into_iter().map(|(a, b)| text[a..b].to_string()).collect();
            let fail = |e: String| AppError::Msg(format!("V{} ({}) failed and was rolled back: {e}", m.version, m.file));
            let insert = format!("INSERT INTO {HISTORY_TABLE} (version, description, checksum) VALUES ({}, {}, {})", crate::sql::literal(kind, Some(&m.version)), crate::sql::literal(kind, Some(&m.description)), crate::sql::literal(kind, Some(&m.checksum)));
            match &pool {
                Pool::Pg(p) => {
                    let mut tx = p.begin().await?;
                    for s in stmts.iter().chain(std::iter::once(&insert)) {
                        sqlx::Executor::execute(&mut *tx, sqlx::raw_sql(s.as_str())).await.map_err(|e| fail(e.to_string()))?;
                    }
                    tx.commit().await.map_err(|e| fail(e.to_string()))?;
                }
                Pool::My(p) => {
                    // MySQL: DDL commits implicitly (only data statements are rolled back together)
                    let mut tx = p.begin().await?;
                    for s in stmts.iter().chain(std::iter::once(&insert)) {
                        sqlx::Executor::execute(&mut *tx, sqlx::raw_sql(s.as_str())).await.map_err(|e| fail(e.to_string()))?;
                    }
                    tx.commit().await.map_err(|e| fail(e.to_string()))?;
                }
                Pool::Redis(..) => return Err(AppError::Msg("Redis has no migrations".into())),
                Pool::Plugin(c) => return Err(AppError::Msg(format!("Migrations are not available for the {} driver yet", c.driver))),
            }
            done.push(m.version.clone());
        }
        Ok(done)
    }
}

/// Save as the next version: V{n}__{description}.sql. The saved path
pub fn save_migration(dir: &str, description: &str, sql: &str) -> AppResult<String> {
    let next = scan_dir(dir)?.iter().filter_map(|f| version_key(&f.0).first().copied()).max().unwrap_or(0) + 1;
    let mut desc: String = description.trim().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    while desc.contains("__") {
        desc = desc.replace("__", "_");
    }
    let desc = desc.trim_matches('_');
    let path = std::path::Path::new(dir).join(format!("V{next}__{}.sql", if desc.is_empty() { "change" } else { desc }));
    std::fs::write(&path, sql)?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn state(kind: DbKind) -> Option<(AppState, String)> {
        let (env, kind_s) = if kind == DbKind::Postgres { ("QURRY_TEST_PG", "postgres") } else { ("QURRY_TEST_MY", "mysql") };
        let e = std::env::var(env).ok()?;
        let p: Vec<&str> = e.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-mig-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        let (user, pass, db) = if kind == DbKind::Mysql { (p[2], Some(p[3].to_string()), p[4]) } else { (p[2], None, p[3]) };
        let cfg: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "x", "name": "x", "kind": kind_s, "host": p[0], "port": p[1].parse::<u16>().unwrap(), "user": user, "database": db })).unwrap();
        st.save_connection(cfg, pass, None).await.unwrap();
        Some((st, db.to_string()))
    }

    /// Compare → run the generated SQL on the target → comparing again shows no differences
    async fn roundtrip(kind: DbKind, setup: &[&str], src: &str, dst: &str) {
        let Some((st, _)) = state(kind).await else { return };
        let q = |s: String| st.run_query("x".into(), s, None, None);
        for s in setup {
            q(s.to_string()).await.unwrap_or_else(|e| panic!("{s}: {e}"));
        }
        let changes = st.compare_schemas("x".into(), src.into(), "x".into(), dst.into()).await.unwrap();
        assert!(!changes.is_empty());
        let kinds: Vec<&str> = changes.iter().map(|c| c.kind.as_str()).collect();
        for want in ["create_table", "add_column", "alter_column", "drop_column", "create_index", "add_constraint", "drop_table", "create_view"] {
            assert!(kinds.contains(&want), "{want} missing in {kinds:?}\n{changes:#?}");
        }
        assert!(changes.iter().filter(|c| c.kind.starts_with("drop")).all(|c| c.destructive));
        // Drops come last
        let first_drop = changes.iter().position(|c| c.destructive).unwrap();
        assert!(changes[first_drop..].iter().all(|c| c.destructive), "{changes:#?}");
        for c in &changes {
            for (a, b) in crate::sql::split_statements(&c.sql, kind) {
                let stmt = c.sql[a..b].to_string();
                q(stmt.clone()).await.unwrap_or_else(|e| panic!("{stmt}: {e}\n{changes:#?}"));
            }
        }
        let again = st.compare_schemas("x".into(), src.into(), "x".into(), dst.into()).await.unwrap();
        assert!(again.is_empty(), "{again:#?}");
    }

    #[tokio::test]
    #[ignore]
    async fn pg_migrations_apply_in_order_and_detect_changes() {
        let Some((st, _)) = state(DbKind::Postgres).await else { return };
        let q = |s: &str| st.run_query("x".into(), s.into(), None, None);
        q("drop table if exists qurry_schema_history, qurry_mig_a").await.ok();
        let dir = std::env::temp_dir().join(format!("qurry-migs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        std::fs::write(dir.join("V1__create.sql"), "create table qurry_mig_a (id int primary key);").unwrap();
        std::fs::write(dir.join("V2__add.sql"), "alter table qurry_mig_a add column name text;\ninsert into qurry_mig_a values (1, 'x');").unwrap();
        std::fs::write(dir.join("V10__later.sql"), "insert into qurry_mig_a values (10, 'y');").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a migration").unwrap();
        let st0 = st.migration_status("x".into(), d.clone()).await.unwrap();
        assert_eq!(st0.iter().map(|m| (m.version.as_str(), m.status.as_str())).collect::<Vec<_>>(), [("1", "pending"), ("2", "pending"), ("10", "pending")]);
        assert_eq!(st.apply_migrations("x".into(), d.clone()).await.unwrap(), ["1", "2", "10"]);
        assert!(st.migration_status("x".into(), d.clone()).await.unwrap().iter().all(|m| m.status == "applied"));
        assert_eq!(q("select count(*) from qurry_mig_a").await.unwrap().rows[0][0], serde_json::json!(2));
        // On failure that file is rolled back and not recorded
        std::fs::write(dir.join("V11__bad.sql"), "insert into qurry_mig_a values (11, 'z');\nselect * from no_such_table;").unwrap();
        let e = st.apply_migrations("x".into(), d.clone()).await.unwrap_err().to_string();
        assert!(e.contains("V11") && e.contains("no_such_table"), "{e}");
        assert_eq!(q("select count(*) from qurry_mig_a").await.unwrap().rows[0][0], serde_json::json!(2));
        std::fs::remove_file(dir.join("V11__bad.sql")).unwrap();
        // If an applied file changed, report it and don't apply
        std::fs::write(dir.join("V1__create.sql"), "create table qurry_mig_a (id bigint primary key);").unwrap();
        std::fs::write(dir.join("V12__more.sql"), "select 1;").unwrap();
        assert_eq!(st.migration_status("x".into(), d.clone()).await.unwrap()[0].status, "changed");
        assert!(st.apply_migrations("x".into(), d.clone()).await.unwrap_err().to_string().contains("V1"));
        // Save as the next version
        let p = save_migration(&d, "add index!", "create index on qurry_mig_a (name);").unwrap();
        assert!(p.ends_with("V13__add_index.sql"), "{p}");
        q("drop table if exists qurry_schema_history, qurry_mig_a").await.ok();
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Data compare → generated SQL on the target → comparing again shows everything equal
    #[tokio::test]
    #[ignore]
    async fn pg_data_compare_and_sync() {
        let Some((st, _)) = state(DbKind::Postgres).await else { return };
        let q = |s: &str| st.run_query("x".into(), s.into(), None, None);
        for s in [
            "drop table if exists qurry_dsrc, qurry_ddst",
            "create table qurry_dsrc (id int, k text, name text, amount numeric(10,2), primary key (id, k))",
            "create table qurry_ddst (id int, k text, name text, amount numeric(10,2), extra int, primary key (id, k))",
            "insert into qurry_dsrc values (1, 'a', 'same', 1.5), (2, 'a', 'it''s new', 2), (3, 'a', 'changed', 3), (4, 'a', null, 4)",
            "insert into qurry_ddst values (1, 'a', 'same', 1.5, 9), (3, 'a', 'old', 3, 9), (4, 'a', 'was set', 4, 9), (5, 'a', 'gone', 5, 9)",
        ] {
            q(s).await.unwrap();
        }
        let d = st.compare_data("x".into(), "public".into(), "qurry_dsrc".into(), "x".into(), "public".into(), "qurry_ddst".into()).await.unwrap();
        assert_eq!((d.same, d.only_source, d.only_target, d.different), (1, 1, 1, 2), "{}", d.sql);
        assert!(d.sql.find("INSERT").unwrap() < d.sql.find("DELETE").unwrap(), "{}", d.sql);
        for (a, b) in crate::sql::split_statements(&d.sql, DbKind::Postgres) {
            q(&d.sql[a..b]).await.unwrap_or_else(|e| panic!("{}: {e}", &d.sql[a..b]));
        }
        let again = st.compare_data("x".into(), "public".into(), "qurry_dsrc".into(), "x".into(), "public".into(), "qurry_ddst".into()).await.unwrap();
        assert_eq!((again.same, again.only_source, again.only_target, again.different), (4, 0, 0, 0), "{}", again.sql);
        assert!(again.sql.trim().is_empty() || again.sql.starts_with("--"), "{}", again.sql);
        q("drop table qurry_dsrc, qurry_ddst").await.ok();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_compare_and_apply_makes_schemas_equal() {
        roundtrip(
            DbKind::Postgres,
            &[
                "drop schema if exists qurry_src cascade",
                "drop schema if exists qurry_dst cascade",
                "create schema qurry_src",
                "create schema qurry_dst",
                "create table qurry_src.customers (id serial primary key, email text not null unique, name varchar(50) default 'anon', score int check (score >= 0))",
                "create table qurry_src.orders (id bigint primary key, customer_id int references qurry_src.customers(id), total numeric(10,2) not null default 0)",
                "create index orders_customer_idx on qurry_src.orders (customer_id)",
                "create view qurry_src.big_orders as select id, total from qurry_src.orders where total > 100",
                // Target: missing column·different type·extra column·extra table
                "create table qurry_dst.customers (id serial primary key, email text, name text, legacy int)",
                "create table qurry_dst.old_stuff (id int)",
            ],
            "qurry_src",
            "qurry_dst",
        )
        .await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_compare_and_apply_makes_schemas_equal() {
        roundtrip(
            DbKind::Mysql,
            &[
                "drop database if exists qurry_src",
                "drop database if exists qurry_dst",
                "create database qurry_src",
                "create database qurry_dst",
                "create table qurry_src.customers (id int auto_increment primary key, email varchar(100) not null, name varchar(50) default 'anon', unique key uq_email (email))",
                "create table qurry_src.orders (id bigint primary key, customer_id int, total decimal(10,2) not null default 0, key orders_customer_idx (customer_id), constraint fk_cust foreign key (customer_id) references qurry_src.customers(id))",
                "create view qurry_src.big_orders as select id, total from qurry_src.orders where total > 100",
                "create table qurry_dst.customers (id int auto_increment primary key, email varchar(10), legacy int)",
                "create table qurry_dst.old_stuff (id int)",
            ],
            "qurry_src",
            "qurry_dst",
        )
        .await;
    }
}
