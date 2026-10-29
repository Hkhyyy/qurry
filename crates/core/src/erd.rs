//! ERD: a schema's tables and columns (with PK and FK marks) and foreign key relationships. The GUI draws a diagram, the TUI a relationship list, and both can copy as Mermaid
use crate::app::{AppState, ensure_pool};
use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErdColumn {
    pub name: String,
    pub data_type: String,
    pub pk: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErdTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<ErdColumn>,
}

/// Columns of the from table reference columns of the to table
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErdRelation {
    pub name: String,
    pub from_schema: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_schema: String,
    pub to_table: String,
    pub to_columns: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Erd {
    pub tables: Vec<ErdTable>,
    pub relations: Vec<ErdRelation>,
}

impl Erd {
    /// Schema with the most tables (its tables use just the name, others are prefixed with the schema)
    fn main_schema(&self) -> &str {
        let mut count: std::collections::HashMap<&str, usize> = Default::default();
        for t in &self.tables {
            *count.entry(t.schema.as_str()).or_default() += 1;
        }
        self.tables.iter().map(|t| t.schema.as_str()).max_by_key(|s| (count[s], std::cmp::Reverse(self.tables.iter().position(|t| t.schema == *s)))).unwrap_or("")
    }

    fn label(&self, schema: &str, name: &str, sep: &str) -> String {
        if schema == self.main_schema() || schema.is_empty() { name.to_string() } else { format!("{schema}{sep}{name}") }
    }

    /// Mermaid erDiagram (for docs and issues). Tables in other schemas as schema_table
    pub fn mermaid(&self) -> String {
        let ident = |s: String| s.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>();
        let mut out = String::from("erDiagram\n");
        for t in &self.tables {
            let fks: std::collections::HashSet<&str> = self.relations.iter().filter(|r| r.from_schema == t.schema && r.from_table == t.name).flat_map(|r| r.from_columns.iter().map(|c| c.as_str())).collect();
            out += &format!("  {} {{\n", ident(self.label(&t.schema, &t.name, "_")));
            for c in &t.columns {
                // Mermaid types are one word (length/precision in parentheses dropped: numeric(12,2) → numeric)
                let ty: String = c.data_type.split('(').next().unwrap_or("").trim().replace(' ', "_").chars().filter(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '[' || *ch == ']').collect();
                let keys = match (c.pk, fks.contains(c.name.as_str())) {
                    (true, true) => " PK, FK",
                    (true, false) => " PK",
                    (false, true) => " FK",
                    _ => "",
                };
                out += &format!("    {} {}{keys}\n", if ty.is_empty() { "unknown".into() } else { ty }, ident(c.name.clone()));
            }
            out += "  }\n";
        }
        for r in &self.relations {
            out += &format!("  {} ||--o{{ {} : \"{}\"\n", ident(self.label(&r.to_schema, &r.to_table, "_")), ident(self.label(&r.from_schema, &r.from_table, "_")), r.from_columns.join(", "));
        }
        out
    }

    /// TUI: per table, what it references (→) · what references it (←)
    pub fn text(&self) -> String {
        let mut out = vec![];
        for t in &self.tables {
            let mut lines = vec![self.label(&t.schema, &t.name, ".")];
            for r in self.relations.iter().filter(|r| r.from_schema == t.schema && r.from_table == t.name) {
                lines.push(format!("  → {} ({} → {})", self.label(&r.to_schema, &r.to_table, "."), r.from_columns.join(", "), r.to_columns.join(", ")));
            }
            for r in self.relations.iter().filter(|r| r.to_schema == t.schema && r.to_table == t.name) {
                lines.push(format!("  ← {} ({} → {})", self.label(&r.from_schema, &r.from_table, "."), r.from_columns.join(", "), r.to_columns.join(", ")));
            }
            if lines.len() == 1 {
                lines.push("  (no foreign keys)".into());
            }
            out.push(lines.join("\n"));
        }
        out.join("\n\n") + "\n"
    }
}

/// (schema, table) → columns
type Columns = std::collections::HashMap<(String, String), Vec<ErdColumn>>;

async fn pg(p: &sqlx::PgPool, schemas: &[String]) -> AppResult<Erd> {
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
    .fetch_all(p)
    .await?;
    let mut wanted: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "select n.nspname::text, c.relname::text from pg_class c join pg_namespace n on n.oid = c.relnamespace
         where c.relkind in ('r', 'p') and n.nspname = any($1) and not c.relispartition order by n.nspname, c.relname",
    )
    .bind(schemas)
    .fetch_all(p)
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
    .fetch_all(p)
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

async fn my(p: &sqlx::MySqlPool, schemas: &[String]) -> AppResult<Erd> {
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
    let rows = q.fetch_all(p).await?;
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
    let mut wanted = q.fetch_all(p).await?;
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
    for (s, t, name, data_type, nullable, pk) in q.fetch_all(p).await? {
        by.entry((s, t)).or_default().push(ErdColumn { name, data_type, pk: pk != 0, nullable: nullable != 0 });
    }
    Ok(Erd { tables: wanted.into_iter().map(|(s, n)| ErdTable { columns: by.remove(&(s.clone(), n.clone())).unwrap_or_default(), schema: s, name: n }).collect(), relations })
}

impl AppState {
    /// Tables and relationships of schemas. If a relationship points to another schema, that table is included too
    pub async fn erd(&self, id: String, schemas: Vec<String>) -> AppResult<Erd> {
        let mut e = match ensure_pool(self, &id).await? {
            Pool::Pg(p) => pg(&p, &schemas).await?,
            Pool::My(p) => my(&p, &schemas).await?,
            Pool::Redis(..) => return Err(crate::error::AppError::Msg("Redis has no tables".into())),
            Pool::Plugin(c) => return Err(crate::error::AppError::Msg(format!("Diagrams are not available for the {} driver yet", c.driver))),
        };
        // In the order of the chosen schemas
        e.tables.sort_by_key(|t| (schemas.iter().position(|s| *s == t.schema).unwrap_or(usize::MAX), t.schema.clone(), t.name.clone()));
        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Erd {
        let col = |n: &str, t: &str, pk: bool| ErdColumn { name: n.into(), data_type: t.into(), pk, nullable: !pk };
        Erd {
            tables: vec![
                ErdTable { schema: "public".into(), name: "customers".into(), columns: vec![col("id", "bigint", true), col("name", "text", false)] },
                ErdTable { schema: "public".into(), name: "orders".into(), columns: vec![col("id", "bigint", true), col("customer_id", "bigint", false)] },
                ErdTable { schema: "billing".into(), name: "invoices".into(), columns: vec![col("id", "bigint", true), col("order_id", "bigint", false)] },
            ],
            relations: vec![
                ErdRelation { name: "orders_customer_id_fkey".into(), from_schema: "public".into(), from_table: "orders".into(), from_columns: vec!["customer_id".into()], to_schema: "public".into(), to_table: "customers".into(), to_columns: vec!["id".into()] },
                ErdRelation { name: "invoices_order_id_fkey".into(), from_schema: "billing".into(), from_table: "invoices".into(), from_columns: vec!["order_id".into()], to_schema: "public".into(), to_table: "orders".into(), to_columns: vec!["id".into()] },
            ],
        }
    }

    #[test]
    fn mermaid_er_diagram() {
        let m = sample().mermaid();
        assert!(m.starts_with("erDiagram\n"), "{m}");
        assert!(m.contains("  customers {\n    bigint id PK\n    text name\n  }"), "{m}");
        let mut e = sample();
        e.tables[0].columns[1].data_type = "numeric(12,2)".into();
        e.tables[1].columns[1].data_type = "character varying(20)".into();
        let m2 = e.mermaid();
        assert!(m2.contains("    numeric name\n") && m2.contains("    character_varying customer_id FK\n"), "{m2}");
        assert!(m.contains("  orders {\n    bigint id PK\n    bigint customer_id FK\n  }"), "{m}");
        // Prefix non-public schemas with the schema name
        assert!(m.contains("  billing_invoices {"), "{m}");
        assert!(m.contains("  customers ||--o{ orders : \"customer_id\""), "{m}");
        assert!(m.contains("  orders ||--o{ billing_invoices : \"order_id\""), "{m}");
    }

    #[test]
    fn relation_text_for_tui() {
        let t = sample().text();
        assert!(t.contains("orders\n  → customers (customer_id → id)\n  ← billing.invoices (order_id → id)"), "{t}");
        assert!(t.contains("customers\n  ← orders (customer_id → id)"), "{t}");
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_erd_reads_tables_and_foreign_keys() {
        let Ok(e) = std::env::var("QURRY_TEST_MY") else { return };
        let p: Vec<&str> = e.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-erd-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        let cfg = crate::store::ConnectionConfig {
            id: "m".into(), name: "m".into(), kind: crate::store::DbKind::Mysql, host: p[0].into(), port: p[1].parse().unwrap(), user: p[2].into(), database: p[4].into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        st.save_connection(cfg, Some(p[3].into()), None).await.unwrap();
        let q = |s: &str| st.run_query("m".into(), s.into(), None, None);
        q("drop table if exists erd_c, erd_b, erd_a").await.ok();
        q("create table erd_a (id int primary key)").await.unwrap();
        q("create table erd_b (id int, x int, a_id int, primary key (id, x), foreign key (a_id) references erd_a(id))").await.unwrap();
        q("create table erd_c (b_id int, b_x int, foreign key (b_id, b_x) references erd_b(id, x))").await.unwrap();
        let erd = st.erd("m".into(), vec![p[4].into()]).await.unwrap();
        let c_rel = erd.relations.iter().find(|r| r.from_table == "erd_c").unwrap();
        assert_eq!((c_rel.from_columns.clone(), c_rel.to_columns.clone()), (vec!["b_id".to_string(), "b_x".to_string()], vec!["id".to_string(), "x".to_string()]));
        let b = erd.tables.iter().find(|t| t.name == "erd_b").unwrap();
        assert_eq!(b.columns.iter().filter(|c| c.pk).count(), 2);
        q("drop table if exists erd_c, erd_b, erd_a").await.ok();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_erd_reads_tables_and_foreign_keys() {
        let Ok(e) = std::env::var("QURRY_TEST_PG") else { return };
        let p: Vec<&str> = e.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-erd-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        let cfg = crate::store::ConnectionConfig {
            id: "e".into(), name: "e".into(), kind: crate::store::DbKind::Postgres, host: p[0].into(), port: p[1].parse().unwrap(), user: p[2].into(), database: p[3].into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        st.store.save(&cfg).unwrap();
        let q = |s: &str| st.run_query("e".into(), s.into(), None, None);
        q("drop schema if exists qurry_erd cascade").await.ok();
        q("create schema qurry_erd").await.unwrap();
        q("create table qurry_erd.a (id int primary key, name text)").await.unwrap();
        q("create table qurry_erd.b (id int, x int, a_id int references qurry_erd.a(id), primary key (id, x))").await.unwrap();
        q("create table qurry_erd.c (b_id int, b_x int, foreign key (b_id, b_x) references qurry_erd.b(id, x))").await.unwrap();
        let erd = st.erd("e".into(), vec!["qurry_erd".into()]).await.unwrap();
        let names: Vec<_> = erd.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        let b = &erd.tables[1];
        assert_eq!(b.columns.iter().filter(|c| c.pk).map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "x"]);
        assert_eq!(erd.relations.len(), 2);
        let c_rel = erd.relations.iter().find(|r| r.from_table == "c").unwrap();
        assert_eq!((c_rel.from_columns.clone(), c_rel.to_columns.clone()), (vec!["b_id".to_string(), "b_x".to_string()], vec!["id".to_string(), "x".to_string()]));
        q("drop schema qurry_erd cascade").await.ok();
    }
}
