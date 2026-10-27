//! 스키마 비교: 대상(target)을 원본(source)과 같게 만드는 DDL. 순서: 테이블 만들기(외래 키 빼고) → 컬럼 → 인덱스·제약 → 외래 키 → 뷰 → 지우기(맨 끝, 위험 표시)
use crate::app::{AppState, ensure_pool};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use std::collections::BTreeMap;

/// 바꿀 것 하나
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// create_table · add_column · alter_column · create_index · add_constraint · create_view · drop_…
    pub kind: String,
    /// 대상 (테이블, 테이블.컬럼 …)
    pub object: String,
    pub sql: String,
    /// 데이터·객체를 지운다 (기본으로 고르지 않는다)
    pub destructive: bool,
}

/// 비교에 쓰는 스키마 모양. 열·인덱스·제약은 이름 → 정의 글 (같으면 같다)
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
    /// MySQL: SHOW CREATE TABLE 의 그 줄 (비교·만들기에 그대로)
    pub line: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableModel {
    /// 순서대로 (이름, 컬럼)
    pub columns: Vec<(String, Col)>,
    /// 외래 키가 아닌 제약 (PK·UNIQUE·CHECK) 과 인덱스: 이름 → 만드는 글
    pub keys: BTreeMap<String, String>,
    /// 외래 키: 이름 → 정의
    pub fks: BTreeMap<String, String>,
}

/// 자기 스키마 이름을 뺀다 (두 스키마를 같은 글로 비교하려고): "s".x · s.x · `s`.x
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

/// 원본 정의를 대상 스키마로
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

/// MySQL: 같은 DB 를 가리키는 REFERENCES `t` 에 DB 이름을 붙인다 (다른 DB 에 연결한 채로 돌려도 맞게)
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
        // CREATE TABLE `t` ( … ) ENGINE=… : 괄호 안 줄
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

/// 대상(dst)을 원본(src)과 같게 만드는 DDL
pub fn diff(src: &Model, dst: &Model, kind: DbKind, src_schema: &str, dst_schema: &str) -> Vec<Change> {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    let tbl = |t: &str| format!("{}.{}", q(dst_schema), q(t));
    let map = |d: &str| {
        // 비교용으로 뺀 스키마 이름을 대상 스키마로 다시 붙인다 (MySQL 외래 키는 qualify_refs 로)
        if kind == DbKind::Mysql { qualify_refs(d, dst_schema) } else { remap(d, src_schema, dst_schema, kind) }
    };
    let pg = kind == DbKind::Postgres;
    let col_sql = |c: &Col| if pg { format!("{}{}{}", c.ty, c.default.as_ref().map(|d| format!(" DEFAULT {d}")).unwrap_or_default(), if c.not_null { " NOT NULL" } else { "" }) } else { c.line.clone() };
    // PG: 비교용으로 스키마를 뺀 정의 → 대상 스키마를 붙인다 (인덱스·기본값의 시퀀스)
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
        // nextval('seq'::regclass) · REFERENCES t( → 대상 스키마
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
                // 제약(PK·UNIQUE·CHECK)은 테이블과 같이, 인덱스는 따로
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
                // 키·인덱스
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
                // 인덱스(KEY·INDEX) 와 제약(CONSTRAINT …)을 나눠 이름 붙인다
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

/// 뷰 정의 (스키마를 뺀 글) 의 테이블 이름에 대상 스키마를 붙인다: FROM t / JOIN t → FROM dst.t
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
    /// 두 스키마를 비교한다 (같은 DB 종류끼리). 대상(target)을 원본(source)과 같게 만드는 DDL
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
            }
        };
        let (s, t) = (model(sp, source_schema.clone()).await?, model(tp, target_schema.clone()).await?);
        Ok(diff(&s, &t, sk, &source_schema, &target_schema))
    }
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

    /// 비교 → 만든 SQL 을 대상에 돌림 → 다시 비교하면 차이가 없다
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
        // 지우는 것은 맨 끝
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
                // 대상: 컬럼 모자람·타입 다름·남는 컬럼·남는 테이블
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
