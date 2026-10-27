//! 데이터 내보내기 (CSV·TSV·JSON·SQL INSERT·Markdown, 파일/클립보드) 와 가져오기 (CSV·TSV·JSON → 테이블).
//! 오래 걸리는 작업은 Job 으로 진행 상황을 알리고 취소할 수 있다
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
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Csv => "CSV",
            Format::Tsv => "TSV",
            Format::Json => "JSON",
            Format::Sql => "SQL INSERT",
            Format::Markdown => "Markdown",
        }
    }

    /// 가져올 파일의 형식 (확장자로)
    pub fn of_path(path: &std::path::Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "csv" => Some(Format::Csv),
            "tsv" | "tab" => Some(Format::Tsv),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

/// 진행 상황 (done/total, total = 0 이면 모름) 과 취소
#[derive(Default)]
pub struct Job {
    /// 처리한 행 수
    pub rows: AtomicU64,
    /// 진행 (done/total). 내보내기는 행, 가져오기는 파일 바이트
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
    /// (행, done, total)
    pub fn progress(&self) -> (u64, u64, u64) {
        (self.rows.load(Ordering::Relaxed), self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed))
    }
    fn check(&self) -> AppResult<()> {
        if self.cancelled() { Err(AppError::Msg(CANCELLED.into())) } else { Ok(()) }
    }
}

pub const CANCELLED: &str = "Cancelled";

// ───────── 내보내기: 형식별 글 ─────────

/// 형식별로 줄을 만든다. table = SQL INSERT 의 대상 (따옴표 친 이름)
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
            Format::Csv | Format::Tsv => self.delimited(cols.iter().map(|c| c.as_str())),
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
            Format::Csv | Format::Tsv => {
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

/// SQL 값: NULL·숫자·TRUE/FALSE 는 그대로, 나머지는 따옴표 (MySQL 은 백슬래시도)
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

/// Markdown 표 칸: | 와 줄바꿈이 표를 깨지 않게
fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace("\r\n", "<br>").replace('\n', "<br>")
}


// ───────── 내보내기: 파일 / 클립보드 ─────────

/// 어디로: 파일, 또는 클립보드에 넣을 글 (메모리)
#[derive(Clone, Debug)]
pub enum Dest {
    File(std::path::PathBuf),
    Clipboard,
}

/// 클립보드로는 이만큼까지 (더 크면 파일로)
const CLIPBOARD_MAX: usize = 64 << 20;

enum Out {
    File(tokio::io::BufWriter<tokio::fs::File>),
    Mem(Vec<u8>),
}

/// 형식에 맞춰 행을 써 나간다. 행마다 job 진행을 올리고, 취소됐으면 멈춘다
pub struct Exporter {
    enc: Encoder,
    out: Out,
    job: Arc<Job>,
    header: bool,
}

/// 끝난 내보내기: 행 수와 (클립보드면) 글
pub struct Exported {
    pub rows: u64,
    pub text: Option<String>,
}

impl Exporter {
    pub async fn create(enc: Encoder, dest: &Dest, job: Arc<Job>) -> AppResult<Self> {
        let out = match dest {
            Dest::File(p) => {
                let mut f = tokio::io::BufWriter::new(tokio::fs::File::create(p).await?);
                // CSV 파일은 BOM 을 붙인다 (Excel 이 한글을 깨뜨리지 않게)
                if enc.format == Format::Csv {
                    tokio::io::AsyncWriteExt::write_all(&mut f, "\u{FEFF}".as_bytes()).await?;
                }
                Out::File(f)
            }
            Dest::Clipboard => Out::Mem(vec![]),
        };
        Ok(Exporter { enc, out, job, header: false })
    }

    /// CSV 로 파일에 (MCP·예전 API)
    pub async fn csv_file(path: &std::path::Path) -> AppResult<Self> {
        Self::create(Encoder::new(Format::Csv, DbKind::Postgres, ""), &Dest::File(path.into()), Job::new()).await
    }

