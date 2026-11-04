//! Whether a console result can be edited like a table tab, and in which table (#86). The statement check and the rules are
//! qurry_shared::result_edit; this asks the server where the columns come from. The catalog lookups run on the pool, not the
//! console's session: they never touch its transaction or open cursor, and cancelling the console's query can't hit them.
use crate::db::{ColumnInfo, Pool};
use crate::session::Session;
use crate::store::DbKind;
use qurry_shared::errors as e;
use qurry_shared::result_edit::{self as re, OneTable, TableInfo};
pub use qurry_shared::result_edit::{EditTarget, Editable};

/// The edit target of the result `s` just returned for `sql`, or why it has none. None: nothing to say (no columns, Redis)
/// or it couldn't be checked (a catalog lookup failed, a temporary table): the GUI and TUI then show their generic text
pub(crate) async fn check(pool: &Pool, s: &mut Session, sql: &str, columns: &[ColumnInfo]) -> Option<Editable> {
    let kind = match s {
        _ if columns.is_empty() => return None,
        Session::Pg(_) => DbKind::Postgres,
        Session::My(_) => DbKind::Mysql,
        Session::Plugin(_) => return Some(Editable::ReadOnly(e::EDIT_DRIVER.into())),
        Session::Redis(_) => return None,
    };
    let shape = match re::one_table_select(sql, kind) {
        Ok(x) => x,
        Err(why) => return Some(Editable::ReadOnly(why)),
    };
    let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    match (pool, s) {
        (Pool::Pg(p), Session::Pg(ps)) => bounded(pg(p, &ps.origins, &names)).await,
        (Pool::My(p), Session::My(ms)) => bounded(mysql(p, ms, shape, &names)).await,
        _ => None,
    }
}

/// A pool with every connection taken (one per console tab) would hold the result back for its whole acquire timeout
async fn bounded(f: impl Future<Output = Option<Editable>>) -> Option<Editable> {
    tokio::time::timeout(std::time::Duration::from_secs(3), f).await.ok().flatten()
}

/// The table's schema, name, relkind, persistence, column numbers and names (table order), primary key columns
const PG_TABLE: &str = "select n.nspname::text, c.relname::text, c.relkind::text, c.relpersistence::text, \
       array(select a.attnum from pg_catalog.pg_attribute a where a.attrelid = c.oid and a.attnum > 0 and not a.attisdropped order by a.attnum), \
       array(select a.attname::text from pg_catalog.pg_attribute a where a.attrelid = c.oid and a.attnum > 0 and not a.attisdropped order by a.attnum), \
       array(select a.attname::text from pg_catalog.pg_constraint k join pg_catalog.pg_attribute a on a.attrelid = k.conrelid and a.attnum = any(k.conkey) \
             where k.conrelid = c.oid and k.contype = 'p' order by a.attnum) \
     from pg_catalog.pg_class c join pg_catalog.pg_namespace n on n.oid = c.relnamespace where c.oid = $1";

/// PostgreSQL reports each column's table OID and attribute number (none for an expression)
async fn pg(p: &sqlx::PgPool, origins: &[Option<(u32, i16)>], names: &[String]) -> Option<Editable> {
    if origins.len() != names.len() {
        return None;
    }
    if let Some(i) = origins.iter().position(Option::is_none) {
        return Some(Editable::ReadOnly(e::edit_computed(&names[i])));
    }
    let oid = origins[0]?.0;
    if origins.iter().flatten().any(|o| o.0 != oid) {
        return Some(Editable::ReadOnly(e::EDIT_MANY_TABLES.into()));
    }
    type Row = (String, String, String, String, Vec<i16>, Vec<String>, Vec<String>);
    let (schema, table, relkind, persistence, nums, cols, pk): Row = sqlx::query_as(PG_TABLE).bind(sqlx::postgres::types::Oid(oid)).persistent(false).fetch_optional(p).await.ok()??;
    // A temporary table exists in the console's session only: the edits, saved on another connection, can't reach it
    if persistence == "t" {
        return Some(Editable::ReadOnly(e::edit_temporary(&table)));
    }
    let not_table = match relkind.as_str() {
        "r" | "p" => None,
        "v" => Some("view"),
        "m" => Some("materialized view"),
        "f" => Some("foreign table"),
        _ => Some("relation"),
    };
    // A system column (ctid) has no number among the table's columns: it counts as computed
    let sources = origins.iter().flatten().map(|(_, a)| nums.iter().position(|n| n == a).map(|i| cols[i].clone())).collect::<Vec<_>>();
    Some(re::decide(&TableInfo { schema, table, not_table, columns: cols, pk }, names, &sources))
}

/// MySQL: sqlx doesn't pass on the column definitions' original table and column, so the select list is lined up with the table's
/// columns (result_edit::mysql_sources)
async fn mysql(p: &sqlx::MySqlPool, ms: &mut crate::mysql::MySession, shape: OneTable, names: &[String]) -> Option<Editable> {
    // The table is in the console's database unless the statement names one (USE changes it per session)
    let db = match &shape.schema {
        Some(d) => d.clone(),
        None => ms.database().await.filter(|d| !d.is_empty())?,
    };
    // A temporary table (asked on the console's session, the only one that sees it) may hide a base table of the same name:
    // the edits, saved on another connection, would change that base table's rows
    if ms.is_temporary(&db, &shape.table).await? {
        return Some(Editable::ReadOnly(e::edit_temporary(&format!("{db}.{}", shape.table))));
    }
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "select cast(t.table_schema as char), cast(t.table_name as char), cast(t.table_type as char), cast(c.column_name as char), cast(c.column_key as char) \
         from information_schema.tables t join information_schema.columns c on c.table_schema = t.table_schema and c.table_name = t.table_name \
         where t.table_schema = ? and t.table_name = ? order by c.ordinal_position",
    )
    .bind(&db)
    .bind(&shape.table)
    .fetch_all(p)
    .await
    .ok()?;
    // Not found: dropped since (a temporary table was caught before)
    let first = rows.first()?;
    let not_table = match first.2.as_str() {
        "BASE TABLE" => None,
        "VIEW" => Some("view"),
        _ => Some("system view"),
    };
    let t = TableInfo {
        schema: first.0.clone(),
        table: first.1.clone(),
        not_table,
        columns: rows.iter().map(|r| r.3.clone()).collect(),
        pk: rows.iter().filter(|r| r.4 == "PRI").map(|r| r.3.clone()).collect(),
    };
    Some(match re::mysql_sources(&shape.items, names, &t) {
        Ok(sources) => re::decide(&t, names, &sources),
        Err(why) => Editable::ReadOnly(why),
    })
}
