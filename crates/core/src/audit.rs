//! Production audit log: changing statements run against a Production Data Source · affected rows · time (shared by GUI and TUI).
//! Undo SQL: for simple UPDATE / DELETE (guard::simple_dml), snapshot the rows about to change right before running (at most UNDO_MAX_ROWS rows)
//! and record SQL that reverts them. It never runs on its own — open it in a console and run it (Production confirmation still applies).
//! Log: config/audit.jsonl (one entry per line, 0600). Passwords are masked with history::redact
use crate::db::{Pool, QueryResult};
use crate::error::AppResult;
use crate::guard::SimpleDml;
use crate::session::Session;
use crate::store::DbKind;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Changing more rows than this records no undo SQL
pub const UNDO_MAX_ROWS: usize = 1000;
/// Skip when the snapshot rows are larger than this (keeps the log file from growing too large)
const UNDO_MAX_BYTES: usize = 1 << 20;
/// Number of entries to keep
const MAX_ENTRIES: usize = 2000;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    /// Milliseconds (Unix)
    pub ts: i64,
    pub conn_id: String,
    pub conn_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub sql: String,
    /// Rows changed (none on error)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Not yet committed when run (Manual mode · explicit BEGIN). A separate COMMIT / ROLLBACK entry follows
    #[serde(default)]
    pub in_tx: bool,
    /// SQL that reverts the change
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    /// Why undo SQL could not be built
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub struct Audit {
    dir: PathBuf,
    /// Consoles with a logged change not yet committed (their COMMIT / ROLLBACK is logged too)
    pending: Mutex<HashSet<String>>,
    write: Mutex<()>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// First word (skips leading comments)
fn first_word(sql: &str) -> String {
    crate::session::strip_comments_lead(sql).split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("").to_string()
}

/// Whether the statement should be logged: it changes data or structure (not reads · session settings · BEGIN · EXPLAIN)
pub fn is_change(sql: &str, kind: DbKind) -> bool {
    if kind == DbKind::Redis {
        let Ok(args) = crate::redisdb::parse_command(sql) else { return false };
        let arg = |i: usize| args.get(i).map(|a| String::from_utf8_lossy(a).to_uppercase()).unwrap_or_default();
        return !args.is_empty() && !crate::redisdb::is_read_command(&arg(0), &arg(1));
    }
    if crate::guard::is_read(sql, kind) {
        return false;
    }
    !matches!(first_word(sql).as_str(), "" | "set" | "use" | "begin" | "start" | "savepoint" | "release" | "commit" | "end" | "rollback" | "abort" | "explain" | "show" | "describe" | "desc")
}

/// Name to log if the statement is COMMIT / ROLLBACK (not ROLLBACK TO SAVEPOINT)
fn tx_end(sql: &str) -> Option<&'static str> {
    let s = crate::session::strip_comments_lead(sql);
    match first_word(sql).as_str() {
        "commit" | "end" => Some("COMMIT"),
        "rollback" | "abort" if !s.starts_with("rollback to") => Some("ROLLBACK"),
        _ => None,
    }
}

impl Audit {
    pub fn new(dir: &Path) -> Self {
        Audit { dir: dir.to_path_buf(), pending: Default::default(), write: Default::default() }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("audit.jsonl")
    }

