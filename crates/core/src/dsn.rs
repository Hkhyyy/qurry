//! A temporary data source from the command line (`qurry-tui postgres://user@host/db`): the URL, then the standard environment
//! variables (PGHOST, PGPORT, PGUSER, PGDATABASE, PGPASSWORD, PGSSLMODE, MYSQL_PWD, REDISCLI_AUTH), then ~/.pgpass, the way psql,
//! mysql and redis-cli read them. Nothing here is saved. Passwords never go into a name or a message: `redact` hides them.
use crate::error::{AppError, AppResult};
use crate::store::{ConnectionConfig, DbKind, SslConfig};
use std::path::{Path, PathBuf};

/// What the command line opens: the data source (never saved) and its password
pub struct Temporary {
    pub config: ConnectionConfig,
    pub password: Option<String>,
    /// Things to tell the user (a password visible in the process list, a ~/.pgpass that was ignored)
    pub notes: Vec<String>,
}

/// Never prints the password
impl std::fmt::Debug for Temporary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Temporary").field("config", &self.config).field("password", &self.password.as_ref().map(|_| "***")).field("notes", &self.notes).finish()
    }
}

/// Where the environment comes from (the process environment in the app, a map in tests)
pub struct Env<'a> {
    pub var: &'a dyn Fn(&str) -> Option<String>,
    pub home: Option<PathBuf>,
}

impl Env<'_> {
    fn get(&self, k: &str) -> Option<String> {
        (self.var)(k).filter(|v| !v.is_empty())
    }
    fn local_user(&self) -> String {
        self.get("USER").or_else(|| self.get("USERNAME")).unwrap_or_default()
    }
}

impl Env<'static> {
    /// The process environment and home folder
    pub fn process() -> Self {
        Env { var: &|k| std::env::var(k).ok(), home: dirs::home_dir() }
    }
}

pub const SCHEMES: &str = "postgres://, postgresql://, mysql://, redis:// or rediss://";

/// `url` with any password shown as *** (the userinfo password and a password= parameter), for titles, messages and logs
pub fn redact(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some(p) => p,
        None => return url.to_string(),
    };
    // The password may hold a raw @ / ? #: everything up to the last @ is userinfo (as store::without_userinfo reads it)
    let rest = match rest.rfind('@') {
        Some(at) => match rest[..at].split_once(':') {
            Some((user, _)) => format!("{user}:***@{}", &rest[at + 1..]),
            None => rest.to_string(),
        },
        None => rest.to_string(),
    };
    let out = format!("{scheme}://{rest}");
    match out.split_once('?') {
        Some((base, q)) => {
            let q: Vec<String> = q.split('&').map(|kv| match kv.split_once('=') {
                Some((k, _)) if k.eq_ignore_ascii_case("password") => format!("{k}=***"),
                _ => kv.to_string(),
            }).collect();
            format!("{base}?{}", q.join("&"))
        }
        None => out,
    }
}

fn decode(s: &str) -> AppResult<String> {
    percent_encoding::percent_decode_str(s).decode_utf8().map(|s| s.to_string()).map_err(|_| AppError::Msg("The URL has a %-escape that isn't UTF-8 text".into()))
}

/// A TLS mode that turns TLS off or doesn't check it, with certificate files given (`wanted`: the mode they need): refused, never dropped
fn unused_files(param: &str, mode: &str, wanted: Option<&str>) -> AppError {
    let (want, names) = match (param, wanted) {
        ("ssl-mode", Some("verify-ca")) => ("VERIFY_CA (or VERIFY_IDENTITY)", "ssl-ca"),
        ("ssl-mode", _) => ("REQUIRED", "ssl-cert / ssl-key"),
        (_, Some("verify-ca")) => ("verify-ca (or verify-full)", "sslrootcert"),
        _ => ("require", "sslcert / sslkey"),
    };
    AppError::Msg(format!("Use {param}={want}: {param}={mode} doesn't use {names}"))
}

/// A temporary data source from DATABASE_URL (in the environment, so a password in it isn't on the command line)
pub fn from_database_url(env: &Env, read_only: bool) -> AppResult<Temporary> {
    let url = env.get("DATABASE_URL").ok_or_else(|| AppError::Msg("--env: DATABASE_URL isn't set. Set it, or give the URL itself: qurry-tui postgres://user@host/db".into()))?;
    parse(&url, read_only, env, false)
}

/// A temporary data source from a URL on the command line; the parts it leaves out come from the environment (and ~/.pgpass for a
/// PostgreSQL password)
pub fn from_url(url: &str, read_only: bool, env: &Env) -> AppResult<Temporary> {
    parse(url, read_only, env, true)
}

