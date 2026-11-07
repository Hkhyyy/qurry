//! Whether a console result can be edited like a table tab, and in which tables (#86, joins #102). The statement check and the
//! rules are qurry_shared::result_edit; this asks the server where the columns come from. The catalog lookups run on the pool,
//! not the console's session: they never touch its transaction or open cursor, and cancelling the console's query can't hit them.
//! Only MySQL's temporary-table check runs on the session, the only one that sees its temporary tables.
use crate::db::{ColumnInfo, Pool};
use crate::session::Session;
use crate::store::DbKind;
use qurry_shared::errors as e;
use qurry_shared::result_edit::{self as re, Shape, Source, TableInfo};
pub use qurry_shared::result_edit::{ColumnEdit, EditTarget, Editable, JoinEdit, JoinTable};

/// The edit targets of the result `s` just returned for `sql`, or why it has none. None: nothing to say (no columns, Redis)
/// or it couldn't be checked (a catalog lookup failed, a temporary table): the GUI and TUI then show their generic text
pub(crate) async fn check(pool: &Pool, s: &mut Session, sql: &str, columns: &[ColumnInfo]) -> Option<Editable> {
    let kind = match s {
        _ if columns.is_empty() => return None,
        Session::Pg(_) => DbKind::Postgres,
        Session::My(_) => DbKind::Mysql,
        Session::Plugin(_) => return Some(Editable::ReadOnly(e::EDIT_DRIVER.into())),
        Session::Redis(_) => return None,
    };
    let shape = match re::select_shape(sql, kind) {
        Ok(x) => x,
        Err(why) => return Some(Editable::ReadOnly(why)),
    };
    let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    match (pool, s) {
        (Pool::Pg(p), Session::Pg(ps)) => bounded(pg(p, &ps.origins, &names, &shape)).await,
        (Pool::My(p), Session::My(ms)) => bounded(mysql(p, ms, &shape, &names)).await,
        _ => None,
    }
}

/// A pool with every connection taken (one per console tab) would hold the result back for its whole acquire timeout
async fn bounded(f: impl Future<Output = Option<Editable>>) -> Option<Editable> {
    tokio::time::timeout(std::time::Duration::from_secs(3), f).await.ok().flatten()
}

/// The tables' OID, schema, name, relkind, persistence, column numbers and names (table order), primary key columns
const PG_TABLES: &str = "select c.oid::int8, n.nspname::text, c.relname::text, c.relkind::text, c.relpersistence::text, \
       array(select a.attnum from pg_catalog.pg_attribute a where a.attrelid = c.oid and a.attnum > 0 and not a.attisdropped order by a.attnum), \
       array(select a.attname::text from pg_catalog.pg_attribute a where a.attrelid = c.oid and a.attnum > 0 and not a.attisdropped order by a.attnum), \
       array(select a.attname::text from pg_catalog.pg_constraint k join pg_catalog.pg_attribute a on a.attrelid = k.conrelid and a.attnum = any(k.conkey) \
             where k.conrelid = c.oid and k.contype = 'p' order by a.attnum) \
     from pg_catalog.pg_class c join pg_catalog.pg_namespace n on n.oid = c.relnamespace where c.oid = any($1)";

/// A table the result's columns come from, as PostgreSQL's catalog has it
struct PgRel {
    oid: u32,
    schema: String,
    table: String,
    relkind: String,
    temporary: bool,
    nums: Vec<i16>,
    cols: Vec<String>,
    pk: Vec<String>,
}

impl PgRel {
    /// The table's info, or why it can't be edited: a temporary table exists in the console's session only, so the edits,
    /// saved on another connection, can't reach it
    fn info(&self) -> Result<TableInfo, String> {
        if self.temporary {
            return Err(e::edit_temporary(&self.table));
        }
        let not_table = match self.relkind.as_str() {
            "r" | "p" => None,
            "v" => Some("view"),
            "m" => Some("materialized view"),
            "f" => Some("foreign table"),
            _ => Some("relation"),
        };
        Ok(TableInfo { schema: self.schema.clone(), table: self.table.clone(), not_table, columns: self.cols.clone(), pk: self.pk.clone() })
    }