    async fn write(&mut self, s: String) -> AppResult<()> {
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

// ───────── 가져오기: 파일 읽기 ─────────

/// 가져올 파일 미리보기 (첫 몇 행)
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub format: Format,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub bytes: u64,
}

/// 한 행 (빈 칸은 NULL) 과 그 행까지 읽은 바이트
type Record = AppResult<(Vec<Option<String>>, u64)>;
type Records = Box<dyn Iterator<Item = Record> + Send>;

/// 파일을 열어 (열 이름, 행들). CSV·TSV 는 흘려 읽고, JSON 은 객체 배열 하나를 통째로 읽는다
pub fn read_file(path: &std::path::Path) -> AppResult<(Format, Vec<String>, Records)> {
    let format = Format::of_path(path).ok_or_else(|| AppError::Msg("Import reads .csv, .tsv or .json files".into()))?;
    match format {
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

fn csv_err(e: csv::Error) -> AppError {
    AppError::Msg(format!("Could not read the file: {e}"))
}

pub fn preview(path: &std::path::Path, rows: usize) -> AppResult<Preview> {
    let (format, columns, it) = read_file(path)?;
    let rows = it.take(rows).map(|r| r.map(|x| x.0)).collect::<AppResult<Vec<_>>>()?;
    Ok(Preview { format, columns, rows, bytes: std::fs::metadata(path)?.len() })
}

// ───────── 작업 실행 (AppState) ─────────

/// 가져오기: 파일 열 번호 → 테이블 컬럼
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSpec {
    pub path: String,
    pub schema: String,
    pub table: String,
    /// 없는 테이블을 만들고 넣는다 (컬럼은 모두 text)
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

/// PG 의 대략 행 수 (EXPLAIN 추정치). 진행 막대용이라 틀려도 된다
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
    /// 작업을 등록한다 (진행 조회·취소용). export/import 가 끝나면 지운다. 미리 등록해 두면 그걸 쓴다
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

    /// 쿼리 전체 결과를 내보낸다. table = SQL INSERT 의 대상 (schema, 이름). 실패·취소하면 쓰던 파일을 지운다
    pub async fn export(&self, id: String, sql: String, format: Format, dest: Dest, table: Option<(String, String)>, job_id: String) -> AppResult<Exported> {
        let job = self.job(&job_id).unwrap_or_else(|| self.start_job(&job_id));
        let r = async {
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

    /// 파일을 테이블에 넣는다 (한 트랜잭션: 실패·취소하면 하나도 안 들어간다). 넣은 행 수
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
        // PG: 글로 보내고 컬럼 타입으로 바꾼다 (숫자·날짜·json·enum 도)
        let types: std::collections::HashMap<String, String> = if spec.create || kind != DbKind::Postgres {
            Default::default()
        } else {
            self.table_detail(id.to_string(), spec.schema.clone(), spec.table.clone()).await?.columns.into_iter().map(|c| (c.name, c.data_type)).collect()
        };
        let mut tx = match pool {
            Pool::Pg(p) => Tx::Pg(p.begin().await?),
            Pool::My(p) => Tx::My(p.begin().await?),
            Pool::Redis(..) => return Err(AppError::Msg("Import into Redis is not supported".into())),
        };
        if spec.create {
            let ty = if kind == DbKind::Mysql { "TEXT" } else { "text" };
            let cols: Vec<String> = spec.columns.iter().map(|(_, c)| format!("{} {ty}", q(c))).collect();
            tx.exec(&format!("CREATE TABLE {table} ({})", cols.join(", ")), vec![]).await?;
        }
        let names: Vec<String> = spec.columns.iter().map(|(_, c)| q(c)).collect();
        let per = (60_000 / spec.columns.len()).clamp(1, 1000);
        // 파일은 다른 스레드에서 읽어 묶음으로 보낸다
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
        // MySQL 은 백슬래시도 이스케이프
        let mut e = Encoder::new(Format::Sql, DbKind::Mysql, "`t`");
        e.header(&["order".into()]);
        assert_eq!(e.row(&[json!("a\\b")]), "INSERT INTO `t` (`order`) VALUES ('a\\\\b');\n");
    }


    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("qurry-import-{}-{name}", uuid::Uuid::new_v4()));
        std::fs::write(&p, body).unwrap();
        p
    }

    /// CSV: BOM·따옴표·빈 칸(NULL)·모자란 칸
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


    /// 실제 DB 로 왕복: 내보내기(형식별)·클립보드·취소, 가져오기(타입 변환·새 테이블·실패하면 롤백).
    /// QURRY_TEST_PG=host:port:user:db (예: 컨테이너) 일 때만 돈다
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

        // 클립보드: 글로 돌려준다
        let e = st.export("t".into(), "select id, name from qurry_tr_src order by id limit 2".into(), Format::Markdown, Dest::Clipboard, None, "j0".into()).await.unwrap();
        assert_eq!(e.text.unwrap(), "| id | name |\n| --- | --- |\n| 1 | n1 |\n| 2 | n2 |\n");

        // 형식마다 파일로 내보내고 다시 넣으면 같다 (CSV·TSV·JSON)
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

        // SQL INSERT 파일은 그대로 실행할 수 있다
        let path = dir.join("out.sql");
        st.export("t".into(), "select * from qurry_tr_src where id <= 3".into(), Format::Sql, Dest::File(path.clone()), Some(("public".into(), "qurry_tr_dst".into())), "j3".into()).await.unwrap();
        sql("truncate qurry_tr_dst").await.unwrap();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            sql(line).await.unwrap();
        }
        assert_eq!(sql("select count(*) from qurry_tr_dst").await.unwrap().rows[0][0], serde_json::json!(3));

        // 새 테이블로 (열 일부만, 이름 바꿔서)
        let csv = dir.join("out.csv");
        let spec = ImportSpec { path: csv.to_string_lossy().into(), schema: "public".into(), table: "qurry_tr_new".into(), create: true, columns: vec![(0, "key".into()), (1, "label".into())] };
        assert_eq!(st.import("t".into(), spec, "j4".into()).await.unwrap(), 2500);
        assert_eq!(sql("select label from qurry_tr_new where key = '7'").await.unwrap().rows[0][0], serde_json::json!("n7"));

        // 중간에 실패하면 하나도 안 들어간다 (기본키 중복)
        sql("truncate qurry_tr_dst").await.unwrap();
        sql("alter table qurry_tr_dst add primary key (id)").await.unwrap();
        sql("insert into qurry_tr_dst (id) values (2400)").await.unwrap();
        let spec = ImportSpec { path: csv.to_string_lossy().into(), schema: "public".into(), table: "qurry_tr_dst".into(), create: false, columns: vec![(0, "id".into()), (1, "name".into())] };
        let err = st.import("t".into(), spec, "j5".into()).await.unwrap_err().to_string();
        assert!(err.starts_with("Rows 2001–2500"), "{err}");
        assert_eq!(sql("select count(*) from qurry_tr_dst").await.unwrap().rows[0][0], serde_json::json!(1));

        // 취소하면 멈추고 쓰던 파일은 지운다
        let path = dir.join("cancel.csv");
        let job = st.start_job("j6");
        job.cancel();
        let mut out = Exporter::create(Encoder::new(Format::Csv, DbKind::Postgres, ""), &Dest::File(path.clone()), job).await.unwrap();
        out.header(&["a".into()]).await.unwrap();
        assert_eq!(out.row(&[Value::Null]).await.unwrap_err().to_string(), CANCELLED);

        let _ = sql("drop table if exists qurry_tr_src, qurry_tr_dst, qurry_tr_new").await;
        let _ = std::fs::remove_dir_all(dir);
    }


    /// MySQL 왕복. QURRY_TEST_MY=host:port:user:password:db 일 때만
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
        // SQL INSERT (백슬래시·작은따옴표·예약어 컬럼)
        let path = dir.join("m.sql");
        st.export("m".into(), "select * from tr_src".into(), Format::Sql, Dest::File(path.clone()), Some((p[4].into(), "tr_dst".into())), "c".into()).await.unwrap();
        sql("truncate tr_dst").await.unwrap();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            sql(line).await.unwrap();
        }
        assert_eq!(sql("select name from tr_dst order by id").await.unwrap().rows, vec![vec![serde_json::json!("a\\b")], vec![serde_json::json!("it's")]]);
        // 새 테이블
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
