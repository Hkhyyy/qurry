//! Data export (CSV · TSV · JSON · SQL INSERT · Markdown, file/clipboard) and import (CSV · TSV · JSON · Excel → table).
//! Long-running work reports progress through a Job and can be cancelled
use crate::app::{AppState, ensure_pool};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Csv,
    Tsv,
    Json,
    Sql,
    Markdown,
    /// Import only: first sheet of Excel (.xlsx · .xlsm · .xls) · OpenDocument (.ods)
    Xlsx,
}

impl Format {
    pub const ALL: [Format; 5] = [Format::Csv, Format::Tsv, Format::Json, Format::Sql, Format::Markdown];

    pub fn ext(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Json => "json",
            Format::Sql => "sql",
            Format::Markdown => "md",
            Format::Xlsx => "xlsx",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Csv => "CSV",
            Format::Tsv => "TSV",
            Format::Json => "JSON",
            Format::Sql => "SQL INSERT",
            Format::Markdown => "Markdown",
            Format::Xlsx => "Excel",
        }
    }

    /// Format of the file to import (by extension)
    pub fn of_path(path: &std::path::Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "csv" => Some(Format::Csv),
            "tsv" | "tab" => Some(Format::Tsv),
            "json" => Some(Format::Json),
            "xlsx" | "xlsm" | "xls" | "ods" => Some(Format::Xlsx),
            _ => None,
        }
    }
}

/// Progress (done/total, total = 0 means unknown) and cancellation
#[derive(Default)]
pub struct Job {
    /// Rows processed
    pub rows: AtomicU64,
    /// Progress (done/total). Rows for export, file bytes for import
    pub done: AtomicU64,
    pub total: AtomicU64,
    cancel: AtomicBool,
}

impl Job {
    pub fn new() -> Arc<Job> {
        Arc::new(Job::default())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed)
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    /// (rows, done, total)
    pub fn progress(&self) -> (u64, u64, u64) {
        (self.rows.load(Ordering::Relaxed), self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed))
    }
    fn check(&self) -> AppResult<()> {
        if self.cancelled() { Err(AppError::Msg(CANCELLED.into())) } else { Ok(()) }
    }
}

pub const CANCELLED: &str = "Cancelled";

// ───────── Export: text per format ─────────

/// Builds lines per format. table = target of SQL INSERT (quoted name)
pub struct Encoder {
    pub format: Format,
    pub kind: DbKind,
    pub table: String,
    cols: Vec<String>,
    rows: u64,
}

impl Encoder {
    pub fn new(format: Format, kind: DbKind, table: impl Into<String>) -> Self {
        Encoder { format, kind, table: table.into(), cols: vec![], rows: 0 }
    }

    pub fn header(&mut self, cols: &[String]) -> String {
        self.cols = cols.to_vec();
        match self.format {
            // Excel is import only (export blocks it)
            Format::Csv | Format::Tsv | Format::Xlsx => self.delimited(cols.iter().map(|c| c.as_str())),
            Format::Json => "[".into(),
            Format::Sql => String::new(),
            Format::Markdown => {
                let names: Vec<String> = cols.iter().map(|c| md_cell(c)).collect();
                format!("| {} |\n|{}\n", names.join(" | "), " --- |".repeat(cols.len()))
            }
        }
    }

    pub fn row(&mut self, vals: &[Value]) -> String {
        self.rows += 1;
        match self.format {
            Format::Csv | Format::Tsv | Format::Xlsx => {
                let cells: Vec<String> = vals.iter().map(crate::db::cell_text).collect();
                self.delimited(cells.iter().map(|c| c.as_str()))
            }
            Format::Json => {
                let obj: serde_json::Map<String, Value> = self.cols.iter().cloned().zip(vals.iter().cloned()).collect();
                format!("{}  {}", if self.rows == 1 { "\n" } else { ",\n" }, Value::Object(obj))
            }
            Format::Sql => {
                let cols: Vec<String> = self.cols.iter().map(|c| crate::sql::quote_ident(self.kind, c)).collect();
                let vals: Vec<String> = vals.iter().map(|v| sql_literal(self.kind, v)).collect();
                format!("INSERT INTO {} ({}) VALUES ({});\n", self.table, cols.join(", "), vals.join(", "))
            }
            Format::Markdown => {
                let cells: Vec<String> = vals.iter().map(|v| md_cell(&crate::db::cell_text(v))).collect();
                format!("| {} |\n", cells.join(" | "))
            }
        }
    }

    pub fn footer(&self) -> String {
        match self.format {
            Format::Json if self.rows == 0 => "]\n".into(),
            Format::Json => "\n]\n".into(),
            _ => String::new(),
        }
    }

    pub fn rows(&self) -> u64 {
        self.rows
    }

