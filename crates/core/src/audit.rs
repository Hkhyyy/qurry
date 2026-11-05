//! Production audit log: changing statements run against a Production Data Source · affected rows · time (shared by GUI and TUI).
//! Each change is written to the log before it runs (not run if that fails), then its result replaces that entry.
//! Undo SQL: for simple UPDATE / DELETE (guard::simple_dml), snapshot the rows about to change right before running, in the same transaction and
//! locked (SELECT … FOR UPDATE, at most UNDO_MAX_ROWS rows), and record SQL that reverts them. It never runs on its own — open it in a console and run it (Production confirmation still applies).
//! Log: config/audit.jsonl (one entry per line, 0600). Passwords are masked with history::redact
use crate::backend::{Backend, dispatch};
use crate::db::{Pool, QueryResult};
use crate::error::{AppError, AppResult};
use crate::guard::SimpleDml;
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
    /// Why undo SQL could not be built, or what to do about an entry of unknown outcome (outcome_unknown)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Logged before it ran and no result recorded: still running, or Qurry stopped (or couldn't write the result), so the outcome is unknown
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub started: bool,
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
    let word = first_word(sql);
    if word == "begin" && kind == DbKind::Mysql && crate::mysql::is_compound(&crate::session::strip_comments_lead(sql)) {
        // MariaDB BEGIN NOT ATOMIC … END runs statements of its own, like a CALL
        return true;
    }
    !matches!(word.as_str(), "" | "set" | "use" | "begin" | "start" | "savepoint" | "release" | "commit" | "end" | "rollback" | "abort" | "explain" | "show" | "describe" | "desc")
}

/// Name to log if the statement is COMMIT / ROLLBACK (not ROLLBACK TO SAVEPOINT)
pub(crate) fn tx_end(sql: &str) -> Option<&'static str> {
    let s = crate::session::strip_comments_lead(sql);
    match first_word(sql).as_str() {
        "commit" | "end" => Some("COMMIT"),
        "rollback" | "abort" if !s.starts_with("rollback to") => Some("ROLLBACK"),
        _ => None,
    }
}

/// Note on an entry written before its statement ran. The entry with the result (same id) replaces it; if none follows, it stays as written
const STARTED: &str = "Running, or stopped before its result was recorded (Qurry quit, or couldn't write the log after it ran)";

/// A new entry for a statement (no result yet)
fn entry(conn_id: &str, conn_name: &str, sql: &str) -> AuditEntry {
    AuditEntry {
        id: uuid::Uuid::new_v4().to_string(),
        ts: now_ms(),
        conn_id: conn_id.into(),
        conn_name: conn_name.into(),
        database: crate::app::split_target(conn_id).1.map(String::from),
        sql: sql.into(),
        rows: None,
        error: None,
        in_tx: false,
        undo: None,
        note: None,
        started: false,
    }
}

impl Audit {
    pub fn new(dir: &Path) -> Self {
        Audit { dir: dir.to_path_buf(), pending: Default::default(), write: Default::default() }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("audit.jsonl")
    }

    /// Entries in file order. A later line with the same id (the result of a statement logged before it ran) replaces the earlier one
    fn read(&self) -> Vec<AuditEntry> {
        let Ok(s) = std::fs::read_to_string(self.file()) else { return vec![] };
        let mut out: Vec<AuditEntry> = Vec::new();
        let mut at = std::collections::HashMap::new();
        for e in s.lines().filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok()) {
            match at.get(&e.id) {
                Some(&i) => out[i] = e,
                None => {
                    at.insert(e.id.clone(), out.len());
                    out.push(e);
                }
            }
        }
        out
    }

    pub fn add(&self, e: AuditEntry) -> AppResult<()> {
        self.append(vec![e])
    }

    /// Append entries in one write
    fn append(&self, entries: Vec<AuditEntry>) -> AppResult<()> {
        use std::io::Write;
        let mut lines = String::new();
        for mut e in entries {
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
            lines.push_str(&serde_json::to_string(&e)?);
            lines.push('\n');
        }
        let _w = self.write.lock().unwrap();
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(self.file())?;
        f.write_all(lines.as_bytes())?;
        // A full disk can surface only when the data reaches it
        f.sync_data()?;
        // If it grew too long, keep only the most recent entries
        let n = std::fs::read(self.file()).map(|b| b.iter().filter(|&&c| c == b'\n').count()).unwrap_or(0);
        if n > MAX_ENTRIES + MAX_ENTRIES / 10 {
            let all = self.read();
            let mut out = String::new();
            for i in &all[all.len().saturating_sub(MAX_ENTRIES)..] {
                out.push_str(&serde_json::to_string(i)?);
                out.push('\n');
            }
            // The entries are on disk already: a failed trim (e.g. a full disk) only leaves the file longer until the next append
            if let Err(e) = crate::secrets::write_private(&self.file(), out.as_bytes()) {
                crate::log!("[qurry] couldn't trim {}: {e}", self.file().display());
            }
        }
        Ok(())
    }

    /// Write entries before their statements run. If that fails, they must not run
    fn start(&self, entries: Vec<AuditEntry>) -> AppResult<Vec<AuditEntry>> {
        if entries.is_empty() {
            return Ok(entries);
        }
        let marked = entries.iter().cloned().map(|mut e| {
            e.note = Some(STARTED.into());
            e.started = true;
            e
        });
        self.append(marked.collect()).map_err(|e| {
            AppError::Msg(format!(
                "Not run: changes to a Production data source run only once they are in the Production log, and it couldn't be written ({}: {e}). Free up disk space, or make the file and its folder writable by you, then run it again.",
                self.file().display()
            ))
        })?;
        Ok(entries)
    }

    /// Write the result of statements that already ran (or the COMMIT / ROLLBACK that ended them)
    fn finish(&self, entries: Vec<AuditEntry>) -> AppResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        self.append(entries).map_err(|e| {
            AppError::Msg(format!(
                "It ran, but the result couldn't be written to the Production log ({}: {e}) — only the statement is there. Free up disk space, or make the file and its folder writable by you.",
                self.file().display()
            ))
        })
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

    /// Before running a statement: log a change, or the COMMIT of a console with an uncommitted logged change.
    /// Error = the log can't be written, so don't run it. Pass the returned entry to `after`
    pub(crate) fn before(&self, conn_id: &str, conn_name: &str, kind: DbKind, tab: Option<&str>, sql: &str) -> AppResult<Option<AuditEntry>> {
        let commit = tx_end(sql) == Some("COMMIT") && tab.is_some_and(|t| self.pending.lock().unwrap().contains(t));
        if !is_change(sql, kind) && !commit {
            return Ok(None);
        }
        Ok(self.start(vec![entry(conn_id, conn_name, if commit { "COMMIT" } else { sql })])?.pop())
    }

    /// After running a statement: record the result of what `before` logged, and log ROLLBACK for consoles with an uncommitted logged change
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn after(&self, started: Option<AuditEntry>, conn_id: &str, conn_name: &str, kind: DbKind, tab: Option<&str>, sql: &str, r: &AppResult<QueryResult>, undo: Option<Result<String, String>>) -> AppResult<()> {
        let Some(mut e) = started else {
            return match (tx_end(sql), tab) {
                (Some("ROLLBACK"), Some(t)) if r.is_ok() => self.tx_closed(conn_id, conn_name, t, "ROLLBACK"),
                _ => Ok(()),
            };
        };
        match r {
            Ok(q) if is_change(sql, kind) => {
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
            // COMMIT
            Ok(_) => {
                if let Some(t) = tab {
                    self.pending.lock().unwrap().remove(t);
                }
            }
            // A COMMIT cut off by a lost connection may have committed: the entry says so, not that it failed, and closes the transaction
            Err(err) if tx_end(sql) == Some("COMMIT") && crate::app::is_conn_lost(err) => {
                e.sql = COMMIT_UNKNOWN.into();
                e.error = Some(err.to_string());
                e.note = Some(COMMIT_UNKNOWN_NOTE.into());
                self.finish(vec![e])?;
                if let Some(t) = tab {
                    self.pending.lock().unwrap().remove(t);
                }
                return Ok(());
            }
            Err(err) => e.error = Some(err.to_string()),
        }
        self.finish(vec![e])
    }

    /// Before running several statements in one transaction: log the changes among them (None = not a change). Error = don't run them
    pub(crate) fn before_batch(&self, conn_id: &str, conn_name: &str, kind: DbKind, statements: &[String]) -> AppResult<Vec<Option<AuditEntry>>> {
        let changes = statements.iter().filter(|st| is_change(st, kind)).map(|st| entry(conn_id, conn_name, st)).collect();
        let mut started = self.start(changes)?.into_iter();
        Ok(statements.iter().map(|st| if is_change(st, kind) { started.next() } else { None }).collect())
    }

    /// After running statements in one transaction, like saving table edits (on failure everything was rolled back). exact_one = 1 row per statement
    pub(crate) fn after_batch(&self, started: Vec<Option<AuditEntry>>, err: Option<String>, exact_one: bool, mut undos: Vec<Option<Result<String, String>>>) -> AppResult<()> {
        undos.resize(started.len(), None);
        let mut done = Vec::new();
        for (e, undo) in started.into_iter().zip(undos) {
            let Some(mut e) = e else { continue };
            e.rows = exact_one.then_some(1);
            e.error = err.as_ref().map(|e| format!("Rolled back: {e}"));
            if err.is_none() {
                match undo {
                    Some(Ok(u)) => e.undo = Some(u),
                    Some(Err(why)) => e.note = Some(why),
                    None => {}
                }
            }
            done.push(e);
        }
        self.finish(done)
    }

    /// Rolling back with the console's Roll back button · by closing the console (logged if it had an uncommitted logged change)
    pub(crate) fn tx_closed(&self, conn_id: &str, conn_name: &str, tab: &str, word: &str) -> AppResult<()> {
        if !self.pending.lock().unwrap().remove(tab) {
            return Ok(());
        }
        let mut e = entry(conn_id, conn_name, word);
        if word == ENDED_UNKNOWN {
            e.note = Some(ENDED_UNKNOWN_NOTE.into());
        }
        self.finish(vec![e])
    }
}

/// Log entry for a console transaction with logged changes that the server ended without a COMMIT
pub(crate) const BY_SERVER: &str = "ROLLBACK (by the server)";
/// Log entry for a console transaction with logged changes closed by Disconnect (and reconnecting after its settings changed): the server rolls it back
pub(crate) const DISCONNECTED: &str = "ROLLBACK (disconnected)";
/// Log entry for a MySQL console transaction with logged changes that a statement committed (DDL, LOCK TABLES, SET autocommit = 1, BEGIN …)
pub(crate) const IMPLICIT_COMMIT: &str = "COMMIT (implicit)";
/// Log entries of unknown outcome and the rules that read the log back: in qurry-shared, so the GUI has them through WASM
pub use qurry_shared::audit::{COMMIT_UNKNOWN, ENDED_UNKNOWN, outcome_unknown, status};
/// What to do about ENDED_UNKNOWN (the entry's note)
pub const ENDED_UNKNOWN_NOTE: &str = "The transaction ended in the statement before this entry (a CALL, a deadlock or a lock wait timeout), and MySQL doesn't say whether it committed or rolled back the changes made before it. Check the data before you run them again.";
/// What to do about COMMIT_UNKNOWN (the entry's note)
pub const COMMIT_UNKNOWN_NOTE: &str = "The server may have committed before the connection was lost. Check the data before you run these changes again.";
/// Title of the notice for the error of a failed Commit (the GUI has it through WASM)
pub use qurry_shared::errors::commit_failed_title;