/// `argv`: the URL is on the command line, where other users see a password in it (ps)
fn parse(url: &str, read_only: bool, env: &Env, argv: bool) -> AppResult<Temporary> {
    let shown = redact(url);
    let bad = |why: &str| AppError::Msg(format!("{why}: {shown}"));
    let (scheme, _) = url.split_once("://").ok_or_else(|| bad(&format!("Not a connection URL (use {SCHEMES})")))?;
    let kind = match scheme.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => DbKind::Postgres,
        "mysql" => DbKind::Mysql,
        "redis" | "rediss" => DbKind::Redis,
        s => return Err(AppError::Msg(format!("Unknown scheme \"{s}://\" — qurry-tui opens {SCHEMES} URLs"))),
    };
    let tls_scheme = scheme.eq_ignore_ascii_case("rediss");
    let u = url::Url::parse(url).map_err(|e| bad(&format!("Not a valid URL ({e})")))?;
    let host = u.host_str().map(|h| h.trim_start_matches('[').trim_end_matches(']')).map(decode).transpose()?.filter(|h| !h.is_empty());
    if host.as_deref().is_some_and(|h| h.contains(',')) {
        return Err(bad("Several hosts in one URL aren't supported — give one host"));
    }
    let user = Some(decode(u.username())?).filter(|s| !s.is_empty());
    let mut password = u.password().map(decode).transpose()?.filter(|s| !s.is_empty());
    let path = decode(u.path().trim_start_matches('/'))?;
    let mut q = std::collections::BTreeMap::new();
    for kv in u.query().unwrap_or_default().split('&').filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        q.insert(decode(k)?.to_ascii_lowercase(), decode(v)?);
    }
    let known: &[&str] = match kind {
        DbKind::Postgres => &["host", "port", "user", "dbname", "password", "sslmode", "sslrootcert", "sslcert", "sslkey"],
        DbKind::Mysql => &["ssl-mode", "sslmode", "ssl-ca", "ssl-cert", "ssl-key"],
        _ if tls_scheme => &["sslmode", "sslrootcert", "sslcert", "sslkey"],
        _ => &[],
    };
    // Common in DATABASE_URL and harmless to leave out (a timeout, a name for the server's activity view, a client library's own
    // setting): ignored with a note. Anything else is refused, so a TLS or password setting is never silently dropped
    let ignored: &[&str] = match kind {
        DbKind::Postgres => &["connect_timeout", "application_name", "pgbouncer", "schema"],
        DbKind::Mysql => &["charset", "connect_timeout"],
        _ => &[],
    };
    let mut notes = Vec::new();
    let skipped: Vec<&str> = q.keys().map(String::as_str).filter(|k| ignored.contains(k)).collect();
    if !skipped.is_empty() {
        notes.push(format!("Ignored URL parameter{} {} (Qurry doesn't use {}).", if skipped.len() == 1 { "" } else { "s" }, skipped.join(", "), if skipped.len() == 1 { "it" } else { "them" }));
    }
    if let Some(k) = q.keys().find(|k| !known.contains(&k.as_str()) && !ignored.contains(&k.as_str())) {
        let list = if known.is_empty() { "none (use rediss:// for TLS)".to_string() } else { known.join(", ") };
        return Err(AppError::Msg(format!("Unknown URL parameter \"{k}\" — supported for {scheme}://: {list}")));
    }
    if q.contains_key("password") {
        password = q.get("password").cloned().filter(|s| !s.is_empty()).or(password);
    }
    if argv && password.is_some() {
        let instead = match kind {
            DbKind::Postgres => "PGPASSWORD or ~/.pgpass",
            DbKind::Mysql => "MYSQL_PWD",
            _ => "REDISCLI_AUTH",
        };
        // What to do first: the status line is one row and cuts the end
        notes.push(format!("⚠ Use {instead} instead of a password in the command line (other users can see it with ps)."));
    }
    let port_of = |v: &str, from: &str| v.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(|| AppError::Msg(format!("{from}: \"{v}\" isn't a port number")));
    let mut ssl: Option<SslConfig> = None;
    let mut allow_plaintext = false;
    let files = |ca: &str, cert: &str, key: &str| (q.get(ca).cloned().unwrap_or_default(), q.get(cert).cloned().unwrap_or_default(), q.get(key).cloned().unwrap_or_default());
    // The TLS mode the certificate files ask for when no mode is given: a CA is checked (verify-ca), a client certificate needs TLS
    let with_files = {
        let (ca, cert, key) = match kind {
            DbKind::Mysql => files("ssl-ca", "ssl-cert", "ssl-key"),
            _ => files("sslrootcert", "sslcert", "sslkey"),
        };
        if !ca.is_empty() {
            Some("verify-ca")
        } else if !cert.is_empty() || !key.is_empty() {
            Some("require")
        } else {
            None
        }
    };
    let (host, port, user, database) = match kind {
        DbKind::Postgres => {
            let host = host.or_else(|| q.get("host").cloned()).or_else(|| env.get("PGHOST")).unwrap_or_else(|| "localhost".into());
            let port = match (u.port(), q.get("port"), env.get("PGPORT")) {
                (Some(p), _, _) => p,
                (None, Some(p), _) => port_of(p, "port=")?,
                (None, None, Some(p)) => port_of(&p, "PGPORT")?,
                _ => 5432,
            };
            let user = user.or_else(|| q.get("user").cloned()).or_else(|| env.get("PGUSER")).unwrap_or_else(|| env.local_user());
            let db = Some(path).filter(|p| !p.is_empty()).or_else(|| q.get("dbname").cloned()).or_else(|| env.get("PGDATABASE")).unwrap_or_else(|| user.clone());
            let mode = q.get("sslmode").cloned().or_else(|| env.get("PGSSLMODE")).map(|m| m.to_ascii_lowercase());
            let (ca, cert, key) = files("sslrootcert", "sslcert", "sslkey");
            let tls = |m: &str| Some(SslConfig { enabled: true, mode: m.into(), ca_file: ca.clone(), cert_file: cert.clone(), key_file: key.clone() });
            match mode.as_deref() {
                // Certificate files with no sslmode turn TLS on (a root certificate is checked: verify-ca), so they are never skipped
                None => ssl = with_files.and_then(tls),
                Some(m @ ("disable" | "allow" | "prefer")) if with_files.is_some() => return Err(unused_files("sslmode", m, with_files)),
                Some("disable") => allow_plaintext = true,
                Some("allow" | "prefer") => {}
                // require with a root certificate checks it, as libpq does (verify-ca)
                Some("require") if !ca.is_empty() => ssl = tls("verify-ca"),
                Some(m @ ("require" | "verify-ca" | "verify-full")) => ssl = tls(m),
                Some(m) => return Err(AppError::Msg(format!("Unknown sslmode \"{m}\" (use disable, prefer, require, verify-ca or verify-full)"))),
            }
            if password.is_none() {
                password = env.get("PGPASSWORD");
            }
            if password.is_none() {
                match pgpass(env, &host, port, &db, &user) {
                    Ok(p) => password = p,
                    Err(note) => notes.push(note),
                }
            }
            (host, port, user, db)
        }
        DbKind::Mysql => {
            let mode = q.get("ssl-mode").or_else(|| q.get("sslmode")).map(|m| m.to_ascii_uppercase());
            let (ca, cert, key) = files("ssl-ca", "ssl-cert", "ssl-key");
            let tls = |m: &str| Some(SslConfig { enabled: true, mode: m.into(), ca_file: ca.clone(), cert_file: cert.clone(), key_file: key.clone() });
            match mode.as_deref() {
                // As the mysql client: ssl-ca with no ssl-mode is VERIFY_CA; a client certificate alone turns TLS on (REQUIRED)
                None => ssl = with_files.and_then(tls),
                Some(m @ ("DISABLED" | "PREFERRED")) if with_files.is_some() => return Err(unused_files("ssl-mode", m, with_files)),
                Some("DISABLED") => allow_plaintext = true,
                Some("PREFERRED") => {}
                // A CA file is checked, never carried along unused
                Some("REQUIRED") if !ca.is_empty() => ssl = tls("verify-ca"),
                Some("REQUIRED") => ssl = tls("require"),
                Some("VERIFY_CA") => ssl = tls("verify-ca"),
                Some("VERIFY_IDENTITY") => ssl = tls("verify-full"),
                Some(m) => return Err(AppError::Msg(format!("Unknown ssl-mode \"{m}\" (use DISABLED, PREFERRED, REQUIRED, VERIFY_CA or VERIFY_IDENTITY)"))),
            }
            if password.is_none() {
                password = env.get("MYSQL_PWD");
            }
            (host.unwrap_or_else(|| "localhost".into()), u.port().unwrap_or(3306), user.unwrap_or_else(|| env.local_user()), path)
        }
        _ => {
            if !path.is_empty() && path.parse::<u32>().is_err() {
                return Err(bad("The Redis database is a number (redis://host:6379/2)"));
            }
            if tls_scheme {
                let (ca, cert, key) = files("sslrootcert", "sslcert", "sslkey");
                // rediss:// always means TLS: verified unless sslmode=require (encryption only) says otherwise; never plain text
                let mode = q.get("sslmode").cloned().unwrap_or_else(|| "verify-full".into()).to_ascii_lowercase();
                crate::db::check_ssl_mode(&mode).map_err(|_| AppError::Msg(format!("rediss:// needs TLS: sslmode is require, verify-ca or verify-full, not \"{mode}\"")))?;
                ssl = Some(SslConfig { enabled: true, mode, ca_file: ca, cert_file: cert, key_file: key });
            }
            if password.is_none() {
                password = env.get("REDISCLI_AUTH");
            }
            (host.unwrap_or_else(|| "127.0.0.1".into()), u.port().unwrap_or(6379), user.unwrap_or_default(), if path.is_empty() { "0".into() } else { path })
        }
    };
    let h = if host.contains(':') { format!("[{host}]") } else { host.clone() };
    let at = if user.is_empty() { String::new() } else { format!("{user}@") };
    let name = format!("{}://{at}{h}:{port}/{database}", scheme.to_ascii_lowercase());
    let mut config: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": name, "kind": kind, "host": host, "port": port, "user": user, "database": database }))?;
    config.read_only = read_only;
    config.ssl = ssl;
    config.options.allow_plaintext = allow_plaintext;
    // Never shared with MCP (it can't see a temporary source anyway: it lives only in this process)
    config.options.mcp = Some("off".into());
    Ok(Temporary { config, password, notes })
}

