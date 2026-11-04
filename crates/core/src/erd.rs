//! ERD: a schema's tables and columns (with PK and FK marks) and foreign key relationships. The GUI draws a diagram, the TUI a relationship list, and both can copy as Mermaid
use crate::app::{AppState, ensure_pool};
use crate::backend::{Backend, dispatch};
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
pub(crate) type Columns = std::collections::HashMap<(String, String), Vec<ErdColumn>>;

impl AppState {
    /// Tables and relationships of schemas. If a relationship points to another schema, that table is included too
    pub async fn erd(&self, id: String, schemas: Vec<String>) -> AppResult<Erd> {
        let mut e = dispatch!(ensure_pool(self, &id).await?, b => b.erd(&schemas).await)?;
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
        let Some(crate::testenv::Server { mut cfg, password }) = crate::testenv::my() else { return };
        cfg.id = "m".into();
        let db = cfg.database.clone();
        let dir = std::env::temp_dir().join(format!("qurry-erd-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(cfg, password, None).await.unwrap();
        let q = |s: &str| st.run_query("m".into(), s.into(), None, None);
        q("drop table if exists erd_c, erd_b, erd_a").await.ok();
        q("create table erd_a (id int primary key)").await.unwrap();
        q("create table erd_b (id int, x int, a_id int, primary key (id, x), foreign key (a_id) references erd_a(id))").await.unwrap();
        q("create table erd_c (b_id int, b_x int, foreign key (b_id, b_x) references erd_b(id, x))").await.unwrap();
        let erd = st.erd("m".into(), vec![db]).await.unwrap();
        let c_rel = erd.relations.iter().find(|r| r.from_table == "erd_c").unwrap();
        assert_eq!((c_rel.from_columns.clone(), c_rel.to_columns.clone()), (vec!["b_id".to_string(), "b_x".to_string()], vec!["id".to_string(), "x".to_string()]));
        let b = erd.tables.iter().find(|t| t.name == "erd_b").unwrap();
        assert_eq!(b.columns.iter().filter(|c| c.pk).count(), 2);
        q("drop table if exists erd_c, erd_b, erd_a").await.ok();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_erd_reads_tables_and_foreign_keys() {
        let Some(crate::testenv::Server { mut cfg, password }) = crate::testenv::pg() else { return };
        cfg.id = "e".into();
        let dir = std::env::temp_dir().join(format!("qurry-erd-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(cfg, password, None).await.unwrap();
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