    fn delimited<'a>(&self, cells: impl Iterator<Item = &'a str>) -> String {
        let d = if self.format == Format::Tsv { '\t' } else { ',' };
        let mut line = cells
            .map(|c| if c.contains([d, '"', '\n', '\r']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.to_string() })
            .collect::<Vec<_>>()
            .join(&d.to_string());
        line.push('\n');
        line
    }
}

/// SQL value: NULL · numbers · TRUE/FALSE as is, the rest quoted (MySQL also escapes backslashes)
pub(crate) fn sql_literal(kind: DbKind, v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        Value::Number(n) => n.to_string(),
        other => {
            let mut s = crate::db::cell_text(other).replace('\'', "''");
            if kind == DbKind::Mysql {
                s = s.replace('\\', "\\\\");
            }
            format!("'{s}'")
        }
    }
}

/// Markdown table cell: keep | and newlines from breaking the table
fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace("\r\n", "<br>").replace('\n', "<br>")
}


// ───────── Export: file / clipboard ─────────

/// Where to: a file, or text for the clipboard (in memory)
#[derive(Clone, Debug)]
pub enum Dest {
    File(std::path::PathBuf),
    Clipboard,
}

/// Up to this much for the clipboard (larger goes to a file)
const CLIPBOARD_MAX: usize = 64 << 20;

enum Out {
    File(tokio::io::BufWriter<tokio::fs::File>),
    Mem(Vec<u8>),
}

/// Writes rows in the given format. Bumps job progress per row and stops if cancelled
pub struct Exporter {
    enc: Encoder,
    out: Out,
    job: Arc<Job>,
    header: bool,
    /// Bytes that may be written to the file (MCP export). None = no limit
    max_bytes: Option<u64>,
    written: u64,
}

/// Finished export: row count and text (for the clipboard)
pub struct Exported {
    pub rows: u64,
    pub text: Option<String>,
}

impl Exporter {
    pub async fn create(enc: Encoder, dest: &Dest, job: Arc<Job>) -> AppResult<Self> {
        let out = match dest {
            Dest::File(p) => {
                let mut f = tokio::io::BufWriter::new(tokio::fs::File::create(p).await?);
                // CSV files get a BOM (so Excel doesn't garble non-ASCII text)
                if enc.format == Format::Csv {
                    tokio::io::AsyncWriteExt::write_all(&mut f, "\u{FEFF}".as_bytes()).await?;
                }
                Out::File(f)
            }
            Dest::Clipboard => Out::Mem(vec![]),
        };
        Ok(Exporter { enc, out, job, header: false, max_bytes: None, written: 0 })
    }

    /// To a CSV file (MCP · old API)
    pub async fn csv_file(path: &std::path::Path) -> AppResult<Self> {
        Self::create(Encoder::new(Format::Csv, DbKind::Postgres, ""), &Dest::File(path.into()), Job::new()).await
    }

    /// Sets the writable size (stops with an error when exceeded)
    pub fn max_bytes(mut self, n: u64) -> Self {
        self.max_bytes = Some(n);
        self
    }

    async fn write(&mut self, s: String) -> AppResult<()> {
        self.written += s.len() as u64;
        if let Some(m) = self.max_bytes.filter(|m| self.written > *m) {
            return Err(AppError::Msg(format!("The export is over {} MB — narrow the query", m >> 20)));
        }
        match &mut self.out {
            Out::File(f) => tokio::io::AsyncWriteExt::write_all(f, s.as_bytes()).await?,
            Out::Mem(v) => {
                if v.len() + s.len() > CLIPBOARD_MAX {
                    return Err(AppError::Msg(format!("Too large for the clipboard (over {} MB). Export to a file instead.", CLIPBOARD_MAX >> 20)));
                }
                v.extend_from_slice(s.as_bytes())
            }
        }
        Ok(())
    }

    pub fn has_header(&self) -> bool {
        self.header
    }

    pub async fn header(&mut self, cols: &[String]) -> AppResult<()> {
        self.header = true;
        let s = self.enc.header(cols);
        self.write(s).await
    }

    pub async fn row(&mut self, vals: &[Value]) -> AppResult<()> {
        self.job.check()?;
        let s = self.enc.row(vals);
        self.write(s).await?;
        self.job.rows.fetch_add(1, Ordering::Relaxed);
        self.job.done.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub async fn finish(mut self) -> AppResult<Exported> {
        let s = self.enc.footer();
        self.write(s).await?;
        let rows = self.enc.rows();
        let text = match self.out {
            Out::File(mut f) => {
                tokio::io::AsyncWriteExt::flush(&mut f).await?;
                None
            }
            Out::Mem(v) => Some(String::from_utf8_lossy(&v).into_owned()),
        };
        Ok(Exported { rows, text })
    }
}

// ───────── Import: reading files ─────────

/// Preview of the file to import (first few rows)
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub format: Format,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub bytes: u64,
}