    fn read(&self) -> Vec<AuditEntry> {
        let Ok(s) = std::fs::read_to_string(self.file()) else { return vec![] };
        s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    pub fn add(&self, mut e: AuditEntry) -> AppResult<()> {
        use std::io::Write;
        e.sql = crate::history::redact(e.sql.trim());
        // Keep at most 64 KB per statement (don't store a huge INSERT whole)
        if e.sql.len() > 64 * 1024 {
            let mut cut = 64 * 1024;
            while !e.sql.is_char_boundary(cut) {
                cut -= 1;
            }
            e.sql.truncate(cut);
            e.sql.push_str("\n-- … (cut)");
        }
        let _w = self.write.lock().unwrap();
        let mut line = serde_json::to_string(&e)?;
        line.push('\n');
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(self.file())?.write_all(line.as_bytes())?;
        // If it grew too long, keep only the most recent entries
        let n = std::fs::read(self.file()).map(|b| b.iter().filter(|&&c| c == b'\n').count()).unwrap_or(0);
        if n > MAX_ENTRIES + MAX_ENTRIES / 10 {
            let all = self.read();
            let mut out = String::new();
            for i in &all[all.len().saturating_sub(MAX_ENTRIES)..] {
                out.push_str(&serde_json::to_string(i)?);
                out.push('\n');
            }
            crate::secrets::write_private(&self.file(), out.as_bytes())?;
        }
        Ok(())
    }

    /// Newest first. conn = only that Data Source (including DB-scoped targets "id::db"), filter = text contained in the statement (case-insensitive)
    pub fn list(&self, conn: Option<&str>, filter: &str, limit: usize) -> Vec<AuditEntry> {
        let f = filter.to_lowercase();
        let mut all = self.read();
        all.sort_by_key(|e| e.ts);
        all.into_iter()
            .rev()
            .filter(|e| conn.is_none_or(|c| crate::app::split_target(&e.conn_id).0 == crate::app::split_target(c).0))
            .filter(|e| f.is_empty() || e.sql.to_lowercase().contains(&f) || e.conn_name.to_lowercase().contains(&f))
            .take(limit)
            .collect()
    }

    /// After running a statement: log it if it changes data, and log COMMIT / ROLLBACK for consoles with an uncommitted logged change
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn after(&self, conn_id: &str, conn_name: &str, kind: DbKind, tab: Option<&str>, sql: &str, r: &AppResult<QueryResult>, undo: Option<Result<String, String>>) {
        let entry = |sql: String| AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            ts: now_ms(),
            conn_id: conn_id.into(),
            conn_name: conn_name.into(),
            database: crate::app::split_target(conn_id).1.map(String::from),
            sql,
            rows: None,
            error: None,
            in_tx: false,
            undo: None,
            note: None,
        };
        if is_change(sql, kind) {
            let mut e = entry(sql.into());
            match r {
                Ok(q) => {
                    e.rows = Some(q.rows_affected);
                    e.in_tx = q.in_transaction;
                    match undo {
                        Some(Ok(u)) if q.rows_affected > 0 => e.undo = Some(u),
                        Some(Err(why)) if q.rows_affected > 0 => e.note = Some(why),
                        _ => {}
                    }
                    if let (true, Some(t)) = (q.in_transaction, tab) {
                        self.pending.lock().unwrap().insert(t.into());
                    }
                }
                Err(err) => e.error = Some(err.to_string()),
            }
            let _ = self.add(e);
        } else if let (Some(word), Some(t)) = (tx_end(sql), tab) {
            if r.is_ok() && self.pending.lock().unwrap().remove(t) {
                let _ = self.add(entry(word.into()));
            }
        }
    }

    /// After running several statements in one transaction, like saving table edits (on failure everything was rolled back). exact_one = 1 row per statement
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn after_batch(&self, conn_id: &str, conn_name: &str, kind: DbKind, statements: &[String], err: Option<String>, exact_one: bool, mut undos: Vec<Option<Result<String, String>>>) {
        undos.resize(statements.len(), None);
        for (st, undo) in statements.iter().zip(undos) {
            if !is_change(st, kind) {
                continue;
            }
            let mut e = AuditEntry {
                id: uuid::Uuid::new_v4().to_string(),
                ts: now_ms(),
                conn_id: conn_id.into(),
                conn_name: conn_name.into(),
                database: crate::app::split_target(conn_id).1.map(String::from),
                sql: st.clone(),
                rows: exact_one.then_some(1),
                error: err.as_ref().map(|e| format!("Rolled back: {e}")),
                in_tx: false,
                undo: None,
                note: None,
            };
            if err.is_none() {
                match undo {
                    Some(Ok(u)) => e.undo = Some(u),
                    Some(Err(why)) => e.note = Some(why),
                    None => {}
                }
            }
            let _ = self.add(e);
        }
    }

