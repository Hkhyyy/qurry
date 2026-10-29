//! Backup / restore: runs pg_dump / pg_restore / psql, mysqldump / mysql (found in PATH and common install locations).
//! Passwords go via environment variables, not the command line (so they don't show in ps). With an SSH tunnel, the tunnel port is used. Progress is bytes written (read)
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
    /// PG: "custom" (restored with pg_restore, default) · "plain" (SQL file)
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub schema_only: bool,
    #[serde(default)]
    pub data_only: bool,
    /// PG: only these schemas (empty = all)
    #[serde(default)]
    pub schemas: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOptions {
    /// PG custom: drop existing objects first (--clean --if-exists)
    #[serde(default)]
    pub clean: bool,
}

/// Folders where tools are likely to be (after PATH)
fn tool_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/homebrew/opt/libpq/bin", "/usr/local/opt/libpq/bin", "/opt/homebrew/opt/mysql-client/bin", "/usr/local/opt/mysql-client/bin", "/opt/homebrew/opt/mysql/bin", "/usr/local/mysql/bin", "/usr/bin"] {
        dirs.push(d.into());
    }
    // Per-version folders: postgresql@16, mysql-client@8.4, Postgres.app, /usr/lib/postgresql/16
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

/// Tool location: PATH, Homebrew, Postgres.app, Linux default locations. If several, the highest version
pub fn find_tool(name: &str) -> Option<PathBuf> {
    find_tool_for(name, None)
}

/// Tool matching the server major version: the closest one at or above the server (pg_dump can't be used if older than the server, and if too new it emits SETs an old server doesn't know).
/// If none is at or above the server, the highest one (the tool reports the version mismatch)
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

/// PG server major version (16, 17 …)
async fn pg_major(st: &AppState, id: &str) -> Option<u64> {
    let r = ensure_pool(st, id).await.ok()?.run_query("SHOW server_version_num", 1).await.ok()?;
    crate::db::cell_text(r.rows.first()?.first()?).parse::<u64>().ok().map(|n| n / 10000)
}

/// Settings that newer pg_dump puts in the dump header but older servers don't know (name, version introduced)
const NEWER_SETTINGS: &[(&str, u64)] = &[("SET transaction_timeout", 17)];

/// Copy of the SQL without SET lines an older server doesn't know (None if the server needs nothing removed)
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

/// File readable and writable only by the owner (0600). The dump contains all the data, so others can't read it in /tmp
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        o.mode(0o600);
        let f = o.open(path)?;
        // Also for a file that already existed (a backup file the user chose to overwrite)
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        return Ok(f);
    }
    #[allow(unreachable_code)]
    o.open(path)
}