/// One row (empty cells are NULL) and the bytes read up to that row
type Record = AppResult<(Vec<Option<String>>, u64)>;
type Records = Box<dyn Iterator<Item = Record> + Send>;

/// Opens the file as (column names, rows). CSV · TSV are streamed, JSON reads one array of objects whole
pub fn read_file(path: &std::path::Path) -> AppResult<(Format, Vec<String>, Records)> {
    let format = Format::of_path(path).ok_or_else(|| AppError::Msg("Import reads .csv, .tsv, .json, .xlsx, .xls or .ods files".into()))?;
    match format {
        Format::Xlsx => {
            use calamine::Reader;
            let mut wb = calamine::open_workbook_auto(path).map_err(|e| AppError::Msg(format!("Could not read the file: {e}")))?;
            let range = wb
                .worksheet_range_at(0)
                .ok_or_else(|| AppError::Msg("The workbook has no sheets".into()))?
                .map_err(|e| AppError::Msg(format!("Could not read the first sheet: {e}")))?;
            let mut rows = range.rows();
            let cols: Vec<String> = rows.next().map(|r| r.iter().enumerate().map(|(i, c)| excel_cell(c).unwrap_or_else(|| format!("column{}", i + 1))).collect()).unwrap_or_default();
            let n = cols.len();
            // Sheets are read at once → collect rows and stream them out (progress is the row ratio)
            let data: Vec<Vec<Option<String>>> = rows
                .map(|r| {
                    let mut cells: Vec<Option<String>> = r.iter().take(n).map(excel_cell).collect();
                    cells.resize(n, None);
                    cells
                })
                .filter(|c| c.iter().any(Option::is_some))
                .collect();
            let (total, size) = (data.len().max(1) as u64, std::fs::metadata(path)?.len());
            let it = data.into_iter().enumerate().map(move |(i, c)| Ok((c, size * (i as u64 + 1) / total)));
            Ok((format, cols, Box::new(it)))
        }
        Format::Json => {
            let v: Value = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
            let Value::Array(items) = v else { return Err(AppError::Msg("JSON import expects an array of objects".into())) };
            let mut cols: Vec<String> = vec![];
            for it in &items {
                let Value::Object(o) = it else { return Err(AppError::Msg("JSON import expects an array of objects".into())) };
                for k in o.keys() {
                    if !cols.contains(k) {
                        cols.push(k.clone());
                    }
                }
            }
            let (n, size) = (items.len().max(1) as u64, std::fs::metadata(path)?.len());
            let c2 = cols.clone();
            let rows = items.into_iter().enumerate().map(move |(i, it)| {
                let cells = c2.iter().map(|c| match it.get(c) {
                    None | Some(Value::Null) => None,
                    Some(Value::String(s)) => Some(s.clone()),
                    Some(other) => Some(other.to_string()),
                });
                Ok((cells.collect(), size * (i as u64 + 1) / n))
            });
            Ok((format, cols, Box::new(rows)))
        }
        _ => {
            let mut r = csv::ReaderBuilder::new().delimiter(if format == Format::Tsv { b'\t' } else { b',' }).flexible(true).from_path(path).map_err(csv_err)?;
            let cols: Vec<String> = r.headers().map_err(csv_err)?.iter().map(|h| h.trim_start_matches('\u{FEFF}').to_string()).collect();
            let n = cols.len();
            let rows = r.into_records().map(move |rec| {
                let rec = rec.map_err(csv_err)?;
                let mut cells: Vec<Option<String>> = rec.iter().map(|c| (!c.is_empty()).then(|| c.to_string())).collect();
                cells.resize(n, None);
                Ok((cells, rec.position().map(|p| p.byte()).unwrap_or(0)))
            });
            Ok((format, cols, Box::new(rows)))
        }
    }
}