/// A PostgreSQL password from PGPASSFILE or ~/.pgpass for this server. Err = the file was ignored, with why (as libpq does: it must
/// not be readable by group or others)
pub fn pgpass(env: &Env, host: &str, port: u16, db: &str, user: &str) -> Result<Option<String>, String> {
    let path = match (env.get("PGPASSFILE"), &env.home) {
        (Some(p), _) => PathBuf::from(p),
        (None, Some(h)) => h.join(".pgpass"),
        (None, None) => return Ok(None),
    };
    pgpass_file(&path, host, port, db, user)
}

fn pgpass_file(path: &Path, host: &str, port: u16, db: &str, user: &str) -> Result<Option<String>, String> {
    let Ok(meta) = std::fs::metadata(path) else { return Ok(None) };
    if !meta.is_file() {
        return Err(format!("{} isn't a plain file — ignored", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(format!("{} ignored: readable by others ({:o}), as psql refuses it — run chmod 600 {}", path.display(), mode & 0o777, path.display()));
        }
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    // A Unix socket directory matches "localhost", as in libpq
    let host = if host.starts_with('/') { "localhost" } else { host };
    let port = port.to_string();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f = pgpass_fields(line);
        if f.len() < 5 {
            continue;
        }
        let m = |pat: &str, v: &str| pat == "*" || pat == v;
        if m(&f[0], host) && m(&f[1], &port) && m(&f[2], db) && m(&f[3], user) {
            return Ok(Some(f[4..].join(":")).filter(|p| !p.is_empty()));
        }
    }
    Ok(None)
}

/// host:port:db:user:password with \: and \\ escapes
fn pgpass_fields(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.last_mut().unwrap().push(n);
                }
            }
            ':' if out.len() < 5 => out.push(String::new()),
            c => out.last_mut().unwrap().push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A lookup in `vars` for Env::var
    fn env_of<'a>(vars: &'a HashMap<&'static str, &'static str>) -> impl Fn(&str) -> Option<String> + 'a {
        move |k: &str| vars.get(k).map(|v| v.to_string())
    }

    fn open(url: &str, vars: &[(&'static str, &'static str)]) -> AppResult<Temporary> {
        let vars: HashMap<_, _> = vars.iter().cloned().chain([("USER", "me")]).collect();
        from_url(url, false, &Env { var: &env_of(&vars), home: None })
    }

    fn parts(t: &Temporary) -> (DbKind, &str, u16, &str, &str, Option<&str>) {
        let c = &t.config;
        (c.kind, c.host.as_str(), c.port, c.user.as_str(), c.database.as_str(), t.password.as_deref())
    }

    #[test]
    fn postgres_url_parts_and_percent_encoding() {
        let t = open("postgres://app%40corp:p%40ss%3Aw%2Fd@db.example:6543/my%20db", &[]).unwrap();
        assert_eq!(parts(&t), (DbKind::Postgres, "db.example", 6543, "app@corp", "my db", Some("p@ss:w/d")));
        // The name shows no password
        assert_eq!(t.config.name, "postgres://app@corp@db.example:6543/my db");
        // The fix first: the status line cuts the end
        assert!(t.notes.iter().any(|n| n.starts_with("⚠ Use ") && n.contains("command line")), "{:?}", t.notes);
        // A raw @ in the password (everything up to the last @ is userinfo)
        assert_eq!(open("postgresql://u:a@b@h/d", &[]).unwrap().password.as_deref(), Some("a@b"));
        // IPv6
        let t = open("postgres://u@[::1]:5433/d", &[]).unwrap();
        assert_eq!((t.config.host.as_str(), t.config.port), ("::1", 5433));
        assert_eq!(t.config.name, "postgres://u@[::1]:5433/d");
        // Query parameters
        let t = open("postgres:///d?host=%2Ftmp&port=5999&user=x", &[]).unwrap();
        assert_eq!(parts(&t), (DbKind::Postgres, "/tmp", 5999, "x", "d", None));
    }

    /// Missing parts: the environment, then libpq's defaults (localhost, 5432, the login name, a database named after the user)
    #[test]
    fn missing_parts_come_from_env_then_defaults() {
        assert_eq!(parts(&open("postgres://", &[]).unwrap()), (DbKind::Postgres, "localhost", 5432, "me", "me", None));
        let env = [("PGHOST", "envhost"), ("PGPORT", "6000"), ("PGUSER", "envuser"), ("PGDATABASE", "envdb"), ("PGPASSWORD", "envpw")];
        assert_eq!(parts(&open("postgres://", &env).unwrap()), (DbKind::Postgres, "envhost", 6000, "envuser", "envdb", Some("envpw")));
        // URL > env
        assert_eq!(parts(&open("postgres://u:urlpw@h:1/d", &env).unwrap()), (DbKind::Postgres, "h", 1, "u", "d", Some("urlpw")));
        assert!(open("postgres://", &[("PGPORT", "x")]).is_err());
        // MySQL / Redis
        assert_eq!(parts(&open("mysql://root@h/shop", &[("MYSQL_PWD", "mpw")]).unwrap()), (DbKind::Mysql, "h", 3306, "root", "shop", Some("mpw")));
        assert_eq!(parts(&open("mysql://", &[]).unwrap()), (DbKind::Mysql, "localhost", 3306, "me", "", None));
        assert_eq!(parts(&open("redis://h/3", &[("REDISCLI_AUTH", "rpw")]).unwrap()), (DbKind::Redis, "h", 6379, "", "3", Some("rpw")));
        assert_eq!(parts(&open("redis://:p@h:7000", &[("REDISCLI_AUTH", "rpw")]).unwrap()), (DbKind::Redis, "h", 7000, "", "0", Some("p")));
        assert!(open("redis://h/x", &[]).is_err());
    }

    /// sslmode (URL, then PGSSLMODE) maps to the data source's TLS settings; rediss:// is always TLS
    #[test]
    fn tls_settings_follow_sslmode() {
        let ssl = |t: &Temporary| (t.config.ssl.as_ref().map(|s| s.mode.clone()), t.config.options.allow_plaintext);
        assert_eq!(ssl(&open("postgres://h/d", &[]).unwrap()), (None, false));
        assert_eq!(ssl(&open("postgres://h/d?sslmode=verify-full&sslrootcert=~/ca.pem", &[]).unwrap()), (Some("verify-full".into()), false));
        assert_eq!(open("postgres://h/d?sslmode=verify-full&sslrootcert=~/ca.pem", &[]).unwrap().config.ssl.unwrap().ca_file, "~/ca.pem");
        assert_eq!(ssl(&open("postgres://h/d", &[("PGSSLMODE", "require")]).unwrap()), (Some("require".into()), false));
        // require with a root certificate checks it (libpq: verify-ca)
        assert_eq!(ssl(&open("postgres://h/d?sslmode=require&sslrootcert=~/rds-ca.pem", &[]).unwrap()), (Some("verify-ca".into()), false));
        assert_eq!(ssl(&open("postgres://h/d?sslmode=prefer", &[("PGSSLMODE", "require")]).unwrap()), (None, false));
        // disable is the explicit "Allow unencrypted connection"
        assert_eq!(ssl(&open("postgres://h/d?sslmode=disable", &[]).unwrap()), (None, true));
        assert!(open("postgres://h/d?sslmode=bogus", &[]).is_err());
        assert_eq!(ssl(&open("mysql://h/d?ssl-mode=VERIFY_IDENTITY", &[]).unwrap()), (Some("verify-full".into()), false));
        assert_eq!(ssl(&open("mysql://h/d?ssl-mode=required", &[]).unwrap()), (Some("require".into()), false));
        assert_eq!(ssl(&open("mysql://h/d?ssl-mode=DISABLED", &[]).unwrap()), (None, true));
        assert_eq!(ssl(&open("rediss://h", &[]).unwrap()), (Some("verify-full".into()), false));
        assert_eq!(ssl(&open("rediss://h?sslmode=require", &[]).unwrap()), (Some("require".into()), false));
        assert!(open("rediss://h?sslmode=disable", &[]).is_err(), "rediss:// is never plain text");
        assert!(open("redis://h?sslmode=require", &[]).unwrap_err().to_string().contains("rediss://"));
        assert_eq!(ssl(&open("redis://h", &[]).unwrap()), (None, false));

        // Certificate files are never dropped. No mode: a CA is checked (verify-ca, as the mysql client's --ssl-ca), a client
        // certificate alone turns TLS on; a mode that wouldn't use them is refused with the mode to use
        let pg = open("postgres://h/d?sslrootcert=/ca.pem", &[]).unwrap();
        assert_eq!(ssl(&pg), (Some("verify-ca".into()), false));
        assert_eq!(pg.config.ssl.unwrap().ca_file, "/ca.pem");
        let pg = open("postgres://h/d?sslcert=/c.pem&sslkey=/k.pem", &[]).unwrap();
        assert_eq!(ssl(&pg), (Some("require".into()), false));
        let s = pg.config.ssl.unwrap();
        assert_eq!((s.cert_file.as_str(), s.key_file.as_str()), ("/c.pem", "/k.pem"));
        assert_eq!(ssl(&open("postgres://h/d?sslrootcert=/ca.pem&sslcert=/c.pem", &[]).unwrap()), (Some("verify-ca".into()), false));
        for (url, env, want) in [
            ("postgres://h/d?sslmode=prefer&sslrootcert=/ca.pem", &[][..], "Use sslmode=verify-ca (or verify-full): sslmode=prefer doesn't use sslrootcert"),
            ("postgres://h/d?sslmode=allow&sslcert=/c.pem", &[], "Use sslmode=require: sslmode=allow doesn't use sslcert / sslkey"),
            ("postgres://h/d?sslmode=disable&sslkey=/k.pem", &[], "sslmode=disable doesn't use"),
            ("postgres://h/d?sslrootcert=/ca.pem", &[("PGSSLMODE", "prefer")], "sslmode=prefer doesn't use sslrootcert"),
            ("mysql://h/d?ssl-mode=PREFERRED&ssl-ca=/ca.pem", &[], "Use ssl-mode=VERIFY_CA (or VERIFY_IDENTITY): ssl-mode=PREFERRED doesn't use ssl-ca"),
            ("mysql://h/d?ssl-mode=disabled&ssl-cert=/c.pem", &[], "Use ssl-mode=REQUIRED: ssl-mode=DISABLED doesn't use ssl-cert / ssl-key"),
        ] {
            let e = open(url, env).unwrap_err().to_string();
            assert!(e.contains(want), "{url}: {e}");
        }
        let my = open("mysql://h/d?ssl-ca=/ca.pem", &[]).unwrap();
        assert_eq!(ssl(&my), (Some("verify-ca".into()), false));
        assert_eq!(my.config.ssl.unwrap().ca_file, "/ca.pem");
        assert_eq!(ssl(&open("mysql://h/d?ssl-cert=/c.pem&ssl-key=/k.pem", &[]).unwrap()), (Some("require".into()), false));
        // REQUIRED with a CA checks it (as require + sslrootcert does for PostgreSQL)
        assert_eq!(ssl(&open("mysql://h/d?ssl-mode=REQUIRED&ssl-ca=/ca.pem", &[]).unwrap()), (Some("verify-ca".into()), false));
        assert_eq!(ssl(&open("mysql://h/d?ssl-mode=VERIFY_IDENTITY&ssl-ca=/ca.pem", &[]).unwrap()), (Some("verify-full".into()), false));
    }

    /// The ps warning is for a password that is really on the command line: not DATABASE_URL (--env), not PGPASSWORD / MYSQL_PWD
    #[test]
    fn command_line_warning_only_for_argv() {
        let warned = |t: &Temporary| t.notes.iter().any(|n| n.contains("command line"));
        assert!(warned(&open("postgres://u:pw@h/d", &[]).unwrap()));
        assert!(warned(&open("postgres://u@h/d?password=pw", &[]).unwrap()));
        assert!(!warned(&open("postgres://u@h/d", &[("PGPASSWORD", "pw")]).unwrap()));
        assert!(!warned(&open("mysql://u@h/d", &[("MYSQL_PWD", "pw")]).unwrap()));
        for url in ["postgres://u:pw@h/d", "mysql://u:pw@h/d", "redis://:pw@h"] {
            let vars: HashMap<&'static str, &'static str> = [("DATABASE_URL", url), ("USER", "me")].into();
            let t = from_database_url(&Env { var: &env_of(&vars), home: None }, false).unwrap();
            assert_eq!(t.password.as_deref(), Some("pw"));
            assert!(!warned(&t), "{url}: {:?}", t.notes);
        }
    }

    #[test]
    fn bad_urls_say_what_is_wrong_without_the_password() {
        let e = open("mongodb://u:secretpw@h/d", &[]).unwrap_err().to_string();
        assert!(e.contains("Unknown scheme") && e.contains("postgres://") && !e.contains("secretpw"), "{e}");
        let e = open("postgres://u:secretpw@h:notaport/d", &[]).unwrap_err().to_string();
        assert!(!e.contains("secretpw") && e.contains("u:***@"), "{e}");
        let e = open("postgres://u:secretpw@h/d?options=-c%20x", &[]).unwrap_err().to_string();
        assert!(e.contains("Unknown URL parameter \"options\"") && !e.contains("secretpw"), "{e}");
        // Common harmless DATABASE_URL parameters are ignored with a note; TLS-like unknown ones are still refused
        let t = open("postgres://u@db/app?connect_timeout=10&application_name=api", &[]).unwrap();
        assert!(t.notes.iter().any(|n| n.contains("Ignored URL parameters application_name, connect_timeout")), "{:?}", t.notes);
        assert!(open("mysql://u@db/app?charset=utf8mb4", &[]).is_ok());
        assert!(open("postgres://u@db/app?pgbouncer=true", &[]).is_ok());
        assert!(open("postgres://u@db/app?sslnegotiation=direct", &[]).unwrap_err().to_string().contains("Unknown URL parameter"));
        assert!(open("postgres://h1,h2/d", &[]).unwrap_err().to_string().contains("Several hosts"));
        assert!(open("db.example:5432", &[]).is_err());
    }

    #[test]
    fn redact_hides_passwords() {
        assert_eq!(redact("postgres://u:secret@h/d"), "postgres://u:***@h/d");
        assert_eq!(redact("postgres://u:se@c:ret@h/d"), "postgres://u:***@h/d");
        assert_eq!(redact("redis://:secret@h:6379/0"), "redis://:***@h:6379/0");
        assert_eq!(redact("postgres://u@h/d?sslmode=require&password=secret"), "postgres://u@h/d?sslmode=require&password=***");
        assert_eq!(redact("postgres://u@h/d"), "postgres://u@h/d");
        assert_eq!(redact("not a url"), "not a url");
    }

    /// ~/.pgpass: first matching line, * wildcards, \: escapes; ignored (with why) when others can read it, as libpq does. URL > env > .pgpass
    #[test]
    fn pgpass_is_read_like_libpq() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(".pgpass");
        std::fs::write(&p, "# comment\nother:5432:*:*:nope\ndb.example:6543:app:alice:pa\\:ss:word\n*:*:*:bob:bobpw\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let vars: HashMap<&'static str, &'static str> = [("USER", "me")].into();
        let env = Env { var: &env_of(&vars), home: Some(dir.path().to_path_buf()) };
        let t = from_url("postgres://alice@db.example:6543/app", false, &env).unwrap();
        assert_eq!(t.password.as_deref(), Some("pa:ss:word"));
        assert!(t.notes.is_empty(), "a password from .pgpass isn't on the command line");
        assert_eq!(from_url("postgres://bob@any/x", false, &env).unwrap().password.as_deref(), Some("bobpw"));
        assert_eq!(from_url("postgres://carol@any/x", false, &env).unwrap().password, None);
        // URL > .pgpass
        assert_eq!(from_url("postgres://bob:url@any/x", false, &env).unwrap().password.as_deref(), Some("url"));
        // env > .pgpass
        let vars2: HashMap<&'static str, &'static str> = [("PGPASSWORD", "envpw")].into();
        let env2 = Env { var: &env_of(&vars2), home: Some(dir.path().to_path_buf()) };
        assert_eq!(from_url("postgres://bob@any/x", false, &env2).unwrap().password.as_deref(), Some("envpw"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
            let t = from_url("postgres://bob@any/x", false, &env).unwrap();
            assert_eq!(t.password, None);
            assert!(t.notes.iter().any(|n| n.contains("chmod 600")), "{:?}", t.notes);
        }
    }

    /// Real servers through a URL, as qurry-tui opens them: a read runs; with --read-only a write is refused (the guard, and for
    /// PostgreSQL the server too); without it the write runs. Uses its own table / key prefix tui_url_connect.
    /// QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres QURRY_TEST_MY=127.0.0.1:13306:root:dbtool:shop QURRY_TEST_REDIS=127.0.0.1:16379 \
    ///   cargo test -p qurry-core temporary_url_on_real_servers -- --ignored
    #[tokio::test]
    #[ignore]
    async fn temporary_url_on_real_servers() {
        let var = |k: &str| (k == "USER").then(|| "me".to_string());
        let env = Env { var: &var, home: None };
        let mut urls = vec![];
        if let Ok(pg) = std::env::var("QURRY_TEST_PG") {
            let p: Vec<&str> = pg.splitn(5, ':').collect();
            let pw = p.get(4).map(|s| format!(":{s}")).unwrap_or_default();
            urls.push((format!("postgres://{}{pw}@{}:{}/{}", p[2], p[0], p[1], p[3]), "create table if not exists tui_url_connect (id int)", "insert into tui_url_connect values (1)", "drop table tui_url_connect"));
        }
        if let Ok(my) = std::env::var("QURRY_TEST_MY") {
            let p: Vec<&str> = my.splitn(5, ':').collect();
            urls.push((format!("mysql://{}:{}@{}:{}/{}", p[2], p[3], p[0], p[1], p[4]), "create table if not exists tui_url_connect (id int)", "insert into tui_url_connect values (1)", "drop table tui_url_connect"));
        }
        if let Ok(r) = std::env::var("QURRY_TEST_REDIS") {
            urls.push((format!("redis://{r}/9"), "", "SET tui_url_connect:k v", "DEL tui_url_connect:k"));
        }
        assert!(!urls.is_empty(), "set QURRY_TEST_PG / QURRY_TEST_MY / QURRY_TEST_REDIS");
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        for (url, setup, write, cleanup) in urls {
            let shown = redact(&url);
            let rw = st.open_temporary(from_url(&url, false, &env).unwrap());
            let ro = st.open_temporary(from_url(&url, true, &env).unwrap());
            let v = st.connect(ro.id.clone()).await.unwrap_or_else(|e| panic!("{shown}: {e}"));
            println!("{shown}: {v}");
            if !setup.is_empty() {
                st.run_query(rw.id.clone(), setup.into(), None, None).await.unwrap();
            }
            let e = st.run_query(ro.id.clone(), write.into(), None, None).await.expect_err("read-only refuses a write").to_string();
            assert!(e.contains("read-only"), "{shown}: {e}");
            st.run_query(rw.id.clone(), write.into(), None, None).await.unwrap_or_else(|e| panic!("{shown}: {e}"));
            st.run_query(rw.id.clone(), cleanup.into(), None, None).await.unwrap();
            for id in [rw.id, ro.id] {
                st.delete_connection(id).await.unwrap();
            }
            // A CA file with no TLS mode is used, not dropped: with a CA that didn't sign the server's certificate (or a server
            // without TLS) the connection fails instead of going on unverified
            let ca_param = match url.split("://").next() {
                Some("postgres") => "sslrootcert",
                Some("mysql") => "ssl-ca",
                _ => continue,
            };
            let ca = dir.path().join("unrelated-ca.pem");
            std::fs::write(&ca, UNRELATED_CA).unwrap();
            let t = st.open_temporary(from_url(&format!("{url}?{ca_param}={}", ca.display()), false, &env).unwrap());
            let e = st.connect(t.id.clone()).await.expect_err("a CA that didn't sign the server's certificate is refused").to_string();
            println!("{shown} with {ca_param}: {e}");
            // (MySQL 5.7 speaks only TLS 1.0 / 1.1, which rustls refuses: HandshakeFailure, still refused)
            assert!(["certificate", "UnknownIssuer", "TLS", "HandshakeFailure"].iter().any(|w| e.contains(w)), "{shown}: {e}");
            st.delete_connection(t.id).await.unwrap();
        }
        assert!(!dir.path().join("connections.json").exists());
    }

    /// A self-signed CA (public certificate only) that signed no test server's certificate
    const UNRELATED_CA: &str = "-----BEGIN CERTIFICATE-----
MIIBejCCASCgAwIBAgIUZEK6IdYf4C6NhpnnDp0so+qzmRwwCgYIKoZIzj0EAwIw
IjEgMB4GA1UEAwwXcXVycnktdGVzdC11bnJlbGF0ZWQtY2EwIBcNMjYxMTA2MTY0
MDAwWhgPMjEyNjEwMTMxNjQwMDBaMCIxIDAeBgNVBAMMF3F1cnJ5LXRlc3QtdW5y
ZWxhdGVkLWNhMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEMfBW4IlqNdAGHIDq
wr1KEQdG2F2XAwpfSK7+G5TYBtdUgZz0hlA47JdQJxKYFssBxTRfH7wYEMS0+qNE
Q/v75qMyMDAwHQYDVR0OBBYEFHPH8/zNj76O2QFYjnxWQInRI6S+MA8GA1UdEwEB
/wQFMAMBAf8wCgYIKoZIzj0EAwIDSAAwRQIhAOMKQY4WbXbde2KiJyO8UZ3UgP0V
AY1Jv8ftzr/bARM2AiBnPKF+MbjNeSwUIByXfnGA80CLk6UKGquTFdP1RXb/Yg==
-----END CERTIFICATE-----
";

    #[test]
    fn read_only_and_never_shared_with_mcp() {
        let vars: HashMap<&'static str, &'static str> = HashMap::new();
        let t = from_url("postgres://u@h/d", true, &Env { var: &env_of(&vars), home: None }).unwrap();
        assert!(t.config.read_only);
        assert_eq!(t.config.mcp_access(true), crate::store::McpAccess::Off);
    }
}