/// How the console's transaction ended when `sql` failed with `e` (open = the session still has it open; None = nothing ended).
/// A lost connection rolls it back on the server, unless `sql` was the COMMIT; otherwise it ended only if the session closed it
/// (an error the server reported: deadlock, failed COMMIT)
pub(crate) fn ended_by(sql: &str, e: &AppError, open: bool) -> Option<&'static str> {
    if crate::app::is_conn_lost(e) {
        return Some(if tx_end(sql) == Some("COMMIT") { COMMIT_UNKNOWN } else { BY_SERVER });
    }
    (!open).then_some(BY_SERVER)
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

/// How to keep a statement's undo: `snapshot` is a SELECT … FOR UPDATE of the rows it changes, run right before it in the same transaction
/// (Session::run_with_snapshot · Backend::execute_batch), so nobody can change the rows it saved in between; `undo` turns its result into SQL that reverts them
/// (rows that start to match only after the snapshot aren't locked: `undo` keeps no undo when the change hits more rows than it saved)
pub(crate) struct UndoPlan {
    pub snapshot: String,
    d: SimpleDml,
    cols: Vec<Col>,
    kind: DbKind,
    sql: String,
}

/// Err = why no undo can be kept (the statement still runs as is)
pub(crate) async fn plan(pool: &Pool, sql: &str, kind: DbKind) -> Result<UndoPlan, String> {
    if !matches!(kind, DbKind::Postgres | DbKind::Mysql) {
        return Err("Undo is kept for PostgreSQL and MySQL only".into());
    }
    let Some(d) = crate::guard::simple_dml(sql, kind) else {
        return Err("Undo is kept for simple UPDATE / DELETE on one table only".into());
    };
    let cols = table_columns(pool, &d.table).await.map_err(|e| format!("Couldn't read the table's columns: {e}"))?;
    if cols.is_empty() {
        return Err(format!("Couldn't find table {}", d.table));
    }
    Ok(UndoPlan { snapshot: snapshot_sql(&d, &cols, kind), d, cols, kind, sql: sql.into() })
}

impl UndoPlan {
    /// Undo SQL from the snapshot's result. changed = rows the statement changed: more than the snapshot saved (rows that matched
    /// only once it ran, row-level security hiding locked rows, triggers) means the undo would miss rows, so none is kept
    pub(crate) fn undo(&self, snap: AppResult<QueryResult>, changed: u64) -> Result<String, String> {
        let (d, cols) = (&self.d, &self.cols);
        let r = snap.map_err(|e| format!("Couldn't save the rows before the change: {e}"))?;
        let head = format!(
            "-- Undo for: {}\n-- Saved in its transaction right before it ran. Rows changed again since then are overwritten.\n",
            self.sql.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect::<String>()
        );
        let saved = match self.kind {
            DbKind::Postgres => r.rows.first().and_then(|x| x.get(1)).and_then(Value::as_i64).unwrap_or(0) as usize,
            _ => r.rows.len(),
        };
        if saved > UNDO_MAX_ROWS {
            return Err(format!("More than {UNDO_MAX_ROWS} rows change: no undo kept"));
        }
        if changed > saved as u64 {
            return Err(format!(
                "It changed {changed} rows but only {saved} were saved right before it ran (rows that matched only once it ran, row-level security or triggers): no undo kept"
            ));
        }
        let body = match self.kind {
            DbKind::Postgres => {
                let json = r.rows.first().and_then(|x| x.first()).and_then(|v| v.as_str()).unwrap_or("[]").to_string();
                if json.len() > UNDO_MAX_BYTES {
                    return Err("The rows are too large to keep an undo".into());
                }
                pg_undo(d, cols, &json)?
            }
            _ => {
                if r.rows.iter().flatten().map(|v| v.as_str().map_or(4, str::len)).sum::<usize>() > UNDO_MAX_BYTES {
                    return Err("The rows are too large to keep an undo".into());
                }
                my_undo(d, cols, &r.rows)?
            }
        };
        Ok(format!("{head}{body}"))
    }
}

/// MySQL `a`.`b` → ["a", "b"]
pub(crate) fn my_name_parts(s: &str) -> Vec<String> {
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
pub(crate) async fn table_columns(pool: &Pool, table: &str) -> AppResult<Vec<Col>> {
    dispatch!(pool, b => b.table_columns(table).await)
}

/// SELECT … FOR UPDATE that snapshots the rows about to change (and locks them until the change commits). PG: rows as one JSON blob (no value loss — unpacked with jsonb_populate_recordset when undoing).
/// MySQL: as text (binary as HEX)
fn snapshot_sql(d: &SimpleDml, cols: &[Col], kind: DbKind) -> String {
    let cond = d.cond.as_ref().map(|c| format!(" WHERE {c}")).unwrap_or_default();
    match kind {
        // UPDATE locks like UPDATE does (FOR NO KEY UPDATE: other sessions' foreign-key checks on child rows aren't blocked); DELETE takes FOR UPDATE
        DbKind::Postgres => format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(qurry_x)), '[]'::jsonb)::text, count(*) FROM (SELECT * FROM {}{cond} LIMIT {}{}{PG_SNAP_END}",
            d.from,
            UNDO_MAX_ROWS + 1,
            if d.delete { PG_LOCK_DELETE } else { PG_LOCK_UPDATE }
        ),
        _ => {
            let exprs: Vec<String> = cols
                .iter()
                .map(|c| {
                    let q = crate::sql::quote_ident(kind, &c.name);
                    if c.binary { format!("HEX({q})") } else { format!("CAST({q} AS CHAR)") }
                })
                .collect();
            format!("SELECT {} FROM {}{cond} LIMIT {} FOR UPDATE", exprs.join(", "), d.from, UNDO_MAX_ROWS + 1)
        }
    }
}

const PG_LOCK_UPDATE: &str = " FOR NO KEY UPDATE";
const PG_LOCK_DELETE: &str = " FOR UPDATE";
const PG_SNAP_END: &str = ") qurry_x";

/// The PostgreSQL snapshot without its row lock (a row lock needs the UPDATE privilege, which a DELETE-only role lacks). None = not from snapshot_sql
pub(crate) fn pg_unlocked(snapshot: &str) -> Option<String> {
    let body = snapshot.strip_suffix(PG_SNAP_END)?;
    let body = body.strip_suffix(PG_LOCK_UPDATE).or_else(|| body.strip_suffix(PG_LOCK_DELETE))?;
    Some(format!("{body}{PG_SNAP_END}"))
}

/// Snapshot errors after which the change must not run: cancelled (Cancel · statement_timeout), lock wait timed out, deadlock, serialization failure.
/// Other errors (no privilege, row lock not allowed on a view, …) only mean no undo is kept, and the change still runs
pub(crate) fn stops_change(e: &AppError) -> bool {
    let AppError::Db(sqlx::Error::Database(d)) = e else { return false };
    match d.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>() {
        // ER_QUERY_INTERRUPTED (KILL QUERY) · ER_LOCK_WAIT_TIMEOUT · ER_LOCK_DEADLOCK
        Some(m) => matches!(m.number(), 1317 | 1205 | 1213),
        None => matches!(d.code().as_deref(), Some("57014" | "55P03" | "40P01" | "40001")),
    }
}