    /// The console's Commit / Roll back buttons · rolling back by closing the console
    pub(crate) fn tx_closed(&self, conn_id: &str, conn_name: &str, tab: &str, word: &str) {
        if self.pending.lock().unwrap().remove(tab) {
            let e = AuditEntry {
                id: uuid::Uuid::new_v4().to_string(),
                ts: now_ms(),
                conn_id: conn_id.into(),
                conn_name: conn_name.into(),
                database: crate::app::split_target(conn_id).1.map(String::from),
                sql: word.into(),
                rows: None,
                error: None,
                in_tx: false,
                undo: None,
                note: None,
            };
            let _ = self.add(e);
        }
    }
}

// ───────── Undo SQL ─────────

/// Table column: name · whether PK · whether generated (cannot be inserted) · MySQL binary (snapshotted as HEX)
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Col {
    pub name: String,
    pub pk: bool,
    pub generated: bool,
    pub binary: bool,
}

/// Right before running: snapshot the rows about to change and build SQL that reverts them. sess = console session (sees state inside the transaction), otherwise the pool.
/// If it can't be built, the reason (the statement still runs as is)
pub(crate) async fn prepare(pool: &Pool, sess: Option<&mut Session>, sql: &str, kind: DbKind) -> Result<String, String> {
    if !matches!(kind, DbKind::Postgres | DbKind::Mysql) {
        return Err("Undo is kept for PostgreSQL and MySQL only".into());
    }
    let Some(d) = crate::guard::simple_dml(sql, kind) else {
        return Err("Undo is kept for simple UPDATE / DELETE on one table only".into());
    };
    let cols = table_columns(pool, &d.table, kind).await.map_err(|e| format!("Couldn't read the table's columns: {e}"))?;
    if cols.is_empty() {
        return Err(format!("Couldn't find table {}", d.table));
    }
    let q = snapshot_sql(&d, &cols, kind);
    let r = match sess {
        Some(s) => s.run(&q, UNDO_MAX_ROWS + 1).await,
        None => pool.run_query(&q, UNDO_MAX_ROWS + 1).await,
    }
    .map_err(|e| format!("Couldn't save the rows before the change: {e}"))?;
    let head = format!(
        "-- Undo for: {}\n-- Saved just before it ran. Rows changed again since then are overwritten.\n",
        sql.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect::<String>()
    );
    let body = match kind {
        DbKind::Postgres => {
            let json = r.rows.first().and_then(|x| x.first()).and_then(|v| v.as_str()).unwrap_or("[]").to_string();
            let n = r.rows.first().and_then(|x| x.get(1)).and_then(Value::as_i64).unwrap_or(0) as usize;
            if n > UNDO_MAX_ROWS {
                return Err(format!("More than {UNDO_MAX_ROWS} rows change: no undo kept"));
            }
            if json.len() > UNDO_MAX_BYTES {
                return Err("The rows are too large to keep an undo".into());
            }
            pg_undo(&d, &cols, &json)?
        }
        _ => {
            if r.rows.len() > UNDO_MAX_ROWS {
                return Err(format!("More than {UNDO_MAX_ROWS} rows change: no undo kept"));
            }
            if r.rows.iter().flatten().map(|v| v.as_str().map_or(4, str::len)).sum::<usize>() > UNDO_MAX_BYTES {
                return Err("The rows are too large to keep an undo".into());
            }
            my_undo(&d, &cols, &r.rows)?
        }
    };
    Ok(format!("{head}{body}"))
}

/// MySQL `a`.`b` → ["a", "b"]
fn my_name_parts(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut q) = (Vec::new(), String::new(), false);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '`' if q && it.peek() == Some(&'`') => {
                cur.push('`');
                it.next();
            }
            '`' => q = !q,
            '.' if !q => out.push(std::mem::take(&mut cur)),
            c if !q && c.is_whitespace() => {}
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Table columns (table = the name as written in SQL: public."Order Items" · `shop`.`users`)
pub(crate) async fn table_columns(pool: &Pool, table: &str, kind: DbKind) -> AppResult<Vec<Col>> {
    Ok(match (pool, kind) {
        (Pool::Pg(p), _) => {
            let rows: Vec<(String, bool, bool)> = sqlx::query_as(
                "SELECT a.attname::text, coalesce(a.attnum = ANY(i.indkey), false), a.attgenerated <> '' \
                 FROM pg_attribute a LEFT JOIN pg_index i ON i.indrelid = a.attrelid AND i.indisprimary \
                 WHERE a.attrelid = $1::regclass AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
            )
            .bind(table)
            .fetch_all(p)
            .await?;
            rows.into_iter().map(|(name, pk, generated)| Col { name, pk, generated, binary: false }).collect()
        }
        (Pool::My(p), _) => {
            let parts = my_name_parts(table);
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
            .fetch_all(p)
            .await?;
            let bin = |t: &str| t.contains("blob") || t.contains("binary") || t == "bit" || ["geometry", "point", "linestring", "polygon", "multipoint", "multilinestring", "multipolygon", "geometrycollection"].contains(&t);
            rows.into_iter().map(|(name, t, pk, generated)| Col { name, pk: pk != 0, generated: generated != 0, binary: bin(&t.to_lowercase()) }).collect()
        }
        _ => vec![],
    })
}