/// The -d of pg_dump · pg_restore · psql: the value is also parsed as a connection string (putting host=… in it goes to another server), so pin it to just a DB name
fn pg_dbname(db: &str) -> String {
    format!("dbname='{}'", db.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A mysql client command in an SQL file that does something on this computer (the line). A second check: mysql runs with --binary-mode,
/// which switches these off. Looks for system / tee / pager / source / connect … at the start of a line, and \! \T \P \. \r \e.
/// (psql needs no line check: it runs with \restrict, and an older psql isn't used for plain SQL)
pub fn local_command(sql: impl std::io::BufRead, kind: DbKind) -> Option<String> {
    if kind != DbKind::Mysql {
        return None;
    }
    for line in sql.split(b'\n') {
        let line = String::from_utf8_lossy(&line.ok()?).into_owned();
        let t = line.trim_start();
        let word = t.split_whitespace().next().unwrap_or("").to_lowercase();
        let bad_word = ["system", "tee", "notee", "pager", "nopager", "source", "connect", "edit"].contains(&word.as_str());
        let bad_slash = t.strip_prefix('\\').is_some_and(|c| c.starts_with(['!', 'T', 'P', '.', 'r', 'e']));
        if bad_word || bad_slash || t.contains("\\!") {
            return Some(line);
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

/// Connection info: (host, port, user, DB, password, settings)
async fn endpoint(st: &AppState, id: &str) -> AppResult<(String, u16, String, String, Option<String>, crate::store::ConnectionConfig)> {
    let cfg = st.source(id)?;
    let (base, db) = split_target(id);
    let db = db.map(String::from).unwrap_or_else(|| crate::sql::default_db(&cfg));
    // With an SSH tunnel, open the tunnel and use its port
    ensure_pool(st, id).await?;
    let local = st.tunnels.read().await.get(id).map(|t| t.local_port);
    let (host, port) = match local {
        Some(p) => ("127.0.0.1".to_string(), p),
        None => (cfg.host.clone(), cfg.port),
    };
    Ok((host, port, cfg.user.clone(), db, st.secret(base)?, cfg))
}

/// Whether psql knows \restrict (from 17.6 · 16.10 · 15.14 · 14.19 · 13.22)
fn psql_restricts(v: &[u64]) -> bool {
    let (major, minor) = (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0));
    match major {
        0..=12 => false,
        13 => minor >= 22,
        14 => minor >= 19,
        15 => minor >= 14,
        16 => minor >= 10,
        17 => minor >= 6,
        _ => true,
    }
}

/// Whether mysql knows --system-command (from 8.0.40 · 8.4.3 · 9.1)
fn mysql_has_system_command_option(tool: &Path) -> bool {
    std::process::Command::new(tool).arg("--help").output().map(|o| String::from_utf8_lossy(&o.stdout).contains("--system-command")).unwrap_or(false)
}

/// Copy to feed psql (0600): removes SETs an older server doesn't know, prepends \restrict key if there is a key, and removes \restrict · \unrestrict in the file
/// (lines pg_dump 17.6+ writes. If left in, commands could be re-enabled with a key chosen by the dump)
fn prepare_psql_input(src: &Path, server: Option<u64>, key: Option<String>) -> AppResult<PathBuf> {
    use std::io::{BufRead, Write};
    let drop: Vec<&str> = NEWER_SETTINGS.iter().filter(|(_, v)| server.is_some_and(|s| s < *v)).map(|(n, _)| *n).collect();
    let out = std::env::temp_dir().join(format!("qurry-restore-{}.sql", uuid::Uuid::new_v4()));
    let mut r = std::io::BufReader::new(std::fs::File::open(src)?);
    let mut w = std::io::BufWriter::new(private_file(&out)?);
    if let Some(k) = &key {
        writeln!(w, "\\restrict {k}")?;
    }
    let mut line = Vec::new();
    while r.read_until(b'\n', &mut line)? > 0 {
        let t = line.trim_ascii_start();
        let restrict_line = key.is_some() && (t.starts_with(b"\\restrict") || t.starts_with(b"\\unrestrict"));
        let newer = line.starts_with(b"SET ") && drop.iter().any(|d| line.starts_with(d.as_bytes()));
        if !restrict_line && !newer {
            w.write_all(&line)?;
        }
        line.clear();
    }
    if let Some(k) = &key {
        writeln!(w, "\n\\unrestrict {k}")?;
    }
    w.flush()?;
    Ok(out)
}

/// Stop if the SQL to restore has lines that run commands on this computer (pg_dump · mysqldump don't write such lines)
fn refuse_local_commands(file: &Path, kind: DbKind) -> AppResult<()> {
    match local_command(std::io::BufReader::new(std::fs::File::open(file)?), kind) {
        Some(line) => Err(AppError::Msg(format!("This file runs commands on your computer, so it was not restored (pg_dump / mysqldump never write these):\n{}", line.chars().take(200).collect::<String>()))),
        None => Ok(()),
    }
}

/// Run a tool: stdin = feed this file with progress, stdout = into this file, watch = progress by this file's size. Kill it on cancel
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

/// PG connection environment variables (password and SSL are not put on the command line)
/// Password file passed to the tool (0600, deleted when the tool finishes)
struct SecretTemp(PathBuf);

impl Drop for SecretTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn secret_temp(prefix: &str, content: &str) -> AppResult<SecretTemp> {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
    let mut f = private_file(&path)?;
    f.write_all(content.as_bytes())?;
    Ok(SecretTemp(path))
}

/// Password goes via a PGPASSFILE matching only this server · user, not an environment variable (so a \connect in the dump to another server doesn't carry it along)
fn pg_env(cmd: &mut tokio::process::Command, host: &str, port: u16, user: &str, password: Option<&str>, cfg: &crate::store::ConnectionConfig) -> AppResult<Option<SecretTemp>> {
    let esc = |s: &str| s.replace('\\', "\\\\").replace(':', "\\:");
    let pass = match password {
        Some(p) => {
            let t = secret_temp("qurry-pgpass", &format!("{}:{port}:*:{}:{}\n", esc(host), esc(user), esc(p)))?;
            cmd.env("PGPASSFILE", &t.0);
            Some(t)
        }
        None => None,
    };
    cmd.env_remove("PGPASSWORD");
    cmd.env("PGCONNECT_TIMEOUT", "10");
    match cfg.active_ssl() {
        Some(s) => {
            crate::db::check_ssl_mode(s.mode())?;
            cmd.env("PGSSLMODE", s.mode());
            for (k, v) in [("PGSSLROOTCERT", &s.ca_file), ("PGSSLCERT", &s.cert_file), ("PGSSLKEY", &s.key_file)] {
                if !v.is_empty() {
                    cmd.env(k, v);
                }
            }
        }
        // Same rule as the app: a remote server must set up TLS unless plain text is allowed
        None => {
            cmd.env("PGSSLMODE", if crate::db::plaintext_allowed(cfg, host) { "prefer" } else { "require" });
        }
    }
    Ok(pass)
}

/// Password via --defaults-extra-file (0600). This option must come first
fn my_args(cmd: &mut tokio::process::Command, host: &str, port: u16, user: &str, password: Option<&str>, cfg: &crate::store::ConnectionConfig) -> AppResult<Option<SecretTemp>> {
    let pass = match password {
        Some(p) => {
            // Option-file escapes: \\ \" and line breaks / tabs, so the value can't end the line or add options
            let v = p.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
            let t = secret_temp("qurry-my", &format!("[client]\npassword=\"{v}\"\n"))?;
            cmd.arg(format!("--defaults-extra-file={}", t.0.display()));
            Some(t)
        }
        None => None,
    };
    cmd.env_remove("MYSQL_PWD");
    cmd.arg("-h").arg(host).arg("-P").arg(port.to_string()).arg("-u").arg(user).arg("--protocol=TCP");
    if cfg.active_ssl().is_none() && !crate::db::plaintext_allowed(cfg, host) {
        cmd.arg("--ssl-mode=REQUIRED");
    }
    if let Some(s) = cfg.active_ssl() {
        crate::db::check_ssl_mode(s.mode())?;
        cmd.arg(format!("--ssl-mode={}", match s.mode() { "verify-full" => "VERIFY_IDENTITY", "verify-ca" => "VERIFY_CA", _ => "REQUIRED" }));
        for (k, v) in [("--ssl-ca", &s.ca_file), ("--ssl-cert", &s.cert_file), ("--ssl-key", &s.key_file)] {
            if !v.is_empty() {
                cmd.arg(format!("{k}={v}"));
            }
        }
    }
    Ok(pass)
}

impl AppState {
    /// Create a backup file. Bytes written (on failure or cancel, the partial file is deleted)
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
                    let _pass = pg_env(&mut cmd, &host, port, &user, password.as_deref(), &cfg)?;
                    run_tool(cmd, "pg_dump", None, None, Some(&file), &job).await?;
                }
                DbKind::Mysql => {
                    let tool = find_tool("mysqldump").ok_or_else(|| missing("mysqldump"))?;
                    let mut cmd = tokio::process::Command::new(tool);
                    let _pass = my_args(&mut cmd, &host, port, &user, password.as_deref(), &cfg)?;
                    cmd.args(["--single-transaction", "--routines", "--triggers", "--events", "--hex-blob"]);
                    if opts.schema_only {
                        cmd.arg("--no-data");
                    }
                    if opts.data_only {
                        cmd.arg("--no-create-info");
                    }
                    // After -- it is read only as a DB name (so a server-provided name like "--tee=…" doesn't become an option)
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

    /// Restore from a backup file (pg_dump custom → pg_restore, SQL → psql / mysql). One transaction: rolled back on failure (except MySQL DDL)
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
                            // Older server: expand to SQL, remove unknown settings, and use psql (pg_restore stops at those SETs)
                            let tmp = std::env::temp_dir().join(format!("qurry-restore-{}.sql", uuid::Uuid::new_v4()));
                            // Create it as an owner-only file before pg_restore writes (the dump contains all the data)
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
                            let _pass = pg_env(&mut cmd, &host, port, &user, password.as_deref(), &cfg)?;
                            run_tool(cmd, "pg_restore", None, None, None, &job).await?;
                        }
                    }
                    if !custom || sql_file.is_some() {
                        let src0 = sql_file.clone().unwrap_or_else(|| file.clone());
                        let tool = find_tool_for("psql", server).ok_or_else(|| missing("psql"))?;
                        // psql commands (\! · \o · \copy … program · \connect …) are disabled with \restrict. For older psql that doesn't know it, read the lines
                        // An older psql can't switch its commands off: don't restore plain SQL with it
                        let v = version_of(&tool);
                        if !psql_restricts(&v) {
                            return Err(AppError::Msg(format!(
                                "Restoring a .sql file needs psql 17.6 / 16.10 / 15.14 / 14.19 / 13.22 or newer (it can switch psql commands off; found {}). Update the PostgreSQL client tools (macOS: brew upgrade libpq · Ubuntu: the postgresql-client package)",
                                v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(".")
                            )));
                        }
                        let src = prepare_psql_input(&src0, server, Some(uuid::Uuid::new_v4().simple().to_string()))?;
                        // Removed however the restore ends (also when it's cancelled): it holds the whole dump and the key
                        let _input = SecretTemp(src.clone());
                        if let Some(t) = sql_file.take() {
                            std::fs::remove_file(t).ok();
                        }
                        let mut cmd = tokio::process::Command::new(tool);
                        cmd.arg("-h").arg(&host).arg("-p").arg(port.to_string()).arg("-U").arg(&user).arg("-d").arg(pg_dbname(&db)).arg("--no-password");
                        cmd.args(["-X", "-q", "-v", "ON_ERROR_STOP=1", "--single-transaction", "-f", "-"]);
                        let _pass = pg_env(&mut cmd, &host, port, &user, password.as_deref(), &cfg)?;
                        job.total.store(std::fs::metadata(&src).map(|m| m.len()).unwrap_or(size), Ordering::Relaxed);
                        run_tool(cmd, "psql", Some(&src), None, None, &job).await.map_err(|e| match e.to_string() {
                            m if m.contains("backslash commands are restricted") => AppError::Msg(format!(
                                "{m}\n(The file uses psql commands such as \\connect — e.g. a dump made with pg_dump -C or pg_dumpall. Qurry restores SQL only: restore it into the database you picked without those lines, or run it with psql yourself)"
                            )),
                            _ => e,
                        })?;
                    }
                }
                DbKind::Mysql => {
                    refuse_local_commands(&file, DbKind::Mysql)?;
                    let tool = find_tool("mysql").ok_or_else(|| missing("mysql"))?;
                    let system_off = mysql_has_system_command_option(&tool);
                    let mut cmd = tokio::process::Command::new(tool);
                    let _pass = my_args(&mut cmd, &host, port, &user, password.as_deref(), &cfg)?;
                    // Disable mysql client commands (system · tee · pager · source · connect …) (only charset · delimiter remain)
                    cmd.arg("--binary-mode");
                    if system_off {
                        cmd.arg("--system-command=OFF");
                    }
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
        use DbKind::Mysql;
        assert!(local_command("-- MySQL dump\nCREATE TABLE t (a int);\nINSERT INTO t VALUES (1);\n".as_bytes(), Mysql).is_none());
        for bad in ["system rm -rf ~", "system\tid", "\\! id", "tee /Users/x/.zshrc", "\\T /tmp/x", "pager sh", "source /etc/passwd", "connect db evil.host"] {
            assert!(local_command(format!("select 1;\n{bad}\n").as_bytes(), Mysql).is_some(), "{bad}");
        }
    }

    /// psql input copy: \restrict key at the top, \restrict · \unrestrict in the file removed, 0600
    #[test]
    fn psql_input_is_restricted() {
        assert!(psql_restricts(&[17, 6]) && psql_restricts(&[16, 10]) && psql_restricts(&[18, 0]) && !psql_restricts(&[16, 9]) && !psql_restricts(&[12, 22]));
        let src = std::env::temp_dir().join(format!("qurry-in-{}.sql", uuid::Uuid::new_v4()));
        std::fs::write(&src, "\\restrict theirs\nSET transaction_timeout = 0;\nCREATE TABLE t (a int);\n\\unrestrict theirs\n").unwrap();
        let out = prepare_psql_input(&src, Some(16), Some("k1".into())).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "\\restrict k1\nCREATE TABLE t (a int);\n\n\\unrestrict k1\n");
        std::fs::remove_file(src).ok();
        std::fs::remove_file(out).ok();
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
        // None if not on this PC (backup shows install instructions)
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

    /// pgvector (only on servers with the extension): nearest-row SQL uses the index operator and returns rows by distance. Vector values show as text
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
        // halfvec · sparsevec also as the text PG shows
        let h = q("select '[1,0.5,-2]'::halfvec h, '{1:0.5,3:1}/5'::sparsevec s, '[0.25,1e-3]'::vector v").await.unwrap();
        assert_eq!(h.rows[0], vec![serde_json::json!("[1,0.5,-2]"), serde_json::json!("{1:0.5,3:1}/5"), serde_json::json!("[0.25,0.001]")]);
        q("drop table qurry_vec").await.ok();
    }

    /// MySQL: mysqldump → drop → restore with mysql (only when the tools are available)
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
        // Dedicated DB: dumping while other tests create and drop tables in the same DB breaks it
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

    /// Backup → drop → restore → data unchanged (both custom and SQL)
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
        // Things like passwords don't go on the command line: even on failure, the tool output is in the message
        let e = st.restore("b".into(), dir.join("missing.sql").to_string_lossy().into(), RestoreOptions::default(), "r-x".into()).await.unwrap_err().to_string();
        assert!(!e.is_empty());
        q("drop schema qurry_bak cascade").await.ok();
        std::fs::remove_dir_all(dir).ok();
    }
}