    /// The column with attribute number `a`. None: a system column (ctid), which counts as computed
    fn column(&self, a: i16) -> Option<&String> {
        self.nums.iter().position(|n| *n == a).map(|i| &self.cols[i])
    }
}

async fn pg_rels(p: &sqlx::PgPool, oids: Vec<u32>) -> Option<Vec<PgRel>> {
    type Row = (i64, String, String, String, String, Vec<i16>, Vec<String>, Vec<String>);
    let oids: Vec<sqlx::postgres::types::Oid> = oids.into_iter().map(sqlx::postgres::types::Oid).collect();
    let rows: Vec<Row> = sqlx::query_as(PG_TABLES).bind(oids).persistent(false).fetch_all(p).await.ok()?;
    Some(rows.into_iter().map(|(oid, schema, table, relkind, persistence, nums, cols, pk)| PgRel { oid: oid as u32, schema, table, relkind, temporary: persistence == "t", nums, cols, pk }).collect())
}

/// PostgreSQL reports each column's table OID and attribute number (none for an expression)
async fn pg(p: &sqlx::PgPool, origins: &[Option<(u32, i16)>], names: &[String], shape: &Shape) -> Option<Editable> {
    if origins.len() != names.len() {
        return None;
    }
    if shape.from.len() > 1 {
        return pg_join(p, origins, names, shape).await;
    }
    if let Some(i) = origins.iter().position(Option::is_none) {
        return Some(Editable::ReadOnly(e::edit_computed(&names[i])));
    }
    let oid = origins[0]?.0;
    if origins.iter().flatten().any(|o| o.0 != oid) {
        return Some(Editable::ReadOnly(e::EDIT_MANY_TABLES.into()));
    }
    let rel = pg_rels(p, vec![oid]).await?.pop()?;
    let t = match rel.info() {
        Ok(t) => t,
        Err(why) => return Some(Editable::ReadOnly(why)),
    };
    let sources = origins.iter().flatten().map(|(_, a)| rel.column(*a).cloned()).collect::<Vec<_>>();
    Some(re::decide(&t, names, &sources))
}

/// A join: each column's table by its origin. A table in FROM more than once (a self-join) has one OID for all its occurrences,
/// so there the select list says which occurrence a column is (result_edit::align), and the origin must agree with it
async fn pg_join(p: &sqlx::PgPool, origins: &[Option<(u32, i16)>], names: &[String], shape: &Shape) -> Option<Editable> {
    let mut oids: Vec<u32> = origins.iter().flatten().map(|o| o.0).collect();
    oids.sort_unstable();
    oids.dedup();
    let rels = pg_rels(p, oids).await?;
    // Each table in FROM: the one relation of its name (and schema, when it names one) among the result's. None: none, or more
    // than one (`users` while crm.users and public.users are both in the result)
    let of: Vec<Option<&PgRel>> = shape
        .from
        .iter()
        .map(|f| {
            let hits: Vec<&PgRel> = rels.iter().filter(|r| r.table == f.table && f.schema.as_ref().is_none_or(|s| *s == r.schema)).collect();
            (hits.len() == 1).then(|| hits[0])
        })
        .collect();
    let tables: Vec<Result<TableInfo, String>> = of.iter().map(|r| r.map_or_else(|| Err(e::EDIT_UNMATCHED.to_string()), PgRel::info)).collect();
    let cols: Vec<Option<Vec<String>>> = of.iter().map(|r| r.map(|r| r.cols.clone())).collect();
    let aligned = re::align(shape, &cols, DbKind::Postgres, names.len());
    let sources = origins
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let Some((oid, att)) = *o else { return Source::Computed };
            let Some(rel) = rels.iter().find(|r| r.oid == oid) else { return Source::Unknown(e::EDIT_UNMATCHED.into()) };
            let Some(column) = rel.column(att).cloned() else { return Source::Computed };
            let ks: Vec<usize> = (0..of.len()).filter(|&k| of[k].is_some_and(|r| r.oid == oid)).collect();
            match ks[..] {
                [] => Source::Unknown(e::EDIT_UNMATCHED.into()),
                [from] => Source::Col { from, column },
                // A self-join: the occurrence the select list puts here, when it is one of them and the same column
                _ => match aligned.as_ref().map(|a| &a[i].0) {
                    Some(Source::Col { from, column: c }) if ks.contains(from) && *c == column => Source::Col { from: *from, column },
                    _ => Source::Unknown(e::edit_self_join(&format!("{}.{}", rel.schema, rel.table))),
                },
            }
        })
        .collect::<Vec<_>>();
    Some(re::decide_join(&tables, names, &sources))
}