/// SELECT that snapshots the rows about to change. PG: rows as one JSON blob (no value loss — unpacked with jsonb_populate_recordset when undoing).
/// MySQL: as text (binary as HEX)
fn snapshot_sql(d: &SimpleDml, cols: &[Col], kind: DbKind) -> String {
    let cond = d.cond.as_ref().map(|c| format!(" WHERE {c}")).unwrap_or_default();
    match kind {
        DbKind::Postgres => format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(qurry_x)), '[]'::jsonb)::text, count(*) FROM (SELECT * FROM {}{cond} LIMIT {}) qurry_x",
            d.from,
            UNDO_MAX_ROWS + 1
        ),
        _ => {
            let exprs: Vec<String> = cols
                .iter()
                .map(|c| {
                    let q = crate::sql::quote_ident(kind, &c.name);
                    if c.binary { format!("HEX({q})") } else { format!("CAST({q} AS CHAR)") }
                })
                .collect();
            format!("SELECT {} FROM {}{cond} LIMIT {}", exprs.join(", "), d.from, UNDO_MAX_ROWS + 1)
        }
    }
}

/// Columns to set when undoing: the columns the UPDATE changed (if unknown, all except PK and generated columns). Changing the PK can't be undone
fn restore_cols<'a>(d: &SimpleDml, cols: &'a [Col], kind: DbKind) -> Result<Vec<&'a Col>, String> {
    if !cols.iter().any(|c| c.pk) {
        return Err(format!("{} has no primary key: no undo kept", d.table));
    }
    let same = |a: &str, b: &str| if kind == DbKind::Mysql { a.eq_ignore_ascii_case(b) } else { a == b };
    if d.set_cols.iter().any(|s| cols.iter().any(|c| c.pk && same(&c.name, s))) {
        return Err("The statement changes the primary key: no undo kept".into());
    }
    let set: Vec<&Col> = cols.iter().filter(|c| !c.pk && !c.generated && d.set_cols.iter().any(|s| same(&c.name, s))).collect();
    Ok(if set.is_empty() { cols.iter().filter(|c| !c.pk && !c.generated).collect() } else { set })
}

/// PG string literal: $qurry$…$qurry$ ('…' if the text contains that tag)
pub(crate) fn pg_text(s: &str) -> String {
    if s.contains("$qurry$") { format!("'{}'", s.replace('\'', "''")) } else { format!("$qurry${s}$qurry$") }
}

fn pg_undo(d: &SimpleDml, cols: &[Col], json: &str) -> Result<String, String> {
    let q = |s: &str| crate::sql::quote_ident(DbKind::Postgres, s);
    let rows = format!("jsonb_populate_recordset(NULL::{}, {})", d.table, pg_text(json));
    if d.delete {
        let names: Vec<String> = cols.iter().filter(|c| !c.generated).map(|c| q(&c.name)).collect();
        let list = names.join(", ");
        return Ok(format!("INSERT INTO {} ({list}) OVERRIDING SYSTEM VALUE\nSELECT {list} FROM {rows};\n", d.table));
    }
    let set: Vec<String> = restore_cols(d, cols, DbKind::Postgres)?.iter().map(|c| format!("{0} = qurry_old.{0}", q(&c.name))).collect();
    let on: Vec<String> = cols.iter().filter(|c| c.pk).map(|c| format!("qurry_t.{0} = qurry_old.{0}", q(&c.name))).collect();
    Ok(format!("UPDATE {} AS qurry_t SET {}\nFROM {rows} AS qurry_old\nWHERE {};\n", d.table, set.join(", "), on.join(" AND ")))
}

