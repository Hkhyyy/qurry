//! DDL tab → Structure view (v): a table's sections as text tables, the same sections as the GUI's TableStructureView
use crate::kit::*;
use qurry_core::meta::{ForeignKeyMeta, TableStructure};
use ratatui::prelude::*;

/// The view's lines: loading, the error (and how to get back), or the sections (wrapped within `width` cells)
pub fn lines(schema: &str, table: &str, s: Option<&Result<TableStructure, String>>, width: usize) -> Vec<Line<'static>> {
    match s {
        None => vec![Line::styled(format!("Loading the structure of {schema}.{table}…"), Style::default().fg(DIM))],
        Some(Err(e)) => vec![
            Line::styled(format!("Couldn't load the structure of {schema}.{table}: {e}"), Style::default().fg(DANGER)),
            Line::from(""),
            key_hints(&[("r", "retry"), ("v", "the SQL view (it still has the full DDL)")]),
        ],
        Some(Ok(s)) => sections(s, width),
    }
}

fn sections(s: &TableStructure, width: usize) -> Vec<Line<'static>> {
    let d = &s.detail;
    let mut out = vec![];
    let cols = d.columns.iter().map(|c| vec![if c.is_pk { format!("{} (PK)", c.name) } else { c.name.clone() }, c.data_type.clone(), yes(c.nullable), c.default_value.clone().unwrap_or_default(), c.comment.clone().unwrap_or_default()]);
    section(&mut out, "Columns", Some(cols.collect()), &["Name", "Type", "Nullable", "Default", "Comment"], width);
    let label = |k: &str| match k {
        "p" => "Primary key".to_string(),
        "f" => "Foreign key".into(),
        "u" => "Unique".into(),
        "c" => "Check".into(),
        k => k.into(),
    };
    let cons = d.constraints.iter().map(|c| vec![c.name.clone(), label(&c.kind), c.definition.clone()]);
    section(&mut out, "Keys and constraints", Some(cons.collect()), &["Name", "Kind", "Definition"], width);
    // No Columns column when the source doesn't report index columns (driver plugins may leave them out)
    let with_cols = d.indexes.iter().any(|i| !i.columns.is_empty());
    let idx = d.indexes.iter().map(|i| {
        let kind = if i.is_primary { "Primary" } else if i.is_unique { "Unique" } else { "" }.to_string();
        if with_cols { vec![i.name.clone(), i.columns.join(", "), kind, i.definition.clone()] } else { vec![i.name.clone(), kind, i.definition.clone()] }
    });
    let head: &[&str] = if with_cols { &["Name", "Columns", "Kind", "Definition"] } else { &["Name", "Kind", "Definition"] };
    section(&mut out, "Indexes", Some(idx.collect()), head, width);
    let out_fk = |f: &ForeignKeyMeta| vec![f.name.clone(), f.columns.join(", "), side(f, false), f.on_update.clone(), f.on_delete.clone()];
    section(&mut out, "Foreign keys", s.foreign_keys.as_ref().map(|l| l.iter().map(out_fk).collect()), &["Name", "Columns", "References", "On update", "On delete"], width);
    let in_fk = |f: &ForeignKeyMeta| vec![f.name.clone(), side(f, true), f.ref_columns.join(", "), f.on_update.clone(), f.on_delete.clone()];
    section(&mut out, "Referenced by", s.referenced_by.as_ref().map(|l| l.iter().map(in_fk).collect()), &["Name", "From", "Columns", "On update", "On delete"], width);
    let trg = s.triggers.as_ref().map(|l| l.iter().map(|t| vec![t.name.clone(), t.timing.clone(), t.events.clone(), t.definition.clone()]).collect());
    section(&mut out, "Triggers", trg, &["Name", "Timing", "Events", "Definition"], width);
    let opts = s.options.as_ref().map(|o| o.iter().map(|(k, v)| vec![k.clone(), v.clone()]).chain(s.comment.iter().map(|c| vec!["Comment".into(), c.clone()])).collect::<Vec<_>>());
    // No count: options are name · value pairs, not things in the table
    out.push(crate::kit::section("Options and comment"));
    match opts {
        Some(rows) if !rows.is_empty() => out.extend(table_lines(&["Option", "Value"], &rows, width)),
        rows => out.push(note(rows.is_some())),
    }
    out
}