/// The error of a change not run because its snapshot stopped (stops_change). lost_tx = the server also rolled back the open transaction (MySQL deadlock)
pub(crate) fn stopped(e: &AppError, lost_tx: bool) -> AppError {
    AppError::Msg(format!(
        "Not run: {e} — it stopped while locking the rows it changes (to keep their undo), so nothing was changed.{}",
        if lost_tx { " The server also rolled back the open transaction: the uncommitted changes made in it before are lost." } else { "" }
    ))
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

    /// How the log closes the console's transaction after a failed statement: a lost connection rolls it back, unless the statement
    /// was the COMMIT (it may have committed); otherwise only when the server ended it
    #[test]
    fn transaction_end_after_a_failure() {
        let lost = || AppError::Db(sqlx::Error::Io(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "connection reset by peer")));
        assert_eq!(ended_by("COMMIT", &lost(), false), Some(COMMIT_UNKNOWN));
        assert_eq!(ended_by("-- done\nend", &lost(), true), Some(COMMIT_UNKNOWN));
        assert_eq!(ended_by("update t set a = 1 where id = 1", &lost(), true), Some(BY_SERVER));
        // Not lost: the session says whether the transaction is still open
        let other = || AppError::Msg("Not run: deadlock".into());
        assert_eq!(ended_by("update t set a = 1 where id = 1", &other(), false), Some(BY_SERVER));
        assert_eq!(ended_by("update t set a = 1 where id = 1", &other(), true), None);
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
        let my = |s: &str| is_change(s, DbKind::Mysql);
        assert!(!my("begin work"));
        assert!(!my("begin/* x */"));
        assert!(my("begin not atomic update t set v = 1; end"));
        assert!(my("BEGIN/*x*/NOT ATOMIC UPDATE t SET v = 1; END"));
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

    /// PostgreSQL: UPDATE locks FOR NO KEY UPDATE (like UPDATE itself), DELETE FOR UPDATE; both can drop the lock (DELETE-only roles)
    #[test]
    fn pg_snapshot_locks_like_the_change() {
        let cols = vec![col("id", true), col("name", false)];
        let snap = |sql: &str| snapshot_sql(&crate::guard::simple_dml(sql, DbKind::Postgres).unwrap(), &cols, DbKind::Postgres);
        let up = snap("update users set name = 'x' where id = 1");
        assert!(up.ends_with("WHERE id = 1 LIMIT 1001 FOR NO KEY UPDATE) qurry_x"), "{up}");
        let del = snap("delete from users where id = 1");
        assert!(del.ends_with("WHERE id = 1 LIMIT 1001 FOR UPDATE) qurry_x"), "{del}");
        assert!(pg_unlocked(&up).unwrap().ends_with("WHERE id = 1 LIMIT 1001) qurry_x"));
        assert!(pg_unlocked(&del).unwrap().ends_with("WHERE id = 1 LIMIT 1001) qurry_x"));
        assert_eq!(pg_unlocked("select * from t for update"), None);
        assert!(snapshot_sql(&crate::guard::simple_dml("update users set name = 'x'", DbKind::Mysql).unwrap(), &cols, DbKind::Mysql).ends_with(" FOR UPDATE"));
    }

    /// A change that hits more rows than the snapshot saved keeps no undo (it would miss rows); as many or fewer is fine
    #[test]
    fn undo_needs_every_changed_row() {
        let qr = |rows: Vec<Vec<Value>>| -> AppResult<QueryResult> { Ok(QueryResult { columns: vec![], rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None }) };
        let cols = vec![col("id", true), col("v", false)];
        let plan = |sql: &str, kind: DbKind| {
            let d = crate::guard::simple_dml(sql, kind).unwrap();
            UndoPlan { snapshot: snapshot_sql(&d, &cols, kind), d, cols: cols.clone(), kind, sql: sql.into() }
        };
        let pg = plan("delete from t where id = 1", DbKind::Postgres);
        let one = || qr(vec![vec![Value::from(r#"[{"id":1,"v":1}]"#), Value::from(1)]]);
        assert!(pg.undo(one(), 1).is_ok());
        assert!(pg.undo(one(), 0).is_ok());
        let e = pg.undo(one(), 2).unwrap_err();
        assert!(e.contains("changed 2 rows but only 1 were saved") && e.contains("row-level security"), "{e}");
        // Row-level security hid the row from the locking read: nothing saved, one row deleted
        assert!(pg.undo(qr(vec![vec![Value::from("[]"), Value::from(0)]]), 1).is_err());
        let my = plan("update t set v = 2 where v = 1", DbKind::Mysql);
        let rows = || qr(vec![vec![Value::from("1"), Value::from("1")]]);
        assert!(my.undo(rows(), 1).is_ok());
        assert!(my.undo(rows(), 3).unwrap_err().contains("changed 3 rows but only 1"));
    }

    /// Once the entry is on disk, a failed trim of a long log is not a failed write (the change isn't refused); the next append trims it
    #[cfg(unix)]
    #[test]
    fn trim_failure_is_not_a_write_failure() {
        let d = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let a = Audit::new(&d);
        let line = serde_json::to_string(&entry("c1", "[PROD] kr", "delete from t where id = 1")).unwrap();
        std::fs::write(a.file(), format!("{line}\n").repeat(MAX_ENTRIES + MAX_ENTRIES / 10 + 5)).unwrap();
        let lines = || std::fs::read_to_string(a.file()).unwrap().lines().count();
        // The trim's temporary file can't be created
        let tmp = a.file().with_extension(format!("tmp-{}", std::process::id()));
        std::fs::create_dir(&tmp).unwrap();
        let before = lines();
        assert!(a.before("c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 2").unwrap().is_some());
        assert_eq!(lines(), before + 1);
        std::fs::remove_dir(&tmp).unwrap();
        a.before("c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 3").unwrap();
        assert!(lines() <= MAX_ENTRIES, "{}", lines());
        assert_eq!(a.list(None, "", 1)[0].sql, "delete from t where id = 3");
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
        let q = |rows_affected: u64, in_transaction: bool| -> AppResult<QueryResult> { Ok(QueryResult { columns: vec![], rows: vec![], rows_affected, truncated: false, elapsed_ms: 0, has_result_set: false, in_transaction, warning: None, editable: None }) };
        let run = |conn: &str, tab: Option<&str>, sql: &str, r: AppResult<QueryResult>, undo: Option<Result<String, String>>| {
            let started = a.before(conn, "[PROD] kr", DbKind::Postgres, tab, sql).unwrap();
            a.after(started, conn, "[PROD] kr", DbKind::Postgres, tab, sql, &r, undo).unwrap();
        };
        run("c1::shop", Some("tab"), "update t set a = 1 where b = 2", q(2, true), Some(Ok("UNDO".into())));
        run("c1", Some("tab"), "select 1", q(0, true), None);
        run("c1", Some("tab"), "commit", q(0, false), None);
        // Don't log COMMIT for a console with no logged change
        run("c1", Some("other"), "commit", q(0, false), None);
        run("c2", None, "alter user x password 'secret'", Err(crate::error::AppError::Msg("denied".into())), None);
        let all = a.list(None, "", 10);
        assert_eq!(all.iter().map(|e| e.sql.as_str()).collect::<Vec<_>>(), vec!["alter user x password '***'", "COMMIT", "update t set a = 1 where b = 2"]);
        assert_eq!(all[0].error.as_deref(), Some("denied"));
        assert_eq!((all[2].rows, all[2].in_tx, all[2].undo.as_deref(), all[2].database.as_deref()), (Some(2), true, Some("UNDO"), Some("shop")));
        assert_eq!(a.list(Some("c1"), "", 10).len(), 2);
        assert_eq!(a.list(None, "UPDATE", 10).len(), 1);
    }

    /// A COMMIT cut off by a lost connection is one entry of unknown outcome (not a failed COMMIT), and it closes the transaction in the log
    #[test]
    fn commit_on_a_lost_connection_is_of_unknown_outcome() {
        let d = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let a = Audit::new(&d);
        let changed = Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: 1, truncated: false, elapsed_ms: 0, has_result_set: false, in_transaction: true, warning: None, editable: None });
        let lost: AppResult<QueryResult> = Err(AppError::Db(sqlx::Error::Io(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "connection reset by peer"))));
        let run = |sql: &str, r: &AppResult<QueryResult>| {
            let started = a.before("c1", "[PROD] kr", DbKind::Postgres, Some("tab"), sql).unwrap();
            a.after(started, "c1", "[PROD] kr", DbKind::Postgres, Some("tab"), sql, r, None).unwrap();
        };
        run("update t set a = 1 where id = 1", &changed);
        run("COMMIT", &lost);
        let all = a.list(None, "", 10);
        assert_eq!(all.iter().map(|e| e.sql.as_str()).collect::<Vec<_>>(), vec![COMMIT_UNKNOWN, "update t set a = 1 where id = 1"]);
        assert!(all[0].error.as_deref().is_some_and(|e| e.contains("connection reset")) && !all[0].started);
        // The entry says what to do next
        assert_eq!(all[0].note.as_deref(), Some(COMMIT_UNKNOWN_NOTE));
        // Nothing left open: a later server rollback adds no entry
        a.tx_closed("c1", "[PROD] kr", "tab", BY_SERVER).unwrap();
        assert_eq!(a.list(None, "", 10).len(), 2);
    }

    /// A transaction that ended in a CALL is an entry of unknown outcome with a note saying what to do; other endings have no note
    #[test]
    fn transaction_ended_in_a_procedure_is_of_unknown_outcome() {
        let d = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let a = Audit::new(&d);
        let changed = Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: 1, truncated: false, elapsed_ms: 0, has_result_set: false, in_transaction: true, warning: None, editable: None });
        for word in [ENDED_UNKNOWN, IMPLICIT_COMMIT] {
            let started = a.before("c1", "[PROD] kr", DbKind::Mysql, Some("tab"), "update t set a = 1 where id = 1").unwrap();
            a.after(started, "c1", "[PROD] kr", DbKind::Mysql, Some("tab"), "update t set a = 1 where id = 1", &changed, None).unwrap();
            a.tx_closed("c1", "[PROD] kr", "tab", word).unwrap();
            let e = a.list(None, "", 1).remove(0);
            assert_eq!(e.sql, word);
            assert_eq!((outcome_unknown(&e.sql), e.note.as_deref()), if word == ENDED_UNKNOWN { (true, Some(ENDED_UNKNOWN_NOTE)) } else { (false, None) });
        }
        assert!(outcome_unknown(COMMIT_UNKNOWN) && !outcome_unknown("COMMIT"));
    }

    /// A change is in the log before it runs; its result replaces that entry. If the result can't be written, that is reported and the first entry stays
    #[cfg(unix)]
    #[test]
    fn changes_are_logged_before_they_run() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let a = Audit::new(&d);
        let ok = Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: 1, truncated: false, elapsed_ms: 0, has_result_set: false, in_transaction: false, warning: None, editable: None });
        assert!(a.before("c1", "[PROD] kr", DbKind::Postgres, None, "select 1").unwrap().is_none());
        let started = a.before("c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 1").unwrap();
        let e = a.list(None, "", 10).remove(0);
        assert_eq!((e.sql.as_str(), e.rows, e.note.as_deref()), ("delete from t where id = 1", None, Some(STARTED)));
        assert!(e.started);
        a.after(started, "c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 1", &ok, Some(Ok("UNDO".into()))).unwrap();
        let all = a.list(None, "", 10);
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].id.as_str(), all[0].rows, all[0].note.as_deref(), all[0].undo.as_deref()), (e.id.as_str(), Some(1), None, Some("UNDO")));
        assert!(!all[0].started);
        // The result can't be written
        let started = a.before("c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 2").unwrap();
        std::fs::set_permissions(a.file(), std::fs::Permissions::from_mode(0o400)).unwrap();
        let err = a.after(started, "c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 2", &ok, None).unwrap_err().to_string();
        assert!(err.starts_with("It ran, but") && err.contains("audit.jsonl"), "{err}");
        // A change that can't be logged first is refused
        let err = a.before("c1", "[PROD] kr", DbKind::Postgres, None, "delete from t where id = 3").unwrap_err().to_string();
        assert!(err.starts_with("Not run: ") && err.contains("run it again"), "{err}");
        let err = a.before_batch("c1", "[PROD] kr", DbKind::Postgres, &["select 1".into(), "delete from t where id = 3".into()]).unwrap_err().to_string();
        assert!(err.starts_with("Not run: "), "{err}");
        std::fs::set_permissions(a.file(), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(a.list(None, "", 10).iter().map(|e| (e.sql.as_str(), e.note.as_deref())).collect::<Vec<_>>(), vec![("delete from t where id = 2", Some(STARTED)), ("delete from t where id = 1", None)]);
        // Batch: only the changes are logged, matched to their statements
        let started = a.before_batch("c1", "[PROD] kr", DbKind::Postgres, &["select 1".into(), "delete from t where id = 4".into()]).unwrap();
        assert_eq!(started.iter().map(|e| e.as_ref().map(|e| e.sql.as_str())).collect::<Vec<_>>(), vec![None, Some("delete from t where id = 4")]);
        a.after_batch(started, None, true, vec![None, Some(Ok("UNDO4".into()))]).unwrap();
        assert_eq!(a.list(None, "id = 4", 10).iter().map(|e| (e.rows, e.undo.as_deref())).collect::<Vec<_>>(), vec![(Some(1), Some("UNDO4"))]);
    }

    /// Production: a change that can't be logged is not run (refused before connecting); reads still run
    #[tokio::test]
    async fn production_change_is_refused_when_the_log_cant_be_written() {
        let st = prod_state_at(crate::testenv::parse_pg("127.0.0.1:1:u:db").cfg, None).await;
        std::fs::create_dir(st.store.dir().join("audit.jsonl")).unwrap();
        let e = st.run_query("d".into(), "update t set a = 1 where id = 1".into(), None, None).await.unwrap_err().to_string();
        assert!(e.starts_with("Not run: ") && e.contains("audit.jsonl") && e.contains("run it again"), "{e}");
        let e = st.execute_batch("d".into(), vec!["delete from t where id = 1".into()], true).await.unwrap_err().to_string();
        assert!(e.starts_with("Not run: "), "{e}");
        // Not a change: not logged, so it goes on (and fails connecting)
        let e = st.run_query("d".into(), "select 1".into(), None, None).await.unwrap_err().to_string();
        assert!(!e.starts_with("Not run: "), "{e}");
    }

    // ───────── Real servers (QURRY_TEST_PG=host:port:user:db · QURRY_TEST_MY=host:port:user:password:db) ─────────

    async fn prod_state(kind: DbKind) -> Option<crate::app::AppState> {
        let crate::testenv::Server { cfg, password } = crate::testenv::server(kind)?;
        Some(prod_state_at(cfg, password).await)
    }

    /// A production data source (confirm changes on) on the server of `c`
    async fn prod_state_at(mut c: crate::store::ConnectionConfig, pass: Option<String>) -> crate::app::AppState {
        (c.id, c.name) = ("d".into(), "[PROD] test".into());
        c.options.confirm_changes = true;
        let dir = std::env::temp_dir().join(format!("qurry-audit-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(c, pass, None).await.unwrap();
        st
    }

    /// The snapshot is taken in the change's transaction with FOR UPDATE: a row another session is changing is saved
    /// as it is once that session commits (the value the change overwrote), not as it was before. Console (Auto) · pool · table edit save.
    /// Then Production refuses a COMMIT that can't be logged, and the transaction stays open
    async fn snapshot_matches_change(kind: DbKind, ddl: &str) {
        let Some(st) = prod_state(kind).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), Some(1000), None);
        q(if kind == DbKind::Mysql { "create database if not exists audit_hardening" } else { "create schema if not exists audit_hardening" }).await.unwrap();
        q("drop table if exists audit_hardening.qurry_snap").await.unwrap();
        q(ddl).await.unwrap();
        let prices = || async { serde_json::to_string(&q("select price from audit_hardening.qurry_snap order by id").await.unwrap().rows).unwrap() };
        for (i, tab) in [Some("t1"), None, Some("batch")].into_iter().enumerate() {
            q("delete from audit_hardening.qurry_snap").await.unwrap();
            q("insert into audit_hardening.qurry_snap (id, price) values (1, 1), (2, 2), (3, 3)").await.unwrap();
            // Another session changes row 1 and holds its lock
            st.set_tx_mode("d".into(), "other".into(), true).await.unwrap();
            st.run_query("d".into(), "update audit_hardening.qurry_snap set price = 99 where id = 1".into(), None, Some("other".into())).await.unwrap();
            let change = format!("update audit_hardening.qurry_snap set price = price + {} where id <= 2", i + 10);
            let run = async {
                match tab {
                    Some("batch") => st.execute_batch("d".into(), vec![change.clone()], false).await.map(|_| ()),
                    t => st.run_query("d".into(), change.clone(), None, t.map(String::from)).await.map(|_| ()),
                }
            };
            let other = async {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                st.commit_tx("other".into()).await
            };
            let (a, b) = tokio::join!(run, other);
            a.unwrap();
            b.unwrap();
            let e = st.audit_log(None, change.clone(), 1).remove(0);
            // A batch records rows per statement only for table edit saves (exactly 1 each)
            assert_eq!(e.rows, (tab != Some("batch")).then_some(2), "{e:?}");
            let undo = e.undo.expect("undo");
            for (a, b) in crate::sql::split_statements(&undo, kind) {
                q(&undo[a..b]).await.unwrap();
            }
            // Row 1 goes back to what the change overwrote (99), not to the value before the other session's change
            assert_eq!(prices().await, r#"[["99.00"],["2.00"],["3.00"]]"#, "{tab:?}\n{undo}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let log = st.store.dir().join("audit.jsonl");
            st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
            st.run_query("d".into(), "delete from audit_hardening.qurry_snap where id = 3".into(), None, Some("m".into())).await.unwrap();
            std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o400)).unwrap();
            let e = st.commit_tx("m".into()).await.unwrap_err().to_string();
            assert!(e.starts_with("Not run: "), "{e}");
            assert!(st.tx_state("m".into()).await);
            std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).unwrap();
            st.rollback_tx("m".into()).await.unwrap();
            assert_eq!(prices().await, r#"[["99.00"],["2.00"],["3.00"]]"#);
        }
        q(if kind == DbKind::Mysql { "drop database audit_hardening" } else { "drop schema audit_hardening cascade" }).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_snapshot_matches_change() {
        let _one = one_at_a_time().await;
        snapshot_matches_change(DbKind::Postgres, "create table audit_hardening.qurry_snap (id int primary key, price numeric(10,2))").await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_snapshot_matches_change() {
        let _one = one_at_a_time().await;
        snapshot_matches_change(DbKind::Mysql, "create table audit_hardening.qurry_snap (id int primary key, price decimal(10,2))").await;
    }

    /// Running the undo SQL after a change restores the original (UPDATE · DELETE, Auto · Manual, table edit save)
    async fn undo_restores(kind: DbKind, ddl: &str, table: &str) {
        let Some(st) = prod_state(kind).await else { return };
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
        undo_restores(DbKind::Postgres, "create table qurry_undo (id int primary key, name text, price numeric(10,2), born timestamptz, bin bytea, total numeric generated always as (price * 2) stored)", "qurry_undo").await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_undo_restores() {
        undo_restores(DbKind::Mysql, "create table qurry_undo (id int primary key, name varchar(20), price decimal(10,2), born datetime, bin varbinary(10), total decimal(10,2) as (price * 2) stored)", "qurry_undo").await;
    }

    /// The real-server tests below share the audit_hardening schema / database (and drop it when they pass), so they run one at a time
    async fn one_at_a_time() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        LOCK.lock().await
    }

    /// A schema (PostgreSQL) / database (MySQL) of this test's own with table `t` (id, v) holding rows (1, 1) (2, 2) (3, 3)
    async fn own_table(st: &crate::app::AppState, kind: DbKind, t: &str) {
        let q = |s: String| st.run_query("d".into(), s, None, None);
        q(if kind == DbKind::Mysql { "create database if not exists audit_hardening".into() } else { "create schema if not exists audit_hardening".into() }).await.unwrap();
        q(format!("drop table if exists audit_hardening.{t}")).await.unwrap();
        q(format!("create table audit_hardening.{t} (id int primary key, v int)")).await.unwrap();
        q(format!("insert into audit_hardening.{t} values (1, 1), (2, 2), (3, 3)")).await.unwrap();
    }

    async fn values(st: &crate::app::AppState, t: &str) -> String {
        serde_json::to_string(&st.run_query("d".into(), format!("select v from audit_hardening.{t} order by id"), None, None).await.unwrap().rows).unwrap()
    }

    async fn drop_own(st: &crate::app::AppState, kind: DbKind) {
        let sql = if kind == DbKind::Mysql { "drop database audit_hardening" } else { "drop schema audit_hardening cascade" };
        st.run_query("d".into(), sql.into(), None, None).await.unwrap();
    }

    /// Cancel while the change waits in its snapshot (another session holds the row): the change is not run, in Auto or in the user's
    /// transaction (which stays open with its earlier change)
    async fn cancelled_snapshot_runs_nothing(kind: DbKind) {
        let Some(st) = prod_state(kind).await else { return };
        own_table(&st, kind, "qurry_cancel").await;
        let run = |tab: &str, sql: &str| st.run_query("d".into(), sql.into(), None, Some(tab.into()));
        st.set_tx_mode("d".into(), "other".into(), true).await.unwrap();
        run("other", "update audit_hardening.qurry_cancel set v = 10 where id = 1").await.unwrap();
        for manual in [false, true] {
            st.set_tx_mode("d".into(), "c".into(), manual).await.unwrap();
            if manual {
                run("c", "update audit_hardening.qurry_cancel set v = 20 where id = 2").await.unwrap();
            }
            let cancel = async {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                st.cancel_query("c".into()).await.unwrap()
            };
            let (r, _) = tokio::join!(run("c", "update audit_hardening.qurry_cancel set v = 30 where id = 1"), cancel);
            let e = r.unwrap_err().to_string();
            assert!(e.starts_with("Not run: ") && e.contains("nothing was changed") && !e.contains("lost"), "{e}");
            assert_eq!(st.tx_state("c".into()).await, manual);
            if manual {
                st.commit_tx("c".into()).await.unwrap();
            }
        }
        st.commit_tx("other".into()).await.unwrap();
        // The cancelled change never ran: row 1 has the other session's value, row 2 the Manual console's
        assert_eq!(values(&st, "qurry_cancel").await, "[[10],[20],[3]]");
        drop_own(&st, kind).await;
    }

    #[tokio::test]
    #[ignore]
    async fn pg_cancelled_snapshot_runs_nothing() {
        let _one = one_at_a_time().await;
        cancelled_snapshot_runs_nothing(DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_cancelled_snapshot_runs_nothing() {
        let _one = one_at_a_time().await;
        cancelled_snapshot_runs_nothing(DbKind::Mysql).await;
    }

    /// MySQL: a deadlock on the snapshot rolls back the user's whole transaction. The change is not run, the console no longer
    /// shows an open transaction, and the message says the earlier changes are lost
    #[tokio::test]
    #[ignore]
    async fn mysql_snapshot_deadlock_reports_the_lost_transaction() {
        let _one = one_at_a_time().await;
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        own_table(&st, DbKind::Mysql, "qurry_dead").await;
        let run = |tab: &str, sql: &str| st.run_query("d".into(), sql.into(), None, Some(tab.into()));
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("m", "update audit_hardening.qurry_dead set v = 10 where id = 1").await.unwrap();
        // o holds more rows, so InnoDB picks m as the deadlock victim
        run("o", "update audit_hardening.qurry_dead set v = 20 where id in (2, 3)").await.unwrap();
        let m = async {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            run("m", "update audit_hardening.qurry_dead set v = 11 where id = 2").await
        };
        let (o, m) = tokio::join!(run("o", "update audit_hardening.qurry_dead set v = 21 where id = 1"), m);
        o.unwrap();
        let e = m.unwrap_err().to_string();
        assert!(e.starts_with("Not run: ") && e.contains("changes made in it before are lost"), "{e}");
        assert!(!st.tx_state("m".into()).await);
        // The log says the first change was rolled back (a later COMMIT doesn't read as committing it)
        assert_eq!(st.audit_log(None, "by the server".into(), 5).len(), 1);
        st.commit_tx("o".into()).await.unwrap();
        // Rolling back now finds nothing of m's: its first change is gone and its second never ran
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(values(&st, "qurry_dead").await, "[[21],[20],[20]]");
        drop_own(&st, DbKind::Mysql).await;
    }

    /// PostgreSQL: the snapshot of an UPDATE doesn't block other sessions' foreign-key checks (FOR NO KEY UPDATE), and a COMMIT of
    /// Qurry's own transaction that fails (deferred foreign key) leaves no transaction open
    #[tokio::test]
    #[ignore]
    async fn pg_snapshot_locks_and_failed_commit() {
        let _one = one_at_a_time().await;
        let Some(st) = prod_state(DbKind::Postgres).await else { return };
        own_table(&st, DbKind::Postgres, "qurry_parent").await;
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("create table audit_hardening.qurry_child (id int primary key, parent_id int references audit_hardening.qurry_parent deferrable initially deferred)").await.unwrap();
        q("insert into audit_hardening.qurry_child values (1, 1)").await.unwrap();
        // A Manual console's uncommitted UPDATE of parent 1 doesn't hold up an insert of a child of parent 1
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        st.run_query("d".into(), "update audit_hardening.qurry_parent set v = 9 where id = 1".into(), None, Some("m".into())).await.unwrap();
        let insert = tokio::time::timeout(std::time::Duration::from_secs(5), q("insert into audit_hardening.qurry_child values (2, 1)")).await;
        assert!(insert.expect("blocked by the snapshot's lock").is_ok());
        st.rollback_tx("m".into()).await.unwrap();
        // Auto: Qurry's BEGIN … DELETE … COMMIT fails at COMMIT
        let e = st.run_query("d".into(), "delete from audit_hardening.qurry_parent where id = 1".into(), None, Some("a".into())).await.unwrap_err().to_string();
        assert!(e.contains("foreign key"), "{e}");
        assert!(!st.tx_state("a".into()).await);
        let r = st.run_query("d".into(), "update audit_hardening.qurry_parent set v = 5 where id = 1".into(), None, Some("a".into())).await.unwrap();
        assert!(!r.in_transaction);
        assert!(!st.audit_log(None, String::new(), 1)[0].in_tx);
        assert_eq!(values(&st, "qurry_parent").await, "[[5],[2],[3]]");
        // Manual: a failed COMMIT is logged, then the rollback the server did; closing the console logs nothing more
        st.set_tx_mode("d".into(), "m2".into(), true).await.unwrap();
        st.run_query("d".into(), "delete from audit_hardening.qurry_parent where id = 1".into(), None, Some("m2".into())).await.unwrap();
        assert!(st.commit_tx("m2".into()).await.unwrap_err().to_string().contains("foreign key"));
        st.close_session("m2".into()).await.unwrap();
        let log: Vec<(String, bool)> = st.audit_log(None, String::new(), 3).into_iter().map(|e| (e.sql, e.error.is_some())).collect();
        assert_eq!(log[..2], [("ROLLBACK (by the server)".to_string(), false), ("COMMIT".to_string(), true)], "{log:?}");
        drop_own(&st, DbKind::Postgres).await;
    }

    /// PostgreSQL: a role with SELECT and DELETE but not UPDATE can't lock rows; its DELETE still keeps undo (snapshot without the lock)
    #[tokio::test]
    #[ignore]
    async fn pg_delete_only_role_keeps_undo() {
        let _one = one_at_a_time().await;
        let Some(st) = prod_state(DbKind::Postgres).await else { return };
        own_table(&st, DbKind::Postgres, "qurry_del").await;
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop role if exists audit_hardening_del").await.unwrap();
        q("create role audit_hardening_del login").await.unwrap();
        q("grant usage on schema audit_hardening to audit_hardening_del").await.unwrap();
        q("grant select, delete on audit_hardening.qurry_del to audit_hardening_del").await.unwrap();
        let mut c = crate::testenv::pg().unwrap().cfg;
        c.user = "audit_hardening_del".into();
        let del = prod_state_at(c, None).await;
        del.run_query("d".into(), "delete from audit_hardening.qurry_del where id = 1".into(), None, Some("t".into())).await.unwrap();
        let entry = del.audit_log(None, String::new(), 1).remove(0);
        let undo = entry.undo.unwrap_or_else(|| panic!("no undo: {:?}", entry.note));
        for (a, b) in crate::sql::split_statements(&undo, DbKind::Postgres) {
            q(&undo[a..b]).await.unwrap();
        }
        assert_eq!(values(&st, "qurry_del").await, "[[1],[2],[3]]");
        del.disconnect("d".into()).await.unwrap();
        drop_own(&st, DbKind::Postgres).await;
        q("drop role audit_hardening_del").await.unwrap();
    }

    /// PostgreSQL row-level security with SELECT and DELETE policies but no UPDATE policy: the locking snapshot sees no rows
    /// (FOR UPDATE applies the UPDATE policies), the DELETE still deletes one. No undo is kept, with the reason
    #[tokio::test]
    #[ignore]
    async fn pg_rls_hidden_rows_keep_no_undo() {
        let _one = one_at_a_time().await;
        let Some(st) = prod_state(DbKind::Postgres).await else { return };
        own_table(&st, DbKind::Postgres, "qurry_rls").await;
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        for s in [
            "drop role if exists audit_hardening_rls",
            "create role audit_hardening_rls login",
            "grant usage on schema audit_hardening to audit_hardening_rls",
            "grant select, update, delete on audit_hardening.qurry_rls to audit_hardening_rls",
            "alter table audit_hardening.qurry_rls enable row level security",
            "create policy s on audit_hardening.qurry_rls for select using (true)",
            "create policy d on audit_hardening.qurry_rls for delete using (true)",
        ] {
            q(s).await.unwrap();
        }
        let mut c = crate::testenv::pg().unwrap().cfg;
        c.user = "audit_hardening_rls".into();
        let rls = prod_state_at(c, None).await;
        let r = rls.run_query("d".into(), "delete from audit_hardening.qurry_rls where id = 1".into(), None, Some("t".into())).await.unwrap();
        assert_eq!(r.rows_affected, 1);
        let entry = rls.audit_log(None, String::new(), 1).remove(0);
        assert!(entry.undo.is_none(), "{entry:?}");
        assert!(entry.note.as_deref().is_some_and(|n| n.contains("changed 1 rows but only 0 were saved")), "{entry:?}");
        assert_eq!(values(&st, "qurry_rls").await, "[[2],[3]]");
        rls.disconnect("d".into()).await.unwrap();
        drop_own(&st, DbKind::Postgres).await;
        q("drop role audit_hardening_rls").await.unwrap();
    }

    /// PostgreSQL READ COMMITTED: a row that starts to match while the snapshot waits for a lock is changed but not saved
    /// (the change takes a newer snapshot). No undo is kept, instead of one that misses that row
    #[tokio::test]
    #[ignore]
    async fn pg_rows_joining_during_the_change_keep_no_undo() {
        let _one = one_at_a_time().await;
        let Some(st) = prod_state(DbKind::Postgres).await else { return };
        own_table(&st, DbKind::Postgres, "qurry_join").await;
        let run = |tab: &str, sql: &str| st.run_query("d".into(), sql.into(), None, Some(tab.into()));
        // Another session holds row 2 and inserts row 4, both matching v < 5, uncommitted
        st.set_tx_mode("d".into(), "other".into(), true).await.unwrap();
        run("other", "update audit_hardening.qurry_join set v = 2 where id = 2").await.unwrap();
        run("other", "insert into audit_hardening.qurry_join values (4, 4)").await.unwrap();
        let other = async {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            st.commit_tx("other".into()).await
        };
        let (r, c) = tokio::join!(run("c", "delete from audit_hardening.qurry_join where v < 5"), other);
        c.unwrap();
        assert_eq!(r.unwrap().rows_affected, 4);
        let entry = st.audit_log(None, "where v < 5".into(), 1).remove(0);
        assert!(entry.undo.is_none(), "{entry:?}");
        assert!(entry.note.as_deref().is_some_and(|n| n.contains("changed 4 rows but only 3 were saved")), "{entry:?}");
        drop_own(&st, DbKind::Postgres).await;
    }

    /// A change that ran but whose result couldn't be logged comes back as a result with a warning (not an error to retry)
    #[cfg(unix)]
    #[tokio::test]
    #[ignore]
    async fn pg_log_failure_after_the_run_is_a_warning() {
        let _one = one_at_a_time().await;
        use std::os::unix::fs::PermissionsExt;
        let Some(st) = prod_state(DbKind::Postgres).await else { return };
        own_table(&st, DbKind::Postgres, "qurry_warn").await;
        let log = st.store.dir().join("audit.jsonl");
        let slow = "do $$ begin perform pg_sleep(1); end $$";
        let lock = || async {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o400)).unwrap();
        };
        let (r, _) = tokio::join!(st.run_query("d".into(), slow.into(), None, Some("t".into())), lock());
        assert!(r.unwrap().warning.is_some_and(|w| w.starts_with("It ran, but")));
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (r, _) = tokio::join!(st.execute_batch("d".into(), vec![slow.into()], false), lock());
        assert!(r.unwrap().warning.is_some_and(|w| w.starts_with("It ran, but")));
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).unwrap();
        // The DDL editor passes it on too
        let (r, _) = tokio::join!(st.run_ddl("d".into(), vec![slow.into()]), lock());
        assert!(r.unwrap().is_some_and(|w| w.starts_with("It ran, but")));
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).unwrap();
        // All stay in the log as "no result recorded"
        assert!(st.audit_log(None, "pg_sleep".into(), 3).iter().all(|e| e.started));
        drop_own(&st, DbKind::Postgres).await;
    }

    /// A schema (PostgreSQL) / database (MySQL) `s` of the test's own, with table `s.t` (id, v) holding rows (1, 1) (2, 2) (3, 3)
    async fn followup_table(st: &crate::app::AppState, kind: DbKind, s: &str) {
        let my = kind == DbKind::Mysql;
        for sql in [
            if my { format!("drop database if exists {s}") } else { format!("drop schema if exists {s} cascade") },
            if my { format!("create database {s}") } else { format!("create schema {s}") },
            format!("create table {s}.t (id int primary key, v int)"),
            format!("insert into {s}.t values (1, 1), (2, 2), (3, 3)"),
        ] {
            st.run_query("d".into(), sql, None, None).await.unwrap();
        }
    }

    async fn followup_values(st: &crate::app::AppState, s: &str) -> String {
        serde_json::to_string(&st.run_query("d".into(), format!("select v from {s}.t order by id"), None, None).await.unwrap().rows).unwrap()
    }

    async fn followup_drop(st: &crate::app::AppState, kind: DbKind, s: &str) {
        let sql = if kind == DbKind::Mysql { format!("drop database {s}") } else { format!("drop schema {s} cascade") };
        st.run_query("d".into(), sql, None, None).await.unwrap();
    }

    /// select version() of the MySQL-protocol server (8.4.3 · 5.7.44-log · 11.8.2-MariaDB-…)
    async fn server_version(st: &crate::app::AppState) -> String {
        st.run_query("d".into(), "select version()".into(), None, None).await.unwrap().rows[0][0].to_string()
    }

    /// Whether the server is MySQL `min` or later (not MariaDB, whose versions count differently): WITH … DELETE needs 8.0, error 4031 8.0.24
    async fn mysql_at_least(st: &crate::app::AppState, min: [u32; 3]) -> bool {
        let v = server_version(st).await;
        let nums: Vec<u32> = v.trim_matches('"').split(['.', '-']).take(3).map(|n| n.parse().unwrap_or(0)).collect();
        !v.contains("MariaDB") && nums.as_slice() >= min.as_slice()
    }

    /// The server ends the console's connection (pg_terminate_backend · KILL) while a Manual transaction holds a logged change.
    /// The next statement fails: the log closes the transaction as rolled back by the server, so a later COMMIT doesn't read as
    /// committing the lost change. A COMMIT cut off the same way is logged as of unknown outcome, and the next run reconnects
    async fn lost_connection_is_logged(kind: DbKind) {
        let Some(st) = prod_state(kind).await else { return };
        let s = "audit_followups_lost";
        followup_table(&st, kind, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let kill = || async {
            let pid = st.sessions.read().await.get("m").unwrap().pid;
            let sql = if kind == DbKind::Mysql { format!("KILL {pid}") } else { format!("select pg_terminate_backend({pid})") };
            st.run_query("d".into(), sql, None, None).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        };
        let log = |n: usize| st.audit_log(None, String::new(), 20).into_iter().map(|e| e.sql).filter(|q| !q.starts_with("KILL") && !q.starts_with("select pg_terminate_backend")).take(n).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        kill().await;
        let e = run(format!("update {s}.t set v = 11 where id = 2")).await.unwrap_err().to_string();
        assert!(e.contains("connection lost") && e.contains("rolled back by the server"), "{e}");
        assert!(!st.tx_state("m".into()).await);
        // The tab reconnects (still Manual): a new change and its COMMIT
        run(format!("update {s}.t set v = 12 where id = 3")).await.unwrap();
        st.commit_tx("m".into()).await.unwrap();
        assert_eq!(
            log(5),
            ["COMMIT".to_string(), format!("update {s}.t set v = 12 where id = 3"), BY_SERVER.into(), format!("update {s}.t set v = 11 where id = 2"), format!("update {s}.t set v = 10 where id = 1")]
        );
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[12]]");
        // COMMIT on a lost connection: it may or may not have committed
        run(format!("update {s}.t set v = 13 where id = 1")).await.unwrap();
        kill().await;
        let e = st.commit_tx("m".into()).await.unwrap_err().to_string();
        assert!(e.contains("unknown whether") && e.contains("Check"), "{e}");
        assert_eq!(log(2), [COMMIT_UNKNOWN.to_string(), format!("update {s}.t set v = 13 where id = 1")]);
        let cut = st.audit_log(None, String::new(), 1).remove(0);
        assert!(cut.error.is_some() && !cut.started, "{cut:?}");
        assert!(!st.tx_state("m".into()).await);
        run("select 1".into()).await.unwrap();
        st.rollback_tx("m".into()).await.unwrap();
        followup_drop(&st, kind, s).await;
    }

    #[tokio::test]
    #[ignore]
    async fn pg_lost_connection_is_logged() {
        lost_connection_is_logged(DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_lost_connection_is_logged() {
        lost_connection_is_logged(DbKind::Mysql).await;
    }

    /// MySQL: a deadlock on the change itself (no snapshot: INSERT … ON DUPLICATE KEY UPDATE) rolls back the whole Manual transaction.
    /// The console no longer shows an open transaction, and the log closes it as rolled back by the server
    #[tokio::test]
    #[ignore]
    async fn mysql_change_deadlock_closes_the_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_dead";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("m", format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        // o holds more rows, so InnoDB picks m as the deadlock victim
        run("o", format!("update {s}.t set v = 20 where id in (2, 3)")).await.unwrap();
        let m = async {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            run("m", format!("insert into {s}.t values (2, 0) on duplicate key update v = 11")).await
        };
        let (o, m) = tokio::join!(run("o", format!("update {s}.t set v = 21 where id = 1")), m);
        o.unwrap();
        let e = m.unwrap_err().to_string();
        assert!(e.contains("Deadlock"), "{e}");
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(st.audit_log(None, String::new(), 2)[0].sql, BY_SERVER);
        st.commit_tx("o".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[21],[20],[20]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual console with no transaction open: a snapshot cancelled after it locked some rows leaves no transaction
    /// (and so no row locks) behind
    #[tokio::test]
    #[ignore]
    async fn mysql_cancelled_snapshot_leaves_no_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_cancel";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("o", format!("update {s}.t set v = 30 where id = 3")).await.unwrap();
        st.set_tx_mode("d".into(), "c".into(), true).await.unwrap();
        // The snapshot locks row 2, then waits for row 3
        let cancel = async {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            st.cancel_query("c".into()).await.unwrap()
        };
        let (r, _) = tokio::join!(run("c", format!("update {s}.t set v = 0 where id >= 2")), cancel);
        let e = r.unwrap_err().to_string();
        assert!(e.starts_with("Not run: ") && !e.contains("lost"), "{e}");
        assert!(!st.tx_state("c".into()).await);
        // The row lock the snapshot took is gone: another connection gets it at once. (Not information_schema.innodb_trx: InnoDB
        // refreshes it only after 100 ms without a read, server-wide, so while other tests read it, it still lists a transaction
        // that already rolled back)
        let mut probe = crate::testenv::my_conn(&crate::testenv::my().unwrap()).await;
        sqlx::raw_sql("set session innodb_lock_wait_timeout = 1; begin").execute(&mut probe).await.unwrap();
        let free = sqlx::raw_sql(&format!("select id from {s}.t where id = 2 for update")).execute(&mut probe).await;
        sqlx::raw_sql("rollback").execute(&mut probe).await.unwrap();
        assert!(free.is_ok(), "row 2 is still locked: {free:?}");
        st.rollback_tx("o".into()).await.unwrap();
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual console whose transaction Qurry didn't see open before (WITH … DELETE, SELECT … FOR UPDATE): a cancelled snapshot
    /// keeps it, with the uncommitted delete and the row locks, instead of rolling it back without a word
    #[tokio::test]
    #[ignore]
    async fn mysql_cancelled_snapshot_keeps_the_open_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_keep";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("o", format!("update {s}.t set v = 30 where id = 3")).await.unwrap();
        st.set_tx_mode("d".into(), "c".into(), true).await.unwrap();
        let open_trx = || async {
            let pid = st.sessions.read().await.get("c").unwrap().pid;
            let r = st.run_query("d".into(), format!("select count(*) from information_schema.innodb_trx where trx_mysql_thread_id = {pid}"), None, None).await.unwrap();
            serde_json::to_string(&r.rows).unwrap()
        };
        // The snapshot locks row 2, then waits for row 3 until it's cancelled
        let cancelled = || async {
            let cancel = async {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                st.cancel_query("c".into()).await.unwrap()
            };
            let (r, _) = tokio::join!(run("c", format!("update {s}.t set v = 0 where id >= 2")), cancel);
            let e = r.unwrap_err().to_string();
            assert!(e.starts_with("Not run: "), "{e}");
        };
        let mut firsts = vec![format!("select v from {s}.t where id = 1 for update")];
        if mysql_at_least(&st, [8, 0, 0]).await {
            firsts.insert(0, format!("with x as (select 1 as id) delete t from {s}.t t join x using (id)"));
        }
        for first in firsts {
            run("c", first.clone()).await.unwrap();
            assert!(st.tx_state("c".into()).await, "{first}");
            if first.starts_with("with") {
                // Logged as not committed yet, so its Roll back is logged too
                assert!(st.audit_log(None, String::new(), 1)[0].in_tx);
            }
            cancelled().await;
            assert!(st.tx_state("c".into()).await, "{first}");
            assert_eq!(open_trx().await, "[[1]]", "{first}");
            let seen = run("c", format!("select id from {s}.t order by id")).await.unwrap();
            let want = if first.starts_with("with") { "[[2],[3]]" } else { "[[1],[2],[3]]" };
            assert_eq!(serde_json::to_string(&seen.rows).unwrap(), want, "{first}");
            st.rollback_tx("c".into()).await.unwrap();
        }
        st.rollback_tx("o".into()).await.unwrap();
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// Forwards 127.0.0.1:<port> to `target` until the task is aborted, which also drops every forwarded connection (like a dead SSH tunnel)
    async fn proxy(target: String) -> (u16, tokio::task::JoinHandle<()>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let h = tokio::spawn(async move {
            let mut conns = tokio::task::JoinSet::new();
            while let Ok((mut c, _)) = l.accept().await {
                let target = target.clone();
                conns.spawn(async move {
                    if let Ok(mut s) = tokio::net::TcpStream::connect(target).await {
                        let _ = tokio::io::copy_bidirectional(&mut c, &mut s).await;
                    }
                });
            }
        });
        (port, h)
    }

    /// The connection to the server goes away for every console (the tunnel dies) while tab b has a logged change in a Manual transaction.
    /// Tab a hits the error and the pool is dropped: b's transaction is closed in the log too, so a later COMMIT can't read as committing it
    async fn dead_pool_closes_every_console(kind: DbKind) {
        let Some(direct) = prod_state(kind).await else { return };
        let s = "audit_followups_pool";
        followup_table(&direct, kind, s).await;
        let crate::testenv::Server { mut cfg, password } = crate::testenv::server(kind).unwrap();
        let (port, tunnel) = proxy(format!("{}:{}", cfg.host, cfg.port)).await;
        (cfg.host, cfg.port) = ("127.0.0.1".into(), port);
        let st = prod_state_at(cfg, password).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        st.set_tx_mode("d".into(), "a".into(), true).await.unwrap();
        st.set_tx_mode("d".into(), "b".into(), true).await.unwrap();
        run("b", format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        tunnel.abort();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let e = run("a", "select 1".into()).await.unwrap_err().to_string();
        assert!(e.contains("connection lost"), "{e}");
        assert!(st.sessions.read().await.is_empty());
        let log = st.audit_log(None, String::new(), 2);
        assert_eq!(log.iter().map(|e| e.sql.as_str()).collect::<Vec<_>>(), vec![BY_SERVER.to_string(), format!("update {s}.t set v = 10 where id = 1")]);
        assert_eq!(followup_values(&direct, s).await, "[[1],[2],[3]]");
        followup_drop(&direct, kind, s).await;
    }

    #[tokio::test]
    #[ignore]
    async fn pg_dead_pool_closes_every_console() {
        dead_pool_closes_every_console(DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_dead_pool_closes_every_console() {
        dead_pool_closes_every_console(DbKind::Mysql).await;
    }

    /// Disconnect (Deactivate › Disconnect and Roll Back) with a logged change uncommitted: the log closes the transaction, so a
    /// COMMIT in the reconnected console doesn't read as committing it
    async fn disconnect_closes_the_logged_transaction(kind: DbKind) {
        let Some(st) = prod_state(kind).await else { return };
        let s = "audit_followups_disc";
        followup_table(&st, kind, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        st.disconnect("d".into()).await.unwrap();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 12 where id = 3")).await.unwrap();
        st.commit_tx("m".into()).await.unwrap();
        let log = st.audit_log(None, String::new(), 4).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        assert_eq!(log, ["COMMIT".to_string(), format!("update {s}.t set v = 12 where id = 3"), DISCONNECTED.into(), format!("update {s}.t set v = 10 where id = 1")]);
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[12]]");
        followup_drop(&st, kind, s).await;
    }

    #[tokio::test]
    #[ignore]
    async fn pg_disconnect_closes_the_logged_transaction() {
        disconnect_closes_the_logged_transaction(DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_disconnect_closes_the_logged_transaction() {
        disconnect_closes_the_logged_transaction(DbKind::Mysql).await;
    }

    /// MySQL Manual: a statement that fails still leaves the transaction open (autocommit=0), with the row locks it took before a
    /// lock wait timeout or what a procedure changed before it failed. The console shows it, so switching to Auto doesn't commit it silently
    #[tokio::test]
    #[ignore]
    async fn mysql_failed_statement_keeps_the_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_failed";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("o", format!("update {s}.t set v = 30 where id = 3")).await.unwrap();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run("m", "set innodb_lock_wait_timeout = 1".into()).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        // Locks row 2, times out on row 3 (no snapshot: not a simple single-table change)
        let e = run("m", format!("update {s}.t t join (select 2 as id union select 3) x using (id) set t.v = 0")).await.unwrap_err().to_string();
        assert!(e.contains("Lock wait timeout"), "{e}");
        assert!(st.tx_state("m".into()).await);
        assert!(st.set_tx_mode("d".into(), "m".into(), false).await.is_err());
        st.rollback_tx("m".into()).await.unwrap();
        st.rollback_tx("o".into()).await.unwrap();
        // A procedure that changes a row, then fails
        run("m", format!("create procedure {s}.p() begin update {s}.t set v = 77 where id = 1; signal sqlstate '45000' set message_text = 'nope'; end")).await.unwrap();
        let e = run("m", format!("call {s}.p()")).await.unwrap_err().to_string();
        assert!(e.contains("nope"), "{e}");
        assert!(st.tx_state("m".into()).await);
        assert!(st.set_tx_mode("d".into(), "m".into(), false).await.is_err());
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        // A typo doesn't open one
        run("m", "selec 1".into()).await.unwrap_err();
        assert!(!st.tx_state("m".into()).await);
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: DDL, BEGIN and a typed SET autocommit = 1 commit the open transaction on the server. The console no longer offers
    /// to roll it back, and the log records the commit instead of a later ROLLBACK that wouldn't undo anything
    #[tokio::test]
    #[ignore]
    async fn mysql_implicit_commit_is_logged() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_implicit";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        let w = run(format!("alter table {s}.t add column c int")).await.unwrap().warning.unwrap_or_default();
        assert!(w.contains("committed the open transaction") && w.contains("saved"), "{w}");
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(3), [IMPLICIT_COMMIT.to_string(), format!("alter table {s}.t add column c int"), format!("update {s}.t set v = 10 where id = 1")]);
        run(format!("update {s}.t set v = 20 where id = 2")).await.unwrap();
        run("begin".into()).await.unwrap();
        assert!(st.tx_state("m".into()).await);
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        // A trailing comment keeps it a BEGIN (the splitter leaves a same-line comment in the statement)
        for begin in ["begin -- next batch", "begin /* x */ work"] {
            run(format!("update {s}.t set v = 20 where id = 2")).await.unwrap();
            run(begin.into()).await.unwrap();
            assert!(st.tx_state("m".into()).await, "{begin}");
            assert_eq!(log(2), [IMPLICIT_COMMIT.to_string(), format!("update {s}.t set v = 20 where id = 2")], "{begin}");
        }
        run(format!("update {s}.t set v = 30 where id = 3")).await.unwrap();
        run("set autocommit = 1".into()).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        // Nothing was left to roll back
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        assert_eq!(serde_json::to_string(&st.run_query("d".into(), format!("select v from {s}.t order by id"), None, None).await.unwrap().rows).unwrap(), "[[10],[20],[30]]");
        // The session follows the server: now Auto, a change commits right away
        run(format!("update {s}.t set v = 40 where id = 3")).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: the server commits the open transaction before DDL and LOCK TABLES even when they then fail, but not when they fail
    /// while parsing or resolving names, nor for temporary tables or MariaDB's ANALYZE <statement>. Where it committed, the console no
    /// longer offers to roll it back, the log records the commit, and a later ROLLBACK doesn't claim to undo it; elsewhere it stays open.
    /// QURRY_TEST_MY can point at MariaDB too (its database must exist)
    #[tokio::test]
    #[ignore]
    async fn mysql_failed_ddl_still_commits() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_ddl";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        // 1060: duplicate column name
        let e = run(format!("alter table {s}.t add column v int")).await.unwrap_err().to_string();
        assert!(e.contains("1060") && e.contains("committed the open transaction") && e.contains("Production log"), "{e}");
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(3), [IMPLICIT_COMMIT.to_string(), format!("alter table {s}.t add column v int"), format!("update {s}.t set v = 10 where id = 1")]);
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        assert_eq!(followup_values(&st, s).await, "[[10],[2],[3]]");
        // 1146: LOCK TABLES of a table that doesn't exist
        run(format!("update {s}.t set v = 20 where id = 2")).await.unwrap();
        run(format!("lock tables {s}.missing write")).await.unwrap_err();
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        // These leave the transaction open: a DDL that doesn't parse (1064), a name too long (1059), temporary tables however spaced,
        // MariaDB's CREATE OR REPLACE TEMPORARY and ANALYZE UPDATE (MySQL: 1064)
        let long = "c".repeat(70);
        for ddl in [
            format!("alter table {s}.t add column"),
            format!("alter table {s}.t add column {long} int"),
            format!("lock tables {s}.{long} write"),
            format!("create  temporary table {s}.tt1 (a int)"),
            format!("create\ntemporary table {s}.tt2 (a int)"),
            format!("create or replace temporary table {s}.tt3 (a int)"),
            format!("analyze update {s}.t set v = 40 where id = 3"),
        ] {
            run(format!("update {s}.t set v = 30 where id = 3")).await.unwrap();
            if let Err(e) = run(ddl.clone()).await {
                assert!(!e.to_string().contains("committed"), "{ddl}: {e}");
            }
            assert!(st.tx_state("m".into()).await, "{ddl}");
            assert_ne!(log(1), [IMPLICIT_COMMIT], "{ddl}");
            st.rollback_tx("m".into()).await.unwrap();
            assert_eq!(log(1), ["ROLLBACK"], "{ddl}");
            assert_eq!(followup_values(&st, s).await, "[[10],[20],[3]]", "{ddl}");
        }
        // Not a Production source: nothing logged, so the error doesn't point to the Production log
        let mut dev = st.source("d").unwrap();
        (dev.id, dev.name, dev.options.confirm_changes) = ("n".into(), "dev".into(), false);
        let pass = crate::testenv::my().unwrap().password;
        st.save_connection(dev, pass, None).await.unwrap();
        st.set_tx_mode("n".into(), "dev".into(), true).await.unwrap();
        st.run_query("n".into(), format!("update {s}.t set v = 50 where id = 3"), None, Some("dev".into())).await.unwrap();
        let e = st.run_query("n".into(), format!("alter table {s}.t add column v int"), None, Some("dev".into())).await.unwrap_err().to_string();
        assert!(e.contains("committed the open transaction") && !e.contains("Production log"), "{e}");
        st.close_session("dev".into()).await.unwrap();
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL: whether a statement committed the open transaction is the server's answer, not a guess from its words. SET autocommit = 1
    /// with autocommit already on, UNLOCK TABLES after BEGIN (which released the locks) and a COMMIT that doesn't parse leave it open;
    /// SET PASSWORD commits it though no word list has it
    #[tokio::test]
    #[ignore]
    async fn mysql_server_says_whether_it_committed() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_server";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        // Auto mode: BEGIN, a change, then a statement that leaves the transaction open
        for (pre, stmt) in [(None, "set autocommit = 1"), (None, "set @note = 'autocommit'"), (Some(format!("lock tables {s}.t write")), "unlock tables")] {
            if let Some(p) = pre {
                run(p).await.unwrap();
            }
            run("begin".into()).await.unwrap();
            run(format!("update {s}.t set v = 5 where id = 1")).await.unwrap();
            run(stmt.into()).await.unwrap();
            assert!(st.tx_state("m".into()).await, "{stmt}");
            assert_ne!(log(1), [IMPLICIT_COMMIT], "{stmt}");
            st.rollback_tx("m".into()).await.unwrap();
            assert_eq!(log(1), ["ROLLBACK"], "{stmt}");
            assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]", "{stmt}");
        }
        // Manual: COMMIT TRANSACTION doesn't parse (1064) and commits nothing
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 6 where id = 2")).await.unwrap();
        let e = run("commit transaction".into()).await.unwrap_err().to_string();
        assert!(e.contains("1064"), "{e}");
        assert!(st.tx_state("m".into()).await);
        assert!(st.set_tx_mode("d".into(), "m".into(), false).await.is_err());
        assert_ne!(log(1), [BY_SERVER]);
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        // Manual: SET PASSWORD commits it
        let user = format!("'{s}'@'localhost'");
        st.run_query("d".into(), format!("create user {user} identified by 'a'"), None, None).await.unwrap();
        run(format!("update {s}.t set v = 7 where id = 3")).await.unwrap();
        // MariaDB takes a hash there unless it's PASSWORD('…') (1372)
        let pw = if server_version(&st).await.contains("MariaDB") { "password('b')" } else { "'b'" };
        run(format!("set password for {user} = {pw}")).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[7]]");
        st.run_query("d".into(), format!("drop user {user}"), None, None).await.unwrap();
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: SET PASSWORD (1133), ALTER USER (1396) and INSTALL PLUGIN (1126) commit the open transaction even when they fail.
    /// The rule is the same as for a statement that ran: the console asks the server, so it shows no open transaction, the error says
    /// the changes are saved, and the log records the commit
    #[tokio::test]
    #[ignore]
    async fn mysql_failed_statement_that_commits() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_failcommit";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        let nobody = "'qurry_nobody'@'localhost'";
        for (i, (code, stmt)) in [
            ("1133", format!("set password for {nobody} = 'b'")),
            ("1396", format!("alter user {nobody} identified by 'b'")),
            ("1126", "install plugin qurry_nope soname 'qurry_nope.so'".to_string()),
        ]
        .into_iter()
        .enumerate()
        {
            run(format!("update {s}.t set v = {} where id = 1", 10 + i)).await.unwrap();
            let e = run(stmt.clone()).await.unwrap_err().to_string();
            assert!(e.contains(code) && e.contains("committed the open transaction"), "{stmt}: {e}");
            assert!(!st.tx_state("m".into()).await, "{stmt}");
            assert_eq!(log(1), [IMPLICIT_COMMIT], "{stmt}");
            st.rollback_tx("m".into()).await.unwrap();
            assert_eq!(log(1), [IMPLICIT_COMMIT], "{stmt}");
            assert_eq!(followup_values(&st, s).await, format!("[[{}],[2],[3]]", 10 + i), "{stmt}");
        }
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: a DDL that times out on its metadata lock (1205: another console's transaction read the table) has already
    /// committed the open transaction. The console shows none open, and neither the error nor the log says it was rolled back: the
    /// error number can't tell (a deadlock in a SET does roll it back), so both say to check the data
    #[tokio::test]
    #[ignore]
    async fn mysql_ddl_lock_timeout_after_commit() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_mdl";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        run("o", format!("create table {s}.u (a int)")).await.unwrap();
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("o", format!("select * from {s}.u")).await.unwrap();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run("m", "set lock_wait_timeout = 1".into()).await.unwrap();
        run("m", format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        let e = run("m", format!("alter table {s}.u add column c int")).await.unwrap_err().to_string();
        assert!(e.contains("1205") && e.contains("committed or rolled back") && e.contains("check the data") && !e.contains("are saved"), "{e}");
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        st.rollback_tx("o".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[10],[2],[3]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: a CALL whose procedure rolls back (an EXIT HANDLER with ROLLBACK; RESIGNAL, or a plain ROLLBACK) or commits ends
    /// the open transaction either way. The console shows none open, and the error (or the warning, when it ran) and the log say it
    /// may have committed or rolled back the changes, not that they are saved
    #[tokio::test]
    #[ignore]
    async fn mysql_call_that_ends_the_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_call";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        // Created before the transaction (CREATE PROCEDURE commits it)
        for p in [
            format!("create procedure {s}.p() begin declare exit handler for sqlexception begin rollback; resignal; end; insert into {s}.nope values (1); end"),
            format!("create procedure {s}.r() begin rollback; end"),
            format!("create procedure {s}.c() begin commit; end"),
        ] {
            st.run_query("d".into(), p, None, None).await.unwrap();
        }
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run(format!("update {s}.t set v = 50 where id = 2")).await.unwrap();
        let e = run(format!("call {s}.p()")).await.unwrap_err().to_string();
        assert!(e.contains("1146") && e.contains("committed or rolled back") && e.contains("check the data") && !e.contains("are saved"), "{e}");
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        run(format!("update {s}.t set v = 60 where id = 3")).await.unwrap();
        let w = run(format!("call {s}.r()")).await.unwrap().warning.unwrap_or_default();
        assert!(w.contains("committed or rolled back") && w.contains("check the data") && !w.contains("are saved"), "{w}");
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        run(format!("update {s}.t set v = 70 where id = 1")).await.unwrap();
        run(format!("call {s}.c()")).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        assert_eq!(followup_values(&st, s).await, "[[70],[2],[3]]");
        // Also with a comment glued to CALL
        run(format!("update {s}.t set v = 71 where id = 2")).await.unwrap();
        run(format!("call/**/{s}.r()")).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        assert_eq!(followup_values(&st, s).await, "[[70],[2],[3]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Auto: a CALL whose procedure starts a transaction and doesn't commit leaves it open on the server. The console shows it
    /// open, so the changes after it are logged as not committed yet and Roll back undoes them (before, it showed none open)
    #[tokio::test]
    #[ignore]
    async fn mysql_auto_call_that_opens_a_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_auto_call";
        followup_table(&st, DbKind::Mysql, s).await;
        st.run_query("d".into(), format!("create procedure {s}.o() begin start transaction; update {s}.t set v = 80 where id = 1; end"), None, None).await.unwrap();
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("a".into()));
        run(format!("call {s}.o()")).await.unwrap();
        assert!(st.tx_state("a".into()).await);
        run(format!("update {s}.t set v = 90 where id = 2")).await.unwrap();
        assert!(st.tx_state("a".into()).await);
        assert!(st.audit_log(None, String::new(), 1)[0].in_tx);
        st.rollback_tx("a".into()).await.unwrap();
        assert!(!st.tx_state("a".into()).await);
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        // Also with a comment glued to CALL
        run(format!("call/**/{s}.o()")).await.unwrap();
        assert!(st.tx_state("a".into()).await);
        st.rollback_tx("a".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: SET, DO and SET NAMES leave the open transaction open: the console still offers Commit / Roll back and the log
    /// doesn't record a commit. Also on TiDB, which accepts the MySQL question inside a transaction: the session finds that out when it
    /// opens and doesn't ask, so it keeps the transaction open. QURRY_TEST_MY can point at TiDB (user root, empty password: host:port:root::db)
    #[tokio::test]
    #[ignore]
    async fn mysql_set_keeps_the_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_set";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        for stmt in ["set @x = 1", "do 1", "set names utf8mb4"] {
            run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
            run(stmt.into()).await.unwrap();
            assert!(st.tx_state("m".into()).await, "{stmt}");
            assert_ne!(log(1), [IMPLICIT_COMMIT], "{stmt}");
            st.rollback_tx("m".into()).await.unwrap();
            assert_eq!(log(1), ["ROLLBACK"], "{stmt}");
            assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]", "{stmt}");
        }
        // DDL still commits it. TiDB can't say so: the transaction stays shown open, and Commit closes it
        run(format!("update {s}.t set v = 20 where id = 2")).await.unwrap();
        run(format!("alter table {s}.t add column c int")).await.unwrap();
        let version = st.run_query("d".into(), "select version()".into(), None, None).await.unwrap().rows[0][0].to_string();
        if version.contains("TiDB") {
            assert!(st.tx_state("m".into()).await);
            st.commit_tx("m".into()).await.unwrap();
        } else {
            assert_eq!(log(1), [IMPLICIT_COMMIT]);
        }
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(followup_values(&st, s).await, "[[1],[20],[3]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: after a SELECT (or a failed change) the console counts a transaction as open without knowing the server has one.
    /// A failing SET or DDL after it doesn't claim that changes were committed: there were none
    #[tokio::test]
    #[ignore]
    async fn mysql_guessed_transaction_claims_no_commit() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_guess";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        for (before, then, failing) in [
            ("select 1".to_string(), None, "set sql_mod = ''".to_string()),
            (format!("update {s}.missing set v = 1"), None, format!("alter table {s}.t add column v int")),
            // The SELECT's read view is an open transaction on the server, which the SET after it asks about: still nothing changed
            (format!("select * from {s}.t"), Some("set @qurry_note = 1"), format!("alter table {s}.t add column v int")),
        ] {
            run(before.clone()).await.ok();
            if let Some(t) = then {
                run(t.into()).await.unwrap();
            }
            assert!(st.tx_state("m".into()).await, "{before}");
            let e = run(failing.clone()).await.unwrap_err().to_string();
            assert!(!e.contains("committed") && !e.contains("saved"), "{before} / {failing}: {e}");
            st.rollback_tx("m".into()).await.unwrap();
        }
        // After a change it does (when the server answers), a change written as WITH … DELETE too (MySQL 8.0+)
        let with_delete = mysql_at_least(&st, [8, 0, 0]).await;
        let mut changes = vec![format!("update {s}.t set v = 30 where id = 3")];
        if with_delete {
            changes.push(format!("with x as (select 2 as id) delete from {s}.t where id in (select id from x)"));
        }
        for change in changes {
            run(change.clone()).await.unwrap();
            let e = run(format!("alter table {s}.t add column v int")).await.unwrap_err().to_string();
            assert!(e.contains("committed the open transaction") && e.contains("saved"), "{change}: {e}");
        }
        assert_eq!(followup_values(&st, s).await, if with_delete { "[[1],[30]]" } else { "[[1],[2],[30]]" });
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MySQL Manual: asking the server whether a transaction is open doesn't touch the next transaction's settings. SET TRANSACTION
    /// ISOLATION LEVEL READ UNCOMMITTED, then other statements the console asks after, and the next SELECT still reads another
    /// console's uncommitted change
    #[tokio::test]
    #[ignore]
    async fn mysql_tx_probe_keeps_set_transaction() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_isolation";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        st.set_tx_mode("d".into(), "o".into(), true).await.unwrap();
        run("o", format!("update {s}.t set v = 99 where id = 1")).await.unwrap();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run("m", "set transaction isolation level read uncommitted".into()).await.unwrap();
        run("m", "set @qurry_note = 1".into()).await.unwrap();
        run("m", "do 0".into()).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        let r = run("m", format!("select v from {s}.t where id = 1")).await.unwrap();
        assert_eq!(serde_json::to_string(&r.rows).unwrap(), "[[99]]");
        assert!(st.tx_state("m".into()).await);
        st.rollback_tx("m".into()).await.unwrap();
        st.rollback_tx("o".into()).await.unwrap();
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// PostgreSQL: COMMIT of a transaction an error aborted is a ROLLBACK on the server. Qurry says so (not committed) and logs
    /// it as rolled back by the server, for the Commit button and a typed COMMIT
    #[tokio::test]
    #[ignore]
    async fn pg_commit_of_an_aborted_transaction() {
        let Some(st) = prod_state(DbKind::Postgres).await else { return };
        let s = "audit_followups_aborted";
        followup_table(&st, DbKind::Postgres, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| (e.sql, e.error.is_some())).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        for commit in ["button", "commit", "end"] {
            run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
            run("select 1 / 0".into()).await.unwrap_err();
            assert!(st.tx_state("m".into()).await);
            let e = match commit {
                "button" => st.commit_tx("m".into()).await.unwrap_err(),
                typed => run(typed.into()).await.unwrap_err(),
            }
            .to_string();
            assert!(e.contains("Not committed") && e.contains("rolled back"), "{commit}: {e}");
            assert!(!st.tx_state("m".into()).await);
            let l = log(2);
            assert_eq!(l[0], (BY_SERVER.to_string(), false), "{commit}: {l:?}");
            assert!(l[1].1, "{commit}: {l:?}");
            assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        }
        // A transaction that recovered with ROLLBACK TO SAVEPOINT commits as usual
        run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        run("savepoint a".into()).await.unwrap();
        run("select 1 / 0".into()).await.unwrap_err();
        run("rollback to savepoint a".into()).await.unwrap();
        st.commit_tx("m".into()).await.unwrap();
        assert_eq!(log(1), [("COMMIT".to_string(), false)]);
        assert_eq!(followup_values(&st, s).await, "[[10],[2],[3]]");
        followup_drop(&st, DbKind::Postgres, s).await;
    }

    /// MySQL 8.0.24+ ends an idle session (wait_timeout) with error 4031: that is a lost connection, so the logged change is closed
    /// as rolled back by the server and the console reconnects. Older servers and MariaDB drop the socket without 4031: the same
    #[tokio::test]
    #[ignore]
    async fn mysql_idle_timeout_is_a_lost_connection() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        let s = "audit_followups_idle";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |sql: String| st.run_query("d".into(), sql, None, Some("m".into()));
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run("set session wait_timeout = 1".into()).await.unwrap();
        run(format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        let e = run("select 1".into()).await.unwrap_err().to_string();
        assert!(e.contains("rolled back by the server"), "{e}");
        if mysql_at_least(&st, [8, 0, 24]).await {
            assert!(e.contains("4031"), "{e}");
        }
        assert_eq!(st.audit_log(None, String::new(), 1)[0].sql, BY_SERVER);
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }

    /// MariaDB: BEGIN NOT ATOMIC … END is a compound statement, not BEGIN. In Manual it doesn't commit the open transaction: no
    /// 'committed' warning, no COMMIT (implicit) in the log, and Roll back undoes the change and is logged. One that commits inside
    /// ended the transaction either way for all Qurry can tell; in Auto, one that starts a transaction leaves it open.
    /// QURRY_TEST_MY pointing at MariaDB (skipped on MySQL)
    #[tokio::test]
    #[ignore]
    async fn mariadb_begin_not_atomic_is_not_begin() {
        let Some(st) = prod_state(DbKind::Mysql).await else { return };
        if !server_version(&st).await.contains("MariaDB") {
            return;
        }
        let s = "audit_followups_not_atomic";
        followup_table(&st, DbKind::Mysql, s).await;
        let run = |tab: &str, sql: String| st.run_query("d".into(), sql, None, Some(tab.into()));
        let log = |n: usize| st.audit_log(None, String::new(), n).into_iter().map(|e| e.sql).collect::<Vec<_>>();
        st.set_tx_mode("d".into(), "m".into(), true).await.unwrap();
        run("m", format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        let r = run("m", "BEGIN NOT ATOMIC SELECT 1; END".into()).await.unwrap();
        assert_eq!(r.warning, None);
        assert!(st.tx_state("m".into()).await);
        assert_ne!(log(1), [IMPLICIT_COMMIT]);
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(log(1), ["ROLLBACK"]);
        assert_eq!(followup_values(&st, s).await, "[[1],[2],[3]]");
        // A plain BEGIN still commits it
        run("m", format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
        run("m", "begin work;".into()).await.unwrap();
        assert_eq!(log(1), [IMPLICIT_COMMIT]);
        st.rollback_tx("m".into()).await.unwrap();
        // ... also with a comment after it, wherever it is; a comment inside BEGIN NOT ATOMIC doesn't make it one
        for b in ["begin -- next batch", "begin/* x */", "BEGIN; -- note", "begin # x"] {
            run("m", format!("update {s}.t set v = 10 where id = 1")).await.unwrap();
            run("m", b.into()).await.unwrap();
            assert_eq!(log(1), [IMPLICIT_COMMIT], "{b}");
            st.rollback_tx("m".into()).await.unwrap();
        }
        for b in ["begin/*x*/not atomic select 1; end", "begin # x\n not atomic select 1; end"] {
            run("m", format!("update {s}.t set v = 11 where id = 1")).await.unwrap();
            run("m", b.into()).await.unwrap();
            assert_ne!(log(1), [IMPLICIT_COMMIT], "{b}");
            st.rollback_tx("m".into()).await.unwrap();
            assert_eq!(followup_values(&st, s).await, "[[10],[2],[3]]", "{b}");
        }
        // In Auto a plain BEGIN with a comment opens a transaction
        run("a", "begin/* x */".into()).await.unwrap();
        assert!(st.tx_state("a".into()).await);
        st.rollback_tx("a".into()).await.unwrap();
        // One that commits inside: the console can't tell committed from rolled back
        run("m", format!("update {s}.t set v = 20 where id = 2")).await.unwrap();
        run("m", "begin not atomic commit; end".into()).await.unwrap();
        assert!(!st.tx_state("m".into()).await);
        assert_eq!(log(1), [ENDED_UNKNOWN]);
        assert_eq!(followup_values(&st, s).await, "[[10],[20],[3]]");
        // Auto: a transaction it starts stays open, and Roll back undoes it
        run("a", format!("begin not atomic start transaction; update {s}.t set v = 30 where id = 3; end")).await.unwrap();
        assert!(st.tx_state("a".into()).await);
        st.rollback_tx("a".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[10],[20],[3]]");
        // One that changes rows is logged and tracked like a CALL: a DDL that then commits it says so and logs the commit
        let change = format!("begin not atomic update {s}.t set v = 40 where id = 1; end");
        run("m", change.clone()).await.unwrap();
        let ddl = format!("create table {s}.x (a int)");
        let w = run("m", ddl.clone()).await.unwrap().warning.unwrap_or_default();
        assert!(w.contains("are saved"), "{w}");
        assert_eq!(log(3), [IMPLICIT_COMMIT.to_string(), ddl, change]);
        st.rollback_tx("m".into()).await.unwrap();
        assert_eq!(followup_values(&st, s).await, "[[40],[20],[3]]");
        followup_drop(&st, DbKind::Mysql, s).await;
    }
}