fn my_undo(d: &SimpleDml, cols: &[Col], rows: &[Vec<Value>]) -> Result<String, String> {
    let q = |s: &str| crate::sql::quote_ident(DbKind::Mysql, s);
    let lit = |c: &Col, v: &Value| match v.as_str() {
        None => "NULL".to_string(),
        Some("") if c.binary => "''".to_string(),
        Some(h) if c.binary => format!("X'{h}'"),
        Some(_) => crate::transfer::sql_literal(DbKind::Mysql, v),
    };
    if d.delete {
        let keep: Vec<usize> = (0..cols.len()).filter(|&i| !cols[i].generated).collect();
        let names: Vec<String> = keep.iter().map(|&i| q(&cols[i].name)).collect();
        let vals: Vec<String> = rows.iter().map(|r| format!("({})", keep.iter().map(|&i| lit(&cols[i], &r[i])).collect::<Vec<_>>().join(", "))).collect();
        return Ok(format!("INSERT INTO {} ({}) VALUES\n{};\n", d.table, names.join(", "), vals.join(",\n")));
    }
    let set = restore_cols(d, cols, DbKind::Mysql)?;
    let idx = |c: &Col| cols.iter().position(|x| x.name == c.name).unwrap();
    let mut out = String::new();
    for r in rows {
        let s: Vec<String> = set.iter().map(|c| format!("{} = {}", q(&c.name), lit(c, &r[idx(c)]))).collect();
        let w: Vec<String> = cols.iter().enumerate().filter(|(_, c)| c.pk).map(|(i, c)| format!("{} = {}", q(&c.name), lit(c, &r[i]))).collect();
        out.push_str(&format!("UPDATE {} SET {} WHERE {};\n", d.table, s.join(", "), w.join(" AND ")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, pk: bool) -> Col {
        Col { name: name.into(), pk, generated: false, binary: false }
    }

    #[test]
    fn which_statements_are_recorded() {
        let pg = |s: &str| is_change(s, DbKind::Postgres);
        assert!(pg("update t set a = 1 where id = 2"));
        assert!(pg("-- x\ndelete from t"));
        assert!(pg("insert into t values (1)"));
        assert!(pg("create table t (a int)"));
        assert!(pg("with x as (delete from t returning *) select * from x"));
        assert!(!pg("select 1"));
        assert!(!pg("set search_path to app"));
        assert!(!pg("begin"));
        assert!(!pg("commit"));
        assert!(!pg("explain (analyze) update t set a = 1"));
        assert!(is_change("SET k v", DbKind::Redis));
        assert!(!is_change("GET k", DbKind::Redis));
        assert_eq!(tx_end("COMMIT"), Some("COMMIT"));
        assert_eq!(tx_end("rollback"), Some("ROLLBACK"));
        assert_eq!(tx_end("rollback to savepoint a"), None);
    }

    #[test]
    fn pg_undo_sql() {
        let cols = vec![col("id", true), col("name", false), col("Note", false), Col { name: "total".into(), pk: false, generated: true, binary: false }];
        let d = crate::guard::simple_dml("delete from public.users u where u.id < 3", DbKind::Postgres).unwrap();
        assert_eq!(
            pg_undo(&d, &cols, r#"[{"id":1}]"#).unwrap(),
            "INSERT INTO public.users (id, name, \"Note\") OVERRIDING SYSTEM VALUE\nSELECT id, name, \"Note\" FROM jsonb_populate_recordset(NULL::public.users, $qurry$[{\"id\":1}]$qurry$);\n"
        );
        let d = crate::guard::simple_dml("update users set name = 'x' where id = 1", DbKind::Postgres).unwrap();
        assert_eq!(
            pg_undo(&d, &cols, "[]").unwrap(),
            "UPDATE users AS qurry_t SET name = qurry_old.name\nFROM jsonb_populate_recordset(NULL::users, $qurry$[]$qurry$) AS qurry_old\nWHERE qurry_t.id = qurry_old.id;\n"
        );
        // Skip when the PK changes or there is no PK
        let d = crate::guard::simple_dml("update users set id = id + 1", DbKind::Postgres).unwrap();
        assert!(pg_undo(&d, &cols, "[]").is_err());
        let d = crate::guard::simple_dml("update users set name = 'x'", DbKind::Postgres).unwrap();
        assert!(pg_undo(&d, &[col("name", false)], "[]").is_err());
        assert_eq!(pg_text("a$qurry$b'"), "'a$qurry$b'''");
    }

    #[test]
    fn mysql_undo_sql() {
        let mut cols = vec![col("id", true), col("name", false), Col { name: "img".into(), pk: false, generated: false, binary: true }];
        let rows = vec![vec![Value::from("1"), Value::from("it's \\ me"), Value::from("CAFE")], vec![Value::from("2"), Value::Null, Value::Null]];
        let d = crate::guard::simple_dml("DELETE FROM `shop`.`users` WHERE id < 3", DbKind::Mysql).unwrap();
        assert_eq!(my_undo(&d, &cols, &rows).unwrap(), "INSERT INTO `shop`.`users` (id, name, img) VALUES\n('1', 'it''s \\\\ me', X'CAFE'),\n('2', NULL, NULL);\n");
        let d = crate::guard::simple_dml("UPDATE users SET Name = 'x' WHERE id < 3", DbKind::Mysql).unwrap();
        assert_eq!(my_undo(&d, &cols, &rows).unwrap(), "UPDATE users SET name = 'it''s \\\\ me' WHERE id = '1';\nUPDATE users SET name = NULL WHERE id = '2';\n");
        cols[0].pk = false;
        assert!(my_undo(&d, &cols, &rows).is_err());
        assert_eq!(my_name_parts("`a.b`.`c``d`"), vec!["a.b", "c`d"]);
        assert_eq!(my_name_parts("t"), vec!["t"]);
    }

    #[test]
    fn keeps_recent_entries_newest_first() {
        let d = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let a = Audit::new(&d);
        let q = |rows_affected: u64, in_transaction: bool| -> AppResult<QueryResult> { Ok(QueryResult { columns: vec![], rows: vec![], rows_affected, truncated: false, elapsed_ms: 0, has_result_set: false, in_transaction }) };
        a.after("c1::shop", "[PROD] kr", DbKind::Postgres, Some("tab"), "update t set a = 1 where b = 2", &q(2, true), Some(Ok("UNDO".into())));
        a.after("c1", "[PROD] kr", DbKind::Postgres, Some("tab"), "select 1", &q(0, true), None);
        a.after("c1", "[PROD] kr", DbKind::Postgres, Some("tab"), "commit", &q(0, false), None);
        // Don't log COMMIT for a console with no logged change
        a.after("c1", "[PROD] kr", DbKind::Postgres, Some("other"), "commit", &q(0, false), None);
        a.after("c2", "dev", DbKind::Postgres, None, "alter user x password 'secret'", &Err(crate::error::AppError::Msg("denied".into())), None);
        let all = a.list(None, "", 10);
        assert_eq!(all.iter().map(|e| e.sql.as_str()).collect::<Vec<_>>(), vec!["alter user x password '***'", "COMMIT", "update t set a = 1 where b = 2"]);
        assert_eq!(all[0].error.as_deref(), Some("denied"));
        assert_eq!((all[2].rows, all[2].in_tx, all[2].undo.as_deref(), all[2].database.as_deref()), (Some(2), true, Some("UNDO"), Some("shop")));
        assert_eq!(a.list(Some("c1"), "", 10).len(), 2);
        assert_eq!(a.list(None, "UPDATE", 10).len(), 1);
    }

    // ───────── Real servers (QURRY_TEST_PG=host:port:user:db · QURRY_TEST_MY=host:port:user:password:db) ─────────

    async fn prod_state(kind: DbKind, env: &str) -> Option<crate::app::AppState> {
        let e = std::env::var(env).ok()?;
        let p: Vec<&str> = e.split(':').collect();
        let (user, pass, db) = if kind == DbKind::Mysql { (p[2], Some(p[3].to_string()), p[4]) } else { (p[2], None, p[3]) };
        let mut options = crate::store::ConnOptions::default();
        options.confirm_changes = true;
        let c = crate::store::ConnectionConfig {
            id: "d".into(), name: "[PROD] test".into(), kind, host: p[0].into(), port: p[1].parse().unwrap(), user: user.into(), database: db.into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options, ssl: None, redis: None,
        };
        let dir = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(c, pass, None).await.unwrap();
        Some(st)
    }

    /// Running the undo SQL after a change restores the original (UPDATE · DELETE, Auto · Manual, table edit save)
    async fn undo_restores(kind: DbKind, env: &str, ddl: &str, table: &str) {
        let Some(st) = prod_state(kind, env).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), Some(1000), None);
        q(&format!("drop table if exists {table}")).await.unwrap();
        q(ddl).await.unwrap();
        q(&format!("insert into {table} (id, name, price, born, bin) values (1, 'a''b', 1.50, '2026-10-29 10:00:00', 'x'), (2, NULL, 2, NULL, NULL), (3, 'c', 3, NULL, NULL)")).await.unwrap();
        let dump = || async { serde_json::to_string(&q(&format!("select * from {table} order by id")).await.unwrap().rows).unwrap() };
        let before = dump().await;
        let undo_last = || st.audit_log(None, String::new(), 1).remove(0);
        // UPDATE (console session, Auto)
        st.run_query("d".into(), format!("update {table} set name = 'zz', price = price * 10 where id <= 2"), None, Some("t1".into())).await.unwrap();
        let e = undo_last();
        assert_eq!(e.rows, Some(2), "{e:?}");
        let undo = e.undo.expect("undo");
        assert_ne!(dump().await, before);
        for (a, b) in crate::sql::split_statements(&undo, kind) {
            q(&undo[a..b]).await.unwrap();
        }
        assert_eq!(dump().await, before, "{undo}");
        // DELETE (pool)
        q(&format!("delete from {table} where id <> 2")).await.unwrap();
        let undo = undo_last().undo.expect("undo");
        for (a, b) in crate::sql::split_statements(&undo, kind) {
            q(&undo[a..b]).await.unwrap();
        }
        assert_eq!(dump().await, before, "{undo}");
        // Manual: uncommitted changes are logged too, and Commit gets its own entry
        st.set_tx_mode("d".into(), "t2".into(), true).await.unwrap();
        st.run_query("d".into(), format!("delete from {table} where id = 3"), None, Some("t2".into())).await.unwrap();
        assert!(undo_last().in_tx);
        st.commit_tx("t2".into()).await.unwrap();
        assert_eq!(undo_last().sql, "COMMIT");
        let undo = st.audit_log(None, String::new(), 2)[1].undo.clone().unwrap();
        for (a, b) in crate::sql::split_statements(&undo, kind) {
            q(&undo[a..b]).await.unwrap();
        }
        assert_eq!(dump().await, before);
        // Table edit save (one transaction)
        st.execute_batch("d".into(), vec![format!("update {table} set name = 'q' where id = 3")], true).await.unwrap();
        let undo = undo_last().undo.unwrap();
        for (a, b) in crate::sql::split_statements(&undo, kind) {
            q(&undo[a..b]).await.unwrap();
        }
        assert_eq!(dump().await, before);
        // Cannot be undone: reason only
        q(&format!("update {table} set id = id + 10 where id = 1")).await.unwrap();
        assert!(undo_last().note.unwrap().contains("primary key"));
        q(&format!("drop table {table}")).await.unwrap();
        assert_eq!(undo_last().sql, format!("drop table {table}"));
    }

    #[tokio::test]
    #[ignore]
    async fn pg_undo_restores() {
        undo_restores(DbKind::Postgres, "QURRY_TEST_PG", "create table qurry_undo (id int primary key, name text, price numeric(10,2), born timestamptz, bin bytea, total numeric generated always as (price * 2) stored)", "qurry_undo").await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_undo_restores() {
        undo_restores(DbKind::Mysql, "QURRY_TEST_MY", "create table qurry_undo (id int primary key, name varchar(20), price decimal(10,2), born datetime, bin varbinary(10), total decimal(10,2) as (price * 2) stored)", "qurry_undo").await;
    }
}