/// Excel cell → text (empty · error cells are NULL). Integers without a decimal point, dates as YYYY-MM-DD[ HH:MM:SS]
fn excel_cell(c: &calamine::Data) -> Option<String> {
    use calamine::Data;
    match c {
        Data::Empty | Data::Error(_) => None,
        Data::String(s) => Some(s.clone()),
        Data::Int(i) => Some(i.to_string()),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => Some(format!("{}", *f as i64)),
        Data::Float(f) => Some(f.to_string()),
        Data::Bool(b) => Some(b.to_string()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Some(s.clone()),
        Data::DateTime(d) => {
            let t = d.as_datetime()?;
            Some(if d.is_duration() || d.as_f64() < 1.0 {
                t.format("%H:%M:%S").to_string()
            } else if t.time() == chrono::NaiveTime::MIN {
                t.format("%Y-%m-%d").to_string()
            } else {
                t.format("%Y-%m-%d %H:%M:%S").to_string()
            })
        }
    }
}

fn csv_err(e: csv::Error) -> AppError {
    AppError::Msg(format!("Could not read the file: {e}"))
}

pub fn preview(path: &std::path::Path, rows: usize) -> AppResult<Preview> {
    let (format, columns, it) = read_file(path)?;
    let rows = it.take(rows).map(|r| r.map(|x| x.0)).collect::<AppResult<Vec<_>>>()?;
    Ok(Preview { format, columns, rows, bytes: std::fs::metadata(path)?.len() })
}

// ───────── Running jobs (AppState) ─────────

/// Import: file column index → table column
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSpec {
    pub path: String,
    pub schema: String,
    pub table: String,
    /// Creates the missing table and inserts (all columns text)
    #[serde(default)]
    pub create: bool,
    pub columns: Vec<(usize, String)>,
}

enum Tx {
    Pg(sqlx::Transaction<'static, sqlx::Postgres>),
    My(sqlx::Transaction<'static, sqlx::MySql>),
}

impl Tx {
    async fn exec(&mut self, sql: &str, binds: Vec<Option<String>>) -> AppResult<()> {
        match self {
            Tx::Pg(t) => {
                let mut q = sqlx::query(sql);
                for b in binds {
                    q = q.bind(b);
                }
                q.execute(&mut **t).await?;
            }
            Tx::My(t) => {
                let mut q = sqlx::query(sql);
                for b in binds {
                    q = q.bind(b);
                }
                q.execute(&mut **t).await?;
            }
        }
        Ok(())
    }
    async fn commit(self) -> AppResult<()> {
        match self {
            Tx::Pg(t) => t.commit().await?,
            Tx::My(t) => t.commit().await?,
        }
        Ok(())
    }
}

/// Approximate PG row count (EXPLAIN estimate). Only for the progress bar, so it may be off
async fn estimate_rows(pool: &Pool, sql: &str) -> Option<u64> {
    let Pool::Pg(p) = pool else { return None };
    let head = sql.split_whitespace().next()?.to_ascii_lowercase();
    if !["select", "with", "table", "values"].contains(&head.as_str()) {
        return None;
    }
    let v: Value = sqlx::query_scalar(&format!("EXPLAIN (FORMAT JSON) {sql}")).fetch_one(p).await.ok()?;
    v.get(0)?.get("Plan")?.get("Plan Rows")?.as_f64().map(|n| n as u64)
}

impl AppState {
    /// Registers a job (for progress queries · cancellation). Removed when export/import finishes. Uses a pre-registered one if any
    pub fn start_job(&self, job_id: &str) -> Arc<Job> {
        let j = Job::new();
        self.jobs.lock().unwrap().insert(job_id.to_string(), j.clone());
        j
    }
    pub fn end_job(&self, job_id: &str) {
        self.jobs.lock().unwrap().remove(job_id);
    }
    pub fn cancel_job(&self, job_id: &str) {
        if let Some(j) = self.jobs.lock().unwrap().get(job_id) {
            j.cancel();
        }
    }
    pub fn job(&self, job_id: &str) -> Option<Arc<Job>> {
        self.jobs.lock().unwrap().get(job_id).cloned()
    }

    /// Exports the full query result. table = target of SQL INSERT (schema, name). Deletes the partial file on failure · cancel
    pub async fn export(&self, id: String, sql: String, format: Format, dest: Dest, table: Option<(String, String)>, job_id: String) -> AppResult<Exported> {
        let job = self.job(&job_id).unwrap_or_else(|| self.start_job(&job_id));
        let r = async {
            if format == Format::Xlsx {
                return Err(AppError::Msg("Excel is for import only — export as CSV to open it in Excel".into()));
            }
            self.check_read_only(&id, &sql)?;
            let kind = self.kind_of(&id)?;
            let pool = ensure_pool(self, &id).await?;
            let q = |s: &str| crate::sql::quote_ident(kind, s);
            let table = match table {
                Some((s, t)) if !s.is_empty() => format!("{}.{}", q(&s), q(&t)),
                Some((_, t)) => q(&t),
                None => "result".into(),
            };
            if let Some(n) = estimate_rows(&pool, &sql).await {
                job.total.store(n, Ordering::Relaxed);
            }
            let mut out = Exporter::create(Encoder::new(format, kind, table), &dest, job.clone()).await?;
            pool.export(&sql, &mut out).await?;
            out.finish().await
        }
        .await;
        self.end_job(&job_id);
        if let (Err(_), Dest::File(p)) = (&r, &dest) {
            let _ = std::fs::remove_file(p);
        }
        r
    }

    /// Inserts the file into a table (one transaction: nothing is inserted on failure · cancel). Returns rows inserted
    pub async fn import(&self, id: String, spec: ImportSpec, job_id: String) -> AppResult<u64> {
        let job = self.job(&job_id).unwrap_or_else(|| self.start_job(&job_id));
        let r = async {
            let cfg = self.source(&id)?;
            if cfg.read_only {
                return Err(AppError::Msg(format!("Data source \"{}\" is read-only", cfg.name)));
            }
            if spec.columns.is_empty() {
                return Err(AppError::Msg("Pick at least one column to import".into()));
            }
            self.import_with(&ensure_pool(self, &id).await?, cfg.kind, &id, &spec, &job).await
        }
        .await;
        self.end_job(&job_id);
        r
    }

    async fn import_with(&self, pool: &Pool, kind: DbKind, id: &str, spec: &ImportSpec, job: &Arc<Job>) -> AppResult<u64> {
        let q = |s: &str| crate::sql::quote_ident(kind, s);
        let table = if spec.schema.is_empty() { q(&spec.table) } else { format!("{}.{}", q(&spec.schema), q(&spec.table)) };
        // PG: send as text and cast to the column types (numbers · dates · json · enums too)
        let types: std::collections::HashMap<String, String> = if spec.create || kind != DbKind::Postgres {
            Default::default()
        } else {
            self.table_detail(id.to_string(), spec.schema.clone(), spec.table.clone()).await?.columns.into_iter().map(|c| (c.name, c.data_type)).collect()
        };
        let mut tx = match pool {
            Pool::Pg(p) => Tx::Pg(p.begin().await?),
            Pool::My(p) => Tx::My(p.begin().await?),
            Pool::Redis(..) => return Err(AppError::Msg("Import into Redis is not supported".into())),
            Pool::Plugin(c) => return Err(AppError::Msg(format!("Import is not available for the {} driver yet", c.driver))),
        };
        if spec.create {
            let ty = if kind == DbKind::Mysql { "TEXT" } else { "text" };
            let cols: Vec<String> = spec.columns.iter().map(|(_, c)| format!("{} {ty}", q(c))).collect();
            tx.exec(&format!("CREATE TABLE {table} ({})", cols.join(", ")), vec![]).await?;
        }
        let names: Vec<String> = spec.columns.iter().map(|(_, c)| q(c)).collect();
        let per = (60_000 / spec.columns.len()).clamp(1, 1000);
        // The file is read on another thread and sent in batches
        let (send, mut recv) = tokio::sync::mpsc::channel::<AppResult<(Vec<Vec<Option<String>>>, u64)>>(4);
        let path = std::path::PathBuf::from(&spec.path);
        job.total.store(std::fs::metadata(&path)?.len(), Ordering::Relaxed);
        let reader = tokio::task::spawn_blocking(move || {
            let (_, _, rows) = match read_file(&path) {
                Ok(x) => x,
                Err(e) => return drop(send.blocking_send(Err(e))),
            };
            let mut batch = vec![];
            let mut pos = 0;
            for r in rows {
                match r {
                    Ok((row, p)) => {
                        batch.push(row);
                        pos = p;
                    }
                    Err(e) => return drop(send.blocking_send(Err(e))),
                }
                if batch.len() >= per && send.blocking_send(Ok((std::mem::take(&mut batch), pos))).is_err() {
                    return;
                }
            }
            if !batch.is_empty() {
                let _ = send.blocking_send(Ok((batch, pos)));
            }
        });
        let mut done = 0u64;
        while let Some(b) = recv.recv().await {
            job.check()?;
            let (batch, pos) = b?;
            let mut n = 0;
            let mut binds = Vec::with_capacity(batch.len() * names.len());
            let groups: Vec<String> = batch
                .into_iter()
                .map(|row| {
                    let cells: Vec<String> = spec
                        .columns
                        .iter()
                        .map(|(from, to)| {
                            n += 1;
                            binds.push(row.get(*from).cloned().flatten());
                            match (kind, types.get(to)) {
                                (DbKind::Postgres, Some(t)) => format!("CAST(${n} AS {t})"),
                                (DbKind::Postgres, None) => format!("${n}"),
                                _ => "?".into(),
                            }
                        })
                        .collect();
                    format!("({})", cells.join(", "))
                })
                .collect();
            let rows = groups.len() as u64;
            let sql = format!("INSERT INTO {table} ({}) VALUES {}", names.join(", "), groups.join(", "));
            tx.exec(&sql, binds).await.map_err(|e| AppError::Msg(format!("Rows {}–{}: {e}", done + 1, done + rows)))?;
            done += rows;
            job.rows.store(done, Ordering::Relaxed);
            job.done.store(pos, Ordering::Relaxed);
        }
        let _ = reader.await;
        job.check()?;
        tx.commit().await?;
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn encode(f: Format, kind: DbKind, rows: &[Vec<Value>]) -> String {
        let mut e = Encoder::new(f, kind, "\"public\".\"t\"");
        let mut s = e.header(&["id".into(), "name".into(), "note".into()]);
        for r in rows {
            s += &e.row(r);
        }
        s + &e.footer()
    }

    fn rows() -> Vec<Vec<Value>> {
        vec![vec![json!(1), json!("a, \"b\""), Value::Null], vec![json!(2), json!("it's\tx|y\nz"), json!(true)]]
    }

    #[test]
    fn csv_and_tsv() {
        assert_eq!(encode(Format::Csv, DbKind::Postgres, &rows()), "id,name,note\n1,\"a, \"\"b\"\"\",\n2,\"it's\tx|y\nz\",true\n");
        assert_eq!(encode(Format::Tsv, DbKind::Postgres, &rows()), "id\tname\tnote\n1\t\"a, \"\"b\"\"\"\t\n2\t\"it's\tx|y\nz\"\ttrue\n");
    }

    /// Excel: first sheet, first row is column names. Integers without decimals · dates · true/false · empty cells are NULL · empty rows skipped
    #[test]
    fn reads_excel_first_sheet() {
        let dir = std::env::temp_dir().join(format!("qurry-xlsx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("users.xlsx");
        std::fs::write(&path, include_bytes!("testdata/sample.xlsx")).unwrap();
        let p = preview(&path, 10).unwrap();
        assert_eq!(p.format, Format::Xlsx);
        assert_eq!(p.columns, vec!["id", "name", "price", "joined", "active", "seen"]);
        let s = |v: &str| Some(v.to_string());
        assert_eq!(p.rows[0], vec![s("1"), s("김철수"), s("1.5"), s("2026-10-29"), s("true"), s("2026-10-29 12:00:00")]);
        assert_eq!(p.rows[1], vec![s("2"), None, s("20"), None, s("false"), None]);
        assert_eq!(p.rows[2], vec![s("3"), s("lee"), None, None, None, None]);
        assert_eq!(p.rows.len(), 3);
        assert_eq!(Format::of_path(std::path::Path::new("a.XLS")), Some(Format::Xlsx));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn json_keeps_types() {
        let s = encode(Format::Json, DbKind::Postgres, &rows());
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v, json!([{ "id": 1, "name": "a, \"b\"", "note": null }, { "id": 2, "name": "it's\tx|y\nz", "note": true }]));
        assert_eq!(encode(Format::Json, DbKind::Postgres, &[]).trim(), "[]");
    }

    #[test]
    fn sql_insert_quotes_per_db() {
        assert_eq!(
            encode(Format::Sql, DbKind::Postgres, &rows()),
            "INSERT INTO \"public\".\"t\" (id, name, note) VALUES (1, 'a, \"b\"', NULL);\nINSERT INTO \"public\".\"t\" (id, name, note) VALUES (2, 'it''s\tx|y\nz', TRUE);\n"
        );
        // MySQL also escapes backslashes
        let mut e = Encoder::new(Format::Sql, DbKind::Mysql, "`t`");
        e.header(&["order".into()]);
        assert_eq!(e.row(&[json!("a\\b")]), "INSERT INTO `t` (`order`) VALUES ('a\\\\b');\n");
    }


    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("qurry-import-{}-{name}", uuid::Uuid::new_v4()));
        std::fs::write(&p, body).unwrap();
        p
    }

    /// CSV: BOM · quotes · empty cells (NULL) · short rows
    #[test]
    fn reads_csv_with_bom_and_nulls() {
        let p = tmp("a.csv", "\u{FEFF}id,name,note\n1,\"a, \"\"b\"\"\",\n2,x\n");
        let pv = preview(&p, 10).unwrap();
        assert_eq!(pv.format, Format::Csv);
        assert_eq!(pv.columns, ["id", "name", "note"]);
        assert_eq!(pv.rows, vec![vec![Some("1".into()), Some("a, \"b\"".into()), None], vec![Some("2".into()), Some("x".into()), None]]);
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn reads_tsv_and_json() {
        let p = tmp("a.tsv", "a\tb\n1\tx y\n");
        assert_eq!(preview(&p, 10).unwrap().rows, vec![vec![Some("1".into()), Some("x y".into())]]);
        std::fs::remove_file(p).unwrap();
        let p = tmp("a.json", r#"[{"id":1,"tags":["x"]},{"id":2,"name":"b","tags":null}]"#);
        let pv = preview(&p, 10).unwrap();
        assert_eq!(pv.columns, ["id", "tags", "name"]);
        assert_eq!(pv.rows, vec![vec![Some("1".into()), Some("[\"x\"]".into()), None], vec![Some("2".into()), None, Some("b".into())]]);
        std::fs::remove_file(p).unwrap();
        assert!(preview(std::path::Path::new("/tmp/x.xlsx"), 1).is_err());
    }


    /// Round trip against a real DB: export (per format) · clipboard · cancel, import (type casts · new table · rollback on failure).
    /// Runs only when QURRY_TEST_PG=host:port:user:db (e.g. a container)
    #[tokio::test]
    #[ignore]
    async fn export_and_import_roundtrip() {
        let Ok(env) = std::env::var("QURRY_TEST_PG") else { return };
        let p: Vec<&str> = env.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-transfer-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir.clone())).unwrap();
        let cfg = crate::store::ConnectionConfig {
            id: "t".into(), name: "t".into(), kind: DbKind::Postgres, host: p[0].into(), port: p[1].parse().unwrap(), user: p[2].into(), database: p[3].into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        st.store.save(&cfg).unwrap();
        let sql = |q: &str| st.run_query("t".into(), q.into(), None, None);
        let _ = sql("drop table if exists qurry_tr_src, qurry_tr_dst, qurry_tr_new").await;
        sql("create table qurry_tr_src (id int primary key, name text, amount numeric(10,2), at timestamptz, meta jsonb)").await.unwrap();
        sql("insert into qurry_tr_src select g, 'n' || g, g * 1.5, '2026-01-01'::timestamptz + g * interval '1 day', jsonb_build_object('g', g) from generate_series(1, 2500) g").await.unwrap();
        sql("create table qurry_tr_dst (like qurry_tr_src)").await.unwrap();

        // Clipboard: returned as text
        let e = st.export("t".into(), "select id, name from qurry_tr_src order by id limit 2".into(), Format::Markdown, Dest::Clipboard, None, "j0".into()).await.unwrap();
        assert_eq!(e.text.unwrap(), "| id | name |\n| --- | --- |\n| 1 | n1 |\n| 2 | n2 |\n");

        // Exporting to a file and importing again gives the same result for each format (CSV · TSV · JSON)
        for f in [Format::Csv, Format::Tsv, Format::Json] {
            let path = dir.join(format!("out.{}", f.ext()));
            let e = st.export("t".into(), "select * from qurry_tr_src".into(), f, Dest::File(path.clone()), None, "j1".into()).await.unwrap();
            assert_eq!(e.rows, 2500);
            sql("truncate qurry_tr_dst").await.unwrap();
            let pv = preview(&path, 3).unwrap();
            let spec = ImportSpec { path: path.to_string_lossy().into(), schema: "public".into(), table: "qurry_tr_dst".into(), create: false, columns: pv.columns.iter().cloned().enumerate().collect() };
            assert_eq!(st.import("t".into(), spec, "j2".into()).await.unwrap(), 2500, "{f:?}");
            let diff = sql("select count(*) from (select * from qurry_tr_src except select * from qurry_tr_dst) x").await.unwrap();
            assert_eq!(diff.rows[0][0], serde_json::json!(0), "{f:?}");
        }

        // SQL INSERT files can be run as is
        let path = dir.join("out.sql");
        st.export("t".into(), "select * from qurry_tr_src where id <= 3".into(), Format::Sql, Dest::File(path.clone()), Some(("public".into(), "qurry_tr_dst".into())), "j3".into()).await.unwrap();
        sql("truncate qurry_tr_dst").await.unwrap();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            sql(line).await.unwrap();
        }
        assert_eq!(sql("select count(*) from qurry_tr_dst").await.unwrap().rows[0][0], serde_json::json!(3));

        // Into a new table (some columns only, renamed)
        let csv = dir.join("out.csv");
        let spec = ImportSpec { path: csv.to_string_lossy().into(), schema: "public".into(), table: "qurry_tr_new".into(), create: true, columns: vec![(0, "key".into()), (1, "label".into())] };
        assert_eq!(st.import("t".into(), spec, "j4".into()).await.unwrap(), 2500);
        assert_eq!(sql("select label from qurry_tr_new where key = '7'").await.unwrap().rows[0][0], serde_json::json!("n7"));

        // Excel: into a typed table (integers · decimals · dates · true/false · empty cells are NULL)
        let xlsx = dir.join("users.xlsx");
        std::fs::write(&xlsx, include_bytes!("testdata/sample.xlsx")).unwrap();
        let _ = sql("drop table if exists qurry_tr_xl").await;
        sql("create table qurry_tr_xl (id int, name text, price numeric(6,2), joined date, active boolean, seen timestamp)").await.unwrap();
        let spec = ImportSpec { path: xlsx.to_string_lossy().into(), schema: "public".into(), table: "qurry_tr_xl".into(), create: false, columns: ["id", "name", "price", "joined", "active", "seen"].into_iter().map(String::from).enumerate().collect() };
        assert_eq!(st.import("t".into(), spec, "j7".into()).await.unwrap(), 3);
        let r = sql("select id, name, price::text, joined::text, active, seen::text from qurry_tr_xl order by id").await.unwrap();
        assert_eq!(serde_json::to_string(&r.rows).unwrap(), r#"[[1,"김철수","1.50","2026-10-29",true,"2026-10-29 12:00:00"],[2,null,"20.00",null,false,null],[3,"lee",null,null,null,null]]"#);
        let _ = sql("drop table qurry_tr_xl").await;

        // A failure midway inserts nothing (duplicate primary key)
        sql("truncate qurry_tr_dst").await.unwrap();
        sql("alter table qurry_tr_dst add primary key (id)").await.unwrap();
        sql("insert into qurry_tr_dst (id) values (2400)").await.unwrap();
        let spec = ImportSpec { path: csv.to_string_lossy().into(), schema: "public".into(), table: "qurry_tr_dst".into(), create: false, columns: vec![(0, "id".into()), (1, "name".into())] };
        let err = st.import("t".into(), spec, "j5".into()).await.unwrap_err().to_string();
        assert!(err.starts_with("Rows 2001–2500"), "{err}");
        assert_eq!(sql("select count(*) from qurry_tr_dst").await.unwrap().rows[0][0], serde_json::json!(1));

        // Cancelling stops and deletes the partial file
        let path = dir.join("cancel.csv");
        let job = st.start_job("j6");
        job.cancel();
        let mut out = Exporter::create(Encoder::new(Format::Csv, DbKind::Postgres, ""), &Dest::File(path.clone()), job).await.unwrap();
        out.header(&["a".into()]).await.unwrap();
        assert_eq!(out.row(&[Value::Null]).await.unwrap_err().to_string(), CANCELLED);

        let _ = sql("drop table if exists qurry_tr_src, qurry_tr_dst, qurry_tr_new").await;
        let _ = std::fs::remove_dir_all(dir);
    }


    /// MySQL round trip. Only when QURRY_TEST_MY=host:port:user:password:db
    #[tokio::test]
    #[ignore]
    async fn mysql_export_and_import() {
        let Ok(env) = std::env::var("QURRY_TEST_MY") else { return };
        let p: Vec<&str> = env.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-transfer-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir.clone())).unwrap();
        let cfg = crate::store::ConnectionConfig {
            id: "m".into(), name: "m".into(), kind: DbKind::Mysql, host: p[0].into(), port: p[1].parse().unwrap(), user: p[2].into(), database: p[4].into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        st.save_connection(cfg, Some(p[3].into()), None).await.unwrap();
        let sql = |q: &str| st.run_query("m".into(), q.into(), None, None);
        let _ = sql("drop table if exists tr_src, tr_dst, tr_new").await;
        sql("create table tr_src (id int primary key, name text, amount decimal(10,2), at datetime, `order` varchar(10))").await.unwrap();
        sql("insert into tr_src values (1, 'a\\\\b', 1.50, '2026-01-01 10:00:00', 'x'), (2, 'it''s', null, null, null)").await.unwrap();
        sql("create table tr_dst like tr_src").await.unwrap();
        for f in [Format::Csv, Format::Json] {
            let path = dir.join(format!("m.{}", f.ext()));
            assert_eq!(st.export("m".into(), "select * from tr_src".into(), f, Dest::File(path.clone()), None, "a".into()).await.unwrap().rows, 2);
            sql("truncate tr_dst").await.unwrap();
            let cols = preview(&path, 1).unwrap().columns;
            let spec = ImportSpec { path: path.to_string_lossy().into(), schema: p[4].into(), table: "tr_dst".into(), create: false, columns: cols.into_iter().enumerate().collect() };
            assert_eq!(st.import("m".into(), spec, "b".into()).await.unwrap(), 2);
            let same = sql("select count(*) from tr_src s join tr_dst d on d.id = s.id and d.name = s.name and d.amount <=> s.amount and d.at <=> s.at and d.`order` <=> s.`order`").await.unwrap();
            assert_eq!(same.rows[0][0], serde_json::json!(2), "{f:?}");
        }
        // SQL INSERT (backslashes · single quotes · reserved-word columns)
        let path = dir.join("m.sql");
        st.export("m".into(), "select * from tr_src".into(), Format::Sql, Dest::File(path.clone()), Some((p[4].into(), "tr_dst".into())), "c".into()).await.unwrap();
        sql("truncate tr_dst").await.unwrap();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            sql(line).await.unwrap();
        }
        assert_eq!(sql("select name from tr_dst order by id").await.unwrap().rows, vec![vec![serde_json::json!("a\\b")], vec![serde_json::json!("it's")]]);
        // New table
        let spec = ImportSpec { path: dir.join("m.csv").to_string_lossy().into(), schema: p[4].into(), table: "tr_new".into(), create: true, columns: vec![(0, "id".into()), (1, "name".into())] };
        assert_eq!(st.import("m".into(), spec, "d".into()).await.unwrap(), 2);
        let _ = sql("drop table if exists tr_src, tr_dst, tr_new").await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn markdown_table() {
        assert_eq!(encode(Format::Markdown, DbKind::Postgres, &rows()), "| id | name | note |\n| --- | --- | --- |\n| 1 | a, \"b\" |  |\n| 2 | it's\tx\\|y<br>z | true |\n");
    }
}