/// A titled section with its row count: the table, "None", or "Not available" when the source doesn't report it (rows None)
fn section(out: &mut Vec<Line<'static>>, title: &str, rows: Option<Vec<Vec<String>>>, head: &[&str], width: usize) {
    match rows {
        Some(rows) => {
            out.push(crate::kit::section(format!("{title} {}", rows.len())));
            if rows.is_empty() {
                out.push(note(true));
            } else {
                out.extend(table_lines(head, &rows, width));
            }
        }
        None => {
            out.push(crate::kit::section(title));
            out.push(note(false));
        }
    }
    out.push(Line::from(""));
}

/// The line under an empty section: None, or not available for this kind of data source
fn note(available: bool) -> Line<'static> {
    Line::styled(if available { "  None" } else { "  Not available for this data source" }, Style::default().fg(DIM))
}

fn yes(b: bool) -> String {
    if b { "yes" } else { "no" }.into()
}

/// One end of a foreign key as schema.table (columns): from = the referencing table
fn side(f: &ForeignKeyMeta, from: bool) -> String {
    if from { format!("{}.{} ({})", f.schema, f.table, f.columns.join(", ")) } else { format!("{}.{} ({})", f.ref_schema, f.ref_table, f.ref_columns.join(", ")) }
}

/// What a text tab shows in `width` cells: a table's Structure view when switched to it, else the text
pub fn body(t: &crate::app::TextTab, width: usize) -> Text<'_> {
    match &t.table {
        Some((s, n)) if t.structure_view => lines(s, n, t.structure.as_ref(), width).into(),
        _ => t.text.as_str().into(),
    }
}

