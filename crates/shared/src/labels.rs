//! Tab titles that tell table tabs with the same name apart (DataGrip).
use serde::Deserialize;

/// A table tab as the title rule sees it
#[derive(Debug, Clone, Deserialize)]
pub struct TableTab {
    pub id: String,
    pub db: String,
    pub schema: String,
    pub table: String,
}

/// schema.table if another tab has the same table name, db.schema.table if the schema matches too; None if the name is unique
pub fn label(t: &TableTab, all: &[TableTab]) -> Option<String> {
    let mut same = all.iter().filter(|x| x.id != t.id && x.table == t.table).peekable();
    same.peek()?;
    Some(if same.any(|x| x.schema == t.schema) { format!("{}.{}.{}", t.db, t.schema, t.table) } else { format!("{}.{}", t.schema, t.table) })
}

/// label for every tab, in order
pub fn table_tab_labels(tabs: &[TableTab]) -> Vec<Option<String>> {
    tabs.iter().map(|t| label(t, tabs)).collect()
}