/// MySQL: the table's info from information_schema (None: not found, dropped since)
async fn my_table(p: &sqlx::MySqlPool, db: &str, table: &str) -> Option<TableInfo> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "select cast(t.table_schema as char), cast(t.table_name as char), cast(t.table_type as char), cast(c.column_name as char), cast(c.column_key as char) \
         from information_schema.tables t join information_schema.columns c on c.table_schema = t.table_schema and c.table_name = t.table_name \
         where t.table_schema = ? and t.table_name = ? order by c.ordinal_position",
    )
    .bind(db)
    .bind(table)
    .fetch_all(p)
    .await
    .ok()?;
    let first = rows.first()?;
    let not_table = match first.2.as_str() {
        "BASE TABLE" => None,
        "VIEW" => Some("view"),
        _ => Some("system view"),
    };
    Some(TableInfo {
        schema: first.0.clone(),
        table: first.1.clone(),
        not_table,
        columns: rows.iter().map(|r| r.3.clone()).collect(),
        pk: rows.iter().filter(|r| r.4 == "PRI").map(|r| r.3.clone()).collect(),
    })
}

/// MySQL: one table of FROM, or why its columns can't be edited. None: it couldn't be checked. A temporary table (asked on the
/// console's session, the only one that sees it) may hide a base table of the same name: the edits, saved on another
/// connection, would change that base table's rows
async fn my_from(p: &sqlx::MySqlPool, ms: &mut crate::mysql::MySession, f: &re::FromTable) -> Option<Result<TableInfo, String>> {
    // The table is in the console's database unless the statement names one (USE changes it per session)
    let db = match &f.schema {
        Some(d) => d.clone(),
        None => ms.database().await.filter(|d| !d.is_empty())?,
    };
    if ms.is_temporary(&db, &f.table).await? {
        return Some(Err(e::edit_temporary(&format!("{db}.{}", f.table))));
    }
    my_table(p, &db, &f.table).await.map(Ok)
}

/// MySQL: sqlx doesn't pass on the column definitions' original table and column, so the select list is lined up with the
/// tables' columns (result_edit::mysql_sources for one table, result_edit::align for a join), and the server's names must agree
async fn mysql(p: &sqlx::MySqlPool, ms: &mut crate::mysql::MySession, shape: &Shape, names: &[String]) -> Option<Editable> {
    if shape.from.len() == 1 {
        let t = match my_from(p, ms, &shape.from[0]).await? {
            Ok(t) => t,
            Err(why) => return Some(Editable::ReadOnly(why)),
        };
        return Some(match re::mysql_sources(&shape.items, names, &t) {
            Ok(sources) => re::decide(&t, names, &sources),
            Err(why) => Editable::ReadOnly(why),
        });
    }
    let mut tables = Vec::new();
    for f in &shape.from {
        tables.push(my_from(p, ms, f).await.unwrap_or_else(|| Err(e::EDIT_UNMATCHED.into())));
    }
    let cols: Vec<Option<Vec<String>>> = tables.iter().map(|t| t.as_ref().ok().map(|t| t.columns.clone())).collect();
    let Some(aligned) = re::align(shape, &cols, DbKind::Mysql, names.len()) else {
        return Some(Editable::ReadOnly(if shape.using { e::EDIT_STAR_USING.into() } else { e::EDIT_UNMATCHED.into() }));
    };
    // The server's names must be the ones read here, or the two disagree on what the statement selects
    if aligned.iter().zip(names).any(|((_, want), got)| want.as_ref().is_some_and(|w| !w.eq_ignore_ascii_case(got))) {
        return Some(Editable::ReadOnly(e::EDIT_UNMATCHED.into()));
    }
    let sources: Vec<Source> = aligned.into_iter().map(|(s, _)| s).collect();
    Some(re::decide_join(&tables, names, &sources))
}