/// A table with every section (tests · the Storybook scene); triggers left None as a driver plugin would
#[cfg(test)]
pub fn sample() -> TableStructure {
    use qurry_core::meta::{ColumnMeta, ConstraintMeta, IndexMeta, TableDetail};
    let col = |name: &str, ty: &str, nullable: bool, default: Option<&str>, comment: Option<&str>, pk: bool| ColumnMeta { name: name.into(), data_type: ty.into(), nullable, default_value: default.map(String::from), comment: comment.map(String::from), is_pk: pk };
    let fk = |name: &str, table: &str, cols: &str, ref_table: &str, del: &str| ForeignKeyMeta { name: name.into(), schema: "public".into(), table: table.into(), columns: vec![cols.into()], ref_schema: "public".into(), ref_table: ref_table.into(), ref_columns: vec!["id".into()], on_update: "NO ACTION".into(), on_delete: del.into() };
    TableStructure {
        detail: TableDetail {
            columns: vec![
                col("id", "bigint", false, Some("nextval('users_id_seq'::regclass)"), None, true),
                col("user_name", "character varying(50)", false, None, Some("이름"), false),
                col("contract_id", "bigint", true, None, None, false),
            ],
            indexes: vec![
                IndexMeta { name: "users_pkey".into(), definition: "CREATE UNIQUE INDEX users_pkey ON public.users USING btree (id)".into(), is_unique: true, is_primary: true, columns: vec!["id".into()] },
                IndexMeta { name: "users_name_idx".into(), definition: "CREATE INDEX users_name_idx ON public.users USING btree (user_name, id)".into(), is_unique: false, is_primary: false, columns: vec!["user_name".into(), "id".into()] },
            ],
            constraints: vec![
                ConstraintMeta { name: "users_pkey".into(), kind: "p".into(), definition: "PRIMARY KEY (id)".into() },
                ConstraintMeta { name: "users_contract_fk".into(), kind: "f".into(), definition: "FOREIGN KEY (contract_id) REFERENCES contracts(id) ON DELETE SET NULL".into() },
            ],
        },
        foreign_keys: Some(vec![fk("users_contract_fk", "users", "contract_id", "contracts", "SET NULL")]),
        referenced_by: Some(vec![fk("orders_user_fk", "orders", "user_id", "users", "CASCADE")]),
        triggers: None,
        options: Some(vec![("Owner".into(), "app".into()), ("Tablespace".into(), "pg_default".into())]),
        comment: Some("Members of the shop".into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{Focus, TabKind, TextTab};
    use crate::testkit::Harness;

    fn ddl_tab(h: &mut Harness, structure: Option<Result<qurry_core::meta::TableStructure, String>>) {
        let text = "CREATE TABLE public.users (\n    id bigint NOT NULL\n);".to_string();
        let table = Some(("public".to_string(), "users".to_string()));
        h.app.push_tab("test", "postgres".into(), "users [DDL]".into(), TabKind::Text(TextTab { text, table, structure, ..Default::default() }));
        h.app.focus = Focus::Grid;
    }

    #[tokio::test]
    async fn v_switches_a_table_ddl_to_its_structure_and_the_choice_is_kept() {
        let mut h = Harness::new().await;
        ddl_tab(&mut h, Some(Ok(super::sample())));
        let sql = h.screen(130, 50);
        assert!(sql.contains("CREATE TABLE public.users") && sql.contains("v Structure"), "{sql}");
        h.key("v");
        assert!(h.app.ddl_structure, "the next DDL tab opens in Structure");
        let s = h.screen(130, 50);
        for want in ["Structure · v SQL", "Columns 3", "id (PK)", "nextval('users_id_seq'::regclass)", "이 름", "Keys and constraints 2", "Primary key", "Indexes 2", "user_name, id", "Foreign keys 1", "public.contracts (id)", "SET NULL", "Referenced by 1", "public.orders (user_id)", "CASCADE", "Owner", "Members of the shop"] {
            assert!(s.contains(want), "{want}\n{s}");
        }
        // (the screen text has a wide character's second cell as a space: "이 름")
        // Columns line up: the type starts in the same cell on each row
        let at = |name: &str, ty: &str| s.lines().find(|l| l.contains(name) && l.contains(ty)).map(|l| unicode_width::UnicodeWidthStr::width(&l[..l.find(ty).unwrap()])).unwrap();
        assert_eq!(at("id (PK)", "bigint"), at("contract_id", "bigint"));
        // Triggers: the source didn't report them
        let trig = s.lines().position(|l| l.contains("Triggers")).unwrap();
        assert!(s.lines().nth(trig + 1).unwrap().contains("Not available for this data source"), "{s}");
        h.key("v");
        assert!(!h.app.ddl_structure);
        assert!(h.screen(130, 50).contains("CREATE TABLE public.users"));
    }

    /// At 120 columns (the explorer beside it) long definitions and comments wrap under their column instead of being cut,
    /// and y copies the SQL as the GUI's Copy does
    #[tokio::test]
    async fn long_cells_wrap_and_y_copies_the_sql() {
        let mut h = Harness::new().await;
        ddl_tab(&mut h, Some(Ok(super::sample())));
        h.key("v");
        let s = h.screen(120, 40);
        for want in ["Comment", "이 름", "REFERENCES contracts(id) ON", "DELETE SET NULL", "USING btree (user_name, id)"] {
            assert!(s.lines().any(|l| l.contains(want)), "{want}\n{s}");
        }
        h.key("y");
        assert_eq!(h.app.status, "Copied 3 lines");
    }

    /// A driver plugin that doesn't report index columns: no empty Columns column
    #[tokio::test]
    async fn indexes_without_columns_leave_the_column_out() {
        let mut h = Harness::new().await;
        let mut st = super::sample();
        st.detail.indexes.iter_mut().for_each(|i| i.columns.clear());
        ddl_tab(&mut h, Some(Ok(st)));
        h.key("v");
        let s = h.screen(130, 50);
        let head = s.lines().skip_while(|l| !l.contains("Indexes 2")).nth(1).unwrap();
        assert!(head.contains("Kind") && !head.contains("Columns"), "{s}");
    }

    #[tokio::test]
    async fn a_failed_structure_says_how_to_get_back_and_other_text_tabs_have_none() {
        let mut h = Harness::new().await;
        ddl_tab(&mut h, Some(Err("connection reset by peer".into())));
        h.key("v");
        let s = h.screen(130, 30);
        assert!(s.contains("Couldn't load the structure of public.users: connection reset by peer"), "{s}");
        assert!(s.contains("r retry · v the SQL view"), "{s}");
        // A diagram or a view's DDL has no Structure view
        h.app.push_tab("test", "postgres".into(), "v_users [DDL]".into(), TabKind::Text(TextTab { text: "CREATE VIEW v_users AS SELECT 1;".into(), ..Default::default() }));
        h.app.focus = Focus::Grid;
        h.key("v");
        assert_eq!(h.app.status, "Only a table's DDL has a Structure view");
        assert!(!h.screen(130, 30).contains("v Structure"));
    }
}
